# blog-rs

A self-hosted blog engine written in Rust. Each post is a composable document of typed content blocks: markdown plus a small registry of interactive shortcodes (code playgrounds, charts, animations, callouts, images, embeds). The engine is designed to run as a single binary on a small VPS, with SQLite for storage and per-block JavaScript loaded only on the pages that need it.

The repository is the build target of a phased plan. Current code ships the rendering pipeline, the data layer, the cryptographic primitives, and a markdown-to-HTML CLI. The HTTP server, admin dashboard, public reader, and newsletter are scoped and planned but not yet implemented.

## Status

| Subsystem                       | Status        | Notes                                                              |
| ------------------------------- | ------------- | ------------------------------------------------------------------ |
| Content pipeline                | Implemented   | Frontmatter, CommonMark, shortcode lexer, render pipeline          |
| Shortcode registry              | Implemented   | callout, code, image, chart, animate, playable, embed              |
| CLI renderer (`blog-rs-render`) | Implemented   | Markdown file in, HTML plus asset manifest out                     |
| SQLite data layer               | Implemented   | Users, posts, tags, sessions, members, outbox, FTS5 search         |
| Auth primitives                 | Partial       | argon2id passwords, CSRF validator, HMAC tokens. Session work next |
| HTTP server                     | Not yet built | Axum binary scaffolded; routes pending                             |
| Admin dashboard                 | Not yet built | Plan written                                                       |
| Public reader                   | Not yet built | Plan written                                                       |
| Members and newsletter          | Not yet built | Plan written                                                       |

## Workspace layout

```
blog-rs/
  Cargo.toml                       workspace manifest
  rust-toolchain.toml              pins stable channel
  justfile                         build, test, lint recipes
  migrations/                      SQLx migration files
  crates/
    content/                       markdown plus shortcode parser and render pipeline
    shortcodes/                    Shortcode trait, args parser, seven block types
    db/                            SQLx pool, queries, FTS5 triggers
    auth/                          argon2id, CSRF, HMAC tokens
  bins/
    blog-rs-render/                CLI that renders one markdown file to HTML
    blog-rs/                       server binary (scaffold only at this point)
  tests/
    fixtures/                      input markdown used by golden tests
```

## Requirements

- Rust 1.78 or newer (the toolchain file pins stable)
- SQLite (linked statically through the `sqlx` features; no system install required for build)

## Quick start

Clone the repository and run the test suite:

```
git clone https://github.com/Bunty9/blog-rs
cd blog-rs
cargo test --workspace
```

Render the included Rust Level 4 research fixture to HTML:

```
cargo run -p blog-rs-render -- \
    tests/fixtures/domain-1-snippet.md \
    --assets-out /tmp/assets.json \
    --frontmatter-out /tmp/fm.yaml \
    > /tmp/out.html
```

The HTML output contains the rendered post body. The JSON manifest lists the CSS and JavaScript assets that the page needs, deduplicated, in the order they were first emitted by the shortcodes.

## Authoring model

Posts are markdown files with a YAML frontmatter block. Interactive content uses shortcodes with the Hugo-style syntax `{{< name args >}}` for self-closing blocks and a paired `{{< /name >}}` for blocks with a body.

Example:

```
---
title: Bare-metal Rust on Cortex-M4
tags: [rust, embedded]
status: draft
---

{{< callout type="info" >}}
Bare-metal Rust drops the standard library entirely.
{{< /callout >}}

{{< code lang="rust" playground="true" >}}
#![no_std]
#![no_main]
{{< /code >}}

{{< chart type="bar" src="data/cycles.json" caption="Preempt cycles" >}}
```

Built-in shortcode names and the assets they pull in:

| Name       | Body     | Notable args                          | Assets                |
| ---------- | -------- | ------------------------------------- | --------------------- |
| `callout`  | required | `type` (info, warn, tip, danger)      | one CSS file          |
| `code`     | required | `lang`, `playground` (bool)           | CodeMirror bundle     |
| `image`    | none     | `src`, `alt`, `caption`, `aspect`     | one CSS file          |
| `chart`    | none     | `type`, `src` or `data`, `caption`    | Chart.js plus glue    |
| `animate`  | required | `preset`, `keyframes`                 | Motion One plus glue  |
| `playable` | none     | `id` (currently `rust-playground`)    | none                  |
| `embed`    | none     | `url` (YouTube, Twitter, fallback)    | none                  |

Adding a new block type means writing a struct that implements the `Shortcode` trait in `crates/shortcodes/src/` and registering it in `default_registry()`. The render pipeline picks it up automatically; the page templates pick up its asset manifest entries automatically.

## Testing

The workspace has 74 tests across four crates:

```
cargo test --workspace
```

Breakdown:

- `content`: 17 unit tests covering frontmatter parsing, markdown rendering, the shortcode lexer, the render pipeline, and the asset manifest, plus 2 integration tests that snapshot the rendered HTML for the seven-block fixture and a research-derived fixture.
- `shortcodes`: 22 unit tests covering the args parser and each shortcode implementation.
- `db`: 19 integration tests against an in-memory SQLite database, covering every table module (users, sessions, posts, tags, members, outbox, search) and the migrations runner.
- `auth`: 14 unit tests covering argon2id round-trips, CSRF double-submit validation, and HMAC token signing.

Snapshot tests use `insta`. To accept regenerated snapshots after an intentional output change, run `INSTA_UPDATE=always cargo test -p content --test golden` and inspect the diff in the `.snap` files before committing.

## Build hygiene

The justfile shortcuts the common tasks:

```
just            # test
just build      # cargo build --workspace
just test       # cargo test --workspace
just fmt        # cargo fmt --all
just lint       # cargo clippy --workspace --all-targets -- -D warnings
```

The clippy bar is set to `-D warnings`. The current workspace is clean.

## License

MIT. See `LICENSE`.
