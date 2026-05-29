//! Shortcode block registry. Each block type implements `Shortcode` and is
//! registered into a `Registry`. The content render pipeline looks up by name.

pub mod animate;
pub mod args;
pub mod callout;
pub mod chart;
pub mod code;
pub mod embed;
pub mod image;
pub mod playable;

use std::collections::HashMap;
use thiserror::Error;

pub use args::{parse as parse_args, ArgsError, ShortcodeArgs};

/// Minimal HTML attribute-value escape: handles `&` and `"` only.
/// Use for URLs and other strings that go inside double-quoted attributes.
pub fn html_attr_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('"', "&quot;")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssetKind {
    Css,
    Js,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Asset {
    pub kind: AssetKind,
    pub src: String,
    pub defer: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderedBlock {
    pub html: String,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RenderError {
    #[error("args: {0}")]
    Args(#[from] ArgsError),
    #[error("missing required body for `{0}`")]
    MissingBody(&'static str),
    #[error("invalid body for `{name}`: {reason}")]
    InvalidBody { name: &'static str, reason: String },
    #[error("other: {0}")]
    Other(String),
}

pub trait Shortcode: Send + Sync {
    fn name(&self) -> &'static str;
    fn render(
        &self,
        args: &ShortcodeArgs,
        body: Option<&str>,
    ) -> Result<RenderedBlock, RenderError>;
    /// Whether this block requires a paired closing tag.
    fn paired(&self) -> bool {
        false
    }
    /// If true, paired body is rendered through markdown before being passed
    /// to render(). If false, body is passed raw. Default: true.
    fn body_is_markdown(&self) -> bool {
        true
    }
}

#[derive(Default)]
pub struct Registry {
    map: HashMap<&'static str, Box<dyn Shortcode>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<S: Shortcode + 'static>(&mut self, s: S) {
        self.map.insert(s.name(), Box::new(s));
    }

    pub fn get(&self, name: &str) -> Option<&dyn Shortcode> {
        self.map.get(name).map(|b| b.as_ref())
    }

    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.map.keys().copied()
    }
}

/// Default registry with all v1 shortcodes.
pub fn default_registry() -> Registry {
    let mut r = Registry::new();
    r.register(callout::Callout);
    r.register(code::Code);
    r.register(image::Image);
    r.register(chart::Chart);
    r.register(animate::Animate);
    r.register(playable::Playable);
    r.register(embed::Embed);
    r
}
