# MCP Server

The bot embeds a Model Context Protocol (MCP) server that exposes Discord
server-management as **22 tools** any MCP client can call. The intended
workflow is to point an AI coding assistant — Claude Code, Cursor, etc. —
at the bot and let it manage your Discord server programmatically.

## What it does

When the bot starts, it spins up an HTTP server on a configurable port
(default `9090`) and registers the tool catalog with the standard MCP
streamable-HTTP transport. An MCP client connects, lists the available
tools, and can then invoke them. Every tool runs against the same
Discord HTTP client the bot itself uses, so the actions execute as the
bot user with the bot's permissions.

The point of this is that you can talk to your Discord server in
natural language from inside an AI assistant:

> "Find the channel category called Archive and move all read-only
> channels into it."
>
> "List every member with the @Verified role who hasn't sent a
> message in 30 days."
>
> "Create a new role 'Beta Testers', colour green, no permissions, and
> assign it to these five users."

Instead of writing Discord API code or clicking through the Discord UI,
your AI assistant calls the matching MCP tools.

## Why this exists

Discord administration is full of repetitive multi-step operations:
auditing roles, cleaning up old channels, mass-renaming, bulk
permission changes. The Discord client doesn't have a scripting
interface, and writing one-off API scripts for every cleanup task is
tedious. An LLM with the right tools can plan the operation, ask for
confirmation, and execute it in a couple of turns.

The bot is also a natural place to put this server: it is *already* a
long-running process holding the Discord HTTP client, with the right
intents and a privileged token. Adding an MCP endpoint costs almost
nothing in startup time and gives you a remote-control interface for
free.

## The protocol

