use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub discord_token: String,
    pub guild_id: Option<u64>,
    pub database_url: String,

    // Channel IDs
    pub honeypot_channel_id: Option<u64>,
    pub verify_channel_id: Option<u64>,
    pub mod_log_channel_id: Option<u64>,
    pub transcript_channel_id: Option<u64>,
    pub join_to_create_channel_id: Option<u64>,
    pub voice_category_id: Option<u64>,
    pub ticket_category_id: Option<u64>,

    // Role IDs
    pub unverified_role_id: Option<u64>,
    pub member_role_id: Option<u64>,
    pub mod_role_id: Option<u64>,

    // Thresholds
    pub min_account_age_days: i64,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();

        let discord_token = env::var("DISCORD_TOKEN")
            .expect("DISCORD_TOKEN environment variable must be set in .env");

        let guild_id = env::var("GUILD_ID").ok().and_then(|v| v.parse().ok());
        let database_url = env::var("DATABASE_URL")
            .unwrap_or_else(|_| "sqlite://overlord.db?mode=rwc".to_string());

        let honeypot_channel_id = env::var("HONEYPOT_CHANNEL_ID").ok().and_then(|v| v.parse().ok());
        let verify_channel_id = env::var("VERIFY_CHANNEL_ID").ok().and_then(|v| v.parse().ok());
        let mod_log_channel_id = env::var("MOD_LOG_CHANNEL_ID").ok().and_then(|v| v.parse().ok());
        let transcript_channel_id = env::var("TRANSCRIPT_CHANNEL_ID").ok().and_then(|v| v.parse().ok());
        let join_to_create_channel_id = env::var("JOIN_TO_CREATE_CHANNEL_ID").ok().and_then(|v| v.parse().ok());
        let voice_category_id = env::var("VOICE_CATEGORY_ID").ok().and_then(|v| v.parse().ok());
        let ticket_category_id = env::var("TICKET_CATEGORY_ID").ok().and_then(|v| v.parse().ok());

        let unverified_role_id = env::var("UNVERIFIED_ROLE_ID").ok().and_then(|v| v.parse().ok());
        let member_role_id = env::var("MEMBER_ROLE_ID").ok().and_then(|v| v.parse().ok());
        let mod_role_id = env::var("MOD_ROLE_ID").ok().and_then(|v| v.parse().ok());

        let min_account_age_days = env::var("MIN_ACCOUNT_AGE_DAYS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3);

        Ok(Self {
            discord_token,
            guild_id,
            database_url,
            honeypot_channel_id,
            verify_channel_id,
            mod_log_channel_id,
            transcript_channel_id,
            join_to_create_channel_id,
            voice_category_id,
            ticket_category_id,
            unverified_role_id,
            member_role_id,
            mod_role_id,
            min_account_age_days,
        })
    }
}

