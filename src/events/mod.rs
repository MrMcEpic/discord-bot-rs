pub mod ready;
pub mod voice_state;

use serenity::all::*;

use crate::ai::deepseek::handle_mention;
use crate::music::embeds::{music_controls, now_playing_embed, queue_embed, status_footer};
use crate::music::voice;
use crate::Data;
use crate::error::BotError;

pub async fn event_handler(
    ctx: &Context,
    event: &poise::serenity_prelude::FullEvent,
    _framework: poise::FrameworkContext<'_, Data, BotError>,
    data: &Data,
) -> Result<(), BotError> {
    match event {
        poise::serenity_prelude::FullEvent::Ready { data_about_bot, .. } => {
            ready::handle_ready(ctx, data_about_bot).await;
        }
        poise::serenity_prelude::FullEvent::VoiceStateUpdate { old, new } => {
            voice_state::handle_voice_state_update(ctx, data, old, new).await;
        }
        poise::serenity_prelude::FullEvent::Message { new_message } => {
            handle_message(ctx, new_message, data).await;
        }
        poise::serenity_prelude::FullEvent::InteractionCreate {
            interaction: Interaction::Component(interaction),
        } => {
            handle_component_interaction(ctx, interaction, data).await;
        }
        _ => {}
    }

    Ok(())
}

async fn handle_message(ctx: &Context, message: &Message, data: &Data) {
    if message.author.bot || message.guild_id.is_none() {
        return;
    }

    let bot_id = ctx.cache.current_user().id;

    let is_mention = message.mentions.iter().any(|u| u.id == bot_id);

    let is_reply_to_bot = if let Some(ref reference) = message.message_reference {
        if let Some(msg_id) = reference.message_id {
            match message.channel_id.message(&ctx.http, msg_id).await {
                Ok(ref_msg) => ref_msg.author.id == bot_id,
                Err(_) => false,
            }
        } else {
            false
        }
    } else {
        false
    };

    if data.config.deepseek_api_key.is_some() && (is_mention || is_reply_to_bot) {
        handle_mention(ctx, message, data).await;
    }
}

async fn handle_component_interaction(
    ctx: &Context,
    interaction: &ComponentInteraction,
    data: &Data,
) {
    let custom_id = &interaction.data.custom_id;

    if !custom_id.starts_with("music_") {
        return;
    }

    let guild_id = match interaction.guild_id {
        Some(id) => id,
        None => return,
    };

    let player_arc = match data.guild_players.get(&guild_id) {
        Some(entry) => entry.value().clone(),
        None => {
            let _ = interaction
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .content("No active player.")
                            .ephemeral(true),
                    ),
                )
                .await;
            return;
        }
    };

    match custom_id.as_str() {
        "music_pauseresume" => {
            let mut p = player_arc.lock().await;
            if p.current.is_none() {
                let _ = interaction
                    .create_response(
                        &ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content("Nothing is playing right now.")
                                .ephemeral(true),
                        ),
                    )
                    .await;
                return;
            }

            // Use stored track handle for pause/resume
            if let Some(handle_entry) = data.track_handles.get(&guild_id) {
                if p.paused {
                    let _ = handle_entry.value().play();
                    p.paused = false;
                } else {
                    let _ = handle_entry.value().pause();
                    p.paused = true;
                }
            }

            let mut embed = now_playing_embed(p.current.as_ref().unwrap());
            if let Some(footer) = status_footer(p.paused, p.loop_mode) {
                embed = embed.footer(footer);
            }
            let controls = music_controls(p.paused, p.loop_mode);

            let _ = interaction
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new()
                            .embed(embed)
                            .components(controls),
                    ),
                )
                .await;
        }
        "music_skip" => {
            let mut p = player_arc.lock().await;
            if let Some(title) = p.skip_current() {
                if let Some(next_track) = p.advance() {
                    drop(p);
                    match voice::play_track(ctx, guild_id, &next_track.url).await {
                        Ok(handle) => {
                            data.track_handles.insert(guild_id, handle);
                        }
                        Err(e) => tracing::error!("Playback error on skip: {e}"),
                    }
                } else {
                    drop(p);
                    voice::stop_playback(ctx, guild_id).await;
                    data.track_handles.remove(&guild_id);
                }
                let _ = interaction
                    .create_response(
                        &ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content(format!("⏭️ Skipped **{title}**.")),
                        ),
                    )
                    .await;
            } else {
                let _ = interaction
                    .create_response(
                        &ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content("Nothing to skip.")
                                .ephemeral(true),
                        ),
                    )
                    .await;
            }
        }
        "music_stop" => {
            let mut p = player_arc.lock().await;
            p.stop_all();
            drop(p);

            data.track_handles.remove(&guild_id);
            voice::stop_playback(ctx, guild_id).await;
            voice::leave_channel(ctx, guild_id).await;

            let embed = CreateEmbed::new()
                .color(0xed4245)
                .title("Playback Stopped")
                .description("Queue cleared. Left voice channel.");

            let _ = interaction
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new()
                            .embed(embed)
                            .components(vec![]),
                    ),
                )
                .await;
        }
        "music_shuffle" => {
            let mut p = player_arc.lock().await;
            let len = p.shuffle();
            if len < 2 {
                let _ = interaction
                    .create_response(
                        &ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content("Not enough songs in queue to shuffle.")
                                .ephemeral(true),
                        ),
                    )
                    .await;
            } else {
                let _ = interaction
                    .create_response(
                        &ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content(format!("🔀 Shuffled **{len}** songs in the queue.")),
                        ),
                    )
                    .await;
            }
        }
        "music_loop" => {
            let mut p = player_arc.lock().await;
            p.loop_mode = p.loop_mode.cycle();

            let mut embed_opt = None;
            if let Some(track) = &p.current {
                let mut embed = now_playing_embed(track);
                if let Some(footer) = status_footer(p.paused, p.loop_mode) {
                    embed = embed.footer(footer);
                }
                embed_opt = Some(embed);
            }
            let controls = music_controls(p.paused, p.loop_mode);

            let mut response = CreateInteractionResponseMessage::new().components(controls);
            if let Some(embed) = embed_opt {
                response = response.embed(embed);
            }

            let _ = interaction
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::UpdateMessage(response),
                )
                .await;
        }
        "music_queue" => {
            let p = player_arc.lock().await;
            let queue_vec: Vec<_> = p.queue.iter().cloned().collect();
            let embed = queue_embed(p.current.as_ref(), &queue_vec);

            let _ = interaction
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .embed(embed)
                            .ephemeral(true),
                    ),
                )
                .await;
        }
        _ => {}
    }
}
