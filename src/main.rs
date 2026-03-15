mod ai;
mod commands;
mod config;
mod connections;
mod db;
mod error;
mod events;
mod music;
mod stocks;
mod util;

use connections::game::ConnectionsGame;
use dashmap::DashMap;
use music::player::GuildPlayer;
use music::voice::PlaybackContext;
use serenity::all::*;
use songbird::SerenityInit;
use songbird::tracks::TrackHandle;
use std::sync::Arc;
use tokio::sync::Mutex;

use config::Config;
use error::BotError;
use util::ratelimit::RateLimiters;

type IdleTimerMap = Arc<DashMap<GuildId, Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>>>;

pub struct Data {
    pub db: sqlx::PgPool,
    pub http_client: reqwest::Client,
    pub guild_players: Arc<DashMap<GuildId, Arc<Mutex<GuildPlayer>>>>,
    pub track_handles: Arc<DashMap<GuildId, TrackHandle>>,
    /// Per-guild "Now Playing" message IDs, so we can delete old ones when advancing.
    pub now_playing_msgs: Arc<DashMap<GuildId, Arc<Mutex<Option<serenity::all::MessageId>>>>>,
    /// Per-guild idle timer handles, used to cancel idle-leave when a new track starts.
    pub idle_timers: IdleTimerMap,
    pub rate_limiters: RateLimiters,
    pub connections_games: Arc<DashMap<ChannelId, Arc<Mutex<ConnectionsGame>>>>,
    pub config: Config,
    /// When this bot instance started — bot messages before this are from a previous instance.
    pub started_at: chrono::DateTime<chrono::Utc>,
}

impl Data {
    /// Build a `PlaybackContext` for the given guild, suitable for passing to
    /// `voice::play_track` so track-end events can advance the queue.
    pub async fn playback_context(
        &self,
        ctx: &serenity::all::prelude::Context,
        guild_id: GuildId,
        channel_id: ChannelId,
    ) -> Option<PlaybackContext> {
        let songbird = songbird::get(ctx).await?;
        let idle_timer_handle = self
            .idle_timers
            .entry(guild_id)
            .or_insert_with(|| Arc::new(Mutex::new(None)))
            .value()
            .clone();
        let now_playing_msg = self
            .now_playing_msgs
            .entry(guild_id)
            .or_insert_with(|| Arc::new(Mutex::new(None)))
            .value()
            .clone();
        Some(PlaybackContext {
            guild_id,
            channel_id,
            songbird,
            serenity_http: ctx.http.clone(),
            http_client: self.http_client.clone(),
            guild_players: self.guild_players.clone(),
            track_handles: self.track_handles.clone(),
            now_playing_msg,
            idle_timer_handle,
        })
    }
}

pub type Context<'a> = poise::Context<'a, Data, BotError>;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Remove PM2's node IPC env vars — they cause child node processes
    // (spawned by yt-dlp for JS challenge solving) to crash with SIGABRT.
    std::env::remove_var("NODE_CHANNEL_FD");
    std::env::remove_var("NODE_CHANNEL_SERIALIZATION_MODE");

    let config = Config::load();
    tracing::info!("Config loaded. Client ID: {}", config.client_id);

    let db = db::init_pool(&config.database_url)
        .await
        .expect("Failed to connect to database");

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("HTTP client");
    let rate_limiters = RateLimiters::new();

    let intents = GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_VOICE_STATES
        | GatewayIntents::GUILD_MEMBERS;

    let token = config.token.clone();

    let db_clone = db.clone();

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some("!".to_string()),
                mention_as_prefix: false,
                ..Default::default()
            },
            commands: vec![commands::m()],
            event_handler: |ctx, event, framework, data| {
                Box::pin(events::event_handler(ctx, event, framework, data))
            },
            on_error: |error| {
                Box::pin(async move {
                    match error {
                        poise::FrameworkError::Command { error, ctx, .. } => {
                            tracing::error!("Command error: {error}");
                            let _ = ctx.say(format!("Error: {error}")).await;
                        }
                        other => {
                            tracing::error!("Framework error: {other}");
                        }
                    }
                })
            },
            ..Default::default()
        })
        .setup(move |_ctx, _ready, _framework| {
            Box::pin(async move {
                Ok(Data {
                    db,
                    http_client,
                    guild_players: Arc::new(DashMap::new()),
                    track_handles: Arc::new(DashMap::new()),
                    now_playing_msgs: Arc::new(DashMap::new()),
                    idle_timers: Arc::new(DashMap::new()),
                    rate_limiters,
                    connections_games: Arc::new(DashMap::new()),
                    config,
                    started_at: chrono::Utc::now(),
                })
            })
        })
        .build();

    let mut client = ClientBuilder::new(&token, intents)
        .framework(framework)
        .register_songbird()
        .await
        .expect("Failed to create client");

    // Spawn background tasks
    let http = client.http.clone();
    let db_for_unban = db_clone.clone();
    tokio::spawn(async move {
        // Wait for bot to be ready
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        tracing::info!("Tempban unban checker started (30s interval).");
        loop {
            if let Ok(expired) = db::queries::get_expired_bans(&db_for_unban).await {
                for ban in expired {
                    if let Ok(guild_id) = ban.guild_id.parse::<u64>() {
                        if let Ok(user_id) = ban.user_id.parse::<u64>() {
                            let _ = http
                                .remove_ban(GuildId::new(guild_id), UserId::new(user_id), Some("Tempban expired"))
                                .await;
                            let _ = db::queries::mark_unbanned_by_id(&db_for_unban, ban.id).await;
                            tracing::info!(
                                "Auto-unbanned {} from guild {}",
                                ban.user_id,
                                ban.guild_id
                            );
                        }
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        }
    });

    // Rate limiter cleanup every 5 minutes
    // (can't easily access data here, so we'll do it in the event handler)

    tracing::info!("Starting bot...");
    if let Err(e) = client.start().await {
        tracing::error!("Client error: {e}");
    }
}
