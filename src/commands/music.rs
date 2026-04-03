use serenity::all::*;

use crate::db::queries::get_guild_settings;
use crate::error::BotError;
use crate::music::embeds::{added_to_queue_embed, music_controls, now_playing_embed, queue_embed, status_footer};
use crate::music::player::{GuildPlayer, LoopMode, MAX_QUEUE_LENGTH};
use crate::music::track::{resolve_track, resolve_tracks};
use crate::music::voice;
use crate::Context;

async fn check_dj_mode(ctx: Context<'_>) -> Result<bool, BotError> {
    let guild_id = match ctx.guild_id() {
        Some(id) => id,
        None => return Ok(false),
    };

    let member = match ctx.author_member().await {
        Some(m) => m.into_owned(),
        None => return Ok(false),
    };

    let perms = ctx.cache()
        .guild(guild_id)
        .map(|guild| guild.member_permissions(&member))
        .unwrap_or(Permissions::empty());
    if perms.contains(Permissions::ADMINISTRATOR) {
        return Ok(false);
    }

    let settings = match get_guild_settings(&ctx.data().db, &guild_id.to_string()).await {
        Some(s) => s,
        None => return Ok(false),
    };

    if !settings.dj_mode_enabled {
        return Ok(false);
    }

    if let Some(ref dj_role_id) = settings.dj_role_id {
        if let Ok(role_id) = dj_role_id.parse::<u64>() {
            if member.roles.contains(&RoleId::new(role_id)) {
                return Ok(false);
            }
        }
    }

    ctx.say("DJ mode is enabled. You need the DJ role to use music commands.").await?;
    Ok(true)
}

fn get_voice_channel(ctx: &Context<'_>) -> Option<ChannelId> {
    let guild_id = ctx.guild_id()?;
    ctx.cache()?.guild(guild_id).and_then(|g| {
        g.voice_states.get(&ctx.author().id).and_then(|vs| vs.channel_id)
    })
}

async fn get_or_create_player(
    ctx: &Context<'_>,
    guild_id: GuildId,
) -> std::sync::Arc<tokio::sync::Mutex<GuildPlayer>> {
    ctx.data()
        .guild_players
        .entry(guild_id)
        .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(GuildPlayer::new(guild_id))))
        .value()
        .clone()
}

/// Play a song
#[poise::command(prefix_command, rename = "play", aliases("p"))]
pub async fn play(
    ctx: Context<'_>,
    #[description = "Song name or URL"] #[rest] query: String,
) -> Result<(), BotError> {
    if check_dj_mode(ctx).await? { return Ok(()); }

    let guild_id = ctx.guild_id().ok_or(BotError::Other("Not in a guild".into()))?;
    let channel_id = match get_voice_channel(&ctx) {
        Some(id) => id,
        None => { ctx.say("You need to be in a voice channel!").await?; return Ok(()); }
    };

    ctx.defer_or_broadcast().await?;

    let display_name = ctx.author_member().await
        .map(|m| m.display_name().to_string())
        .unwrap_or_else(|| ctx.author().name.clone());

    let (track, cookies_stale) = match resolve_track(&query, &display_name).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Track resolve failed: {e}");
            ctx.say("Couldn't find that song. Try a different search.").await?;
            return Ok(());
        }
    };
    if cookies_stale {
        ctx.say("⚠️ YouTube cookies are expired. Music still works but age-restricted content won't. Someone needs to refresh `cookies.txt`.").await?;
    }

    voice::join_channel(ctx.serenity_context(), guild_id, channel_id).await?;

    let player = get_or_create_player(&ctx, guild_id).await;
    let mut p = player.lock().await;

    if p.current.is_some() {
        if p.is_full() {
            ctx.say(format!("Queue is full (max {MAX_QUEUE_LENGTH} songs).")).await?;
            return Ok(());
        }
        let pos = p.enqueue(track.clone());
        ctx.send(poise::CreateReply::default().embed(added_to_queue_embed(&track, pos))).await?;
    } else {
        p.current = Some(track.clone());
        p.paused = false;
        drop(p);

        let pctx = ctx.data().playback_context(ctx.serenity_context(), guild_id, channel_id).await;
        match voice::play_track(ctx.serenity_context(), guild_id, &track.url, &ctx.data().http_client, pctx.as_ref()).await {
            Ok(handle) => { ctx.data().track_handles.insert(guild_id, handle); }
            Err(e) => {
                tracing::error!("Playback error: {e}");
                ctx.say(format!("Playback error: {e}")).await?;
                return Ok(());
            }
        }

        let p = player.lock().await;
        let controls = music_controls(false, p.loop_mode);
        let reply = ctx.send(poise::CreateReply::default().embed(now_playing_embed(&track)).components(controls)).await?;
        // Store the "Now Playing" message ID so TrackEndHandler can delete it
        if let Ok(msg) = reply.message().await {
            if let Some(pctx) = &pctx {
                *pctx.now_playing_msg.lock().await = Some(msg.id);
            }
        }
    }

    Ok(())
}

