---
title: Data Integrity
description: How Walsync guarantees your data is safe
---

## SHA256 Checksums

Every snapshot includes a SHA256 checksum stored in S3 metadata.

On restore, the checksum is verified automatically.

## What's Tested

- Byte-for-byte comparison
- All byte values (0x00-0xFF)
- Multi-database scenarios
