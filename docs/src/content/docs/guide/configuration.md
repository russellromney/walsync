---
title: Configuration
description: Complete configuration guide for walsync
---

Walsync is configured through environment variables and CLI options. This guide covers all configuration options and common deployment patterns.

## Environment Variables

### Required: S3 Credentials

| Variable | Description | Example |
|----------|-------------|---------|
| `AWS_ACCESS_KEY_ID` | S3 access key | `AKIA...` or `tid_...` |
| `AWS_SECRET_ACCESS_KEY` | S3 secret key | `wJalr...` or `tsec_...` |

### Optional: S3 Configuration

| Variable | Description | Default |
|----------|-------------|---------|
| `AWS_ENDPOINT_URL_S3` | Custom S3 endpoint | AWS S3 |
| `AWS_REGION` | AWS region | `auto` |
| `RUST_LOG` | Log level | `walsync=info` |

## S3 Provider Setup

### Tigris (Fly.io)

Tigris is an S3-compatible object store from Fly.io with global distribution.

```bash
# Create a bucket
fly storage create my-walsync-bucket

# Get credentials (shown after creation)
export AWS_ACCESS_KEY_ID=tid_xxxxxxxxxxxxx
export AWS_SECRET_ACCESS_KEY=tsec_xxxxxxxxxxxxx
export AWS_ENDPOINT_URL_S3=https://fly.storage.tigris.dev
```

### AWS S3

```bash
# Create IAM user with S3 access, then:
export AWS_ACCESS_KEY_ID=AKIA...
export AWS_SECRET_ACCESS_KEY=...
export AWS_REGION=us-east-1
```

**Recommended IAM Policy:**

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": [
        "s3:GetObject",
        "s3:PutObject",
        "s3:DeleteObject",
        "s3:ListBucket"
      ],
      "Resource": [
        "arn:aws:s3:::my-walsync-bucket",
        "arn:aws:s3:::my-walsync-bucket/*"
      ]
    }
  ]
}
```

### Cloudflare R2

```bash
export AWS_ACCESS_KEY_ID=...  # R2 access key
export AWS_SECRET_ACCESS_KEY=...  # R2 secret key
export AWS_ENDPOINT_URL_S3=https://<account-id>.r2.cloudflarestorage.com
```

### MinIO (Self-Hosted)

```bash
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin
export AWS_ENDPOINT_URL_S3=http://localhost:9000
```

### Backblaze B2

```bash
export AWS_ACCESS_KEY_ID=...  # B2 application key ID
export AWS_SECRET_ACCESS_KEY=...  # B2 application key
export AWS_ENDPOINT_URL_S3=https://s3.us-west-002.backblazeb2.com
```

## CLI Options Reference

### Global Options

These options are available on all commands:

| Option | Environment Variable | Description |
|--------|---------------------|-------------|
| `--endpoint <URL>` | `AWS_ENDPOINT_URL_S3` | Custom S3 endpoint |
| `-b, --bucket <BUCKET>` | - | S3 bucket name (required) |

### watch Options

| Option | Default | Description |
|--------|---------|-------------|
| `--snapshot-interval <SECONDS>` | `3600` | Full snapshot interval (1 hour) |

**Tuning snapshot interval:**

- **Lower values (300-900):** More frequent snapshots, faster recovery, more S3 requests
- **Default (3600):** Good balance for most workloads
- **Higher values (7200+):** Less S3 overhead, longer recovery time

### restore Options

| Option | Description |
|--------|-------------|
| `-o, --output <PATH>` | Output path for restored database (required) |
| `--point-in-time <ISO8601>` | Restore to specific timestamp |

## Logging

Walsync uses the standard Rust `RUST_LOG` environment variable:

```bash
# Default: info level for walsync
export RUST_LOG=walsync=info

# Debug logging
export RUST_LOG=walsync=debug

# Trace logging (very verbose)
export RUST_LOG=walsync=trace

# Include AWS SDK logs
export RUST_LOG=walsync=debug,aws_config=debug
```

## Deployment Patterns

### systemd Service

For production Linux servers:

```ini
# /etc/systemd/system/walsync.service
[Unit]
Description=Walsync SQLite Backup
After=network.target

[Service]
Type=simple
User=app
Group=app
WorkingDirectory=/var/lib/app

# Credentials
Environment=AWS_ACCESS_KEY_ID=tid_xxxxx
Environment=AWS_SECRET_ACCESS_KEY=tsec_xxxxx
Environment=AWS_ENDPOINT_URL_S3=https://fly.storage.tigris.dev
Environment=RUST_LOG=walsync=info

# Command
ExecStart=/usr/local/bin/walsync watch \
  /var/lib/app/data.db \
  --bucket my-backups \
  --snapshot-interval 1800