/// Queue an entire playlist
#[poise::command(prefix_command, rename = "playlist", aliases("pl"))]
pub async fn playlist(
    ctx: Context<'_>,
    #[description = "Playlist URL"] #[rest] query: String,
) -> Result<(), BotError> {
    if check_dj_mode(ctx).await? { return Ok(()); }

    let guild_id = ctx.guild_id().ok_or(BotError::Other("Not in a guild".into()))?;
    let channel_id = match get_voice_channel(&ctx) {
        Some(id) => id,
        None => { ctx.say("You need to be in a voice channel!").await?; return Ok(()); }
    };

    ctx.defer_or_broadcast().await?;

    let display_name = ctx.author_member().await
        .map(|m| m.display_name().to_string())
        .unwrap_or_else(|| ctx.author().name.clone());

    let (tracks, cookies_stale) = match resolve_tracks(&query, &display_name, false).await {
        Ok((t, stale)) if !t.is_empty() => (t, stale),
        Ok(_) => { ctx.say("No tracks found in that playlist.").await?; return Ok(()); }
        Err(e) => {
            tracing::error!("Playlist resolve failed: {e}");
            ctx.say("Couldn't load that playlist.").await?;
            return Ok(());
        }
    };
    if cookies_stale {
        ctx.say("⚠️ YouTube cookies are expired. Music still works but age-restricted content won't. Someone needs to refresh `cookies.txt`.").await?;
    }

    voice::join_channel(ctx.serenity_context(), guild_id, channel_id).await?;

    let player = get_or_create_player(&ctx, guild_id).await;
    let mut p = player.lock().await;

    let mut to_add = tracks;
    let first_track = if p.current.is_none() && !to_add.is_empty() {
        Some(to_add.remove(0))
    } else {
        None
    };

    let added = p.enqueue_many(to_add);

    if let Some(track) = first_track {
        p.current = Some(track.clone());
        p.paused = false;
        drop(p);
        let pctx = ctx.data().playback_context(ctx.serenity_context(), guild_id, channel_id).await;
        if let Ok(handle) = voice::play_track(ctx.serenity_context(), guild_id, &track.url, &ctx.data().http_client, pctx.as_ref()).await {
            ctx.data().track_handles.insert(guild_id, handle);
        }
        let p = player.lock().await;
        let embed = CreateEmbed::new()
            .color(0x57f287)
            .title("Playlist Queued")
            .description(format!("Added **{}** tracks to the queue.", added + 1))
            .field("Now Playing", p.current.as_ref().map_or("Unknown", |t| &t.title), false);
        let controls = music_controls(false, p.loop_mode);
        let reply = ctx.send(poise::CreateReply::default().embed(embed).components(controls)).await?;
        if let Ok(msg) = reply.message().await {
            if let Some(ref pctx) = pctx {
                *pctx.now_playing_msg.lock().await = Some(msg.id);
            }
        }
    } else {
        let embed = CreateEmbed::new()
            .color(0x57f287)
            .title("Playlist Queued")
            .description(format!("Added **{added}** tracks to the queue."));
        ctx.send(poise::CreateReply::default().embed(embed)).await?;
    }

    Ok(())
}

