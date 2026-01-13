---
title: Multi-Database Sync
description: How Walsync efficiently syncs many SQLite databases with one process
---

# Multi-Database Sync

One of Walsync's key advantages is efficiently syncing multiple SQLite databases from a single process.

## Configuration

Create a `walsync.toml` to define multiple databases:

```toml
[storage]
bucket = "my-bucket"
endpoint = "https://fly.storage.tigris.dev"

[[databases]]
path = "/data/users.db"
prefix = "users"

[[databases]]
path = "/data/orders.db"
prefix = "orders"

[[databases]]
path = "/data/analytics.db"
prefix = "analytics"
```

Then start watching all databases:

```bash
walsync watch --config walsync.toml
```

## How It Works

### Shared Resources

Walsync maintains a single:
- **S3 client** with connection pooling for all uploads
- **File watcher** that monitors all database WAL files
- **Configuration context** loaded once at startup

### Per-Database Isolation

Each database still gets:
- Its own S3 prefix (namespace)
- Independent WAL tracking
- Separate checksum verification
- Individual restore capability

### Memory Efficiency

```
┌─────────────────────────────────────┐
│         Walsync Process             │
├─────────────────────────────────────┤
│  Shared S3 Client     (~20MB)       │
│  File Watcher         (~5MB)        │
│  ─────────────────────────────      │
│  Database 1 state     (~500KB)      │
│  Database 2 state     (~500KB)      │
│  Database 3 state     (~500KB)      │
│  ...                                │
└─────────────────────────────────────┘
```

Adding more databases adds minimal overhead (~500KB-1MB each).

## Use Cases

### Multi-Tenant Applications

Each tenant gets their own SQLite database:

```toml
[[databases]]
path = "/data/tenants/acme.db"
prefix = "tenants/acme"

[[databases]]
path = "/data/tenants/globex.db"
prefix = "tenants/globex"
```

### Microservices

Each service manages its own database:

```toml
[[databases]]
path = "/data/auth.db"
prefix = "services/auth"

[[databases]]
path = "/data/billing.db"
prefix = "services/billing"
```

### Edge Deployments

Containers or edge nodes with multiple databases:

```toml
[[databases]]
path = "/app/cache.db"
prefix = "edge/node-1/cache"

[[databases]]
path = "/app/sessions.db"
prefix = "edge/node-1/sessions"
```

## Restoring Individual Databases

Restore any single database without affecting others:

```bash
walsync restore --prefix orders -o /data/orders-restored.db
```

Or restore all databases:

```bash
walsync restore-all --config walsync.toml --output-dir /data/restored/
```
