use crate::ContentError;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Frontmatter {
    pub title: String,
    pub subtitle: Option<String>,
    pub tags: Vec<String>,
    pub series: Option<String>,
    pub series_order: Option<u32>,
    pub cover_image: Option<String>,
    pub status: PostStatus,
    pub canonical: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PostStatus {
    Draft,
    Published,
    Scheduled,
}

impl Default for PostStatus {
    fn default() -> Self {
        Self::Draft
    }
}

static FM_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?s)\A---\r?\n(.*?)\r?\n---\r?\n?").unwrap());

/// Splits source into (frontmatter, body). If no frontmatter present, returns
/// default Frontmatter and the full input as body.
pub fn split(src: &str) -> Result<(Frontmatter, &str), ContentError> {
    if let Some(m) = FM_RE.captures(src) {
        let yaml = m.get(1).unwrap().as_str();
        let fm: Frontmatter = serde_yaml::from_str(yaml)?;
        let rest = &src[m.get(0).unwrap().end()..];
        Ok((fm, rest))
    } else {
        Ok((Frontmatter::default(), src))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = include_str!("../../../tests/fixtures/frontmatter-only.md");

    #[test]
    fn parses_fixture() {
        let (fm, body) = split(SRC).unwrap();
        assert_eq!(fm.title, "Sample Post");
        assert_eq!(fm.tags, vec!["rust", "embedded"]);
        assert_eq!(fm.series.as_deref(), Some("rust-level-4"));
        assert_eq!(fm.series_order, Some(1));
        assert_eq!(fm.status, PostStatus::Draft);
        assert!(body.trim_start().starts_with("Body text here."));
    }

    #[test]
    fn handles_no_frontmatter() {
        let (fm, body) = split("just body").unwrap();
        assert_eq!(fm, Frontmatter::default());
        assert_eq!(body, "just body");
    }

    #[test]
    fn rejects_invalid_yaml() {
        let bad = "---\n: : :\n---\nbody";
        assert!(matches!(split(bad), Err(ContentError::Frontmatter(_))));
    }
}
