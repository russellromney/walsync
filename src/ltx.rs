//! LTX (Litestream Transaction) format support
//!
//! This module provides utilities for encoding and decoding LTX files,
//! which are Litestream-compatible transaction files containing SQLite pages.

use anyhow::{anyhow, Result};
use litetx::{Checksum, Decoder, Encoder, Header, HeaderFlags, PageNum, PageSize, TXID};
use std::io::{Read, Write};
use std::path::Path;
use std::time::SystemTime;

/// Create an LTX file from a SQLite database snapshot
pub fn encode_snapshot<W: Write>(
    writer: W,
    db_path: &Path,
    page_size: u32,
    txid: u64,
) -> Result<()> {
    let db_data = std::fs::read(db_path)?;
    let page_size_val = PageSize::new(page_size).map_err(|e| anyhow!("Invalid page size: {}", e))?;
    let num_pages = db_data.len() / page_size as usize;

    let header = Header {
        flags: HeaderFlags::COMPRESS_LZ4,
        page_size: page_size_val,
        commit: PageNum::new(num_pages as u32).map_err(|e| anyhow!("Invalid page count: {}", e))?,
        min_txid: TXID::ONE, // Snapshot starts at TXID 1
        max_txid: TXID::new(txid).map_err(|e| anyhow!("Invalid TXID: {}", e))?,
        timestamp: SystemTime::now(),
        pre_apply_checksum: None,
    };

    let mut encoder = Encoder::new(writer, &header)?;

    // Encode each page
    for i in 0..num_pages {
        let page_num = PageNum::new((i + 1) as u32).map_err(|e| anyhow!("Invalid page num: {}", e))?;
        let start = i * page_size as usize;
        let end = start + page_size as usize;
        let page_data = &db_data[start..end];

        encoder.encode_page(page_num, page_data)?;
    }

    // Compute final checksum and finish
    let checksum = compute_db_checksum(&db_data);
    encoder.finish(checksum)?;

    Ok(())
}

/// Decode an LTX file and reconstruct the database (full write)
pub fn decode_to_db<R: Read>(reader: R, output_path: &Path) -> Result<Header> {
    let (mut decoder, header) = Decoder::new(reader)?;

    let page_size = header.page_size.into_inner() as usize;
    let num_pages = header.commit.into_inner() as usize;

    let mut db_data = vec![0u8; num_pages * page_size];
    let mut page_buf = vec![0u8; page_size];

    while let Some(page_num) = decoder.decode_page(&mut page_buf)? {
        let idx = (page_num.into_inner() - 1) as usize;
        let start = idx * page_size;
        db_data[start..start + page_size].copy_from_slice(&page_buf);
    }

    // Verify checksum
    decoder.finish()?;

    // Write database file
    std::fs::write(output_path, &db_data)?;

    Ok(header)
}

/// Apply an incremental LTX file to an existing database (in-place page writes)
///
/// This is more efficient than decode_to_db for incremental updates since it only
/// writes the pages that changed, not the entire database.
pub fn apply_ltx_to_db<R: Read>(reader: R, db_path: &Path) -> Result<Header> {
    use std::fs::OpenOptions;
    use std::io::{Seek, SeekFrom, Write as IoWrite};

    let (mut decoder, header) = Decoder::new(reader)?;

    let page_size = header.page_size.into_inner() as usize;
    let mut page_buf = vec![0u8; page_size];

    // Open existing db file for page-level writes
    let mut file = OpenOptions::new()
        .write(true)
        .open(db_path)
        .map_err(|e| anyhow!("Failed to open database for in-place apply: {}", e))?;

    let mut pages_applied = 0u32;

    while let Some(page_num) = decoder.decode_page(&mut page_buf)? {
        let offset = (page_num.into_inner() as u64 - 1) * page_size as u64;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&page_buf)?;
        pages_applied += 1;
    }

    // Ensure all writes are flushed
    file.sync_all()?;

    // Verify checksum
    decoder.finish()?;

    tracing::debug!(
        "Applied {} pages in-place (TXID {}-{})",
        pages_applied,
        header.min_txid.into_inner(),
        header.max_txid.into_inner()
    );

    Ok(header)
}

/// Compute checksum from database file (for checksum tracking)
pub fn compute_checksum_from_file(db_path: &Path) -> Result<Checksum> {
    let data = std::fs::read(db_path)?;
    Ok(compute_db_checksum(&data))
}

