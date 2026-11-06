use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rand::seq::SliceRandom;
use serenity::all::{
    ButtonStyle, ComponentInteraction, Context, CreateActionRow, CreateButton,
    CreateEmbed, CreateInputText, CreateInteractionResponse, CreateInteractionResponseMessage,
    CreateModal, InputTextStyle, ModalInteraction, RoleId, UserId,
};
use tracing::{error, info};
use crate::config::Config;

pub const BTN_VERIFY_START: &str = "btn_verify_identity";
pub const MODAL_VERIFY_SUBMIT: &str = "modal_verify_submit";
pub const INPUT_VERIFY_ANSWER: &str = "input_verify_answer";

const CODE_LEN: usize = 6;
const CODE_TTL: Duration = Duration::from_secs(5 * 60);
// No 0/O, 1/I/L, so the code is unambiguous to read and type
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

/// One-time verification codes issued per user. A fresh code is issued on every button click
/// and consumed by the first modal submission, right or wrong.
#[derive(Clone, Default)]
pub struct VerificationCodes {
    codes: Arc<Mutex<HashMap<UserId, (String, Instant)>>>,
}

impl VerificationCodes {
    fn issue(&self, user_id: UserId) -> String {
        let code = generate_code();
        let now = Instant::now();
        let mut codes = self.codes.lock().unwrap();
        // Drop abandoned codes so the map cannot grow without bound
        codes.retain(|_, (_, issued)| now.duration_since(*issued) < CODE_TTL);
        codes.insert(user_id, (code.clone(), now));
        code
    }

    /// Consume the user's code and report whether `answer` matched it before expiry.
    fn redeem(&self, user_id: UserId, answer: &str) -> bool {
        let Some((code, issued)) = self.codes.lock().unwrap().remove(&user_id) else {
            return false;
        };
        issued.elapsed() < CODE_TTL && answer.trim().eq_ignore_ascii_case(&code)
    }
}

fn generate_code() -> String {
    let mut rng = rand::thread_rng();
    (0..CODE_LEN)
        .map(|_| *CODE_ALPHABET.choose(&mut rng).unwrap() as char)
        .collect()
}

/// Builds the verification message with embed & button
pub fn build_verification_panel() -> (CreateEmbed, CreateActionRow) {
    let embed = CreateEmbed::new()
        .title("🛡️ Server Verification Gate")
        .color(0x3B82F6) // Blue
        .description(
            "Welcome! To prevent automated spam accounts and protect the community, \
            please complete a quick verification check.\n\n\
            Click the button below and type the code you are shown to gain access to the server."
        )
        .footer(serenity::all::CreateEmbedFooter::new("Protected by Overlord Security"));

    let button = CreateButton::new(BTN_VERIFY_START)
        .label("Verify Identity")
        .style(ButtonStyle::Success)
        .emoji('🛡');

    let row = CreateActionRow::Buttons(vec![button]);

    (embed, row)
}

/// Handles when the user clicks the "Verify Identity" button -> displays modal with a fresh code
pub async fn handle_verify_button(
    ctx: &Context,
    interaction: &ComponentInteraction,
    codes: &VerificationCodes,
) -> anyhow::Result<()> {
    let code = codes.issue(interaction.user.id);

    let input = CreateInputText::new(
        InputTextStyle::Short,
        format!("Type this code to confirm: {}", code),
        INPUT_VERIFY_ANSWER,
    )
    .required(true)
    .min_length(CODE_LEN as u16)
    .max_length(CODE_LEN as u16);

    let modal = CreateModal::new(MODAL_VERIFY_SUBMIT, "Human Verification")
        .components(vec![CreateActionRow::InputText(input)]);

    let response = CreateInteractionResponse::Modal(modal);
    interaction.create_response(&ctx.http, response).await?;

    Ok(())
}

