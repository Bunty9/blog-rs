use crate::{shortcode_lexer, ContentError, Frontmatter};
use shortcodes::{default_registry, Registry, RenderedBlock};

#[derive(Debug)]
pub struct RenderOutput {
    pub frontmatter: Frontmatter,
    pub html: String,
    pub assets: crate::AssetManifest,
}

/// Public entry point - uses the default registry of v1 shortcodes.
pub fn render(src: &str) -> Result<RenderOutput, ContentError> {
    render_with_registry(src, &default_registry())
}

pub fn render_with_registry(src: &str, reg: &Registry) -> Result<RenderOutput, ContentError> {
    let (fm, body) = crate::frontmatter::split(src)?;
    let paired: Vec<&str> = reg
        .names()
        .filter(|n| reg.get(n).map(|s| s.paired()).unwrap_or(false))
        .collect();

    let tokens = shortcode_lexer::tokenize(body, &paired)?;
    let mut html = String::with_capacity(body.len() * 2);
    let mut manifest = crate::AssetManifest::default();

    for tok in tokens {
        match tok {
            shortcode_lexer::Token::Text(t) => {
                html.push_str(&crate::markdown::to_html(t));
            }
            shortcode_lexer::Token::Self_ { name, raw_args, .. } => {
                let block = resolve(reg, name, raw_args, None)?;
                emit(&mut html, &mut manifest, block);
            }
            shortcode_lexer::Token::Paired {
                name,
                raw_args,
                body: inner,
                ..
            } => {
                let inner_html = crate::markdown::to_html(inner);
                let block = resolve(reg, name, raw_args, Some(&inner_html))?;
                emit(&mut html, &mut manifest, block);
            }
        }
    }

    Ok(RenderOutput {
        frontmatter: fm,
        html,
        assets: manifest,
    })
}

fn resolve(
    reg: &Registry,
    name: &str,
    raw_args: &str,
    body: Option<&str>,
) -> Result<RenderedBlock, ContentError> {
    let sc = reg
        .get(name)
        .ok_or_else(|| ContentError::UnknownShortcode(name.into()))?;
    let args = shortcodes::parse_args(raw_args).map_err(|e| ContentError::MalformedArgs {
        offset: 0,
        reason: e.to_string(),
    })?;
    sc.render(&args, body)
        .map_err(|e| ContentError::Render(e.to_string()))
}

fn emit(html: &mut String, manifest: &mut crate::AssetManifest, block: RenderedBlock) {
    html.push_str(&block.html);
    for sa in block.assets {
        let kind = match sa.kind {
            shortcodes::AssetKind::Css => crate::asset::AssetKind::Css,
            shortcodes::AssetKind::Js => crate::asset::AssetKind::Js,
        };
        manifest.add(crate::Asset {
            kind,
            src: sa.src,
            defer: sa.defer,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_markdown_passes_through() {
        let out = render("# hi\n\nbody").unwrap();
        assert!(out.html.contains("<h1>hi</h1>"));
        assert!(out.html.contains("<p>body</p>"));
        assert!(out.assets.is_empty());
    }

    #[test]
    fn self_closing_shortcode() {
        let src = "before\n\n{{< chart type=\"bar\" src=\"x.json\" >}}\n\nafter";
        let out = render(src).unwrap();
        assert!(out.html.contains("<canvas"));
        assert!(out.html.contains("<p>before</p>"));
        assert!(out.html.contains("<p>after</p>"));
        assert!(!out.assets.is_empty());
    }

    #[test]
    fn paired_callout_with_markdown_body() {
        let src = "{{< callout type=\"warn\" >}}\n**heads up**\n{{< /callout >}}";
        let out = render(src).unwrap();
        assert!(out.html.contains(r#"data-callout="warn""#));
        assert!(out.html.contains("<strong>heads up</strong>"));
    }

    #[test]
    fn unknown_shortcode_errors() {
        let err = render("{{< nope >}}").unwrap_err();
        assert!(matches!(err, ContentError::UnknownShortcode(_)));
    }

    #[test]
    fn deduplicates_assets() {
        let src = r#"
{{< chart type="line" src="a.json" >}}

{{< chart type="bar"  src="b.json" >}}
"#;
        let out = render(src).unwrap();
        assert_eq!(out.assets.assets.len(), 3);
    }
}
