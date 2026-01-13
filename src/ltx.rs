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

/// Decode an LTX file and reconstruct the database
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
    fn test_snapshot_roundtrip() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let ltx_path = dir.path().join("test.ltx");
        let restored_path = dir.path().join("restored.db");

        // Create a simple SQLite database (4KB page size, 1 page)
        let page_size = 4096u32;
        let db_data = vec![0x42u8; page_size as usize]; // Single page of 0x42
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
    }
}
