use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use serenity::all::{ChannelId, Context, GuildId};
use songbird::events::{Event, EventContext, EventHandler, TrackEvent};
use songbird::input::{ChildContainer, Input};
use songbird::tracks::TrackHandle;
use songbird::Songbird;
use tokio::sync::Mutex;

use super::player::GuildPlayer;
use super::track::AudioPipeline;

/// Shared references needed by the track-end event handler and idle timer.
/// Cloned into each handler instance.
#[derive(Clone)]
pub struct PlaybackContext {
    pub guild_id: GuildId,
    pub songbird: Arc<Songbird>,
    pub guild_players: Arc<DashMap<GuildId, Arc<Mutex<GuildPlayer>>>>,
    pub track_handles: Arc<DashMap<GuildId, TrackHandle>>,
    /// If set, the handle for the current idle-leave timer task.
    /// Stored externally so new tracks can cancel it.
    pub idle_timer_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

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
///
/// If `pctx` is provided, a track-end event listener is registered so the next
/// song in the queue starts automatically when this one finishes.
pub async fn play_track(
    ctx: &Context,
    guild_id: GuildId,
    url: &str,
    pctx: Option<&PlaybackContext>,
) -> Result<TrackHandle, String> {
    let manager = songbird::get(ctx)
        .await
        .ok_or("Songbird not initialized")?
        .clone();

    let call = manager.get(guild_id).ok_or("Not in a voice channel")?;

    let mut pipeline = AudioPipeline::spawn(url)?;
    let children = pipeline.into_children();
    let container = ChildContainer::new(children);
    let input: Input = container.into();

    let mut handler = call.lock().await;
    handler.stop();
    let track_handle = handler.play_input(input);

    // Register the track-end event so the next song plays automatically
    if let Some(pctx) = pctx {
        // Cancel any pending idle timer since we're playing something
        cancel_idle_timer(pctx).await;

        let end_handler = TrackEndHandler {
            pctx: pctx.clone(),
        };
        let _ = track_handle.add_event(Event::Track(TrackEvent::End), end_handler);
    }

    Ok(track_handle)
}

/// Play the next track directly from a PlaybackContext (used by the event handler,
/// which doesn't have a serenity Context).
async fn play_next_from_context(pctx: &PlaybackContext, url: &str) -> Result<TrackHandle, String> {
    let call = pctx
        .songbird
        .get(pctx.guild_id)
        .ok_or("Not in a voice channel")?;

    let mut pipeline = AudioPipeline::spawn(url)?;
    let children = pipeline.into_children();
    let container = ChildContainer::new(children);
    let input: Input = container.into();

    let mut handler = call.lock().await;
    handler.stop();
    let track_handle = handler.play_input(input);

    // Register the same end-of-track event on the new track
    let end_handler = TrackEndHandler {
        pctx: pctx.clone(),
    };
    let _ = track_handle.add_event(Event::Track(TrackEvent::End), end_handler);

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

/// Cancel any pending idle timer for a playback context.
pub async fn cancel_idle_timer(pctx: &PlaybackContext) {
    let mut timer = pctx.idle_timer_handle.lock().await;
    if let Some(handle) = timer.take() {
        handle.abort();
    }
}

/// Start a 5-minute idle timer. When it fires, leave the voice channel and clean up.
fn start_idle_timer(pctx: PlaybackContext) {
    let idle_handle_store = pctx.idle_timer_handle.clone();
    let handle = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(300)).await;
        tracing::info!(
            "Idle timeout for guild {} — leaving voice channel",
            pctx.guild_id
        );

        // Leave the voice channel via songbird
        let _ = pctx.songbird.leave(pctx.guild_id).await;

        // Clean up player state
        if let Some(player_entry) = pctx.guild_players.get(&pctx.guild_id) {
            let player = player_entry.value().clone();
            let mut p = player.lock().await;
            p.leave_empty();
        }
        pctx.guild_players.remove(&pctx.guild_id);
        pctx.track_handles.remove(&pctx.guild_id);
    });

    // Store the handle so cancel_idle_timer can abort it later.
    tokio::spawn(async move {
        let mut timer = idle_handle_store.lock().await;
        *timer = Some(handle);
    });
}

/// Event handler that fires when a track ends.
/// Advances the guild queue and plays the next track, or starts the idle timer.
struct TrackEndHandler {
    pctx: PlaybackContext,
}

#[async_trait]
impl EventHandler for TrackEndHandler {
    async fn act(&self, _ctx: &EventContext<'_>) -> Option<Event> {
        let guild_id = self.pctx.guild_id;

        // Get the player for this guild
        let player_arc = match self.pctx.guild_players.get(&guild_id) {
            Some(entry) => entry.value().clone(),
            None => {
                tracing::debug!("TrackEndHandler: no player for guild {guild_id}");
                return None;
            }
        };

        let mut p = player_arc.lock().await;
        let next_track = p.advance();
        drop(p);

        match next_track {
            Some(track) => {
                tracing::info!(
                    "TrackEndHandler: advancing to next track '{}' in guild {guild_id}",
                    track.title
                );

                match play_next_from_context(&self.pctx, &track.url).await {
                    Ok(handle) => {
                        self.pctx.track_handles.insert(guild_id, handle);
                    }
                    Err(e) => {
                        tracing::error!(
                            "TrackEndHandler: failed to play next track in guild {guild_id}: {e}"
                        );
                        // Clear current track since playback failed
                        if let Some(entry) = self.pctx.guild_players.get(&guild_id) {
                            let mut p = entry.value().lock().await;
                            p.current = None;
                        }
                        self.pctx.track_handles.remove(&guild_id);
                        start_idle_timer(self.pctx.clone());
                    }
                }
            }
            None => {
                tracing::info!(
                    "TrackEndHandler: queue exhausted in guild {guild_id}, starting idle timer"
                );
                self.pctx.track_handles.remove(&guild_id);
                start_idle_timer(self.pctx.clone());
            }
        }

        // Return None to keep the default behavior (event consumed, track is done)
        None
    }
}
