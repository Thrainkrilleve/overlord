mod config;
mod database;
mod guild_settings;
mod modules;
mod util;

use std::collections::HashSet;
use std::sync::Arc;

use config::Config;
use database::DbPool;
use guild_settings::GuildSettings;
use modules::security::RaidTracker;
use serenity::all::{CreateEmbed, CreateMessage, FullEvent, GatewayIntents, GuildId, Interaction};
use tokio::sync::Mutex;
use tracing::{info, warn};

pub struct Data {
    pub db: DbPool,
    pub config: Config,
    /// Guilds whose slash commands were registered during this process.
    pub registered_guilds: Arc<Mutex<HashSet<u64>>>,
    pub raid_tracker: RaidTracker,
    pub verify_codes: modules::verification::VerificationCodes,
}

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;

/// Deploy the human verification gate in this channel
#[poise::command(
    slash_command,
    guild_only,
    default_member_permissions = "ADMINISTRATOR"
)]
pub async fn setup_verification(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let (embed, row) = modules::verification::build_verification_panel();
    let msg = CreateMessage::new().embed(embed).components(vec![row]);
    ctx.channel_id()
        .send_message(&ctx.serenity_context().http, msg)
        .await?;
    if let Some(guild_id) = ctx.guild_id() {
        guild_settings::set_field(
            &ctx.data().db,
            guild_id.get(),
            guild_settings::SettingField::VerifyChannel,
            Some(ctx.channel_id().get() as i64),
        )
        .await?;
    }
    ctx.say("✅ Verification gate panel deployed. This channel is now the verify channel for this server.").await?;
    Ok(())
}

/// Deploy the support ticket hub in this channel
#[poise::command(
    slash_command,
    guild_only,
    default_member_permissions = "ADMINISTRATOR"
)]
pub async fn setup_tickets(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let (embed, row) = modules::tickets::build_ticket_panel();
    let msg = CreateMessage::new().embed(embed).components(vec![row]);
    ctx.channel_id()
        .send_message(&ctx.serenity_context().http, msg)
        .await?;
    ctx.say("✅ Ticket hub panel deployed!").await?;
    Ok(())
}

/// View live Overlord security and configuration status for this server
#[poise::command(
    slash_command,
    guild_only,
    default_member_permissions = "ADMINISTRATOR"
)]
pub async fn security_status(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("Must be run in a server")?;
    let settings = guild_settings::load(&ctx.data().db, guild_id.get()).await?;
    let embed =
        modules::settings::settings_embed(&settings, "🛡️ Overlord Security & Services Dashboard");
    // Ephemeral: the dashboard names the honeypot channel, which must not be public
    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;
    Ok(())
}

async fn register_guild_commands(
    ctx: &serenity::all::Context,
    commands: &[poise::Command<Data, Error>],
    guild_id: GuildId,
    registered: &Mutex<HashSet<u64>>,
) -> Result<(), Error> {
    {
        let mut guard = registered.lock().await;
        if !guard.insert(guild_id.get()) {
            return Ok(());
        }
    }
    if let Err(err) = poise::builtins::register_in_guild(ctx, commands, guild_id).await {
        registered.lock().await.remove(&guild_id.get());
        return Err(err.into());
    }
    info!("Registered slash commands in guild {}", guild_id);
    Ok(())
}

/// Register commands for every server this process is in.
///
/// Guild commands are also registered globally. Discord shows the guild command in place of a
/// global command with the same name, so a server does not list each command twice. Global
/// registration still covers a server whose per-guild update was rate limited. `DEV_GUILD_ONLY`
/// skips both and updates only `GUILD_ID`, which keeps command edits instant while developing.
async fn register_on_startup(
    ctx: &serenity::all::Context,
    commands: &[poise::Command<Data, Error>],
    config: &Config,
    guild_ids: &[GuildId],
    registered: &Mutex<HashSet<u64>>,
) -> Result<(), Error> {
    if config.dev_guild_only {
        let guild_id = GuildId::new(config.guild_id.expect("DEV_GUILD_ONLY requires GUILD_ID"));
        info!(
            "Dev mode: registering slash commands only in guild {}",
            guild_id
        );
        poise::builtins::register_in_guild(ctx, commands, guild_id).await?;
        registered.lock().await.insert(guild_id.get());
        return Ok(());
    }

    info!("Registering slash commands globally");
    if let Err(err) = poise::builtins::register_globally(ctx, commands).await {
        warn!(
            "Global slash command registration failed: {}. Joined servers are still registered individually.",
            err
        );
    }
    for guild_id in guild_ids {
        if let Err(err) = register_guild_commands(ctx, commands, *guild_id, registered).await {
            warn!(
                "Could not register slash commands in guild {}: {}",
                guild_id, err
            );
        }
    }
    Ok(())
}

