use crate::guild_settings::{GuildSettings, SettingField};
use crate::{Context, Error};
use serenity::all::{ChannelType, CreateEmbed, GuildChannel, Role};

#[derive(Debug, Clone, Copy, poise::ChoiceParameter)]
pub enum SettingKey {
    #[name = "Mod log channel"]
    ModLog,
    #[name = "Honeypot channel"]
    Honeypot,
    #[name = "Verify channel"]
    VerifyChannel,
    #[name = "Transcript channel"]
    Transcript,
    #[name = "Join-to-create voice channel"]
    JoinToCreate,
    #[name = "Temp voice category"]
    VoiceCategory,
    #[name = "Ticket category"]
    TicketCategory,
    #[name = "Unverified role"]
    UnverifiedRole,
    #[name = "Member role"]
    MemberRole,
    #[name = "Staff role"]
    ModRole,
    #[name = "Minimum account age (days)"]
    MinAccountAge,
}

enum ExpectedValue {
    TextChannel,
    VoiceChannel,
    Category,
    Role,
    Days,
}

impl SettingKey {
    fn field(self) -> SettingField {
        match self {
            Self::ModLog => SettingField::ModLogChannel,
            Self::Honeypot => SettingField::HoneypotChannel,
            Self::VerifyChannel => SettingField::VerifyChannel,
            Self::Transcript => SettingField::TranscriptChannel,
            Self::JoinToCreate => SettingField::JoinToCreateChannel,
            Self::VoiceCategory => SettingField::VoiceCategory,
            Self::TicketCategory => SettingField::TicketCategory,
            Self::UnverifiedRole => SettingField::UnverifiedRole,
            Self::MemberRole => SettingField::MemberRole,
            Self::ModRole => SettingField::ModRole,
            Self::MinAccountAge => SettingField::MinAccountAge,
        }
    }

    fn expected(self) -> ExpectedValue {
        match self {
            Self::ModLog | Self::Honeypot | Self::VerifyChannel | Self::Transcript => {
                ExpectedValue::TextChannel
            }
            Self::JoinToCreate => ExpectedValue::VoiceChannel,
            Self::VoiceCategory | Self::TicketCategory => ExpectedValue::Category,
            Self::UnverifiedRole | Self::MemberRole | Self::ModRole => ExpectedValue::Role,
            Self::MinAccountAge => ExpectedValue::Days,
        }
    }
}

fn channel_accepted(kind: ChannelType, expected: ExpectedValue) -> bool {
    match expected {
        ExpectedValue::TextChannel => matches!(kind, ChannelType::Text | ChannelType::News),
        ExpectedValue::VoiceChannel => matches!(kind, ChannelType::Voice),
        ExpectedValue::Category => matches!(kind, ChannelType::Category),
        ExpectedValue::Role | ExpectedValue::Days => false,
    }
}

fn expected_label(expected: ExpectedValue) -> &'static str {
    match expected {
        ExpectedValue::TextChannel => "a text channel",
        ExpectedValue::VoiceChannel => "a voice channel",
        ExpectedValue::Category => "a category",
        ExpectedValue::Role => "a role",
        ExpectedValue::Days => "a number of days",
    }
}

fn channel_mention(id: Option<u64>, empty: &str) -> String {
    id.map(|id| format!("<#{}>", id))
        .unwrap_or_else(|| empty.to_string())
}

fn role_mention(id: Option<u64>) -> String {
    id.map(|id| format!("<@&{}>", id))
        .unwrap_or_else(|| "Not set".into())
}

pub fn settings_embed(settings: &GuildSettings, title: &str) -> CreateEmbed {
    let mut embed = CreateEmbed::new()
        .title(title)
        .color(0x3B82F6)
        .field(
            "Min Account Age",
            format!("{} days", settings.min_account_age_days),
            true,
        )
        .field(
            "Honeypot Trap",
            channel_mention(settings.honeypot_channel_id, "Disabled"),
            true,
        )
        .field(
            "Mod Log Channel",
            channel_mention(settings.mod_log_channel_id, "Not configured"),
            true,
        )
        .field(
            "Transcript Channel",
            channel_mention(settings.transcript_channel_id, "Not configured"),
            true,
        )
        .field(
            "Temp Voice Creator",
            channel_mention(settings.join_to_create_channel_id, "Not configured"),
            true,
        )
        .field(
            "Temp Voice Category",
            channel_mention(settings.voice_category_id, "Not configured"),
            true,
        )
        .field(
            "Verify Channel",
            channel_mention(settings.verify_channel_id, "Not configured"),
            true,
        )
        .field(
            "Ticket Category",
            channel_mention(settings.ticket_category_id, "Not configured"),
            true,
        )
        .field(
            "Unverified Role",
            role_mention(settings.unverified_role_id),
            true,
        )
        .field("Member Role", role_mention(settings.member_role_id), true)
        .field("Staff Role", role_mention(settings.mod_role_id), true)
        .timestamp(chrono::Utc::now());

    if !settings.configured {
        embed = embed.description(
            "This server has no saved settings yet. Channels, roles, and logs stay off until an admin uses `/config set`.",
        );
    }

    embed.footer(serenity::all::CreateEmbedFooter::new(
        "Change these with /config set. /config clear turns a channel or role feature off. This reply is only visible to you.",
    ))
}

