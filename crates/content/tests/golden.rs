use content::render;
use once_cell::sync::Lazy;
use regex::Regex;

static CHART_ID_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"id="chart-[a-z0-9]+""#).unwrap());

fn normalize(html: &str) -> String {
    CHART_ID_RE
        .replace_all(html, r#"id="chart-XXXX""#)
        .to_string()
}

#[test]
fn all_shortcodes_snapshot() {
    let src = include_str!("../../../tests/fixtures/all-shortcodes.md");
    let out = render(src).unwrap();

    insta::assert_yaml_snapshot!("frontmatter", out.frontmatter);
    insta::assert_snapshot!("html", normalize(&out.html));

    let mut assets: Vec<_> = out
        .assets
        .assets
        .iter()
        .map(|a| (a.kind, a.src.clone(), a.defer))
        .collect();
    assets.sort_by(|a, b| a.1.cmp(&b.1));
    insta::assert_debug_snapshot!("assets", assets);
}

#[test]
fn domain_1_snapshot() {
    let src = include_str!("../../../tests/fixtures/domain-1-snippet.md");
    let out = content::render(src).unwrap();
    insta::assert_snapshot!("domain-1-html", normalize(&out.html));
}
