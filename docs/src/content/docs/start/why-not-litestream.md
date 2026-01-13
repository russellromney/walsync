---
title: Why Not Litestream?
description: When to use walsync vs Litestream
---

## Litestream is Awesome

[Litestream](https://litestream.io) is an excellent tool for SQLite replication. Walsync is heavily inspired by Litestream's approach to WAL streaming and uses the same LTX file format for compatibility.

**Use Litestream when:**
- Battle-tested production stability is critical
- Your team is familiar with the Go ecosystem
- You want the most mature tooling

## When Walsync Shines

Walsync was built for **resource-constrained environments** and **Rust-native deployments**.

### Memory Efficiency

Walsync's Rust implementation has a smaller memory footprint:

| Databases | Litestream | Walsync | Savings |
|-----------|-----------|---------|---------|
| 1 | ~33 MB | ~12 MB | 21 MB |
| 10 | ~35 MB | ~12 MB | 23 MB |

*Note: Litestream v0.5+ supports multi-database in a single process.*

### Native Rust Integration

If you're building in Rust, walsync provides:
- Native async/await with tokio
- No CGO dependencies
- Direct library integration (coming soon)
- Smaller binary size (~8 MB vs ~15 MB)

### Multi-Tenant Architecture

For multi-tenant SQLite deployments:

```bash
# Back up all tenant databases with one process
walsync watch \
  /var/lib/app/tenant1/app.db \
  /var/lib/app/tenant2/app.db \
  /var/lib/app/tenant3/app.db \
  -b s3://backups
```

### Read Replicas

Walsync includes built-in read replica support:

```bash
# Create a read replica that polls for updates
walsync replicate s3://bucket/mydb --local replica.db --interval 5s
```

## Feature Comparison

| Feature | Litestream | Walsync |
|---------|-----------|---------|
| WAL streaming | Yes | Yes |
| S3/compatible storage | Yes | Yes |
| Point-in-time restore | Yes | Yes |
| Multi-database | Yes (v0.5+) | Yes |
| LTX format | Yes | Yes (compatible) |
| Read replicas | Via restore | Built-in polling |
| Compression | LZ4 | LZ4 |
| Language | Go | Rust |
| Memory footprint | ~33 MB | ~12 MB |
| Maturity | Production-proven | Alpha |

## The Right Tool

- **Production workloads needing stability**: Use Litestream
- **Rust-native environments**: Use walsync
- **Memory-constrained deployments**: Use walsync
- **Built-in read replica polling**: Use walsync

Both tools solve the same core problem (SQLite backup to S3) and use compatible LTX file formats. Pick the one that fits your stack.
