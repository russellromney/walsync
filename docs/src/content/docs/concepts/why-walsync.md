---
title: Why Walsync?
description: How Walsync compares to Litestream and other SQLite sync tools
---

# Why Walsync?

Walsync was inspired by [Litestream](https://litestream.io/), the excellent SQLite replication tool. While Litestream pioneered WAL-based replication, Walsync addresses key scalability challenges for multi-database deployments.

## Memory Efficiency at Scale

Both Litestream (v0.5+) and Walsync support multi-database sync from a single process. Walsync's Rust implementation has a smaller memory footprint:

- **Walsync:** ~12 MB baseline (constant regardless of database count)
- **Litestream:** ~33 MB baseline

For resource-constrained environments (512MB VMs, containers, edge deployments), this 21 MB difference matters. See [benchmarks](/concepts/benchmarks/) for detailed measurements.

## Walsync's Multi-Database Architecture

A single Walsync process handles:
- Shared S3 client with connection pooling
- Efficient file watching across all databases
- Consolidated configuration
- Minimal per-database overhead

## When to Use Walsync

**Choose Walsync when:**
- You have multiple SQLite databases to sync
- Memory efficiency matters (containers, edge deployments)
- You need cryptographic data integrity verification
- You want a unified configuration for many databases

**Choose Litestream when:**
- You have a single database
- You need mature, battle-tested replication
- Point-in-time recovery granularity is critical

## Design Philosophy

Walsync is built on these principles:

1. **Efficiency first** - Minimal resource usage, even at scale
2. **Correctness always** - SHA256 checksums verify every sync
3. **Simple operations** - One binary, one process, many databases
4. **Cloud-native** - Built for S3-compatible storage (AWS, Tigris, R2, MinIO)
