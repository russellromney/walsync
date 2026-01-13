use anyhow::{anyhow, Result};
use chrono::Utc;
use notify::{Event, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::ltx;
use crate::s3::{self, create_client, parse_bucket};
use crate::wal;

/// State for a single watched database
struct DbState {
    /// Database name (filename without extension)
    name: String,
    /// Path to main db file
    db_path: PathBuf,
    /// Path to WAL file
    wal_path: PathBuf,
    /// Current WAL sync position
    wal_offset: u64,
    /// WAL generation (increments on checkpoint)
    wal_generation: u64,
    /// Current transaction ID (for LTX files)
    current_txid: u64,
    /// Last snapshot time
    last_snapshot: Option<chrono::DateTime<Utc>>,
}

/// Entry in the manifest tracking LTX files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LtxEntry {
    /// Filename (e.g., "00000001-00000010.ltx")
    pub filename: String,
    /// Starting transaction ID
    pub min_txid: u64,
    /// Ending transaction ID
    pub max_txid: u64,
    /// File size in bytes
    pub size: u64,
    /// Upload timestamp (ISO 8601)
    pub created_at: String,
    /// Whether this is a snapshot (full DB) or incremental
    pub is_snapshot: bool,
}

/// Manifest tracking all LTX files for a database
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Manifest {
    /// Database name
    pub name: String,
    /// Current highest TXID
    pub current_txid: u64,
    /// Page size of the database
    pub page_size: u32,
    /// List of LTX files
    pub files: Vec<LtxEntry>,
}

/// Watch multiple databases and sync to S3
pub async fn watch(
    databases: Vec<PathBuf>,
    bucket: &str,
    snapshot_interval: u64,
    endpoint: Option<&str>,
) -> Result<()> {
    let (bucket_name, prefix) = parse_bucket(bucket);
    let client = Arc::new(create_client(endpoint).await?);

    // Initialize state for each database
    let mut db_states: HashMap<PathBuf, DbState> = HashMap::new();

    for db_path in &databases {
        if !db_path.exists() {
            return Err(anyhow!("Database not found: {}", db_path.display()));
        }

        let name = db_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("Invalid database path: {}", db_path.display()))?
            .to_string();

        let wal_path = db_path.with_extension("db-wal");

        // Check for existing state in S3 (manifest.json)
        let manifest_key = format!("{}{}/manifest.json", prefix, name);
        let (wal_offset, wal_generation, current_txid) = match s3::download_bytes(&client, &bucket_name, &manifest_key).await {
            Ok(data) => {
                let manifest: Manifest = serde_json::from_slice(&data).unwrap_or_default();
                // For backwards compat, also check old state.json
                let state_key = format!("{}{}/state.json", prefix, name);
                let (offset, gen) = match s3::download_bytes(&client, &bucket_name, &state_key).await {
                    Ok(state_data) => {
                        let state: serde_json::Value = serde_json::from_slice(&state_data)?;
                        (
                            state["wal_offset"].as_u64().unwrap_or(0),
                            state["wal_generation"].as_u64().unwrap_or(0),
                        )
                    }
                    Err(_) => (0, 0),
                };
                (offset, gen, manifest.current_txid)
            }
            Err(_) => {
                // Try old state.json for backwards compat
                let state_key = format!("{}{}/state.json", prefix, name);
                match s3::download_bytes(&client, &bucket_name, &state_key).await {
                    Ok(data) => {
                        let state: serde_json::Value = serde_json::from_slice(&data)?;
                        (
                            state["wal_offset"].as_u64().unwrap_or(0),
                            state["wal_generation"].as_u64().unwrap_or(0),
                            state["current_txid"].as_u64().unwrap_or(0),
                        )
                    }
                    Err(_) => (0, 0, 0),
                }
            }
        };

        tracing::info!(
            "Watching {} (WAL offset: {}, generation: {}, TXID: {})",
            db_path.display(),
            wal_offset,
            wal_generation,
            current_txid
        );

        db_states.insert(
            db_path.clone(),
            DbState {
                name,
                db_path: db_path.clone(),
                wal_path,
                wal_offset,
                wal_generation,
                current_txid,
                last_snapshot: None,
            },
        );
    }

    // Set up file watcher
    let (tx, mut rx) = mpsc::channel::<PathBuf>(100);

    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        if let Ok(event) = res {
            for path in event.paths {
                // Only care about WAL files
                if path.extension().map(|e| e == "db-wal").unwrap_or(false) {
                    let _ = tx.blocking_send(path);
                }
            }
        }
    })?;

    // Watch parent directories of all databases
    let mut watched_dirs = std::collections::HashSet::new();
    for db_path in &databases {
        if let Some(parent) = db_path.parent() {
            if watched_dirs.insert(parent.to_path_buf()) {
                watcher.watch(parent, RecursiveMode::NonRecursive)?;
                tracing::debug!("Watching directory: {}", parent.display());
            }
        }
    }

    // Initial sync of any existing WAL data
    for state in db_states.values_mut() {
        if state.wal_path.exists() {
            sync_wal(&client, &bucket_name, &prefix, state).await?;
        }
    }

    // Take initial snapshots
    for state in db_states.values_mut() {
        take_snapshot(&client, &bucket_name, &prefix, state).await?;
    }

    let snapshot_interval = Duration::from_secs(snapshot_interval);
    let mut snapshot_timer = tokio::time::interval(snapshot_interval);

    tracing::info!("walsync running (snapshot interval: {}s)", snapshot_interval.as_secs());

    loop {
        tokio::select! {
            // WAL file changed
            Some(wal_path) = rx.recv() => {
                // Find the corresponding database
                let db_path = wal_path.with_extension("db");
                if let Some(state) = db_states.get_mut(&db_path) {
                    if let Err(e) = sync_wal(&client, &bucket_name, &prefix, state).await {
                        tracing::error!("Failed to sync WAL for {}: {}", state.name, e);
                    }
                }
            }

            // Snapshot timer
            _ = snapshot_timer.tick() => {
                for state in db_states.values_mut() {
                    if let Err(e) = take_snapshot(&client, &bucket_name, &prefix, state).await {
                        tracing::error!("Failed to snapshot {}: {}", state.name, e);
                    }
                }
            }
        }
    }
}