[Model Context Protocol](https://modelcontextprotocol.io) is an open
JSON-RPC-based protocol from Anthropic that lets clients (LLMs and IDE
extensions) discover and call tools exposed by servers. The bot uses
the **Streamable HTTP** transport, which is a simple HTTP-and-SSE
flavour of MCP (no stdio handshakes, no daemon-spawning).

The server is built on the [`rmcp`](https://crates.io/crates/rmcp)
Rust SDK. Tool definitions are written as decorated `async fn`s on a
single `DiscordTools` struct in `src/mcp/tools.rs`; the
`#[tool(description = "...")]` attribute generates the JSON schema
from each function's parameter type.

## Connecting Claude Code

The simplest way to test the server is to add it to your local Claude
Code config (`~/.claude.json`):

```json
{
  "mcpServers": {
    "discord": {
      "type": "http",
      "url": "http://localhost:9090/mcp"
    }
  }
}
```

Then restart Claude Code. The next time you start a session, the
`discord` server should appear in your tool list and you can call any
of the 22 tools by name.

If you've set `MCP_AUTH_TOKEN` (see below), add the bearer token to
the same entry:

```json
{
  "mcpServers": {
    "discord": {
      "type": "http",
      "url": "http://localhost:9090/mcp",
      "headers": {
        "Authorization": "Bearer your-token-here"
      }
    }
  }
}
```

Other MCP-capable clients (Cursor, Continue, custom code) follow the
same pattern: point them at `http://<host>:<port>/mcp` over HTTP and
optionally pass a bearer token.

## Tool catalog

The 22 tools are grouped into five categories. The full reference,
including parameter schemas, lives in
[Reference: MCP Tool Catalog](../reference/mcp-tool-catalog.md). The
table below is the one-line summary.

### Guilds

| Tool | What it does |
|---|---|
| `list_guilds` | List every Discord server the bot is a member of, with names and IDs. |

### Server

| Tool | What it does |
|---|---|
| `get_guild_info` | Name, owner, approximate member count, channel/role counts. |
| `send_message` | Post a message to a channel. **Privileged.** |
| `delete_messages` | Bulk-delete the most recent N messages from a channel (1–100). |

### Channels

| Tool | What it does |
|---|---|
| `list_channels` | All channels in the server with IDs, types, and positions. |
| `create_channel` | Create a text, voice, category, forum, or stage channel. |
| `delete_channel` | Delete a channel. |
| `edit_channel` | Edit name, topic, NSFW flag, slowmode, parent category. |
| `move_channel` | Move a channel to a new position or category. |
| `set_channel_permissions` | Apply permission overrides for a role or member. |

### Roles

| Tool | What it does |
|---|---|
| `list_roles` | All roles with IDs, colours, positions, permissions. |
| `create_role` | Create a new role. |
| `delete_role` | Delete a role. |
| `edit_role` | Edit name, colour, permissions, hoist, mentionable. |

### Members

| Tool | What it does |
|---|---|
| `list_members` | List server members (max 1000 per call, paginate with `after`). |
| `get_member` | Detailed info about one member. |
| `assign_role` | Add a role to a member. |
| `remove_role` | Remove a role from a member. |
| `ban_member` | Ban a user, optionally with a reason and message-history purge. |
| `unban_member` | Unban a user. |
| `kick_member` | Kick a member. |
| `timeout_member` | Time out a member for a duration like `1h`, `30m`, `7d`. |

That's 22 tools total: 1 + 3 + 6 + 4 + 8.

## Multi-guild support

Almost every tool takes an optional `guild_id` parameter. When it's
omitted, the tool acts against the bot's *configured* guild — the one
named in the `GUILD_ID` env var. When it's supplied, the tool acts
against that guild instead, *as long as the bot is a member of it*.

This means you can run a single bot across several Discord servers and
manage them all through one MCP endpoint. The `list_guilds` tool is
deliberately the one that does not take a `guild_id` parameter — use
it to discover what's available, then pass the right ID into the
follow-up calls.

## Port and binding

Two environment variables control the listen address (loaded in
`src/config.rs`):

| Variable | Default | Notes |
|---|---|---|
| `MCP_PORT` | `9090` | TCP port the server listens on. |
| `MCP_BIND_ADDR` | `127.0.0.1` | Interface to bind to. The default is **localhost-only**, deliberately. |

The defaults are chosen so that a fresh install is not exposing
anything to the public internet. To make the server reachable from
other machines on a private network, set `MCP_BIND_ADDR=0.0.0.0` and
arrange your own network ACLs. To expose it over the public internet
*at all*, see the security section below first.

## Authentication

A third environment variable controls authentication:

| Variable | Default | Notes |
|---|---|---|
| `MCP_AUTH_TOKEN` | empty (none) | Bearer token. **Required** unless `MCP_BIND_ADDR` is a loopback interface (`127.0.0.1`, `::1`). |

The middleware in `src/mcp/mod.rs` is a single `from_fn` layer:

- If `MCP_AUTH_TOKEN` is empty, every request is allowed through.
- If it is set, every request must carry a matching
  `Authorization: Bearer <token>` header, otherwise it is rejected
  with `401 Unauthorized`.
- The bearer-token comparison is **constant-time** (via
  `subtle::ConstantTimeEq`) so a network attacker can't use response
  timing to probe the token byte-by-byte.

### Strict startup guard

The bot **refuses to start** if `MCP_AUTH_TOKEN` is empty *and*
`MCP_BIND_ADDR` is not a loopback address. This replaces the older
"soft warning" behaviour — the misconfiguration that used to expose
an unauthenticated MCP endpoint on the public internet now fails the
boot instead, with a pointed error explaining how to fix it. Either
set a token or bind to loopback; there is no third option.

The mcp-gateway applies the same rule even more strictly: because it
always listens on `0.0.0.0`, it refuses to start without
`MCP_AUTH_TOKEN` set, no loopback escape hatch available. The
gateway also forwards that same token on every outgoing request to
its backends — one shared secret covers both the inbound check and
the outbound forward — so operators configure a single value and
the bundled docker-compose deploy (where backends bind `0.0.0.0:9090`
and therefore require a token themselves) works without extra
plumbing.

### Request size limit

Every incoming request body is capped at **64 KiB**. Oversize bodies
are rejected with `413 Payload Too Large`. Bodies that fit but aren't
valid JSON-RPC come back with a standards-conformant
`{"jsonrpc":"2.0","error":{"code":-32700,"message":"Parse error"},"id":null}`
response rather than a generic 400.

## The MCP gateway

If you run multiple bot instances out of the same host (production
and staging, or two community bots, etc.), each one binds its own port
and exposes its own catalog. Pointing Claude Code at all of them
individually means maintaining a list of URLs and switching between
them.

The repository ships a separate `mcp-gateway` service that solves
this. It listens on a single port, multiplexes requests to whichever
bot's MCP endpoint they belong to, and presents a unified tool list
to clients (with each tool prefixed by the instance name from the
`INSTANCES` env var, so `bot_a__list_channels` vs `bot_b__list_channels`).

The gateway is the recommended entry point in production — see
[Architecture: MCP Gateway Routing](../architecture/mcp-gateway-routing.md)
for how it routes and authenticates requests.

## Security considerations

The MCP tools are powerful. `ban_member` permanently removes a user;
`delete_channel` is destructive and unrecoverable; `send_message` lets
the bot post anywhere it has Send Messages permission. There is no
in-bot confirmation gate on MCP calls — a misfiring AI agent or a
compromised client could do real damage in seconds.

This means the threat model is **the MCP endpoint itself is privileged
infrastructure**. Treat it the same way you'd treat a bare-metal SSH
session into your Discord server.

Concrete recommendations:

- **`MCP_AUTH_TOKEN` is required unless bound to loopback.** The bot
  enforces this at startup: if `MCP_BIND_ADDR` is anything other than
  a loopback address and the token is unset, the process refuses to
  boot. Generate the token with `openssl rand -hex 32` or similar.
- **Default to `127.0.0.1`.** The default `MCP_BIND_ADDR` is localhost
  for a reason. Do not change it unless you're putting something
  meaningful in front of it.
- **Do not put the port on the public internet.** Even with a bearer
  token, you're one token leak away from a takeover. Use a
  WireGuard / Tailscale / SSH tunnel / VPN to reach it remotely.
- **Audit the bot's Discord permissions.** The MCP server can only do
  what the bot itself can do. If you grant the bot Administrator,
  you've granted Administrator to anyone holding the MCP token.
- See
  [Deployment: MCP Exposure](../deployment/mcp-exposure.md) for the
  long-form discussion and example reverse-proxy configurations.

## Known limitations

- **OAuth 2.1 is not implemented.** The MCP spec defines an OAuth-based
  flow that some clients prefer (Claude Code's HTTP transport, for
  instance, expects it). The bot speaks bearer-token auth only, so
  you'll see warnings in those clients about the auth mode being
  unrecognised — they still work, but the OAuth handshake never
  happens.
- **Rate limiting is Discord's, not ours.** The tools call the
  Discord HTTP API directly with no extra throttling. Bulk operations
  on members, roles, or messages are bounded by Discord's rate
  limits, and a too-eager AI agent will trigger 429 responses you'll
  see surfaced as `Discord API error: ...` in the tool output.
- **Per-call API timeout is 10 seconds.** `API_TIMEOUT` in
  `src/mcp/tools.rs`. Long-running Discord calls (e.g. fetching a
  large guild's full member list) may time out; in that case, page
  through with smaller `limit` values.
- **Discord permission errors are surfaced as plain strings**, not as
  structured error codes. If a tool says `Missing Permissions`, the
  bot itself doesn't have the permission needed for the operation.
- **The `timeout_member` tool's `reason` argument is currently
  cosmetic** — it's part of the schema for forward compatibility,
  but the underlying call doesn't yet thread it through to Discord's
  audit log.

## Example use cases

- **Mass channel cleanup.** "List every channel in the
  `#archive-2022` category that hasn't had a message this year and
  delete them." The AI calls `list_channels`, filters mentally, and
  invokes `delete_channel` for each one.
- **Role audits.** "List every member with the `@Donor` role and
  cross-check against the active payment list." Pair `list_members`
  with whatever data source you point the AI at.
- **Permissions sweep.** "Make sure every channel under the
  `#mods-only` category has the `@Moderator` role allowed and
  `@everyone` denied." `set_channel_permissions` per channel.
- **Bulk message cleanup.** "Delete the last 50 messages in `#spam`."
  One call to `delete_messages`.
- **Server bootstrapping.** "Create a category 'Beta', three text
  channels under it, and a role with permissions limited to that
  category." Several `create_channel` and `create_role` calls in
  sequence.

The general pattern is *list-then-act*: ask the AI to discover what's
there, then have it execute the change with a clear plan you can
approve.

## Cross-references

- [Architecture: MCP Gateway Routing](../architecture/mcp-gateway-routing.md)
  — how the gateway multiplexes multiple bot instances.
- [Deployment: MCP Exposure](../deployment/mcp-exposure.md) — safe
  ways to make the server reachable from outside the host.
- [Reference: MCP Tool Catalog](../reference/mcp-tool-catalog.md) —
  the full per-tool schema reference, including every parameter and
  default.
- [Adding an MCP Tool](../development/adding-an-mcp-tool.md) — how to
  extend the catalog with new tools.
- [Environment Variables](../configuration/environment-variables.md) —
  reference for `MCP_PORT`, `MCP_BIND_ADDR`, and `MCP_AUTH_TOKEN`.
