---
title: Why Walsync?
description: How Walsync compares to Litestream and other SQLite sync tools
---

# Why Walsync?

Walsync was inspired by [Litestream](https://litestream.io/), the excellent SQLite replication tool. While Litestream pioneered WAL-based replication, Walsync addresses key scalability challenges for multi-database deployments.

## The Problem with One-Process-Per-Database

Litestream follows a one-process-per-database model. This works great for single-database applications, but creates significant overhead when you need to sync many databases:

| Databases | Litestream Processes | Memory Overhead |
|-----------|---------------------|-----------------|
| 1 | 1 | ~50MB |
| 5 | 5 | ~250MB |
| 10 | 10 | ~500MB |
| 100 | 100 | ~5GB |

Each Litestream process maintains its own:
- S3 client and connection pool
- WAL monitoring thread
- Configuration state
- Memory buffers

## Walsync's Multi-Database Architecture

Walsync uses a single process to sync multiple databases:

| Databases | Walsync Processes | Memory Overhead |
|-----------|------------------|-----------------|
| 1 | 1 | ~30MB |
| 5 | 1 | ~35MB |
| 10 | 1 | ~40MB |
| 100 | 1 | ~80MB |

One process handles:
- Shared S3 client with connection pooling
- Efficient file watching across all databases
- Consolidated configuration
- Minimal per-database overhead

## Feature Comparison

| Feature | Litestream | Walsync |
|---------|------------|---------|
| WAL replication | Yes | Yes |
| Point-in-time recovery | Yes | Yes |
| S3-compatible storage | Yes | Yes |
| Multi-database (single process) | No | Yes |
| Data integrity verification | No | Yes (SHA256) |
| Memory efficiency | Good | Excellent |
| Rust implementation | No (Go) | Yes |
| Python bindings | No | Yes |

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
