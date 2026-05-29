#!/usr/bin/env bash
# Fetch pinned vendor JS/CSS bundles into assets/.
#
# Run manually after a fresh clone or when bumping a pinned version. The
# resulting files are committed to the repo so `cargo build` stays hermetic
# (no network in build.rs, no rollup, no npm).
#
# Usage:   ./scripts/fetch-vendor.sh
# Re-run:  delete or version-bump entries below, then run again.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# url|dest|min_bytes
ITEMS=(
  "https://cdn.jsdelivr.net/npm/chart.js@4.4.1/dist/chart.umd.js|assets/vendor/chart.umd.min.js|50000"
  "https://cdn.jsdelivr.net/npm/motion@10.18.0/dist/motion.min.js|assets/vendor/motion.min.js|10000"
  "https://cdn.jsdelivr.net/npm/htmx.org@1.9.10/dist/htmx.min.js|assets/vendor/htmx.min.js|20000"
  "https://cdn.jsdelivr.net/npm/prismjs@1.29.0/prism.min.js|assets/blocks/code/codemirror.bundle.js|5000"
  "https://cdn.jsdelivr.net/npm/prismjs@1.29.0/themes/prism.min.css|assets/blocks/code/codemirror.css|1000"
)

fail() { echo "fetch-vendor: $*" >&2; exit 1; }

for item in "${ITEMS[@]}"; do
  IFS='|' read -r url dest min_bytes <<<"$item"
  mkdir -p "$(dirname "$dest")"
  tmp="${dest}.tmp"
  echo "fetching $url"
  http_code=$(curl --fail --silent --show-error --location \
    --max-time 60 --retry 2 --retry-delay 2 \
    -o "$tmp" -w '%{http_code}' "$url") || fail "curl failed for $url"
  if [[ "$http_code" != "200" ]]; then
    rm -f "$tmp"
    fail "non-200 ($http_code) for $url"
  fi
  size=$(wc -c <"$tmp" | tr -d ' ')
  if (( size < min_bytes )); then
    rm -f "$tmp"
    fail "$dest only $size bytes (< $min_bytes); refusing to commit a stub"
  fi
  mv "$tmp" "$dest"
  printf '  -> %s (%s bytes)\n' "$dest" "$size"
done

echo "done"
