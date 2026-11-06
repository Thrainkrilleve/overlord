mod config;
mod database;
mod modules;
mod util;

use config::Config;
use database::DbPool;
use modules::security::RaidTracker;
use serenity::all::{CreateEmbed, CreateMessage, FullEvent, GatewayIntents, Interaction, RoleId};
use tracing::{info, warn};

pub struct Data {
    pub db: DbPool,
    pub config: Config,
    pub raid_tracker: RaidTracker,
    pub verify_codes: modules::verification::VerificationCodes,
}

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;

/// Deploy the human verification gate in this channel
#[poise::command(slash_command, guild_only, default_member_permissions = "ADMINISTRATOR")]
pub async fn setup_verification(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let (embed, row) = modules::verification::build_verification_panel();
    let msg = CreateMessage::new().embed(embed).components(vec![row]);
    ctx.channel_id().send_message(&ctx.serenity_context().http, msg).await?;
    ctx.say("✅ Verification gate panel deployed!").await?;
    Ok(())
}

/// Deploy the support ticket hub in this channel
#[poise::command(slash_command, guild_only, default_member_permissions = "ADMINISTRATOR")]
pub async fn setup_tickets(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let (embed, row) = modules::tickets::build_ticket_panel();
    let msg = CreateMessage::new().embed(embed).components(vec![row]);
    ctx.channel_id().send_message(&ctx.serenity_context().http, msg).await?;
    ctx.say("✅ Ticket hub panel deployed!").await?;
    Ok(())
}

/// View live Overlord security and configuration status
#[poise::command(slash_command, guild_only, default_member_permissions = "ADMINISTRATOR")]
pub async fn security_status(ctx: Context<'_>) -> Result<(), Error> {
    let cfg = &ctx.data().config;
    let embed = CreateEmbed::new()
        .title("🛡️ Overlord Security & Services Dashboard")
        .color(0x3B82F6)
        .field("Min Account Age", format!("{} days", cfg.min_account_age_days), true)
        .field(
            "Honeypot Trap",
            cfg.honeypot_channel_id
                .map(|id| format!("<#{}>", id))
                .unwrap_or_else(|| "Disabled".into()),
            true,
        )
        .field(
            "Mod Log Channel",
            cfg.mod_log_channel_id
                .map(|id| format!("<#{}>", id))
                .unwrap_or_else(|| "Not configured".into()),
            true,
        )
        .field(
            "Transcript Channel",
            cfg.transcript_channel_id
                .map(|id| format!("<#{}>", id))
                .unwrap_or_else(|| "Not configured".into()),
            true,
        )
        .field(
            "Temp Voice Creator",
            cfg.join_to_create_channel_id
                .map(|id| format!("<#{}>", id))
                .unwrap_or_else(|| "Not configured".into()),
            true,
        )
        .field(
            "Verify Channel",
            cfg.verify_channel_id
                .map(|id| format!("<#{}>", id))
                .unwrap_or_else(|| "Not configured".into()),
            true,
        )
        .field(
            "Member Role",
            cfg.member_role_id
                .map(|id| format!("<@&{}>", id))
                .unwrap_or_else(|| "Not set".into()),
            true,
        )
        .field(
            "Staff Role",
            cfg.mod_role_id
                .map(|id| format!("<@&{}>", id))
                .unwrap_or_else(|| "Not set".into()),
            true,
        )
        .timestamp(chrono::Utc::now());

    // Ephemeral: the dashboard names the honeypot channel, which must not be public
    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true)).await?;
    Ok(())
}