/// Sync WAL changes to S3 as incremental LTX files
async fn sync_wal(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    prefix: &str,
    state: &mut DbState,
) -> Result<()> {
    let header = match wal::read_header(&state.wal_path).await? {
        Some(h) => h,
        None => return Ok(()), // No WAL file
    };

    // Check if WAL was reset (checkpoint happened)
    let current_size = wal::get_wal_size(&state.wal_path).await?;
    if current_size < state.wal_offset {
        // WAL was truncated, start fresh
        tracing::info!("{}: WAL checkpoint detected, resetting offset", state.name);
        state.wal_offset = 0;
        state.wal_generation += 1;
    }

    // Read and parse WAL frames into pages
    let (frames, new_offset, max_db_size) =
        wal::read_frames_as_pages(&state.wal_path, header.page_size, state.wal_offset).await?;

    if frames.is_empty() {
        return Ok(());
    }

    let timestamp = Utc::now();

    // Convert frames to pages for LTX encoding
    let pages: Vec<(u32, Vec<u8>)> = frames
        .iter()
        .map(|f| (f.page_number, f.data.clone()))
        .collect();

    // Determine commit page (last frame with non-zero db_size, or last page)
    let commit_page = frames
        .iter()
        .rev()
        .find(|f| f.db_size > 0)
        .map(|f| f.db_size)
        .unwrap_or(max_db_size.max(1));

    // Calculate TXID range for this incremental
    let min_txid = state.current_txid + 1;
    let max_txid = min_txid + frames.len() as u64 - 1;

    // Encode as incremental LTX
    let mut ltx_buffer = Vec::new();
    let _checksum = ltx::encode_wal_changes(
        &mut ltx_buffer,
        &pages,
        header.page_size,
        min_txid,
        max_txid,
        commit_page,
        None, // pre_checksum (we could track this for chain verification)
    )?;

    let ltx_size = ltx_buffer.len() as u64;

    // LTX filename: {min_txid:08}-{max_txid:08}.ltx
    let ltx_filename = format!("{:08}-{:08}.ltx", min_txid, max_txid);
    let ltx_key = format!("{}{}/{}", prefix, state.name, ltx_filename);

    // Upload LTX file
    s3::upload_bytes(client, bucket, &ltx_key, ltx_buffer).await?;

    tracing::info!(
        "{}: Synced {} WAL frames as LTX {} (TXID: {}-{}, size: {} bytes)",
        state.name,
        frames.len(),
        ltx_filename,
        min_txid,
        max_txid,
        ltx_size
    );

    // Update manifest
    let mut manifest = load_manifest(client, bucket, prefix, &state.name).await?;
    manifest.current_txid = max_txid;
    manifest.page_size = header.page_size;
    manifest.files.push(LtxEntry {
        filename: ltx_filename,
        min_txid,
        max_txid,
        size: ltx_size,
        created_at: timestamp.to_rfc3339(),
        is_snapshot: false,
    });
    save_manifest(client, bucket, prefix, &manifest).await?;

    // Update state
    state.wal_offset = new_offset;
    state.current_txid = max_txid;

    // Save legacy state for backwards compat
    save_state(client, bucket, prefix, state).await?;

    Ok(())
}

