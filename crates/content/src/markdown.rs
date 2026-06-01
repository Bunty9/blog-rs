use pulldown_cmark::{html, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::toc::Slugger;

/// Raw heading collected while parsing.
#[derive(Debug)]
pub struct HeadingInfo {
    pub level: u8,
    pub id: String,
    pub text: String,
}

/// Output from [`to_html_full`].
pub struct MarkdownOutput {
    pub html: String,
    /// Flat list of headings in document order.
    pub headings: Vec<HeadingInfo>,
    /// Word count excluding code-block content.
    pub word_count: usize,
}

/// Render CommonMark to HTML, collecting heading info and word count.
///
/// Each heading (h1–h6) gets an `id=` attribute derived by [`Slugger`].
/// Word count is accumulated from all text events **outside** fenced/indented
/// code blocks.
pub fn to_html_full(src: &str) -> MarkdownOutput {
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS;

    let parser = Parser::new_ext(src, opts);

    let mut headings: Vec<HeadingInfo> = Vec::new();
    let mut word_count: usize = 0;
    let mut slugger = Slugger::default();

    // Two-pass strategy:
    //   Pass 1: walk events to collect heading text and pre-compute slugs (so
    //           dedup order exactly matches document order).
    //   Pass 2: replay events, substituting heading-start events with ones that
    //           carry the pre-computed `id=` attribute; accumulate word counts
    //           outside code blocks.
    let events: Vec<Event> = parser.collect();

    // Two-pass: first collect heading spans (start_idx → end_idx) with their text,
    // then build the transformed event stream.
    //
    // Strategy: iterate once to find each heading's text (between Start and End),
    // pre-compute all slugs (so dedup order matches document order), then iterate
    // again emitting modified events.

    // Pass 1: extract (event_index_of_start, level, plain_text) for each heading.
    struct HeadingSpan {
        start_idx: usize,
        level: u8,
        slug: String,
        text: String,
    }

    let mut heading_spans: Vec<HeadingSpan> = Vec::new();
    {
        let mut pending: Option<(usize, u8, String)> = None; // (start_idx, level, text)
        let mut depth = 0usize; // nesting inside a heading (for inline elements)
        for (i, ev) in events.iter().enumerate() {
            match ev {
                Event::Start(Tag::Heading { level, .. }) if pending.is_none() => {
                    pending = Some((i, heading_level_to_u8(*level), String::new()));
                    depth = 1;
                }
                Event::Start(_) if pending.is_some() => {
                    depth += 1;
                }
                Event::End(TagEnd::Heading(_)) if pending.is_some() => {
                    depth -= 1;
                    if depth == 0 {
                        if let Some((start_idx, level, text)) = pending.take() {
                            let slug = slugger.slug(&text);
                            heading_spans.push(HeadingSpan {
                                start_idx,
                                level,
                                slug,
                                text,
                            });
                        }
                    }
                }
                Event::End(_) if pending.is_some() => {
                    depth -= 1;
                }
                Event::Text(t) if pending.is_some() => {
                    if let Some((_, _, ref mut acc)) = pending {
                        acc.push_str(t);
                    }
                }
                Event::Code(t) if pending.is_some() => {
                    // inline code inside a heading contributes to slug text
                    if let Some((_, _, ref mut acc)) = pending {
                        acc.push_str(t);
                    }
                }
                _ => {}
            }
        }
    }

    // Build a lookup: start_idx → index into heading_spans.
    let start_to_span: std::collections::HashMap<usize, usize> = heading_spans
        .iter()
        .enumerate()
        .map(|(span_i, hs)| (hs.start_idx, span_i))
        .collect();

    // Pass 2: transform events — replace heading Start with one carrying id=,
    // collect word counts outside code blocks, emit HTML.
    let transformed: Vec<Event> = {
        let mut out_events: Vec<Event> = Vec::with_capacity(events.len());
        let mut in_code_block_local = false;

        for (i, ev) in events.iter().enumerate() {
            match ev {
                Event::Start(Tag::Heading { level, .. }) => {
                    if let Some(&span_i) = start_to_span.get(&i) {
                        let slug = &heading_spans[span_i].slug;
                        // Replace the heading start event with one that has id attribute.
                        // pulldown-cmark 0.10 Tag::Heading has id/classes fields.
                        out_events.push(Event::Start(Tag::Heading {
                            level: *level,
                            id: Some(slug.as_str().into()),
                            classes: Default::default(),
                            attrs: Default::default(),
                        }));
                    } else {
                        out_events.push(ev.clone());
                    }
                }
                Event::Start(Tag::CodeBlock(_)) => {
                    in_code_block_local = true;
                    out_events.push(ev.clone());
                }
                Event::End(TagEnd::CodeBlock) => {
                    in_code_block_local = false;
                    out_events.push(ev.clone());
                }
                Event::Text(t) => {
                    if !in_code_block_local {
                        word_count += count_words(t);
                    }
                    out_events.push(ev.clone());
                }
                _ => {
                    out_events.push(ev.clone());
                }
            }
        }
        out_events
    };

    // Collect headings in document order from heading_spans (already sorted by start_idx).
    for hs in &heading_spans {
        headings.push(HeadingInfo {
            level: hs.level,
            id: hs.slug.clone(),
            text: hs.text.clone(),
        });
    }

    let mut html = String::with_capacity(src.len());
    html::push_html(&mut html, transformed.into_iter());

    MarkdownOutput {
        html,
        headings,
        word_count,
    }
}

/// Compatibility shim: render CommonMark to HTML, no heading info or word count.
pub fn to_html(src: &str) -> String {
    to_html_full(src).html
}

fn heading_level_to_u8(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn count_words(text: &str) -> usize {
    text.split_whitespace().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_basic_markdown() {
        let html = to_html("# Hello\n\n**bold**");
        assert!(html.contains(r#"<h1 id="hello">"#));
        assert!(html.contains("<strong>bold</strong>"));
    }

    #[test]
    fn renders_tables() {
        let md = "| a | b |\n|---|---|\n| 1 | 2 |";
        let html = to_html(md);
        assert!(html.contains("<table>"));
        assert!(html.contains("<th>a</th>"));
    }

    #[test]
    fn passes_through_inline_html() {
        let html = to_html("<div class=\"x\">y</div>");
        assert!(html.contains("<div class=\"x\">y</div>"));
    }

    #[test]
    fn heading_ids_are_slugified() {
        let out = to_html_full("# Hello World\n\n## My Section\n\ntext");
        assert!(out.html.contains(r#"<h1 id="hello-world">"#));
        assert!(out.html.contains(r#"<h2 id="my-section">"#));
        assert_eq!(out.headings.len(), 2);
        assert_eq!(out.headings[0].id, "hello-world");
        assert_eq!(out.headings[1].id, "my-section");
    }

    #[test]
    fn word_count_excludes_code_blocks() {
        let src = "hello world\n\n```\nfn ignored_code() {}\n```\n\nfoo";
        let out = to_html_full(src);
        // "hello", "world", "foo" = 3 words; code block text excluded
        assert_eq!(out.word_count, 3);
    }
}
