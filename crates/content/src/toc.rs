use serde::Serialize;
use std::collections::HashMap;

/// A single entry in a table of contents.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TocEntry {
    pub level: u8,
    pub id: String,
    pub text: String,
    pub children: Vec<TocEntry>,
}

/// The table of contents for a document, represented as a forest of
/// [`TocEntry`] nodes.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct Toc(pub Vec<TocEntry>);

impl Toc {
    /// Build a nested [`Toc`] from a flat list of `(level, id, text)` triples.
    ///
    /// The nesting follows the same rules as HTML heading hierarchy: a heading
    /// whose level is greater than the previous heading becomes a child; one
    /// at the same or lower level pops back to the appropriate ancestor.
    pub fn from_flat(entries: impl IntoIterator<Item = (u8, String, String)>) -> Self {
        // We build the tree by maintaining a stack of (level, node) pairs.
        // Each element in the stack is a mutable "current node" accumulator.
        let mut roots: Vec<TocEntry> = Vec::new();
        // Stack of (level, accumulated children so far) for ancestors.
        // When we encounter a heading we pop back up until we find the right parent.
        let mut stack: Vec<(u8, TocEntry)> = Vec::new();

        for (level, id, text) in entries {
            let entry = TocEntry {
                level,
                id,
                text,
                children: Vec::new(),
            };

            // Pop stack entries that are at the same or deeper level — they
            // are "closed" siblings/children of the current heading.
            while let Some((top_level, _)) = stack.last() {
                if *top_level >= level {
                    let (_, closed) = stack.pop().unwrap();
                    if let Some((_, parent)) = stack.last_mut() {
                        parent.children.push(closed);
                    } else {
                        roots.push(closed);
                    }
                } else {
                    break;
                }
            }

            stack.push((level, entry));
        }

        // Drain remaining stack in reverse order.
        while let Some((_, closed)) = stack.pop() {
            if let Some((_, parent)) = stack.last_mut() {
                parent.children.push(closed);
            } else {
                roots.push(closed);
            }
        }

        // Roots are accumulated in document order: each root is pushed to `roots`
        // as soon as it is "closed" (either by a same-or-lower-level successor or
        // during the final drain). The drain processes the stack from top (deepest)
        // to bottom (shallowest), so children are attached before their parent and
        // the parent is eventually pushed to `roots` in the correct order.
        Toc(roots)
    }
}

/// Generates unique slug identifiers for headings within a single document.
///
/// Converts text to lowercase, collapses non-alphanumeric runs to `-`,
/// strips leading/trailing `-`, and appends `-2`, `-3`, … for repeats.
#[derive(Debug, Default)]
pub struct Slugger {
    seen: HashMap<String, u32>,
}

impl Slugger {
    /// Produce a slug from `text`, deduplicating within this document.
    pub fn slug(&mut self, text: &str) -> String {
        let base = slugify_text(text);
        let count = self.seen.entry(base.clone()).or_insert(0);
        *count += 1;
        if *count == 1 {
            base
        } else {
            format!("{}-{}", base, count)
        }
    }
}

/// Pure function: lowercase + replace non-alphanumeric runs with `-`, trim `-`.
fn slugify_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_sep = false;

    for ch in text.chars() {
        if ch.is_alphanumeric() {
            for lc in ch.to_lowercase() {
                out.push(lc);
            }
            in_sep = false;
        } else if !in_sep && !out.is_empty() {
            out.push('-');
            in_sep = true;
        }
    }

    // Strip trailing separator.
    if out.ends_with('-') {
        out.pop();
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_dedupes() {
        let mut s = Slugger::default();
        assert_eq!(s.slug("Intro"), "intro");
        assert_eq!(s.slug("Intro"), "intro-2");
        assert_eq!(s.slug("Hello, World!"), "hello-world");
    }

    #[test]
    fn slugify_basic() {
        let mut s = Slugger::default();
        assert_eq!(s.slug("Hello World"), "hello-world");
        assert_eq!(s.slug("foo--bar"), "foo-bar");
        assert_eq!(s.slug("  leading"), "leading");
        assert_eq!(s.slug("trailing  "), "trailing");
    }

    #[test]
    fn toc_from_flat_nesting() {
        // [(1,"a"),(2,"b"),(2,"c"),(1,"d")] nests b,c under a and d as sibling root
        let entries = vec![
            (1u8, "a".into(), "A".into()),
            (2u8, "b".into(), "B".into()),
            (2u8, "c".into(), "C".into()),
            (1u8, "d".into(), "D".into()),
        ];
        let toc = Toc::from_flat(entries);
        assert_eq!(toc.0.len(), 2, "expected 2 roots: a and d");
        assert_eq!(toc.0[0].id, "a");
        assert_eq!(toc.0[0].children.len(), 2, "a should have 2 children");
        assert_eq!(toc.0[0].children[0].id, "b");
        assert_eq!(toc.0[0].children[1].id, "c");
        assert_eq!(toc.0[1].id, "d");
        assert_eq!(toc.0[1].children.len(), 0);
    }

    #[test]
    fn toc_empty() {
        let toc = Toc::from_flat(std::iter::empty());
        assert!(toc.0.is_empty());
    }

    #[test]
    fn toc_single_root() {
        let toc = Toc::from_flat(vec![(1u8, "intro".into(), "Intro".into())]);
        assert_eq!(toc.0.len(), 1);
        assert_eq!(toc.0[0].id, "intro");
    }
}
