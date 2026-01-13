<p align="center">
  <img src="logo.svg" alt="Walsync" width="200">
</p>

# walsync

**Lightweight SQLite WAL sync to S3/Tigris with explicit data integrity verification.**

Like Litestream but:
- ✅ **Explicit SHA256 checksums** - Stored in S3 metadata, verified on restore
- ✅ **Production-grade data integrity** - Byte-for-byte database reconstruction guaranteed
- ✅ **Multi-database efficiency** - Single process handles N databases (vs N Litestream processes)
- ✅ **~7MB binary** - Rust vs Go runtime overhead
- ✅ **Minimal memory** - ~12MB RSS regardless of database count

## Installation

### CLI (Rust)
```bash
cargo install walsync
```

### Python Package
```bash
pip install walsync
```

Then use from Python:
```python
from walsync import WalSync

# Create instance
ws = WalSync("s3://my-bucket", endpoint="https://fly.storage.tigris.dev")

# Snapshot a database
ws.snapshot("/path/to/app.db")

# List backed up databases
dbs = ws.list()

# Restore a database
ws.restore("app", "/path/to/restored.db")
```

## Quick Start

```bash
# Watch databases and sync to S3
walsync watch db1.db db2.db -b s3://my-bucket/backups

# With Tigris endpoint
walsync watch app.db -b s3://my-bucket --endpoint https://fly.storage.tigris.dev

# With auto-compaction after each snapshot
walsync watch app.db -b s3://my-bucket --compact-after-snapshot

# Take immediate snapshot
walsync snapshot app.db -b s3://my-bucket

# List backed up databases
walsync list -b s3://my-bucket

# Restore database
walsync restore mydb -o restored.db -b s3://my-bucket

# Clean up old snapshots (dry-run)
walsync compact mydb -b s3://my-bucket

# Actually delete old snapshots
walsync compact mydb -b s3://my-bucket --force
```

## How It Works

```
Local:                          S3:
app.db                          /app/snapshots/20240110120000.db
app.db-wal  ────────────────►   /app/wal/00000001-20240110120001234.wal
           (file watcher)       /app/wal/00000001-20240110120005678.wal
                                /app/state.json
```

1. **Watch** - Monitor WAL files for changes (inotify/kqueue)
2. **Sync** - Upload new WAL frames to S3 incrementally
3. **Snapshot** - Periodic full database snapshots (configurable interval)
4. **Restore** - Download snapshot + replay WAL segments

## Commands

### `walsync watch`

Watch databases and continuously sync WAL changes.

```bash
walsync watch <DATABASES>... -b <BUCKET> [OPTIONS]

Options:
  --snapshot-interval <SECS>    Snapshot interval (default: 3600)
  --endpoint <URL>              S3 endpoint (for Tigris/MinIO)
  --compact-after-snapshot      Run compaction after each snapshot
  --compact-interval <SECS>     Compaction interval in seconds (0 = disabled)
  --retain-hourly <N>           Hourly snapshots to keep (default: 24)
  --retain-daily <N>            Daily snapshots to keep (default: 7)
  --retain-weekly <N>           Weekly snapshots to keep (default: 12)
  --retain-monthly <N>          Monthly snapshots to keep (default: 12)
```

### `walsync snapshot`

Take an immediate snapshot.

```bash
walsync snapshot <DATABASE> -b <BUCKET>
```

### `walsync restore`

Restore a database from S3.

```bash
walsync restore <NAME> -o <OUTPUT> -b <BUCKET>

Options:
  --point-in-time <ISO8601>  Restore to specific time
```

### `walsync compact`

Clean up old snapshots using retention policy (GFS rotation).

```bash
walsync compact <NAME> -b <BUCKET> [OPTIONS]

Options:
  --hourly <N>    Hourly snapshots to keep (default: 24)
  --daily <N>     Daily snapshots to keep (default: 7)
  --weekly <N>    Weekly snapshots to keep (default: 12)
  --monthly <N>   Monthly snapshots to keep (default: 12)
  --force         Actually delete files (default: dry-run only)
```

**Example:**
```bash
# Preview what would be deleted
walsync compact mydb -b s3://my-bucket

# Actually delete old snapshots
walsync compact mydb -b s3://my-bucket --force

# Keep more hourly snapshots
walsync compact mydb -b s3://my-bucket --hourly 48 --force
```

