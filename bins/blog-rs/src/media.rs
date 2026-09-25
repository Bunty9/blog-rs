//! Media upload helpers: content-type sniffing (magic bytes only — never
//! trust the client's declared type or filename extension), hashing, and the
//! strict filename format used both to name stored files and to validate
//! `/media/:filename` requests against path traversal.

use sha2::{Digest, Sha256};

/// Sniff PNG / JPEG / GIF / WebP from magic bytes. Returns
/// `(content_type, extension)`. Everything else (including SVG, which is
/// script-capable and deliberately not supported) is `None`.
pub fn sniff(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(("image/png", "png"))
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some(("image/jpeg", "jpg"))
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(("image/gif", "gif"))
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(("image/webp", "webp"))
    } else {
        None
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Best-effort width/height from the file's own header. Only PNG and GIF are
/// trivial to parse without an image-decoding crate; everything else is left
/// NULL (the columns are nullable for exactly this reason).
pub fn sniff_dims(content_type: &str, bytes: &[u8]) -> (Option<i64>, Option<i64>) {
    match content_type {
        "image/png" if bytes.len() >= 24 => {
            let w = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
            let h = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
            (Some(w as i64), Some(h as i64))
        }
        "image/gif" if bytes.len() >= 10 => {
            let w = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
            let h = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
            (Some(w as i64), Some(h as i64))
        }
        _ => (None, None),
    }
}

/// Validate a `/media/:filename` path segment strictly: 16 lowercase-hex
/// chars, a dot, and one of the four allowed extensions. No regex needed —
/// this alone rules out `..`, `/`, and anything else path-traversal-shaped.
/// Returns the content type to serve it with.
pub fn valid_filename(name: &str) -> Option<&'static str> {
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.len() != 16
        || !stem
            .bytes()
            .all(|b| b.is_ascii_digit() || (b.is_ascii_lowercase() && b.is_ascii_hexdigit()))
    {
        return None;
    }
    match ext {
        "png" => Some("image/png"),
        "jpg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, // signature
        0x00, 0x00, 0x00, 0x0d, b'I', b'H', b'D', b'R', // IHDR length+type
        0x00, 0x00, 0x00, 0x01, // width = 1
        0x00, 0x00, 0x00, 0x02, // height = 2
    ];

    #[test]
    fn sniff_png() {
        assert_eq!(sniff(TINY_PNG), Some(("image/png", "png")));
        assert_eq!(sniff_dims("image/png", TINY_PNG), (Some(1), Some(2)));
    }

    #[test]
    fn sniff_jpeg() {
        assert_eq!(sniff(&[0xff, 0xd8, 0xff, 0xe0]), Some(("image/jpeg", "jpg")));
    }

    #[test]
    fn sniff_gif() {
        let mut gif = b"GIF89a".to_vec();
        gif.extend_from_slice(&3u16.to_le_bytes()); // width
        gif.extend_from_slice(&4u16.to_le_bytes()); // height
        assert_eq!(sniff(&gif), Some(("image/gif", "gif")));
        assert_eq!(sniff_dims("image/gif", &gif), (Some(3), Some(4)));
    }

    #[test]
    fn sniff_webp() {
        let mut webp = b"RIFF".to_vec();
        webp.extend_from_slice(&[0, 0, 0, 0]);
        webp.extend_from_slice(b"WEBP");
        assert_eq!(sniff(&webp), Some(("image/webp", "webp")));
    }

    #[test]
    fn sniff_rejects_svg_and_text() {
        assert_eq!(sniff(b"<svg xmlns='...'></svg>"), None);
        assert_eq!(sniff(b"just some text"), None);
    }

    #[test]
    fn filename_validation() {
        assert_eq!(
            valid_filename("0123456789abcdef.png"),
            Some("image/png")
        );
        assert_eq!(valid_filename("0123456789abcdef.svg"), None); // bad ext
        assert_eq!(valid_filename("../../etc/passwd.png"), None); // too long stem, bad chars
        assert_eq!(valid_filename("0123456789ABCDEF.png"), None); // uppercase hex
        assert_eq!(valid_filename("short.png"), None);
        assert_eq!(valid_filename("noextension"), None);
    }
}
