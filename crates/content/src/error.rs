use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ContentError {
    #[error("frontmatter parse failed: {0}")]
    Frontmatter(#[from] serde_yaml::Error),

    #[error("unterminated shortcode `{name}` opened at byte {offset}")]
    UnterminatedShortcode { name: String, offset: usize },

    #[error("unknown shortcode `{0}`")]
    UnknownShortcode(String),

    #[error("malformed shortcode args at byte {offset}: {reason}")]
    MalformedArgs { offset: usize, reason: String },

    #[error("shortcode render failed: {0}")]
    Render(String),
}
