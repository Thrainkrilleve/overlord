use crate::config::EnvSeed;
use crate::database::DbPool;

/// One server's Overlord settings. A missing row means every channel and role feature is off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuildSettings {
    pub guild_id: u64,
    /// False when this server has no row yet.
    pub configured: bool,
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
    /// When false, the honeypot channel is ignored even if one is saved.
    pub honeypot_enabled: bool,
    pub raid_join_threshold: i64,
    pub raid_window_seconds: i64,
    /// `alert`, `lockdown`, or `ban`. Anything else is treated as `alert`.
    pub raid_action: String,
    /// Extra staff roles, comma-separated. `mod_role_id` is always included.
    pub staff_role_ids: String,
    pub max_open_tickets: i64,
}

impl GuildSettings {
    pub fn unconfigured(guild_id: u64) -> Self {
        Self {
            guild_id,
            configured: false,
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
            honeypot_enabled: true,
            raid_join_threshold: 5,
            raid_window_seconds: 10,
            raid_action: "alert".to_string(),
            staff_role_ids: String::new(),
            max_open_tickets: 1,
        }
    }

    pub fn honeypot_active(&self) -> bool {
        self.honeypot_enabled && self.honeypot_channel_id.is_some()
    }

    pub fn raid_threshold(&self) -> usize {
        self.raid_join_threshold.clamp(2, 50) as usize
    }

    pub fn raid_window_secs(&self) -> u64 {
        self.raid_window_seconds.clamp(5, 120) as u64
    }

    /// Unknown values stay on the non-destructive action.
    pub fn raid_action_kind(&self) -> &'static str {
        match self.raid_action.as_str() {
            "lockdown" => "lockdown",
            "ban" => "ban",
            _ => "alert",
        }
    }

    pub fn max_tickets(&self) -> i64 {
        self.max_open_tickets.clamp(1, 5)
    }

    pub fn staff_role_list(&self) -> Vec<u64> {
        let mut ids = Vec::new();
        if let Some(id) = self.mod_role_id {
            ids.push(id);
        }
        for part in self
            .staff_role_ids
            .split(|c: char| c == ',' || c.is_whitespace())
        {
            if let Ok(id) = part.parse::<u64>() {
                if id != 0 && !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        ids
    }

    pub fn field_text(&self, field: SettingField) -> String {
        let channel = |id: Option<u64>| {
            id.map(|id| format!("#{id}"))
                .unwrap_or_else(|| "Not set".into())
        };
        let role = |id: Option<u64>| {
            id.map(|id| format!("@{id}"))
                .unwrap_or_else(|| "Not set".into())
        };
        match field {
            SettingField::HoneypotChannel => channel(self.honeypot_channel_id),
            SettingField::VerifyChannel => channel(self.verify_channel_id),
            SettingField::ModLogChannel => channel(self.mod_log_channel_id),
            SettingField::TranscriptChannel => channel(self.transcript_channel_id),
            SettingField::JoinToCreateChannel => channel(self.join_to_create_channel_id),
            SettingField::VoiceCategory => channel(self.voice_category_id),
            SettingField::TicketCategory => channel(self.ticket_category_id),
            SettingField::UnverifiedRole => role(self.unverified_role_id),
            SettingField::MemberRole => role(self.member_role_id),
            SettingField::ModRole => role(self.mod_role_id),
            SettingField::MinAccountAge => format!("{} days", self.min_account_age_days),
            SettingField::HoneypotEnabled => {
                if self.honeypot_enabled {
                    "On".into()
                } else {
                    "Off".into()
                }
            }
            SettingField::RaidJoinThreshold => format!("{} joins", self.raid_threshold()),
            SettingField::RaidWindowSeconds => format!("{} seconds", self.raid_window_secs()),
            SettingField::MaxOpenTickets => format!("{} per user", self.max_tickets()),
        }
    }
}

/// Columns an admin can change. The string is a fixed identifier, never user input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingField {
    HoneypotChannel,
    VerifyChannel,
    ModLogChannel,
    TranscriptChannel,
    JoinToCreateChannel,
    VoiceCategory,
    TicketCategory,
    UnverifiedRole,
    MemberRole,
    ModRole,
    MinAccountAge,
    HoneypotEnabled,
    RaidJoinThreshold,
    RaidWindowSeconds,
    MaxOpenTickets,
}