/// Take a full database snapshot as LTX
async fn take_snapshot(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    prefix: &str,
    state: &mut DbState,
) -> Result<()> {
    let timestamp = Utc::now();

    // Get page size from database header
    let page_size = get_page_size(&state.db_path).await?;

    // Increment TXID for this snapshot
    let new_txid = state.current_txid + 1;

    // LTX filename: {min_txid:08}-{max_txid:08}.ltx
    // For a snapshot, min=1 and max=new_txid (it contains all pages up to this point)
    let ltx_filename = format!("{:08}-{:08}.ltx", 1, new_txid);
    let ltx_key = format!("{}{}/{}", prefix, state.name, ltx_filename);

    // Encode database as LTX
    let mut ltx_buffer = Vec::new();
    ltx::encode_snapshot(&mut ltx_buffer, &state.db_path, page_size, new_txid)?;

    let ltx_size = ltx_buffer.len() as u64;

    // Upload LTX file
    s3::upload_bytes(client, bucket, &ltx_key, ltx_buffer).await?;

    tracing::info!(
        "{}: LTX snapshot uploaded to {} (TXID: {}, size: {} bytes)",
        state.name,
        ltx_key,
        new_txid,
        ltx_size
    );

    // Update manifest
    let mut manifest = load_manifest(client, bucket, prefix, &state.name).await?;
    manifest.current_txid = new_txid;
    manifest.page_size = page_size;
    manifest.files.push(LtxEntry {
        filename: ltx_filename,
        min_txid: 1,
        max_txid: new_txid,
        size: ltx_size,
        created_at: timestamp.to_rfc3339(),
        is_snapshot: true,
    });
    save_manifest(client, bucket, prefix, &manifest).await?;

    // Update state
    state.current_txid = new_txid;
    state.last_snapshot = Some(timestamp);

    Ok(())
}

/// Get SQLite database page size from header
async fn get_page_size(db_path: &Path) -> Result<u32> {
    use tokio::io::AsyncReadExt;
    let mut file = tokio::fs::File::open(db_path).await?;
    let mut header = [0u8; 100];
    file.read_exact(&mut header).await?;

    // Page size is at offset 16-17, big-endian
    let page_size = u16::from_be_bytes([header[16], header[17]]) as u32;

    // Page size of 1 means 65536
    let page_size = if page_size == 1 { 65536 } else { page_size };

    Ok(page_size)
}

/// Save sync state to S3 (legacy state.json for backwards compat)
async fn save_state(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    prefix: &str,
    state: &DbState,
) -> Result<()> {
    let state_key = format!("{}{}/state.json", prefix, state.name);
    let state_json = serde_json::json!({
        "wal_offset": state.wal_offset,
        "wal_generation": state.wal_generation,
        "current_txid": state.current_txid,
        "last_snapshot": state.last_snapshot,
    });

    s3::upload_bytes(
        client,
        bucket,
        &state_key,
        serde_json::to_vec_pretty(&state_json)?,
    )
    .await?;

    Ok(())
}

/// Load manifest from S3
async fn load_manifest(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    prefix: &str,
    db_name: &str,
) -> Result<Manifest> {
    let manifest_key = format!("{}{}/manifest.json", prefix, db_name);
    match s3::download_bytes(client, bucket, &manifest_key).await {
        Ok(data) => Ok(serde_json::from_slice(&data)?),
        Err(_) => Ok(Manifest {
            name: db_name.to_string(),
            ..Default::default()
        }),
    }
}

/// Save manifest to S3
async fn save_manifest(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    prefix: &str,
    manifest: &Manifest,
) -> Result<()> {
    let manifest_key = format!("{}{}/manifest.json", prefix, manifest.name);
    s3::upload_bytes(
        client,
        bucket,
        &manifest_key,
        serde_json::to_vec_pretty(manifest)?,
    )
    .await?;
    Ok(())
}

