//! Askama template structs that are not tied to a single route module.
//!
//! Per-route templates (admin/reader/xml partials) keep their derive next to
//! the handler. Cross-cutting templates — currently the two email templates
//! used by the signup flow and the outbox worker — live here so the route
//! module and the worker can both render them.

use askama::Template;

#[derive(Template)]
#[template(path = "email/confirm.html")]
pub struct ConfirmEmail<'a> {
    pub site_title: &'a str,
    pub confirm_url: String,
    pub ttl_hours: u32,
}

#[derive(Template)]
#[template(path = "email/post.html")]
pub struct PostEmail<'a> {
    pub site_title: &'a str,
    pub post_title: &'a str,
    pub subtitle: Option<&'a str>,
    pub excerpt: Option<&'a str>,
    pub post_url: String,
    pub unsubscribe_url: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use askama::Template;

    #[test]
    fn confirm_email_renders() {
        let t = ConfirmEmail {
            site_title: "blog-rs",
            confirm_url: "https://example.com/confirm/abc".into(),
            ttl_hours: 48,
        };
        let out = t.render().unwrap();
        assert!(out.contains("Confirm subscription"));
        assert!(out.contains("https://example.com/confirm/abc"));
        assert!(out.contains("48 hours"));
    }

    #[test]
    fn post_email_renders_with_subtitle_and_excerpt() {
        let t = PostEmail {
            site_title: "blog-rs",
            post_title: "Hello",
            subtitle: Some("a subtitle"),
            excerpt: Some("an excerpt"),
            post_url: "https://example.com/p/hello".into(),
            unsubscribe_url: "https://example.com/u/xyz".into(),
        };
        let out = t.render().unwrap();
        assert!(out.contains("Hello"));
        assert!(out.contains("a subtitle"));
        assert!(out.contains("an excerpt"));
        assert!(out.contains("https://example.com/u/xyz"));
    }

    #[test]
    fn post_email_omits_optional_blocks() {
        let t = PostEmail {
            site_title: "blog-rs",
            post_title: "Hello",
            subtitle: None,
            excerpt: None,
            post_url: "https://example.com/p/hello".into(),
            unsubscribe_url: "https://example.com/u/xyz".into(),
        };
        let out = t.render().unwrap();
        assert!(out.contains("Hello"));
        assert!(out.contains("Read the full post"));
    }
}
