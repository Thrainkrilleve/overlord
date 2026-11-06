use regex::Regex;
use serenity::all::{
    Context, CreateEmbed, Message, MessageUpdateEvent, Timestamp,
};
use std::sync::OnceLock;
use tracing::info;
use crate::database::DbPool;

static INVITE_REGEX: OnceLock<Regex> = OnceLock::new();

fn get_invite_regex() -> &'static Regex {
    INVITE_REGEX.get_or_init(|| {
        Regex::new(r"(?i)(discord\.(gg|io|me|li)|discord(app)?\.com/invite)/[a-zA-Z0-9-]+").unwrap()
    })
}

/// The rule a message breaks, if any. Mass mentions take priority over invites.
fn find_violation(content: &str, mention_count: usize) -> Option<&'static str> {
    if mention_count > 5 {
        Some("Mass Mention Spam (> 5 users mentioned)")
    } else if get_invite_regex().is_match(content) {
        Some("Unauthorized Discord Invite Link")
    } else {
        None
    }
}

pub async fn inspect_message(
    ctx: &Context,
    msg: &Message,
    db: &DbPool,
    mod_log_channel_id: Option<u64>,
    mod_role_id: Option<u64>,
) -> anyhow::Result<bool> {
    if msg.author.bot {
        return Ok(false);
    }

    let guild_id = match msg.guild_id {
        Some(g) => g,
        None => return Ok(false),
    };

    if crate::util::is_staff_message(ctx, msg, guild_id, mod_role_id) {
        return Ok(false);
    }

    if let Some(reason) = find_violation(&msg.content, msg.mentions.len()) {
        info!("AutoMod caught message from {}: {}", msg.author.tag(), reason);

        // Delete message
        let _ = msg.delete(&ctx.http).await;

        // Apply a 10-minute timeout
        let until = chrono::Utc::now() + chrono::Duration::minutes(10);
        if let Ok(mut member) = guild_id.member(&ctx.http, msg.author.id).await {
            let timestamp = Timestamp::from_unix_timestamp(until.timestamp()).unwrap();
            let _ = member.disable_communication_until_datetime(&ctx.http, timestamp).await;
        }

        // Record in SQLite
        let bot_id = ctx.cache.current_user().id.get();
        let _ = crate::database::record_infraction(
            db,
            msg.author.id.get(),
            guild_id.get(),
            bot_id,
            "automod_timeout",
            reason,
        )
        .await;

        // Log to mod channel
        let embed = CreateEmbed::new()
            .title("🛡️ AutoMod Filter Triggered")
            .color(0xEF4444)
            .field("User", format!("{} (`{}`)", msg.author.tag(), msg.author.id.get()), false)
            .field("Reason", reason, true)
            .field("Penalty", "10-minute Timeout + Message Purged", true)
            .field("Content", crate::util::embed_field(&msg.content, crate::util::EMBED_FIELD_MAX), false)
            .timestamp(chrono::Utc::now());
        crate::util::send_mod_log(&ctx.http, mod_log_channel_id, embed).await;

        return Ok(true);
    }

    Ok(false)
}

/// Re-check a message after its author edits it, so an edit can't sneak in an invite or mentions.
pub async fn inspect_edit(
    ctx: &Context,
    event: &MessageUpdateEvent,
    db: &DbPool,
    mod_log_channel_id: Option<u64>,
    mod_role_id: Option<u64>,
) -> anyhow::Result<bool> {
    // Link previews and pins also fire updates; only edits by the author set edited_timestamp
    if event.edited_timestamp.is_none() || event.guild_id.is_none() {
        return Ok(false);
    }

    // Messages aren't cached, so fetch the edited message. It may already be gone (e.g. AutoMod
    // deleted the original and this update is its link preview arriving late).
    let Ok(mut msg) = event.channel_id.message(&ctx.http, event.id).await else {
        return Ok(false);
    };
    // REST messages omit these, but the staff check and timeout need them
    msg.guild_id = event.guild_id;
    msg.member = event.member.clone().flatten();

    inspect_message(ctx, &msg, db, mod_log_channel_id, mod_role_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catches_all_invite_formats() {
        for link in [
            "join discord.gg/abc123",
            "https://discord.gg/abc123",
            "DISCORD.GG/AbC",
            "https://discord.com/invite/abc123",
            "https://discordapp.com/invite/abc123",
            "discord.me/server",
            "discord.gg/my-vanity",
        ] {
            assert!(find_violation(link, 0).is_some(), "missed {}", link);
        }
    }

    #[test]
    fn ignores_normal_discord_links() {
        for text in [
            "hello world",
            "https://discord.com/channels/1/2/3",
            "https://discord.com/developers/applications",
            "I love discord",
        ] {
            assert!(find_violation(text, 0).is_none(), "false positive on {}", text);
        }
    }

    #[test]
    fn mass_mentions_over_five() {
        assert!(find_violation("hi", 5).is_none());
        assert_eq!(find_violation("hi", 6), Some("Mass Mention Spam (> 5 users mentioned)"));
    }
}
