//! Markdown + shortcode rendering pipeline.

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
