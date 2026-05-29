use crate::ContentError;
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token<'a> {
    /// Plain markdown text between shortcodes.
    Text(&'a str),
    /// `{{< name args... >}}` self-closing.
    Self_ {
        name: &'a str,
        raw_args: &'a str,
        offset: usize,
    },
    /// `{{< name args... >}} ... {{< /name >}}` paired.
    Paired {
        name: &'a str,
        raw_args: &'a str,
        body: &'a str,
        offset: usize,
    },
}

static OPEN_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\{\{<\s*([a-zA-Z_][a-zA-Z0-9_-]*)\s*([^>]*?)\s*>\}\}").unwrap());

const CLOSE_RE_FMT: &str = r"\{\{<\s*/\s*{name}\s*>\}\}";

/// Scan `src` once and collect (start, end) byte ranges for every HTML comment
/// (`<!-- ... -->`). Shortcode tokens whose offset falls inside any such range
/// are ignored by `tokenize`.
fn comment_ranges(src: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        if &bytes[i..i + 4] == b"<!--" {
            let start = i;
            let mut j = i + 4;
            let mut end = bytes.len();
            while j + 3 <= bytes.len() {
                if &bytes[j..j + 3] == b"-->" {
                    end = j + 3;
                    break;
                }
                j += 1;
            }
            out.push((start, end));
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

fn in_any_range(offset: usize, ranges: &[(usize, usize)]) -> bool {
    ranges.iter().any(|(s, e)| offset >= *s && offset < *e)
}

/// Tokenize source into interleaved Text and shortcode tokens.
/// A shortcode is treated as paired iff its name appears in `paired_names`
/// and a matching close tag exists later. Otherwise it is self-closing.
pub fn tokenize<'a>(src: &'a str, paired_names: &[&str]) -> Result<Vec<Token<'a>>, ContentError> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    let comments = comment_ranges(src);

    for cap in OPEN_RE.captures_iter(src) {
        let m_all = cap.get(0).unwrap();
        let name = cap.get(1).unwrap().as_str();
        let raw_args = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        let offset = m_all.start();

        if offset < cursor {
            continue;
        }
        if in_any_range(offset, &comments) {
            continue;
        }
        if offset > cursor {
            out.push(Token::Text(&src[cursor..offset]));
        }

        if paired_names.contains(&name) {
            let close_re = Regex::new(&CLOSE_RE_FMT.replace("{name}", name)).unwrap();
            let after = m_all.end();
            if let Some(close) = close_re.find_at(src, after) {
                let body = &src[after..close.start()];
                out.push(Token::Paired {
                    name,
                    raw_args,
                    body,
                    offset,
                });
                cursor = close.end();
                continue;
            } else {
                return Err(ContentError::UnterminatedShortcode {
                    name: name.into(),
                    offset,
                });
            }
        }

        out.push(Token::Self_ {
            name,
            raw_args,
            offset,
        });
        cursor = m_all.end();
    }

    if cursor < src.len() {
        out.push(Token::Text(&src[cursor..]));
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_shortcodes() {
        let toks = tokenize("just text", &[]).unwrap();
        assert_eq!(toks, vec![Token::Text("just text")]);
    }

    #[test]
    fn self_closing() {
        let src = "a {{< chart src=\"x.json\" >}} b";
        let toks = tokenize(src, &["callout"]).unwrap();
        assert_eq!(toks.len(), 3);
        assert!(matches!(toks[1], Token::Self_ { name: "chart", .. }));
    }

    #[test]
    fn paired() {
        let src = "{{< callout type=\"warn\" >}}hello{{< /callout >}}";
        let toks = tokenize(src, &["callout"]).unwrap();
        assert_eq!(toks.len(), 1);
        match &toks[0] {
            Token::Paired {
                name,
                body,
                raw_args,
                ..
            } => {
                assert_eq!(*name, "callout");
                assert_eq!(*body, "hello");
                assert_eq!(*raw_args, "type=\"warn\"");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn unterminated_paired_errors() {
        let src = "{{< callout type=\"warn\" >}}oops";
        let err = tokenize(src, &["callout"]).unwrap_err();
        assert!(matches!(err, ContentError::UnterminatedShortcode { .. }));
    }

    #[test]
    fn ignores_shortcode_tokens_inside_html_comments() {
        let src = "before <!-- mention of {{< chart >}} --> after";
        let toks = tokenize(src, &[]).unwrap();
        assert_eq!(toks.len(), 1);
        assert!(matches!(&toks[0], Token::Text(_)));
    }
}