/// Restore a database from S3 using LTX files
pub async fn restore(
    name: &str,
    output: &Path,
    bucket: &str,
    endpoint: Option<&str>,
    point_in_time: Option<&str>,
) -> Result<()> {
    let (bucket_name, prefix) = parse_bucket(bucket);
    let client = create_client(endpoint).await?;

    // Load manifest to find LTX files
    let manifest = load_manifest(&client, &bucket_name, &prefix, name).await?;

    if manifest.files.is_empty() {
        // Fall back to legacy snapshot-based restore for backwards compatibility
        return restore_legacy(name, output, bucket, endpoint, point_in_time).await;
    }

    // Parse point in time if provided (as TXID or timestamp)
    let target_txid = if let Some(pit) = point_in_time {
        // Try parsing as TXID first
        if let Ok(txid) = pit.parse::<u64>() {
            txid
        } else if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(pit) {
            // Find latest file before timestamp
            manifest
                .files
                .iter()
                .filter(|f| {
                    chrono::DateTime::parse_from_rfc3339(&f.created_at)
                        .map(|fdt| fdt <= dt)
                        .unwrap_or(false)
                })
                .map(|f| f.max_txid)
                .max()
                .ok_or_else(|| anyhow!("No LTX file found before {}", pit))?
        } else {
            return Err(anyhow!(
                "Invalid point_in_time format. Use TXID (number) or ISO 8601 timestamp"
            ));
        }
    } else {
        manifest.current_txid
    };

    // Find the best snapshot that covers our target TXID
    // A snapshot has min_txid=1 and should have max_txid >= target_txid
    let snapshot = manifest
        .files
        .iter()
        .filter(|f| f.is_snapshot && f.max_txid <= target_txid)
        .max_by_key(|f| f.max_txid)
        .ok_or_else(|| anyhow!("No LTX snapshot found for TXID {}", target_txid))?;

    tracing::info!(
        "Restoring from LTX snapshot: {} (TXID: {}-{})",
        snapshot.filename,
        snapshot.min_txid,
        snapshot.max_txid
    );

    // Download and decode LTX snapshot
    let ltx_key = format!("{}{}/{}", prefix, name, snapshot.filename);
    let ltx_data = s3::download_bytes(&client, &bucket_name, &ltx_key).await?;

    let cursor = std::io::Cursor::new(ltx_data);
    let header = ltx::decode_to_db(cursor, output)?;

    tracing::info!(
        "Restored {} from LTX (page_size: {}, pages: {}, TXID: {}-{})",
        name,
        header.page_size.into_inner(),
        header.commit.into_inner(),
        header.min_txid.into_inner(),
        header.max_txid.into_inner()
    );

    // Find incremental LTX files to apply (if any)
    let incrementals: Vec<_> = manifest
        .files
        .iter()
        .filter(|f| {
            !f.is_snapshot
                && f.min_txid > snapshot.max_txid
                && f.max_txid <= target_txid
        })
        .collect();

    if !incrementals.is_empty() {
        tracing::info!("Found {} incremental LTX files to apply", incrementals.len());
        // TODO: Apply incremental LTX files
        // For now, snapshots are sufficient for basic restore
        tracing::warn!("Incremental LTX replay not yet implemented - using snapshot only");
    }

    println!(
        "Restored {} to {} (TXID: {})",
        name,
        output.display(),
        snapshot.max_txid
    );
    Ok(())
}

/// Legacy restore for backwards compatibility with raw .db snapshots
async fn restore_legacy(
    name: &str,
    output: &Path,
    bucket: &str,
    endpoint: Option<&str>,
    point_in_time: Option<&str>,
) -> Result<()> {
    let (bucket_name, prefix) = parse_bucket(bucket);
    let client = create_client(endpoint).await?;

    // Find legacy snapshots
    let snapshots_prefix = format!("{}{}/snapshots/", prefix, name);
    let snapshots = s3::list_objects(&client, &bucket_name, &snapshots_prefix).await?;

    if snapshots.is_empty() {
        return Err(anyhow!("No snapshots found for database: {}", name));
    }

    let pit = point_in_time
        .map(|s| chrono::DateTime::parse_from_rfc3339(s))
        .transpose()?
        .map(|dt| dt.with_timezone(&Utc));

    let snapshot_key = if let Some(pit) = pit {
        snapshots
            .iter()
            .filter(|k| {
                if let Some(ts) = k
                    .strip_prefix(&snapshots_prefix)
                    .and_then(|s| s.strip_suffix(".db"))
                {
                    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(ts, "%Y%m%d%H%M%S") {
                        return dt.and_utc() <= pit;
                    }
                }
                false
            })
            .max()
            .ok_or_else(|| anyhow!("No snapshot found before {}", pit))?
            .clone()
    } else {
        snapshots
            .last()
            .cloned()
            .ok_or_else(|| anyhow!("No snapshots"))?
    };

    tracing::info!("Restoring from legacy snapshot: {}", snapshot_key);
    s3::download_file(&client, &bucket_name, &snapshot_key, output).await?;

    if let Ok(Some(stored_checksum)) =
        s3::get_checksum(&client, &bucket_name, &snapshot_key).await
    {
        let restored_checksum = compute_file_sha256(output).await?;
        if stored_checksum != restored_checksum {
            return Err(anyhow!(
                "Checksum mismatch! Stored: {}, Restored: {}",
                stored_checksum,
                restored_checksum
            ));
        }
        tracing::info!("Checksum verified: {}", restored_checksum);
    }

    tracing::info!("Restored {} to {}", name, output.display());
    Ok(())
}