/// Handles modal submission from the user
pub async fn handle_verify_modal_submit(
    ctx: &Context,
    interaction: &ModalInteraction,
    config: &Config,
    codes: &VerificationCodes,
) -> anyhow::Result<()> {
    let mut user_input = String::new();

    for row in &interaction.data.components {
        for component in &row.components {
            if let serenity::all::ActionRowComponent::InputText(input) = component {
                if input.custom_id == INPUT_VERIFY_ANSWER {
                    if let Some(val) = &input.value {
                        user_input = val.clone();
                    }
                }
            }
        }
    }

    let guild_id = match interaction.guild_id {
        Some(g) => g,
        None => return Ok(()),
    };

    let user_id = interaction.user.id;

    let (title, description, color) = if !codes.redeem(user_id, &user_input) {
        (
            "❌ Verification Failed",
            "That code was wrong or has expired. Click the button again to get a new code.",
            0xEF4444,
        )
    } else if grant_member_roles(ctx, guild_id, user_id, config).await {
        info!("User {} successfully verified", user_id);
        (
            "✅ Verification Successful",
            "Welcome aboard! Your roles have been assigned and server channels are now unlocked.",
            0x10B981,
        )
    } else {
        (
            "⚠️ Verification Incomplete",
            "Your code was correct, but your role could not be assigned. Please contact a staff member.",
            0xF59E0B,
        )
    };

    let embed = CreateEmbed::new().title(title).description(description).color(color);
    let msg = CreateInteractionResponseMessage::new().embed(embed).ephemeral(true);
    interaction.create_response(&ctx.http, CreateInteractionResponse::Message(msg)).await?;

    Ok(())
}

/// Grant the member role and drop the unverified role. Returns false if the member role
/// could not be granted (the usual cause is the bot's role sitting below the member role).
async fn grant_member_roles(
    ctx: &Context,
    guild_id: serenity::all::GuildId,
    user_id: UserId,
    config: &Config,
) -> bool {
    let reason = Some("Passed Overlord Verification");

    if let Some(member_role) = config.member_role_id {
        if let Err(e) = ctx.http.add_member_role(guild_id, user_id, RoleId::new(member_role), reason).await {
            error!("Failed to grant member role to {}: {}", user_id, e);
            return false;
        }
    }

    if let Some(unverified_role) = config.unverified_role_id {
        if let Err(e) = ctx.http.remove_member_role(guild_id, user_id, RoleId::new(unverified_role), reason).await {
            error!("Failed to remove unverified role from {}: {}", user_id, e);
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_use_only_unambiguous_characters() {
        for _ in 0..100 {
            let code = generate_code();
            assert_eq!(code.len(), CODE_LEN);
            assert!(code.bytes().all(|b| CODE_ALPHABET.contains(&b)), "bad code {}", code);
        }
    }

    #[test]
    fn correct_code_verifies_case_insensitively() {
        let codes = VerificationCodes::default();
        let user = UserId::new(1);
        let code = codes.issue(user);
        assert!(codes.redeem(user, &format!("  {}  ", code.to_lowercase())));
    }

    #[test]
    fn code_is_single_use() {
        let codes = VerificationCodes::default();
        let user = UserId::new(1);
        let code = codes.issue(user);
        assert!(codes.redeem(user, &code));
        assert!(!codes.redeem(user, &code));
    }

    #[test]
    fn wrong_answer_consumes_the_code() {
        let codes = VerificationCodes::default();
        let user = UserId::new(1);
        let code = codes.issue(user);
        assert!(!codes.redeem(user, "WRONG1"));
        assert!(!codes.redeem(user, &code));
    }

    #[test]
    fn codes_are_per_user() {
        // Another user cannot redeem a code that was issued to someone else
        let codes = VerificationCodes::default();
        let code_a = codes.issue(UserId::new(1));
        assert!(!codes.redeem(UserId::new(2), &code_a));
        assert!(codes.redeem(UserId::new(1), &code_a));
    }

    #[test]
    fn the_old_fixed_word_no_longer_passes() {
        let codes = VerificationCodes::default();
        let user = UserId::new(1);
        // "VERIFY" can never be a code: the alphabet has no letter I
        codes.issue(user);
        assert!(!codes.redeem(user, "VERIFY"));
    }
}
