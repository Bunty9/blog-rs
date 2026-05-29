use crate::{html_attr_escape, RenderError, RenderedBlock, Shortcode, ShortcodeArgs};

pub struct Embed;

impl Shortcode for Embed {
    fn name(&self) -> &'static str {
        "embed"
    }

    fn render(
        &self,
        args: &ShortcodeArgs,
        _body: Option<&str>,
    ) -> Result<RenderedBlock, RenderError> {
        let url = args.required("url")?;
        let (provider, inner) = classify(url);

        if let Some(p) = provider {
            Ok(RenderedBlock {
                html: format!(
                    r#"<figure class="embed-block" data-provider="{p}">{inner}</figure>"#
                ),
                assets: vec![],
            })
        } else {
            let esc = html_attr_escape(url);
            Ok(RenderedBlock {
                html: format!(r#"<p><a href="{esc}" rel="noopener">{esc}</a></p>"#),
                assets: vec![],
            })
        }
    }
}

fn classify(url: &str) -> (Option<&'static str>, String) {
    if let Some(id) = youtube_id(url) {
        let id_esc = html_attr_escape(id);
        let html = format!(
            r#"<iframe loading="lazy" src="https://www.youtube-nocookie.com/embed/{id_esc}" title="YouTube video" allow="accelerometer; encrypted-media; picture-in-picture" allowfullscreen></iframe>"#
        );
        return (Some("youtube"), html);
    }
    if url.contains("twitter.com/") || url.contains("x.com/") {
        let esc = html_attr_escape(url);
        let html = format!(
            r#"<blockquote class="twitter-tweet"><a href="{esc}">{esc}</a></blockquote>"#
        );
        return (Some("twitter"), html);
    }
    (None, String::new())
}

fn youtube_id(url: &str) -> Option<&str> {
    if let Some(rest) = url.strip_prefix("https://www.youtube.com/watch?v=") {
        return Some(rest.split('&').next().unwrap_or(rest));
    }
    if let Some(rest) = url.strip_prefix("https://youtu.be/") {
        return Some(rest.split('?').next().unwrap_or(rest));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn youtube_full_url() {
        let mut a = ShortcodeArgs::new();
        a.insert("url", "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        let out = Embed.render(&a, None).unwrap();
        assert!(out.html.contains(r#"data-provider="youtube""#));
        assert!(out.html.contains("dQw4w9WgXcQ"));
    }

    #[test]
    fn youtube_short_url() {
        let mut a = ShortcodeArgs::new();
        a.insert("url", "https://youtu.be/abc123");
        let out = Embed.render(&a, None).unwrap();
        assert!(out.html.contains("abc123"));
    }

    #[test]
    fn unknown_url_falls_back_to_link() {
        let mut a = ShortcodeArgs::new();
        a.insert("url", "https://example.com/x");
        let out = Embed.render(&a, None).unwrap();
        assert!(out.html.contains(r#"<a href="https://example.com/x""#));
    }
}
