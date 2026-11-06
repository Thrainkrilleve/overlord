use serenity::all::{CreateEmbed, CreateMessage, Timestamp, User};
use crate::util::{embed_field, send_mod_log};
use crate::{Data, Error};

type Context<'a> = poise::Context<'a, Data, Error>;

/// Refuse to act on a member whose highest role is at or above the invoking moderator's.
/// Discord only enforces hierarchy against the bot's role, so this must be checked here.
async fn ensure_can_moderate(ctx: Context<'_>, target: &User) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Must be run in a server")?;
    let http = &ctx.serenity_context().http;

    // Users who are not in the server (e.g. pre-emptive bans) have no roles to compare.
    let target_member = match guild_id.member(http, target.id).await {
        Ok(m) => m,
        Err(_) => return Ok(()),
    };
    let moderator = guild_id.member(http, ctx.author().id).await?;

    let allowed = {
        let guild = ctx.guild().ok_or("Server is not cached yet, try again shortly")?;
        let position = |m: &serenity::all::Member| guild.member_highest_role(m).map_or(0, |r| r.position);
        if target.id == guild.owner_id {
            false
        } else {
            moderator.user.id == guild.owner_id || position(&moderator) > position(&target_member)
        }
    };

    if !allowed {
        return Err("You can't moderate a member whose highest role is equal to or above yours.".into());
    }
    Ok(())
}

/// DM the user about a moderation action. Must run before a kick/ban, while the bot still
/// shares a server with them. Users with DMs closed are silently skipped.
async fn notify_user(ctx: Context<'_>, user: &User, action: &str, reason: &str) {
    let guild_name = ctx.guild().map(|g| g.name.clone()).unwrap_or_else(|| "the server".into());
    let dm = CreateMessage::new().content(format!("You were {} in **{}**: {}", action, guild_name, reason));
    let _ = user.direct_message(&ctx.serenity_context().http, dm).await;
}

/// Issue a warning to a server member
#[poise::command(slash_command, guild_only, default_member_permissions = "MODERATE_MEMBERS")]
pub async fn warn(
    ctx: Context<'_>,
    #[description = "The user to warn"] user: User,
    #[description = "Reason for the warning"] #[max_length = 500] reason: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or("Must be run in a server")?;
    let moderator_id = ctx.author().id;
    ensure_can_moderate(ctx, &user).await?;

    let case_id = crate::database::record_infraction(
        &ctx.data().db,
        user.id.get(),
        guild_id.get(),
        moderator_id.get(),
        "warn",
        &reason,
    )
    .await?;

    let embed = CreateEmbed::new()
        .title("⚠️ Member Warned")
        .color(0xFBBF24)
        .field("User", format!("{} (`{}`)", user.tag(), user.id), true)
        .field("Moderator", ctx.author().tag(), true)
        .field("Case #", case_id.to_string(), true)
        .field("Reason", &reason, false)
        .timestamp(chrono::Utc::now());

    ctx.send(poise::CreateReply::default().embed(embed.clone())).await?;
    notify_user(ctx, &user, "warned", &reason).await;
    send_mod_log(&ctx.serenity_context().http, ctx.data().config.mod_log_channel_id, embed).await;

    Ok(())
}

/// Temporarily timeout/mute a server member
#[poise::command(slash_command, guild_only, default_member_permissions = "MODERATE_MEMBERS")]
pub async fn timeout(
    ctx: Context<'_>,
    #[description = "The user to timeout"] user: User,
    // Discord caps timeouts at 28 days
    #[description = "Duration in minutes (max 40320 = 28 days)"] #[min = 1] #[max = 40320] minutes: u32,
    #[description = "Reason for timeout"] #[max_length = 500] reason: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or("Must be run in a server")?;
    let moderator_id = ctx.author().id;
    ensure_can_moderate(ctx, &user).await?;

    let mut member = guild_id.member(&ctx.serenity_context().http, user.id).await?;
    let until = chrono::Utc::now() + chrono::Duration::minutes(minutes as i64);
    let timestamp = Timestamp::from_unix_timestamp(until.timestamp())?;

    member.disable_communication_until_datetime(&ctx.serenity_context().http, timestamp).await?;

    let case_id = crate::database::record_infraction(
        &ctx.data().db,
        user.id.get(),
        guild_id.get(),
        moderator_id.get(),
        "timeout",
        &reason,
    )
    .await?;

    let embed = CreateEmbed::new()
        .title("⏳ Member Timed Out")
        .color(0xF97316)
        .field("User", format!("{} (`{}`)", user.tag(), user.id), true)
        .field("Duration", format!("{} minutes", minutes), true)
        .field("Moderator", ctx.author().tag(), true)
        .field("Case #", case_id.to_string(), true)
        .field("Reason", &reason, false)
        .timestamp(chrono::Utc::now());

    ctx.send(poise::CreateReply::default().embed(embed.clone())).await?;
    notify_user(ctx, &user, &format!("timed out for {} minutes", minutes), &reason).await;
    send_mod_log(&ctx.serenity_context().http, ctx.data().config.mod_log_channel_id, embed).await;

    Ok(())
}

