use crate::{Asset, AssetKind, RenderError, RenderedBlock, Shortcode, ShortcodeArgs};

pub struct Chart;

const CHART_JS: &str = "/assets/vendor/chart.umd.min.js";
const BLOCK_JS: &str = "/assets/blocks/chart.js";
const BLOCK_CSS: &str = "/assets/blocks/chart.css";

impl Shortcode for Chart {
    fn name(&self) -> &'static str {
        "chart"
    }

    fn render(
        &self,
        args: &ShortcodeArgs,
        _body: Option<&str>,
    ) -> Result<RenderedBlock, RenderError> {
        let kind = args.get("type").unwrap_or("line");
        if !matches!(kind, "line" | "bar" | "scatter" | "radar" | "doughnut") {
            return Err(RenderError::InvalidBody {
                name: "chart",
                reason: format!("unsupported chart type `{kind}`"),
            });
        }

        let src = args.get("src");
        let data = args.get("data");
        if src.is_none() && data.is_none() {
            return Err(RenderError::InvalidBody {
                name: "chart",
                reason: "either `src` or `data` is required".into(),
            });
        }

        let caption = args
            .get("caption")
            .map(|c| format!("<figcaption>{}</figcaption>", html_escape(c)))
            .unwrap_or_default();

        let attrs = if let Some(s) = src {
            format!(r#"data-chart-src="{}""#, html_escape(s))
        } else {
            format!(
                r#"data-chart-inline='{}'"#,
                data.unwrap().replace('\'', "&#39;")
            )
        };

        let id = next_block_id();
        let html = format!(
            r#"<figure class="chart-block">
<canvas id="chart-{id}" data-chart-type="{kind}" {attrs}></canvas>
{caption}
</figure>"#
        );

        Ok(RenderedBlock {
            html,
            assets: vec![
                Asset {
                    kind: AssetKind::Css,
                    src: BLOCK_CSS.into(),
                    defer: false,
                },
                Asset {
                    kind: AssetKind::Js,
                    src: CHART_JS.into(),
                    defer: true,
                },
                Asset {
                    kind: AssetKind::Js,
                    src: BLOCK_JS.into(),
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
}

fn next_block_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(1);
    let mut v = N.fetch_add(1, Ordering::Relaxed);
    let mut s = String::with_capacity(8);
    while v > 0 {
        let d = (v % 36) as u32;
        s.push(std::char::from_digit(d, 36).unwrap());
        v /= 36;
    }
    while s.len() < 8 {
        s.push('0');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_data_or_src() {
        let mut a = ShortcodeArgs::new();
        a.insert("type", "line");
        let err = Chart.render(&a, None).unwrap_err();
        assert!(matches!(err, RenderError::InvalidBody { .. }));
    }

    #[test]
    fn rejects_bad_type() {
        let mut a = ShortcodeArgs::new();
        a.insert("type", "pieces");
        a.insert("src", "x.json");
        let err = Chart.render(&a, None).unwrap_err();
        assert!(matches!(err, RenderError::InvalidBody { .. }));
    }

    #[test]
    fn emits_canvas_and_assets() {
        let mut a = ShortcodeArgs::new();
        a.insert("type", "bar");
        a.insert("src", "data/x.json");
        let out = Chart.render(&a, None).unwrap();
        assert!(out.html.contains("<canvas"));
        assert!(out.html.contains(r#"data-chart-type="bar""#));
        assert_eq!(out.assets.len(), 3);
    }
}