/// Skip the current track
#[poise::command(prefix_command, rename = "skip", aliases("s"))]
pub async fn skip(ctx: Context<'_>) -> Result<(), BotError> {
    if check_dj_mode(ctx).await? { return Ok(()); }

    let guild_id = ctx.guild_id().ok_or(BotError::Other("Not in a guild".into()))?;
    let player = get_or_create_player(&ctx, guild_id).await;
    let mut p = player.lock().await;

    if let Some(title) = p.skip_current() {
        if let Some(next_track) = p.advance() {
            let loop_mode = p.loop_mode;
            drop(p);
            let pctx = ctx.data().playback_context(ctx.serenity_context(), guild_id, ctx.channel_id()).await;
            match voice::play_track(ctx.serenity_context(), guild_id, &next_track.url, &ctx.data().http_client, pctx.as_ref()).await {
                Ok(handle) => { ctx.data().track_handles.insert(guild_id, handle); }
                Err(e) => tracing::error!("Playback error on skip: {e}"),
            }
            ctx.say(format!("Skipped **{title}**.")).await?;
            // Send new "Now Playing" embed with controls
            let embed = now_playing_embed(&next_track);
            let controls = music_controls(false, loop_mode);
            let reply = ctx.send(poise::CreateReply::default().embed(embed).components(controls)).await?;
            if let Ok(msg) = reply.message().await {
                if let Some(ref pctx) = pctx {
                    *pctx.now_playing_msg.lock().await = Some(msg.id);
                }
            }
        } else {
            drop(p);
            voice::stop_playback(ctx.serenity_context(), guild_id).await;
            ctx.data().track_handles.remove(&guild_id);
            ctx.say(format!("Skipped **{title}**. Queue is empty.")).await?;
        }
    } else {
        ctx.say("Nothing is playing right now.").await?;
    }

    Ok(())
}

/// Stop playback and leave voice
#[poise::command(prefix_command, rename = "stop")]
pub async fn stop(ctx: Context<'_>) -> Result<(), BotError> {
    if check_dj_mode(ctx).await? { return Ok(()); }

    let guild_id = ctx.guild_id().ok_or(BotError::Other("Not in a guild".into()))?;
    let player = get_or_create_player(&ctx, guild_id).await;
    let mut p = player.lock().await;
    p.stop_all();
    drop(p);

    // Cancel any pending idle timer
    if let Some(pctx) = ctx.data().playback_context(ctx.serenity_context(), guild_id, ctx.channel_id()).await {
        voice::cancel_idle_timer(&pctx).await;
    }

    ctx.data().track_handles.remove(&guild_id);
    voice::stop_playback(ctx.serenity_context(), guild_id).await;
    voice::leave_channel(ctx.serenity_context(), guild_id).await;

    ctx.say("Stopped playback, cleared queue, and left voice.").await?;
    Ok(())
}

/// Pause playback
#[poise::command(prefix_command, rename = "pause")]
pub async fn pause(ctx: Context<'_>) -> Result<(), BotError> {
    if check_dj_mode(ctx).await? { return Ok(()); }

    let guild_id = ctx.guild_id().ok_or(BotError::Other("Not in a guild".into()))?;
    let player = get_or_create_player(&ctx, guild_id).await;
    let mut p = player.lock().await;

    if p.current.is_some() && !p.paused {
        if let Some(handle) = ctx.data().track_handles.get(&guild_id) {
            let _ = handle.value().pause();
        }
        p.paused = true;
        ctx.say("Paused.").await?;
    } else {
        ctx.say("Nothing is playing right now.").await?;
    }

    Ok(())
}

/// Resume playback
#[poise::command(prefix_command, rename = "resume", aliases("r"))]
pub async fn resume(ctx: Context<'_>) -> Result<(), BotError> {
    if check_dj_mode(ctx).await? { return Ok(()); }

    let guild_id = ctx.guild_id().ok_or(BotError::Other("Not in a guild".into()))?;
    let player = get_or_create_player(&ctx, guild_id).await;
    let mut p = player.lock().await;

    if p.current.is_some() && p.paused {
        if let Some(handle) = ctx.data().track_handles.get(&guild_id) {
            let _ = handle.value().play();
        }
        p.paused = false;
        ctx.say("Resumed.").await?;
    } else {
        ctx.say("Playback is not paused.").await?;
    }

    Ok(())
}

