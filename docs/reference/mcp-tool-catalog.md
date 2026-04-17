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
