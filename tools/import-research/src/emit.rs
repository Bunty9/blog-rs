//! Render a parsed Domain into a frontmatter + body markdown file.

use crate::parse::Domain;

pub fn to_article(d: &Domain) -> String {
    let tags = derive_tags(d);
    let mut out = String::with_capacity(d.body.len() + 256);

    out.push_str("---\n");
    out.push_str(&format!("title: {}\n", yaml_quote(&d.title)));
    out.push_str("subtitle: ~\n");
    out.push_str(&format!("tags: [{}]\n", tags.join(", ")));
    out.push_str("series: rust-level-4\n");
    out.push_str(&format!("series_order: {}\n", d.number));
    out.push_str("status: draft\n");
    out.push_str("---\n\n");

    if !d.summary.is_empty() {
        out.push_str(r#"{{< callout type="info" >}}"#);
        out.push('\n');
        out.push_str(d.summary.trim());
        out.push('\n');
        out.push_str(r#"{{< /callout >}}"#);
        out.push_str("\n\n");
    }

    out.push_str(&d.body);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn derive_tags(d: &Domain) -> Vec<String> {
    let mut tags = vec!["rust".to_string(), "level-4".to_string()];
    let lt = d.title.to_ascii_lowercase();
    if lt.contains("bare-metal") || lt.contains("firmware") {
        tags.push("embedded".into());
    }
    if lt.contains("proxy") || lt.contains("network") {
        tags.push("networking".into());
    }
    if lt.contains("storage") || lt.contains("database") {
        tags.push("databases".into());
    }
    if lt.contains("ledger") || lt.contains("decentralized") || lt.contains("blockchain") {
        tags.push("blockchain".into());
    }
    if lt.contains("trading") || lt.contains("latency") || lt.contains("hft") {
        tags.push("hft".into());
    }
    tags
}

fn yaml_quote(s: &str) -> String {
    if s.contains(':') || s.contains('#') || s.contains('"') || s.contains('\'') {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::Domain;

    fn fixture() -> Domain {
        Domain {
            number: 1,
            title: "Bare-Metal Firmware: a deep dive".into(),
            slug: "bare-metal-firmware-a-deep-dive".into(),
            summary: "Bare-metal Rust drops std".into(),
            body: "Body here.\n".into(),
        }
    }

    #[test]
    fn emits_frontmatter_and_callout() {
        let s = to_article(&fixture());
        assert!(s.starts_with("---\n"));
        assert!(s.contains("series: rust-level-4"));
        assert!(s.contains("series_order: 1"));
        assert!(s.contains(r#"{{< callout type="info" >}}"#));
        assert!(s.contains("Body here."));
    }

    #[test]
    fn yaml_quotes_colons() {
        let s = to_article(&fixture());
        assert!(s.contains(r#"title: "Bare-Metal Firmware: a deep dive""#));
    }
}
