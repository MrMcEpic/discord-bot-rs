pub mod member_join;
pub mod ready;
pub mod voice_state;

use serenity::all::*;

use crate::ai::chat::handle_mention;
use crate::connections::embeds as conn_embeds;
use crate::connections::game::GuessResult;
use crate::db::queries::get_guild_settings;
use crate::error::BotError;
use crate::music::embeds::{music_controls, now_playing_embed, queue_embed, status_footer};
use crate::music::voice;
use crate::wordle::embeds as wordle_embeds;
use crate::wordle::game::{self as wordle_game, GuessOutcome};
use crate::Data;

pub async fn event_handler(
	ctx: &Context,
	event: &poise::serenity_prelude::FullEvent,
	_framework: poise::FrameworkContext<'_, Data, BotError>,
	data: &Data,
) -> Result<(), BotError> {
	match event {
		poise::serenity_prelude::FullEvent::Ready { data_about_bot, .. } => {
			ready::handle_ready(ctx, data_about_bot, &data.cmd_prefix).await;

			// Start MCP server (only once, guard against reconnect re-fires)
			if !data
				.mcp_started
				.swap(true, std::sync::atomic::Ordering::SeqCst)
			{
				let http = ctx.http.clone();
				let guild_id =
					GuildId::new(data.config.guild_id.parse().expect("Invalid GUILD_ID"));
				let mcp_port = data.config.mcp_port;
				let mcp_bind_addr = data.config.mcp_bind_addr.clone();
				let mcp_auth_token = data.config.mcp_auth_token.clone();

				// Build webhook router if chargeback is enabled
				let webhook_router = if let Some(ref mc_cfg) = data.minecraft_config {
					if mc_cfg.chargeback {
						mc_cfg.chargeback_config.as_ref().and_then(|cb_cfg| {
							// Gate: chargeback router only spins up when both MC verify URL
							// and secret are configured. The URL itself isn't stored on
							// WebhookState — buttons read it from `Data` at click time.
							data.mc_verify_url.as_ref()?;
							let verify_secret = data.mc_verify_secret.as_ref()?;
							let state = crate::minecraft::chargeback::WebhookState {
								http: ctx.http.clone(),
								guild_id,
								chargeback_config: cb_cfg.clone(),
								mc_verify_secret: verify_secret.clone(),
							};
							Some(crate::minecraft::chargeback::build_webhook_router(state))
						})
					} else {
						None
					}
				} else {
					None
				};

				tokio::spawn(async move {
					crate::run_supervised("mcp_server", || async {
						if let Err(e) = crate::mcp::start(
							http,
							guild_id,
							mcp_port,
							mcp_bind_addr,
							mcp_auth_token,
							webhook_router,
						)
						.await
						{
							// Bot keeps running without MCP — operator sees a clear error
							// instead of a panic-loop in the supervisor wrapper. Common
							// causes: port already bound, security gate refusal, axum
							// transport error.
							tracing::error!(error = %e, "MCP server stopped");
						}
					})
					.await;
				});
			}
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
		poise::serenity_prelude::FullEvent::GuildMemberAddition { new_member } => {
			member_join::handle_member_join(ctx, new_member, data).await;
		}
		_ => {}
	}

	Ok(())
}

async fn handle_message(ctx: &Context, message: &Message, data: &Data) {
	if message.author.bot || message.guild_id.is_none() {
		return;
	}

	// Auto-role: track message count and check promotion
	if let (Some(ref ar_config), Some(guild_id)) = (&data.auto_role_config, message.guild_id) {
		let gid = guild_id.to_string();
		let uid = message.author.id.to_string();
		if let Ok(activity) =
			crate::db::queries::increment_message_count(&data.db, &gid, &uid).await
		{
			if !activity.promoted && crate::autorole::meets_criteria(&activity, ar_config) {
				let http = ctx.http.clone();
				let pool = data.db.clone();
				let config = ar_config.clone();
				let author_id = message.author.id;
				tokio::spawn(async move {
					crate::run_supervised("auto_role_message_promote", || async {
						if let Err(e) =
							crate::autorole::try_promote(&http, &pool, guild_id, author_id, &config)
								.await
						{
							tracing::warn!("Auto-role promotion failed for {}: {}", author_id, e);
						}
					})
					.await;
				});
			}
		}
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

	// Check for Wordle guesses: 5 alphabetic characters in a channel with an active game
	let content = message.content.trim().to_lowercase();
	if content.len() == 5 && content.chars().all(|c| c.is_ascii_alphabetic()) {
		if let Some(game_arc) = data
			.wordle_games
			.get(&message.channel_id)
			.map(|e| e.value().clone())
		{
			let mut game = game_arc.lock().await;
			if !game.is_over() && !game.is_expired() {
				if !wordle_game::is_valid_word(&content) {
					// Invalid word — ephemeral-like reply, then delete both
					if let Ok(reply) = message.reply(&ctx.http, "Not a valid word.").await {
						let http = ctx.http.clone();
						let ch = message.channel_id;
						let reply_id = reply.id;
						tokio::spawn(async move {
							tokio::time::sleep(std::time::Duration::from_secs(2)).await;
							let _ = http.delete_message(ch, reply_id, None).await;
						});
					}
					return;
				}

				let outcome = game.make_guess(&content);

				// Delete the user's guess message
				let _ = message.delete(&ctx.http).await;

				// Update the game embed
				let embed = match outcome {
					GuessOutcome::Won => wordle_embeds::game_over_embed(&game, true),
					GuessOutcome::Lost => wordle_embeds::game_over_embed(&game, false),
					GuessOutcome::Continue => wordle_embeds::game_embed(&game),
				};

				let _ = ctx
					.http
					.edit_message(
						game.channel_id,
						game.message_id,
						&EditMessage::new().embed(embed),
						vec![],
					)
					.await;

				// Clean up finished games
				if game.is_over() {
					drop(game);
					data.wordle_games.remove(&message.channel_id);
				}

				return;
			} else if game.is_expired() {
				drop(game);
				data.wordle_games.remove(&message.channel_id);
			}
		}
	}

	let has_any_ai_key = data.ai_router.chat().is_some() || data.ai_router.vision().is_some();
	if has_any_ai_key && (is_mention || is_reply_to_bot) {
		handle_mention(ctx, message, data).await;
	}
}

async fn handle_component_interaction(
	ctx: &Context,
	interaction: &ComponentInteraction,
	data: &Data,
) {
	let custom_id = &interaction.data.custom_id;

	if custom_id.starts_with("game_") {
		handle_game_interaction(ctx, interaction, data).await;
		return;
	}

	if custom_id.starts_with("cb_") {
		crate::minecraft::chargeback::handle_button(ctx, interaction, data).await;
		return;
	}

	if !custom_id.starts_with("music_") {
		return;
	}

	let guild_id = match interaction.guild_id {
		Some(id) => id,
		None => return,
	};

	// Per-user rate limit. Buttons share the same `music` limiter as prefix
	// commands so a user spamming the UI counts against the same budget.
	let cooldown = data
		.rate_limiters
		.music
		.check(&interaction.user.id.to_string());
	if cooldown > 0 {
		let _ = interaction
			.create_response(
				&ctx.http,
				CreateInteractionResponse::Message(
					CreateInteractionResponseMessage::new()
						.content(format!("Slow down — try again in {cooldown}s."))
						.ephemeral(true),
				),
			)
			.await;
		return;
	}

	// Voice presence + DJ mode check (skip for read-only "music_queue")
	if custom_id != "music_queue" {
		// Check if user is in the same voice channel as the bot
		let user_voice_channel = ctx.cache.guild(guild_id).and_then(|g| {
			g.voice_states
				.get(&interaction.user.id)
				.and_then(|vs| vs.channel_id)
		});

		let bot_voice_channel = ctx.cache.guild(guild_id).and_then(|g| {
			g.voice_states
				.get(&ctx.cache.current_user().id)
				.and_then(|vs| vs.channel_id)
		});

		match (user_voice_channel, bot_voice_channel) {
			(None, _) => {
				let _ = interaction
					.create_response(
						&ctx.http,
						CreateInteractionResponse::Message(
							CreateInteractionResponseMessage::new()
								.content("You need to be in a voice channel to use music controls.")
								.ephemeral(true),
						),
					)
					.await;
				return;
			}
			(Some(user_vc), Some(bot_vc)) if user_vc != bot_vc => {
				let _ = interaction
					.create_response(
						&ctx.http,
						CreateInteractionResponse::Message(
							CreateInteractionResponseMessage::new()
								.content("You need to be in the same voice channel as the bot.")
								.ephemeral(true),
						),
					)
					.await;
				return;
			}
			_ => {}
		}

		// DJ mode check
		if let Some(member) = &interaction.member {
			let is_admin = member
				.permissions
				.is_some_and(|p| p.contains(Permissions::ADMINISTRATOR));
			if !is_admin {
				if let Some(settings) = get_guild_settings(&data.db, &guild_id.to_string()).await {
					if settings.dj_mode_enabled {
						if let Some(ref dj_role_id) = settings.dj_role_id {
							if let Ok(role_id) = dj_role_id.parse::<u64>() {
								if !member.roles.contains(&RoleId::new(role_id)) {
									let _ = interaction
                                        .create_response(
                                            &ctx.http,
                                            CreateInteractionResponse::Message(
                                                CreateInteractionResponseMessage::new()
                                                    .content("DJ mode is enabled. You need the DJ role to use music controls.")
                                                    .ephemeral(true),
                                            ),
                                        )
                                        .await;
									return;
								}
							}
						}
					}
				}
			}
		}
	}

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
					let loop_mode = p.loop_mode;
					drop(p);
					let pctx = data
						.playback_context(ctx, guild_id, interaction.channel_id)
						.await;
					match voice::play_track(
						ctx,
						guild_id,
						&next_track.url,
						&data.http_client,
						pctx.as_ref(),
					)
					.await
					{
						Ok(handle) => {
							data.track_handles.insert(guild_id, handle);
						}
						Err(e) => tracing::error!("Playback error on skip: {e}"),
					}

					// Update the original message to just show "Skipped" (remove controls)
					let skip_embed = CreateEmbed::new()
						.color(0x5865f2)
						.description(format!("⏭️ Skipped **{title}**."));
					let _ = interaction
						.create_response(
							&ctx.http,
							CreateInteractionResponse::UpdateMessage(
								CreateInteractionResponseMessage::new()
									.embed(skip_embed)
									.components(vec![]),
							),
						)
						.await;

					// Replace the prior "Now Playing" embed with controls.
					let embed = now_playing_embed(&next_track);
					let controls = music_controls(false, loop_mode);
					if let Some(ref pctx) = pctx {
						if let Err(e) = voice::replace_now_playing_message(
							&ctx.http,
							interaction.channel_id,
							&pctx.now_playing_msg,
							embed,
							Some(controls),
						)
						.await
						{
							tracing::warn!("music_skip button: failed to send NP message: {e}");
						}
					}
				} else {
					drop(p);
					voice::stop_playback(ctx, guild_id).await;
					data.track_handles.remove(&guild_id);
					let _ = interaction
						.create_response(
							&ctx.http,
							CreateInteractionResponse::Message(
								CreateInteractionResponseMessage::new()
									.content(format!("⏭️ Skipped **{title}**. Queue is empty.")),
							),
						)
						.await;
				}
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

			// Cancel any pending idle timer
			if let Some(pctx) = data
				.playback_context(ctx, guild_id, interaction.channel_id)
				.await
			{
				voice::cancel_idle_timer(&pctx).await;
			}

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

async fn handle_game_interaction(ctx: &Context, interaction: &ComponentInteraction, data: &Data) {
	let channel_id = interaction.channel_id;
	let custom_id = &interaction.data.custom_id;

	let game_arc = match data.connections_games.get(&channel_id) {
		Some(entry) => entry.value().clone(),
		None => {
			let _ = interaction
				.create_response(
					&ctx.http,
					CreateInteractionResponse::Message(
						CreateInteractionResponseMessage::new()
							.content(
								"No active game in this channel. Start one with `!m connections`.",
							)
							.ephemeral(true),
					),
				)
				.await;
			return;
		}
	};

	let mut game = game_arc.lock().await;

	// Check expiration
	if game.is_expired() {
		drop(game);
		data.connections_games.remove(&channel_id);
		let _ = interaction
            .create_response(
                &ctx.http,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content("Game expired due to inactivity. Start a new one with `!m connections`.")
                        .ephemeral(true),
                ),
            )
            .await;
		return;
	}

	// Check if game is already over
	if game.is_over() {
		let _ = interaction
			.create_response(
				&ctx.http,
				CreateInteractionResponse::Message(
					CreateInteractionResponseMessage::new()
						.content(
							"This game is already over. Start a new one with `!m connections`.",
						)
						.ephemeral(true),
				),
			)
			.await;
		return;
	}

	match custom_id.as_str() {
		id if id.starts_with("game_word_") => {
			if let Ok(index) = id.strip_prefix("game_word_").unwrap().parse::<usize>() {
				game.toggle_select(index);
				game.status_message = None;
			}
		}
		"game_shuffle" => {
			game.shuffle_board();
		}
		"game_deselect" => {
			game.deselect_all();
		}
		"game_submit" => {
			if game.selected.len() != 4 {
				let _ = interaction
					.create_response(
						&ctx.http,
						CreateInteractionResponse::Message(
							CreateInteractionResponseMessage::new()
								.content("Select exactly 4 words before submitting.")
								.ephemeral(true),
						),
					)
					.await;
				return;
			}

			let user_mention = format!("<@{}>", interaction.user.id);
			match game.submit_guess() {
				GuessResult::Correct { category_index } => {
					let cat = &game.categories[category_index];
					let emoji =
						crate::connections::game::ConnectionsGame::difficulty_emoji(cat.difficulty);
					game.status_message = Some(format!(
						"{emoji} **{}** solved by {}!",
						cat.title, user_mention
					));
				}
				GuessResult::OneAway => {
					game.status_message = Some(format!(
						"❌ One away! (guessed by {}) — {} mistakes remaining",
						user_mention, game.mistakes_remaining
					));
				}
				GuessResult::Wrong => {
					game.status_message = Some(format!(
						"❌ Wrong! (guessed by {}) — {} mistakes remaining",
						user_mention, game.mistakes_remaining
					));
				}
				GuessResult::AlreadyGuessed => {
					game.status_message = Some("Already guessed this combination.".to_string());
				}
			}
		}
		_ => return,
	}

	// Build updated embed + buttons
	let (embed, buttons) = if game.is_over() {
		let won = game.is_won();
		(conn_embeds::game_over_embed(&game, won), vec![])
	} else {
		(
			conn_embeds::game_embed(&game),
			conn_embeds::game_buttons(&game),
		)
	};

	let _ = interaction
		.create_response(
			&ctx.http,
			CreateInteractionResponse::UpdateMessage(
				CreateInteractionResponseMessage::new()
					.embed(embed)
					.components(buttons),
			),
		)
		.await;

	// Clean up finished games
	if game.is_over() {
		drop(game);
		data.connections_games.remove(&channel_id);
	}
}