async fn reply(ctx: Context<'_>, content: String) -> Result<(), Error> {
    ctx.send(
        poise::CreateReply::default()
            .content(content)
            .ephemeral(true),
    )
    .await?;
    Ok(())
}

/// View or change this server's Overlord settings
#[poise::command(
    slash_command,
    guild_only,
    default_member_permissions = "ADMINISTRATOR",
    subcommands("show", "set", "clear")
)]
pub async fn config(_ctx: Context<'_>) -> Result<(), Error> {
    // Slash invocations always include a subcommand, so this body is unused.
    Ok(())
}

/// Show this server's channels, roles, and account-age policy
#[poise::command(
    slash_command,
    guild_only,
    default_member_permissions = "ADMINISTRATOR"
)]
pub async fn show(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Must be run in a server")?;
    let settings = crate::guild_settings::load(&ctx.data().db, guild_id.get()).await?;
    let embed = settings_embed(&settings, "⚙️ Overlord Settings");
    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;
    Ok(())
}

/// Set one channel, role, or the minimum account age for this server
#[poise::command(
    slash_command,
    guild_only,
    default_member_permissions = "ADMINISTRATOR"
)]
pub async fn set(
    ctx: Context<'_>,
    #[description = "Which setting to change"] key: SettingKey,
    #[description = "Channel, when this setting is a channel or category"] channel: Option<
        GuildChannel,
    >,
    #[description = "Role, when this setting is a role"] role: Option<Role>,
    #[description = "Days, for minimum account age (0-3650)"] days: Option<i64>,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Must be run in a server")?;
    let field = key.field();
    let stored = match key.expected() {
        ExpectedValue::TextChannel | ExpectedValue::VoiceChannel | ExpectedValue::Category => {
            let channel = channel.ok_or_else(|| {
                format!(
                    "Choose {} for {}.",
                    expected_label(key.expected()),
                    field.label()
                )
            })?;
            if channel.guild_id != guild_id {
                return Err("That channel is not in this server.".into());
            }
            if !channel_accepted(channel.kind, key.expected()) {
                return Err(format!(
                    "Choose {} for {}.",
                    expected_label(key.expected()),
                    field.label()
                )
                .into());
            }
            channel.id.get() as i64
        }
        ExpectedValue::Role => {
            let role = role.ok_or_else(|| format!("Choose a role for {}.", field.label()))?;
            if role.guild_id != guild_id {
                return Err("That role is not in this server.".into());
            }
            role.id.get() as i64
        }
        ExpectedValue::Days => {
            let days = days.ok_or("Enter a number of days for the minimum account age.")?;
            if !(0..=3650).contains(&days) {
                return Err("Minimum account age must be between 0 and 3650 days.".into());
            }
            days
        }
    };

    crate::guild_settings::set_field(&ctx.data().db, guild_id.get(), field, Some(stored)).await?;

    let confirmation = match key.expected() {
        ExpectedValue::TextChannel | ExpectedValue::VoiceChannel | ExpectedValue::Category => {
            format!("{} set to <#{}> for this server.", field.label(), stored)
        }
        ExpectedValue::Role => format!("{} set to <@&{}> for this server.", field.label(), stored),
        ExpectedValue::Days => format!(
            "Minimum account age set to {} days for this server.",
            stored
        ),
    };
    reply(ctx, confirmation).await
}

/// Turn off a channel or role feature in this server
#[poise::command(
    slash_command,
    guild_only,
    default_member_permissions = "ADMINISTRATOR"
)]
pub async fn clear(
    ctx: Context<'_>,
    #[description = "Which setting to clear"] key: SettingKey,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Must be run in a server")?;
    let field = key.field();
    if matches!(key, SettingKey::MinAccountAge) {
        crate::guild_settings::set_field(&ctx.data().db, guild_id.get(), field, Some(3)).await?;
        return reply(
            ctx,
            "Minimum account age reset to 3 days for this server.".into(),
        )
        .await;
    }
    crate::guild_settings::set_field(&ctx.data().db, guild_id.get(), field, None).await?;
    reply(
        ctx,
        format!(
            "{} cleared. That feature is now off in this server.",
            field.label()
        ),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_kinds_follow_the_setting() {
        assert!(channel_accepted(
            ChannelType::Text,
            ExpectedValue::TextChannel
        ));
        assert!(channel_accepted(
            ChannelType::News,
            ExpectedValue::TextChannel
        ));
        assert!(!channel_accepted(
            ChannelType::Voice,
            ExpectedValue::TextChannel
        ));
        assert!(channel_accepted(
            ChannelType::Voice,
            ExpectedValue::VoiceChannel
        ));
        assert!(!channel_accepted(
            ChannelType::Text,
            ExpectedValue::VoiceChannel
        ));
        assert!(channel_accepted(
            ChannelType::Category,
            ExpectedValue::Category
        ));
        assert!(!channel_accepted(
            ChannelType::Category,
            ExpectedValue::TextChannel
        ));
    }
}
