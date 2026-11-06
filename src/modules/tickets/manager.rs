use serenity::all::{
    ButtonStyle, ChannelId, ChannelType, ComponentInteraction, Context, CreateActionRow,
    CreateButton, CreateChannel, CreateEmbed, CreateInputText, CreateInteractionResponse,
    CreateInteractionResponseFollowup, CreateInteractionResponseMessage, CreateMessage,
    CreateModal, InputTextStyle, ModalInteraction, PermissionOverwrite, PermissionOverwriteType, Permissions, RoleId,
};
use crate::config::Config;
use crate::database::DbPool;
use crate::modules::tickets::transcript;

pub const TICKET_BTN_GENERAL: &str = "ticket_btn_general";
pub const TICKET_BTN_REPORT: &str = "ticket_btn_report";
pub const TICKET_BTN_BILLING: &str = "ticket_btn_billing";

pub const TICKET_BTN_CLAIM: &str = "ticket_btn_claim";
pub const TICKET_BTN_CLOSE: &str = "ticket_btn_close";

pub const TICKET_MODAL_CREATE: &str = "ticket_modal_create";
pub const INPUT_TICKET_SUBJECT: &str = "input_ticket_subject";
pub const INPUT_TICKET_DETAILS: &str = "input_ticket_details";

/// Builds the interactive ticket panel embed & buttons
pub fn build_ticket_panel() -> (CreateEmbed, CreateActionRow) {
    let embed = CreateEmbed::new()
        .title("🎫 Support & Assistance Hub")
        .color(0x6366F1)
        .description(
            "Need assistance from our team? Select an option below to open a private ticket.\n\n\
            • **General Support**: Questions, assistance, feedback\n\
            • **Report User / Scam**: Exploiters, suspicious activity, malicious links\n\
            • **Role / VIP Inquiries**: Role rewards, access issues, inquiries"
        )
        .footer(serenity::all::CreateEmbedFooter::new("Overlord Support System"));

    let b_general = CreateButton::new(TICKET_BTN_GENERAL)
        .label("General Support")
        .style(ButtonStyle::Primary)
        .emoji('📩');

    let b_report = CreateButton::new(TICKET_BTN_REPORT)
        .label("Report User / Scam")
        .style(ButtonStyle::Danger)
        .emoji('🚨');

    let b_billing = CreateButton::new(TICKET_BTN_BILLING)
        .label("Role / VIP")
        .style(ButtonStyle::Secondary)
        .emoji('💎');

    let row = CreateActionRow::Buttons(vec![b_general, b_report, b_billing]);
    (embed, row)
}

/// Whether the clicking member may claim/close tickets: holds the mod role or can manage channels.
fn is_staff(interaction: &ComponentInteraction, config: &Config) -> bool {
    let Some(member) = &interaction.member else { return false };
    let has_mod_role = config
        .mod_role_id
        .is_some_and(|id| member.roles.contains(&RoleId::new(id)));
    let can_manage = member
        .permissions
        .is_some_and(|p| p.contains(Permissions::MANAGE_CHANNELS) || p.contains(Permissions::ADMINISTRATOR));
    has_mod_role || can_manage
}

async fn reject_non_staff(ctx: &Context, interaction: &ComponentInteraction) -> anyhow::Result<()> {
    let msg = CreateInteractionResponseMessage::new()
        .content("⛔ Only support staff can use this control.")
        .ephemeral(true);
    interaction.create_response(&ctx.http, CreateInteractionResponse::Message(msg)).await?;
    Ok(())
}

/// Prompt modal when a user clicks a ticket category button
pub async fn handle_ticket_open_click(
    ctx: &Context,
    interaction: &ComponentInteraction,
    category: &str,
) -> anyhow::Result<()> {
    let subject_input = CreateInputText::new(
        InputTextStyle::Short,
        "Brief Subject:",
        INPUT_TICKET_SUBJECT,
    )
    .placeholder(format!("[{}] Summary of your issue", category))
    .required(true)
    .max_length(100);

    let details_input = CreateInputText::new(
        InputTextStyle::Paragraph,
        "Detailed Explanation:",
        INPUT_TICKET_DETAILS,
    )
    .placeholder("Please explain your question or report with any relevant details...")
    .required(true)
    .max_length(1000);

    let modal = CreateModal::new(format!("{}:{}", TICKET_MODAL_CREATE, category), format!("New Ticket: {}", category))
        .components(vec![
            CreateActionRow::InputText(subject_input),
            CreateActionRow::InputText(details_input),
        ]);

    interaction.create_response(&ctx.http, CreateInteractionResponse::Modal(modal)).await?;
    Ok(())
}

