# GHOST Platform Deployment Guide

## Deployment Profiles

### 1. Development (Local)

```bash
# Build and run directly
cargo build --release
./target/release/ghost serve --config ghost.yml
```

### 2. Homelab (Docker Compose)

```bash
cd deploy
cp ../schemas/ghost-config.example.yml ghost.yml
# Edit ghost.yml with your settings

export GHOST_TOKEN=$(openssl rand -hex 32)
docker compose up -d
```

Services:
- Gateway: http://localhost:18789
- Monitor: http://localhost:18790
- Dashboard: http://localhost:5173

### 3. Production (Docker Compose)

```bash
cd deploy
cp ../schemas/ghost-config.example.yml ghost.yml
# Edit ghost.yml for production

export GHOST_TOKEN=$(openssl rand -hex 32)
export GHOST_BACKUP_KEY=$(openssl rand -hex 32)
export GHOST_CONTACT_EMAIL=ops@example.com

docker compose -f docker-compose.prod.yml up -d
```

### systemd (Bare Metal)

```bash
sudo cp target/release/ghost /usr/local/bin/
sudo useradd --system --create-home ghost
sudo mkdir -p /etc/ghost
sudo cp ghost.yml /etc/ghost/
sudo cp deploy/ghost.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now ghost
```

## Health Checks

```bash
# Gateway health
curl http://localhost:18789/api/health

# Monitor health
curl http://localhost:18790/health

# CLI status
ghost status
```

## Backup

```bash
export GHOST_BACKUP_KEY=your-secret-key
ghost backup --output-path ./my-backup.ghost-backup
```

## Restore

```bash
ghost restore --input ./my-backup.ghost-backup
# or restore into an explicit fresh target
ghost restore --input ./my-backup.ghost-backup --target ./restored-ghost
```
