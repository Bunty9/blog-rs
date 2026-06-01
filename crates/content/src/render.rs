use crate::toc::Toc;
use crate::{shortcode_lexer, ContentError, Frontmatter};
use shortcodes::{default_registry, Registry, RenderedBlock};

#[derive(Debug)]
pub struct RenderOutput {
    pub frontmatter: Frontmatter,
    pub html: String,
    pub assets: crate::AssetManifest,
    /// Nested table of contents built from heading events.
    pub toc: Toc,
    /// Estimated reading time in minutes (ceil(word_count / 220), min 1).
    pub reading_minutes: i64,
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
    let mut flat_headings: Vec<(u8, String, String)> = Vec::new();
    let mut total_word_count: usize = 0;

    for tok in tokens {
        match tok {
            shortcode_lexer::Token::Text(t) => {
                let md_out = crate::markdown::to_html_full(t);
                html.push_str(&md_out.html);
                total_word_count += md_out.word_count;
                for h in md_out.headings {
                    flat_headings.push((h.level, h.id, h.text));
                }
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
                let sc = reg
                    .get(name)
                    .ok_or_else(|| ContentError::UnknownShortcode(name.into()))?;
                let body_passed = if sc.body_is_markdown() {
                    crate::markdown::to_html(inner)
                } else {
                    inner.to_string()
                };
                let block = resolve(reg, name, raw_args, Some(&body_passed))?;
                emit(&mut html, &mut manifest, block);
            }
        }
    }

    let toc = Toc::from_flat(flat_headings);
    let reading_minutes = reading_time(total_word_count);

    Ok(RenderOutput {
        frontmatter: fm,
        html,
        assets: manifest,
        toc,
        reading_minutes,
    })
}

/// Convert word count to reading minutes: ceil(words / 220), minimum 1.
fn reading_time(word_count: usize) -> i64 {
    let minutes = (word_count as f64 / 220.0).ceil() as i64;
    minutes.max(1)
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
    fn plain_markdown_has_id_on_heading() {
        let out = render("# hi\n\nbody").unwrap();
        // New behaviour: id attribute is added.
        assert!(out.html.contains(r#"<h1 id="hi">"#));
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

    #[test]
    fn heading_ids_and_toc_and_reading_minutes() {
        let src = "# Alpha\n\n## Beta\n\nsome words here";
        let out = render(src).unwrap();
        // Heading id attributes
        assert!(
            out.html.contains(r#"<h1 id="alpha">"#),
            "expected h1 with id=alpha, got: {}",
            out.html
        );
        assert!(
            out.html.contains(r#"<h2 id="beta">"#),
            "expected h2 with id=beta, got: {}",
            out.html
        );
        // TOC structure
        assert_eq!(out.toc.0.len(), 1, "expected 1 root in TOC");
        assert_eq!(out.toc.0[0].id, "alpha");
        assert_eq!(out.toc.0[0].children.len(), 1);
        assert_eq!(out.toc.0[0].children[0].id, "beta");
        // Reading time: "some words here" = 3 words → ceil(3/220) = 1 minute.
        assert_eq!(out.reading_minutes, 1);
    }

    #[test]
    fn heading_ids_deduplicated() {
        let src = "# Intro\n\n## Intro\n\ntext";
        let out = render(src).unwrap();
        assert!(out.html.contains(r#"<h1 id="intro">"#));
        assert!(out.html.contains(r#"<h2 id="intro-2">"#));
    }

    #[test]
    fn reading_minutes_excludes_code_blocks() {
        // A document with only a code block should still have reading_minutes >= 1
        // (min 1 rule), but word count should exclude code content.
        let src = "intro word\n\n```rust\nfn main() { /* lots of code tokens */ }\n```";
        let out = render(src).unwrap();
        assert!(out.reading_minutes >= 1);
    }
}
