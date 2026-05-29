use import_research::{parse, emit};

const SRC: &str = include_str!("fixtures/mini-research.md");

#[test]
fn split_then_emit_produces_two_articles() {
    let domains = parse::split_domains(SRC).unwrap();
    assert_eq!(domains.len(), 2);

    let a = emit::to_article(&domains[0]);
    assert!(a.contains("series_order: 1"));
    assert!(a.contains(r#"{{< callout type="info" >}}"#));
    assert!(a.contains(r#"{{< code lang="rust" playground="true" >}}"#));
    assert!(a.contains("#![no_std]"));
    assert!(a.contains("<!-- TODO: chart? -->"));
    assert!(a.contains("<!-- TODO: diagram? -->"));
    assert!(!a.contains(".1\n"));    // footnotes gone
    assert!(!a.contains(".4 "));     // footnotes gone

    let b = emit::to_article(&domains[1]);
    assert!(b.contains("series_order: 2"));
    assert!(b.contains("async fn handle()"));
}

#[test]
fn idempotent_when_writing_to_disk() {
    let tmp = tempfile::tempdir().unwrap();
    let path_a = tmp.path().join("domain-1-bare-metal-foo.md");
    let domains = parse::split_domains(SRC).unwrap();
    std::fs::write(&path_a, emit::to_article(&domains[0])).unwrap();
    let v1 = std::fs::read_to_string(&path_a).unwrap();
    std::fs::write(&path_a, emit::to_article(&domains[0])).unwrap();
    let v2 = std::fs::read_to_string(&path_a).unwrap();
    assert_eq!(v1, v2);
}
