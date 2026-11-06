use serenity::all::{
    ChannelId, ChannelType, Context, CreateChannel, VoiceState,
};
use tracing::{error, info};
use crate::config::Config;
use crate::database::DbPool;

pub async fn handle_voice_state_update(
    ctx: &Context,
    old: Option<VoiceState>,
    new: VoiceState,
    config: &Config,
    db: &DbPool,
) -> anyhow::Result<()> {
    let guild_id = match new.guild_id.or_else(|| old.as_ref().and_then(|o| o.guild_id)) {
        Some(g) => g,
        None => return Ok(()),
    };

    // 1. Check if user joined the "Join to Create" channel
    if let Some(master_id) = config.join_to_create_channel_id {
        if let Some(new_ch) = new.channel_id {
            if new_ch.get() == master_id {
                let user_id = new.user_id;
                let user = user_id.to_user(&ctx.http).await?;
                let room_name = format!("🔊 {}'s Room", user.name);

                let mut builder = CreateChannel::new(room_name).kind(ChannelType::Voice);
                if let Some(cat_id) = config.voice_category_id {
                    builder = builder.category(ChannelId::new(cat_id));
                }

                match guild_id.create_channel(&ctx.http, builder).await {
                    Ok(created) => {
                        // Record before moving so the user's later departure is recognised
                        let _ = crate::database::record_temp_voice(db, created.id.get(), user_id.get()).await;

                        // If the user already left, nobody will ever leave this room, so remove it now
                        if let Err(e) = guild_id.move_member(&ctx.http, user_id, created.id).await {
                            info!("Could not move {} into new room ({:?}); removing it", user.name, e);
                            let _ = created.id.delete(&ctx.http).await;
                            let _ = crate::database::remove_temp_voice(db, created.id.get()).await;
                        } else {
                            info!("Created temp voice room {} for user {}", created.id, user.name);
                        }
                    }
                    Err(e) => {
                        error!("Failed to create temp voice channel: {:?}", e);
                    }
                }
            }
        }
    }

    // 2. Check if user left a temp voice room and if it's now empty
    if let Some(old_state) = old {
        if let Some(old_ch) = old_state.channel_id {
            if crate::database::is_temp_voice(db, old_ch.get()).await.unwrap_or(false) {
                // Check member count in guild cache
                let is_empty = if let Some(guild) = ctx.cache.guild(guild_id) {
                    let count = guild
                        .voice_states
                        .values()
                        .filter(|vs| vs.channel_id == Some(old_ch))
                        .count();
                    count == 0
                } else {
                    false
                };

                if is_empty {
                    info!("Temp voice channel {} is empty, deleting...", old_ch);
                    let _ = old_ch.delete(&ctx.http).await;
                    let _ = crate::database::remove_temp_voice(db, old_ch.get()).await;
                }
            }
        }
    }

    Ok(())
}