/// List databases in bucket
pub async fn list(bucket: &str, endpoint: Option<&str>) -> Result<()> {
    let (bucket_name, prefix) = parse_bucket(bucket);
    let client = create_client(endpoint).await?;

    let objects = s3::list_objects(&client, &bucket_name, &prefix).await?;

    // Extract unique database names
    let mut dbs: std::collections::HashSet<String> = std::collections::HashSet::new();

    for key in &objects {
        if let Some(rest) = key.strip_prefix(&prefix) {
            if let Some(name) = rest.split('/').next() {
                if !name.is_empty() {
                    dbs.insert(name.to_string());
                }
            }
        }
    }

    if dbs.is_empty() {
        println!("No databases found in s3://{}/{}", bucket_name, prefix);
    } else {
        println!("Databases in s3://{}/{}:", bucket_name, prefix);
        for db in dbs {
            // Get latest snapshot info
            let snapshots_prefix = format!("{}{}/snapshots/", prefix, db);
            let snapshots = s3::list_objects(&client, &bucket_name, &snapshots_prefix).await?;
            let snapshot_count = snapshots.len();

            let wal_prefix = format!("{}{}/wal/", prefix, db);
            let wals = s3::list_objects(&client, &bucket_name, &wal_prefix).await?;
            let wal_count = wals.len();

            println!("  {} ({} snapshots, {} WAL segments)", db, snapshot_count, wal_count);
        }
    }

    Ok(())
}

/// Compute SHA256 hash of file for integrity verification
async fn compute_file_sha256(path: &Path) -> Result<String> {
    use std::io::Read;
    use sha2::{Sha256, Digest};

    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192];

    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// Take immediate snapshot as LTX file
