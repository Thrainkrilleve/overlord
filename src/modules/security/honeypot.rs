use serenity::all::{Context, CreateEmbed, Message};
use tracing::{error, info};

pub async fn handle_honeypot_message(
    ctx: &Context,
    msg: &Message,
    honeypot_channel_id: u64,
    mod_log_channel_id: Option<u64>,
    mod_role_id: Option<u64>,
) -> anyhow::Result<()> {
    if msg.channel_id.get() != honeypot_channel_id || msg.author.bot {
        return Ok(());
    }

    let guild_id = match msg.guild_id {
        Some(g) => g,
        None => return Ok(()),
    };

    let author_id = msg.author.id;
    let author_tag = format!("{} ({})", msg.author.tag(), author_id.get());
    let message_snippet = crate::util::embed_field(&msg.content, 200);

    // 1. Delete message
    let _ = msg.delete(&ctx.http).await;

    // Staff posts are a test of the trap (and protect against accidental posts): detect and log,
    // but never ban
    if crate::util::is_staff_message(ctx, msg, guild_id, mod_role_id) {
        info!(target: "security::honeypot", "Honeypot test by staff member {}", author_tag);
        let embed = CreateEmbed::new()
            .title("🧪 Honeypot Test: Trap Working")
            .color(0x3B82F6) // Blue
            .description(format!(
                "A staff member posted in <#{}>. The message was deleted, but staff are never banned. \
                A non-staff account posting here is banned instantly.",
                honeypot_channel_id
            ))
            .field("Staff Member", author_tag, false)
            .field("Message Content", message_snippet, false)
            .timestamp(chrono::Utc::now());
        crate::util::send_mod_log(&ctx.http, mod_log_channel_id, embed).await;
        return Ok(());
    }

    info!(target: "security::honeypot", "Honeypot triggered by {}", author_tag);

    // 2. Ban offender
    let ban_reason = "Overlord Security: Honeypot Trigger (Automated Scraper/Raid Trap)";
    let ban_result = guild_id.ban_with_reason(&ctx.http, author_id, 1, ban_reason).await;
    if let Err(e) = &ban_result {
        error!("Failed to ban honeypot violator {}: {}", author_id, e);
    }

    // 3. Post to mod-log, saying truthfully whether the ban happened
    let (description, action, color) = match &ban_result {
        Ok(()) => (
            format!(
                "An account sent a message in the designated trap channel <#{}> and was automatically banned.",
                honeypot_channel_id
            ),
            "Instant Ban + Message Purge".to_string(),
            0xDC2626, // Deep red
        ),
        Err(e) => (
            format!(
                "An account sent a message in the designated trap channel <#{}>, but **the ban failed**. \
                Check that the bot's role is above this member's roles and has Ban Members.",
                honeypot_channel_id
            ),
            crate::util::embed_field(&format!("Message purged; ban failed: {}", e), 200),
            0xF59E0B, // Amber
        ),
    };

    let embed = CreateEmbed::new()
        .title("🚨 Honeypot Trap Triggered")
        .color(color)
        .description(description)
        .field("Offender", author_tag, false)
        .field("Offender ID", author_id.to_string(), true)
        .field("Action Taken", action, true)
        .field("Message Content", message_snippet, false)
        .timestamp(chrono::Utc::now());
    crate::util::send_mod_log(&ctx.http, mod_log_channel_id, embed).await;

    Ok(())
}