async fn event_handler(
    ctx: &serenity::all::Context,
    event: &FullEvent,
    framework: poise::FrameworkContext<'_, Data, Error>,
    data: &Data,
) -> Result<(), Error> {
    use modules::{moderation, security, tickets, verification, voice};
    use util::log_failure;

    match event {
        FullEvent::GuildCreate { guild, is_new } => {
            if data.config.dev_guild_only {
                if *is_new == Some(true) && Some(guild.id.get()) != data.config.guild_id {
                    info!(
                        "Joined {} ({}) while DEV_GUILD_ONLY is set. Slash commands were not registered here.",
                        guild.name, guild.id
                    );
                }
            } else if let Err(err) = register_guild_commands(
                ctx,
                &framework.options().commands,
                guild.id,
                &data.registered_guilds,
            )
            .await
            {
                warn!(
                    "Could not register slash commands in {} ({}): {}",
                    guild.name, guild.id, err
                );
            }
        }

        FullEvent::GuildMemberAddition { new_member } => {
            let guild_id = new_member.guild_id;
            let settings = guild_settings::load(&data.db, guild_id.get()).await?;

            // 1. Assign this server's unverified role first so raid alerts never delay it.
            //    A role that belongs to another server is refused.
            if let Some(role) =
                util::role_in_guild(ctx, guild_id, settings.unverified_role_id).await
            {
                log_failure(
                    "assign unverified role",
                    new_member.add_role(&ctx.http, role).await,
                );
            }

            // 2. Anti-raid tracking for this guild only
            let (count, should_alert) = data.raid_tracker.record_join(guild_id.get()).await;
            if should_alert {
                warn!(
                    target: "security::anti_raid",
                    "Potential raid detected in guild {}: {} joins inside the raid window",
                    guild_id,
                    count
                );
                if settings.mod_log_channel_id.is_none() {
                    warn!(
                        "Raid detected in guild {} but no mod log channel is set. Use /config set.",
                        guild_id
                    );
                }
                util::send_mod_log(
                    ctx,
                    guild_id,
                    settings.mod_log_channel_id,
                    security::raid_alert_embed(count),
                )
                .await;
            }

            // 3. Account safety evaluation against this server's minimum age
            let check = security::check_member(new_member, settings.min_account_age_days);
            if check.is_suspicious {
                let embed = CreateEmbed::new()
                    .title("⚠️ Suspicious / Young Account Joined")
                    .color(0xF59E0B)
                    .field(
                        "User",
                        format!("{} (`{}`)", new_member.user.tag(), new_member.user.id),
                        false,
                    )
                    .field(
                        "Account Age",
                        format!("{} days old", check.account_age_days),
                        true,
                    )
                    .field("Flag Reason", check.reason.unwrap_or_default(), true)
                    .timestamp(chrono::Utc::now());
                util::send_mod_log(ctx, guild_id, settings.mod_log_channel_id, embed).await;
            }
        }

        FullEvent::Message { new_message } => {
            log_failure(
                "honeypot",
                security::handle_honeypot_message(ctx, new_message, &data.db).await,
            );
            log_failure(
                "automod",
                moderation::inspect_message(ctx, new_message, &data.db).await,
            );
        }

        FullEvent::MessageUpdate { event, .. } => {
            log_failure(
                "automod (edit)",
                moderation::inspect_edit(ctx, event, &data.db).await,
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
                    tickets::handle_ticket_claim(ctx, component, &data.db).await
                } else if custom_id == tickets::TICKET_BTN_CLOSE {
                    tickets::handle_ticket_close(ctx, component, &data.db).await
                } else {
                    Ok(())
                };
                log_failure(&format!("button `{}`", custom_id), result);
            }
            Interaction::Modal(modal) => {
                let custom_id = modal.data.custom_id.as_str();

                let result = if custom_id == verification::MODAL_VERIFY_SUBMIT {
                    verification::handle_verify_modal_submit(
                        ctx,
                        modal,
                        &data.db,
                        &data.verify_codes,
                    )
                    .await
                } else if custom_id.starts_with(tickets::TICKET_MODAL_CREATE) {
                    let category = custom_id.split(':').nth(1).unwrap_or("General");
                    tickets::handle_ticket_modal_submit(ctx, modal, &data.db, category).await
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
                voice::handle_voice_state_update(ctx, old.clone(), new.clone(), &data.db).await,
            );
        }

        _ => {}
    }

    Ok(())
}

