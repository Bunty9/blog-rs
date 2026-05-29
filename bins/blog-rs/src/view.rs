//! Shared view-models and helpers for the reader templates.

use content::{Asset, AssetKind, AssetManifest};
use serde::Serialize;

pub const PAGE_SIZE: i64 = 10;

/// One row in the asset injection block.
#[derive(Debug, Clone, Serialize)]
pub struct AssetTag {
    pub kind: &'static str, // "css" or "js"
    pub src: String,
    pub defer: bool,
}

impl AssetTag {
    pub fn from_manifest(m: &AssetManifest) -> Vec<Self> {
        let mut out = Vec::with_capacity(m.assets.len());
        for a in &m.assets {
            out.push(Self::from(a));
        }
        out
    }
}

impl From<&Asset> for AssetTag {
    fn from(a: &Asset) -> Self {
        let kind = match a.kind {
            AssetKind::Css => "css",
            AssetKind::Js => "js",
        };
        Self {
            kind,
            src: a.src.clone(),
            defer: a.defer,
        }
    }
}

/// Cheap site-wide context handed to every template via the base layout.
#[derive(Debug, Clone)]
pub struct SiteCtx {
    pub title: String,
    pub base_url: String,
    pub description: String,
}

impl SiteCtx {
    /// Build a site-wide context. Reads `BLOG_BASE_URL`, `BLOG_TITLE`,
    /// `BLOG_DESCRIPTION` from the environment so a deployment can override
    /// without touching code. Defaults match the server's default bind.
    pub fn placeholder() -> Self {
        Self {
            title: std::env::var("BLOG_TITLE").unwrap_or_else(|_| "blog-rs".into()),
            base_url: std::env::var("BLOG_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:8080".into()),
            description: std::env::var("BLOG_DESCRIPTION")
                .unwrap_or_else(|_| "A personal Rust blog".into()),
        }
    }
}

/// Pagination view-model used by home/tag/search.
#[derive(Debug, Clone)]
pub struct Pagination {
    pub current: i64,
    pub total: i64,
    /// Path prefix used to construct page links (e.g. "/", "/tags/foo").
    pub path: String,
    /// Extra query suffix to preserve (e.g. "&q=rust"). Empty if none.
    pub query_suffix: String,
}

impl Pagination {
    pub fn new(current: i64, total_items: i64, path: impl Into<String>) -> Self {
        let total = ((total_items + PAGE_SIZE - 1) / PAGE_SIZE).max(1);
        Self {
            current: current.clamp(1, total),
            total,
            path: path.into(),
            query_suffix: String::new(),
        }
    }

    pub fn with_query_suffix(mut self, s: impl Into<String>) -> Self {
        self.query_suffix = s.into();
        self
    }

    pub fn has_prev(&self) -> bool {
        self.current > 1
    }
    pub fn has_next(&self) -> bool {
        self.current < self.total
    }
    pub fn prev_url(&self) -> String {
        self.url_for(self.current - 1)
    }
    pub fn next_url(&self) -> String {
        self.url_for(self.current + 1)
    }

    pub fn url_for(&self, page: i64) -> String {
        if self.query_suffix.is_empty() {
            if page == 1 {
                self.path.clone()
            } else {
                format!("{}?page={}", self.path, page)
            }
        } else {
            // Preserve a non-page query (search uses "?q=..."); we always emit page= here.
            format!("{}?page={}{}", self.path, page, self.query_suffix)
        }
    }
}

/// Bound a raw `?page=` value to `[1, total]`. Returns 1 for `0`, missing,
/// or out-of-range input.
pub fn clamp_page(raw: Option<i64>, total_items: i64) -> i64 {
    let total = ((total_items + PAGE_SIZE - 1) / PAGE_SIZE).max(1);
    raw.unwrap_or(1).max(1).min(total)
}

/// Render an epoch-seconds timestamp as RFC 2822 (for RSS).
pub fn rfc2822(secs: i64) -> String {
    use chrono::{DateTime, Utc};
    match DateTime::<Utc>::from_timestamp(secs, 0) {
        Some(dt) => dt.to_rfc2822(),
        None => String::from("Thu, 01 Jan 1970 00:00:00 +0000"),
    }
}

/// Render an epoch-seconds timestamp as `YYYY-MM-DD` (for sitemap + cards).
pub fn iso_date(secs: i64) -> String {
    use chrono::{DateTime, Utc};
    match DateTime::<Utc>::from_timestamp(secs, 0) {
        Some(dt) => dt.format("%Y-%m-%d").to_string(),
        None => "1970-01-01".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pagination_clamps_low() {
        let p = Pagination::new(0, 25, "/");
        assert_eq!(p.current, 1);
    }

    #[test]
    fn pagination_clamps_high() {
        // 25 items / 10 per page = 3 pages
        let p = Pagination::new(999, 25, "/");
        assert_eq!(p.current, 3);
        assert_eq!(p.total, 3);
        assert!(!p.has_next());
        assert!(p.has_prev());
    }

    #[test]
    fn pagination_empty_collection_has_one_page() {
        let p = Pagination::new(1, 0, "/");
        assert_eq!(p.total, 1);
        assert!(!p.has_next() && !p.has_prev());
    }

    #[test]
    fn pagination_url_for_page_1_drops_query() {
        let p = Pagination::new(2, 25, "/");
        assert_eq!(p.url_for(1), "/");
        assert_eq!(p.url_for(2), "/?page=2");
    }

    #[test]
    fn pagination_url_preserves_suffix() {
        let p = Pagination::new(1, 25, "/search").with_query_suffix("&q=rust");
        assert_eq!(p.url_for(2), "/search?page=2&q=rust");
    }

    #[test]
    fn clamp_page_handles_missing_and_zero() {
        assert_eq!(clamp_page(None, 5), 1);
        assert_eq!(clamp_page(Some(0), 5), 1);
        assert_eq!(clamp_page(Some(99), 5), 1); // 5 items = 1 page
    }

    #[test]
    fn rfc2822_known_epoch() {
        // 2023-11-14T22:13:20Z
        assert_eq!(rfc2822(1_700_000_000), "Tue, 14 Nov 2023 22:13:20 +0000");
    }

    #[test]
    fn iso_date_known_epoch() {
        assert_eq!(iso_date(1_700_000_000), "2023-11-14");
    }
}
