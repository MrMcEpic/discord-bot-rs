use serenity::all::*;

use crate::Data;

const DEEPSEEK_URL: &str = "https://api.deepseek.com/chat/completions";
const GEMINI_URL: &str = "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions";

pub async fn handle_member_join(ctx: &Context, member: &Member, data: &Data) {
	let guild_id = member.guild_id;

	// --- Join Role ---
	if let Some(ref jr_config) = data.join_role_config {
		match jr_config.role.parse::<u64>() {
			Ok(role_id) => {
				if let Err(e) = ctx
					.http
					.add_member_role(
						guild_id,
						member.user.id,
						RoleId::new(role_id),
						Some("Auto join role"),
					)
					.await
				{
					tracing::warn!("Failed to assign join role to {}: {e}", member.user.id);
				} else {
					tracing::info!(
						"Assigned join role to {} in guild {}",
						member.user.id,
						guild_id
					);
				}
			}
			Err(_) => tracing::warn!("Invalid join role ID: {}", jr_config.role),
		}
	}

	// --- Welcome Message ---
	if let (Some(ref wc_config), Some(ref welcome_prompt)) =
		(&data.welcome_config, &data.welcome_prompt)
	{
		// Rate limit: per-user, 1 welcome / 5 seconds
		if data
			.rate_limiters
			.welcome
			.check(&member.user.id.to_string())
			> 0
		{
			tracing::debug!("Skipping welcome for {} (rate limited)", member.user.id);
			return;
		}

		let channel_id = match wc_config.channel.parse::<u64>() {
			Ok(id) => ChannelId::new(id),
			Err(_) => {
				tracing::warn!("Invalid welcome channel ID: {}", wc_config.channel);
				return;
			}
		};

		let api_key = data
			.config
			.deepseek_api_key
			.as_ref()
			.or(data.config.gemini_api_key.as_ref());

		let Some(key) = api_key else {
			tracing::warn!("Welcome message enabled but no AI API key available");
			return;
		};

		let (url, model) = if data.config.deepseek_api_key.is_some() {
			(DEEPSEEK_URL, "deepseek-chat")
		} else {
			(GEMINI_URL, "gemini-3-flash-preview")
		};

		let system_prompt = format!(
			"{}\n\n## Welcome Message Instructions\n{}",
			data.personality, welcome_prompt
		);

		let display_name = member.display_name().to_string();
		let user_mention = format!("<@{}>", member.user.id);

		let messages = vec![
			serde_json::json!({
				"role": "system",
				"content": system_prompt,
			}),
			serde_json::json!({
				"role": "user",
				"content": format!(
					"A new member has joined the server! Their name is {} and their mention is {}. Write a welcome message for them.",
					display_name, user_mention
				),
			}),
		];

		let body = serde_json::json!({
			"model": model,
			"messages": messages,
			"max_tokens": 512,
		});

		match data
			.http_client
			.post(url)
			.header("Content-Type", "application/json")
			.header("Authorization", format!("Bearer {}", key))
			.timeout(std::time::Duration::from_secs(30))
			.json(&body)
			.send()
			.await
		{
			Ok(response) if response.status().is_success() => {
				if let Ok(json) = response.json::<serde_json::Value>().await {
					if let Some(content) = json["choices"][0]["message"]["content"].as_str() {
						let content = content.trim();
						if !content.is_empty() {
							if let Err(e) = channel_id
								.send_message(&ctx.http, CreateMessage::new().content(content))
								.await
							{
								tracing::warn!("Failed to send welcome message: {e}");
							}
						}
					}
				}
			}
			Ok(response) => {
				tracing::warn!(
					"Welcome AI request failed with status {}",
					response.status()
				);
			}
			Err(e) => {
				tracing::warn!("Welcome AI request error: {e}");
			}
		}
	}
}