/// Creates the private ticket channel once the modal is submitted
pub async fn handle_ticket_modal_submit(
    ctx: &Context,
    interaction: &ModalInteraction,
    config: &Config,
    db: &DbPool,
    category: &str,
) -> anyhow::Result<()> {
    let guild_id = match interaction.guild_id {
        Some(g) => g,
        None => return Ok(()),
    };

    let user = &interaction.user;

    // One open ticket per user. If the recorded channel was deleted by hand, release it.
    if let Some(existing) = crate::database::find_open_ticket(db, user.id.get(), guild_id.get()).await? {
        if ctx.http.get_channel(ChannelId::new(existing)).await.is_ok() {
            let reply = CreateInteractionResponseMessage::new()
                .content(format!("You already have an open ticket: <#{}>", existing))
                .ephemeral(true);
            interaction.create_response(&ctx.http, CreateInteractionResponse::Message(reply)).await?;
            return Ok(());
        }
        crate::database::close_ticket(db, existing).await?;
    }

    let mut subject = String::new();
    let mut details = String::new();

    for row in &interaction.data.components {
        for comp in &row.components {
            if let serenity::all::ActionRowComponent::InputText(input) = comp {
                match input.custom_id.as_str() {
                    INPUT_TICKET_SUBJECT => subject = input.value.clone().unwrap_or_default(),
                    INPUT_TICKET_DETAILS => details = input.value.clone().unwrap_or_default(),
                    _ => {}
                }
            }
        }
    }

    // Permission overwrites:
    // 1. Deny @everyone view channel
    // 2. Allow user view, send, history
    // 3. Allow mod role view, send, history
    let mut overwrites = vec![
        PermissionOverwrite {
            allow: Permissions::empty(),
            deny: Permissions::VIEW_CHANNEL,
            kind: PermissionOverwriteType::Role(RoleId::new(guild_id.get())), // @everyone role ID is the guild ID
        },
        PermissionOverwrite {
            allow: Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES | Permissions::READ_MESSAGE_HISTORY | Permissions::ATTACH_FILES,
            deny: Permissions::empty(),
            kind: PermissionOverwriteType::Member(user.id),
        },
    ];

    if let Some(mod_role) = config.mod_role_id {
        overwrites.push(PermissionOverwrite {
            allow: Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES | Permissions::READ_MESSAGE_HISTORY | Permissions::ATTACH_FILES,
            deny: Permissions::empty(),
            kind: PermissionOverwriteType::Role(RoleId::new(mod_role)),
        });
    }

    let channel_name = format!("ticket-{}", user.name.to_lowercase().replace(' ', "-"));
    let mut builder = CreateChannel::new(channel_name)
        .kind(ChannelType::Text)
        .permissions(overwrites);

    if let Some(cat_id) = config.ticket_category_id {
        builder = builder.category(ChannelId::new(cat_id));
    }

    let ticket_channel = guild_id.create_channel(&ctx.http, builder).await?;

    // Record in DB
    let ticket_id = crate::database::create_ticket(
        db,
        ticket_channel.id.get(),
        user.id.get(),
        guild_id.get(),
        category,
    )
    .await?;

    // Build in-channel controls
    let embed = CreateEmbed::new()
        .title(format!("Ticket #{} - {}", ticket_id, category))
        .color(0x3B82F6)
        .field("Opened By", format!("<@{}>", user.id), true)
        .field("Category", category, true)
        .field("Subject", subject, false)
        .field("Details", details, false)
        .description("Support staff will be with you shortly. Use the controls below to manage this ticket.")
        .timestamp(chrono::Utc::now());

    let b_claim = CreateButton::new(TICKET_BTN_CLAIM)
        .label("Claim Ticket")
        .style(ButtonStyle::Success)
        .emoji('🙋');

    let b_close = CreateButton::new(TICKET_BTN_CLOSE)
        .label("Close & Archive")
        .style(ButtonStyle::Danger)
        .emoji('🔒');

    let controls = CreateActionRow::Buttons(vec![b_claim, b_close]);
    let initial_msg = CreateMessage::new().content(format!("<@{}> Welcome!", user.id)).embed(embed).components(vec![controls]);
    ticket_channel.send_message(&ctx.http, initial_msg).await?;

    // Respond to user
    let reply = CreateInteractionResponseMessage::new()
        .content(format!("✅ Ticket created! Head over to <#{}>.", ticket_channel.id.get()))
        .ephemeral(true);
    interaction.create_response(&ctx.http, CreateInteractionResponse::Message(reply)).await?;

    Ok(())
}

/// Handles claiming a ticket by a staff member
pub async fn handle_ticket_claim(
    ctx: &Context,
    interaction: &ComponentInteraction,
    config: &Config,
    db: &DbPool,
) -> anyhow::Result<()> {
    if !is_staff(interaction, config) {
        return reject_non_staff(ctx, interaction).await;
    }

    crate::database::claim_ticket(db, interaction.channel_id.get(), interaction.user.id.get()).await?;

    let embed = CreateEmbed::new()
        .description(format!("🙋 Ticket has been claimed by <@{}>.", interaction.user.id.get()))
        .color(0x10B981);

    interaction.create_response(
        &ctx.http,
        CreateInteractionResponse::Message(CreateInteractionResponseMessage::new().embed(embed)),
    ).await?;

    Ok(())
}

/// Handles closing, archiving, and deleting a ticket
pub async fn handle_ticket_close(
    ctx: &Context,
    interaction: &ComponentInteraction,
    config: &Config,
    db: &DbPool,
) -> anyhow::Result<()> {
    if !is_staff(interaction, config) {
        return reject_non_staff(ctx, interaction).await;
    }

    // 1. Respond
    let resp = CreateInteractionResponseMessage::new()
        .content("🔒 Ticket closing. Archiving transcript...")
        .ephemeral(true);
    let _ = interaction.create_response(&ctx.http, CreateInteractionResponse::Message(resp)).await;

    // 2. Generate and upload transcript. Never delete the channel unless it was archived.
    if let Err(e) = transcript::generate_and_archive_transcript(
        ctx,
        db,
        interaction.channel_id,
        config.transcript_channel_id,
        &interaction.user,
    ).await {
        let followup = CreateInteractionResponseFollowup::new()
            .content(format!("⚠️ Transcript could not be archived ({}). The ticket was left open.", e))
            .ephemeral(true);
        interaction.create_followup(&ctx.http, followup).await?;
        return Ok(());
    }

    // 3. Mark closed in DB
    crate::database::close_ticket(db, interaction.channel_id.get()).await?;

    // 4. Delete channel after 2 seconds
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    let _ = interaction.channel_id.delete(&ctx.http).await;

    Ok(())
}
