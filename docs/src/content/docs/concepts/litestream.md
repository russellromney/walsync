---
title: Why Not Litestream?
description: When to use walsync vs Litestream
---

## Litestream is Awesome

[Litestream](https://litestream.io) is an excellent tool for SQLite replication. We use it extensively and highly recommend it. Walsync is heavily inspired by Litestream's approach to WAL streaming.

**Use Litestream when:**
- You have a single database
- Battle-tested production stability is critical
- Your team is familiar with the Go ecosystem
- You want the most mature tooling

## When Walsync Shines

Walsync was built for a specific use case: **many SQLite databases on resource-constrained servers**.

### The Multi-Database Problem

Litestream runs one process per database. For a single database, this is fine:

| Databases | Litestream Memory |
|-----------|------------------|
| 1 | ~33 MB |

But as you add databases, memory scales linearly:

| Databases | Litestream | Walsync | Savings |
|-----------|-----------|---------|---------|
| 1 | 33 MB | 12 MB | 21 MB |
| 5 | 152 MB | 14 MB | **138 MB** |
| 10 | 286 MB | 12 MB | **274 MB** |
| 20 | 600 MB | 12 MB | **588 MB** |

On a 512MB Fly.io VM running 10 tenant databases, Litestream alone would consume over half your memory.

### Multi-Tenant Architecture

If you're running multi-tenant SQLite (like [Turso](https://turso.tech), [Tenement](https://github.com/russellromney/tenement), or your own setup), walsync lets you back up all tenant databases with a single ~12MB process.

```bash
# Back up all tenant databases with one process
walsync watch \
  /var/lib/app/tenant1/app.db \
  /var/lib/app/tenant2/app.db \
  /var/lib/app/tenant3/app.db \
  -b s3://backups
```

## Feature Comparison

| Feature | Litestream | Walsync |
|---------|-----------|---------|
| WAL streaming | Yes | Yes |
| S3/compatible storage | Yes | Yes |
| Point-in-time restore | Yes | Yes |
| Multi-database | 1 process each | Single process |
| Memory (10 DBs) | ~286 MB | ~12 MB |
| Compression | Built-in | Via SQLite extensions |
| Encryption | Not built-in | Via SQLite extensions |
| SHA256 checksums | Implicit | Explicit in S3 metadata |
| Maturity | Production-proven | Newer |

## The Right Tool

- **1-3 databases on a standard server**: Use Litestream
- **Many databases on a small server**: Use walsync
- **Multi-tenant SQLite**: Use walsync

Both tools solve the same core problem (SQLite backup to S3) with different tradeoffs. Pick the one that fits your architecture.
