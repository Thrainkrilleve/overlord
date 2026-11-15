use std::env;
use std::net::SocketAddr;

/// Channel, role, and policy values read from the environment.
///
/// These are copied once into the `GUILD_ID` server's database row. They are not read again
/// while handling events, so a second server never inherits them.
#[derive(Clone, Debug)]
pub struct EnvSeed {
    pub honeypot_channel_id: Option<u64>,
    pub verify_channel_id: Option<u64>,
    pub mod_log_channel_id: Option<u64>,
    pub transcript_channel_id: Option<u64>,
    pub join_to_create_channel_id: Option<u64>,
    pub voice_category_id: Option<u64>,
    pub ticket_category_id: Option<u64>,
    pub unverified_role_id: Option<u64>,
    pub member_role_id: Option<u64>,
    pub mod_role_id: Option<u64>,
    pub min_account_age_days: i64,
}

impl Default for EnvSeed {
    fn default() -> Self {
        Self {
            honeypot_channel_id: None,
            verify_channel_id: None,
            mod_log_channel_id: None,
            transcript_channel_id: None,
            join_to_create_channel_id: None,
            voice_category_id: None,
            ticket_category_id: None,
            unverified_role_id: None,
            member_role_id: None,
            mod_role_id: None,
            min_account_age_days: 3,
        }
    }
}

/// Process-wide settings. Everything that differs per server lives in `guild_settings`.
#[derive(Clone, Debug)]
pub struct Config {
    pub discord_token: String,
    pub database_url: String,
    /// Server whose `.env` channel and role IDs are copied into the database on first start.
    pub guild_id: Option<u64>,
    /// When set, slash commands are registered only in `guild_id` (local development).
    pub dev_guild_only: bool,
    pub seed: EnvSeed,
    /// None when `DASHBOARD_ENABLED` is off. The bot still runs.
    pub dashboard: Option<DashboardConfig>,
}

/// Web dashboard settings. The client secret is redacted in `Debug`.
#[derive(Clone)]
pub struct DashboardConfig {
    pub bind: SocketAddr,
    pub public_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_url: String,
    pub cookie_secure: bool,
    pub trust_cloudflare: bool,
    /// Bitfield sent as `permissions` on the bot invite URL.
    pub invite_permissions: u64,
}

