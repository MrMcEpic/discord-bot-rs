use serenity::all::*;

use crate::Data;
use crate::music::voice;

pub async fn handle_voice_state_update(
    ctx: &Context,
    data: &Data,
    old: &Option<VoiceState>,
    _new: &VoiceState,
) {
    // Only trigger when someone leaves a voice channel
    let old_channel_id = match old.as_ref().and_then(|o| o.channel_id) {
        Some(id) => id,
        None => return,
    };

    let guild_id = match old.as_ref().and_then(|o| o.guild_id) {
        Some(id) => id,
        None => return,
    };

    let bot_id = ctx.cache.current_user().id;

    // Check if bot is in this channel
    let guild = match ctx.cache.guild(guild_id) {
        Some(g) => g.clone(),
        None => return,
    };

    let bot_in_channel = guild
        .voice_states
        .get(&bot_id)
        .and_then(|vs| vs.channel_id)
        == Some(old_channel_id);

    if !bot_in_channel {
        return;
    }

    // Count non-bot members in the channel
    let humans = guild
        .voice_states
        .values()
        .filter(|vs| {
            vs.channel_id == Some(old_channel_id)
                && vs.user_id != bot_id
                && !guild
                    .members
                    .get(&vs.user_id)
                    .map_or(false, |m| m.user.bot)
        })
        .count();

    if humans == 0 {
        tracing::info!("Voice channel empty in {} — leaving", guild.name);

        // Cancel any pending idle timer
        if let Some(pctx) = data.playback_context(ctx, guild_id).await {
            voice::cancel_idle_timer(&pctx).await;
        }

        // Clean up player state
        if let Some(player_entry) = data.guild_players.get(&guild_id) {
            let player = player_entry.value().clone();
            let mut p = player.lock().await;
            p.leave_empty();
        }
        data.guild_players.remove(&guild_id);
        data.idle_timers.remove(&guild_id);

        data.track_handles.remove(&guild_id);
        voice::stop_playback(ctx, guild_id).await;
        voice::leave_channel(ctx, guild_id).await;
    }
}
