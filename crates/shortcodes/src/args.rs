use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShortcodeArgs {
    map: HashMap<String, String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ArgsError {
    #[error("missing required arg `{0}`")]
    Missing(String),
    #[error("arg `{0}` failed to parse as {1}")]
    Parse(String, &'static str),
    #[error("malformed args string at byte {0}: {1}")]
    Malformed(usize, String),
}

impl ShortcodeArgs {
    pub fn new() -> Self { Self::default() }

    pub fn insert(&mut self, k: impl Into<String>, v: impl Into<String>) {
        self.map.insert(k.into(), v.into());
    }

    pub fn get(&self, k: &str) -> Option<&str> {
        self.map.get(k).map(String::as_str)
    }

    pub fn required(&self, k: &str) -> Result<&str, ArgsError> {
        self.get(k).ok_or_else(|| ArgsError::Missing(k.into()))
    }

    pub fn optional<T: std::str::FromStr>(&self, k: &str, ty: &'static str) -> Result<Option<T>, ArgsError> {
        match self.get(k) {
            None => Ok(None),
            Some(s) => s.parse::<T>().map(Some).map_err(|_| ArgsError::Parse(k.into(), ty)),
        }
    }

    pub fn bool(&self, k: &str) -> bool {
        matches!(self.get(k), Some("true") | Some("1") | Some("yes"))
    }
}

/// Parse `key="value" key2="value with spaces" flag=true` style args.
pub fn parse(input: &str) -> Result<ShortcodeArgs, ArgsError> {
    let mut out = ShortcodeArgs::new();
    let bytes = input.as_bytes();
    let mut i = 0usize;
    let len = bytes.len();

    while i < len {
        while i < len && bytes[i].is_ascii_whitespace() { i += 1; }
        if i >= len { break; }

        let key_start = i;
        while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'-') {
            i += 1;
        }
        if key_start == i {
            return Err(ArgsError::Malformed(i, "expected key".into()));
        }
        let key = std::str::from_utf8(&bytes[key_start..i])
            .map_err(|_| ArgsError::Malformed(i, "non-utf8 key".into()))?
            .to_string();

        if i >= len || bytes[i] != b'=' {
            return Err(ArgsError::Malformed(i, format!("expected `=` after `{}`", key)));
        }
        i += 1;

        if i >= len {
            return Err(ArgsError::Malformed(i, "expected value".into()));
        }
        let value = match bytes[i] {
            b'"' | b'\'' => {
                let quote = bytes[i];
                i += 1;
                let vstart = i;
                while i < len && bytes[i] != quote { i += 1; }
                if i >= len {
                    return Err(ArgsError::Malformed(i, "unterminated quoted value".into()));
                }
                let v = std::str::from_utf8(&bytes[vstart..i])
                    .map_err(|_| ArgsError::Malformed(i, "non-utf8 value".into()))?
                    .to_string();
                i += 1;
                v
            }
            _ => {
                let vstart = i;
                while i < len && !bytes[i].is_ascii_whitespace() { i += 1; }
                std::str::from_utf8(&bytes[vstart..i])
                    .map_err(|_| ArgsError::Malformed(i, "non-utf8 value".into()))?
                    .to_string()
            }
        };

        out.insert(key, value);
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_double_quoted() {
        let a = parse(r#"src="data/x.json" type="line""#).unwrap();
        assert_eq!(a.get("src"), Some("data/x.json"));
        assert_eq!(a.get("type"), Some("line"));
    }

    #[test]
    fn parses_bare_and_bool() {
        let a = parse("lang=rust playground=true").unwrap();
        assert_eq!(a.get("lang"), Some("rust"));
        assert!(a.bool("playground"));
    }

    #[test]
    fn rejects_unterminated_quote() {
        assert!(matches!(parse(r#"src="oops"#), Err(ArgsError::Malformed(_, _))));
    }

    #[test]
    fn rejects_missing_equals() {
        assert!(matches!(parse("lang rust"), Err(ArgsError::Malformed(_, _))));
    }
}
