use crate::{Asset, AssetKind, RenderedBlock, RenderError, Shortcode, ShortcodeArgs};

pub struct Animate;

const MOTION_JS: &str = "/assets/vendor/motion.min.js";
const BLOCK_JS: &str = "/assets/blocks/animate.js";

impl Shortcode for Animate {
    fn name(&self) -> &'static str { "animate" }
    fn paired(&self) -> bool { true }

    fn render(&self, args: &ShortcodeArgs, body: Option<&str>) -> Result<RenderedBlock, RenderError> {
        let inner = body.ok_or(RenderError::MissingBody("animate"))?;
        let preset = args.get("preset").unwrap_or("fade");
        if !matches!(preset, "fade" | "slide-up" | "slide-left" | "scale" | "custom") {
            return Err(RenderError::InvalidBody {
                name: "animate",
                reason: format!("unknown preset `{preset}`"),
            });
        }
        let keyframes = args.get("keyframes").unwrap_or("");

        let html = format!(
            r#"<div class="animate-block" data-preset="{preset}" data-keyframes='{kf}'>
{inner}
</div>"#,
            kf = keyframes.replace('\'', "&#39;")
        );

        Ok(RenderedBlock {
            html,
            assets: vec![
                Asset { kind: AssetKind::Js, src: MOTION_JS.into(), defer: true },
                Asset { kind: AssetKind::Js, src: BLOCK_JS.into(),  defer: true },
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_preset() {
        let mut a = ShortcodeArgs::new();
        a.insert("preset", "wobble");
        let err = Animate.render(&a, Some("x")).unwrap_err();
        assert!(matches!(err, RenderError::InvalidBody { .. }));
    }

    #[test]
    fn defaults_to_fade() {
        let a = ShortcodeArgs::new();
        let out = Animate.render(&a, Some("<p>hi</p>")).unwrap();
        assert!(out.html.contains(r#"data-preset="fade""#));
    }
}
