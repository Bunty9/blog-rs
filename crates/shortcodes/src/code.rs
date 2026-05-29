use crate::{Asset, AssetKind, RenderError, RenderedBlock, Shortcode, ShortcodeArgs};

pub struct Code;

const CM_JS: &str = "/assets/blocks/code/codemirror.bundle.js";
const CM_CSS: &str = "/assets/blocks/code/codemirror.css";

impl Shortcode for Code {
    fn name(&self) -> &'static str {
        "code"
    }
    fn paired(&self) -> bool {
        true
    }
    fn body_is_markdown(&self) -> bool {
        false
    }

    fn render(
        &self,
        args: &ShortcodeArgs,
        body: Option<&str>,
    ) -> Result<RenderedBlock, RenderError> {
        let source = body.ok_or(RenderError::MissingBody("code"))?;
        let lang = args.get("lang").unwrap_or("text");
        let playground = args.bool("playground") && lang == "rust";
        let escaped = html_escape(source);
        let play_attr = if playground {
            r#" data-playground="rust""#
        } else {
            ""
        };

        let html = format!(
            r#"<figure class="code-block" data-lang="{lang}"{play_attr}>
<pre><code class="language-{lang}">{escaped}</code></pre>
</figure>"#
        );

        Ok(RenderedBlock {
            html,
            assets: vec![
                Asset {
                    kind: AssetKind::Css,
                    src: CM_CSS.into(),
                    defer: false,
                },
                Asset {
                    kind: AssetKind::Js,
                    src: CM_JS.into(),
                    defer: true,
                },
            ],
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
    fn escapes_html_in_source() {
        let mut a = ShortcodeArgs::new();
        a.insert("lang", "rust");
        let out = Code.render(&a, Some("let x = <T>::new();")).unwrap();
        assert!(out.html.contains("&lt;T&gt;"));
    }

    #[test]
    fn playground_only_for_rust() {
        let mut a = ShortcodeArgs::new();
        a.insert("lang", "python");
        a.insert("playground", "true");
        let out = Code.render(&a, Some("x = 1")).unwrap();
        assert!(!out.html.contains("data-playground"));
    }

    #[test]
    fn playground_for_rust() {
        let mut a = ShortcodeArgs::new();
        a.insert("lang", "rust");
        a.insert("playground", "true");
        let out = Code.render(&a, Some("fn main() {}")).unwrap();
        assert!(out.html.contains(r#"data-playground="rust""#));
    }
}
