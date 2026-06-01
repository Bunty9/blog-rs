#!/usr/bin/env bash
# blog-rs deploy helper for the Nitro5 (Docker + Cloudflare Tunnel).
# Usage:
#   ./deploy.sh up          Build + start app + tunnel (detached)
#   ./deploy.sh update      Rebuild image + restart (detached)
#   ./deploy.sh stop        Stop app + tunnel
#   ./deploy.sh logs        Tail logs (app + tunnel)
#   ./deploy.sh status      Container health + resource usage
#   ./deploy.sh restore     Force a Litestream restore of /data/blog.db from R2
#   ./deploy.sh backup-now  Snapshot the DB to R2 immediately
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

BASE="-f docker-compose.yml"
TUNNEL="-f docker-compose.cloudflare.yml"
SVC="blog-rs"

require_env() {
  if [[ ! -f .env ]]; then
    echo "ERROR: .env not found. Copy .env.example -> .env and fill it in (chmod 600 .env)." >&2
    exit 1
  fi
}

case "${1:-}" in
  up)
    require_env
    docker compose $BASE $TUNNEL up -d --build
    ;;
  update)
    require_env
    docker compose $BASE $TUNNEL build
    docker compose $BASE $TUNNEL up -d
    ;;
  stop)
    docker compose $BASE $TUNNEL down
    ;;
  logs)
    docker compose $BASE $TUNNEL logs -f --tail=100
    ;;
  status)
    docker compose $BASE $TUNNEL ps
    docker stats --no-stream || true
    ;;
  restore)
    # Restore inside a one-off container that mounts the data volume.
    docker compose $BASE run --rm --entrypoint \
      "litestream restore -o /data/blog.db /data/blog.db" "$SVC"
    ;;
  backup-now)
    docker compose $BASE exec "$SVC" litestream snapshot /data/blog.db || \
      echo "snapshot requires litestream running with a configured replica"
    ;;
  *)
    grep -E '^#( |$)' "$0" | sed -E 's/^# ?//'
    exit 1
    ;;
esac
