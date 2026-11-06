use serenity::all::{
    ChannelId, Context, CreateEmbed, CreateMessage, GuildId, Http, Message, Permissions, RoleId,
};
use tracing::{error, warn};

/// Discord rejects embed field values that are empty or longer than 1024 characters.
pub const EMBED_FIELD_MAX: usize = 1024;

/// Clamp text to `max` characters (on char boundaries) and substitute a placeholder for empty
/// text, so it is always a valid embed field value.
pub fn embed_field(text: &str, max: usize) -> String {
    let max = max.min(EMBED_FIELD_MAX);
    if text.trim().is_empty() {
        return "*(no text content)*".to_string();
    }
    if text.chars().count() <= max {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max - 3).collect();
    format!("{}...", truncated)
}

/// Post an embed to the mod-log channel, if one is configured. Failures are logged, not returned,
/// because a broken log channel should never stop the moderation action itself.
pub async fn send_mod_log(http: &Http, mod_log_channel_id: Option<u64>, embed: CreateEmbed) {
    let Some(log_id) = mod_log_channel_id else { return };
    if let Err(e) = ChannelId::new(log_id)
        .send_message(http, CreateMessage::new().embed(embed))
        .await
    {
        warn!("Failed to post to mod-log channel {}: {}", log_id, e);
    }
}

/// Whether a message's author is staff: holds the mod role, owns the server, or has Manage Messages
/// or Administrator through any of their roles.
pub fn is_staff_message(ctx: &Context, msg: &Message, guild_id: GuildId, mod_role_id: Option<u64>) -> bool {
    let Some(member) = &msg.member else { return false };
    if mod_role_id.is_some_and(|id| member.roles.contains(&RoleId::new(id))) {
        return true;
    }
    let Some(guild) = ctx.cache.guild(guild_id) else { return false };
    if guild.owner_id == msg.author.id {
        return true;
    }
    member
        .roles
        .iter()
        .filter_map(|id| guild.roles.get(id))
        .any(|r| r.permissions.intersects(Permissions::ADMINISTRATOR | Permissions::MANAGE_MESSAGES))
}

/// Log an event handler's error with the name of the handler that produced it.
pub fn log_failure<T, E: std::fmt::Display>(handler: &str, result: Result<T, E>) {
    if let Err(e) = result {
        error!("{} failed: {:#}", handler, e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_gets_placeholder() {
        assert_eq!(embed_field("", 100), "*(no text content)*");
        assert_eq!(embed_field("   \n", 100), "*(no text content)*");
    }

    #[test]
    fn short_text_is_unchanged() {
        assert_eq!(embed_field("hello", 100), "hello");
    }

    #[test]
    fn long_text_is_truncated_to_max_chars() {
        let out = embed_field(&"a".repeat(2000), EMBED_FIELD_MAX);
        assert_eq!(out.chars().count(), EMBED_FIELD_MAX);
        assert!(out.ends_with("..."));
    }

    #[test]
    fn multibyte_text_truncates_on_char_boundaries() {
        // 300 emoji = 1200 bytes; slicing by bytes would panic
        let out = embed_field(&"😀".repeat(300), 200);
        assert_eq!(out.chars().count(), 200);
    }

    #[test]
    fn max_is_capped_at_discord_limit() {
        let out = embed_field(&"a".repeat(5000), 4000);
        assert_eq!(out.chars().count(), EMBED_FIELD_MAX);
    }
}
