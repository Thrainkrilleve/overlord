use serenity::all::{
    ChannelId, Context, CreateEmbed, CreateMessage, GuildId, Message, Permissions, RoleId,
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

/// Whether a configured channel or role may be used for this server.
#[derive(Debug, PartialEq, Eq)]
pub enum ResourceCheck {
    /// Nothing was configured. Callers should skip the feature quietly.
    Unset,
    /// The resource belongs to this server.
    Owned,
    /// It belongs to another server, or its server could not be confirmed.
    Rejected,
}

pub fn check_resource(
    configured: Option<u64>,
    located_guild: Option<u64>,
    expected_guild: u64,
) -> ResourceCheck {
    match configured {
        None => ResourceCheck::Unset,
        Some(_) => match located_guild {
            Some(guild) if guild == expected_guild => ResourceCheck::Owned,
            _ => ResourceCheck::Rejected,
        },
    }
}

fn cached_channel_guild(ctx: &Context, guild_id: GuildId, channel_id: ChannelId) -> bool {
    ctx.cache
        .guild(guild_id)
        .is_some_and(|guild| guild.channels.contains_key(&channel_id))
}

fn cached_role_guild(ctx: &Context, guild_id: GuildId, role_id: RoleId) -> bool {
    ctx.cache
        .guild(guild_id)
        .is_some_and(|guild| guild.roles.contains_key(&role_id))
}

async fn locate_channel(ctx: &Context, guild_id: GuildId, channel_id: u64) -> Option<u64> {
    let channel_id = ChannelId::new(channel_id);
    if cached_channel_guild(ctx, guild_id, channel_id) {
        return Some(guild_id.get());
    }
    match channel_id.to_channel(ctx).await {
        Ok(serenity::all::Channel::Guild(channel)) => Some(channel.guild_id.get()),
        _ => None,
    }
}

async fn locate_role(ctx: &Context, guild_id: GuildId, role_id: u64) -> Option<u64> {
    let role_id = RoleId::new(role_id);
    if cached_role_guild(ctx, guild_id, role_id) {
        return Some(guild_id.get());
    }
    match ctx.http.get_guild_roles(guild_id).await {
        Ok(roles) if roles.iter().any(|role| role.id == role_id) => Some(guild_id.get()),
        _ => None,
    }
}

/// The channel id when it belongs to `guild_id`. Unset and foreign channels are `None`.
/// A foreign channel is refused so a log or transcript cannot be posted in another server.
pub async fn channel_in_guild(
    ctx: &Context,
    guild_id: GuildId,
    channel_id: Option<u64>,
) -> Option<ChannelId> {
    let raw = channel_id?;
    let located = locate_channel(ctx, guild_id, raw).await;
    match check_resource(Some(raw), located, guild_id.get()) {
        ResourceCheck::Owned => Some(ChannelId::new(raw)),
        ResourceCheck::Unset => None,
        ResourceCheck::Rejected => {
            match located {
                Some(other) => warn!(
                    "Refusing channel {}: it belongs to guild {}, not {}",
                    raw, other, guild_id
                ),
                None => warn!(
                    "Refusing channel {}: it is not a channel in guild {}",
                    raw, guild_id
                ),
            }
            None
        }
    }
}

/// The role id when it belongs to `guild_id`.
pub async fn role_in_guild(
    ctx: &Context,
    guild_id: GuildId,
    role_id: Option<u64>,
) -> Option<RoleId> {
    let raw = role_id?;
    let located = locate_role(ctx, guild_id, raw).await;
    match check_resource(Some(raw), located, guild_id.get()) {
        ResourceCheck::Owned => Some(RoleId::new(raw)),
        ResourceCheck::Unset => None,
        ResourceCheck::Rejected => {
            warn!(
                "Refusing role {}: it is not a role in guild {}",
                raw, guild_id
            );
            None
        }
    }
}

/// Post an embed to this server's mod-log channel. A missing channel is skipped. A channel that
/// belongs to a different server is refused. Failures are logged, not returned, because a broken
/// log channel should never stop the moderation action itself.
pub async fn send_mod_log(
    ctx: &Context,
    guild_id: GuildId,
    mod_log_channel_id: Option<u64>,
    embed: CreateEmbed,
) {
    let Some(log_id) = channel_in_guild(ctx, guild_id, mod_log_channel_id).await else {
        return;
    };
    if let Err(e) = log_id
        .send_message(&ctx.http, CreateMessage::new().embed(embed))
        .await
    {
        warn!("Failed to post to mod-log channel {}: {}", log_id, e);
    }
}

/// Whether a message's author is staff: holds the mod role, owns the server, or has Manage Messages
/// or Administrator through any of their roles.
pub fn is_staff_message(
    ctx: &Context,
    msg: &Message,
    guild_id: GuildId,
    mod_role_id: Option<u64>,
) -> bool {
    let Some(member) = &msg.member else {
        return false;
    };
    if mod_role_id.is_some_and(|id| member.roles.contains(&RoleId::new(id))) {
        return true;
    }
    let Some(guild) = ctx.cache.guild(guild_id) else {
        return false;
    };
    if guild.owner_id == msg.author.id {
        return true;
    }
    member
        .roles
        .iter()
        .filter_map(|id| guild.roles.get(id))
        .any(|r| {
            r.permissions
                .intersects(Permissions::ADMINISTRATOR | Permissions::MANAGE_MESSAGES)
        })
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

    #[test]
    fn resources_from_another_guild_are_rejected() {
        assert_eq!(check_resource(None, Some(2), 1), ResourceCheck::Unset);
        assert_eq!(check_resource(Some(50), Some(1), 1), ResourceCheck::Owned);
        assert_eq!(
            check_resource(Some(50), Some(2), 1),
            ResourceCheck::Rejected
        );
        assert_eq!(check_resource(Some(50), None, 1), ResourceCheck::Rejected);
    }
}