/// Kick a member from the server
#[poise::command(slash_command, guild_only, default_member_permissions = "KICK_MEMBERS")]
pub async fn kick(
    ctx: Context<'_>,
    #[description = "The user to kick"] user: User,
    #[description = "Reason for kick"] #[max_length = 500] reason: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or("Must be run in a server")?;
    ensure_can_moderate(ctx, &user).await?;

    notify_user(ctx, &user, "kicked", &reason).await;
    guild_id.kick_with_reason(&ctx.serenity_context().http, user.id, &reason).await?;

    let case_id = crate::database::record_infraction(
        &ctx.data().db,
        user.id.get(),
        guild_id.get(),
        ctx.author().id.get(),
        "kick",
        &reason,
    )
    .await?;

    let embed = CreateEmbed::new()
        .title("👢 Member Kicked")
        .color(0xEF4444)
        .field("User", format!("{} (`{}`)", user.tag(), user.id), true)
        .field("Moderator", ctx.author().tag(), true)
        .field("Case #", case_id.to_string(), true)
        .field("Reason", &reason, false)
        .timestamp(chrono::Utc::now());

    ctx.send(poise::CreateReply::default().embed(embed.clone())).await?;
    send_mod_log(&ctx.serenity_context().http, ctx.data().config.mod_log_channel_id, embed).await;
    Ok(())
}

/// Ban a member from the server
#[poise::command(slash_command, guild_only, default_member_permissions = "BAN_MEMBERS")]
pub async fn ban(
    ctx: Context<'_>,
    #[description = "The user to ban"] user: User,
    #[description = "Reason for ban"] #[max_length = 500] reason: String,
    #[description = "Days of messages to delete (0-7)"] d_days: Option<u8>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or("Must be run in a server")?;
    let delete_days = d_days.unwrap_or(1).min(7);
    ensure_can_moderate(ctx, &user).await?;

    notify_user(ctx, &user, "banned", &reason).await;
    guild_id.ban_with_reason(&ctx.serenity_context().http, user.id, delete_days, &reason).await?;

    let case_id = crate::database::record_infraction(
        &ctx.data().db,
        user.id.get(),
        guild_id.get(),
        ctx.author().id.get(),
        "ban",
        &reason,
    )
    .await?;

    let embed = CreateEmbed::new()
        .title("🔨 Member Banned")
        .color(0x991B1B)
        .field("User", format!("{} (`{}`)", user.tag(), user.id), true)
        .field("Moderator", ctx.author().tag(), true)
        .field("Case #", case_id.to_string(), true)
        .field("Reason", &reason, false)
        .timestamp(chrono::Utc::now());

    ctx.send(poise::CreateReply::default().embed(embed.clone())).await?;
    send_mod_log(&ctx.serenity_context().http, ctx.data().config.mod_log_channel_id, embed).await;
    Ok(())
}

/// Bulk delete messages from the current channel
#[poise::command(slash_command, guild_only, default_member_permissions = "MANAGE_MESSAGES")]
pub async fn purge(
    ctx: Context<'_>,
    #[description = "Number of messages to delete (1-100)"] #[min = 1] #[max = 100] count: u8,
) -> Result<(), Error> {
    // Ephemeral, so the "thinking..." reply is not a channel message that gets purged itself
    ctx.defer_ephemeral().await?;
    let http = &ctx.serenity_context().http;
    let channel_id = ctx.channel_id();

    // Discord's bulk delete rejects the whole request if any message is older than 14 days
    let cutoff = chrono::Utc::now().timestamp() - 14 * 24 * 60 * 60;
    let messages: Vec<_> = channel_id
        .messages(http, serenity::all::GetMessages::new().limit(count.clamp(1, 100)))
        .await?
        .into_iter()
        .filter(|m| m.timestamp.unix_timestamp() > cutoff)
        .map(|m| m.id)
        .collect();

    let deleted = messages.len();
    if deleted > 0 {
        channel_id.delete_messages(http, messages).await?;
    }

    let skipped = count as usize - deleted;
    let note = if skipped > 0 {
        format!(" ({} older than 14 days or not found were skipped)", skipped)
    } else {
        String::new()
    };
    ctx.send(poise::CreateReply::default().content(format!("🧹 Purged {} messages{}.", deleted, note))).await?;
    Ok(())
}

/// View infraction history for a user
#[poise::command(slash_command, guild_only, default_member_permissions = "MODERATE_MEMBERS")]
pub async fn infractions(
    ctx: Context<'_>,
    #[description = "User to inspect"] user: User,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or("Must be run in a server")?;

    let history = crate::database::get_infractions(&ctx.data().db, user.id.get(), guild_id.get()).await?;

    if history.is_empty() {
        ctx.send(poise::CreateReply::default().content(format!("No previous infractions found for {}.", user.tag()))).await?;
        return Ok(());
    }

    let mut embed = CreateEmbed::new()
        .title(format!("Infraction History: {}", user.tag()))
        .color(0x6366F1);

    for inf in history.iter().take(10) {
        embed = embed.field(
            format!("[Case #{}] Action: {}", inf.id, inf.action.to_uppercase()),
            // 10 fields share the 6000-char embed budget, so keep each reason short
            format!("**Reason**: {}\n**Date**: {}", embed_field(&inf.reason, 300), inf.created_at),
            false,
        );
    }

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}
