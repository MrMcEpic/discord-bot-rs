use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::{extract::State, middleware, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use serenity::all::*;
use std::sync::Arc;

use crate::instance_config::ChargebackConfig;

#[derive(Clone)]
pub struct WebhookState {
	pub http: Arc<Http>,
	pub guild_id: GuildId,
	pub chargeback_config: ChargebackConfig,
	pub mc_verify_secret: String,
}

#[derive(Debug, Deserialize)]
pub struct ChargebackPayload {
	pub uuid: String,
	pub username: String,
	pub discord_id: Option<String>,
	pub tier: String,
	pub timestamp: String,
}

#[derive(Serialize)]
struct WebhookResponse {
	success: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	error: Option<String>,
}

pub fn build_webhook_router(state: WebhookState) -> Router {
	let secret = state.mc_verify_secret.clone();
	let auth = middleware::from_fn(move |req: Request, next: Next| {
		let expected = secret.clone();
		async move {
			let auth = req
				.headers()
				.get("authorization")
				.and_then(|v| v.to_str().ok())
				.unwrap_or("");
			if auth == format!("Bearer {}", expected) {
				next.run(req).await
			} else {
				(
					StatusCode::UNAUTHORIZED,
					Json(serde_json::json!({"error": "Unauthorized"})),
				)
					.into_response()
			}
		}
	});

	Router::new()
		.route("/webhook/chargeback", post(handle_chargeback))
		.layer(auth)
		.with_state(state)
}

async fn handle_chargeback(
	State(state): State<WebhookState>,
	Json(payload): Json<ChargebackPayload>,
) -> impl IntoResponse {
	tracing::info!(
		"Chargeback webhook received: player={}, tier={}, discord_id={:?}",
		payload.username,
		payload.tier,
		payload.discord_id
	);

	let restricted_role_id = match state.chargeback_config.restricted_role.parse::<u64>() {
		Ok(id) => RoleId::new(id),
		Err(_) => {
			tracing::error!("Invalid restricted_role ID in config");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(WebhookResponse {
					success: false,
					error: Some("Server config error".to_string()),
				}),
			);
		}
	};

	let staff_channel_id = match state.chargeback_config.staff_channel.parse::<u64>() {
		Ok(id) => ChannelId::new(id),
		Err(_) => {
			tracing::error!("Invalid staff_channel ID in config");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(WebhookResponse {
					success: false,
					error: Some("Server config error".to_string()),
				}),
			);
		}
	};

	// Strip roles + apply restricted role if discord_id present
	if let Some(ref discord_id_str) = payload.discord_id {
		if let Ok(uid) = discord_id_str.parse::<u64>() {
			let user_id = UserId::new(uid);
			let edit = EditMember::new().roles(vec![restricted_role_id]);
			match state
				.http
				.edit_member(
					state.guild_id,
					user_id,
					&edit,
					Some("Chargeback: roles stripped, user restricted"),
				)
				.await
			{
				Ok(_) => {
					tracing::info!("Chargeback: stripped roles and restricted user {}", user_id)
				}
				Err(e) => {
					tracing::warn!("Chargeback: failed to modify roles for {}: {}", user_id, e)
				}
			}
		}
	}

	// Build and send staff alert
	let embed = build_chargeback_embed(&payload);
	let buttons = build_chargeback_buttons(&payload);
	let msg = CreateMessage::new().embed(embed).components(vec![buttons]);

	match staff_channel_id.send_message(&state.http, msg).await {
		Ok(_) => tracing::info!("Chargeback alert posted to staff channel"),
		Err(e) => tracing::error!("Failed to post chargeback alert: {}", e),
	}

	(
		StatusCode::OK,
		Json(WebhookResponse {
			success: true,
			error: None,
		}),
	)
}

fn build_chargeback_embed(payload: &ChargebackPayload) -> CreateEmbed {
	let discord_field = match &payload.discord_id {
		Some(id) => format!("<@{}> ({})", id, id),
		None => "Not linked".to_string(),
	};

	let status_text = if payload.discord_id.is_some() {
		"All roles stripped. User restricted."
	} else {
		"No Discord account linked. MC-side actions only."
	};

	let tier_display = match payload.tier.as_str() {
		"supporter" => "Supporter",
		"premium" => "Premium",
		other => other,
	};

	CreateEmbed::new()
		.title("⚠️ CHARGEBACK ALERT")
		.color(0xE74C3C)
		.field("Player", &payload.username, true)
		.field("Tier", tier_display, true)
		.field("Discord", &discord_field, false)
		.field("MC UUID", &payload.uuid, false)
		.field("Time", &payload.timestamp, false)
		.footer(CreateEmbedFooter::new(status_text))
}

fn build_chargeback_buttons(payload: &ChargebackPayload) -> CreateActionRow {
	let ban_label = if payload.discord_id.is_some() {
		"🔨 Ban"
	} else {
		"🔨 Ban MC"
	};

	CreateActionRow::Buttons(vec![
		CreateButton::new(format!("cb_ban:{}", payload.uuid))
			.label(ban_label)
			.style(ButtonStyle::Danger),
		CreateButton::new(format!("cb_dismiss:{}", payload.uuid))
			.label("❌ Dismiss")
			.style(ButtonStyle::Secondary),
	])
}

