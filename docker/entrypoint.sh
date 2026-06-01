#!/usr/bin/env bash
# blog-rs container entrypoint.
# - Binds the app to 0.0.0.0:$PORT (Cloudflare/Docker convention).
# - Points the DB at the persistent /data volume.
# - If R2/Litestream is configured: restore-on-boot (only when the local DB is
#   missing) then run the app under Litestream so the WAL is replicated live.
# - Otherwise: run the app directly (handy for local testing without backups).
set -euo pipefail

export PORT="${PORT:-8080}"
export BLOG_RS__BIND="0.0.0.0:${PORT}"
export BLOG_RS__DATABASE_URL="${BLOG_RS__DATABASE_URL:-sqlite:///data/blog.db?mode=rwc}"

mkdir -p /data/media

if [[ -n "${LITESTREAM_BUCKET:-}" && -n "${LITESTREAM_ACCESS_KEY_ID:-}" ]]; then
  echo "[entrypoint] litestream configured -> restore-if-needed, then replicate"
  litestream restore -if-db-not-exists -if-replica-exists /data/blog.db || true
  exec litestream replicate -exec "blog-rs"
else
  echo "[entrypoint] no litestream config -> starting blog-rs without backups"
  exec blog-rs
fi
