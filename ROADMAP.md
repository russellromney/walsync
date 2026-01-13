# walsync Roadmap

## Vision

Litestream-compatible SQLite sync in Rust. Optimized for multi-tenant deployments (Cinch, Tenement).

**Core differentiators:**
- LTX format (Litestream-compatible) with SHA256 verification on top
- Lower memory footprint (~12MB vs ~33MB)
- Built-in dashboard + Prometheus metrics
- Opinionated defaults (grandfather/father/son retention)

---

## Alpha (v0.3) - Target Scope

### Core Commands
```bash
walsync watch <db>... [--config file]   # Watch and sync
walsync snapshot <db>                    # Immediate snapshot
walsync restore <name> -o <output>       # Restore database
walsync list                             # List backups
walsync explain [--config file]          # Show trigger schedule
```

### LTX Format Integration
- [x] Add `litetx` dependency (done)
- [x] Basic encode/decode functions (done)
- [ ] Replace raw snapshot uploads with LTX files
- [ ] Store incremental WAL changes as LTX (not raw WAL segments)
- [ ] Point-in-time restore from LTX files

### Sync Triggers
```toml
[sync]
max_changes = 100      # Sync after N WAL frames
max_interval = "10s"   # Or after N seconds (whichever first)

[snapshots]
interval = "1h"        # Full snapshot every hour
on_idle = "5m"         # Or after 5 min idle
on_startup = true      # Snapshot when watch starts
```

### Retention (Grandfather/Father/Son)
```toml
[retention]
hourly = 24            # Keep 24 hourly snapshots
daily = 7              # Keep 7 daily snapshots
weekly = 12            # Keep 12 weekly snapshots
monthly = 12           # Keep 12 monthly snapshots
```

Automatic compaction: merge Level 1 (30s) → Level 2 (5min) → Level 3 (hourly).

### Dashboard + Metrics
- Built-in web UI at `--dashboard-port 8080`
- `/metrics` endpoint for Prometheus
- Shows: databases, last sync, WAL size, next snapshot, errors

### Multi-Database
```toml
[[databases]]
path = "/data/*.db"    # Wildcard support
prefix = "tenant"

[[databases]]
path = "/data/app.db"
prefix = "app"
snapshot_interval = "30m"  # Per-DB override
```

### Data Integrity
- SHA256 checksum in S3 metadata (existing)
- LTX CRC64 checksum (from litetx)
- Reject partial uploads on restore
- Graceful shutdown: complete in-flight uploads (5s timeout)

---

## Post-Alpha Features

### Read Replicas (Poll-based)
```bash
walsync replicate s3://bucket/mydb --local replica.db --interval 5s
```
- Polls S3 for new LTX files
- Applies to local read-only database
- No network required between primary/replica

### Read Replicas (Push-based) - Future
```bash
# Primary
walsync watch mydb.db --push-to http://replica:8080

# Replica
walsync serve --port 8080 --db replica.db
```
- Lower latency than polling
- Requires network connectivity

### Additional Commands
```bash
walsync verify --bucket s3://...        # Verify all checksums
walsync cleanup --dry-run               # Preview deletions
walsync compact --level 2               # Manual compaction
```

---

## Design Decisions

### Single Writer
- Enforced at S3 level (conditional writes)
- No multi-writer support (use orchestration for HA)
- Simpler failure modes

### No Shadow WAL
- Let SQLite checkpoint freely
- Detect WAL reset, take new snapshot
- Trade: more snapshots vs simpler code

### LTX vs Custom Format
- Use `litetx` crate (Superfly/Fly.io maintained)
- Litestream-compatible storage format
- Add SHA256 verification on top of LTX CRC64

### Checkpointing
- Transaction-aware recording (like Litestream v0.5+)
- Don't block SQLite checkpoints
- Re-snapshot when WAL continuity breaks

---

## S3 Layout (LTX-based)

```
s3://bucket/prefix/
├── mydb/
│   ├── 00000001-00000001.ltx     # Snapshot (TXID 1-1)
│   ├── 00000002-00000010.ltx     # Incremental (TXID 2-10)
│   ├── 00000011-00000050.ltx     # Incremental (TXID 11-50)
│   ├── 00000001-00000050.ltx     # Compacted (TXID 1-50)
│   └── manifest.json             # Index of LTX files
└── otherdb/
    └── ...
```

---

## Current Status (v0.2)

- [x] WAL sync to S3/Tigris
- [x] SHA256 checksums in S3 metadata
- [x] Multi-database support (single process)
- [x] Snapshot scheduling
- [x] Point-in-time restore (basic)
- [x] Python bindings
- [x] LTX encode/decode functions

---

## References

- [Litestream Revamped](https://fly.io/blog/litestream-revamped/) - LTX format, multi-DB
- [Litestream v0.5.0](https://fly.io/blog/litestream-v050-is-here/) - Compaction levels
- [litetx crate](https://docs.rs/litetx/) - Rust LTX implementation
- [Litestream How It Works](https://litestream.io/how-it-works/) - WAL mechanics
