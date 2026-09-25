//! Tiny in-memory fixed-window rate limiter for brute-force / spam
//! throttling on login and signup.
//!
// ponytail: per-process, in-memory, resets on restart and isn't shared
// across replicas. Fine for a single self-hosted instance; move to a shared
// store (e.g. a sqlite table) if this ever runs with more than one process.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::http::HeaderMap;

/// Once the map holds more distinct keys than this, prune expired entries on
/// the next `check()` so a spray of random keys (spoofed IPs, throwaway
/// emails) can't grow memory without bound.
const PRUNE_THRESHOLD: usize = 10_000;

pub const LOGIN_MAX_ATTEMPTS: u32 = 10;
pub const LOGIN_WINDOW: Duration = Duration::from_secs(15 * 60);
pub const SIGNUP_MAX_ATTEMPTS: u32 = 5;
pub const SIGNUP_WINDOW: Duration = Duration::from_secs(60 * 60);

pub struct RateLimiter {
    max: u32,
    window: Duration,
    hits: Mutex<HashMap<String, (Instant, u32)>>,
}

impl RateLimiter {
    pub fn new(max: u32, window: Duration) -> Self {
        Self {
            max,
            window,
            hits: Mutex::new(HashMap::new()),
        }
    }

    pub fn window_secs(&self) -> u64 {
        self.window.as_secs()
    }

    /// Records a hit for `key` and returns whether it's still allowed under
    /// the limit for the current fixed window. Never held across `.await`.
    pub fn check(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut hits = self.hits.lock().unwrap();

        if hits.len() > PRUNE_THRESHOLD {
            hits.retain(|_, (start, _)| now.duration_since(*start) < self.window);
        }

        match hits.get_mut(key) {
            Some((start, count)) if now.duration_since(*start) < self.window => {
                if *count >= self.max {
                    false
                } else {
                    *count += 1;
                    true
                }
            }
            _ => {
                hits.insert(key.to_string(), (now, 1));
                true
            }
        }
    }

    /// Forgets `key`, e.g. clearing the email key on a successful login so a
    /// legitimate user isn't punished for earlier typos.
    pub fn reset(&self, key: &str) {
        self.hits.lock().unwrap().remove(key);
    }
}

/// Resolve the client IP for rate-limiting: `CF-Connecting-IP` (production
/// runs behind a Cloudflare Tunnel), else the first hop of
/// `X-Forwarded-For`, else the socket peer address if available, else
/// "unknown". Headers are spoofable when not behind a trusted proxy, which is
/// why login also keys on the submitted email.
pub fn client_ip(headers: &HeaderMap, connect_info: Option<SocketAddr>) -> String {
    if let Some(v) = header_str(headers, "cf-connecting-ip") {
        return v;
    }
    if let Some(v) = header_str(headers, "x-forwarded-for") {
        if let Some(first) = v.split(',').next() {
            let first = first.trim();
            if !first.is_empty() {
                return first.to_string();
            }
        }
    }
    if let Some(addr) = connect_info {
        return addr.ip().to_string();
    }
    "unknown".to_string()
}

fn header_str(headers: &HeaderMap, name: &str) -> Option<String> {
    let v = headers.get(name)?.to_str().ok()?.trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn allows_up_to_max() {
        let rl = RateLimiter::new(3, Duration::from_secs(60));
        assert!(rl.check("a"));
        assert!(rl.check("a"));
        assert!(rl.check("a"));
    }

    #[test]
    fn blocks_max_plus_one() {
        let rl = RateLimiter::new(3, Duration::from_secs(60));
        assert!(rl.check("a"));
        assert!(rl.check("a"));
        assert!(rl.check("a"));
        assert!(!rl.check("a"));
    }

    #[test]
    fn keys_are_independent() {
        let rl = RateLimiter::new(1, Duration::from_secs(60));
        assert!(rl.check("a"));
        assert!(rl.check("b"));
        assert!(!rl.check("a"));
    }

    #[test]
    fn resets_after_window_elapses() {
        let rl = RateLimiter::new(1, Duration::from_millis(50));
        assert!(rl.check("a"));
        assert!(!rl.check("a"));
        sleep(Duration::from_millis(80));
        assert!(rl.check("a"));
    }

    #[test]
    fn reset_forgets_key_immediately() {
        let rl = RateLimiter::new(1, Duration::from_secs(60));
        assert!(rl.check("a"));
        assert!(!rl.check("a"));
        rl.reset("a");
        assert!(rl.check("a"));
    }

    #[test]
    fn client_ip_prefers_cf_connecting_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("cf-connecting-ip", "1.2.3.4".parse().unwrap());
        headers.insert("x-forwarded-for", "9.9.9.9, 1.1.1.1".parse().unwrap());
        assert_eq!(client_ip(&headers, None), "1.2.3.4");
    }

    #[test]
    fn client_ip_falls_back_to_forwarded_for_first_hop() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "9.9.9.9, 1.1.1.1".parse().unwrap());
        assert_eq!(client_ip(&headers, None), "9.9.9.9");
    }

    #[test]
    fn client_ip_falls_back_to_connect_info() {
        let headers = HeaderMap::new();
        let addr: SocketAddr = "127.0.0.1:1234".parse().unwrap();
        assert_eq!(client_ip(&headers, Some(addr)), "127.0.0.1");
    }

    #[test]
    fn client_ip_falls_back_to_unknown() {
        let headers = HeaderMap::new();
        assert_eq!(client_ip(&headers, None), "unknown");
    }
}
