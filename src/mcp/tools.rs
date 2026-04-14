use rmcp::handler::server::wrapper::Parameters;
use rmcp::{
	handler::server::router::tool::ToolRouter, handler::server::tool::ToolCallContext, model::*,
	service::RequestContext, tool, tool_router, ErrorData as McpError, RoleServer, ServerHandler,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
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

// --- Helpers ---

fn parse_id(s: &str) -> Result<u64, McpError> {
	s.parse::<u64>()
		.map_err(|_| McpError::invalid_params(format!("Invalid ID: {s}"), None))
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
		let count = p.count.min(100).max(1);
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
		let channel_id = ChannelId::new(parse_id(&params.0.channel_id)?);
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
		let target_id = parse_id(&p.target_id)?;
		let allow = p
			.allow
			.as_deref()
			.unwrap_or("0")
			.parse::<u64>()
			.unwrap_or(0);
		let deny = p.deny.as_deref().unwrap_or("0").parse::<u64>().unwrap_or(0);
		let perm_type = if p.target_type == "member" { "1" } else { "0" };
		let map = serde_json::json!({ "allow": allow.to_string(), "deny": deny.to_string(), "type": perm_type });
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
}
