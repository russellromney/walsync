# walsync

**Lightweight SQLite WAL sync to S3/Tigris.**

Like Litestream but:
- ~7MB binary (vs Litestream's runtime overhead)
- Watch multiple databases at once
- Minimal memory footprint (~5-10MB RSS)

## Installation

```bash
cargo install walsync
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

## License

Apache 2.0
