use std::env;

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
        })
    }
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
}
