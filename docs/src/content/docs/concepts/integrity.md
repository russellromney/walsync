---
title: Data Integrity
description: How Walsync guarantees your data is safe with SHA256 checksums
---

Walsync uses SHA256 cryptographic checksums to guarantee your data arrives exactly as it was sent. Unlike tools that rely on S3's built-in checksums alone, Walsync provides end-to-end verification.

## The Problem

Data corruption can happen at many points:
- Network transmission errors
- S3 storage issues
- Disk failures during upload/download
- Software bugs

Without verification, you might restore a corrupted database and not know until it's too late.

## Walsync's Solution

### Upload Workflow

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│  Read DB     │────▶│  Compute     │────▶│  Upload to   │
│  from disk   │     │  SHA256      │     │  S3 + hash   │
└──────────────┘     └──────────────┘     └──────────────┘
```

1. Read the database file from disk
2. Compute SHA256 hash of the entire file
3. Upload file to S3 with hash stored in object metadata

### Download Workflow

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│  Download    │────▶│  Compute     │────▶│  Compare     │
│  from S3     │     │  SHA256      │     │  hashes      │
└──────────────┘     └──────────────┘     └──────────────┘
                                                  │
                           ┌──────────────────────┴──────────────────────┐
                           │                                             │
                           ▼                                             ▼
                    ┌──────────────┐                              ┌──────────────┐
                    │  Match:      │                              │  Mismatch:   │
                    │  Write file  │                              │  Error!      │
                    └──────────────┘                              └──────────────┘
```

1. Download file and stored hash from S3
2. Compute SHA256 of downloaded data
3. Compare computed hash with stored hash
4. Only write to disk if hashes match

## What Gets Verified

- **Snapshot files** - Full database snapshots
- **WAL segments** - Write-ahead log chunks
- **All byte values** - Tested with 0x00-0xFF to catch encoding issues

## Verification in Practice

### Automatic Verification

Checksums are verified automatically on every restore:

```bash
walsync restore mydb.db --bucket my-bucket -o restored.db
# ✓ Checksum verified: a3f2b9c8d4e5...
```

### Manual Verification

Verify a backup without restoring:

```bash
walsync verify mydb.db --bucket my-bucket
# ✓ All checksums valid
```

## Why SHA256?

- **Cryptographic strength** - Computationally infeasible to find collisions
- **Industry standard** - Used by Git, package managers, TLS
- **Fast** - Hardware acceleration on modern CPUs
- **256-bit output** - 64 hex characters, compact to store

## Comparison with Other Tools

| Tool | Integrity Verification |
|------|----------------------|
| Litestream | S3 checksums only |
| SQLite backup | None |
| rsync | MD5 (weaker) |
| **Walsync** | **SHA256 end-to-end** |
