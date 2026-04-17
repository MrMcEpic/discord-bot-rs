mod ai;
mod autorole;
mod commands;
mod config;
mod connections;
mod db;
mod error;
mod events;
mod instance_config;
mod mcp;
mod minecraft;
mod music;
mod stocks;
mod util;
mod wordle;

use connections::game::ConnectionsGame;
use dashmap::DashMap;
use futures::future::FutureExt;
use music::player::GuildPlayer;
use music::voice::PlaybackContext;
use serenity::all::*;
use songbird::tracks::TrackHandle;
use songbird::SerenityInit;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use wordle::game::WordleGame;

/// Run a single iteration body with panic recovery. Logs and swallows any
/// panic so a transient crash inside a long-running loop doesn't kill the
/// whole task. Use this around the *body* of every background loop iteration.
pub async fn run_supervised<F, Fut>(task_name: &'static str, body: F)
where
	F: FnOnce() -> Fut,
	Fut: std::future::Future<Output = ()>,
{
	if let Err(panic) = AssertUnwindSafe(body()).catch_unwind().await {
		let msg = if let Some(s) = panic.downcast_ref::<&'static str>() {
			(*s).to_string()
		} else if let Some(s) = panic.downcast_ref::<String>() {
			s.clone()
		} else {
			"<non-string panic payload>".to_string()
		};
		tracing::error!(
			task = task_name,
			panic = %msg,
			"Background task iteration panicked; loop will continue with next iteration"
		);
	}
}

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
	pub rate_limiters: Arc<RateLimiters>,
	pub connections_games: Arc<DashMap<ChannelId, Arc<Mutex<ConnectionsGame>>>>,
	pub wordle_games: Arc<DashMap<ChannelId, Arc<Mutex<WordleGame>>>>,
	pub config: Config,
	/// Capability-routed AI providers built once at startup from `config`.
	/// Replaces the old inline `ApiEndpoint { url, model, api_key }` literals
	/// scattered through `ai/chat.rs`. See `src/ai/providers/mod.rs`.
	pub ai_router: ai::providers::ProviderRouter,
	/// Ordered provider names to try when the primary provider returns the
	/// `CENSORED` sentinel. Resolved from `instance_config.ai.fallback.on_censored`
	/// at startup. Empty = no cascade (snarky-reply canned behaviour preserved).
	pub ai_fallback_on_censored: Vec<String>,
	pub personality: String,
	pub bot_name: String,
	/// Pre-rendered command invocation prefix used by the help embed and any
	/// other code that needs to print example commands. Equal to
	/// `command_prefix + command_root + " "` for the default case (`"!m "`)
	/// or just `command_prefix` when `command_root` is empty (`"!"`).
	pub cmd_prefix: String,
	pub auto_role_config: Option<instance_config::AutoRoleConfig>,
	pub minecraft_config: Option<instance_config::MinecraftConfig>,
	pub join_role_config: Option<instance_config::JoinRoleConfig>,
	pub welcome_config: Option<instance_config::WelcomeConfig>,
	pub welcome_prompt: Option<String>,
	pub mc_verify_url: Option<String>,
	pub mc_verify_secret: Option<String>,
	pub mcp_started: AtomicBool,
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

	let db = db::init_pool(&config.database_url, &config.db_schema)
		.await
		.expect("Failed to connect to database");

	let http_client = reqwest::Client::builder()
		.timeout(std::time::Duration::from_secs(30))
		.build()
		.expect("HTTP client");
	let rate_limiters = Arc::new(RateLimiters::new());
	let rate_limiters_for_cleanup = rate_limiters.clone();

	let intents = GatewayIntents::GUILDS
		| GatewayIntents::GUILD_MESSAGES
		| GatewayIntents::MESSAGE_CONTENT
		| GatewayIntents::GUILD_VOICE_STATES
		| GatewayIntents::GUILD_MEMBERS;

	let config_dir = instance_config::InstanceConfig::config_dir();
	let instance_cfg = instance_config::InstanceConfig::load(&config_dir);
	let personality = instance_cfg.load_personality(&config_dir);
	tracing::info!(
		"Instance config loaded: {} (prefix: {})",
		instance_cfg.bot_name,
		instance_cfg.command_prefix
	);

	let auto_role_config = if instance_cfg.features.auto_role {
		match &instance_cfg.auto_role {
			Some(cfg) => {
				tracing::info!("Auto-role module enabled (from={}, to={}, min_age={}, min_messages={}, require_all={})",
                    cfg.from_role, cfg.to_role, cfg.min_age, cfg.min_messages, cfg.require_all);
				Some(cfg.clone())
			}
			None => {
				tracing::warn!("Auto-role feature enabled but [auto_role] config section missing");
				None
			}
		}
	} else {
		None
	};

	let minecraft_config = if instance_cfg.features.minecraft {
		match &instance_cfg.minecraft {
			Some(mc) => {
				if mc.donator_sync {
					match &mc.donator_sync_config {
                        Some(ds) => tracing::info!("Donator sync enabled (supporter={}, premium={}, interval={}s)",
                            ds.supporter_role, ds.premium_role, ds.check_interval),
                        None => tracing::warn!("minecraft.donator_sync = true but [minecraft.donator_sync_config] missing"),
                    }
				}
				if mc.chargeback {
					match &mc.chargeback_config {
						Some(cb) => tracing::info!(
							"Chargeback alerts enabled (staff_channel={}, restricted_role={})",
							cb.staff_channel,
							cb.restricted_role
						),
						None => tracing::warn!(
							"minecraft.chargeback = true but [minecraft.chargeback_config] missing"
						),
					}
				}
				if mc.verify {
					tracing::info!("Minecraft verification module enabled");
				}
				Some(mc.clone())
			}
			None => {
				tracing::warn!("features.minecraft = true but [minecraft] config section missing");
				None
			}
		}
	} else {
		None
	};

	let join_role_config = if instance_cfg.features.join_role {
		match &instance_cfg.join_role {
			Some(cfg) => {
				tracing::info!("Join-role module enabled (role={})", cfg.role);
				Some(cfg.clone())
			}
			None => {
				tracing::warn!("Join-role feature enabled but [join_role] config section missing");
				None
			}
		}
	} else {
		None
	};

	let welcome_prompt = if instance_cfg.features.welcome {
		match instance_cfg.load_welcome_prompt(&config_dir) {
			Some(prompt) => {
				tracing::info!("Welcome module enabled");
				Some(prompt)
			}
			None => {
				tracing::warn!("Welcome feature enabled but prompt file missing or empty");
				None
			}
		}
	} else {
		None
	};

	let welcome_config = if instance_cfg.features.welcome && welcome_prompt.is_some() {
		instance_cfg.welcome.clone()
	} else {
		None
	};

	if welcome_config.is_some()
		&& config.deepseek_api_key.is_none()
		&& config.gemini_api_key.is_none()
	{
		tracing::warn!("Welcome feature enabled but no AI API key (DEEPSEEK_API_KEY or GEMINI_API_KEY) configured");
	}

	let token = config.token.clone();
	let guild_id_for_tasks = config.guild_id.clone();
	let mc_verify_url_for_tasks = config.mc_verify_url.clone();
	let mc_verify_secret_for_tasks = config.mc_verify_secret.clone();

	let db_clone = db.clone();

	let framework = poise::Framework::builder()
		.options(poise::FrameworkOptions {
			prefix_options: poise::PrefixFrameworkOptions {
				prefix: Some(instance_cfg.command_prefix.clone()),
				mention_as_prefix: false,
				..Default::default()
			},
			commands: {
				let mut m_cmd = commands::m();
				if instance_cfg.features.minecraft {
					if let Some(ref mc) = instance_cfg.minecraft {
						if mc.verify {
							m_cmd.subcommands.push(commands::minecraft::verify());
						}
					}
				}
				// command_root configurable at runtime — see instance_config.rs.
				// "m" (default): register the wrapper as `m` so users invoke
				//   <prefix>m <subcommand>. Existing behaviour.
				// custom name (e.g. "bot"): rename the wrapper so two bots in
				//   the same guild can be reached at distinct paths.
				// "" (empty): skip the wrapper entirely; promote each child
				//   to the root command list so users invoke <prefix><subcommand>.
				if instance_cfg.command_root.is_empty() {
					m_cmd.subcommands
				} else {
					m_cmd.name = instance_cfg.command_root.clone();
					vec![m_cmd]
				}
			},
			event_handler: |ctx, event, framework, data| {
				Box::pin(events::event_handler(ctx, event, framework, data))
			},
			on_error: |error| {
				Box::pin(async move {
					match error {
						poise::FrameworkError::Command { error, ctx, .. } => {
							tracing::error!("Command error: {error}");
							let _ = ctx.say(format!("Error: {}", error.user_message())).await;
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
				let mc_verify_url = config.mc_verify_url.clone();
				let mc_verify_secret = config.mc_verify_secret.clone();
				let ai_router = ai::providers::ProviderRouter::from_config(&config);
				let ai_fallback_on_censored = instance_cfg.ai.fallback.on_censored.clone();
				// Resolve once at startup so unknown / unconfigured names log a
				// warning here, not on every CENSORED-cascade attempt.
				let _ = ai_router.cascade_for(&ai_fallback_on_censored);
				Ok(Data {
					db,
					http_client,
					guild_players: Arc::new(DashMap::new()),
					track_handles: Arc::new(DashMap::new()),
					now_playing_msgs: Arc::new(DashMap::new()),
					idle_timers: Arc::new(DashMap::new()),
					rate_limiters,
					connections_games: Arc::new(DashMap::new()),
					wordle_games: Arc::new(DashMap::new()),
					config,
					ai_router,
					ai_fallback_on_censored,
					personality,
					bot_name: instance_cfg.bot_name.clone(),
					cmd_prefix: if instance_cfg.command_root.is_empty() {
						instance_cfg.command_prefix.clone()
					} else {
						format!(
							"{}{} ",
							instance_cfg.command_prefix, instance_cfg.command_root
						)
					},
					auto_role_config,
					minecraft_config,
					join_role_config,
					welcome_config,
					welcome_prompt,
					mc_verify_url,
					mc_verify_secret,
					mcp_started: AtomicBool::new(false),
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

	// Spawn background tasks. We track them in a JoinSet so shutdown can
	// observe (and a future Tier 2.3 rate-limiter task can join here too).
	// Each loop body is wrapped in `run_supervised` so a panic in one
	// iteration is logged but doesn't kill the loop.
	let mut background_tasks: JoinSet<()> = JoinSet::new();

	// Rate limiter cleanup: prune empty/expired buckets every 5 minutes.
	// Each `check()` inserts into a per-user DashMap entry that's never
	// removed on its own; without this, memory grows with unique-user count.
	background_tasks.spawn(async move {
		// Stagger initial wait so this doesn't fight startup work.
		tokio::time::sleep(std::time::Duration::from_secs(60)).await;
		tracing::info!("Rate limiter cleanup task started (300s interval).");
		loop {
			run_supervised("rate_limiter_cleanup", || async {
				rate_limiters_for_cleanup.cleanup_all();
			})
			.await;
			tokio::time::sleep(std::time::Duration::from_secs(300)).await;
		}
	});

	let http = client.http.clone();
	let db_for_unban = db_clone.clone();
	background_tasks.spawn(async move {
		// Wait for bot to be ready
		tokio::time::sleep(std::time::Duration::from_secs(5)).await;
		tracing::info!("Tempban unban checker started (30s interval).");
		loop {
			run_supervised("tempban_unban", || async {
				if let Ok(expired) = db::queries::get_expired_bans(&db_for_unban).await {
					for ban in expired {
						if let Ok(guild_id) = ban.guild_id.parse::<u64>() {
							if let Ok(user_id) = ban.user_id.parse::<u64>() {
								let _ = http
									.remove_ban(
										GuildId::new(guild_id),
										UserId::new(user_id),
										Some("Tempban expired"),
									)
									.await;
								let _ =
									db::queries::mark_unbanned_by_id(&db_for_unban, ban.id).await;
								tracing::info!(
									"Auto-unbanned {} from guild {}",
									ban.user_id,
									ban.guild_id
								);
							}
						}
					}
				}
			})
			.await;
			tokio::time::sleep(std::time::Duration::from_secs(30)).await;
		}
	});

	// Auto-role background task: check time-based promotions every 60s
	if let Some(ref ar_config) = instance_cfg.auto_role {
		if instance_cfg.features.auto_role {
			let http_ar = client.http.clone();
			let db_ar = db_clone.clone();
			let ar_config = ar_config.clone();
			let guild_id_str = guild_id_for_tasks.clone();
			background_tasks.spawn(async move {
				tokio::time::sleep(std::time::Duration::from_secs(10)).await;
				tracing::info!("Auto-role time checker started (60s interval).");
				loop {
					run_supervised("auto_role_time_check", || async {
						if let Ok(members) =
							db::queries::get_unpromoted_members(&db_ar, &guild_id_str).await
						{
							for activity in members {
								if autorole::meets_criteria(&activity, &ar_config) {
									if let Ok(uid) = activity.user_id.parse::<u64>() {
										if let Ok(gid) = activity.guild_id.parse::<u64>() {
											if let Err(e) = autorole::try_promote(
												&http_ar,
												&db_ar,
												GuildId::new(gid),
												UserId::new(uid),
												&ar_config,
											)
											.await
											{
												tracing::warn!(
													"Auto-role time promotion failed for {}: {}",
													uid,
													e
												);
											}
										}
									}
								}
							}
						}
					})
					.await;
					tokio::time::sleep(std::time::Duration::from_secs(60)).await;
				}
			});
		}
	}

	// Donator sync background task: poll MC server for donator status
	if let Some(ref mc_cfg) = instance_cfg.minecraft {
		if instance_cfg.features.minecraft && mc_cfg.donator_sync {
			if let Some(ref ds_config) = mc_cfg.donator_sync_config {
				if let (Some(ref verify_url), Some(ref verify_secret)) =
					(&mc_verify_url_for_tasks, &mc_verify_secret_for_tasks)
				{
					let http_ds = client.http.clone();
					let http_client_ds = reqwest::Client::builder()
						.timeout(std::time::Duration::from_secs(10))
						.build()
						.expect("HTTP client for donator sync");
					let ds_config = ds_config.clone();
					let restricted_role = mc_cfg
						.chargeback_config
						.as_ref()
						.and_then(|cb| cb.restricted_role.parse::<u64>().ok().map(RoleId::new));
					let verify_url = verify_url.clone();
					let verify_secret = verify_secret.clone();
					let guild_id_ds = guild_id_for_tasks.clone();
					background_tasks.spawn(async move {
						tokio::time::sleep(std::time::Duration::from_secs(15)).await;
						tracing::info!(
							"Donator sync checker started ({}s interval).",
							ds_config.check_interval
						);
						loop {
							run_supervised("donator_sync", || async {
								match minecraft::donator_sync::fetch_donators(
									&http_client_ds,
									&verify_url,
									&verify_secret,
								)
								.await
								{
									Ok(donators) => {
										if let Ok(gid) = guild_id_ds.parse::<u64>() {
											if let Err(e) = minecraft::donator_sync::sync_roles(
												&http_ds,
												GuildId::new(gid),
												&donators,
												&ds_config,
												restricted_role,
											)
											.await
											{
												tracing::warn!("Donator sync error: {e}");
											}
										}
									}
									Err(e) => {
										tracing::warn!(
											"Donator sync: failed to fetch donators: {e}"
										);
									}
								}
							})
							.await;
							tokio::time::sleep(std::time::Duration::from_secs(
								ds_config.check_interval,
							))
							.await;
						}
					});
				} else {
					tracing::warn!(
						"Donator sync enabled but MC_VERIFY_URL or MC_VERIFY_SECRET not set"
					);
				}
			}
		}
	}

	// Detect dead background tasks. JoinSet::join_next yields each task as it
	// finishes; with panic-recovery wrappers in place the loops should never
	// exit on their own, so any completion here is noteworthy.
	tokio::spawn(async move {
		while let Some(res) = background_tasks.join_next().await {
			match res {
				Ok(()) => tracing::error!("A background task exited unexpectedly (loop returned)"),
				Err(e) if e.is_panic() => {
					tracing::error!("A background task panicked outside its supervised body: {e}")
				}
				Err(e) => tracing::error!("Background task join error: {e}"),
			}
		}
	});

	tracing::info!("Starting bot...");
	let shard_manager = client.shard_manager.clone();
	tokio::select! {
		res = client.start() => {
			if let Err(e) = res {
				tracing::error!("Client error: {e}");
			}
		}
		_ = shutdown_signal() => {
			tracing::info!("Shutdown signal received, stopping bot...");
			shard_manager.shutdown_all().await;
		}
	}
}

/// Future that resolves on Ctrl-C or (unix) SIGTERM. Used to race against
/// `client.start()` so we can drive a clean shutdown of the shard manager
/// (which in turn cleans up songbird voice connections).
async fn shutdown_signal() {
	let ctrl_c = async {
		if let Err(e) = tokio::signal::ctrl_c().await {
			tracing::error!("Failed to install Ctrl-C handler: {e}");
			// If we can't install the handler, never resolve — let the other
			// branch of the select! drive shutdown.
			std::future::pending::<()>().await;
		}
	};

	#[cfg(unix)]
	let terminate = async {
		match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
			Ok(mut sig) => {
				sig.recv().await;
			}
			Err(e) => {
				tracing::error!("Failed to install SIGTERM handler: {e}");
				std::future::pending::<()>().await;
			}
		}
	};

	#[cfg(not(unix))]
	let terminate = std::future::pending::<()>();

	tokio::select! {
		_ = ctrl_c => {},
		_ = terminate => {},
	}
}
