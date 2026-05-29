use crate::{html_attr_escape, RenderError, RenderedBlock, Shortcode, ShortcodeArgs};

pub struct Playable;

const RUST_PLAYGROUND: &str = "https://play.rust-lang.org/?version=stable&mode=debug&edition=2021";

impl Shortcode for Playable {
    fn name(&self) -> &'static str {
        "playable"
    }

    fn render(
        &self,
        args: &ShortcodeArgs,
        _body: Option<&str>,
    ) -> Result<RenderedBlock, RenderError> {
        let id = args.required("id")?;
        match id {
            "rust-playground" => {
                let gist = args.get("gist").unwrap_or("");
                let url = if gist.is_empty() {
                    RUST_PLAYGROUND.to_string()
                } else {
                    format!("{RUST_PLAYGROUND}&gist={gist}")
                };
                let url_esc = html_attr_escape(&url);
                let html = format!(
                    r#"<iframe class="playable-block" src="{url_esc}" loading="lazy" title="Rust Playground" sandbox="allow-scripts allow-same-origin allow-forms"></iframe>"#
                );
                Ok(RenderedBlock {
                    html,
                    assets: vec![],
                })
            }
            other => Err(RenderError::Other(format!("unknown playable id `{other}`"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_playground_iframe() {
        let mut a = ShortcodeArgs::new();
        a.insert("id", "rust-playground");
        a.insert("gist", "abc123");
        let out = Playable.render(&a, None).unwrap();
        assert!(out.html.contains("play.rust-lang.org"));
        assert!(out.html.contains("gist=abc123"));
    }

    #[test]
    fn unknown_id_errors() {
        let mut a = ShortcodeArgs::new();
        a.insert("id", "godbolt");
        let err = Playable.render(&a, None).unwrap_err();
        assert!(matches!(err, RenderError::Other(_)));
    }
}
