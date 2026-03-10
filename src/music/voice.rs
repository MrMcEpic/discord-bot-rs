use serenity::all::{ChannelId, Context, GuildId};
use songbird::input::{ChildContainer, Input};
use songbird::tracks::TrackHandle;

use super::track::AudioPipeline;

/// Join a voice channel.
pub async fn join_channel(
    ctx: &Context,
    guild_id: GuildId,
    channel_id: ChannelId,
) -> Result<(), String> {
    let manager = songbird::get(ctx)
        .await
        .ok_or("Songbird not initialized")?
        .clone();

    let _call = manager
        .join(guild_id, channel_id)
        .await
        .map_err(|e| format!("Failed to join voice: {e}"))?;

    // Self-deafen
    {
        let mut handler = _call.lock().await;
        let _ = handler.deafen(true).await;
    }

    Ok(())
}

/// Leave a voice channel.
pub async fn leave_channel(ctx: &Context, guild_id: GuildId) {
    if let Some(manager) = songbird::get(ctx).await {
        let _ = manager.leave(guild_id).await;
    }
}

/// Play audio using our yt-dlp|ffmpeg pipeline via songbird ChildContainer.
/// Returns a TrackHandle for controlling playback.
pub async fn play_track(
    ctx: &Context,
    guild_id: GuildId,
    url: &str,
) -> Result<TrackHandle, String> {
    let manager = songbird::get(ctx)
        .await
        .ok_or("Songbird not initialized")?
        .clone();

    let call = manager.get(guild_id).ok_or("Not in a voice channel")?;

    let pipeline = AudioPipeline::spawn(url)?;
    let children = pipeline.into_children();
    let container = ChildContainer::new(children);
    let input: Input = container.into();

    let mut handler = call.lock().await;
    handler.stop();
    let track_handle = handler.play_input(input);

    Ok(track_handle)
}

/// Stop playback in a guild
pub async fn stop_playback(ctx: &Context, guild_id: GuildId) {
    if let Some(manager) = songbird::get(ctx).await {
        if let Some(call) = manager.get(guild_id) {
            let mut handler = call.lock().await;
            handler.stop();
        }
    }
}
