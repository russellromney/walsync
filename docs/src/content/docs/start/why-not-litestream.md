---
title: Why Not Litestream?
description: When to use walsync vs Litestream
---

## Litestream is Awesome

[Litestream](https://litestream.io) is an excellent tool for SQLite replication, created by [Ben Johnson](https://github.com/benbjohnson). Walsync wouldn't exist without Litestream's pioneering work on WAL-based replication to S3. We use the same [LTX file format](https://github.com/superfly/ltx) for compatibility and are grateful for the open-source foundation that makes projects like this possible.

**Use Litestream when:**
- Battle-tested production stability is critical
- Your team is familiar with the Go ecosystem
- You want the most mature tooling

## When Walsync Shines

Walsync was built for **resource-constrained environments** and **Rust-native deployments**.

### Memory Efficiency

Walsync's Rust implementation has a smaller memory footprint (~12 MB vs ~33 MB baseline). For resource-constrained environments (small VMs, containers, edge deployments), this difference matters.

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

## The Right Tool

- **Production workloads needing stability**: Use Litestream
- **Rust-native environments**: Use walsync
- **Memory-constrained deployments**: Use walsync
- **Built-in read replica polling**: Use walsync

Both tools solve the same core problem (SQLite backup to S3) and use compatible LTX file formats. Pick the one that fits your stack.
