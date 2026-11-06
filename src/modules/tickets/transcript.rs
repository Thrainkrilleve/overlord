use serenity::all::{
    ChannelId, Context, CreateAttachment, CreateEmbed, CreateMessage, GetMessages, MessageId, User,
    UserId,
};
use tracing::info;
use crate::database::DbPool;

/// Convert SQLite's `datetime('now')` text (UTC) into a Discord timestamp tag, which each viewer's
/// client renders in their own time zone.
fn discord_timestamp(sqlite_utc: &str) -> Option<String> {
    let parsed = chrono::NaiveDateTime::parse_from_str(sqlite_utc, "%Y-%m-%d %H:%M:%S").ok()?;
    Some(format!("<t:{}:f>", parsed.and_utc().timestamp()))
}

/// "name (id)" for the transcript file, falling back to the bare ID if the user can't be fetched.
async fn user_label(ctx: &Context, id: u64) -> String {
    match ctx.http.get_user(UserId::new(id)).await {
        Ok(user) => format!("{} ({})", user.tag(), id),
        Err(_) => id.to_string(),
    }
}

pub async fn generate_and_archive_transcript(
    ctx: &Context,
    db: &DbPool,
    channel_id: ChannelId,
    transcript_channel_id: Option<u64>,
    closer: &User,
) -> anyhow::Result<()> {
    let log_id = transcript_channel_id
        .ok_or_else(|| anyhow::anyhow!("TRANSCRIPT_CHANNEL_ID is not configured"))?;

    // 1. Fetch every channel message, 100 at a time (newest first)
    let mut messages = Vec::new();
    let mut before: Option<MessageId> = None;
    loop {
        let mut request = GetMessages::new().limit(100);
        if let Some(id) = before {
            request = request.before(id);
        }
        let batch = channel_id.messages(&ctx.http, request).await?;
        let done = batch.len() < 100;
        before = batch.last().map(|m| m.id);
        messages.extend(batch);
        if done {
            break;
        }
    }
    messages.reverse(); // Chronological order

    // Ticket details: the DB row, plus the subject from the bot's opening embed (not stored in the DB)
    let ticket = crate::database::get_ticket(db, channel_id.get()).await?;
    let subject = messages
        .iter()
        .filter(|m| m.author.bot)
        .flat_map(|m| &m.embeds)
        .flat_map(|e| &e.fields)
        .find(|f| f.name == "Subject")
        .map(|f| f.value.clone())
        .unwrap_or_default();

    let ticket_label = ticket.as_ref().map_or_else(|| "Unknown".to_string(), |t| format!("#{}", t.id));
    let category = ticket.as_ref().map_or("Unknown", |t| t.category.as_str());
    let opened_by_id = ticket.as_ref().map(|t| t.user_id as u64);
    let claimed_by_id = ticket.as_ref().and_then(|t| t.claimed_by).map(|id| id as u64);
    let opened_at = ticket.as_ref().map_or("Unknown", |t| t.created_at.as_str());

    let opened_by_label = match opened_by_id {
        Some(id) => user_label(ctx, id).await,
        None => "Unknown".to_string(),
    };
    let claimed_by_label = match claimed_by_id {
        Some(id) => user_label(ctx, id).await,
        None => "Unclaimed".to_string(),
    };
    let closer_label = format!("{} ({})", closer.tag(), closer.id);

    // 2. Format transcript
    let mut transcript_text = format!(
        "=====================================================\n\
         OVERLORD TICKET TRANSCRIPT\n\
         Ticket: {}\n\
         Category: {}\n\
         Subject: {}\n\
         Opened by: {}\n\
         Opened at: {} UTC\n\
         Claimed by: {}\n\
         Closed by: {}\n\
         Closed at: {}\n\
         Channel ID: {}\n\
         Total Messages: {}\n\
         =====================================================\n\n",
        ticket_label,
        category,
        if subject.is_empty() { "Unknown" } else { subject.as_str() },
        opened_by_label,
        opened_at,
        claimed_by_label,
        closer_label,
        chrono::Utc::now().to_rfc3339(),
        channel_id.get(),
        messages.len()
    );

    for msg in &messages {
        let timestamp = msg.timestamp.to_string();
        let author = format!("{} ({})", msg.author.tag(), msg.author.id.get());
        let attachments: Vec<String> = msg.attachments.iter().map(|a| a.url.clone()).collect();
        let attachment_str = if !attachments.is_empty() {
            format!("\n  [Attachments]: {}", attachments.join(", "))
        } else {
            String::new()
        };

        // Embeds carry the ticket subject/details from the opening message
        let mut embed_str = String::new();
        for embed in &msg.embeds {
            if let Some(title) = &embed.title {
                embed_str.push_str(&format!("\n  [Embed] {}", title));
            }
            if let Some(desc) = &embed.description {
                embed_str.push_str(&format!("\n    {}", desc));
            }
            for field in &embed.fields {
                embed_str.push_str(&format!("\n    {}: {}", field.name, field.value));
            }
        }

        transcript_text.push_str(&format!(
            "[{}] {}:\n  {}{}{}\n\n",
            timestamp, author, msg.content, embed_str, attachment_str
        ));
    }

    // 3. Post to transcript archive channel
    let filename = match &ticket {
        Some(t) => format!("ticket-{}-transcript.txt", t.id),
        None => format!("transcript-{}.txt", channel_id.get()),
    };
    let attachment = CreateAttachment::bytes(transcript_text.as_bytes().to_vec(), &filename);

    let mention = |id: Option<u64>, fallback: &str| id.map_or_else(|| fallback.to_string(), |id| format!("<@{}>", id));
    let embed = CreateEmbed::new()
        .title(format!("📋 Ticket {} Closed & Archived", ticket_label))
        .color(0x3B82F6)
        .field("Category", category, true)
        .field("Opened By", mention(opened_by_id, "Unknown"), true)
        .field("Claimed By", mention(claimed_by_id, "Unclaimed"), true)
        .field("Subject", crate::util::embed_field(&subject, 256), false)
        .field("Closed By", format!("<@{}>", closer.id), true)
        .field(
            "Opened At",
            discord_timestamp(opened_at).unwrap_or_else(|| format!("{} UTC", opened_at)),
            true,
        )
        .field("Messages", messages.len().to_string(), true)
        .timestamp(chrono::Utc::now());

    let msg_builder = CreateMessage::new().embed(embed).add_file(attachment);
    ChannelId::new(log_id).send_message(&ctx.http, msg_builder).await?;
    info!("Transcript archived for channel {}", channel_id.get());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_utc_becomes_discord_timestamp() {
        // 2026-11-02 04:01:05 UTC
        assert_eq!(discord_timestamp("2026-11-02 04:01:05").as_deref(), Some("<t:1793592065:f>"));
    }

    #[test]
    fn unparseable_time_is_none() {
        assert_eq!(discord_timestamp("Unknown"), None);
    }
}