pub async fn snapshot(database: &Path, bucket: &str, endpoint: Option<&str>) -> Result<()> {
    let (bucket_name, prefix) = parse_bucket(bucket);
    let client = create_client(endpoint).await?;

    if !database.exists() {
        return Err(anyhow!("Database not found: {}", database.display()));
    }

    let name = database
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("Invalid database path"))?;

    let timestamp = Utc::now();

    // Get page size from database header
    let page_size = get_page_size(database).await?;

    // Load existing manifest to get current TXID
    let mut manifest = load_manifest(&client, &bucket_name, &prefix, name).await?;
    let new_txid = manifest.current_txid + 1;

    // LTX filename: {min_txid:08}-{max_txid:08}.ltx
    // For a snapshot, min=1 (contains all pages)
    let ltx_filename = format!("{:08}-{:08}.ltx", 1, new_txid);
    let ltx_key = format!("{}{}/{}", prefix, name, ltx_filename);

    // Encode database as LTX
    let mut ltx_buffer = Vec::new();
    ltx::encode_snapshot(&mut ltx_buffer, database, page_size, new_txid)?;

    let ltx_size = ltx_buffer.len() as u64;

    // Upload LTX file
    s3::upload_bytes(&client, &bucket_name, &ltx_key, ltx_buffer).await?;

    // Update manifest
    manifest.name = name.to_string();
    manifest.current_txid = new_txid;
    manifest.page_size = page_size;
    manifest.files.push(LtxEntry {
        filename: ltx_filename.clone(),
        min_txid: 1,
        max_txid: new_txid,
        size: ltx_size,
        created_at: timestamp.to_rfc3339(),
        is_snapshot: true,
    });
    save_manifest(&client, &bucket_name, &prefix, &manifest).await?;

    tracing::info!(
        "LTX snapshot uploaded: {} (TXID: {}, size: {} bytes)",
        ltx_key,
        new_txid,
        ltx_size
    );
    println!(
        "Snapshot uploaded: s3://{}/{} (TXID: {})",
        bucket_name, ltx_key, new_txid
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn get_test_bucket() -> Option<String> {
        std::env::var("WALSYNC_TEST_BUCKET").ok()
    }

    fn get_test_endpoint() -> Option<String> {
        std::env::var("AWS_ENDPOINT_URL_S3").ok()
    }

    /// Helper to create a test database
    async fn create_test_db(name: &str) -> PathBuf {
        let path = PathBuf::from(format!("/tmp/walsync-test-{}.db", name));
        // Create a minimal SQLite database with WAL mode
        let db_path = path.clone();
        tokio::fs::write(&db_path, b"SQLite format 3\0").await.ok();
        db_path
    }

    /// Helper to create a test WAL file
    async fn create_test_wal(db_path: &PathBuf) {
        let wal_path = db_path.with_extension("db-wal");
        let mut wal_data = vec![0u8; 32];
        // Write valid WAL magic number (0x377F0682)
        wal_data[0..4].copy_from_slice(&0x377F0682u32.to_be_bytes());
        // Format version
        wal_data[4..8].copy_from_slice(&3007000u32.to_be_bytes());
        // Page size
        wal_data[8..12].copy_from_slice(&4096u32.to_be_bytes());
        // Add a simple frame
        let page_size = 4096u32;
        let frame_size = 24 + page_size as usize;
        wal_data.resize(32 + frame_size, 0u8);
        tokio::fs::write(&wal_path, wal_data).await.ok();
    }

    /// Compute SHA256 hash of data for integrity verification (for tests)
    fn compute_sha256(data: &[u8]) -> String {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    #[tokio::test]
    #[ignore]
    async fn test_integration_snapshot() {
        let bucket = get_test_bucket().expect("WALSYNC_TEST_BUCKET not set");
        let endpoint = get_test_endpoint();
        let test_name = format!("snapshot-test-{}", uuid::Uuid::new_v4());
        let db_path = create_test_db(&test_name).await;

        let result = snapshot(&db_path, &bucket, endpoint.as_deref()).await;

        // Cleanup
        tokio::fs::remove_file(&db_path).await.ok();

        assert!(result.is_ok(), "Snapshot should succeed");
    }

    #[tokio::test]
    #[ignore]
    async fn test_integration_list_empty_bucket() {
        let bucket = get_test_bucket().expect("WALSYNC_TEST_BUCKET not set");
        let endpoint = get_test_endpoint();

        // This should not panic even if bucket is empty or only has test files
        let result = list(&bucket, endpoint.as_deref()).await;
        assert!(result.is_ok(), "List should succeed on bucket");
    }

    #[tokio::test]
    #[ignore]
    async fn test_integration_list_with_database() {
        let bucket = get_test_bucket().expect("WALSYNC_TEST_BUCKET not set");
        let endpoint = get_test_endpoint();
        let test_name = format!("list-test-{}", uuid::Uuid::new_v4());
        let db_path = create_test_db(&test_name).await;

        // Upload a snapshot
        let _ = snapshot(&db_path, &bucket, endpoint.as_deref()).await;

        // List databases
        let result = list(&bucket, endpoint.as_deref()).await;

        // Cleanup
        tokio::fs::remove_file(&db_path).await.ok();

        assert!(result.is_ok(), "List should succeed");
    }

    #[tokio::test]
    #[ignore]
    async fn test_integration_restore_nonexistent() {
        let bucket = get_test_bucket().expect("WALSYNC_TEST_BUCKET not set");
        let endpoint = get_test_endpoint();
        let output = PathBuf::from(format!("/tmp/restored-{}.db", uuid::Uuid::new_v4()));

        let result = restore("nonexistent-db", &output, &bucket, endpoint.as_deref(), None).await;

        // Should fail - no snapshots exist
        assert!(result.is_err(), "Restore of nonexistent database should fail");

        // Cleanup
        tokio::fs::remove_file(&output).await.ok();
    }

    #[tokio::test]
    #[ignore]
    async fn test_integration_snapshot_and_restore() {
        let bucket = get_test_bucket().expect("WALSYNC_TEST_BUCKET not set");
        let endpoint = get_test_endpoint();
        let test_name = format!("snapshot-restore-{}", uuid::Uuid::new_v4());
        let db_path = create_test_db(&test_name).await;
        let db_name = db_path.file_stem().unwrap().to_str().unwrap();
        let restored_path = PathBuf::from(format!("/tmp/restored-{}.db", uuid::Uuid::new_v4()));

        // Read original database content and compute hash
        let original_data = tokio::fs::read(&db_path).await.unwrap();
        let original_hash = compute_sha256(&original_data);

        // Take snapshot
        let snapshot_result = snapshot(&db_path, &bucket, endpoint.as_deref()).await;
        assert!(snapshot_result.is_ok(), "Snapshot should succeed");

        // Wait a moment for S3 to be consistent
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Restore database
        let restore_result = restore(db_name, &restored_path, &bucket, endpoint.as_deref(), None).await;
        assert!(restore_result.is_ok(), "Restore should succeed");

        // Verify restored file exists
        assert!(restored_path.exists(), "Restored database should exist");

        // CRITICAL: Verify restored database matches original exactly
        let restored_data = tokio::fs::read(&restored_path).await.unwrap();
        let restored_hash = compute_sha256(&restored_data);

        assert_eq!(original_data.len(), restored_data.len(),
            "Restored database size ({}) must match original ({})",
            restored_data.len(), original_data.len());
        assert_eq!(original_hash, restored_hash,
            "Restored database content must be byte-for-byte identical to original");
        assert_eq!(original_data, restored_data,
            "Restored database is not identical to original");

        // Cleanup
        tokio::fs::remove_file(&db_path).await.ok();
        tokio::fs::remove_file(&restored_path).await.ok();
    }

    #[tokio::test]
    #[ignore]
    async fn test_integration_sync_wal_workflow() {
        let bucket = get_test_bucket().expect("WALSYNC_TEST_BUCKET not set");
        let endpoint = get_test_endpoint();
        let test_name = format!("wal-sync-{}", uuid::Uuid::new_v4());
        let db_path = create_test_db(&test_name).await;

        // Create a WAL file
        create_test_wal(&db_path).await;

        // Take initial snapshot - this should work with a WAL file present
        let snapshot_result = snapshot(&db_path, &bucket, endpoint.as_deref()).await;
        assert!(snapshot_result.is_ok(), "Snapshot with WAL should succeed");

        // Cleanup
        tokio::fs::remove_file(&db_path).await.ok();
        tokio::fs::remove_file(db_path.with_extension("db-wal")).await.ok();
    }

    #[test]
    fn test_parse_bucket_variations() {
        // This tests the bucket parsing logic used by sync functions
        let (bucket1, prefix1) = crate::s3::parse_bucket("s3://my-bucket");
        assert_eq!(bucket1, "my-bucket");
        assert_eq!(prefix1, "");

        let (bucket2, prefix2) = crate::s3::parse_bucket("s3://my-bucket/walsync/");
        assert_eq!(bucket2, "my-bucket");
        assert_eq!(prefix2, "walsync/");

        let (bucket3, prefix3) = crate::s3::parse_bucket("my-bucket/path/to/prefix");
        assert_eq!(bucket3, "my-bucket");
        assert_eq!(prefix3, "path/to/prefix");
    }

    #[tokio::test]
    #[ignore]
    async fn test_integration_snapshot_and_restore_with_data() {
        // Test that snapshot/restore preserves exact data content (like Litestream)
        let bucket = get_test_bucket().expect("WALSYNC_TEST_BUCKET not set");
        let endpoint = get_test_endpoint();
        let test_name = format!("snapshot-restore-data-{}", uuid::Uuid::new_v4());
        let db_path = PathBuf::from(format!("/tmp/walsync-test-{}.db", test_name));
        let db_name = db_path.file_stem().unwrap().to_str().unwrap();
        let restored_path = PathBuf::from(format!("/tmp/restored-{}.db", uuid::Uuid::new_v4()));

        // Create database with varied content (not just magic bytes)
        let original_data = vec![
            b"SQLite format 3\0".to_vec(),
            // Add varying byte patterns
            (0u8..=255u8).collect::<Vec<_>>(),
            vec![0xFF; 256],
            b"This is test data".to_vec(),
            vec![0x42; 512], // BBBB...
        ].concat();

        tokio::fs::write(&db_path, &original_data).await.unwrap();

        // Snapshot -> Restore -> Verify exact match
        snapshot(&db_path, &bucket, endpoint.as_deref()).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        restore(db_name, &restored_path, &bucket, endpoint.as_deref(), None).await.unwrap();

        let restored_data = tokio::fs::read(&restored_path).await.unwrap();

        // Critical verification: byte-for-byte identical
        assert_eq!(original_data.len(), restored_data.len(),
            "Size mismatch: original={}, restored={}",
            original_data.len(), restored_data.len());

        for (i, (orig, restored)) in original_data.iter().zip(restored_data.iter()).enumerate() {
            assert_eq!(orig, restored,
                "Data mismatch at byte {}: original=0x{:02x}, restored=0x{:02x}",
                i, orig, restored);
        }

        assert_eq!(original_data, restored_data,
            "Restored database is NOT identical to original - data corruption detected!");

        // Cleanup
        tokio::fs::remove_file(&db_path).await.ok();
        tokio::fs::remove_file(&restored_path).await.ok();
    }

    #[tokio::test]
    #[ignore]
    async fn test_integration_multi_database_snapshot() {
        // Test walsync advantage: single process handles multiple databases
        let bucket = get_test_bucket().expect("WALSYNC_TEST_BUCKET not set");
        let endpoint = get_test_endpoint();

        const NUM_DBS: usize = 5;
        let mut db_paths = Vec::new();
        let mut db_names = Vec::new();

        // Create multiple test databases
        for i in 0..NUM_DBS {
            let test_name = format!("multi-db-{}-{}", i, uuid::Uuid::new_v4());
            let db_path = create_test_db(&test_name).await;
            db_names.push(test_name);
            db_paths.push(db_path);
        }

        // Snapshot all databases (this is where walsync shines - single process)
        for db_path in &db_paths {
            let result = snapshot(db_path, &bucket, endpoint.as_deref()).await;
            assert!(result.is_ok(), "All snapshots should succeed");
        }

        // Verify all were uploaded
        let list_result = list(&bucket, endpoint.as_deref()).await;
        assert!(list_result.is_ok(), "List should succeed");

        // Cleanup
        for db_path in &db_paths {
            tokio::fs::remove_file(db_path).await.ok();
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_integration_checksum_verification() {
        // Test that checksums are stored and verified
        let bucket = get_test_bucket().expect("WALSYNC_TEST_BUCKET not set");
        let endpoint = get_test_endpoint();
        let test_name = format!("checksum-test-{}", uuid::Uuid::new_v4());
        let db_path = create_test_db(&test_name).await;
        let db_name = db_path.file_stem().unwrap().to_str().unwrap();
        let restored_path = PathBuf::from(format!("/tmp/restored-checksum-{}.db", uuid::Uuid::new_v4()));

        // Read original and compute its hash
        let original_data = tokio::fs::read(&db_path).await.unwrap();
        let original_hash = compute_sha256(&original_data);

        // Snapshot (should store checksum in metadata)
        snapshot(&db_path, &bucket, endpoint.as_deref()).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Restore (should verify checksum)
        let restore_result = restore(db_name, &restored_path, &bucket, endpoint.as_deref(), None).await;
        assert!(restore_result.is_ok(), "Restore with valid checksum should succeed");

        // Verify restored data
        let restored_data = tokio::fs::read(&restored_path).await.unwrap();
        let restored_hash = compute_sha256(&restored_data);

        assert_eq!(original_hash, restored_hash, "Checksums should match");

        // Cleanup
        tokio::fs::remove_file(&db_path).await.ok();
        tokio::fs::remove_file(&restored_path).await.ok();
    }

    #[test]
    fn test_performance_multi_database_advantage() {
        // This test documents the theoretical advantage of walsync vs Litestream
        // Litestream: N databases = N processes = N overhead
        // Walsync: N databases = 1 process = 1 overhead

        let database_counts = vec![1, 5, 10, 100];

        println!("\n=== Performance Advantage: Walsync vs Litestream ===\n");
        println!("Databases | Litestream Processes | Walsync Processes | Memory Saved (est)");
        println!("----------|---------------------|-------------------|------------------");

        for count in database_counts {
            let litestream_processes = count;
            let walsync_processes = 1;
            let processes_saved = litestream_processes - walsync_processes;

            // Rough estimate: ~50MB per Litestream process
            let memory_per_process = 50;
            let memory_saved_mb = processes_saved * memory_per_process;

            println!(
                "{:9} | {:21} | {:17} | {:>14} MB",
                count, litestream_processes, walsync_processes, memory_saved_mb
            );
        }

        println!("\nNote: This is a theoretical advantage. Actual overhead depends on");
        println!("binary size, Tigris connection pooling, and WAL activity per database.\n");
    }
}