async fn event_handler(
    ctx: &serenity::all::Context,
    event: &FullEvent,
    _framework: poise::FrameworkContext<'_, Data, Error>,
    data: &Data,
) -> Result<(), Error> {
    use modules::{moderation, security, tickets, verification, voice};
    use util::log_failure;

    match event {
        FullEvent::GuildMemberAddition { new_member } => {
            // 1. Assign unverified role first so raid alerts never delay it
            if let Some(unverified_id) = data.config.unverified_role_id {
                log_failure(
                    "assign unverified role",
                    new_member.add_role(&ctx.http, RoleId::new(unverified_id)).await,
                );
            }

            // 2. Anti-Raid tracking
            data.raid_tracker.record_join(ctx, data.config.mod_log_channel_id).await;

            // 3. Account safety evaluation
            let check = security::check_member(new_member, data.config.min_account_age_days);
            if check.is_suspicious {
                let embed = CreateEmbed::new()
                    .title("⚠️ Suspicious / Young Account Joined")
                    .color(0xF59E0B)
                    .field("User", format!("{} (`{}`)", new_member.user.tag(), new_member.user.id), false)
                    .field("Account Age", format!("{} days old", check.account_age_days), true)
                    .field("Flag Reason", check.reason.unwrap_or_default(), true)
                    .timestamp(chrono::Utc::now());
                util::send_mod_log(&ctx.http, data.config.mod_log_channel_id, embed).await;
            }
        }

        FullEvent::Message { new_message } => {
            // 1. Honeypot trap check
            if let Some(hp_id) = data.config.honeypot_channel_id {
                log_failure(
                    "honeypot",
                    security::handle_honeypot_message(
                        ctx,
                        new_message,
                        hp_id,
                        data.config.mod_log_channel_id,
                        data.config.mod_role_id,
                    )
                    .await,
                );
            }

            // 2. AutoMod inspection
            log_failure(
                "automod",
                moderation::inspect_message(
                    ctx,
                    new_message,
                    &data.db,
                    data.config.mod_log_channel_id,
                    data.config.mod_role_id,
                )
                .await,
            );
        }

        FullEvent::MessageUpdate { event, .. } => {
            log_failure(
                "automod (edit)",
                moderation::inspect_edit(
                    ctx,
                    event,
                    &data.db,
                    data.config.mod_log_channel_id,
                    data.config.mod_role_id,
                )
                .await,
            );
        }

        FullEvent::InteractionCreate { interaction } => match interaction {
            Interaction::Component(component) => {
                let custom_id = component.data.custom_id.as_str();

                let result = if custom_id == verification::BTN_VERIFY_START {
                    verification::handle_verify_button(ctx, component, &data.verify_codes).await
                } else if custom_id == tickets::TICKET_BTN_GENERAL {
                    tickets::handle_ticket_open_click(ctx, component, "General").await
                } else if custom_id == tickets::TICKET_BTN_REPORT {
                    tickets::handle_ticket_open_click(ctx, component, "Report").await
                } else if custom_id == tickets::TICKET_BTN_BILLING {
                    tickets::handle_ticket_open_click(ctx, component, "VIP & Role").await
                } else if custom_id == tickets::TICKET_BTN_CLAIM {
                    tickets::handle_ticket_claim(ctx, component, &data.config, &data.db).await
                } else if custom_id == tickets::TICKET_BTN_CLOSE {
                    tickets::handle_ticket_close(ctx, component, &data.config, &data.db).await
                } else {
                    Ok(())
                };
                log_failure(&format!("button `{}`", custom_id), result);
            }
            Interaction::Modal(modal) => {
                let custom_id = modal.data.custom_id.as_str();

                let result = if custom_id == verification::MODAL_VERIFY_SUBMIT {
                    verification::handle_verify_modal_submit(ctx, modal, &data.config, &data.verify_codes).await
                } else if custom_id.starts_with(tickets::TICKET_MODAL_CREATE) {
                    let category = custom_id.split(':').nth(1).unwrap_or("General");
                    tickets::handle_ticket_modal_submit(ctx, modal, &data.config, &data.db, category).await
                } else {
                    Ok(())
                };
                log_failure(&format!("modal `{}`", custom_id), result);
            }
            _ => {}
        },

        FullEvent::VoiceStateUpdate { old, new } => {
            log_failure(
                "temp voice",
                voice::handle_voice_state_update(ctx, old.clone(), new.clone(), &data.config, &data.db).await,
            );
        }

        _ => {}
    }

    Ok(())
}

/// Warn at startup about settings that leave a feature switched off, so a missing ID in `.env`
/// is obvious in the logs rather than discovered later.
fn log_config_warnings(config: &Config) {
    let checks = [
        (config.mod_log_channel_id.is_none(), "MOD_LOG_CHANNEL_ID unset: security alerts and mod actions will not be logged"),
        (config.transcript_channel_id.is_none(), "TRANSCRIPT_CHANNEL_ID unset: tickets cannot be closed until it is set"),
        (config.mod_role_id.is_none(), "MOD_ROLE_ID unset: staff without Manage Channels cannot see or manage tickets"),
        (config.member_role_id.is_none(), "MEMBER_ROLE_ID unset: verification will not grant any role"),
        (config.honeypot_channel_id.is_none(), "HONEYPOT_CHANNEL_ID unset: honeypot trap disabled"),
        (config.join_to_create_channel_id.is_none(), "JOIN_TO_CREATE_CHANNEL_ID unset: temporary voice rooms disabled"),
    ];
    for (missing, message) in checks {
        if missing {
            warn!("{}", message);
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "overlord=info,serenity=warn,poise=info".into()),
        )
        .init();

    info!("Starting Discord Overlord Security & Management Suite...");

    let config = Config::from_env()?;
    log_config_warnings(&config);
    let bot_token = config.discord_token.clone();
    let db = database::init_db(&config.database_url).await?;
    info!("SQLite database connection pool established.");

    let intents = GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MEMBERS
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_VOICE_STATES;

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                setup_verification(),
                setup_tickets(),
                security_status(),
                modules::moderation::warn(),
                modules::moderation::timeout(),
                modules::moderation::kick(),
                modules::moderation::ban(),
                modules::moderation::purge(),
                modules::moderation::infractions(),
            ],
            event_handler: |ctx, event, framework, data| {
                Box::pin(event_handler(ctx, event, framework, data))
            },
            ..Default::default()
        })
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                info!("Bot online as {}", _ready.user.tag());

                // Register commands: to target guild for instant testing, or globally
                if let Some(guild_id) = config.guild_id {
                    info!("Registering slash commands to guild ID: {}", guild_id);
                    poise::builtins::register_in_guild(ctx, &framework.options().commands, serenity::all::GuildId::new(guild_id)).await?;
                } else {
                    info!("Registering slash commands globally...");
                    poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                }

                Ok(Data {
                    db,
                    config,
                    raid_tracker: RaidTracker::new(),
                    verify_codes: Default::default(),
                })
            })
        })
        .build();

    let client = serenity::all::ClientBuilder::new(&bot_token, intents)
        .framework(framework)
        .await;

    client?.start().await?;

    Ok(())
}