impl SettingField {
    fn column(self) -> &'static str {
        match self {
            Self::HoneypotChannel => "honeypot_channel_id",
            Self::VerifyChannel => "verify_channel_id",
            Self::ModLogChannel => "mod_log_channel_id",
            Self::TranscriptChannel => "transcript_channel_id",
            Self::JoinToCreateChannel => "join_to_create_channel_id",
            Self::VoiceCategory => "voice_category_id",
            Self::TicketCategory => "ticket_category_id",
            Self::UnverifiedRole => "unverified_role_id",
            Self::MemberRole => "member_role_id",
            Self::ModRole => "mod_role_id",
            Self::MinAccountAge => "min_account_age_days",
            Self::HoneypotEnabled => "honeypot_enabled",
            Self::RaidJoinThreshold => "raid_join_threshold",
            Self::RaidWindowSeconds => "raid_window_seconds",
            Self::MaxOpenTickets => "max_open_tickets",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::HoneypotChannel => "Honeypot channel",
            Self::VerifyChannel => "Verify channel",
            Self::ModLogChannel => "Mod log channel",
            Self::TranscriptChannel => "Transcript channel",
            Self::JoinToCreateChannel => "Join-to-create channel",
            Self::VoiceCategory => "Temp voice category",
            Self::TicketCategory => "Ticket category",
            Self::UnverifiedRole => "Unverified role",
            Self::MemberRole => "Member role",
            Self::ModRole => "Staff role",
            Self::MinAccountAge => "Minimum account age",
            Self::HoneypotEnabled => "Honeypot trap",
            Self::RaidJoinThreshold => "Raid threshold",
            Self::RaidWindowSeconds => "Raid window",
            Self::MaxOpenTickets => "Max open tickets",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextSetting {
    RaidAction,
    StaffRoleIds,
}

impl TextSetting {
    fn column(self) -> &'static str {
        match self {
            Self::RaidAction => "raid_action",
            Self::StaffRoleIds => "staff_role_ids",
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct SettingsRow {
    honeypot_channel_id: Option<i64>,
    verify_channel_id: Option<i64>,
    mod_log_channel_id: Option<i64>,
    transcript_channel_id: Option<i64>,
    join_to_create_channel_id: Option<i64>,
    voice_category_id: Option<i64>,
    ticket_category_id: Option<i64>,
    unverified_role_id: Option<i64>,
    member_role_id: Option<i64>,
    mod_role_id: Option<i64>,
    min_account_age_days: i64,
    honeypot_enabled: i64,
    raid_join_threshold: i64,
    raid_window_seconds: i64,
    raid_action: String,
    staff_role_ids: String,
    max_open_tickets: i64,
}

fn snowflake(value: Option<i64>) -> Option<u64> {
    value.and_then(|value| u64::try_from(value).ok())
}

fn snowflake_i64(value: Option<u64>) -> Option<i64> {
    value.and_then(|value| i64::try_from(value).ok())
}

impl SettingsRow {
    fn into_settings(self, guild_id: u64) -> GuildSettings {
        GuildSettings {
            guild_id,
            configured: true,
            honeypot_channel_id: snowflake(self.honeypot_channel_id),
            verify_channel_id: snowflake(self.verify_channel_id),
            mod_log_channel_id: snowflake(self.mod_log_channel_id),
            transcript_channel_id: snowflake(self.transcript_channel_id),
            join_to_create_channel_id: snowflake(self.join_to_create_channel_id),
            voice_category_id: snowflake(self.voice_category_id),
            ticket_category_id: snowflake(self.ticket_category_id),
            unverified_role_id: snowflake(self.unverified_role_id),
            member_role_id: snowflake(self.member_role_id),
            mod_role_id: snowflake(self.mod_role_id),
            min_account_age_days: self.min_account_age_days,
            honeypot_enabled: self.honeypot_enabled != 0,
            raid_join_threshold: self.raid_join_threshold,
            raid_window_seconds: self.raid_window_seconds,
            raid_action: self.raid_action,
            staff_role_ids: self.staff_role_ids,
            max_open_tickets: self.max_open_tickets,
        }
    }
}

/// Settings for `guild_id` only. An unknown server gets defaults and no channel or role IDs.
pub async fn load(pool: &DbPool, guild_id: u64) -> anyhow::Result<GuildSettings> {
    let row = sqlx::query_as::<_, SettingsRow>(
        r#"
        SELECT honeypot_channel_id, verify_channel_id, mod_log_channel_id, transcript_channel_id,
               join_to_create_channel_id, voice_category_id, ticket_category_id,
               unverified_role_id, member_role_id, mod_role_id, min_account_age_days,
               COALESCE(honeypot_enabled, 1) AS honeypot_enabled,
               COALESCE(raid_join_threshold, 5) AS raid_join_threshold,
               COALESCE(raid_window_seconds, 10) AS raid_window_seconds,
               COALESCE(raid_action, 'alert') AS raid_action,
               COALESCE(staff_role_ids, '') AS staff_role_ids,
               COALESCE(max_open_tickets, 1) AS max_open_tickets
        FROM guild_settings
        WHERE guild_id = ?
        "#,
    )
    .bind(guild_id as i64)
    .fetch_optional(pool)
    .await?;

    Ok(match row {
        Some(row) => row.into_settings(guild_id),
        None => GuildSettings::unconfigured(guild_id),
    })
}

pub async fn set_field(
    pool: &DbPool,
    guild_id: u64,
    field: SettingField,
    value: Option<i64>,
) -> anyhow::Result<()> {
    // `column` is one of the fixed names in `SettingField::column`, not user text.
    let column = field.column();
    let sql = format!(
        "INSERT INTO guild_settings (guild_id, {column}) VALUES (?, ?)
         ON CONFLICT(guild_id) DO UPDATE SET {column} = excluded.{column}, updated_at = datetime('now')"
    );
    sqlx::query(&sql)
        .bind(guild_id as i64)
        .bind(value)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_text(
    pool: &DbPool,
    guild_id: u64,
    field: TextSetting,
    value: &str,
) -> anyhow::Result<()> {
    let column = field.column();
    let sql = format!(
        "INSERT INTO guild_settings (guild_id, {column}) VALUES (?, ?)
         ON CONFLICT(guild_id) DO UPDATE SET {column} = excluded.{column}, updated_at = datetime('now')"
    );
    sqlx::query(&sql)
        .bind(guild_id as i64)
        .bind(value)
        .execute(pool)
        .await?;
    Ok(())
}

/// Insert `.env` values for `guild_id` when that server has no row yet.
/// Returns whether a row was created. An existing row is left untouched.
pub async fn seed_if_absent(pool: &DbPool, guild_id: u64, seed: &EnvSeed) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        INSERT OR IGNORE INTO guild_settings (
            guild_id,
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
            min_account_age_days
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(guild_id as i64)
    .bind(snowflake_i64(seed.honeypot_channel_id))
    .bind(snowflake_i64(seed.verify_channel_id))
    .bind(snowflake_i64(seed.mod_log_channel_id))
    .bind(snowflake_i64(seed.transcript_channel_id))
    .bind(snowflake_i64(seed.join_to_create_channel_id))
    .bind(snowflake_i64(seed.voice_category_id))
    .bind(snowflake_i64(seed.ticket_category_id))
    .bind(snowflake_i64(seed.unverified_role_id))
    .bind(snowflake_i64(seed.member_role_id))
    .bind(snowflake_i64(seed.mod_role_id))
    .bind(seed.min_account_age_days)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn settings_are_isolated_per_guild() {
        let db = crate::database::test_pool("settings-iso").await;
        set_field(&db, 1, SettingField::ModLogChannel, Some(111))
            .await
            .unwrap();
        set_field(&db, 1, SettingField::MinAccountAge, Some(9))
            .await
            .unwrap();
        set_field(&db, 1, SettingField::MemberRole, Some(555))
            .await
            .unwrap();
        set_field(&db, 2, SettingField::ModLogChannel, Some(222))
            .await
            .unwrap();

        let first = load(&db, 1).await.unwrap();
        let second = load(&db, 2).await.unwrap();
        let unknown = load(&db, 3).await.unwrap();

        assert!(first.configured);
        assert_eq!(first.mod_log_channel_id, Some(111));
        assert_eq!(first.member_role_id, Some(555));
        assert_eq!(first.min_account_age_days, 9);
        assert_eq!(first.honeypot_channel_id, None);

        assert_eq!(second.mod_log_channel_id, Some(222));
        assert_eq!(second.member_role_id, None);
        assert_eq!(second.min_account_age_days, 3);

        assert!(!unknown.configured);
        assert_eq!(unknown.mod_log_channel_id, None);
        assert_eq!(unknown.member_role_id, None);
        assert_eq!(unknown.min_account_age_days, 3);

        set_field(&db, 2, SettingField::ModLogChannel, Some(333))
            .await
            .unwrap();
        set_field(&db, 2, SettingField::ModLogChannel, None)
            .await
            .unwrap();
        assert_eq!(load(&db, 1).await.unwrap().mod_log_channel_id, Some(111));
        assert_eq!(load(&db, 2).await.unwrap().mod_log_channel_id, None);
        // Clearing the log channel does not reset the other guild or this guild's age.
        assert_eq!(load(&db, 1).await.unwrap().min_account_age_days, 9);
        assert_eq!(load(&db, 2).await.unwrap().min_account_age_days, 3);
    }

    #[tokio::test]
    async fn clearing_one_field_keeps_the_others() {
        let db = crate::database::test_pool("settings-clear").await;
        set_field(&db, 7, SettingField::TranscriptChannel, Some(10))
            .await
            .unwrap();
        set_field(&db, 7, SettingField::ModRole, Some(20))
            .await
            .unwrap();
        set_field(&db, 7, SettingField::TranscriptChannel, None)
            .await
            .unwrap();

        let settings = load(&db, 7).await.unwrap();
        assert_eq!(settings.transcript_channel_id, None);
        assert_eq!(settings.mod_role_id, Some(20));
    }

    #[tokio::test]
    async fn seed_applies_once_and_only_to_that_guild() {
        let db = crate::database::test_pool("settings-seed").await;
        let mut seed = EnvSeed {
            mod_log_channel_id: Some(1000),
            member_role_id: Some(2000),
            min_account_age_days: 14,
            ..EnvSeed::default()
        };

        assert!(seed_if_absent(&db, 10, &seed).await.unwrap());
        assert!(!seed_if_absent(&db, 10, &seed).await.unwrap());

        seed.mod_log_channel_id = Some(9999);
        seed.min_account_age_days = 1;
        assert!(!seed_if_absent(&db, 10, &seed).await.unwrap());

        let seeded = load(&db, 10).await.unwrap();
        assert_eq!(seeded.mod_log_channel_id, Some(1000));
        assert_eq!(seeded.member_role_id, Some(2000));
        assert_eq!(seeded.min_account_age_days, 14);

        let other = load(&db, 11).await.unwrap();
        assert!(!other.configured);
        assert_eq!(other.mod_log_channel_id, None);
        assert_eq!(other.min_account_age_days, 3);

        set_field(&db, 10, SettingField::ModLogChannel, Some(1001))
            .await
            .unwrap();
        assert!(!seed_if_absent(&db, 10, &seed).await.unwrap());
        assert_eq!(load(&db, 10).await.unwrap().mod_log_channel_id, Some(1001));
    }
}
