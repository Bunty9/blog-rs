//! Session token minting + cookie serialisation. The DB layer stores the
//! resulting strings; cookies carry them between server and client.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use cookie::{Cookie, SameSite};
use rand::RngCore;
use time::Duration;

pub const SESSION_COOKIE: &str = "__Host-sid";
pub const CSRF_COOKIE: &str = "XSRF-TOKEN";

/// Mint a 256-bit URL-safe-base64 opaque token. Used for both session and CSRF.
pub fn mint_token() -> String {
    let mut buf = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

/// Build a `__Host-sid` cookie. `Secure`, `HttpOnly`, `SameSite=Lax`, root path.
pub fn session_cookie(token: &str, lifetime: Duration) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, token.to_string()))
        .path("/")
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .max_age(lifetime)
        .build()
}

/// Build the CSRF cookie. Readable by JS so HTMX can echo it as a header.
pub fn csrf_cookie(token: &str, lifetime: Duration) -> Cookie<'static> {
    Cookie::build((CSRF_COOKIE, token.to_string()))
        .path("/")
        .http_only(false)
        .secure(true)
        .same_site(SameSite::Lax)
        .max_age(lifetime)
        .build()
}

/// Build an expired version of either cookie, for logout.
pub fn expire_cookie(name: &'static str) -> Cookie<'static> {
    Cookie::build((name, String::new()))
        .path("/")
        .max_age(Duration::seconds(0))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mint_yields_unique_tokens() {
        let a = mint_token();
        let b = mint_token();
        assert_ne!(a, b);
        // 32 bytes → 43 base64-url-no-pad chars
        assert_eq!(a.len(), 43);
    }

    #[test]
    fn session_cookie_attrs() {
        let c = session_cookie("abc", Duration::hours(1));
        assert!(c.http_only().unwrap_or(false));
        assert!(c.secure().unwrap_or(false));
        assert_eq!(c.name(), SESSION_COOKIE);
        assert_eq!(c.same_site(), Some(SameSite::Lax));
    }

    #[test]
    fn csrf_cookie_is_not_httponly() {
        let c = csrf_cookie("abc", Duration::hours(1));
        assert!(!c.http_only().unwrap_or(true));
    }

    #[test]
    fn expire_cookie_max_age_zero() {
        let c = expire_cookie(SESSION_COOKIE);
        assert_eq!(c.max_age(), Some(Duration::seconds(0)));
    }
}
