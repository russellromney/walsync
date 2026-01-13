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

### Documentation
- **Litestream acknowledgment** - Credit Litestream, explain when to use it vs walsync
- **Performance benchmarks** - Memory, CPU, latency comparisons across scenarios (in progress: see `bench/`)

## Not Planned

Walsync is intentionally simple. These features are out of scope:

- **Compression** - Use SQLite extensions (sqlite-zstd); WAL inherits it automatically
- **Encryption** - Use SQLCipher or SQLite SEE; WAL inherits it automatically
- **Compaction** - WAL segments stay as-is; use S3 lifecycle rules for cleanup
- **LTX format** - No custom binary format like Litestream; raw WAL is debuggable
- **Retention policies** - Use S3 lifecycle rules
- **GUI/web interface** - CLI-first tool for developers
- **Restore UI** - Just `walsync restore`, no wizards