impl std::fmt::Debug for DashboardConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DashboardConfig")
            .field("bind", &self.bind)
            .field("public_url", &self.public_url)
            .field("client_id", &self.client_id)
            .field("client_secret", &"redacted")
            .field("redirect_url", &self.redirect_url)
            .field("cookie_secure", &self.cookie_secure)
            .field("trust_cloudflare", &self.trust_cloudflare)
            .field("invite_permissions", &self.invite_permissions)
            .finish()
    }
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();

        let discord_token = env::var("DISCORD_TOKEN")
            .expect("DISCORD_TOKEN environment variable must be set in .env");

        let guild_id = parse_optional_u64(env::var("GUILD_ID").ok());
        let database_url = env::var("DATABASE_URL")
            .unwrap_or_else(|_| "sqlite://overlord.db?mode=rwc".to_string());
        let dev_guild_only = parse_env_flag(env::var("DEV_GUILD_ONLY").ok().as_deref());

        let seed = EnvSeed {
            honeypot_channel_id: parse_optional_u64(env::var("HONEYPOT_CHANNEL_ID").ok()),
            verify_channel_id: parse_optional_u64(env::var("VERIFY_CHANNEL_ID").ok()),
            mod_log_channel_id: parse_optional_u64(env::var("MOD_LOG_CHANNEL_ID").ok()),
            transcript_channel_id: parse_optional_u64(env::var("TRANSCRIPT_CHANNEL_ID").ok()),
            join_to_create_channel_id: parse_optional_u64(
                env::var("JOIN_TO_CREATE_CHANNEL_ID").ok(),
            ),
            voice_category_id: parse_optional_u64(env::var("VOICE_CATEGORY_ID").ok()),
            ticket_category_id: parse_optional_u64(env::var("TICKET_CATEGORY_ID").ok()),
            unverified_role_id: parse_optional_u64(env::var("UNVERIFIED_ROLE_ID").ok()),
            member_role_id: parse_optional_u64(env::var("MEMBER_ROLE_ID").ok()),
            mod_role_id: parse_optional_u64(env::var("MOD_ROLE_ID").ok()),
            min_account_age_days: env::var("MIN_ACCOUNT_AGE_DAYS")
                .ok()
                .and_then(|v| v.trim().parse::<i64>().ok())
                .filter(|days| (0..=3650).contains(days))
                .unwrap_or(3),
        };

        Ok(Self {
            discord_token,
            database_url,
            guild_id,
            dev_guild_only,
            seed,
            dashboard: dashboard_from_parts(
                parse_env_flag(env::var("DASHBOARD_ENABLED").ok().as_deref()),
                env::var("DASHBOARD_BIND").ok().as_deref(),
                env::var("DASHBOARD_PUBLIC_URL").ok().as_deref(),
                env::var("DASHBOARD_DISCORD_CLIENT_ID").ok().as_deref(),
                env::var("DASHBOARD_DISCORD_CLIENT_SECRET").ok().as_deref(),
                env::var("DASHBOARD_OAUTH_REDIRECT_URL").ok().as_deref(),
                env::var("DASHBOARD_COOKIE_SECURE").ok().as_deref(),
                parse_env_flag(env::var("DASHBOARD_TRUST_CLOUDFLARE").ok().as_deref()),
                env::var("DASHBOARD_INVITE_PERMISSIONS").ok().as_deref(),
            )?,
        })
    }
}

#[allow(clippy::too_many_arguments)]
pub fn dashboard_from_parts(
    enabled: bool,
    bind: Option<&str>,
    public_url: Option<&str>,
    client_id: Option<&str>,
    client_secret: Option<&str>,
    redirect_url: Option<&str>,
    cookie_secure: Option<&str>,
    trust_cloudflare: bool,
    invite_permissions: Option<&str>,
) -> anyhow::Result<Option<DashboardConfig>> {
    if !enabled {
        return Ok(None);
    }
    let public_url = normalize_origin(public_url.unwrap_or(""))?;
    let client_id = client_id.unwrap_or("").trim().to_string();
    if client_id.is_empty() || !client_id.chars().all(|c| c.is_ascii_digit()) {
        anyhow::bail!("DASHBOARD_DISCORD_CLIENT_ID must be the numeric Discord application id");
    }
    let client_secret = client_secret.unwrap_or("").trim().to_string();
    if client_secret.is_empty() {
        anyhow::bail!("DASHBOARD_DISCORD_CLIENT_SECRET is required when the dashboard is enabled");
    }
    let redirect_url = redirect_url
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_end_matches('/').to_string())
        .unwrap_or_else(|| format!("{public_url}/auth/callback"));
    if !redirect_url.starts_with(&public_url) {
        anyhow::bail!("DASHBOARD_OAUTH_REDIRECT_URL must start with DASHBOARD_PUBLIC_URL");
    }
    let cookie_secure = match cookie_secure
        .map(|s| s.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("1") | Some("true") | Some("yes") | Some("on") => true,
        Some("0") | Some("false") | Some("no") | Some("off") => false,
        Some(_) => anyhow::bail!("DASHBOARD_COOKIE_SECURE must be true or false"),
        None => public_url.starts_with("https://"),
    };
    if cookie_secure && public_url.starts_with("http://") {
        anyhow::bail!(
            "DASHBOARD_PUBLIC_URL is http but cookies are marked Secure. Set DASHBOARD_COOKIE_SECURE=false only for local testing."
        );
    }
    let bind = bind
        .unwrap_or("0.0.0.0:8080")
        .trim()
        .parse::<SocketAddr>()
        .map_err(|_| anyhow::anyhow!("DASHBOARD_BIND must be an address like 0.0.0.0:8080"))?;
    let invite_permissions = match invite_permissions
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        None => 1_099_798_014_998,
        Some(value) => value.parse::<u64>().map_err(|_| {
            anyhow::anyhow!("DASHBOARD_INVITE_PERMISSIONS must be a non-negative integer")
        })?,
    };
    Ok(Some(DashboardConfig {
        bind,
        public_url,
        client_id,
        client_secret,
        redirect_url,
        cookie_secure,
        trust_cloudflare,
        invite_permissions,
    }))
}

