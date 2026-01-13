# Walsync Benchmark Results

Run date: 2026-01-13

## Real-World Performance

### Restore Performance (Tigris)

| DB Size | Time (ms) | Throughput (MB/s) |
|---------|-----------|-------------------|
| 1.2 MB  | 971       | 1.24              |
| 12 MB   | 1,502     | 8.00              |
| 120 MB  | 6,954     | 17.27             |

**Key findings:**
- Restore throughput scales with database size
- 17 MB/s for large databases is good for Tigris
- Small databases dominated by connection overhead

### Multi-Database Throughput

**Configuration:** 10 databases, 100 writes/sec

| Metric | Value |
|--------|-------|
| CPU Usage | 0.9% |
| Memory | 17.6 MB |
| Total Processes | 1 |

**Key findings:**
- Single process handles 10 databases efficiently
- ~18 MB total memory (vs ~500 MB for litestream with 10 processes)
- Very low CPU overhead

### Sync Latency (commit → S3)

**Status:** Test needs refinement

The sync latency benchmark timed out waiting for S3 objects to appear. This indicates either:
1. WAL segments are batched before upload (expected behavior)
2. Polling mechanism needs adjustment
3. File watcher delay in detecting WAL changes

**Next steps:**
- Add logging to track when walsync detects WAL changes
- Measure from file write → S3 upload complete
- Test with forced checkpoints

### Network Recovery

**Status:** Not yet implemented

Placeholder benchmark for measuring recovery time after network outages.

## Micro-Benchmarks (CPU)

From `cargo bench`:

| Operation | Time |
|-----------|------|
| WAL header parse | 47 ns |
| WAL frame header parse | 32 ns |
| SHA256 (1KB) | 6 μs |
| SHA256 (100KB) | 512 μs |
| SHA256 (1MB) | 5.23 ms |

**Key findings:**
- WAL parsing is extremely fast (< 50ns)
- SHA256 is the bottleneck for checksums
- For 100MB database, checksum takes ~500ms

## Comparison vs Litestream

### Memory Usage (Measured)

| DBs | Litestream | Walsync | Savings |
|-----|-----------|---------|---------|
| 1   | 35 MB     | ~0 MB   | **35 MB** |
| 5   | 158 MB    | ~0 MB   | **158 MB** |
| 10  | 327 MB    | 1 MB    | **326 MB** |
| 20  | 685 MB    | 3 MB    | **682 MB** |

*Measured on macOS with 100KB test databases, 5 second measurement window*

### Process Model

- **Litestream**: 1 process per database
- **Walsync**: 1 process for N databases

### When to Use Each

**Use Walsync when:**
- Multiple databases (5+)
- Resource-constrained environments
- Memory is a concern
- Need explicit SHA256 verification

**Use Litestream when:**
- Single database
- Battle-tested production stability is critical
- Team familiar with Go ecosystem

## Test Environment

- **Platform:** macOS (aarch64)
- **Rust:** 1.x (release build)
- **S3 Provider:** Tigris (Fly.io)
- **Network:** Residential broadband

## Running Benchmarks

```bash
# All real-world benchmarks
make bench-realworld

# Specific test
python bench/realworld.py --test restore

# Compare with litestream (requires litestream installed)
make bench-compare

# Micro-benchmarks
make bench
```

## Future Improvements

1. **Fix sync latency test** - Track actual upload times instead of polling
2. **Add write throughput test** - How many commits/sec can walsync handle?
3. **Network failure recovery** - Simulate network outage and measure catchup
4. **Checkpoint impact** - Measure sync lag during SQLite checkpoint
5. **WAL replay benchmark** - Once implemented, measure replay speed
