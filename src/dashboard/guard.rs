use std::net::{IpAddr, Ipv4Addr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine;
use sha2::{Digest, Sha256};

/// Blocks inline styles and scripts. Sign-in is a normal link, so `form-action`
/// stays `'self'` and does not need to allow discord.com.
pub const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' https://cdn.discordapp.com; font-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'";

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

#[derive(Clone, Copy)]
pub struct CsrfSource<'a> {
    pub origin: Option<&'a str>,
    pub sec_fetch_site: Option<&'a str>,
    pub public_url: &'a str,
    pub host: Option<&'a str>,
    pub forwarded_host: Option<&'a str>,
    pub forwarded_proto: Option<&'a str>,
    pub trust_cloudflare: bool,
}

/// Whether this POST was sent by the site itself.
///
/// A browser form POST always includes `Origin` and `Sec-Fetch-Site`. The
/// second one is not a header a page can forge: `same-origin` means the form
/// was loaded from this host, even if `Origin` is not a byte-for-byte match
/// of `DASHBOARD_PUBLIC_URL` (case, `:443`, or a proxy rewrite). `cross-site`
/// and `same-site` are rejected. When the browser sends neither header, the
/// session token is still required by the caller.
pub fn csrf_source_ok(source: CsrfSource<'_>) -> bool {
    if let Some(site) = source
        .sec_fetch_site
        .map(str::trim)
        .filter(|site| !site.is_empty())
    {
        if site.eq_ignore_ascii_case("same-origin") {
            return true;
        }
        if !site.eq_ignore_ascii_case("none") {
            return false;
        }
    }
    let Some(origin) = source.origin.map(str::trim).filter(|origin| !origin.is_empty()) else {
        return true;
    };
    expected_origins(&source)
        .iter()
        .any(|expected| origins_equal(origin, expected))
}

fn expected_origins(source: &CsrfSource<'_>) -> Vec<String> {
    let mut allowed = vec![source.public_url.to_string()];
    let scheme = external_scheme(source);
    if let Some(host) = first_host(source.host) {
        allowed.push(format!("{scheme}://{host}"));
    }
    if source.trust_cloudflare {
        if let Some(host) = first_host(source.forwarded_host) {
            allowed.push(format!("{scheme}://{host}"));
        }
    }
    allowed
}

fn external_scheme(source: &CsrfSource<'_>) -> &'static str {
    if source.trust_cloudflare {
        if let Some(proto) = first_host(source.forwarded_proto) {
            if proto.eq_ignore_ascii_case("https") {
                return "https";
            }
            if proto.eq_ignore_ascii_case("http") {
                return "http";
            }
        }
    }
    if source.public_url.starts_with("http://") {
        "http"
    } else {
        "https"
    }
}

fn first_host(value: Option<&str>) -> Option<&str> {
    let value = value?.split(',').next()?.trim();
    if value.is_empty() || value.contains([' ', '/', '?', '#']) {
        None
    } else {
        Some(value)
    }
}

fn origins_equal(left: &str, right: &str) -> bool {
    match (canonical_origin(left), canonical_origin(right)) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

fn canonical_origin(raw: &str) -> Option<(String, String)> {
    let raw = raw.trim().trim_end_matches('/');
    let (scheme, rest) = raw.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let rest = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let (host, port) = split_host_port(rest)?;
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    let port = match (scheme.as_str(), port) {
        ("https", "443") | ("http", "80") | (_, "") => String::new(),
        (_, port) => port.to_string(),
    };
    let host = if port.is_empty() {
        host
    } else {
        format!("{host}:{port}")
    };
    Some((scheme, host))
}

fn split_host_port(rest: &str) -> Option<(&str, &str)> {
    if let Some(host) = rest.strip_prefix('[') {
        let (host, port) = host.split_once(']')?;
        let port = port.strip_prefix(':').unwrap_or("");
        return Some((host, port));
    }
    match rest.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.chars().all(|ch| ch.is_ascii_digit()) => {
            Some((host, port))
        }
        _ => Some((rest, "")),
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

    fn source<'a>(origin: Option<&'a str>, site: Option<&'a str>) -> CsrfSource<'a> {
        CsrfSource {
            origin,
            sec_fetch_site: site,
            public_url: "https://overlord.example.com",
            host: Some("overlord.example.com"),
            forwarded_host: None,
            forwarded_proto: None,
            trust_cloudflare: false,
        }
    }

    #[test]
    fn same_origin_form_is_accepted_and_a_foreign_one_is_not() {
        let public = "https://overlord.example.com";
        assert!(csrf_source_ok(source(None, None)));
        assert!(csrf_source_ok(source(Some(public), None)));
        assert!(csrf_source_ok(source(
            Some("https://OVERLORD.EXAMPLE.COM:443"),
            None
        )));
        assert!(csrf_source_ok(source(
            Some("https://overlord.example.com/servers/1/verification"),
            None
        )));
        assert!(csrf_source_ok(source(
            Some("https://evil.example"),
            Some("same-origin")
        )));
        assert!(!csrf_source_ok(source(Some("https://evil.example"), None)));
        assert!(!csrf_source_ok(source(
            Some(public),
            Some("cross-site")
        )));
        assert!(!csrf_source_ok(source(Some(public), Some("same-site"))));
        assert!(!csrf_source_ok(source(Some("http://overlord.example.com"), None)));
        let mut forwarded = source(Some("https://dash.example"), None);
        forwarded.public_url = "https://overlord.example.com";
        forwarded.host = Some("127.0.0.1:8080");
        forwarded.forwarded_host = Some("dash.example");
        forwarded.forwarded_proto = Some("https");
        forwarded.trust_cloudflare = false;
        assert!(!csrf_source_ok(forwarded));
        forwarded.trust_cloudflare = true;
        assert!(csrf_source_ok(forwarded));
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
