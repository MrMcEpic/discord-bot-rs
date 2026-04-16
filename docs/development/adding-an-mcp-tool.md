# Adding an MCP Tool

This page walks through adding a new tool to the embedded MCP server,
the one bound on `MCP_PORT` (default `9090`) inside the bot process.
By the end you'll have an end-to-end example: a parameter struct, a
`#[tool]`-annotated handler, and a working tool call from an MCP
client like Claude Code or the bundled gateway.

If you're confused about what MCP is or where it sits in the
architecture, read the [MCP Server feature page](../features/mcp-server.md)
first. This page assumes you know that the bot exposes a JSON-RPC
endpoint, that tools live on the `DiscordTools` impl in
`src/mcp/tools.rs`, and that the
[`rmcp`](https://github.com/modelcontextprotocol/rust-sdk) crate's
`#[tool]` and `#[tool_router]` macros do the heavy lifting.

## What "adding a tool" means

Three pieces, in this order:

1. A **parameter struct** that derives `Deserialize` and `JsonSchema`
   so `rmcp` can produce a JSON Schema for clients.
2. An **async method on `impl DiscordTools`** annotated with
   `#[tool(description = "...")]`.
3. **Nothing else.** Registration is automatic. The
   `#[tool_router(router = tool_router)]` macro on the impl block
   discovers every method tagged with `#[tool]` and wires them into
   the `ToolRouter`. There is no list to keep in sync. There is no
   second file to edit.

This is the single most important thing to internalise: write the
function, the macro registers it. If your new tool doesn't show up
after a rebuild, it's because the macro didn't see it — and the cause
is almost always a missing `#[tool(...)]` attribute or putting the
function inside a separate `impl` block from the one tagged with
`#[tool_router]`.

## Open the file

Every tool in the codebase lives in
[`src/mcp/tools.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/mcp/tools.rs).
Read the top of the file to see how it's organised:

- The `use` block imports `rmcp` types and the `Parameters` wrapper.
- A run of parameter structs at the top — `CreateChannelParams`,
  `EditRoleParams`, `BanParams`, etc. Each one is its own
  `#[derive(Debug, Deserialize, JsonSchema)] pub struct`.
- A few small helpers: `parse_id`, `channel_type_num`,
  `parse_duration_secs`, the `discord_call!` macro that wraps every
  Serenity HTTP call with a 10-second timeout.
- The `ServerHandler` impl that wires `list_tools` and `call_tool`
  through the router.
- The big `#[tool_router(router = tool_router)] impl DiscordTools`
  block — every tool lives in this one block, with section dividers
  like `// ===== CHANNELS =====`.

Add your new tool to the bottom of an appropriate section, or create
a new section if you're starting a new area.

## Step 1: Write the parameter struct

Worked example: a `lookup_user_by_name` tool that takes a username
substring and returns matching members. Define the params:

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct LookupUserByNameParams {
    /// Guild/server ID (optional, defaults to configured guild)
    pub guild_id: Option<String>,
    /// Substring to match against display name or username (case-insensitive)
    pub query: String,
    /// Max results (1-50, default 10)
    pub limit: Option<u8>,
}
```

A few things to copy from the existing tools:

- **`Deserialize` and `JsonSchema`** are both required. The first
  parses incoming JSON, the second produces the schema MCP clients
  see in `tools/list`.
- **Doc comments on every field** become the schema's per-property
  descriptions. A client like Claude reads them. Spend the extra
  thirty seconds on each one.
- **`guild_id` is always optional and always at the top** when the
  tool can be run against a guild. The `resolve_guild()` helper on
  `DiscordTools` falls back to the configured guild when it's `None`,
  which keeps the per-instance MCP setup ergonomic.
- **IDs are `String`, not `u64`.** Discord snowflakes are 64-bit, but
  JSON numbers aren't reliably 64-bit on the client side, so the wire
  format is always strings. Parse to `u64` inside the handler with
  the `parse_id` helper.
- **Use `Option<T>` for any field that has a default**, then apply
  the default inside the handler with `.unwrap_or(...)`. Don't fight
  `serde` with `#[serde(default = "...")]` unless the default is
  expensive enough to matter.

## Step 2: Write the handler

Add the method to the `impl DiscordTools` block tagged with
`#[tool_router]`. The body is your code; everything around it is
boilerplate copied from a neighbour:

```rust
#[tool(description = "Find members whose display name or username contains the substring (case-insensitive)")]
async fn lookup_user_by_name(
    &self,
    params: Parameters<LookupUserByNameParams>,
) -> Result<CallToolResult, McpError> {
    let p = params.0;
    let gid = self.resolve_guild(p.guild_id.as_deref())?;
    let limit = p.limit.unwrap_or(10).clamp(1, 50);
    let query = p.query.to_lowercase();

    let members = discord_call!(self.http.get_guild_members(gid, Some(1000), None));

    let matches: Vec<String> = members
        .iter()
        .filter(|m| {
            m.display_name().to_lowercase().contains(&query)
                || m.user.name.to_lowercase().contains(&query)
        })
        .take(limit as usize)
        .map(|m| {
            format!(
                "{} (@{}) | ID: {}",
                m.display_name(),
                m.user.name,
                m.user.id
            )
        })
        .collect();

    let body = if matches.is_empty() {
        format!("No members matching '{}'.", p.query)
    } else {
        format!("{} match(es):\n{}", matches.len(), matches.join("\n"))
    };

    Ok(CallToolResult::success(vec![Content::text(body)]))
}
```

Notes on the patterns this copies:

- **`#[tool(description = "...")]`** is what makes the macro pick up
  the function. The description is what the MCP client sees in the
  tool list — write it for an LLM, not a person. State what the tool
  does and what kind of input it wants. If the tool is privileged
  (sends messages, deletes data, mutates state) say so in the
  description, the same way `send_message` does ("PRIVILEGED —
  recommend manual approval"). MCP clients generally surface that
  text in their approval UIs.
- **`params: Parameters<YourParams>`** is the universal signature for
  tools that take input. Pull `p = params.0` at the top.
- **`self.resolve_guild(...)`** is the helper for the optional
  guild-id pattern. Prefer it over reading `params.guild_id`
  directly — it gives you the configured guild as a fallback and
  keeps the error type consistent.
- **`discord_call!(...)`** wraps every Serenity HTTP call in a
  10-second timeout and converts errors into `McpError` via
  `api_err`. Don't call `self.http.<method>().await?` directly; you
  lose the timeout guard and you have to write the error mapping
  by hand.
- **The return body is plain text** in `Content::text(...)`. MCP
  supports structured content but this codebase keeps everything as
  human-readable text — the consumer is usually an LLM that handles
  prose just fine. If your tool returns a list, format it line by
  line; if it returns a count plus a list, prepend the count. Match
  the existing tools' shape.
- **Validation errors** use `McpError::invalid_params(...)`. Internal
  failures use `McpError::internal_error(...)`. The helpers
  `api_err` and `timeout_err` cover the common cases.

That's the whole tool. Build:

```bash
CONFIG_DIR=instances/local cargo run
```

Then issue a `tools/list` against the MCP server (or restart your
Claude Code session and ask it to list discord-bot tools). Your tool
will be there, with the description and the JSON Schema you wrote.

## Step 3: Test it from a client

The simplest end-to-end check is an HTTP call with `curl`:

```bash
curl -s -X POST http://127.0.0.1:9090/mcp \
  -H 'content-type: application/json' \
  -H 'authorization: Bearer YOUR_MCP_TOKEN' \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tools/call",
    "params": {
      "name": "lookup_user_by_name",
      "arguments": { "query": "alice", "limit": 5 }
    }
  }'
```

(Drop the `authorization` header if you didn't set `MCP_AUTH_TOKEN`.)

A working response wraps the text body in MCP's content envelope. A
broken one tells you what's wrong:

- **`Method not found`** — your tool isn't registered. The macro
  didn't see it. Re-check the `#[tool(description = "...")]`
  attribute and that the function lives inside the same impl block
  as the `#[tool_router]` attribute.
- **`Invalid parameters`** — your params struct doesn't match what
  you sent. Either the JSON Schema published in `tools/list`
  disagrees with the call, or a required field is missing. The error
  message names the field.
- **`Discord API error: ...`** — the underlying Serenity call
  failed. Most often a permission issue (the bot isn't allowed to do
  what you asked) or a bad ID.
- **`Discord API request timed out`** — the call exceeded the
  10-second `API_TIMEOUT` constant. This is rare for read calls and
  usually indicates an upstream Discord problem.

## Patterns from the existing tools

Most of what you'd need is already in `tools.rs`. A few patterns worth
knowing about explicitly:

- **Channel-scoped operations** that don't strictly need a `guild_id`
  for the API call still take it as an optional param for clarity.
  See `send_message`, `delete_messages`, `edit_channel`. They pull
  `let _gid = self.resolve_guild(p.guild_id.as_deref())?;` to validate
  the param shape and discard the result.
- **Bulk vs single delete** — `delete_messages` calls `delete_messages`
  on the channel for >1 ID and `delete_message` for exactly 1, because
  the bulk endpoint rejects single-message deletes. Discord API quirk;
  copy the pattern.
- **Permission overrides** — `set_channel_permissions` accepts
  permission bits as decimal strings (because JSON numbers can lose
  precision), parses them with `.parse::<u64>().unwrap_or(0)`, and
  builds a `PermissionOverwrite` struct. Mirror this if you need to
  pass any 64-bit field over the wire.
- **Pagination** — `list_members` uses `limit` plus `after` (the last
  user ID seen). MCP clients are expected to call repeatedly; don't
  try to fetch unbounded data in one tool call.
- **Description tags** for privileged operations — `send_message`
  contains "PRIVILEGED — recommend manual approval" in its
  description. Add the same hint to any tool that mutates Discord
  state in a way you'd want a human to confirm.

## When the tool needs more than a `Parameters<T>`

Most tools take one params struct. If yours takes none — for example,
`list_guilds` — drop the `params` argument entirely:

```rust
#[tool(description = "List all Discord servers (guilds) this bot is connected to")]
async fn list_guilds(&self) -> Result<CallToolResult, McpError> {
    let guilds = discord_call!(self.http.get_guilds(None, None));
    // ...
    Ok(CallToolResult::success(vec![Content::text(/* ... */)]))
}
```

`rmcp` synthesises an empty schema for tools with no params.

If you want to access state that lives on `Data` (the bot's main state
struct, not the MCP server's), you'll have to thread it through. The
current `DiscordTools::new` constructor takes only `Arc<Http>` and a
`GuildId`. To add database access, add a field for the pool, extend
`new`, and update the call site in `src/mcp/mod.rs` where
`DiscordTools::new` is invoked. There's no example of this in the
codebase yet — every existing tool only needs Discord HTTP — but the
shape is straightforward: clone the `Arc<sqlx::PgPool>` into the
struct the same way `http: Arc<Http>` is cloned today.

## Documentation

After your tool ships:

- Add a row to `docs/reference/mcp-tool-catalog.md` describing the
  tool, its parameters, and an example call.
- Update `CHANGELOG.md` under `[Unreleased]` in `Added`.

## Common gotchas

- **Forgot `#[tool(description = "...")]`.** The function compiles
  as a regular method, the macro ignores it, and the tool never
  appears in `tools/list`.
- **Forgot `JsonSchema` on the params struct.** You get a confusing
  trait-bound error from the macro expansion. Add the derive.
- **Used `u64` for an ID.** The wire format will accept it sometimes
  and lose precision other times. Always `String` in the params
  struct, parsed with `parse_id` inside the handler.
- **Skipped `discord_call!`.** Your tool can hang for minutes on a
  slow Discord response, blocking the MCP worker. Always wrap.
- **Returned a 10MB response.** MCP clients don't enjoy this any
  more than humans do. Truncate large lists, paginate, or summarise.
  `list_members` caps at 1000 per call.
- **Two `impl DiscordTools` blocks.** Only one of them is the
  `#[tool_router]` block. Tools in any other `impl` are invisible.

## Next steps

- [MCP Server feature page](../features/mcp-server.md) — what
  capabilities the bot already exposes and how external clients
  connect.
- [MCP Gateway Routing](../architecture/mcp-gateway-routing.md) —
  how tool calls find the right instance in a multi-bot deployment.
- [Adding a Feature Module](adding-a-feature-module.md) — when your
  MCP tool is part of a larger new area of the bot.
