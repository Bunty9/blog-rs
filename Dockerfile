# syntax=docker/dockerfile:1

###########################  builder  ###########################
# Pinned to 1.86 to match rust-toolchain.toml + the time crate pin in
# Cargo.lock (newer rustc would re-bump transitive deps). Reproducible.
FROM rust:1.86-bookworm AS builder
WORKDIR /app
# Whole workspace: the binary embeds assets (rust-embed) and migrations
# (sqlx::migrate!) at compile time, so they must be present during the build.
COPY . .
RUN cargo build --release -p blog-rs

###########################  litestream  ########################
# Pull just the static litestream binary from its official image.
FROM litestream/litestream:0.3.13 AS litestream

###########################  runtime  ###########################
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --create-home --home-dir /home/app --shell /usr/sbin/nologin app \
 && mkdir -p /data && chown app:app /data

COPY --from=litestream /usr/local/bin/litestream /usr/local/bin/litestream
COPY --from=builder    /app/target/release/blog-rs /usr/local/bin/blog-rs
COPY docker/entrypoint.sh /entrypoint.sh
COPY docker/litestream.yml /etc/litestream.yml
RUN chmod +x /entrypoint.sh

USER app
ENV PORT=8080
EXPOSE 8080
# Liveness for `docker`/compose; compose overrides interval/retries.
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
  CMD curl -fsS "http://localhost:${PORT}/healthz" || exit 1
ENTRYPOINT ["/entrypoint.sh"]
