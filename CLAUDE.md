# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A self-hosted blog engine: a single Axum binary backed by SQLite. Posts are markdown with YAML frontmatter plus Hugo-style shortcodes (`{{< name args >}}` … `{{< /name >}}`) that render to HTML and emit a per-page asset manifest, so each block's CSS/JS loads only on pages that use it. Ships an htmx admin dashboard, a free-member newsletter with a background outbox worker, FTS5 search, RSS/sitemap, and a CLI renderer.

## Commands

Recipes live in the `justfile`. The clippy bar is `-D warnings`; the workspace is kept clean.

```
just                 # default: cargo test --workspace
just build           # cargo build --workspace
just test            # cargo test --workspace
just lint            # cargo clippy --workspace --all-targets -- -D warnings
just fmt             # cargo fmt --all
just coverage        # cargo llvm-cov --workspace --fail-under-lines 70
just snap-review     # cargo insta review (snapshot diffs)
```

- Single test: `cargo test -p <crate> <test_name>` (e.g. `cargo test -p db members`).
- Snapshot tests use `insta`. After an intentional output change: `INSTA_UPDATE=always cargo test -p content --test golden`, then inspect the `.snap` diff before committing.
- E2E (Playwright) lives in `tests/e2e/`; needs `package-lock.json` (`cd tests/e2e && npm install`) and Node 20+. Runs against a release build in CI.
- CI (`.github/workflows/ci.yml`) runs fmt-check → clippy → workspace tests → Playwright e2e on every push/PR.
- Run the CLI renderer: `cargo run -p blog-rs-render -- <file.md> --assets-out a.json --frontmatter-out fm.yaml > out.html`.
- Boot the server: `cargo run -p blog-rs` (see env vars below; `BLOG_RS__SIGNING_KEY` is mandatory or it refuses to start).

## Workspace architecture

The dependency direction is strictly **content/shortcodes → db → auth → bins**. Lower crates have no HTTP or rendering knowledge.

- `crates/shortcodes` — the `Shortcode` trait, an args parser, and the seven block types (callout, code, image, chart, animate, playable, embed). `default_registry()` wires them up.
- `crates/content` — frontmatter split, CommonMark via `pulldown-cmark`, the shortcode lexer, and the render pipeline. `render()` walks lexer tokens, calls into the registry, and accumulates an `AssetManifest`. This crate is pure: markdown+shortcodes in, `RenderOutput { frontmatter, html, assets }` out.
- `crates/db` — owns the SQLx SQLite pool and migrations; one module per table with typed queries. No HTTP, no rendering. `db::initialize()` connects and runs migrations; tests use `test_support::fresh_pool()` (in-memory). FTS5 search is driven by triggers (migration `0003`).
- `crates/auth` — argon2id password hashing, sessions, double-submit CSRF, and HMAC-signed member tokens with TTL.
- `bins/blog-rs` — the server. `routes/` splits into `health`, `reader` (public), `members`, and `admin` (nested under `/admin`, htmx). Tower middleware stack (catch-panic, trace, compression, correlation-id) wraps everything. `worker/outbox.rs` is a background dispatcher spawned in `main`, cancelled via a `CancellationToken` on graceful shutdown so it stops between ticks rather than mid-send.
- `bins/blog-rs-render` — thin CLI wrapper over `content::render`.
- `tools/import-research` — converts a markdown research dump into per-domain seed articles under `content/articles/`.

## Key conventions and gotchas

- **Adding a shortcode**: implement `Shortcode` in `crates/shortcodes/src/`, register it in `default_registry()`. The render pipeline and per-page asset injection pick it up automatically — no template edits needed.
- **Render caching**: `content::RENDER_VERSION` (in `content/src/lib.rs`) stamps cached `body_html` rows. Bump it whenever a registry/markdown/escape change would alter output for the same input; stale rows are found via `body_html_version <> RENDER_VERSION`.
- **Two env var families.** The `Config` struct (bind, database_url, signing_key, session/token TTLs, pool size, admin_bootstrap) loads via `figment` with the **`BLOG_RS__`** prefix and `__` nesting (e.g. `BLOG_RS__ADMIN_BOOTSTRAP__EMAIL`). Site/mail/worker settings are read separately from their own vars: `BLOG_BASE_URL`, `BLOG_TITLE`, `BLOG_DESCRIPTION`, `BLOG_RS_MAIL` (`test` writes to `./test-mailbox.eml`), `BLOG_SMTP_*`, `OUTBOX_POLL_INTERVAL`, `OUTBOX_RECLAIM_AFTER`. Don't conflate the prefixes.
- **First boot**: seeds the admin row from `BLOG_RS__ADMIN_BOOTSTRAP__*`, then ignores those vars once `users` is non-empty (password lives only as an argon2id hash).
- **Migrations** are append-only SQL files in `migrations/` (`0001`…`0009`), run automatically on startup and in `fresh_pool()`.
- **Mailer** is pluggable (`mailer/`): `smtp` for production, `test_file` writes `.eml` to disk for tests/local.
- Config tests use `figment::Jail` to isolate `BLOG_RS__*` env state across parallel runs — follow that pattern when adding config tests.

## Known follow-ups (from README)

- `db::members::enqueue_confirm` writes confirm-purpose outbox rows with `post_id = 0`; migration `0007` made `post_id` nullable but production may still want a clean schema/sentinel decision.
- Seed articles under `content/articles/` carry `<!-- TODO: chart? -->` markers awaiting author review before public publish.
