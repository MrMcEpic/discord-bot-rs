use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use serenity::all::{ChannelId, Context, CreateMessage, GuildId, Http, MessageId};
use songbird::driver::Bitrate;
use songbird::events::{Event, EventContext, EventHandler, TrackEvent};
use songbird::input::YoutubeDl;
use songbird::tracks::TrackHandle;
use songbird::Songbird;
use tokio::sync::Mutex;

use super::embeds::{music_controls, now_playing_embed};
use super::player::GuildPlayer;
use super::track::ytdlp_user_args;

/// Shared references needed by the track-end event handler and idle timer.
/// Cloned into each handler instance.
#[derive(Clone)]
pub struct PlaybackContext {
	pub guild_id: GuildId,
	pub channel_id: ChannelId,
	pub songbird: Arc<Songbird>,
	pub serenity_http: Arc<Http>,
	pub http_client: reqwest::Client,
	pub guild_players: Arc<DashMap<GuildId, Arc<Mutex<GuildPlayer>>>>,
	pub track_handles: Arc<DashMap<GuildId, TrackHandle>>,
	/// The most recent "Now Playing" message, so we can delete it when advancing.
	pub now_playing_msg: Arc<Mutex<Option<MessageId>>>,
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

	tracing::info!("Attempting to join voice: guild={guild_id}, channel={channel_id}");

	let _call = manager.join(guild_id, channel_id).await.map_err(|e| {
		tracing::error!("Songbird join failed: {e:?}");
		format!("Failed to join voice: {e}")
	})?;

	tracing::info!("Successfully joined voice channel {channel_id}");

	// Self-deafen and set bitrate to 256kbps (matching TS bot quality)
	{
		let mut handler = _call.lock().await;
		let _ = handler.deafen(true).await;
		handler.set_bitrate(Bitrate::Bits(256_000));
	}

	Ok(())
}

/// Leave a voice channel.
pub async fn leave_channel(ctx: &Context, guild_id: GuildId) {
	if let Some(manager) = songbird::get(ctx).await {
		let _ = manager.leave(guild_id).await;
	}
}

/// Create a YoutubeDl input source with our custom yt-dlp args (cookies, etc).
fn make_ytdl_source(http_client: &reqwest::Client, url: &str) -> YoutubeDl<'static> {
	YoutubeDl::new(http_client.clone(), url.to_string()).user_args(ytdlp_user_args())
}

/// Play audio using songbird's YoutubeDl input (HTTP streaming via yt-dlp).
/// Returns a TrackHandle for controlling playback.
///
/// If `pctx` is provided, a track-end event listener is registered so the next
/// song in the queue starts automatically when this one finishes.
pub async fn play_track(
	ctx: &Context,
	guild_id: GuildId,
	url: &str,
	http_client: &reqwest::Client,
	pctx: Option<&PlaybackContext>,
) -> Result<TrackHandle, String> {
	let manager = songbird::get(ctx)
		.await
		.ok_or("Songbird not initialized")?
		.clone();

	let call = manager.get(guild_id).ok_or("Not in a voice channel")?;

	let source = make_ytdl_source(http_client, url);

	let mut handler = call.lock().await;
	handler.stop();
	let track_handle = handler.play_input(source.into());

	// Register the track-end event so the next song plays automatically
	if let Some(pctx) = pctx {
		// Cancel any pending idle timer since we're playing something
		cancel_idle_timer(pctx).await;

		let end_handler = TrackEndHandler { pctx: pctx.clone() };
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

	let source = make_ytdl_source(&pctx.http_client, url);

	let mut handler = call.lock().await;
	handler.stop();
	let track_handle = handler.play_input(source.into());

	// Register the same end-of-track event on the new track
	let end_handler = TrackEndHandler { pctx: pctx.clone() };
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

				// Delete the old "Now Playing" message
				let old_msg = self.pctx.now_playing_msg.lock().await.take();
				if let Some(msg_id) = old_msg {
					let _ = self
						.pctx
						.channel_id
						.delete_message(&self.pctx.serenity_http, msg_id)
						.await;
				}

				match play_next_from_context(&self.pctx, &track.url).await {
					Ok(handle) => {
						self.pctx.track_handles.insert(guild_id, handle);

						// Send new "Now Playing" embed with controls
						let p = player_arc.lock().await;
						let embed = now_playing_embed(&track);
						let controls = music_controls(false, p.loop_mode);
						drop(p);

						if let Ok(msg) = self
							.pctx
							.channel_id
							.send_message(
								&self.pctx.serenity_http,
								CreateMessage::new().embed(embed).components(controls),
							)
							.await
						{
							*self.pctx.now_playing_msg.lock().await = Some(msg.id);
						}
					}
					Err(e) => {
						tracing::error!(
							"TrackEndHandler: failed to play next track in guild {guild_id}: {e}"
						);
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