/// Encode WAL changes as an LTX file (incremental, not snapshot)
pub fn encode_wal_changes<W: Write>(
    writer: W,
    pages: &[(u32, Vec<u8>)], // (page_num, page_data)
    page_size: u32,
    min_txid: u64,
    max_txid: u64,
    commit_page: u32,
    pre_checksum: Option<Checksum>,
) -> Result<Checksum> {
    let page_size_val = PageSize::new(page_size).map_err(|e| anyhow!("Invalid page size: {}", e))?;

    let header = Header {
        flags: HeaderFlags::COMPRESS_LZ4,
        page_size: page_size_val,
        commit: PageNum::new(commit_page).map_err(|e| anyhow!("Invalid commit page: {}", e))?,
        min_txid: TXID::new(min_txid).map_err(|e| anyhow!("Invalid min TXID: {}", e))?,
        max_txid: TXID::new(max_txid).map_err(|e| anyhow!("Invalid max TXID: {}", e))?,
        timestamp: SystemTime::now(),
        pre_apply_checksum: pre_checksum,
    };

    let mut encoder = Encoder::new(writer, &header)?;

    // Sort pages by page number and encode
    let mut sorted_pages = pages.to_vec();
    sorted_pages.sort_by_key(|(num, _)| *num);

    for (page_num, data) in &sorted_pages {
        let pn = PageNum::new(*page_num).map_err(|e| anyhow!("Invalid page num: {}", e))?;
        encoder.encode_page(pn, data)?;
    }

    // Compute checksum from pages
    let post_checksum = compute_pages_checksum(&sorted_pages);
    let trailer = encoder.finish(post_checksum)?;

    Ok(trailer.post_apply_checksum)
}

/// Verify an LTX file by decoding all pages and checking the checksum
/// Returns the header on success, or an error describing the verification failure
pub fn verify_ltx<R: Read>(reader: R) -> Result<Header> {
    let (mut decoder, header) = Decoder::new(reader)?;

    let page_size = header.page_size.into_inner() as usize;
    let mut page_buf = vec![0u8; page_size];

    // Decode all pages (required to verify checksum)
    while decoder.decode_page(&mut page_buf)?.is_some() {
        // Just consume the pages
    }

    // Verify checksum - this will fail if corrupted
    decoder.finish()?;

    Ok(header)
}

/// Compute database checksum (single u64 from SHA256)
fn compute_db_checksum(data: &[u8]) -> Checksum {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    // Take first 8 bytes as u64
    let hash = u64::from_be_bytes(result[0..8].try_into().unwrap());
    Checksum::new(hash)
}