fn normalize_origin(raw: &str) -> anyhow::Result<String> {
    let raw = raw.trim().trim_end_matches('/');
    let https = raw.starts_with("https://");
    let http = raw.starts_with("http://");
    if (!https && !http) || raw.contains('@') || raw.chars().any(|c| c.is_whitespace()) {
        anyhow::bail!("DASHBOARD_PUBLIC_URL must be an origin like https://overlord.example.com");
    }
    let rest = raw.split_once("://").map(|(_, rest)| rest).unwrap_or("");
    if rest.is_empty() || rest.contains('/') {
        anyhow::bail!("DASHBOARD_PUBLIC_URL must be an origin like https://overlord.example.com");
    }
    Ok(raw.to_string())
}

pub(crate) fn parse_optional_u64(value: Option<String>) -> Option<u64> {
    value.and_then(|v| v.trim().parse().ok())
}

pub(crate) fn parse_env_flag(value: Option<&str>) -> bool {
    matches!(
        value.map(|s| s.trim().to_ascii_lowercase()).as_deref(),
        Some("1") | Some("true") | Some("yes") | Some("on")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_or_invalid_ids_are_absent() {
        assert_eq!(parse_optional_u64(None), None);
        assert_eq!(parse_optional_u64(Some("  ".into())), None);
        assert_eq!(parse_optional_u64(Some("nope".into())), None);
        assert_eq!(parse_optional_u64(Some(" 42 ".into())), Some(42));
    }

    #[test]
    fn env_flag_accepts_common_truthy_spellings() {
        assert!(parse_env_flag(Some("true")));
        assert!(parse_env_flag(Some(" YES ")));
        assert!(parse_env_flag(Some("1")));
        assert!(parse_env_flag(Some("on")));
        assert!(!parse_env_flag(None));
        assert!(!parse_env_flag(Some("false")));
        assert!(!parse_env_flag(Some("0")));
        assert!(!parse_env_flag(Some("")));
    }

    #[test]
    fn dashboard_stays_off_without_the_flag() {
        let cfg =
            dashboard_from_parts(false, None, None, None, None, None, None, false, None).unwrap();
        assert!(cfg.is_none());
    }

    #[test]
    fn dashboard_requires_https_cookies_to_match_the_url() {
        let err = dashboard_from_parts(
            true,
            Some("127.0.0.1:8080"),
            Some("http://overlord.example.com"),
            Some("123"),
            Some("secret"),
            None,
            Some("true"),
            false,
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("Secure"));
    }

    #[test]
    fn dashboard_accepts_a_placeholder_https_origin() {
        let cfg = dashboard_from_parts(
            true,
            Some("0.0.0.0:8080"),
            Some("https://overlord.example.com/"),
            Some("123456789012345678"),
            Some("placeholder-secret"),
            None,
            None,
            true,
            Some("8"),
        )
        .unwrap()
        .unwrap();
        assert_eq!(cfg.public_url, "https://overlord.example.com");
        assert_eq!(cfg.invite_permissions, 8);
        assert_eq!(
            cfg.redirect_url,
            "https://overlord.example.com/auth/callback"
        );
        assert!(cfg.cookie_secure);
        assert!(cfg.trust_cloudflare);
    }
}
