---
title: Deployment
description: Deploy walsync in production environments
---

Production deployment guides for different platforms.

## systemd (Linux)

The recommended way to run walsync on Linux servers.

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

**Enable and start:**

```bash
sudo systemctl daemon-reload
sudo systemctl enable walsync
sudo systemctl start walsync
```

**View logs:**

```bash
sudo journalctl -u walsync -f
```

**Check status:**

```bash
sudo systemctl status walsync
```

## Docker

### Dockerfile

```dockerfile
FROM rust:1.75-slim as builder
WORKDIR /app
RUN cargo install walsync

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/walsync /usr/local/bin/

ENTRYPOINT ["walsync"]
```

### Docker Compose

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
      RUST_LOG: walsync=info
    volumes:
      - app-data:/data:ro
    depends_on:
      - app
    restart: always

volumes:
  app-data:
```

**Run:**

```bash
docker-compose up -d
docker-compose logs -f walsync
```

## Fly.io

### fly.toml

```toml
app = "myapp"

[env]
  AWS_ENDPOINT_URL_S3 = "https://fly.storage.tigris.dev"
  RUST_LOG = "walsync=info"

[[mounts]]
  source = "data"
  destination = "/data"

[processes]
  app = "myapp"
  walsync = "walsync watch /data/app.db --bucket my-backups"
```

### Set secrets

```bash
fly secrets set AWS_ACCESS_KEY_ID=tid_xxxxx
fly secrets set AWS_SECRET_ACCESS_KEY=tsec_xxxxx
```

### Scale the walsync process

```bash
fly scale count walsync=1
```

## Kubernetes

### Deployment

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
        - name: RUST_LOG
          value: walsync=info
        volumeMounts:
        - name: data
          mountPath: /data
          readOnly: true
      volumes:
      - name: data
        persistentVolumeClaim:
          claimName: app-data
```

### Secret

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: walsync-secrets
type: Opaque
stringData:
  aws-access-key-id: tid_xxxxx
  aws-secret-access-key: tsec_xxxxx
```

### Apply

```bash
kubectl apply -f walsync-secret.yaml
kubectl apply -f walsync-deployment.yaml
kubectl logs -f deployment/walsync
```

## Sidecar Pattern

Run walsync as a sidecar container alongside your application:

```yaml
apiVersion: v1
kind: Pod
metadata:
  name: app-with-backup
spec:
  containers:
  - name: app
    image: myapp
    volumeMounts:
    - name: data
      mountPath: /data

  - name: walsync
    image: walsync
    args: ["watch", "/data/app.db", "--bucket", "my-backups"]
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
    emptyDir: {}
```

## Health Checks

Walsync exposes a Prometheus metrics endpoint at `http://127.0.0.1:16767/metrics` (configurable via `--metrics-port`, disable with `--no-metrics`).

**Monitoring options:**

1. **Metrics endpoint:** Scrape `/metrics` for Prometheus metrics (localhost only)
2. **Process status:** Check if the walsync process is running
3. **S3 objects:** Check for recent WAL uploads
4. **Logs:** Monitor for errors in logs

Example health check script:

```bash
#!/bin/bash
# Check if walsync process is running
pgrep -x walsync > /dev/null || exit 1

# Check for recent S3 activity (last 5 minutes)
LAST_MODIFIED=$(aws s3 ls s3://my-backups/app.db/ \
  --endpoint-url $AWS_ENDPOINT_URL_S3 \
  --recursive | tail -1 | awk '{print $1" "$2}')

if [ -z "$LAST_MODIFIED" ]; then
  exit 1
fi

# Parse and check timestamp (implementation depends on your needs)
exit 0
```
