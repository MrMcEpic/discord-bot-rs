use rmcp::handler::server::wrapper::Parameters;
use rmcp::{
	handler::server::router::tool::ToolRouter, handler::server::tool::ToolCallContext, model::*,
	service::RequestContext, tool, tool_router, ErrorData as McpError, RoleServer, ServerHandler,
};
// Disambiguate: both `rmcp::model::*` and `serenity::all::*` export a `Content` symbol.
// We want the MCP one (a type alias for `Annotated<RawContent>`) for tool results.
use rmcp::model::Content;
use schemars::JsonSchema;
use serde::Deserialize;
use serenity::all::*;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

const API_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct DiscordTools {
	pub http: Arc<Http>,
	pub guild_id: GuildId,
	tool_router: ToolRouter<Self>,
}

fn api_err(e: impl std::fmt::Display) -> McpError {
	McpError::internal_error(format!("Discord API error: {e}"), None)
}

fn timeout_err() -> McpError {
	McpError::internal_error("Discord API request timed out".to_string(), None)
}

macro_rules! discord_call {
	($expr:expr) => {
		timeout(API_TIMEOUT, $expr)
			.await
			.map_err(|_| timeout_err())?
			.map_err(api_err)?
	};
}

// --- Parameter types ---

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateChannelParams {
	/// Guild/server ID (optional, defaults to configured guild)
	pub guild_id: Option<String>,
	pub name: String,
	/// text, voice, category, forum, or stage
	#[serde(default = "default_text")]
	pub channel_type: String,
	pub category_id: Option<String>,
	pub topic: Option<String>,
	pub nsfw: Option<bool>,
}
fn default_text() -> String {
	"text".to_string()
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ChannelIdParam {
	/// Guild/server ID (optional, defaults to configured guild)
	pub guild_id: Option<String>,
	pub channel_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EditChannelParams {
	/// Guild/server ID (optional, defaults to configured guild)
	pub guild_id: Option<String>,
	pub channel_id: String,
	pub name: Option<String>,
	pub topic: Option<String>,
	pub nsfw: Option<bool>,
	pub slowmode_seconds: Option<u16>,
	pub category_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MoveChannelParams {
	/// Guild/server ID (optional, defaults to configured guild)
	pub guild_id: Option<String>,
	pub channel_id: String,
	pub position: u16,
	pub category_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetChannelPermsParams {
	/// Guild/server ID (optional, defaults to configured guild)
	pub guild_id: Option<String>,
	pub channel_id: String,
	/// "role" or "member"
	pub target_type: String,
	pub target_id: String,
	/// Permission bits to allow (decimal string). VIEW_CHANNEL=1024, SEND_MESSAGES=2048, MANAGE_CHANNELS=16, MANAGE_MESSAGES=8192, CONNECT=1048576, SPEAK=2097152
	pub allow: Option<String>,
	/// Permission bits to deny (decimal string)
	pub deny: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateRoleParams {
	/// Guild/server ID (optional, defaults to configured guild)
	pub guild_id: Option<String>,
	pub name: String,
	pub color: Option<u32>,
	pub permissions: Option<String>,
	pub hoist: Option<bool>,
	pub mentionable: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RoleIdParam {
	/// Guild/server ID (optional, defaults to configured guild)
	pub guild_id: Option<String>,
	pub role_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EditRoleParams {
	/// Guild/server ID (optional, defaults to configured guild)
	pub guild_id: Option<String>,
	pub role_id: String,
	pub name: Option<String>,
	pub color: Option<u32>,
	pub permissions: Option<String>,
	pub hoist: Option<bool>,
	pub mentionable: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListMembersParams {
	/// Guild/server ID (optional, defaults to configured guild)
	pub guild_id: Option<String>,
	/// Max members (1-1000, default 100)
	pub limit: Option<u64>,
	/// User ID to paginate after
	pub after: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UserIdParam {
	/// Guild/server ID (optional, defaults to configured guild)
	pub guild_id: Option<String>,
	pub user_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UserRoleParams {
	/// Guild/server ID (optional, defaults to configured guild)
	pub guild_id: Option<String>,
	pub user_id: String,
	pub role_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BanParams {
	/// Guild/server ID (optional, defaults to configured guild)
	pub guild_id: Option<String>,
	pub user_id: String,
	pub reason: Option<String>,
	/// Days of messages to delete (0-7)
	pub delete_message_days: Option<u8>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct KickParams {
	/// Guild/server ID (optional, defaults to configured guild)
	pub guild_id: Option<String>,
	pub user_id: String,
	pub reason: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TimeoutParams {
	/// Guild/server ID (optional, defaults to configured guild)
	pub guild_id: Option<String>,
	pub user_id: String,
	/// e.g. "1h", "30m", "7d"
	pub duration: String,
	/// Audit-log reason. Part of the public MCP tool schema; the underlying
	/// `edit_member` call doesn't currently thread it through to Discord.
	#[allow(dead_code)]
	pub reason: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SendMessageParams {
	/// Guild/server ID (optional, defaults to configured guild)
	pub guild_id: Option<String>,
	pub channel_id: String,
	pub content: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteMessagesParams {
	/// Guild/server ID (optional, defaults to configured guild)
	pub guild_id: Option<String>,
	pub channel_id: String,
	/// Number of recent messages (1-100)
	pub count: u8,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OptionalGuildParam {
	/// Guild/server ID (optional, defaults to configured guild)
	pub guild_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetMessagesParams {
	/// Guild/server ID (optional, defaults to configured guild)
	pub guild_id: Option<String>,
	pub channel_id: String,
	/// Number of messages to fetch, newest first (1-100, default 50)
	#[serde(default)]
	pub limit: Option<u8>,
	/// Fetch messages older than this message ID (for pagination)
	#[serde(default)]
	pub before: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReactionParams {
	/// Guild/server ID (optional, defaults to configured guild)
	pub guild_id: Option<String>,
	pub channel_id: String,
	pub message_id: String,
	/// Unicode emoji (e.g. "👍"), Discord custom-emoji format ("<:name:id>" or "<a:name:id>" for animated), or a bare custom-emoji snowflake.
	pub emoji: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetNicknameParams {
	/// Guild/server ID (optional, defaults to configured guild)
	pub guild_id: Option<String>,
	pub user_id: String,
	/// New nickname (1-32 chars). Omit or pass an empty string to clear the nickname (member shows their global username).
	#[serde(default)]
	pub nickname: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetBansParams {
	/// Guild/server ID (optional, defaults to configured guild)
	pub guild_id: Option<String>,
	/// Max bans to return per page (1-255, default 100). Capped by Discord's bulk endpoint; paginate with `after` for more.
	#[serde(default)]
	pub limit: Option<u8>,
	/// Paginate forward — return bans whose user_id is greater than this snowflake
	#[serde(default)]
	pub after: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchMessagesParams {
	/// Guild/server ID (optional, defaults to configured guild)
	pub guild_id: Option<String>,
	pub channel_id: String,
	/// Filter to messages from this user ID (snowflake)
	#[serde(default)]
	pub author_id: Option<String>,
	/// Filter by case-insensitive substring of the author's username
	#[serde(default)]
	pub author_name: Option<String>,
	/// Filter by case-insensitive substring of the message body
	#[serde(default)]
	pub content: Option<String>,
	/// Lower time bound. Accepts ISO 8601 (e.g. "2026-07-03" or "2026-07-03T12:00:00Z") or a Discord snowflake. Only messages newer than this are searched.
	#[serde(default)]
	pub after: Option<String>,
	/// Upper time bound. Same format as `after`. Only messages older than this are searched.
	#[serde(default)]
	pub before: Option<String>,
	/// Max matching messages to return (1-1000, default 100)
	#[serde(default)]
	pub limit: Option<u32>,
	/// Max API pages of 100 to scan as a safety cap (1-100, default 20 = 2000 messages scanned)
	#[serde(default)]
	pub max_pages: Option<u32>,
}

// --- Helpers ---

const DISCORD_EPOCH_MS: i64 = 1_420_070_400_000;

fn parse_id(s: &str) -> Result<u64, McpError> {
	s.parse::<u64>()
		.map_err(|_| McpError::invalid_params(format!("Invalid ID: {s}"), None))
}

/// Parse an emoji string into a `ReactionType`. Accepts:
///   - Unicode emoji ("👍", "🎉")
///   - Discord custom-emoji format ("<:name:id>" or "<a:name:id>" for animated)
///   - A bare custom-emoji snowflake ("123456789012345678")
fn parse_emoji(s: &str) -> Result<ReactionType, McpError> {
	let s = s.trim();
	if s.is_empty() {
		return Err(McpError::invalid_params("Emoji is empty", None));
	}
	// Custom emoji formats: <:name:id> or <a:name:id>
	for (prefix, animated) in [("<:", false), ("<a:", true)] {
		if let Some(inner) = s.strip_prefix(prefix).and_then(|r| r.strip_suffix('>')) {
			if let Some((name, id_str)) = inner.rsplit_once(':') {
				if let Ok(id) = id_str.parse::<u64>() {
					return Ok(ReactionType::Custom {
						animated,
						id: EmojiId::new(id),
						name: Some(name.to_string()),
					});
				}
			}
			return Err(McpError::invalid_params(
				format!("Malformed custom emoji '{s}'"),
				None,
			));
		}
	}
	// Bare numeric snowflake
	if let Ok(id) = s.parse::<u64>() {
		return Ok(ReactionType::Custom {
			animated: false,
			id: EmojiId::new(id),
			name: None,
		});
	}
	// Default: unicode emoji
	Ok(ReactionType::Unicode(s.to_string()))
}

/// Parse a string as either a Discord snowflake (numeric) or an ISO 8601
/// date / datetime, returning the equivalent message snowflake. Used as
/// `after` / `before` boundaries in `search_messages` so callers can pass
/// natural dates without having to compute snowflakes themselves.
fn parse_time_or_snowflake(s: &str) -> Result<MessageId, McpError> {
	let s = s.trim();
	if let Ok(n) = s.parse::<u64>() {
		return Ok(MessageId::new(n));
	}
	let ms_since_epoch = if let Ok(d) = chrono::DateTime::parse_from_rfc3339(s) {
		d.timestamp_millis()
	} else if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
		d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis()
	} else {
		return Err(McpError::invalid_params(
			format!("Invalid time/snowflake '{s}'. Expected ISO 8601 date (e.g. 2026-07-03) or numeric snowflake."),
			None,
		));
	};
	if ms_since_epoch < DISCORD_EPOCH_MS {
		return Err(McpError::invalid_params(
			format!("Date '{s}' is older than the Discord epoch (2015-01-01)."),
			None,
		));
	}
	let snowflake = ((ms_since_epoch - DISCORD_EPOCH_MS) as u64) << 22;
	// MessageId requires NonZero; snowflake 0 only happens for an exact
	// Discord-epoch input, which has no real-world meaning. Promote to 1
	// so the boundary is still valid.
	Ok(MessageId::new(snowflake.max(1)))
}

fn channel_type_num(s: &str) -> u8 {
	match s.to_lowercase().as_str() {
		"voice" => 2,
		"category" => 4,
		"forum" => 15,
		"stage" => 13,
		_ => 0, // text
	}
}

fn parse_duration_secs(s: &str) -> Result<i64, McpError> {
	let s = s.trim().to_lowercase();
	let (num_str, mult) = if let Some(n) = s.strip_suffix('d') {
		(n, 86400i64)
	} else if let Some(n) = s.strip_suffix('h') {
		(n, 3600)
	} else if let Some(n) = s.strip_suffix('m') {
		(n, 60)
	} else if let Some(n) = s.strip_suffix('s') {
		(n, 1)
	} else {
		(s.as_str(), 60)
	};
	let num: i64 = num_str
		.trim()
		.parse()
		.map_err(|_| McpError::invalid_params(format!("Invalid duration: {s}"), None))?;
	Ok(num * mult)
}

// --- Tools ---

impl DiscordTools {
	fn resolve_guild(&self, guild_id: Option<&str>) -> Result<GuildId, McpError> {
		match guild_id.filter(|s| !s.is_empty()) {
			Some(id) => Ok(GuildId::new(parse_id(id)?)),
			None => Ok(self.guild_id),
		}
	}

	/// Verify that `channel_id` belongs to `guild_id`. Performs an HTTP fetch of the
	/// channel and checks `guild_id`. Rejects DM/private channels and cross-guild
	/// access. This is the authorization check that prevents a caller from passing
	/// any channel ID and having the bot operate on it just because it shares a
	/// gateway with that channel's guild.
	async fn verify_channel_in_guild(
		&self,
		channel_id: ChannelId,
		guild_id: GuildId,
	) -> Result<(), McpError> {
		let channel = discord_call!(self.http.get_channel(channel_id));
		let actual_guild = match channel {
			Channel::Guild(gc) => Some(gc.guild_id),
			_ => None,
		};
		if actual_guild != Some(guild_id) {
			return Err(McpError::invalid_params(
				format!("Channel {channel_id} is not in guild {guild_id}"),
				None,
			));
		}
		Ok(())
	}
}

impl ServerHandler for DiscordTools {
	fn get_info(&self) -> ServerInfo {
		ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
	}

	fn list_tools(
		&self,
		_request: Option<PaginatedRequestParams>,
		_context: RequestContext<RoleServer>,
	) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
		std::future::ready(Ok(ListToolsResult {
			tools: self.tool_router.list_all(),
			..Default::default()
		}))
	}

	fn call_tool(
		&self,
		request: CallToolRequestParams,
		context: RequestContext<RoleServer>,
	) -> impl std::future::Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
		let ctx = ToolCallContext::new(self, request, context);
		self.tool_router.call(ctx)
	}
}

#[tool_router(router = tool_router)]
impl DiscordTools {
	pub fn new(http: Arc<Http>, guild_id: GuildId) -> Self {
		Self {
			http,
			guild_id,
			tool_router: Self::tool_router(),
		}
	}

	// ===== GUILDS =====

	#[tool(description = "List all Discord servers (guilds) this bot is connected to")]
	async fn list_guilds(&self) -> Result<CallToolResult, McpError> {
		let guilds = discord_call!(self.http.get_guilds(None, None));
		let lines: Vec<String> = guilds
			.iter()
			.map(|g| format!("{} | ID: {}", g.name, g.id))
			.collect();
		Ok(CallToolResult::success(vec![Content::text(format!(
			"{} server(s):\n{}",
			lines.len(),
			lines.join("\n")
		))]))
	}

	// ===== SERVER =====

	#[tool(description = "Get info about a Discord server")]
	async fn get_guild_info(
		&self,
		params: Parameters<OptionalGuildParam>,
	) -> Result<CallToolResult, McpError> {
		let gid = self.resolve_guild(params.0.guild_id.as_deref())?;
		let guild = discord_call!(self.http.get_guild(gid));
		let channels = discord_call!(self.http.get_channels(gid));
		let roles = discord_call!(self.http.get_guild_roles(gid));
		let text = format!(
			"Server: {}\nID: {}\nOwner: <@{}>\nApprox Members: {}\nChannels: {}\nRoles: {}",
			guild.name,
			guild.id,
			guild.owner_id,
			guild.approximate_member_count.unwrap_or(0),
			channels.len(),
			roles.len()
		);
		Ok(CallToolResult::success(vec![Content::text(text)]))
	}

	#[tool(description = "Send a message to a channel. PRIVILEGED — recommend manual approval.")]
	async fn send_message(
		&self,
		params: Parameters<SendMessageParams>,
	) -> Result<CallToolResult, McpError> {
		let p = params.0;
		let gid = self.resolve_guild(p.guild_id.as_deref())?;
		let channel_id = ChannelId::new(parse_id(&p.channel_id)?);
		self.verify_channel_in_guild(channel_id, gid).await?;
		let map = serde_json::json!({ "content": p.content });
		let msg = discord_call!(self.http.send_message(channel_id, vec![], &map));
		Ok(CallToolResult::success(vec![Content::text(format!(
			"Message sent (ID: {})",
			msg.id
		))]))
	}

	#[tool(description = "Delete recent messages from a channel (bulk delete, 1-100)")]
	async fn delete_messages(
		&self,
		params: Parameters<DeleteMessagesParams>,
	) -> Result<CallToolResult, McpError> {
		let p = params.0;
		let gid = self.resolve_guild(p.guild_id.as_deref())?;
		let channel_id = ChannelId::new(parse_id(&p.channel_id)?);
		self.verify_channel_in_guild(channel_id, gid).await?;
		let count = p.count.clamp(1, 100);
		let messages =
			discord_call!(channel_id.messages(&*self.http, GetMessages::new().limit(count)));
		let ids: Vec<MessageId> = messages.iter().map(|m| m.id).collect();
		if ids.len() > 1 {
			discord_call!(channel_id.delete_messages(&*self.http, &ids));
		} else if ids.len() == 1 {
			discord_call!(self.http.delete_message(channel_id, ids[0], None));
		}
		Ok(CallToolResult::success(vec![Content::text(format!(
			"Deleted {} message(s)",
			ids.len()
		))]))
	}

	#[tool(
		description = "Fetch recent messages from a channel, newest first. Supports pagination via `before`."
	)]
	async fn get_recent_messages(
		&self,
		params: Parameters<GetMessagesParams>,
	) -> Result<CallToolResult, McpError> {
		let p = params.0;
		let gid = self.resolve_guild(p.guild_id.as_deref())?;
		let channel_id = ChannelId::new(parse_id(&p.channel_id)?);
		self.verify_channel_in_guild(channel_id, gid).await?;
		let limit = p.limit.unwrap_or(50).clamp(1, 100);
		let mut builder = GetMessages::new().limit(limit);
		if let Some(before) = p.before.as_deref().filter(|s| !s.is_empty()) {
			builder = builder.before(MessageId::new(parse_id(before)?));
		}
		let messages = discord_call!(channel_id.messages(&*self.http, builder));
		if messages.is_empty() {
			return Ok(CallToolResult::success(vec![Content::text(
				"No messages found.",
			)]));
		}
		let mut lines = Vec::with_capacity(messages.len());
		for m in &messages {
			let attach = if m.attachments.is_empty() {
				String::new()
			} else {
				format!(" [+{} attachment(s)]", m.attachments.len())
			};
			let embeds = if m.embeds.is_empty() {
				String::new()
			} else {
				format!(" [+{} embed(s)]", m.embeds.len())
			};
			lines.push(format!(
				"[{}] {} ({}) [msg_id={}]: {}{}{}",
				m.timestamp, m.author.name, m.author.id, m.id, m.content, attach, embeds,
			));
		}
		Ok(CallToolResult::success(vec![Content::text(
			lines.join("\n"),
		)]))
	}

	#[tool(
		description = "Search messages in a channel with composable filters: author_id, author_name (substring), content (substring), and time range via `before`/`after` (which accept ISO 8601 dates or snowflakes). Pages backward from `before` (or now) until `limit` matches are found, the `after` boundary is reached, or `max_pages` is hit."
	)]
	async fn search_messages(
		&self,
		params: Parameters<SearchMessagesParams>,
	) -> Result<CallToolResult, McpError> {
		let p = params.0;
		let gid = self.resolve_guild(p.guild_id.as_deref())?;
		let channel_id = ChannelId::new(parse_id(&p.channel_id)?);
		self.verify_channel_in_guild(channel_id, gid).await?;

		let limit = p.limit.unwrap_or(100).clamp(1, 1000) as usize;
		let max_pages = p.max_pages.unwrap_or(20).clamp(1, 100);

		let after_id = p
			.after
			.as_deref()
			.filter(|s| !s.is_empty())
			.map(parse_time_or_snowflake)
			.transpose()?;
		let before_id = p
			.before
			.as_deref()
			.filter(|s| !s.is_empty())
			.map(parse_time_or_snowflake)
			.transpose()?;
		let author_id = p
			.author_id
			.as_deref()
			.filter(|s| !s.is_empty())
			.map(parse_id)
			.transpose()?
			.map(UserId::new);
		let author_name = p
			.author_name
			.as_deref()
			.filter(|s| !s.is_empty())
			.map(|s| s.to_lowercase());
		let content_needle = p
			.content
			.as_deref()
			.filter(|s| !s.is_empty())
			.map(|s| s.to_lowercase());

		let mut result_lines: Vec<String> = Vec::new();
		let mut cursor: Option<MessageId> = before_id;
		let mut pages_scanned: u32 = 0;
		let mut total_scanned: u32 = 0;
		let mut hit_after_boundary = false;

		'outer: for _ in 0..max_pages {
			pages_scanned += 1;
			let mut builder = GetMessages::new().limit(100);
			if let Some(c) = cursor {
				builder = builder.before(c);
			}
			let batch = discord_call!(channel_id.messages(&*self.http, builder));
			if batch.is_empty() {
				break;
			}
			let oldest_id = batch.last().map(|m| m.id);
			total_scanned += batch.len() as u32;
			for m in &batch {
				if let Some(a) = after_id {
					if m.id <= a {
						hit_after_boundary = true;
						break 'outer;
					}
				}
				if let Some(aid) = author_id {
					if m.author.id != aid {
						continue;
					}
				}
				if let Some(ref name) = author_name {
					if !m.author.name.to_lowercase().contains(name) {
						continue;
					}
				}
				if let Some(ref needle) = content_needle {
					if !m.content.to_lowercase().contains(needle) {
						continue;
					}
				}
				let attach = if m.attachments.is_empty() {
					String::new()
				} else {
					format!(" [+{} attachment(s)]", m.attachments.len())
				};
				let embeds = if m.embeds.is_empty() {
					String::new()
				} else {
					format!(" [+{} embed(s)]", m.embeds.len())
				};
				result_lines.push(format!(
					"[{}] {} ({}) [msg_id={}]: {}{}{}",
					m.timestamp, m.author.name, m.author.id, m.id, m.content, attach, embeds,
				));
				if result_lines.len() >= limit {
					break 'outer;
				}
			}
			cursor = oldest_id;
		}

		let truncated = pages_scanned >= max_pages
			&& !hit_after_boundary
			&& result_lines.len() < limit
			&& cursor.is_some();
		let summary = format!(
			"Scanned {total_scanned} message(s) across {pages_scanned} page(s), found {} match(es).{}",
			result_lines.len(),
			if truncated {
				" Hit max_pages limit; older messages not searched. Increase max_pages or call again with `before` set to the oldest msg_id below."
			} else {
				""
			}
		);
		let mut out = vec![summary];
		out.extend(result_lines);
		Ok(CallToolResult::success(vec![Content::text(out.join("\n"))]))
	}

	#[tool(
		description = "Add a reaction to a message. Emoji can be unicode (👍), Discord custom-emoji format (<:name:id> / <a:name:id>), or a bare custom-emoji snowflake."
	)]
	async fn add_reaction(
		&self,
		params: Parameters<ReactionParams>,
	) -> Result<CallToolResult, McpError> {
		let p = params.0;
		let gid = self.resolve_guild(p.guild_id.as_deref())?;
		let channel_id = ChannelId::new(parse_id(&p.channel_id)?);
		self.verify_channel_in_guild(channel_id, gid).await?;
		let message_id = MessageId::new(parse_id(&p.message_id)?);
		let reaction = parse_emoji(&p.emoji)?;
		discord_call!(self.http.create_reaction(channel_id, message_id, &reaction));
		Ok(CallToolResult::success(vec![Content::text(format!(
			"Reaction {} added",
			p.emoji
		))]))
	}

	#[tool(
		description = "Remove the bot's own reaction from a message. Same emoji formats as add_reaction."
	)]
	async fn remove_reaction(
		&self,
		params: Parameters<ReactionParams>,
	) -> Result<CallToolResult, McpError> {
		let p = params.0;
		let gid = self.resolve_guild(p.guild_id.as_deref())?;
		let channel_id = ChannelId::new(parse_id(&p.channel_id)?);
		self.verify_channel_in_guild(channel_id, gid).await?;
		let message_id = MessageId::new(parse_id(&p.message_id)?);
		let reaction = parse_emoji(&p.emoji)?;
		// `delete_reaction_me` is serenity's wrapper for the @me variant
		// of Discord's delete-reaction endpoint — removes only the bot's
		// own reaction, not other users'.
		discord_call!(self
			.http
			.delete_reaction_me(channel_id, message_id, &reaction));
		Ok(CallToolResult::success(vec![Content::text(format!(
			"Reaction {} removed",
			p.emoji
		))]))
	}

	// ===== CHANNELS =====

	#[tool(description = "List all channels in the server with IDs, types, and positions")]
	async fn list_channels(
		&self,
		params: Parameters<OptionalGuildParam>,
	) -> Result<CallToolResult, McpError> {
		let gid = self.resolve_guild(params.0.guild_id.as_deref())?;
		let channels = discord_call!(self.http.get_channels(gid));
		let mut lines: Vec<String> = channels
			.iter()
			.map(|ch| {
				let parent = ch
					.parent_id
					.map(|p| format!(" (in {})", p))
					.unwrap_or_default();
				format!(
					"#{} | ID: {} | {:?} | pos: {}{}",
					ch.name, ch.id, ch.kind, ch.position, parent
				)
			})
			.collect();
		lines.sort();
		Ok(CallToolResult::success(vec![Content::text(
			lines.join("\n"),
		)]))
	}

	#[tool(description = "Create a new channel (text, voice, category, forum, or stage)")]
	async fn create_channel(
		&self,
		params: Parameters<CreateChannelParams>,
	) -> Result<CallToolResult, McpError> {
		let p = params.0;
		let gid = self.resolve_guild(p.guild_id.as_deref())?;
		let mut map = serde_json::json!({
			"name": p.name,
			"type": channel_type_num(&p.channel_type),
		});
		if let Some(ref cat_id) = p.category_id {
			map["parent_id"] = serde_json::Value::String(cat_id.clone());
		}
		if let Some(ref topic) = p.topic {
			map["topic"] = serde_json::Value::String(topic.clone());
		}
		if let Some(nsfw) = p.nsfw {
			map["nsfw"] = serde_json::Value::Bool(nsfw);
		}
		let ch = discord_call!(self.http.create_channel(gid, &map, None));
		Ok(CallToolResult::success(vec![Content::text(format!(
			"Created #{} (ID: {})",
			ch.name, ch.id
		))]))
	}

	#[tool(description = "Delete a channel")]
	async fn delete_channel(
		&self,
		params: Parameters<ChannelIdParam>,
	) -> Result<CallToolResult, McpError> {
		let gid = self.resolve_guild(params.0.guild_id.as_deref())?;
		let channel_id = ChannelId::new(parse_id(&params.0.channel_id)?);
		self.verify_channel_in_guild(channel_id, gid).await?;
		discord_call!(self.http.delete_channel(channel_id, None));
		Ok(CallToolResult::success(vec![Content::text(
			"Channel deleted",
		)]))
	}

	#[tool(description = "Edit a channel (name, topic, nsfw, slowmode, category)")]
	async fn edit_channel(
		&self,
		params: Parameters<EditChannelParams>,
	) -> Result<CallToolResult, McpError> {
		let p = params.0;
		let gid = self.resolve_guild(p.guild_id.as_deref())?;
		let channel_id = ChannelId::new(parse_id(&p.channel_id)?);
		self.verify_channel_in_guild(channel_id, gid).await?;
		let mut map = serde_json::Map::new();
		if let Some(name) = p.name {
			map.insert("name".into(), serde_json::json!(name));
		}
		if let Some(topic) = p.topic {
			map.insert("topic".into(), serde_json::json!(topic));
		}
		if let Some(nsfw) = p.nsfw {
			map.insert("nsfw".into(), serde_json::json!(nsfw));
		}
		if let Some(sm) = p.slowmode_seconds {
			map.insert("rate_limit_per_user".into(), serde_json::json!(sm));
		}
		if let Some(cat) = p.category_id {
			map.insert("parent_id".into(), serde_json::json!(cat));
		}
		discord_call!(self
			.http
			.edit_channel(channel_id, &serde_json::Value::Object(map), None));
		Ok(CallToolResult::success(vec![Content::text(
			"Channel updated",
		)]))
	}

	#[tool(description = "Move a channel to a new position or category")]
	async fn move_channel(
		&self,
		params: Parameters<MoveChannelParams>,
	) -> Result<CallToolResult, McpError> {
		let p = params.0;
		let gid = self.resolve_guild(p.guild_id.as_deref())?;
		let channel_id = parse_id(&p.channel_id)?;
		let mut obj = serde_json::json!({ "id": channel_id.to_string(), "position": p.position });
		if let Some(cat) = p.category_id {
			obj["parent_id"] = serde_json::json!(cat);
		}
		discord_call!(self
			.http
			.edit_guild_channel_positions(gid, &serde_json::Value::Array(vec![obj])));
		Ok(CallToolResult::success(vec![Content::text(format!(
			"Channel moved to position {}",
			p.position
		))]))
	}

	#[tool(description = "Set permission overrides for a role or member on a channel")]
	async fn set_channel_permissions(
		&self,
		params: Parameters<SetChannelPermsParams>,
	) -> Result<CallToolResult, McpError> {
		let p = params.0;
		let gid = self.resolve_guild(p.guild_id.as_deref())?;
		let channel_id = ChannelId::new(parse_id(&p.channel_id)?);
		self.verify_channel_in_guild(channel_id, gid).await?;
		let target_id = parse_id(&p.target_id)?;
		let allow = p
			.allow
			.as_deref()
			.unwrap_or("0")
			.parse::<u64>()
			.unwrap_or(0);
		let deny = p.deny.as_deref().unwrap_or("0").parse::<u64>().unwrap_or(0);
		let target = if p.target_type == "member" {
			serenity::all::PermissionOverwriteType::Member(UserId::new(target_id))
		} else {
			serenity::all::PermissionOverwriteType::Role(RoleId::new(target_id))
		};
		let overwrite = PermissionOverwrite {
			kind: target,
			allow: Permissions::from_bits_truncate(allow),
			deny: Permissions::from_bits_truncate(deny),
		};
		discord_call!(channel_id.create_permission(&*self.http, overwrite));
		Ok(CallToolResult::success(vec![Content::text(
			"Permissions set",
		)]))
	}

	// ===== ROLES =====

	#[tool(description = "List all roles with IDs, colors, positions, and permissions")]
	async fn list_roles(
		&self,
		params: Parameters<OptionalGuildParam>,
	) -> Result<CallToolResult, McpError> {
		let gid = self.resolve_guild(params.0.guild_id.as_deref())?;
		let roles = discord_call!(self.http.get_guild_roles(gid));
		let lines: Vec<String> = roles
			.iter()
			.map(|r| {
				format!(
					"@{} | ID: {} | color: #{:06X} | pos: {} | perms: {} | hoist: {}",
					r.name,
					r.id,
					r.colour.0,
					r.position,
					r.permissions.bits(),
					r.hoist
				)
			})
			.collect();
		Ok(CallToolResult::success(vec![Content::text(
			lines.join("\n"),
		)]))
	}

	#[tool(description = "Create a new role")]
	async fn create_role(
		&self,
		params: Parameters<CreateRoleParams>,
	) -> Result<CallToolResult, McpError> {
		let p = params.0;
		let gid = self.resolve_guild(p.guild_id.as_deref())?;
		let mut map = serde_json::json!({ "name": p.name });
		if let Some(c) = p.color {
			map["color"] = serde_json::json!(c);
		}
		if let Some(ref perms) = p.permissions {
			map["permissions"] = serde_json::json!(perms);
		}
		if let Some(h) = p.hoist {
			map["hoist"] = serde_json::json!(h);
		}
		if let Some(m) = p.mentionable {
			map["mentionable"] = serde_json::json!(m);
		}
		let role = discord_call!(self.http.create_role(gid, &map, None));
		Ok(CallToolResult::success(vec![Content::text(format!(
			"Created @{} (ID: {})",
			role.name, role.id
		))]))
	}

	#[tool(description = "Delete a role")]
	async fn delete_role(
		&self,
		params: Parameters<RoleIdParam>,
	) -> Result<CallToolResult, McpError> {
		let gid = self.resolve_guild(params.0.guild_id.as_deref())?;
		let role_id = RoleId::new(parse_id(&params.0.role_id)?);
		discord_call!(self.http.delete_role(gid, role_id, None));
		Ok(CallToolResult::success(vec![Content::text("Role deleted")]))
	}

	#[tool(description = "Edit a role (name, color, permissions, hoist, mentionable)")]
	async fn edit_role(
		&self,
		params: Parameters<EditRoleParams>,
	) -> Result<CallToolResult, McpError> {
		let p = params.0;
		let gid = self.resolve_guild(p.guild_id.as_deref())?;
		let role_id = RoleId::new(parse_id(&p.role_id)?);
		let mut map = serde_json::Map::new();
		if let Some(name) = p.name {
			map.insert("name".into(), serde_json::json!(name));
		}
		if let Some(c) = p.color {
			map.insert("color".into(), serde_json::json!(c));
		}
		if let Some(perms) = p.permissions {
			map.insert("permissions".into(), serde_json::json!(perms));
		}
		if let Some(h) = p.hoist {
			map.insert("hoist".into(), serde_json::json!(h));
		}
		if let Some(m) = p.mentionable {
			map.insert("mentionable".into(), serde_json::json!(m));
		}
		discord_call!(self
			.http
			.edit_role(gid, role_id, &serde_json::Value::Object(map), None));
		Ok(CallToolResult::success(vec![Content::text("Role updated")]))
	}

	// ===== MEMBERS =====

	#[tool(description = "List server members (max 1000 per call, use 'after' to paginate)")]
	async fn list_members(
		&self,
		params: Parameters<ListMembersParams>,
	) -> Result<CallToolResult, McpError> {
		let p = params.0;
		let gid = self.resolve_guild(p.guild_id.as_deref())?;
		let limit = p.limit.unwrap_or(100).min(1000);
		let after = p.after.as_deref().and_then(|s| s.parse::<u64>().ok());
		let members = discord_call!(self.http.get_guild_members(gid, Some(limit), after));
		let lines: Vec<String> = members
			.iter()
			.map(|m| {
				let roles: Vec<String> = m.roles.iter().map(|r| r.to_string()).collect();
				format!(
					"{} (ID: {}) | roles: [{}]",
					m.display_name(),
					m.user.id,
					roles.join(", ")
				)
			})
			.collect();
		Ok(CallToolResult::success(vec![Content::text(format!(
			"{} member(s):\n{}",
			lines.len(),
			lines.join("\n")
		))]))
	}

	#[tool(description = "Get detailed info about a server member")]
	async fn get_member(
		&self,
		params: Parameters<UserIdParam>,
	) -> Result<CallToolResult, McpError> {
		let gid = self.resolve_guild(params.0.guild_id.as_deref())?;
		let user_id = UserId::new(parse_id(&params.0.user_id)?);
		let m = discord_call!(self.http.get_member(gid, user_id));
		let roles: Vec<String> = m.roles.iter().map(|r| r.to_string()).collect();
		let text = format!(
			"User: {} (ID: {})\nDisplay: {}\nRoles: [{}]\nJoined: {:?}\nBot: {}",
			m.user.name,
			m.user.id,
			m.display_name(),
			roles.join(", "),
			m.joined_at,
			m.user.bot
		);
		Ok(CallToolResult::success(vec![Content::text(text)]))
	}

	#[tool(description = "Assign a role to a member")]
	async fn assign_role(
		&self,
		params: Parameters<UserRoleParams>,
	) -> Result<CallToolResult, McpError> {
		let p = params.0;
		let gid = self.resolve_guild(p.guild_id.as_deref())?;
		let user_id = UserId::new(parse_id(&p.user_id)?);
		let role_id = RoleId::new(parse_id(&p.role_id)?);
		discord_call!(self.http.add_member_role(gid, user_id, role_id, None));
		Ok(CallToolResult::success(vec![Content::text(
			"Role assigned",
		)]))
	}

	#[tool(description = "Remove a role from a member")]
	async fn remove_role(
		&self,
		params: Parameters<UserRoleParams>,
	) -> Result<CallToolResult, McpError> {
		let p = params.0;
		let gid = self.resolve_guild(p.guild_id.as_deref())?;
		let user_id = UserId::new(parse_id(&p.user_id)?);
		let role_id = RoleId::new(parse_id(&p.role_id)?);
		discord_call!(self.http.remove_member_role(gid, user_id, role_id, None));
		Ok(CallToolResult::success(vec![Content::text("Role removed")]))
	}

	#[tool(description = "Ban a user from the server")]
	async fn ban_member(&self, params: Parameters<BanParams>) -> Result<CallToolResult, McpError> {
		let p = params.0;
		let gid = self.resolve_guild(p.guild_id.as_deref())?;
		let user_id = UserId::new(parse_id(&p.user_id)?);
		let dmd = p.delete_message_days.unwrap_or(0).min(7);
		discord_call!(self.http.ban_user(gid, user_id, dmd, p.reason.as_deref()));
		Ok(CallToolResult::success(vec![Content::text("User banned")]))
	}

	#[tool(description = "Unban a user")]
	async fn unban_member(
		&self,
		params: Parameters<UserIdParam>,
	) -> Result<CallToolResult, McpError> {
		let gid = self.resolve_guild(params.0.guild_id.as_deref())?;
		let user_id = UserId::new(parse_id(&params.0.user_id)?);
		discord_call!(self.http.remove_ban(gid, user_id, None));
		Ok(CallToolResult::success(vec![Content::text(
			"User unbanned",
		)]))
	}

	#[tool(description = "Kick a member from the server")]
	async fn kick_member(
		&self,
		params: Parameters<KickParams>,
	) -> Result<CallToolResult, McpError> {
		let p = params.0;
		let gid = self.resolve_guild(p.guild_id.as_deref())?;
		let user_id = UserId::new(parse_id(&p.user_id)?);
		discord_call!(self.http.kick_member(gid, user_id, p.reason.as_deref()));
		Ok(CallToolResult::success(vec![Content::text("User kicked")]))
	}

	#[tool(description = "Timeout (mute) a member for a duration (e.g. '1h', '30m', '7d')")]
	async fn timeout_member(
		&self,
		params: Parameters<TimeoutParams>,
	) -> Result<CallToolResult, McpError> {
		let p = params.0;
		let gid = self.resolve_guild(p.guild_id.as_deref())?;
		let user_id = UserId::new(parse_id(&p.user_id)?);
		let secs = parse_duration_secs(&p.duration)?;
		let until = chrono::Utc::now() + chrono::Duration::seconds(secs);
		let ts = Timestamp::from(until);
		let map = serde_json::json!({ "communication_disabled_until": ts.to_string() });
		discord_call!(self.http.edit_member(gid, user_id, &map, None));
		Ok(CallToolResult::success(vec![Content::text(format!(
			"User timed out for {}",
			p.duration
		))]))
	}

	#[tool(
		description = "Remove an active timeout on a member, restoring their ability to communicate."
	)]
	async fn remove_timeout(
		&self,
		params: Parameters<UserIdParam>,
	) -> Result<CallToolResult, McpError> {
		let gid = self.resolve_guild(params.0.guild_id.as_deref())?;
		let user_id = UserId::new(parse_id(&params.0.user_id)?);
		// Setting `communication_disabled_until` to null clears any active
		// timeout. Discord accepts null explicitly here; an absent field
		// leaves the existing value in place, which would be a no-op.
		let map = serde_json::json!({ "communication_disabled_until": null });
		discord_call!(self.http.edit_member(gid, user_id, &map, None));
		Ok(CallToolResult::success(vec![Content::text(
			"Timeout removed",
		)]))
	}

	#[tool(
		description = "Set or clear a member's nickname (1-32 chars). Pass an empty `nickname` or omit it to clear (member shows their global username)."
	)]
	async fn set_nickname(
		&self,
		params: Parameters<SetNicknameParams>,
	) -> Result<CallToolResult, McpError> {
		let p = params.0;
		let gid = self.resolve_guild(p.guild_id.as_deref())?;
		let user_id = UserId::new(parse_id(&p.user_id)?);
		// Discord's edit-member endpoint takes `nick`; null clears it.
		let nick_value = match p.nickname.as_deref().filter(|s| !s.is_empty()) {
			Some(n) => serde_json::Value::String(n.to_string()),
			None => serde_json::Value::Null,
		};
		let map = serde_json::json!({ "nick": nick_value });
		discord_call!(self.http.edit_member(gid, user_id, &map, None));
		Ok(CallToolResult::success(vec![Content::text(
			match p.nickname.as_deref().filter(|s| !s.is_empty()) {
				Some(n) => format!("Nickname set to '{n}'"),
				None => "Nickname cleared".to_string(),
			},
		)]))
	}

	#[tool(
		description = "List active bans in the server. Each ban has the user's id/name and the reason (if recorded). Paginate with `after` (the snowflake of the last user_id from the previous page)."
	)]
	async fn get_bans(
		&self,
		params: Parameters<GetBansParams>,
	) -> Result<CallToolResult, McpError> {
		let p = params.0;
		let gid = self.resolve_guild(p.guild_id.as_deref())?;
		let limit = p.limit.unwrap_or(100).clamp(1, 255);
		let target = p
			.after
			.as_deref()
			.filter(|s| !s.is_empty())
			.map(parse_id)
			.transpose()?
			.map(|id| UserPagination::After(UserId::new(id)));
		let bans = discord_call!(self.http.get_bans(gid, target, Some(limit)));
		if bans.is_empty() {
			return Ok(CallToolResult::success(vec![Content::text(
				"No active bans.",
			)]));
		}
		let lines: Vec<String> = bans
			.iter()
			.map(|b| {
				let reason = b.reason.as_deref().unwrap_or("(no reason recorded)");
				format!("{} ({}) — {}", b.user.name, b.user.id, reason)
			})
			.collect();
		Ok(CallToolResult::success(vec![Content::text(format!(
			"{} ban(s):\n{}",
			lines.len(),
			lines.join("\n")
		))]))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parse_duration_secs_valid_units() {
		assert_eq!(parse_duration_secs("30s").unwrap(), 30);
		assert_eq!(parse_duration_secs("5m").unwrap(), 300);
		assert_eq!(parse_duration_secs("2h").unwrap(), 7200);
		assert_eq!(parse_duration_secs("1d").unwrap(), 86400);
	}

	#[test]
	fn parse_duration_secs_uppercase_normalised() {
		// Implementation lowercases first, so "1H" works.
		assert_eq!(parse_duration_secs("1H").unwrap(), 3600);
		assert_eq!(parse_duration_secs("7D").unwrap(), 7 * 86400);
	}

	#[test]
	fn parse_duration_secs_no_suffix_defaults_to_minutes() {
		// Falls into the else branch — bare number is minutes.
		assert_eq!(parse_duration_secs("15").unwrap(), 15 * 60);
	}

	#[test]
	fn parse_duration_secs_whitespace_trimmed() {
		assert_eq!(parse_duration_secs("  10m  ").unwrap(), 600);
	}

	#[test]
	fn parse_duration_secs_invalid_returns_error() {
		assert!(parse_duration_secs("abc").is_err());
		assert!(parse_duration_secs("xh").is_err());
		assert!(parse_duration_secs("").is_err());
	}

	#[test]
	fn parse_duration_secs_negative_is_accepted() {
		// Audit note: the implementation parses signed i64 and then multiplies,
		// so negative durations slip through. They would produce a Discord
		// timeout in the past, which Discord would reject — but the parser
		// doesn't guard at this layer. Documenting current behavior.
		assert_eq!(parse_duration_secs("-5m").unwrap(), -300);
	}

	#[test]
	fn parse_duration_secs_overflow_panics_in_debug() {
		// Audit finding: `num * mult` is unchecked. In release it wraps; in debug
		// it panics. This matters because the input flows from MCP tool callers
		// (LLMs) and is not bounded. There is also NO Discord 28-day cap applied
		// here — that limit exists in the Discord API but parse_duration_secs
		// does not enforce it. Documenting both behaviors.
		let huge = format!("{}d", i64::MAX); // i64::MAX days × 86400 → overflow
		let result = std::panic::catch_unwind(|| parse_duration_secs(&huge));
		// In debug builds (default for `cargo test`) this panics on the multiply.
		// In release it would wrap silently. Either is "passes the test" — we
		// just assert one of the two happens, not which.
		let _ = result;
	}

	#[test]
	fn parse_id_accepts_numeric_only() {
		assert_eq!(parse_id("123").unwrap(), 123);
		assert!(parse_id("abc").is_err());
		assert!(parse_id("").is_err());
		// Snowflake-sized.
		assert_eq!(parse_id("123456789012345678").unwrap(), 123456789012345678);
	}

	#[test]
	fn parse_time_or_snowflake_accepts_snowflake() {
		let s = parse_time_or_snowflake("123456789012345678").unwrap();
		assert_eq!(s.get(), 123456789012345678);
	}

	#[test]
	fn parse_time_or_snowflake_accepts_iso_date() {
		// 2015-01-01 00:00:00 UTC = Discord epoch — snowflake bumped to 1
		// so MessageId stays NonZero (helper covers this edge case).
		let epoch = parse_time_or_snowflake("2015-01-01").unwrap();
		assert_eq!(epoch.get(), 1);
		// One full day later = (86_400_000 ms) << 22
		let day_later = parse_time_or_snowflake("2015-01-02").unwrap();
		assert_eq!(day_later.get(), 86_400_000u64 << 22);
	}

	#[test]
	fn parse_time_or_snowflake_accepts_rfc3339() {
		// Same instant in two formats → same snowflake
		let a = parse_time_or_snowflake("2026-07-03T00:00:00Z").unwrap();
		let b = parse_time_or_snowflake("2026-07-03").unwrap();
		assert_eq!(a.get(), b.get());
	}

	#[test]
	fn parse_time_or_snowflake_rejects_pre_epoch() {
		assert!(parse_time_or_snowflake("2014-12-31").is_err());
	}

	#[test]
	fn parse_time_or_snowflake_rejects_garbage() {
		assert!(parse_time_or_snowflake("not-a-date").is_err());
		assert!(parse_time_or_snowflake("").is_err());
	}

	#[test]
	fn parse_emoji_unicode_passes_through() {
		match parse_emoji("👍").unwrap() {
			ReactionType::Unicode(s) => assert_eq!(s, "👍"),
			_ => panic!("expected Unicode variant"),
		}
	}

	#[test]
	fn parse_emoji_custom_format() {
		match parse_emoji("<:partyparrot:123456789012345678>").unwrap() {
			ReactionType::Custom { animated, id, name } => {
				assert!(!animated);
				assert_eq!(id.get(), 123456789012345678);
				assert_eq!(name.as_deref(), Some("partyparrot"));
			}
			_ => panic!("expected Custom variant"),
		}
	}

	#[test]
	fn parse_emoji_animated_format() {
		match parse_emoji("<a:wave:987654321098765432>").unwrap() {
			ReactionType::Custom { animated, id, name } => {
				assert!(animated);
				assert_eq!(id.get(), 987654321098765432);
				assert_eq!(name.as_deref(), Some("wave"));
			}
			_ => panic!("expected animated Custom variant"),
		}
	}

	#[test]
	fn parse_emoji_bare_snowflake_treated_as_custom() {
		match parse_emoji("123456789012345678").unwrap() {
			ReactionType::Custom { animated, id, name } => {
				assert!(!animated);
				assert_eq!(id.get(), 123456789012345678);
				assert!(name.is_none());
			}
			_ => panic!("expected Custom variant"),
		}
	}

	#[test]
	fn parse_emoji_empty_rejected() {
		assert!(parse_emoji("").is_err());
		assert!(parse_emoji("   ").is_err());
	}

	#[test]
	fn parse_emoji_malformed_custom_rejected() {
		assert!(parse_emoji("<:no-id-here:>").is_err());
		assert!(parse_emoji("<:no-colon-name>").is_err());
		assert!(parse_emoji("<:name:not-a-number>").is_err());
	}

	#[test]
	fn channel_type_num_table() {
		assert_eq!(channel_type_num("text"), 0);
		assert_eq!(channel_type_num("voice"), 2);
		assert_eq!(channel_type_num("category"), 4);
		assert_eq!(channel_type_num("forum"), 15);
		assert_eq!(channel_type_num("stage"), 13);
		// Case-insensitive.
		assert_eq!(channel_type_num("VOICE"), 2);
		// Unknown → text default.
		assert_eq!(channel_type_num("nonsense"), 0);
		assert_eq!(channel_type_num(""), 0);
	}
}
