---
title: Quick Start
description: Get your first backup running in 5 minutes
---

## Installation

```bash
cargo install walsync
```

## Set Credentials

```bash
export AWS_ACCESS_KEY_ID=your-key
export AWS_SECRET_ACCESS_KEY=your-secret
export AWS_ENDPOINT_URL_S3=https://fly.storage.tigris.dev
```

## Take a Snapshot

```bash
walsync snapshot myapp.db --bucket my-backups
```

## Watch for Changes

```bash
walsync watch myapp.db --bucket my-backups --interval 60
```

## Restore

```bash
walsync restore myapp.db --bucket my-backups --output restored.db
```
