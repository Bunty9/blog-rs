# Deploying blog-rs (Nitro5 + Docker + Cloudflare Tunnel)

$0/mo, always-on, self-hosted at `https://blog.bunty9.com`. No inbound ports
(outbound-only Cloudflare Tunnel); SQLite continuously backed up to Cloudflare
R2 via Litestream. Mirrors the `energy-money-life` tunnel pattern.

## Architecture

```
browser ── TLS ──> Cloudflare edge ──(outbound tunnel)──> Nitro5
                                                            └─ cloudflared (sidecar)
                                                                 └─ localhost:8080 ─> blog-rs (Axum)
                                                                                        └─ /data volume: blog.db (+WAL), media/
                                                            blog.db ──(Litestream WAL)──> Cloudflare R2
```

- `blog-rs` binds `0.0.0.0:8080` **inside the container only**; nothing is published to the host.
- `cloudflared` shares the app netns (`network_mode: "service:blog-rs"`) and forwards `blog.bunty9.com` → `http://localhost:8080`.
- Migrations run on boot (embedded). The image is self-contained (assets + migrations compiled in).

## Files

| File | Purpose |
|------|---------|
| `Dockerfile` | multi-stage build (rust:1.86 → debian-slim + litestream + curl) |
| `docker/entrypoint.sh` | sets bind/DB env, restore-on-boot, runs app under Litestream |
| `docker/litestream.yml` | DB → R2 replica config |
| `docker-compose.yml` | app service + `blog_data` volume + healthcheck |
| `docker-compose.cloudflare.yml` | cloudflared tunnel overlay |
| `.env.example` | all required env/secrets (copy → `.env`) |
| `deploy.sh` | up / update / stop / logs / status / restore / backup-now |

## One-time setup

### 1. Cloudflare Tunnel
1. Zero-Trust dashboard → Networks → Tunnels → **Create a tunnel** (Cloudflared), name `blog-rs`.
2. Copy the **connector token** → `TUNNEL_TOKEN` in `.env`.
3. Add a **Public Hostname**: `blog.bunty9.com` → Service `HTTP` `localhost:8080`. (The CNAME is created automatically; `bunty9.com` must be on Cloudflare DNS.)

### 2. Cloudflare R2 (backups)
1. R2 → **Create bucket** `blog-rs-backups`.
2. R2 → Manage API Tokens → create token (Object Read & Write) → note **Access Key ID** + **Secret**.
3. Endpoint is `https://<account-id>.r2.cloudflarestorage.com`.
4. Fill `LITESTREAM_ENDPOINT`, `LITESTREAM_BUCKET`, `LITESTREAM_ACCESS_KEY_ID`, `LITESTREAM_SECRET_ACCESS_KEY` in `.env`.

### 3. Secrets
```bash
cp .env.example .env
# signing key:
echo "BLOG_RS__SIGNING_KEY=$(head -c 32 /dev/urandom | base64 | tr '+/' '-_' | tr -d '=')"
# edit .env: paste the key, set admin email/password, TUNNEL_TOKEN, R2 creds, mail
chmod 600 .env
```

## Deploy (on the Nitro5)
```bash
git pull
./deploy.sh up          # first build is slow on the i5 (~minutes), then starts
docker compose ps       # both containers healthy
curl -fsS https://blog.bunty9.com/healthz
```
Log in at `https://blog.bunty9.com/admin/login` with the bootstrap admin.

## Update
```bash
git pull && ./deploy.sh update
```

## Backup & restore
- Replication is automatic (Litestream `-exec`).
- **Restore drill** (verify backups work):
  ```bash
  ./deploy.sh stop
  docker volume rm blog-rs_blog_data    # confirm the exact volume name via `docker volume ls`
  ./deploy.sh up                        # entrypoint restores blog.db from R2 on boot
  ```
  Confirm posts/members are present after restore.

## Optional hardening
- **Cloudflare Access** policy on `blog.bunty9.com/admin*` (Zero-Trust → Access → Applications): require OTP/Google/GitHub to reach the admin dashboard at the edge, before requests ever hit blog-rs.

## Notes / deferred
- **Media-dir backup** is not configured yet — Litestream replicates only the SQLite DB. Media uploads aren't built yet; when they ship, add an `rclone`/`restic` cron sync of `/data/media` → R2.
- **Building on the Nitro5** is intentional (no registry needed). A GHCR build+push job can be added later if build time becomes annoying.
- Graceful stop: the app handles SIGTERM, so `docker stop` / `./deploy.sh stop` lets the outbox worker finish its tick before exit.
