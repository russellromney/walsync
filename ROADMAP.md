# walsync Roadmap

## Current (v0.2)

- [x] WAL sync to S3/Tigris
- [x] SHA256 checksums in S3 metadata
- [x] Multi-database support (single process)
- [x] Snapshot scheduling
- [x] Point-in-time restore
- [x] Python bindings

## Planned

### WAL Replay on Restore
- Currently restore only downloads latest snapshot
- Need to replay WAL segments after snapshot for true point-in-time recovery

### Leader-Follower Replication (`walsync replicate`)
Low-latency real-time replication mode for read replicas.

**Why:** S3 polling/events adds seconds of latency. Direct streaming enables sub-100ms replication.

**Approach:**
- Leader streams WAL frames over TCP to connected followers
- Followers apply frames to local SQLite copy
- Separate from backup mode (different use case)

**Open questions:**
- Protocol: raw TCP vs WebSocket vs QUIC?
- Discovery: static config vs mDNS vs control plane?
- Consistency: async replication or synchronous commit?

### Compression
- Optional gzip/zstd for WAL segments
- Reduce S3 storage and transfer costs

## Not Planned

- Encryption (use S3 server-side encryption)
- Custom binary format (keep it simple, debuggable)
- GUI/web interface