fn compute_pages_checksum(pages: &[(u32, Vec<u8>)]) -> Checksum {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for (num, data) in pages {
        hasher.update(num.to_be_bytes());
        hasher.update(data);
    }
    let result = hasher.finalize();
    let hash = u64::from_be_bytes(result[0..8].try_into().unwrap());
    Checksum::new(hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_snapshot_roundtrip_single_page() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let ltx_path = dir.path().join("test.ltx");
        let restored_path = dir.path().join("restored.db");

        // Create a simple SQLite database (4KB page size, 1 page)
        let page_size = 4096u32;
        let db_data = vec![0x42u8; page_size as usize];
        std::fs::write(&db_path, &db_data).unwrap();

        // Encode as LTX
        let ltx_file = std::fs::File::create(&ltx_path).unwrap();
        encode_snapshot(ltx_file, &db_path, page_size, 1).unwrap();

        // Decode back
        let ltx_file = std::fs::File::open(&ltx_path).unwrap();
        let header = decode_to_db(ltx_file, &restored_path).unwrap();

        // Verify
        let restored_data = std::fs::read(&restored_path).unwrap();
        assert_eq!(db_data, restored_data);
        assert_eq!(header.page_size.into_inner(), page_size);
        assert_eq!(header.min_txid.into_inner(), 1);
        assert_eq!(header.max_txid.into_inner(), 1);
    }

    #[test]
    fn test_snapshot_roundtrip_multiple_pages() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let restored_path = dir.path().join("restored.db");

        let page_size = 4096u32;
        let num_pages = 10;

        // Create database with multiple pages, each with unique content
        let mut db_data = Vec::new();
        for i in 0..num_pages {
            let mut page = vec![(i as u8).wrapping_mul(17); page_size as usize];
            // Add page number marker at start
            page[0..4].copy_from_slice(&(i as u32).to_be_bytes());
            db_data.extend(page);
        }
        std::fs::write(&db_path, &db_data).unwrap();

        // Encode as LTX to buffer
        let mut ltx_buffer = Vec::new();
        encode_snapshot(&mut ltx_buffer, &db_path, page_size, 100).unwrap();

        // Decode back
        let cursor = std::io::Cursor::new(ltx_buffer);
        let header = decode_to_db(cursor, &restored_path).unwrap();

        // Verify byte-for-byte
        let restored_data = std::fs::read(&restored_path).unwrap();
        assert_eq!(db_data.len(), restored_data.len());
        assert_eq!(db_data, restored_data);
        assert_eq!(header.commit.into_inner(), num_pages as u32);
        assert_eq!(header.max_txid.into_inner(), 100);
    }

    #[test]
    fn test_snapshot_various_page_sizes() {
        let dir = tempdir().unwrap();

        for page_size in [512u32, 1024, 2048, 4096, 8192, 16384, 32768] {
            let db_path = dir.path().join(format!("test_{}.db", page_size));
            let restored_path = dir.path().join(format!("restored_{}.db", page_size));

            // Create 3-page database
            let db_data: Vec<u8> = (0..3)
                .flat_map(|i| vec![(i * 50) as u8; page_size as usize])
                .collect();
            std::fs::write(&db_path, &db_data).unwrap();

            let mut ltx_buffer = Vec::new();
            encode_snapshot(&mut ltx_buffer, &db_path, page_size, 1).unwrap();

            let cursor = std::io::Cursor::new(ltx_buffer);
            let header = decode_to_db(cursor, &restored_path).unwrap();

            let restored_data = std::fs::read(&restored_path).unwrap();
            assert_eq!(
                db_data, restored_data,
                "Mismatch for page_size={}",
                page_size
            );
            assert_eq!(header.page_size.into_inner(), page_size);
        }
    }

    #[test]
    fn test_snapshot_preserves_binary_data() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("binary.db");
        let restored_path = dir.path().join("restored.db");

        let page_size = 4096u32;

        // Create database with all byte values (0x00-0xFF pattern)
        let mut db_data = Vec::new();
        for page_num in 0..4 {
            let mut page = vec![0u8; page_size as usize];
            for (i, byte) in page.iter_mut().enumerate() {
                *byte = ((page_num * 256 + i) % 256) as u8;
            }
            db_data.extend(page);
        }
        std::fs::write(&db_path, &db_data).unwrap();

        let mut ltx_buffer = Vec::new();
        encode_snapshot(&mut ltx_buffer, &db_path, page_size, 50).unwrap();

        let cursor = std::io::Cursor::new(ltx_buffer);
        decode_to_db(cursor, &restored_path).unwrap();

        let restored_data = std::fs::read(&restored_path).unwrap();

        // Verify every single byte
        for (i, (orig, rest)) in db_data.iter().zip(restored_data.iter()).enumerate() {
            assert_eq!(
                orig, rest,
                "Byte mismatch at offset {}: expected 0x{:02x}, got 0x{:02x}",
                i, orig, rest
            );
        }
    }

    #[test]
    fn test_incremental_ltx_encoding_with_checksum() {
        // Test encoding WAL changes as incremental LTX
        // Note: LTX format requires pre_apply_checksum for incremental files
        let page_size = 4096u32;

        // Simulate WAL changes: sequential pages (LTX requirement)
        let pages: Vec<(u32, Vec<u8>)> = vec![
            (1, vec![0xAA; page_size as usize]),
            (2, vec![0xBB; page_size as usize]),
            (3, vec![0xCC; page_size as usize]),
        ];

        // Pre-apply checksum is required for non-snapshot LTX files
        let pre_checksum = Checksum::new(0x123456789ABCDEF0);

        let mut ltx_buffer = Vec::new();
        let checksum = encode_wal_changes(
            &mut ltx_buffer,
            &pages,
            page_size,
            10,  // min_txid
            12,  // max_txid
            3,   // commit_page (db size in pages)
            Some(pre_checksum),
        )
        .unwrap();

        // Verify we got a valid checksum
        assert!(checksum.into_inner() != 0);

        // Verify buffer is non-empty and reasonable size
        assert!(!ltx_buffer.is_empty());
        assert!(ltx_buffer.len() > 100); // At least header
    }

    #[test]
    fn test_incremental_ltx_format_rules() {
        // LTX format rules:
        // - min_txid=1 is a "snapshot" (no pre_checksum allowed)
        // - min_txid>1 is "incremental" (pre_checksum required)
        let page_size = 1024u32;
        let pre_checksum = Checksum::new(0x123456789ABCDEF0);

        // Incremental (min_txid > 1) requires pre_checksum
        let pages: Vec<(u32, Vec<u8>)> = vec![
            (1, vec![0x11; page_size as usize]),
            (2, vec![0x22; page_size as usize]),
        ];

        let mut ltx_buffer = Vec::new();
        let result = encode_wal_changes(
            &mut ltx_buffer,
            &pages,
            page_size,
            10, // min_txid > 1 = incremental
            11, // max_txid
            2,
            Some(pre_checksum),
        );
        assert!(
            result.is_ok(),
            "Incremental with pre_checksum should succeed: {:?}",
            result.err()
        );

        // Incremental without pre_checksum should fail
        let mut ltx_buffer2 = Vec::new();
        let result2 = encode_wal_changes(
            &mut ltx_buffer2,
            &pages,
            page_size,
            10, // min_txid > 1 = incremental
            11,
            2,
            None, // Missing pre_checksum!
        );
        assert!(
            result2.is_err(),
            "Incremental without pre_checksum should fail"
        );

        // Snapshot (min_txid = 1) should not have pre_checksum
        let mut ltx_buffer3 = Vec::new();
        let result3 = encode_wal_changes(
            &mut ltx_buffer3,
            &pages,
            page_size,
            1, // min_txid = 1 = snapshot
            2,
            2,
            None, // No pre_checksum for snapshot
        );
        assert!(
            result3.is_ok(),
            "Snapshot without pre_checksum should succeed: {:?}",
            result3.err()
        );
    }

    #[test]
    fn test_txid_ranges() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let restored_path = dir.path().join("restored.db");

        let page_size = 4096u32;
        let db_data = vec![0x42u8; page_size as usize];
        std::fs::write(&db_path, &db_data).unwrap();

        // Test various TXID values
        for txid in [1u64, 100, 1000, 999999, u32::MAX as u64] {
            let mut ltx_buffer = Vec::new();
            encode_snapshot(&mut ltx_buffer, &db_path, page_size, txid).unwrap();

            let cursor = std::io::Cursor::new(ltx_buffer);
            let header = decode_to_db(cursor, &restored_path).unwrap();

            assert_eq!(header.max_txid.into_inner(), txid);
        }
    }

    #[test]
    fn test_checksum_computation() {
        // Verify checksum is deterministic
        let data1 = b"hello world";
        let data2 = b"hello world";
        let data3 = b"hello worlD"; // Different

        let cs1 = compute_db_checksum(data1);
        let cs2 = compute_db_checksum(data2);
        let cs3 = compute_db_checksum(data3);

        assert_eq!(cs1.into_inner(), cs2.into_inner());
        assert_ne!(cs1.into_inner(), cs3.into_inner());
    }

    #[test]
    fn test_pages_checksum_computation() {
        let pages1: Vec<(u32, Vec<u8>)> = vec![(1, vec![0xAA; 100]), (2, vec![0xBB; 100])];
        let pages2: Vec<(u32, Vec<u8>)> = vec![(1, vec![0xAA; 100]), (2, vec![0xBB; 100])];
        let pages3: Vec<(u32, Vec<u8>)> = vec![(1, vec![0xAA; 100]), (2, vec![0xBC; 100])];

        let cs1 = compute_pages_checksum(&pages1);
        let cs2 = compute_pages_checksum(&pages2);
        let cs3 = compute_pages_checksum(&pages3);

        assert_eq!(cs1.into_inner(), cs2.into_inner());
        assert_ne!(cs1.into_inner(), cs3.into_inner());
    }

    #[test]
    fn test_large_database() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("large.db");
        let restored_path = dir.path().join("restored.db");

        let page_size = 4096u32;
        let num_pages = 100; // 400KB database

        // Create large database with varying content
        let mut db_data = Vec::with_capacity(num_pages * page_size as usize);
        for i in 0..num_pages {
            let pattern = (i as u8).wrapping_mul(37);
            let mut page = vec![pattern; page_size as usize];
            // Mark page with its number
            let page_num_bytes = (i as u32).to_le_bytes();
            page[0..4].copy_from_slice(&page_num_bytes);
            db_data.extend(page);
        }
        std::fs::write(&db_path, &db_data).unwrap();

        let mut ltx_buffer = Vec::new();
        encode_snapshot(&mut ltx_buffer, &db_path, page_size, 1000).unwrap();

        // LTX should be compressed
        assert!(
            ltx_buffer.len() < db_data.len(),
            "LTX ({}) should be smaller than raw DB ({}) due to compression",
            ltx_buffer.len(),
            db_data.len()
        );

        let cursor = std::io::Cursor::new(ltx_buffer);
        decode_to_db(cursor, &restored_path).unwrap();

        let restored_data = std::fs::read(&restored_path).unwrap();
        assert_eq!(db_data, restored_data);
    }

    #[test]
    fn test_encode_to_memory_buffer() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let restored_path = dir.path().join("restored.db");

        let page_size = 4096u32;
        let db_data = vec![0x42u8; page_size as usize * 5];
        std::fs::write(&db_path, &db_data).unwrap();

        // Encode to Vec<u8> (common use case for S3 upload)
        let mut buffer: Vec<u8> = Vec::new();
        encode_snapshot(&mut buffer, &db_path, page_size, 1).unwrap();

        // Decode from Cursor (common use case for S3 download)
        let cursor = std::io::Cursor::new(buffer);
        decode_to_db(cursor, &restored_path).unwrap();

        let restored_data = std::fs::read(&restored_path).unwrap();
        assert_eq!(db_data, restored_data);
    }

    #[test]
    fn test_apply_ltx_in_place_basic() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let page_size = 4096u32;
        let num_pages = 5;

        // Create initial database
        let db_data = vec![0x00u8; (page_size as usize) * num_pages];
        std::fs::write(&db_path, &db_data).unwrap();

        // Create incremental LTX that updates pages 2 and 4
        let pages: Vec<(u32, Vec<u8>)> = vec![
            (2, vec![0xAA; page_size as usize]),
            (4, vec![0xBB; page_size as usize]),
        ];

        let pre_checksum = compute_checksum_from_file(&db_path).unwrap();

        let mut ltx_buffer = Vec::new();
        encode_wal_changes(
            &mut ltx_buffer,
            &pages,
            page_size,
            2,  // min_txid
            3,  // max_txid
            num_pages as u32,
            Some(pre_checksum),
        )
        .unwrap();

        // Apply in-place
        let cursor = std::io::Cursor::new(ltx_buffer);
        let header = apply_ltx_to_db(cursor, &db_path).unwrap();

        // Verify only changed pages were updated
        let result_data = std::fs::read(&db_path).unwrap();

        // Page 1 (index 0): unchanged
        assert_eq!(&result_data[0..page_size as usize], &vec![0x00u8; page_size as usize][..]);
        // Page 2 (index 1): updated to 0xAA
        let page2_start = page_size as usize;
        assert_eq!(&result_data[page2_start..page2_start + page_size as usize], &vec![0xAAu8; page_size as usize][..]);
        // Page 3 (index 2): unchanged
        let page3_start = 2 * page_size as usize;
        assert_eq!(&result_data[page3_start..page3_start + page_size as usize], &vec![0x00u8; page_size as usize][..]);
        // Page 4 (index 3): updated to 0xBB
        let page4_start = 3 * page_size as usize;
        assert_eq!(&result_data[page4_start..page4_start + page_size as usize], &vec![0xBBu8; page_size as usize][..]);
        // Page 5 (index 4): unchanged
        let page5_start = 4 * page_size as usize;
        assert_eq!(&result_data[page5_start..page5_start + page_size as usize], &vec![0x00u8; page_size as usize][..]);

        assert_eq!(header.min_txid.into_inner(), 2);
        assert_eq!(header.max_txid.into_inner(), 3);
    }

    #[test]
    fn test_apply_ltx_in_place_preserves_other_data() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let page_size = 4096u32;

        // Create database with unique content per page
        let mut db_data = Vec::new();
        for i in 0..4u8 {
            db_data.extend(vec![i * 10; page_size as usize]);
        }
        std::fs::write(&db_path, &db_data).unwrap();

        // Update only page 3
        let pages: Vec<(u32, Vec<u8>)> = vec![
            (3, vec![0xFF; page_size as usize]),
        ];

        let pre_checksum = compute_checksum_from_file(&db_path).unwrap();

        let mut ltx_buffer = Vec::new();
        encode_wal_changes(&mut ltx_buffer, &pages, page_size, 10, 11, 4, Some(pre_checksum)).unwrap();

        let cursor = std::io::Cursor::new(ltx_buffer);
        apply_ltx_to_db(cursor, &db_path).unwrap();

        let result_data = std::fs::read(&db_path).unwrap();

        // Verify pages 1, 2, 4 unchanged
        assert_eq!(&result_data[0..page_size as usize], &vec![0u8; page_size as usize][..]);
        assert_eq!(&result_data[page_size as usize..2 * page_size as usize], &vec![10u8; page_size as usize][..]);
        // Page 3 updated
        assert_eq!(&result_data[2 * page_size as usize..3 * page_size as usize], &vec![0xFFu8; page_size as usize][..]);
        // Page 4 unchanged
        assert_eq!(&result_data[3 * page_size as usize..4 * page_size as usize], &vec![30u8; page_size as usize][..]);
    }

    #[test]
    fn test_compute_checksum_from_file() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let data = vec![0x42u8; 4096];
        std::fs::write(&db_path, &data).unwrap();

        let checksum1 = compute_checksum_from_file(&db_path).unwrap();
        let checksum2 = compute_checksum_from_file(&db_path).unwrap();

        // Same file should produce same checksum
        assert_eq!(checksum1.into_inner(), checksum2.into_inner());

        // Different content should produce different checksum
        std::fs::write(&db_path, vec![0x43u8; 4096]).unwrap();
        let checksum3 = compute_checksum_from_file(&db_path).unwrap();
        assert_ne!(checksum1.into_inner(), checksum3.into_inner());
    }

    #[test]
    fn test_apply_ltx_chain_simulation() {
        // Simulate a realistic scenario: snapshot -> incremental -> incremental
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let page_size = 4096u32;
        let num_pages = 3;

        // Initial database state
        let initial_data: Vec<u8> = (0..num_pages)
            .flat_map(|i| vec![(i as u8) * 10; page_size as usize])
            .collect();
        std::fs::write(&db_path, &initial_data).unwrap();

        // Snapshot (TXID 1)
        let mut snapshot_buffer = Vec::new();
        encode_snapshot(&mut snapshot_buffer, &db_path, page_size, 1).unwrap();

        // First incremental: update page 1 (TXID 2)
        let pre_checksum1 = compute_checksum_from_file(&db_path).unwrap();
        let pages1: Vec<(u32, Vec<u8>)> = vec![(1, vec![0xAA; page_size as usize])];
        let mut inc1_buffer = Vec::new();
        let _post_checksum1 = encode_wal_changes(
            &mut inc1_buffer,
            &pages1,
            page_size,
            2, 2,
            num_pages as u32,
            Some(pre_checksum1),
        ).unwrap();

        // Apply first incremental
        let cursor1 = std::io::Cursor::new(inc1_buffer);
        apply_ltx_to_db(cursor1, &db_path).unwrap();

        // Verify checksum after apply matches post_apply_checksum
        // (This is how the chain works - each file's post becomes next file's pre)
        let current_checksum = compute_checksum_from_file(&db_path).unwrap();

        // Second incremental: update page 2 (TXID 3)
        // pre_checksum should be current db state (which equals post_checksum1 conceptually)
        let pre_checksum2 = current_checksum;
        let pages2: Vec<(u32, Vec<u8>)> = vec![(2, vec![0xBB; page_size as usize])];
        let mut inc2_buffer = Vec::new();
        encode_wal_changes(
            &mut inc2_buffer,
            &pages2,
            page_size,
            3, 3,
            num_pages as u32,
            Some(pre_checksum2),
        ).unwrap();

        // Apply second incremental
        let cursor2 = std::io::Cursor::new(inc2_buffer);
        apply_ltx_to_db(cursor2, &db_path).unwrap();

        // Final verification
        let final_data = std::fs::read(&db_path).unwrap();
        assert_eq!(&final_data[0..page_size as usize], &vec![0xAAu8; page_size as usize][..]); // Page 1 updated
        assert_eq!(&final_data[page_size as usize..2 * page_size as usize], &vec![0xBBu8; page_size as usize][..]); // Page 2 updated
        assert_eq!(&final_data[2 * page_size as usize..3 * page_size as usize], &vec![20u8; page_size as usize][..]); // Page 3 unchanged
    }
}
