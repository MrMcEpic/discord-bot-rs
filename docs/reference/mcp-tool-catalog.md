# MCP Tool Catalog

Complete catalog of MCP tools exposed by the embedded MCP server. discord-bot-rs ships with an in-process [Model Context Protocol](https://modelcontextprotocol.io/) server that lets any MCP-compatible client (Claude, your editor, automation scripts) drive the bot's Discord guild over HTTP. See [MCP Server](../features/mcp-server.md) for a feature-level overview and [MCP Exposure](../deployment/mcp-exposure.md) for connection details.

All tools live in [`src/mcp/tools.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/mcp/tools.rs) and are registered on the `DiscordTools` router via the `rmcp` `#[tool]` macro.

## Conventions

- **`guild_id`** is optional on every tool that accepts it. When omitted, the tool falls back to the bot instance's configured guild (`GUILD_ID`). Pass an explicit guild ID only when calling a multi-guild bot.
- **IDs** are passed as decimal strings (Discord snowflakes do not fit in JSON's safe-integer range).
- **Timeouts**: every Discord API call is wrapped in a 10 s timeout; the tool returns an error result if Discord doesn't respond in time.
- **Return format**: all tools return human-readable plain text inside a `Content::text` block. There is no machine-parseable JSON return type — these tools are designed for an LLM in the loop.
- **Permissions**: the bot account itself must have permission to perform the underlying action; MCP tools do not bypass Discord's permission model. The `send_message` tool is flagged as privileged in its description and the README recommends configuring your client to require manual approval for it.

## Guilds

### `list_guilds`

List every Discord server (guild) the bot is currently a member of.

**Parameters:** none.

**Example:**
```json
{
  "name": "list_guilds",
  "arguments": {}
}
```

**Returns:** A line per guild in the form `<name> | ID: <snowflake>`, prefixed with the total count.

## Server

### `get_guild_info`

Get summary information about a server: name, owner, approximate member count, and channel/role counts.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |

**Example:**
```json
{
  "name": "get_guild_info",
  "arguments": {}
}
```

**Returns:** A multi-line text block with `Server`, `ID`, `Owner`, `Approx Members`, `Channels`, and `Roles` fields.

### `send_message`

Send a plain-text message to a channel. **Privileged** — the source code marks this as something a client should require manual approval for.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `channel_id` | string | yes | Target channel snowflake. |
| `content` | string | yes | Message body. |

**Example:**
```json
{
  "name": "send_message",
  "arguments": {
    "channel_id": "1234567890123456789",
    "content": "Hello from MCP."
  }
}
```

**Returns:** `Message sent (ID: <snowflake>)`.

### `delete_messages`

Bulk-delete the most recent messages from a channel (1–100). Falls back to a single-message delete if only one message is in scope. Subject to Discord's 14-day bulk-delete restriction.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `channel_id` | string | yes | Target channel snowflake. |
| `count` | integer (1–100) | yes | Number of recent messages to delete. Clamped server-side. |

**Example:**
```json
{
  "name": "delete_messages",
  "arguments": {
    "channel_id": "1234567890123456789",
    "count": 25
  }
}
```

**Returns:** `Deleted N message(s)`.

### `get_recent_messages`

Fetch recent messages from a channel, newest first. Each message is returned on its own line as `[timestamp] author_name (author_id) [msg_id=...]: content` followed by `[+N attachment(s)]` and `[+N embed(s)]` markers when present. Use the `before` parameter to paginate backward — pass the oldest `msg_id` from the previous response.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild; used to verify the channel belongs to that guild before reading. |
| `channel_id` | string | yes | Target channel snowflake. |
| `limit` | integer (1–100) | no | Number of messages to fetch. Defaults to 50, clamped server-side. |
| `before` | string | no | Message snowflake. If set, only messages older than this ID are returned. |

**Example:**
```json
{
  "name": "get_recent_messages",
  "arguments": {
    "channel_id": "1234567890123456789",
    "limit": 25
  }
}
```

**Returns:** Newline-separated lines, one per message, or `No messages found.` if the channel is empty in the requested window.

### `search_messages`

Search a channel for messages matching one or more filters. All filters compose: pass an `author_id` plus a date range to find what someone said in July, or `content` plus `author_name` for a substring match scoped to one user. The implementation pages backward from `before` (or "now") in batches of 100, applying the filters client-side, and stops when `limit` matches are collected, the `after` boundary is reached, or `max_pages` (the safety cap on Discord API calls) is hit. The first line of the response is a summary stating how many messages were scanned and whether the search was truncated; subsequent lines are the matched messages in the same format as `get_recent_messages`.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild; used to verify the channel belongs to that guild. |
| `channel_id` | string | yes | Target channel snowflake. |
| `author_id` | string | no | Filter to messages from this user snowflake. |
| `author_name` | string | no | Filter by case-insensitive substring of the author's username. |
| `content` | string | no | Filter by case-insensitive substring of the message body. |
| `after` | string | no | Lower time bound. ISO 8601 date (`2026-07-03` or `2026-07-03T12:00:00Z`) or a Discord snowflake. Older messages are not returned. |
| `before` | string | no | Upper time bound. Same format as `after`. Newer messages are not returned. |
| `limit` | integer (1-1000) | no | Max matching messages to return. Default 100, clamped server-side. |
| `max_pages` | integer (1-100) | no | Max API pages of 100 messages each to scan. Default 20 (= 2000 messages of search depth). Raise it for deep searches; the response says when the cap was hit. |

**Examples:**

All messages in a single day:
```json
{
  "name": "search_messages",
  "arguments": {
    "channel_id": "1234567890123456789",
    "after": "2026-07-03",
    "before": "2026-07-04",
    "limit": 1000,
    "max_pages": 50
  }
}
```

All messages from one user in a window:
```json
{
  "name": "search_messages",
  "arguments": {
    "channel_id": "1234567890123456789",
    "author_id": "9876543210987654321",
    "after": "2026-07-01",
    "before": "2026-08-01"
  }
}
```

Find a phrase from a specific person:
```json
{
  "name": "search_messages",
  "arguments": {
    "channel_id": "1234567890123456789",
    "author_name": "epic",
    "content": "deployment"
  }
}
```

**Returns:** A summary line followed by one line per matched message. The summary names the totals scanned and called out if the search was truncated by `max_pages`. To continue a truncated search, call again with `before` set to the oldest `msg_id` returned.

### `add_reaction`

Add a reaction to a message. Useful for AI-driven moderation flows ("react with ✅ when handled") or lightweight feedback signals.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild; used to verify the channel belongs to it. |
| `channel_id` | string | yes | Channel containing the message. |
| `message_id` | string | yes | Target message snowflake. |
| `emoji` | string | yes | Unicode emoji (`👍`), Discord custom-emoji format (`<:name:id>` or `<a:name:id>` for animated), or a bare custom-emoji snowflake. |

**Example:**
```json
{
  "name": "add_reaction",
  "arguments": {
    "channel_id": "1234567890123456789",
    "message_id": "1234567890123456790",
    "emoji": "✅"
  }
}
```

**Returns:** `Reaction <emoji> added`.

### `remove_reaction`

Remove **the bot's own** reaction from a message. Cannot be used to clear other users' reactions.

**Parameters:** identical to `add_reaction`.

**Example:**
```json
{
  "name": "remove_reaction",
  "arguments": {
    "channel_id": "1234567890123456789",
    "message_id": "1234567890123456790",
    "emoji": "✅"
  }
}
```

**Returns:** `Reaction <emoji> removed`.

## Channels

### `list_channels`

List every channel in the guild with ID, type, position, and parent category.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |

**Example:**
```json
{
  "name": "list_channels",
  "arguments": {}
}
```

**Returns:** Sorted lines like `#general | ID: <snowflake> | Text | pos: 0 (in <parent>)`.

### `create_channel`

Create a new channel. Supports text, voice, category, forum, and stage channels.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `name` | string | yes | Channel name. |
| `channel_type` | string | no (default `text`) | One of `text`, `voice`, `category`, `forum`, `stage`. |
| `category_id` | string | no | Parent category snowflake. |
| `topic` | string | no | Channel topic (text channels). |
| `nsfw` | boolean | no | Mark channel NSFW. |

**Example:**
```json
{
  "name": "create_channel",
  "arguments": {
    "name": "announcements",
    "channel_type": "text",
    "topic": "One-way broadcasts only"
  }
}
```

**Returns:** `Created #<name> (ID: <snowflake>)`.

### `delete_channel`

Permanently delete a channel.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Guild snowflake. Defaults to the instance's configured guild; used to verify the channel belongs to that guild before deletion. |
| `channel_id` | string | yes | Channel snowflake. |

**Example:**
```json
{
  "name": "delete_channel",
  "arguments": {
    "channel_id": "1234567890123456789"
  }
}
```

**Returns:** `Channel deleted`.

### `edit_channel`

Update channel metadata. Any omitted field is left unchanged.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `channel_id` | string | yes | Channel snowflake. |
| `name` | string | no | New name. |
| `topic` | string | no | New topic. |
| `nsfw` | boolean | no | NSFW flag. |
| `slowmode_seconds` | integer | no | Slowmode rate limit per user (in seconds). |
| `category_id` | string | no | New parent category snowflake. |

**Example:**
```json
{
  "name": "edit_channel",
  "arguments": {
    "channel_id": "1234567890123456789",
    "topic": "Updated topic",
    "slowmode_seconds": 10
  }
}
```

**Returns:** `Channel updated`.

### `move_channel`

Move a channel to a new position (and optionally a new parent category).

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `channel_id` | string | yes | Channel snowflake. |
| `position` | integer | yes | New position within its category/guild. |
| `category_id` | string | no | New parent category. |

**Example:**
```json
{
  "name": "move_channel",
  "arguments": {
    "channel_id": "1234567890123456789",
    "position": 3
  }
}
```

**Returns:** `Channel moved to position N`.

### `create_voice_channel`

Voice-specialised companion to `create_channel`. Creates a Discord voice channel with optional bitrate and user_limit.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `name` | string | yes | Channel name. |
| `category_id` | string | no | Snowflake of the parent category to nest under. |
| `bitrate` | integer | no | Voice bitrate in bps. Default 64000; range 8000-96000 by default, up to 128000/256000/384000 on tier-1/2/3 boosted guilds. |
| `user_limit` | integer (0-99) | no | Max simultaneous users. 0 = unlimited (default). |

**Example:**
```json
{
  "name": "create_voice_channel",
  "arguments": {
    "name": "Standup Room",
    "user_limit": 10,
    "bitrate": 96000
  }
}
```

**Returns:** `Created voice channel '<name>' (ID: <snowflake>)`.

### `create_stage_channel`

Create a Discord stage channel — a voice channel with explicit speaker/audience separation, used for presentations and AMAs.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `name` | string | yes | Channel name. |
| `category_id` | string | no | Snowflake of the parent category to nest under. |

**Example:**
```json
{
  "name": "create_stage_channel",
  "arguments": {
    "name": "Q&A Session"
  }
}
```

**Returns:** `Created stage channel '<name>' (ID: <snowflake>)`.

### `edit_voice_channel`

Voice-specialised companion to `edit_channel`. Edit the bitrate, user_limit, RTC region, name, or parent category of a voice channel.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `channel_id` | string | yes | Voice channel snowflake. |
| `name` | string | no | New channel name. |
| `bitrate` | integer | no | New bitrate in bps. |
| `user_limit` | integer (0-99) | no | New max simultaneous users (0 = unlimited). |
| `category_id` | string | no | Snowflake of a category to move the channel under. |
| `rtc_region` | string | no | RTC region override (e.g. `us-west`, `europe`). Pass empty string to clear and let Discord auto-pick. |

At least one of name/bitrate/user_limit/category_id/rtc_region must be supplied.

**Example:**
```json
{
  "name": "edit_voice_channel",
  "arguments": {
    "channel_id": "1234567890123456789",
    "bitrate": 128000,
    "user_limit": 25
  }
}
```

**Returns:** `Voice channel '<name>' updated`.

### `set_channel_permissions`

Apply a permission overwrite (for a role or a member) on a single channel.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `channel_id` | string | yes | Channel snowflake. |
| `target_type` | string | yes | `role` or `member`. |
| `target_id` | string | yes | Role or user snowflake. |
| `allow` | string | no | Decimal permission bits to grant. Defaults to `0`. |
| `deny` | string | no | Decimal permission bits to deny. Defaults to `0`. |

Common bit values from the schema description: `VIEW_CHANNEL=1024`, `SEND_MESSAGES=2048`, `MANAGE_CHANNELS=16`, `MANAGE_MESSAGES=8192`, `CONNECT=1048576`, `SPEAK=2097152`.

**Example:**
```json
{
  "name": "set_channel_permissions",
  "arguments": {
    "channel_id": "1234567890123456789",
    "target_type": "role",
    "target_id": "9876543210987654321",
    "deny": "2048"
  }
}
```

**Returns:** `Permissions set`.

## Roles

### `list_roles`

List every role in the guild with name, ID, hex color, position, raw permission bits, and hoist flag.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |

**Example:**
```json
{
  "name": "list_roles",
  "arguments": {}
}
```

**Returns:** Lines like `@Moderator | ID: <snowflake> | color: #5865F2 | pos: 5 | perms: 1071698660929 | hoist: true`.

### `create_role`

Create a new role.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `name` | string | yes | Role name. |
| `color` | integer | no | RGB color as a 24-bit integer (e.g. `5793266` for `#5865F2`). |
| `permissions` | string | no | Decimal permission bitfield. |
| `hoist` | boolean | no | Display the role separately in the member list. |
| `mentionable` | boolean | no | Allow `@<role>` mentions. |

**Example:**
```json
{
  "name": "create_role",
  "arguments": {
    "name": "Trusted",
    "color": 5793266,
    "hoist": true,
    "mentionable": true
  }
}
```

**Returns:** `Created @<name> (ID: <snowflake>)`.

### `delete_role`

Permanently delete a role.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `role_id` | string | yes | Role snowflake. |

**Example:**
```json
{
  "name": "delete_role",
  "arguments": {
    "role_id": "9876543210987654321"
  }
}
```

**Returns:** `Role deleted`.

### `edit_role`

Update an existing role. Any omitted field is left unchanged.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `role_id` | string | yes | Role snowflake. |
| `name` | string | no | New name. |
| `color` | integer | no | New RGB color. |
| `permissions` | string | no | New decimal permission bitfield. |
| `hoist` | boolean | no | Hoist flag. |
| `mentionable` | boolean | no | Mentionable flag. |

**Example:**
```json
{
  "name": "edit_role",
  "arguments": {
    "role_id": "9876543210987654321",
    "name": "Trusted Member",
    "mentionable": false
  }
}
```

**Returns:** `Role updated`.

## Members

### `list_members`

List members in the guild. Paginated — each call fetches up to 1000 members, and the `after` parameter takes the last user ID from the previous page.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `limit` | integer (1–1000) | no | Max members to return. Defaults to 100. |
| `after` | string | no | User snowflake to paginate after. |

**Example:**
```json
{
  "name": "list_members",
  "arguments": {
    "limit": 200
  }
}
```

**Returns:** Lines like `<display_name> (ID: <snowflake>) | roles: [<role_id>, ...]`, prefixed with the total count.

### `get_member`

Get detailed information about a single member: username, display name, roles, join date, and bot flag.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `user_id` | string | yes | Member's user snowflake. |

**Example:**
```json
{
  "name": "get_member",
  "arguments": {
    "user_id": "123456789012345678"
  }
}
```

**Returns:** Multi-line block with `User`, `Display`, `Roles`, `Joined`, and `Bot` fields.

### `assign_role`

Add a role to a member.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `user_id` | string | yes | Target user snowflake. |
| `role_id` | string | yes | Role snowflake. |

**Example:**
```json
{
  "name": "assign_role",
  "arguments": {
    "user_id": "123456789012345678",
    "role_id": "9876543210987654321"
  }
}
```

**Returns:** `Role assigned`.

### `remove_role`

Remove a role from a member.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `user_id` | string | yes | Target user snowflake. |
| `role_id` | string | yes | Role snowflake. |

**Example:**
```json
{
  "name": "remove_role",
  "arguments": {
    "user_id": "123456789012345678",
    "role_id": "9876543210987654321"
  }
}
```

**Returns:** `Role removed`.

### `ban_member`

Ban a user from the server.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `user_id` | string | yes | Target user snowflake. |
| `reason` | string | no | Audit-log reason. |
| `delete_message_days` | integer (0–7) | no | How many days of recent messages to delete. Defaults to 0; clamped server-side. |

**Example:**
```json
{
  "name": "ban_member",
  "arguments": {
    "user_id": "123456789012345678",
    "reason": "spam",
    "delete_message_days": 1
  }
}
```

**Returns:** `User banned`.

### `unban_member`

Lift an existing ban.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `user_id` | string | yes | Target user snowflake. |

**Example:**
```json
{
  "name": "unban_member",
  "arguments": {
    "user_id": "123456789012345678"
  }
}
```

**Returns:** `User unbanned`.

### `kick_member`

Kick a member from the server (does not ban them).

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `user_id` | string | yes | Target user snowflake. |
| `reason` | string | no | Audit-log reason. |

**Example:**
```json
{
  "name": "kick_member",
  "arguments": {
    "user_id": "123456789012345678",
    "reason": "inactivity"
  }
}
```

**Returns:** `User kicked`.

### `timeout_member`

Apply a Discord timeout (communication disable) to a member for a given duration.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `user_id` | string | yes | Target user snowflake. |
| `duration` | string | yes | Duration like `30s`, `30m`, `1h`, `7d`. Bare numbers are interpreted as minutes. |
| `reason` | string | no | Audit-log reason. Currently accepted by the schema but not threaded to Discord by the underlying call. |

**Example:**
```json
{
  "name": "timeout_member",
  "arguments": {
    "user_id": "123456789012345678",
    "duration": "1h"
  }
}
```

**Returns:** `User timed out for <duration>`.

### `remove_timeout`

Lift an active timeout on a member. Inverse of `timeout_member`.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `user_id` | string | yes | Target user snowflake. |

**Example:**
```json
{
  "name": "remove_timeout",
  "arguments": {
    "user_id": "123456789012345678"
  }
}
```

**Returns:** `Timeout removed`.

### `set_nickname`

Set or clear a member's nickname (1–32 characters). Pass an empty `nickname` (or omit it) to clear, which makes the member display their global Discord username.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `user_id` | string | yes | Target user snowflake. |
| `nickname` | string | no | New nickname; omit or pass empty string to clear. |

**Example:**
```json
{
  "name": "set_nickname",
  "arguments": {
    "user_id": "123456789012345678",
    "nickname": "Big Boss"
  }
}
```

**Returns:** `Nickname set to '<n>'` or `Nickname cleared`.

### `get_bans`

List active bans in the server, with each user's id/name and the moderator-supplied reason if recorded. Paginate by passing the last user_id from the previous response as `after`.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `limit` | integer (1-255) | no | Max bans per page. Default 100. |
| `after` | string | no | Paginate forward — return bans whose user_id is greater than this snowflake. |

**Example:**
```json
{
  "name": "get_bans",
  "arguments": {
    "limit": 50
  }
}
```

**Returns:** `<count> ban(s):` followed by one line per ban as `<username> (<user_id>) — <reason>`.

### `move_voice_member`

Move a member to a different voice channel. The member must currently be in a voice channel in this guild — Discord rejects with 400 if they aren't connected to voice.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `user_id` | string | yes | Target user snowflake. |
| `channel_id` | string | yes | Voice channel to drop them into. |

**Example:**
```json
{
  "name": "move_voice_member",
  "arguments": {
    "user_id": "123456789012345678",
    "channel_id": "1234567890123456790"
  }
}
```

**Returns:** `Moved user to voice channel <id>`.

### `disconnect_voice_member`

Disconnect a member from voice. The member must currently be in a voice channel in this guild for the disconnect to take effect.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `user_id` | string | yes | Target user snowflake. |

**Example:**
```json
{
  "name": "disconnect_voice_member",
  "arguments": {
    "user_id": "123456789012345678"
  }
}
```

**Returns:** `User disconnected from voice`.

### `modify_voice_state`

Server-mute or server-deafen a member when they're in voice. Pass `mute` and/or `deafen` explicitly; omitted fields are left unchanged. At least one of the two must be provided.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `user_id` | string | yes | Target user snowflake. |
| `mute` | boolean | no | Server-mute (true) or unmute (false) when in voice. Omit to leave unchanged. |
| `deafen` | boolean | no | Server-deafen (true) or undeafen (false) when in voice. Omit to leave unchanged. |

**Example:**
```json
{
  "name": "modify_voice_state",
  "arguments": {
    "user_id": "123456789012345678",
    "mute": true
  }
}
```

**Returns:** `Voice state updated: <fields>`.

## Direct Messages

DM tools open (or reuse) a private channel between the bot and the target user, then operate on that channel like any other text channel. There's no `guild_id` parameter and no cross-guild verification — DMs aren't part of any guild. The underlying `create_private_channel` call is idempotent, so repeated calls don't proliferate channels.

**Permissions / setup notes:**
- The bot doesn't need a special Discord permission to send DMs, but the *target user* must allow DMs from server members and must share a guild with the bot. If they don't, the send returns a 403 Forbidden which surfaces here as `Discord API error`.
- Reading DM history via `read_private_messages` uses the REST API (not the gateway), so the `DIRECT_MESSAGES` privileged intent is **not** required for these tools to work.
- `edit_private_message` and `delete_private_message` only work on messages the bot itself sent — Discord won't let any bot edit or delete another user's DMs.

### `send_private_message`

Send a direct message to a user. Opens the DM channel automatically. **Privileged.**

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `user_id` | string | yes | Target user snowflake. The bot DMs them; Discord rejects with 403 if they have DMs disabled or don't share a guild. |
| `content` | string | yes | Message body. |

**Example:**
```json
{
  "name": "send_private_message",
  "arguments": {
    "user_id": "123456789012345678",
    "content": "Following up on your moderation question — let me know if this resolves it."
  }
}
```

**Returns:** `Message sent`.

### `read_private_messages`

Read recent DMs between the bot and a user, newest first. Same output format as `get_recent_messages` but scoped to the DM channel.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `user_id` | string | yes | Target user snowflake. |
| `limit` | integer (1-100) | no | Number of messages to fetch. Default 50. |
| `before` | string | no | Message snowflake; only messages older than this are returned. |

**Example:**
```json
{
  "name": "read_private_messages",
  "arguments": {
    "user_id": "123456789012345678",
    "limit": 20
  }
}
```

**Returns:** Newline-separated lines, one per message, formatted as `[timestamp] author_name (author_id) [msg_id=...]: content` plus optional attachment/embed markers. `No messages found.` if the channel is empty.

### `edit_private_message`

Edit one of the bot's previously-sent DMs to a user.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `user_id` | string | yes | The DM partner (same as in the original send call). |
| `message_id` | string | yes | Snowflake of the message to edit. |
| `content` | string | yes | Replacement content. |

**Example:**
```json
{
  "name": "edit_private_message",
  "arguments": {
    "user_id": "123456789012345678",
    "message_id": "1234567890123456790",
    "content": "Updated: this answer was wrong; the correct procedure is …"
  }
}
```

**Returns:** `Message edited`.

### `delete_private_message`

Delete one of the bot's previously-sent DMs to a user.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `user_id` | string | yes | The DM partner. |
| `message_id` | string | yes | Snowflake of the message to delete. |

**Example:**
```json
{
  "name": "delete_private_message",
  "arguments": {
    "user_id": "123456789012345678",
    "message_id": "1234567890123456790"
  }
}
```

**Returns:** `Message deleted`.

## Webhooks

Webhooks let an MCP client post as arbitrary identities (custom username + avatar per message) — the standard pattern for relays, persona bots, and cross-platform bridges. The bot uses its `Manage Webhooks` permission to create / delete / list webhooks; sending through one only needs the webhook id and token.

### `list_webhooks`

List webhooks attached to a channel. Each entry includes id, name, and (when the bot has Manage Webhooks) the token, which `send_webhook_message` requires.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild; used to verify the channel. |
| `channel_id` | string | yes | Target channel snowflake. |

**Example:**
```json
{
  "name": "list_webhooks",
  "arguments": {
    "channel_id": "1234567890123456789"
  }
}
```

**Returns:** `<count> webhook(s):` followed by one line per webhook as `<name> (id=<id>) — token=<token>`. `No webhooks in this channel.` if empty.

### `create_webhook`

Create a new webhook on a channel. Returns the webhook's id and token; capture both — the token is required for `send_webhook_message` and is only returned at create time.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `guild_id` | string | no | Server ID. Defaults to the configured guild. |
| `channel_id` | string | yes | Target channel snowflake. |
| `name` | string | yes | Webhook display name (1-80 chars). Discord rejects names containing the substring "discord" (case-insensitive). |

**Example:**
```json
{
  "name": "create_webhook",
  "arguments": {
    "channel_id": "1234567890123456789",
    "name": "Daily Standup Bot"
  }
}
```

**Returns:** `Webhook created: id=<id> token=<token>`.

### `delete_webhook`

Delete a webhook by ID.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `webhook_id` | string | yes | Webhook snowflake. |

**Example:**
```json
{
  "name": "delete_webhook",
  "arguments": {
    "webhook_id": "1234567890123456789"
  }
}
```

**Returns:** `Webhook deleted`.

### `send_webhook_message`

Send a message through a webhook. The optional `username` and `avatar_url` parameters override the webhook's defaults for *this message only* — useful for relay/persona patterns where one webhook delivers messages on behalf of many identities. **Privileged** — webhooks bypass the bot's role permissions.

**Parameters:**

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `webhook_id` | string | yes | Webhook snowflake. |
| `token` | string | yes | Webhook token (returned by `create_webhook` / `list_webhooks`). |
| `content` | string | yes | Message body. |
| `username` | string | no | Override the webhook's display name for this message. |
| `avatar_url` | string | no | Override the webhook's avatar (URL) for this message. |

**Example:**
```json
{
  "name": "send_webhook_message",
  "arguments": {
    "webhook_id": "1234567890123456789",
    "token": "abcdef123456...",
    "content": "[#general → Slack] Daisy: deploy is green",
    "username": "Daisy (via Slack)",
    "avatar_url": "https://example.com/daisy.png"
  }
}
```

**Returns:** `Webhook message sent`.
