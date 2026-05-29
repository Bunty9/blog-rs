//! Markdown + shortcode rendering pipeline.

/// Version stamp for cached `body_html` rows. Bump this when any change to
/// the shortcode registry, markdown options, or escape rules would produce
/// different output for the same input. Persisted callers (see `db::posts`)
/// write this value alongside `body_html`, so a future regen pass can find
/// stale rows via `body_html_version <> content::RENDER_VERSION`.
pub const RENDER_VERSION: u32 = 1;

pub mod asset;
pub mod error;
pub mod frontmatter;
pub mod markdown;
pub mod render;
pub mod shortcode_lexer;

pub use asset::{Asset, AssetKind, AssetManifest};
pub use error::ContentError;
pub use frontmatter::Frontmatter;
pub use render::{render, RenderOutput};