### `walsync list`

List backed up databases.

```bash
walsync list -b <BUCKET>
```

## Environment Variables

- `AWS_ACCESS_KEY_ID` - AWS/Tigris access key
- `AWS_SECRET_ACCESS_KEY` - AWS/Tigris secret key
- `AWS_ENDPOINT_URL_S3` - S3 endpoint (for Tigris/MinIO)
- `AWS_REGION` - AWS region (default: us-east-1)

## S3 Layout

```
s3://bucket/prefix/
├── dbname/
│   ├── snapshots/
│   │   ├── 20240110120000.db
│   │   └── 20240110130000.db
│   ├── wal/
│   │   ├── 00000001-20240110120001234.wal
│   │   ├── 00000001-20240110120005678.wal
│   │   └── ...
│   └── state.json
└── otherdb/
    └── ...
```

## Data Integrity

### SHA256 Verification
Every snapshot includes an SHA256 checksum stored in S3 object metadata (`x-amz-meta-sha256`). During restore, checksums are automatically verified:

```
✓ Checksum stored during snapshot
✓ Verified automatically on restore
✓ Fail-fast on corruption detection
✓ Works with existing backups (optional)
```

See [DATA_INTEGRITY.md](DATA_INTEGRITY.md) for complete testing details.

### Snapshot Compaction

Walsync uses Grandfather/Father/Son (GFS) rotation to manage snapshot retention:

| Tier | Default | Description |
|------|---------|-------------|
| Hourly | 24 | Snapshots from last 24 hours |
| Daily | 7 | One per day for last week |
| Weekly | 12 | One per week for last 12 weeks |
| Monthly | 12 | One per month beyond 12 weeks |

**Safety guarantees:**
- Always keeps latest snapshot
- Minimum 2 snapshots retained
- Dry-run by default (--force required to delete)

**Auto-compaction modes:**
```bash
# After each snapshot
walsync watch app.db -b s3://bucket --compact-after-snapshot

# On interval (every hour)
walsync watch app.db -b s3://bucket --compact-interval 3600
```

### Multi-Database Scalability

| Databases | Litestream | Walsync | Savings |
|-----------|-----------|---------|---------|
| 1 | 33 MB (1 process) | 12 MB (1 process) | **21 MB** |
| 5 | 152 MB (5 processes) | 14 MB (1 process) | **138 MB** |
| 10 | 286 MB (10 processes) | 12 MB (1 process) | **274 MB** |
| 20 | 600 MB (20 processes) | 12 MB (1 process) | **588 MB** |

*Measured on macOS with 100KB test databases. See [BENCHMARK_RESULTS.md](BENCHMARK_RESULTS.md) for full results.*

Single walsync process handles multiple databases with shared S3 connection pooling.

## Testing

32 comprehensive tests covering:
- ✅ Byte-for-byte data integrity (snapshot → restore → verify)
- ✅ SHA256 checksum storage and verification
- ✅ Multi-database concurrent snapshots
- ✅ WAL file format parsing
- ✅ S3 operations

Run tests: `./run_tests.sh` (requires Tigris credentials in `.env`)

## Use with Tenement/Slum

Perfect for backing up tenant SQLite databases:

```bash
# In your tenement deployment
walsync watch \
  /var/lib/ourfam/romneys/app.db \
  /var/lib/ourfam/smiths/app.db \
  /var/lib/ourfam/jones/app.db \
  -b s3://backups/ourfam \
  --endpoint https://fly.storage.tigris.dev
```

All databases sync with single process, saving ~275MB memory vs Litestream for 10 databases.

## Documentation

- [ROADMAP.md](ROADMAP.md) - Planned features and direction
- [BENCHMARK_RESULTS.md](BENCHMARK_RESULTS.md) - Performance benchmark results
- [CHECKSUM_STRATEGY.md](CHECKSUM_STRATEGY.md) - SHA256 implementation strategy
- [DATA_INTEGRITY.md](DATA_INTEGRITY.md) - Data integrity guarantees and testing
- [TESTING.md](TESTING.md) - Comprehensive testing guide
- [bench/](bench/) - Performance benchmarks (micro, comparison, real-world)

## License

Apache 2.0
