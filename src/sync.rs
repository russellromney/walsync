use anyhow::{anyhow, Result};
use chrono::Utc;
use notify::{Event, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

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
    /// Last snapshot time
    last_snapshot: Option<chrono::DateTime<Utc>>,
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

        // Check for existing state in S3
        let state_key = format!("{}{}/state.json", prefix, name);
        let (wal_offset, wal_generation) = match s3::download_bytes(&client, &bucket_name, &state_key).await {
            Ok(data) => {
                let state: serde_json::Value = serde_json::from_slice(&data)?;
                (
                    state["wal_offset"].as_u64().unwrap_or(0),
                    state["wal_generation"].as_u64().unwrap_or(0),
                )
            }
            Err(_) => (0, 0),
        };

        tracing::info!(
            "Watching {} (WAL offset: {}, generation: {})",
            db_path.display(),
            wal_offset,
            wal_generation
        );

        db_states.insert(
            db_path.clone(),
            DbState {
                name,
                db_path: db_path.clone(),
                wal_path,
                wal_offset,
                wal_generation,
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

/// Sync WAL changes to S3
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

    // Read new frames
    let (frames, new_offset, frame_count) =
        wal::read_frames_from(&state.wal_path, header.page_size, state.wal_offset).await?;

    if frame_count == 0 {
        return Ok(());
    }

    // Upload WAL segment
    let timestamp = Utc::now().format("%Y%m%d%H%M%S%3f");
    let wal_key = format!(
        "{}{}/wal/{:08}-{}.wal",
        prefix, state.name, state.wal_generation, timestamp
    );

    s3::upload_bytes(client, bucket, &wal_key, frames).await?;

    tracing::info!(
        "{}: Synced {} WAL frames ({} -> {})",
        state.name,
        frame_count,
        state.wal_offset,
        new_offset
    );

    state.wal_offset = new_offset;

    // Save state
    save_state(client, bucket, prefix, state).await?;

    Ok(())
}

/// Take a full database snapshot
async fn take_snapshot(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    prefix: &str,
    state: &mut DbState,
) -> Result<()> {
    let timestamp = Utc::now();
    let snapshot_key = format!(
        "{}{}/snapshots/{}.db",
        prefix,
        state.name,
        timestamp.format("%Y%m%d%H%M%S")
    );

    // Upload the main database file
    s3::upload_file(client, bucket, &snapshot_key, &state.db_path).await?;

    tracing::info!("{}: Snapshot uploaded to {}", state.name, snapshot_key);

    state.last_snapshot = Some(timestamp);

    // Reset WAL tracking after snapshot (checkpoint should have happened or will happen)
    // Note: In production, you might want to trigger a checkpoint here

    Ok(())
}

/// Save sync state to S3
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

/// Restore a database from S3
pub async fn restore(
    name: &str,
    output: &Path,
    bucket: &str,
    endpoint: Option<&str>,
    point_in_time: Option<&str>,
) -> Result<()> {
    let (bucket_name, prefix) = parse_bucket(bucket);
    let client = create_client(endpoint).await?;

    // Find latest snapshot before point_in_time (or just latest)
    let snapshots_prefix = format!("{}{}/snapshots/", prefix, name);
    let snapshots = s3::list_objects(&client, &bucket_name, &snapshots_prefix).await?;

    if snapshots.is_empty() {
        return Err(anyhow!("No snapshots found for database: {}", name));
    }

    // Parse point in time if provided
    let pit = point_in_time
        .map(|s| chrono::DateTime::parse_from_rfc3339(s))
        .transpose()?
        .map(|dt| dt.with_timezone(&Utc));

    // Find best snapshot
    let snapshot_key = if let Some(pit) = pit {
        // Find latest snapshot before point_in_time
        snapshots
            .iter()
            .filter(|k| {
                // Extract timestamp from key
                if let Some(ts) = k.strip_prefix(&snapshots_prefix).and_then(|s| s.strip_suffix(".db")) {
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
        snapshots.last().cloned().ok_or_else(|| anyhow!("No snapshots"))?
    };

    tracing::info!("Restoring from snapshot: {}", snapshot_key);

    // Download snapshot
    s3::download_file(&client, &bucket_name, &snapshot_key, output).await?;

    // Find and apply WAL segments
    let wal_prefix = format!("{}{}/wal/", prefix, name);
    let mut wal_segments = s3::list_objects(&client, &bucket_name, &wal_prefix).await?;
    wal_segments.sort();

    // Filter WAL segments: only those after snapshot
    let snapshot_ts = snapshot_key
        .strip_prefix(&snapshots_prefix)
        .and_then(|s| s.strip_suffix(".db"))
        .unwrap_or("");

    let wal_segments: Vec<_> = wal_segments
        .into_iter()
        .filter(|k| {
            if let Some(ts) = k.strip_prefix(&wal_prefix).and_then(|s| s.strip_suffix(".wal")) {
                // WAL format: {generation}-{timestamp}.wal
                if let Some((_, ts_part)) = ts.split_once('-') {
                    return ts_part > snapshot_ts;
                }
            }
            false
        })
        .collect();

    if !wal_segments.is_empty() {
        tracing::info!("Applying {} WAL segments", wal_segments.len());

        // For now, we just note that WAL replay would happen here
        // Full WAL replay requires SQLite integration
        tracing::warn!("WAL replay not yet implemented - snapshot only restore");
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

/// Take immediate snapshot
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
    let snapshot_key = format!(
        "{}{}/snapshots/{}.db",
        prefix,
        name,
        timestamp.format("%Y%m%d%H%M%S")
    );

    s3::upload_file(&client, &bucket_name, &snapshot_key, database).await?;

    println!("Snapshot uploaded: s3://{}/{}", bucket_name, snapshot_key);
    Ok(())
}
