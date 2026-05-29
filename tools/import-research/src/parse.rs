//! Domain splitter + footnote stripper + rust-code-block detector.

use once_cell::sync::Lazy;
use regex::Regex;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("malformed domain heading at byte {0}")]
    BadHeading(usize),
}

/// One domain section, post-cleanup.
#[derive(Debug, Clone)]
pub struct Domain {
    pub number: u32,
    pub title: String,
    pub slug: String,
    /// First paragraph after the heading — used as the callout summary.
    pub summary: String,
    /// Full body with footnotes stripped and code blocks shortcode-wrapped.
    pub body: String,
}

static DOMAIN_HEADING: Lazy<Regex> = Lazy::new(|| {
    // Matches: `## **Domain 1: Bare-Metal Firmware ...**`
    // Captures: (1) number, (2) full title-after-colon.
    Regex::new(r"(?m)^## \*\*Domain (\d+): ([^*\n]+?)\*\*\s*$").unwrap()
});

static FOOTNOTE_MARKER: Lazy<Regex> = Lazy::new(|| {
    // Matches: `.4`, `\.4`, `.11` immediately following a word char (no leading whitespace).
    Regex::new(r"\\?\.\d+(?:\b|$)").unwrap()
});

static IMAGE_REF: Lazy<Regex> = Lazy::new(|| {
    // `![][image1]` placeholders the source uses for diagrams — leave a TODO comment.
    Regex::new(r"!\[\]\[image\d+\]").unwrap()
});

static RUST_FENCE_PROSE: Lazy<Regex> = Lazy::new(|| {
    // Detects pseudo-fenced "Rust" code blocks where the source author wrote:
    //   Rust  <newline>
    //   <code>
    //   <blank line>
    // We capture from the `Rust` marker up to the first blank line as the
    // code body. This is an approximation — see tests for accepted shapes.
    Regex::new(r"(?m)^Rust\s*\n((?:.+\n)+?)\n").unwrap()
});

/// Heuristic table detector: a markdown table whose data rows contain a digit
/// in any cell — candidate for a chart visualization.
static NUMERIC_TABLE: Lazy<Regex> = Lazy::new(|| {
    // Conservative: capture a header line `| ... |`, divider `| :-: | ... |`,
    // and at least one data row containing a digit between pipes.
    Regex::new(r"(?m)^\|[^\n]*\|\s*\n\|[\s:|-]+\|\s*\n(?:\|[^\n]*\d[^\n]*\|\s*\n)+").unwrap()
});

pub fn split_domains(src: &str) -> Result<Vec<Domain>, ParseError> {
    let mut starts: Vec<(usize, u32, String)> = Vec::new();
    for cap in DOMAIN_HEADING.captures_iter(src) {
        let m = cap.get(0).unwrap();
        let n: u32 = cap[1].parse().map_err(|_| ParseError::BadHeading(m.start()))?;
        let title = cap[2].trim().to_string();
        starts.push((m.end(), n, title));
    }

    let mut domains = Vec::new();
    for (i, (start, num, title)) in starts.iter().enumerate() {
        let end = starts.get(i + 1).map(|(s, _, _)| *s).unwrap_or(src.len());
        // Walk back from `end` to the start of the next heading line.
        let mut real_end = end;
        if i + 1 < starts.len() {
            // strip the trailing "\n## " bytes
            while real_end > *start && !src[..real_end].ends_with('\n') {
                real_end -= 1;
            }
            // and the `##` itself was matched at the end of the slice; remove the
            // last 3 chars `## ` if present.
        }
        let raw_body = &src[*start..real_end];
        let cleaned = clean_body(raw_body);
        let summary = first_paragraph(&cleaned);

        domains.push(Domain {
            number: *num,
            title: title.clone(),
            slug: slugify(title),
            summary,
            body: cleaned,
        });
    }
    Ok(domains)
}

fn clean_body(input: &str) -> String {
    // 1. strip footnote markers and image refs.
    let s1 = FOOTNOTE_MARKER.replace_all(input, "");
    let s2 = IMAGE_REF.replace_all(&s1, "<!-- TODO: diagram? -->");

    // 2. wrap pseudo-fenced rust blocks.
    let s3 = RUST_FENCE_PROSE.replace_all(&s2, |caps: &regex::Captures| {
        let body = caps.get(1).unwrap().as_str()
            .replace("\\#", "#")
            .replace("\\!", "!")
            .replace("\\[", "[")
            .replace("\\]", "]")
            .replace("\\_", "_")
            .replace("\\-", "-")
            .replace("\\>", ">");
        format!("{{{{< code lang=\"rust\" playground=\"true\" >}}}}\n{body}{{{{< /code >}}}}\n\n")
    });

    // 3. annotate numeric tables.
    let mut out = String::with_capacity(s3.len());
    let mut last = 0usize;
    for m in NUMERIC_TABLE.find_iter(&s3) {
        out.push_str(&s3[last..m.start()]);
        out.push_str("<!-- TODO: chart? -->\n");
        out.push_str(m.as_str());
        last = m.end();
    }
    out.push_str(&s3[last..]);

    out.trim_start_matches('\n').to_string()
}

fn first_paragraph(body: &str) -> String {
    let mut buf = String::new();
    for line in body.lines() {
        if line.trim().is_empty() {
            if !buf.is_empty() {
                break;
            }
            continue;
        }
        if line.starts_with('#') || line.starts_with('|') || line.starts_with("```") {
            continue;
        }
        if !buf.is_empty() { buf.push(' '); }
        buf.push_str(line.trim());
        if buf.len() > 600 { break; }
    }
    buf
}

pub fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_dash = true;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') { out.pop(); }
    while out.starts_with('-') { out.remove(0); }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINY: &str = "## **Domain 1: Bare-Metal Foo**\n\nIntro paragraph for domain 1.1\n\nRust  \n\\#\\[panic_handler\\]\nfn p() {}\n\n## **Domain 2: Async Bar**\n\nIntro for domain 2.4 with footnote.\n";

    #[test]
    fn splits_two_domains() {
        let d = split_domains(TINY).unwrap();
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].number, 1);
        assert_eq!(d[0].title, "Bare-Metal Foo");
        assert_eq!(d[0].slug, "bare-metal-foo");
        assert_eq!(d[1].slug, "async-bar");
    }

    #[test]
    fn strips_footnote_markers() {
        let d = split_domains(TINY).unwrap();
        assert!(!d[0].body.contains(".1"));
        assert!(!d[1].body.contains(".4"));
    }

    #[test]
    fn wraps_rust_pseudo_fence() {
        let d = split_domains(TINY).unwrap();
        assert!(d[0].body.contains(r#"{{< code lang="rust" playground="true" >}}"#));
        assert!(d[0].body.contains(r#"{{< /code >}}"#));
        // backslash escapes removed
        assert!(d[0].body.contains("#[panic_handler]"));
    }

    #[test]
    fn summary_is_first_paragraph() {
        let d = split_domains(TINY).unwrap();
        assert!(d[0].summary.starts_with("Intro paragraph for domain"));
    }

    #[test]
    fn slugify_handles_punct_and_spaces() {
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("  --foo--  "), "foo");
        assert_eq!(slugify("Domain N"), "domain-n");
    }
}
