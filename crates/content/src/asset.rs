use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssetKind {
    Css,
    Js,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Asset {
    pub kind: AssetKind,
    pub src: String,
    pub defer: bool,
}

impl Asset {
    pub fn css(src: impl Into<String>) -> Self {
        Self {
            kind: AssetKind::Css,
            src: src.into(),
            defer: false,
        }
    }
    pub fn js(src: impl Into<String>, defer: bool) -> Self {
        Self {
            kind: AssetKind::Js,
            src: src.into(),
            defer,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetManifest {
    pub assets: Vec<Asset>,
}

impl AssetManifest {
    pub fn add(&mut self, a: Asset) {
        if !self.assets.contains(&a) {
            self.assets.push(a);
        }
    }
    pub fn extend(&mut self, other: impl IntoIterator<Item = Asset>) {
        for a in other {
            self.add(a);
        }
    }
    pub fn is_empty(&self) -> bool {
        self.assets.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedupes_identical_assets() {
        let mut m = AssetManifest::default();
        m.add(Asset::css("/a.css"));
        m.add(Asset::css("/a.css"));
        m.add(Asset::js("/b.js", true));
        m.add(Asset::js("/b.js", true));
        assert_eq!(m.assets.len(), 2);
    }

    #[test]
    fn keeps_distinct_assets() {
        let mut m = AssetManifest::default();
        m.add(Asset::js("/a.js", true));
        m.add(Asset::js("/a.js", false)); // different `defer`
        assert_eq!(m.assets.len(), 2);
    }
}
