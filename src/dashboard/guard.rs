use std::net::{IpAddr, Ipv4Addr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine;
use sha2::{Digest, Sha256};

pub const ADMINISTRATOR: u64 = 1 << 3;
pub const MANAGE_GUILD: u64 = 1 << 5;

pub fn can_manage_server(owner: bool, permissions: u64) -> bool {
    owner || permissions & ADMINISTRATOR != 0 || permissions & MANAGE_GUILD != 0
}

/// Cloudflare's header is used only when the operator turned that on.
/// Otherwise a client can put any address there and skip the rate limit.
pub fn client_ip(
    peer: Option<IpAddr>,
    cf_connecting_ip: Option<&str>,
    trust_cloudflare: bool,
) -> IpAddr {
    if trust_cloudflare {
        if let Some(ip) = cf_connecting_ip.and_then(|value| value.trim().parse().ok()) {
            return ip;
        }
    }
    peer.unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
}

pub fn ct_eq(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn code_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

pub fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

pub fn origin_allowed(origin: Option<&str>, public_url: &str) -> bool {
    match origin.map(str::trim) {
        None | Some("") => true,
        Some(origin) => origin.trim_end_matches('/') == public_url.trim_end_matches('/'),
    }
}

pub fn cookie_value<'a>(header: Option<&'a str>, name: &str) -> Option<&'a str> {
    let header = header?;
    let prefix = format!("{name}=");
    for part in header.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix(&prefix) {
            if value.is_empty() || value.chars().any(|c| c.is_whitespace() || c == ';') {
                return None;
            }
            return Some(value);
        }
    }
    None
}

pub fn set_cookie(name: &str, value: &str, max_age: i64, secure: bool) -> String {
    let mut cookie = format!("{name}={value}; HttpOnly; SameSite=Lax; Path=/; Max-Age={max_age}");
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

#[derive(Default)]
pub struct RateLimiter {
    hits: Mutex<Vec<(String, Instant)>>,
}

impl RateLimiter {
    pub fn allow(&self, key: &str, limit: usize, window: Duration) -> bool {
        let mut hits = self.hits.lock().unwrap_or_else(|err| err.into_inner());
        let now = Instant::now();
        hits.retain(|(_, at)| now.duration_since(*at) < window);
        let count = hits.iter().filter(|(k, _)| k == key).count();
        if count >= limit {
            return false;
        }
        hits.push((key.to_string(), now));
        if hits.len() > 20_000 {
            hits.clear();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manage_server_requires_owner_admin_or_manage_guild() {
        assert!(can_manage_server(true, 0));
        assert!(can_manage_server(false, ADMINISTRATOR));
        assert!(can_manage_server(false, MANAGE_GUILD));
        assert!(can_manage_server(false, ADMINISTRATOR | MANAGE_GUILD));
        assert!(!can_manage_server(false, 0));
        assert!(!can_manage_server(false, 1 << 1));
    }

    #[test]
    fn cloudflare_ip_is_ignored_unless_configured() {
        let peer_ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 8));
        let peer = Some(peer_ip);
        let spoofed = Some("203.0.113.9");
        assert_eq!(client_ip(peer, spoofed, false), peer_ip);
        assert_eq!(
            client_ip(peer, spoofed, true),
            "203.0.113.9".parse::<IpAddr>().unwrap()
        );
        assert_eq!(client_ip(peer, Some("not-an-ip"), true), peer_ip);
    }

    #[test]
    fn session_cookie_is_httponly_lax_and_optionally_secure() {
        let secure = set_cookie("overlord_session", "abc", 60, true);
        assert!(secure.contains("HttpOnly"));
        assert!(secure.contains("SameSite=Lax"));
        assert!(secure.contains("Secure"));
        let local = set_cookie("overlord_session", "abc", 60, false);
        assert!(!local.contains("Secure"));
    }

    #[test]
    fn pkce_matches_the_rfc_vector() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            code_challenge(verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn csrf_compare_rejects_a_different_token() {
        assert!(ct_eq("abc", "abc"));
        assert!(!ct_eq("abc", "abd"));
        assert!(!ct_eq("abc", "abcd"));
    }

    #[test]
    fn html_escape_covers_markup() {
        assert_eq!(
            esc("<script>\"a&b\"</script>"),
            "&lt;script&gt;&quot;a&amp;b&quot;&lt;/script&gt;"
        );
    }

    #[test]
    fn foreign_origin_is_rejected() {
        assert!(origin_allowed(None, "https://overlord.example.com"));
        assert!(origin_allowed(
            Some("https://overlord.example.com"),
            "https://overlord.example.com"
        ));
        assert!(!origin_allowed(
            Some("https://evil.example"),
            "https://overlord.example.com"
        ));
    }

    #[test]
    fn rate_limit_trips_on_the_third_hit() {
        let limiter = RateLimiter::default();
        assert!(limiter.allow("ip", 2, Duration::from_secs(60)));
        assert!(limiter.allow("ip", 2, Duration::from_secs(60)));
        assert!(!limiter.allow("ip", 2, Duration::from_secs(60)));
        assert!(limiter.allow("other", 2, Duration::from_secs(60)));
    }
}