/// Handle Ban/Dismiss button clicks from the chargeback alert embed.
/// Called from events/mod.rs when a `cb_*` custom_id is received.
pub async fn handle_button(ctx: &Context, interaction: &ComponentInteraction, data: &crate::Data) {
	let custom_id = &interaction.data.custom_id;
	let (action, uuid) = match custom_id.split_once(':') {
		Some(("cb_ban", uuid)) => ("ban", uuid.to_string()),
		Some(("cb_dismiss", uuid)) => ("dismiss", uuid.to_string()),
		_ => return,
	};

	// Permission check: require Moderator, Admin, or Owner
	let staff_roles: Vec<RoleId> = vec![
		RoleId::new(123456789012345678), // Moderator
		RoleId::new(123456789012345678), // Admin
		RoleId::new(123456789012345678), // Owner
	];

	let member = match &interaction.member {
		Some(m) => m,
		None => return,
	};

	if !member.roles.iter().any(|r| staff_roles.contains(r)) {
		let _ = interaction
			.create_response(
				&ctx.http,
				CreateInteractionResponse::Message(
					CreateInteractionResponseMessage::new()
						.content("You don't have permission to do this.")
						.ephemeral(true),
				),
			)
			.await;
		return;
	}

	let staff_user = &interaction.user;
	let guild_id = match interaction.guild_id {
		Some(id) => id,
		None => return,
	};

	// Read context from the embed
	let embed = match interaction.message.embeds.first() {
		Some(e) => e,
		None => return,
	};

	let discord_field = embed
		.fields
		.iter()
		.find(|f| f.name == "Discord")
		.map(|f| f.value.clone())
		.unwrap_or_default();
	let is_linked = discord_field != "Not linked";

	// Extract discord user ID from the embed field format: "<@ID> (ID)"
	let discord_user_id: Option<UserId> = if is_linked {
		discord_field
			.split('(')
			.nth(1)
			.and_then(|s| s.trim_end_matches(')').parse::<u64>().ok())
			.map(UserId::new)
	} else {
		None
	};

	match action {
		"ban" => {
			// Discord ban (if linked)
			if let Some(user_id) = discord_user_id {
				let reason = format!("Chargeback ban by {}", staff_user.name);
				match ctx.http.ban_user(guild_id, user_id, 0, Some(&reason)).await {
					Ok(_) => tracing::info!(
						"Chargeback: Discord-banned {} by {}",
						user_id,
						staff_user.name
					),
					Err(e) => {
						tracing::warn!("Chargeback: Discord ban failed for {}: {}", user_id, e)
					}
				}
			}

			// MC ban
			if let (Some(ref url), Some(ref secret)) = (&data.mc_verify_url, &data.mc_verify_secret)
			{
				let ban_url = format!("{}/api/ban", url.trim_end_matches('/'));
				let body = serde_json::json!({
					"uuid": uuid,
					"reason": format!("Chargeback ban issued by Discord staff ({})", staff_user.name)
				});
				match data
					.http_client
					.post(&ban_url)
					.header("Authorization", format!("Bearer {}", secret))
					.json(&body)
					.send()
					.await
				{
					Ok(resp) if resp.status().is_success() => {
						tracing::info!("Chargeback: MC ban sent for UUID {}", uuid);
					}
					Ok(resp) => {
						let status = resp.status();
						let body = resp.text().await.unwrap_or_default();
						tracing::warn!("Chargeback: MC ban failed ({}): {}", status, body);
						if let Some(ref mc_cfg) = data.minecraft_config {
							if let Some(ref cb_cfg) = mc_cfg.chargeback_config {
								if let Ok(ch) = cb_cfg.staff_channel.parse::<u64>() {
									let _ = ChannelId::new(ch)
										.send_message(
											&ctx.http,
											CreateMessage::new().content(format!(
												"⚠️ MC ban failed for UUID `{}`: {} {}",
												uuid, status, body
											)),
										)
										.await;
								}
							}
						}
					}
					Err(e) => {
						tracing::warn!("Chargeback: MC ban request failed: {}", e);
						if let Some(ref mc_cfg) = data.minecraft_config {
							if let Some(ref cb_cfg) = mc_cfg.chargeback_config {
								if let Ok(ch) = cb_cfg.staff_channel.parse::<u64>() {
									let _ = ChannelId::new(ch)
										.send_message(
											&ctx.http,
											CreateMessage::new().content(format!(
												"⚠️ MC ban failed for UUID `{}`: {}",
												uuid, e
											)),
										)
										.await;
								}
							}
						}
					}
				}
			}

			// Update embed
			let new_embed = rebuild_embed(embed, format!("Banned by {}", staff_user.name));
			let response = CreateInteractionResponse::UpdateMessage(
				CreateInteractionResponseMessage::new()
					.embed(new_embed)
					.components(vec![]),
			);
			let _ = interaction.create_response(&ctx.http, response).await;
		}
		"dismiss" => {
			let new_embed = rebuild_embed(embed, format!("Dismissed by {}", staff_user.name));
			let response = CreateInteractionResponse::UpdateMessage(
				CreateInteractionResponseMessage::new()
					.embed(new_embed)
					.components(vec![]),
			);
			let _ = interaction.create_response(&ctx.http, response).await;
		}
		_ => {}
	}
}

/// Reconstruct a CreateEmbed from a received Embed, replacing the footer.
/// Needed because serenity 0.12 has no From<Embed> for CreateEmbed.
fn rebuild_embed(embed: &Embed, new_footer: String) -> CreateEmbed {
	let mut builder = CreateEmbed::new().color(0x2C2F33);
	if let Some(ref title) = embed.title {
		builder = builder.title(title);
	}
	for field in &embed.fields {
		builder = builder.field(&field.name, &field.value, field.inline);
	}
	builder.footer(CreateEmbedFooter::new(new_footer))
}
