use crate::{Asset, AssetKind, RenderError, RenderedBlock, Shortcode, ShortcodeArgs};

pub struct Callout;

const CSS: &str = "/assets/blocks/callout.css";

impl Shortcode for Callout {
    fn name(&self) -> &'static str {
        "callout"
    }
    fn paired(&self) -> bool {
        true
    }

    fn render(
        &self,
        args: &ShortcodeArgs,
        body: Option<&str>,
    ) -> Result<RenderedBlock, RenderError> {
        let body = body.ok_or(RenderError::MissingBody("callout"))?;
        let kind = args.get("type").unwrap_or("info");
        if !matches!(kind, "info" | "warn" | "tip" | "danger") {
            return Err(RenderError::InvalidBody {
                name: "callout",
                reason: format!("unknown type `{kind}`"),
            });
        }
        let html = format!(
            r#"<aside class="callout callout-{kind}" data-callout="{kind}">{body}</aside>"#
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_type_is_info() {
        let args = ShortcodeArgs::new();
        let out = Callout.render(&args, Some("<p>hi</p>")).unwrap();
        assert!(out.html.contains(r#"data-callout="info""#));
        assert!(out.html.contains("<p>hi</p>"));
    }

    #[test]
    fn rejects_bad_type() {
        let mut args = ShortcodeArgs::new();
        args.insert("type", "nope");
        let err = Callout.render(&args, Some("x")).unwrap_err();
        assert!(matches!(err, RenderError::InvalidBody { .. }));
    }

    #[test]
    fn requires_body() {
        let args = ShortcodeArgs::new();
        let err = Callout.render(&args, None).unwrap_err();
        assert!(matches!(err, RenderError::MissingBody("callout")));
    }
}