# Restart policy
Restart=always
RestartSec=5

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/app

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
sudo systemctl daemon-reload
sudo systemctl enable walsync
sudo systemctl start walsync
sudo journalctl -u walsync -f  # View logs
```

### Docker

```dockerfile
FROM rust:1.75-slim as builder
WORKDIR /app
RUN cargo install walsync

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/walsync /usr/local/bin/

ENTRYPOINT ["walsync"]
```

Docker Compose:

```yaml
version: '3.8'
services:
  app:
    image: myapp
    volumes:
      - app-data:/data

  walsync:
    image: walsync
    command: watch /data/app.db --bucket my-backups
    environment:
      AWS_ACCESS_KEY_ID: ${AWS_ACCESS_KEY_ID}
      AWS_SECRET_ACCESS_KEY: ${AWS_SECRET_ACCESS_KEY}
      AWS_ENDPOINT_URL_S3: https://fly.storage.tigris.dev
    volumes:
      - app-data:/data:ro
    depends_on:
      - app

volumes:
  app-data:
```

### Fly.io

For Fly.io deployments with Tigris:

```toml
# fly.toml
[env]
  AWS_ENDPOINT_URL_S3 = "https://fly.storage.tigris.dev"

[[mounts]]
  source = "data"
  destination = "/data"
```

Set secrets:

```bash
fly secrets set AWS_ACCESS_KEY_ID=tid_xxxxx
fly secrets set AWS_SECRET_ACCESS_KEY=tsec_xxxxx
```

### Kubernetes

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: walsync
spec:
  replicas: 1
  selector:
    matchLabels:
      app: walsync
  template:
    metadata:
      labels:
        app: walsync
    spec:
      containers:
      - name: walsync
        image: walsync:latest
        args:
          - watch
          - /data/app.db
          - --bucket
          - my-backups
        env:
        - name: AWS_ACCESS_KEY_ID
          valueFrom:
            secretKeyRef:
              name: walsync-secrets
              key: aws-access-key-id
        - name: AWS_SECRET_ACCESS_KEY
          valueFrom:
            secretKeyRef:
              name: walsync-secrets
              key: aws-secret-access-key
        - name: AWS_ENDPOINT_URL_S3
          value: https://fly.storage.tigris.dev
        volumeMounts:
        - name: data
          mountPath: /data
          readOnly: true
      volumes:
      - name: data
        persistentVolumeClaim:
          claimName: app-data
```

## Multi-Database Configuration

Watch multiple databases with a single process:

```bash
walsync watch \
  /var/lib/app1/data.db \
  /var/lib/app2/data.db \
  /var/lib/app3/data.db \
  --bucket my-backups \
  --endpoint https://fly.storage.tigris.dev
```

Each database is stored in S3 under its filename:
- `s3://my-backups/data.db/` (first)
- `s3://my-backups/data.db/` (conflicts!)

**Use unique filenames** or **bucket prefixes** for multiple databases with the same name:

```bash
# Option 1: Unique filenames
walsync watch app1.db app2.db app3.db --bucket my-backups

# Option 2: Different buckets/prefixes
walsync watch app.db --bucket tenant-1-backups
walsync watch app.db --bucket tenant-2-backups
```

## Security Best Practices

1. **Use IAM roles** instead of access keys when possible (AWS EC2, ECS, Lambda)

2. **Restrict bucket access** with minimal IAM permissions

3. **Enable bucket versioning** for additional protection:
   ```bash
   aws s3api put-bucket-versioning \
     --bucket my-backups \
     --versioning-configuration Status=Enabled
   ```

4. **Use server-side encryption**:
   ```bash
   aws s3api put-bucket-encryption \
     --bucket my-backups \
     --server-side-encryption-configuration '{
       "Rules": [{"ApplyServerSideEncryptionByDefault": {"SSEAlgorithm": "AES256"}}]
     }'
   ```

5. **Rotate credentials** regularly and use secrets management

6. **Set lifecycle rules** to delete old WAL segments:
   ```json
   {
     "Rules": [{
       "ID": "DeleteOldWAL",
       "Status": "Enabled",
       "Filter": {"Prefix": "*/wal/"},
       "Expiration": {"Days": 30}
     }]
   }
   ```

## Troubleshooting

### Connection Issues

```bash
# Test S3 connectivity
aws s3 ls s3://my-bucket --endpoint-url $AWS_ENDPOINT_URL_S3

# Enable debug logging
RUST_LOG=walsync=debug,aws_config=debug walsync list --bucket my-bucket
```

### Permission Errors

Ensure your credentials have `s3:PutObject`, `s3:GetObject`, `s3:ListBucket`, and `s3:DeleteObject` permissions.

### Database Locked

If you see "database is locked" errors, ensure only one walsync process is watching each database.
