use crate::{Asset, AssetKind, RenderError, RenderedBlock, Shortcode, ShortcodeArgs};

pub struct Image;

const CSS: &str = "/assets/blocks/image.css";

impl Shortcode for Image {
    fn name(&self) -> &'static str {
        "image"
    }

    fn render(
        &self,
        args: &ShortcodeArgs,
        _body: Option<&str>,
    ) -> Result<RenderedBlock, RenderError> {
        let src = args.required("src")?;
        let alt = args.get("alt").unwrap_or("");
        let caption = args.get("caption");
        let width = args.get("width").unwrap_or("100%");
        let aspect = args.get("aspect");

        let aspect_style = aspect
            .map(|a| format!(" style=\"aspect-ratio:{a};\""))
            .unwrap_or_default();

        let caption_html = caption
            .map(|c| format!("<figcaption>{}</figcaption>", html_escape(c)))
            .unwrap_or_default();

        let html = format!(
            r#"<figure class="img-block" style="--w:{width};"{aspect_style}>
<img src="{src}" alt="{alt}" loading="lazy" />
{caption_html}
</figure>"#,
            src = html_escape(src),
            alt = html_escape(alt),
        );

        Ok(RenderedBlock {
            html,
            assets: vec![Asset {
                kind: AssetKind::Css,
                src: CSS.into(),
                defer: false,
            }],
        })
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_src() {
        let a = ShortcodeArgs::new();
        let err = Image.render(&a, None).unwrap_err();
        assert!(matches!(err, RenderError::Args(_)));
    }

    #[test]
    fn renders_with_caption_and_aspect() {
        let mut a = ShortcodeArgs::new();
        a.insert("src", "/m/x.png");
        a.insert("alt", "x");
        a.insert("caption", "X & Y");
        a.insert("aspect", "16/9");
        let out = Image.render(&a, None).unwrap();
        assert!(out.html.contains(r#"src="/m/x.png""#));
        assert!(out.html.contains("X &amp; Y"));
        assert!(out.html.contains("aspect-ratio:16/9"));
    }
}
