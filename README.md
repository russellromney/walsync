# walsync

**Lightweight SQLite WAL sync to S3/Tigris with explicit data integrity verification.**

Like Litestream but:
- ✅ **Explicit SHA256 checksums** - Stored in S3 metadata, verified on restore
- ✅ **Production-grade data integrity** - Byte-for-byte database reconstruction guaranteed
- ✅ **Multi-database efficiency** - Single process handles N databases (vs N Litestream processes)
- ✅ **~7MB binary** - Rust vs Go runtime overhead
- ✅ **Minimal memory** - ~5-10MB RSS for 5+ databases

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

# Take immediate snapshot
walsync snapshot app.db -b s3://my-bucket

# List backed up databases
walsync list -b s3://my-bucket

# Restore database
walsync restore mydb -o restored.db -b s3://my-bucket
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
  --snapshot-interval <SECS>  Snapshot interval (default: 3600)
  --endpoint <URL>            S3 endpoint (for Tigris/MinIO)
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

### Multi-Database Scalability

| Scenario | Litestream | Walsync | Savings |
|----------|-----------|---------|---------|
| 5 databases | 5 processes × 50MB | 1 process × 10MB | **200 MB** |
| 10 databases | 10 processes × 50MB | 1 process × 10MB | **450 MB** |
| 100 databases | 100 processes × 50MB | 1 process × 10MB | **4950 MB** |

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

All databases sync with single process, saving 200MB+ memory vs Litestream.

## Documentation

- [CHECKSUM_STRATEGY.md](CHECKSUM_STRATEGY.md) - SHA256 implementation strategy
- [DATA_INTEGRITY.md](DATA_INTEGRITY.md) - Data integrity guarantees and testing
- [TESTING.md](TESTING.md) - Comprehensive testing guide

## License

Apache 2.0