/// Show the current queue
#[poise::command(prefix_command, rename = "queue", aliases("q"))]
pub async fn queue(ctx: Context<'_>) -> Result<(), BotError> {
    let guild_id = ctx.guild_id().ok_or(BotError::Other("Not in a guild".into()))?;
    let player = get_or_create_player(&ctx, guild_id).await;
    let p = player.lock().await;
    let queue_vec: Vec<_> = p.queue.iter().cloned().collect();
    let embed = queue_embed(p.current.as_ref(), &queue_vec);
    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Show what's currently playing
#[poise::command(prefix_command, rename = "nowplaying", aliases("np"))]
pub async fn nowplaying(ctx: Context<'_>) -> Result<(), BotError> {
    let guild_id = ctx.guild_id().ok_or(BotError::Other("Not in a guild".into()))?;
    let player = get_or_create_player(&ctx, guild_id).await;
    let p = player.lock().await;

    if let Some(track) = &p.current {
        let mut embed = now_playing_embed(track);
        if let Some(footer) = status_footer(p.paused, p.loop_mode) {
            embed = embed.footer(footer);
        }
        let controls = music_controls(p.paused, p.loop_mode);
        ctx.send(poise::CreateReply::default().embed(embed).components(controls)).await?;
    } else {
        ctx.say("Nothing is playing right now.").await?;
    }

    Ok(())
}

/// Remove a song from the queue
#[poise::command(prefix_command, rename = "remove")]
pub async fn remove(
    ctx: Context<'_>,
    #[description = "Queue position (1-based)"] position: usize,
) -> Result<(), BotError> {
    if check_dj_mode(ctx).await? { return Ok(()); }

    let guild_id = ctx.guild_id().ok_or(BotError::Other("Not in a guild".into()))?;
    let player = get_or_create_player(&ctx, guild_id).await;
    let mut p = player.lock().await;

    if let Some(removed) = p.remove(position) {
        ctx.say(format!("Removed **{}** from the queue.", removed.title)).await?;
    } else {
        ctx.say(format!(
            "Invalid position. Queue has {} song{}.",
            p.queue.len(),
            if p.queue.len() == 1 { "" } else { "s" }
        )).await?;
    }

    Ok(())
}

/// Toggle loop mode
#[poise::command(prefix_command, rename = "loop", aliases("l"))]
pub async fn loop_cmd(
    ctx: Context<'_>,
    #[description = "Mode: off, track, queue"] mode: Option<String>,
) -> Result<(), BotError> {
    if check_dj_mode(ctx).await? { return Ok(()); }

    let guild_id = ctx.guild_id().ok_or(BotError::Other("Not in a guild".into()))?;
    let player = get_or_create_player(&ctx, guild_id).await;
    let mut p = player.lock().await;

    let new_mode = match mode.as_deref() {
        Some("track") | Some("t") => LoopMode::Track,
        Some("queue") | Some("q") => LoopMode::Queue,
        Some("off") | Some("none") => LoopMode::Off,
        _ => p.loop_mode.cycle(),
    };

    p.loop_mode = new_mode;
    ctx.say(new_mode.label()).await?;
    Ok(())
}

/// Shuffle the queue
#[poise::command(prefix_command, rename = "shuffle")]
pub async fn shuffle(ctx: Context<'_>) -> Result<(), BotError> {
    if check_dj_mode(ctx).await? { return Ok(()); }

    let guild_id = ctx.guild_id().ok_or(BotError::Other("Not in a guild".into()))?;
    let player = get_or_create_player(&ctx, guild_id).await;
    let mut p = player.lock().await;

    let len = p.shuffle();
    if len < 2 {
        ctx.say("Not enough songs in queue to shuffle.").await?;
    } else {
        ctx.say(format!("Shuffled **{len}** songs in the queue.")).await?;
    }

    Ok(())
}
