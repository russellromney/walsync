---
title: CLI Reference
description: Complete CLI reference with options and examples
---

## Overview

```bash
walsync <COMMAND>

Commands:
  snapshot  Take an immediate snapshot
  watch     Watch SQLite databases and sync WAL changes to S3
  restore   Restore a database from S3
  list      List databases in S3 bucket
  help      Print help for a command
```

---

## snapshot

Take a one-time snapshot of a database to S3.

```bash
walsync snapshot [OPTIONS] --bucket <BUCKET> <DATABASE>
```

### Arguments

| Argument | Description |
|----------|-------------|
| `<DATABASE>` | Path to the SQLite database file |

### Options

| Option | Description |
|--------|-------------|
| `-b, --bucket <BUCKET>` | S3 bucket (required) |
| `--endpoint <ENDPOINT>` | S3 endpoint URL for Tigris/MinIO/etc. Also reads from `AWS_ENDPOINT_URL_S3` |
| `-h, --help` | Print help |

### Examples

```bash
# Snapshot to AWS S3
walsync snapshot myapp.db --bucket my-backups

# Snapshot to Tigris
walsync snapshot myapp.db \
  --bucket my-backups \
  --endpoint https://fly.storage.tigris.dev

# Using environment variable for endpoint
export AWS_ENDPOINT_URL_S3=https://fly.storage.tigris.dev
walsync snapshot myapp.db --bucket my-backups
```

### Output

```
Snapshotting myapp.db to s3://my-backups/myapp.db/...
✓ Snapshot complete (1.2 MB, 445ms)
  Checksum: a3f2b9c8d4e5f6a7b8c9d0e1f2a3b4c5...
```

---

## watch

Continuously watch one or more databases and sync WAL changes to S3.

```bash
walsync watch [OPTIONS] --bucket <BUCKET> <DATABASES>...
```

### Arguments

| Argument | Description |
|----------|-------------|
| `<DATABASES>...` | One or more database files to watch |

### Options

| Option | Description |
|--------|-------------|
| `-b, --bucket <BUCKET>` | S3 bucket (required) |
| `--snapshot-interval <SECONDS>` | Full snapshot interval in seconds (default: 3600 = 1 hour) |
| `--endpoint <ENDPOINT>` | S3 endpoint URL for Tigris/MinIO/etc. Also reads from `AWS_ENDPOINT_URL_S3` |
| `-h, --help` | Print help |

### Examples

```bash
# Watch a single database
walsync watch myapp.db --bucket my-backups

# Watch multiple databases (single process!)
walsync watch app.db users.db analytics.db --bucket my-backups

# Custom snapshot interval (every 30 minutes)
walsync watch myapp.db \
  --bucket my-backups \
  --snapshot-interval 1800

# Watch with Tigris endpoint
walsync watch myapp.db \
  --bucket my-backups \
  --endpoint https://fly.storage.tigris.dev
```

### Output

```
Watching 3 database(s)...
  - app.db
  - users.db
  - analytics.db

[2024-01-15 10:30:00] app.db: WAL sync (4 frames, 16KB)
[2024-01-15 10:30:05] users.db: WAL sync (2 frames, 8KB)
[2024-01-15 11:30:00] app.db: Scheduled snapshot (1.2 MB)
```

### Running as a Service

For production, run walsync as a systemd service:

```ini
# /etc/systemd/system/walsync.service
[Unit]
Description=Walsync SQLite backup
After=network.target

[Service]
Type=simple
User=app
Environment=AWS_ACCESS_KEY_ID=your-key
Environment=AWS_SECRET_ACCESS_KEY=your-secret
Environment=AWS_ENDPOINT_URL_S3=https://fly.storage.tigris.dev
ExecStart=/usr/local/bin/walsync watch \
  /var/lib/app/data.db \
  --bucket my-backups
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

---

## restore

Restore a database from S3 backup.

```bash
walsync restore [OPTIONS] --output <OUTPUT> --bucket <BUCKET> <NAME>
```

### Arguments

| Argument | Description |
|----------|-------------|
| `<NAME>` | Database name as stored in S3 (usually the original filename) |

### Options

| Option | Description |
|--------|-------------|
| `-o, --output <OUTPUT>` | Output path for restored database (required) |
| `-b, --bucket <BUCKET>` | S3 bucket (required) |
| `--endpoint <ENDPOINT>` | S3 endpoint URL for Tigris/MinIO/etc. Also reads from `AWS_ENDPOINT_URL_S3` |
| `--point-in-time <TIMESTAMP>` | Restore to specific point in time (ISO 8601 format) |
| `-h, --help` | Print help |

### Examples

```bash
# Basic restore
walsync restore myapp.db \
  --bucket my-backups \
  --output restored.db

# Restore to specific point in time
walsync restore myapp.db \
  --bucket my-backups \
  --output restored.db \
  --point-in-time "2024-01-15T10:30:00Z"

# Restore from Tigris
walsync restore myapp.db \
  --bucket my-backups \
  --output restored.db \
  --endpoint https://fly.storage.tigris.dev
```

### Output

```
Restoring myapp.db from s3://my-backups/...
  Downloading snapshot... done (1.2 MB)
  Applying WAL segments... done (47 segments)
  Verifying checksum... ✓ a3f2b9c8d4e5f6a7...
✓ Restored to restored.db
```

---

## list

List databases and snapshots stored in S3.

```bash
walsync list [OPTIONS] --bucket <BUCKET>
```

### Options

| Option | Description |
|--------|-------------|
| `-b, --bucket <BUCKET>` | S3 bucket (required) |
| `--endpoint <ENDPOINT>` | S3 endpoint URL for Tigris/MinIO/etc. Also reads from `AWS_ENDPOINT_URL_S3` |
| `-h, --help` | Print help |

### Examples

```bash
# List all databases
walsync list --bucket my-backups

# List with Tigris endpoint
walsync list \
  --bucket my-backups \
  --endpoint https://fly.storage.tigris.dev
```

### Output

```
Databases in s3://my-backups/:

  myapp.db
    Latest snapshot: 2024-01-15 10:30:00 (1.2 MB)
    WAL segments: 47
    Checksum: a3f2b9c8d4e5...

  users.db
    Latest snapshot: 2024-01-15 10:31:00 (256 KB)
    WAL segments: 12
    Checksum: b4c3d2e1f0a9...
```

---

## Environment Variables

Walsync reads these environment variables:

| Variable | Description |
|----------|-------------|
| `AWS_ACCESS_KEY_ID` | AWS/S3 access key |
| `AWS_SECRET_ACCESS_KEY` | AWS/S3 secret key |
| `AWS_ENDPOINT_URL_S3` | S3 endpoint URL (for Tigris, MinIO, etc.) |
| `AWS_REGION` | AWS region (optional, defaults to `auto`) |

### Example Setup

```bash
# For Tigris (Fly.io)
export AWS_ACCESS_KEY_ID=tid_xxxxx
export AWS_SECRET_ACCESS_KEY=tsec_xxxxx
export AWS_ENDPOINT_URL_S3=https://fly.storage.tigris.dev

# For AWS S3
export AWS_ACCESS_KEY_ID=AKIA...
export AWS_SECRET_ACCESS_KEY=...
export AWS_REGION=us-east-1

# For MinIO
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin
export AWS_ENDPOINT_URL_S3=http://localhost:9000
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Invalid arguments |
| 3 | S3 connection error |
| 4 | Database error |
| 5 | Checksum verification failed |