/// Warn at startup about settings that leave a feature switched off for the seeded server.
fn log_settings_warnings(settings: &GuildSettings) {
    let checks = [
        (
            settings.mod_log_channel_id.is_none(),
            "mod log channel is unset, so security alerts and mod actions will not be logged",
        ),
        (
            settings.transcript_channel_id.is_none(),
            "transcript channel is unset, so tickets cannot be closed until it is set",
        ),
        (
            settings.mod_role_id.is_none(),
            "staff role is unset, so staff without Manage Channels cannot see or manage tickets",
        ),
        (
            settings.member_role_id.is_none(),
            "member role is unset, so verification will not grant any role",
        ),
        (
            settings.honeypot_channel_id.is_none(),
            "honeypot channel is unset, so the honeypot trap is disabled",
        ),
        (
            settings.join_to_create_channel_id.is_none(),
            "join-to-create channel is unset, so temporary voice rooms are disabled",
        ),
    ];
    for (missing, message) in checks {
        if missing {
            warn!(
                "Guild {}: {}. Set it with /config set.",
                settings.guild_id, message
            );
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
    if config.dev_guild_only && config.guild_id.is_none() {
        anyhow::bail!("DEV_GUILD_ONLY is set but GUILD_ID is missing. Set GUILD_ID or turn DEV_GUILD_ONLY off.");
    }

    let bot_token = config.discord_token.clone();
    let db = database::init_db(&config.database_url).await?;
    info!("SQLite database connection pool established.");

    if let Some(guild_id) = config.guild_id {
        let seeded = guild_settings::seed_if_absent(&db, guild_id, &config.seed).await?;
        if seeded {
            info!(
                "Saved .env channel, role, and account-age settings for guild {}. Other servers use /config. Later edits to those .env values will not override this guild.",
                guild_id
            );
        } else {
            info!(
                "Guild {} already has settings saved in the database. Channel and role values in .env were left unused. Change them with /config.",
                guild_id
            );
        }
        log_settings_warnings(&guild_settings::load(&db, guild_id).await?);
    } else {
        info!("No GUILD_ID in the environment. Each server configures itself with /config. Nothing was copied from .env.");
    }

    let intents = GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MEMBERS
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_VOICE_STATES;

    let registered_guilds = Arc::new(Mutex::new(HashSet::new()));
    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                setup_verification(),
                setup_tickets(),
                security_status(),
                modules::settings::config(),
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
        .setup(move |ctx, ready, framework| {
            let registered_guilds = Arc::clone(&registered_guilds);
            let bot_tag = ready.user.tag();
            let guild_ids: Vec<GuildId> = ready.guilds.iter().map(|guild| guild.id).collect();
            Box::pin(async move {
                info!("Bot online as {}", bot_tag);
                register_on_startup(
                    ctx,
                    &framework.options().commands,
                    &config,
                    &guild_ids,
                    &registered_guilds,
                )
                .await?;
                Ok(Data {
                    db,
                    config,
                    registered_guilds,
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
