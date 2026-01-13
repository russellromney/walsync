---
title: Benchmarks
description: Performance benchmarks and comparisons
---

## Memory Usage vs Litestream

The key advantage of walsync is memory efficiency when managing multiple databases.

| Databases | Litestream | Walsync | Savings |
|-----------|-----------|---------|---------|
| 1 | 33 MB (1 process) | 12 MB | **21 MB** |
| 5 | 152 MB (5 processes) | 14 MB | **138 MB** |
| 10 | 286 MB (10 processes) | 12 MB | **274 MB** |
| 20 | 600 MB (20 processes) | 12 MB | **588 MB** |

*Measured on macOS (aarch64) with 100KB test databases, 5-second measurement window.*

**Key observation:** Walsync memory stays constant (~12 MB) regardless of database count. Litestream scales linearly (~30 MB per process).

## Restore Performance

Tested with Tigris (Fly.io S3-compatible storage):

| DB Size | Time (ms) | Throughput (MB/s) |
|---------|-----------|-------------------|
| 1.2 MB | 971 | 1.24 |
| 12 MB | 1,502 | 8.00 |
| 120 MB | 6,954 | 17.27 |

**Key findings:**
- Restore throughput scales with database size
- 17 MB/s for large databases is good for Tigris
- Small databases dominated by connection overhead

## Sync Latency

Time to snapshot a database to Tigris:

```bash
uv run bench/realworld.py --test sync
```

**Measured results** (~100KB database to Tigris):
- **p50:** 445ms
- **p95:** 539ms
- **Mean:** 445ms

Latency is dominated by S3 upload time over residential broadband.

## Write Throughput

Maximum sustainable commits per second with walsync watching:

```bash
uv run bench/realworld.py --test throughput
```

**Measured results** (macOS aarch64):
- **Max commits/sec:** 25,874
- **Avg commit latency:** 0.04ms

Walsync imposes virtually no overhead on SQLite commit performance.

## Checkpoint Impact

SQLite checkpoints merge WAL into the main database. Impact on sync:

```bash
uv run bench/realworld.py --test checkpoint
```

**Measured results:**
- **Normal commit latency:** 0.07ms
- **Post-checkpoint latency:** 0.08ms (minimal impact)
- **Checkpoint duration:** 7ms for 1MB WAL

Checkpoints have negligible impact on write performance.

## Network Recovery

Time to catch up after walsync is restarted:

```bash
uv run bench/realworld.py --test network
```

**Measured results** (5 second simulated outage):
- **Writes during outage:** 49 rows
- **Catchup time:** ~5s (immediate on restart)
- **Data loss:** 0 (WAL preserves all writes)

Strategy: Stop walsync, write for 5 seconds, restart and measure catchup time. All writes made during the outage are synced when walsync restarts.

## Micro-Benchmarks

Internal operation performance from `cargo bench`:

| Operation | Time |
|-----------|------|
| WAL header parse | 47 ns |
| WAL frame header parse | 32 ns |
| SHA256 (1KB) | 6 us |
| SHA256 (100KB) | 512 us |
| SHA256 (1MB) | 5.23 ms |

**Key findings:**
- WAL parsing is extremely fast (< 50ns)
- SHA256 is the bottleneck for checksums
- For 100MB database, checksum takes ~500ms

## Running Benchmarks

### Micro-benchmarks (Rust)
```bash
make bench
# or: cargo bench
```

### Comparison with Litestream
```bash
make bench-compare
# or: uv run bench/compare.py
```

Options:
```bash
uv run bench/compare.py --dbs 1,5,10     # Specific counts
uv run bench/compare.py --duration 10    # Longer measurement
uv run bench/compare.py --db-size 1000   # 1MB test databases
uv run bench/compare.py --json           # JSON output
```

### Real-world benchmarks
```bash
make bench-realworld
# or: uv run bench/realworld.py
```

Options:
```bash
uv run bench/realworld.py --test restore    # Just restore test
uv run bench/realworld.py --sizes 1,10,100  # Specific sizes (MB)
```

## Test Environment

- **Platform:** macOS (aarch64)
- **Rust:** Release build
- **S3 Provider:** Tigris (Fly.io)
- **Network:** Residential broadband

## When to Use Each

**Use Walsync when:**
- Multiple databases (5+)
- Resource-constrained environments (512MB VMs)
- Memory is a concern
- Need explicit SHA256 verification

**Use Litestream when:**
- Single database
- Battle-tested production stability is critical
- Team familiar with Go ecosystem
