# Glossary

Definitions for terms that show up across the codebase and the rest
of the documentation. Skim this if you've seen a word twice and aren't
sure what it means here specifically.

### AGPL

Short for **GNU Affero General Public License, version 3 or later**,
the licence this project ships under. AGPL is GPL with one extra
clause: software interacted with over a network is treated the same
as distributed software for the purpose of source-availability
obligations. A Discord bot inherently interacts with users over a
network, so anyone running a modified version is required to make
their source available to its users on request. See the
[FAQ](faq.md#whats-the-license-really) for the practical shape.

### DeepSeek

A Chinese AI provider whose `deepseek-chat` model is the bot's
primary backend for `@mention` AI chat when `DEEPSEEK_API_KEY` is set.
Cheap, fast, supports OpenAI-style tool calls. See the
[AI Pipeline](../architecture/ai-pipeline.md) page.

### Gemini

Google's hosted LLM service. Used as the fallback path when DeepSeek
returns an error and `GEMINI_API_KEY` is set, or as the only
provider when DeepSeek isn't configured.

### Gateway (Discord)

The persistent WebSocket connection the bot maintains with Discord's
servers. Events (messages, member joins, voice updates) arrive over
the gateway; the bot acknowledges them and replies via Discord's
HTTP API. Maintained by [serenity](#serenity-poise-songbird).

### Gateway (MCP)

A separate small Rust crate, [`mcp-gateway/`](https://github.com/MrMcEpic/discord-bot-rs/tree/master/mcp-gateway),
that sits in front of one or more bot instances and exposes a single
external MCP endpoint. It routes incoming tool calls to the right
instance based on a configured guild-to-instance map or an explicit
`instance` parameter. See
[MCP Gateway Routing](../architecture/mcp-gateway-routing.md).

### `ghcr.io`

GitHub Container Registry, where the project publishes Docker images
when a release is tagged. The image reference looks like
`ghcr.io/mrmcepic/discord-bot-rs:latest`. The `docker-compose.yml`
in the repo can either build locally or pull from `ghcr.io`.

### Hard tabs

The project's indentation choice, enforced by `rustfmt.toml`. Every
indent level is one tab character, displayed as four columns. Don't
expand tabs to spaces; `cargo fmt` will undo it.

### Instance

One configured Discord bot identity. An instance is everything
contained in an `instances/<name>/` directory: `config.toml`, `.env`
(with the Discord token, client ID, guild ID, database schema, and
optional API keys), `personality.txt`, and any prompt files. Each
running bot process serves exactly one instance. Two instances on
the same machine are two separate processes sharing the same
Postgres but writing to different schemas. See
[Multi-Instance Model](../architecture/multi-instance-model.md).

### Intents

Discord's gateway permissions system. The bot must declare which
classes of events it wants — `GUILDS`, `GUILD_MESSAGES`,
`MESSAGE_CONTENT`, `GUILD_VOICE_STATES`, `GUILD_MEMBERS` — and Discord
filters anything else out before sending. Two intents are
*privileged* and require an explicit toggle in the developer portal:
**Message Content** (needed to read prefix commands) and **Server
Members** (needed for joins, auto-role, welcome). The full list is
declared in `main.rs`.

### mdBook

The static site generator the documentation is built with. Markdown
under `docs/` plus `book.toml` plus a theme equals the site at
[mrmcepic.github.io/discord-bot-rs](https://mrmcepic.github.io/discord-bot-rs/).
`mdbook serve` gives you a live preview at `localhost:3000`. See
[Building Locally](../development/building-locally.md#building-the-docs).

### Mermaid

A text-based diagramming tool. The architecture pages use Mermaid
graphs (in fenced ` ```mermaid ` blocks) for component diagrams and
sequence diagrams. mdBook renders them client-side via
`mermaid.min.js`.

### OGG/Opus passthrough

The format the music pipeline uses for streaming audio to Discord.
yt-dlp downloads source media; ffmpeg remuxes (without re-encoding)
into an Opus-in-OGG container at 256 kbps; songbird sends the bytes
straight to Discord. "Passthrough" means the bot never decodes and
re-encodes — it just copies the Opus packets, which is fast and
preserves quality. The downside is that source media that isn't
already Opus has to be transcoded by ffmpeg, which the pipeline
handles transparently.

### Personality

The contents of `personality.txt` in an instance directory, used as
the system prompt for the AI chat pipeline. Each instance has its
own; the loader panics if the file is missing or empty. See
[Personality Files](../configuration/personality.md).

### Poise

The command framework on top of [serenity](#serenity-poise-songbird).
Provides typed `Context<'_, Data, BotError>`, a derive macro for
prefix commands, automatic argument parsing for Discord types, and a
subcommand tree. This project uses poise's prefix commands
exclusively.

### Prefix command

A Discord bot command invoked by a leading prefix character (default
`!` here, configurable per-instance via `command_prefix`) plus the
command name. Prefix commands use the `MESSAGE_CONTENT` intent
because the bot reads message text directly. Distinct from slash
commands, which Discord registers as application commands and
auto-completes for users — this project deliberately uses prefix
commands only. See the [FAQ](faq.md#why-prefix-commands-and-no-slash-commands)
for the reasoning.

### Schema-per-instance

The database isolation strategy used in multi-instance deployments.
One PostgreSQL database hosts every instance; each instance has its
own `DB_SCHEMA` (e.g. `bot1`, `bot2`); on every connection the bot
runs `SET search_path TO "<schema>"`. Tables, sequences, and indexes
all live inside the schema, so two instances pointed at the same
database but different schemas can't see each other's data. See
[Multi-Instance Model](../architecture/multi-instance-model.md) and
`init_pool` in `src/db/mod.rs`.

### Serenity, Poise, Songbird

The core Rust Discord stack the project is built on.
**[Serenity](https://github.com/serenity-rs/serenity)** owns the
gateway connection, the typed Discord model, and the HTTP client.
**[Poise](https://github.com/serenity-rs/poise)** is a command
framework on top of serenity.
**[Songbird](https://github.com/serenity-rs/songbird)** is the voice
driver — voice gateway, UDP transport, Opus packetisation. All three
are maintained by the serenity-rs organisation.

### Snowflake

Discord's 64-bit ID format. Every guild, channel, user, role, and
message has one. They're 64-bit integers but exposed in JSON as
strings (because JavaScript can't safely represent 64-bit integers
as numbers). The bot follows the same convention internally —
parameter structs use `String` for IDs and parse to `u64` on the way
in via the `parse_id` helper.

### sqlx

The async PostgreSQL client the bot uses for every database call.
The project uses sqlx's runtime helpers (`query`, `query_as`) with
`bind` rather than its compile-time `query!` / `query_as!` macros, so
the crate builds without a live database at compile time. See
`src/db/queries.rs`.

### Tool (AI)

A function the bot exposes to the LLM through DeepSeek's or
Gemini's tool-call protocol. When the LLM emits a structured tool
call in its response, the bot dispatches it to the matching Rust
handler (e.g. `web_search`, `play_song`, `start_wordle`,
`create_tempban`) and feeds the result back into the conversation.
Defined in `src/ai/tools.rs`, dispatched in `src/ai/deepseek.rs`.

### Tool (MCP)

A function the bot exposes to external Model Context Protocol
clients — Claude Code, the bundled mcp-gateway, anything else
speaking JSON-RPC against the bot's `/mcp` endpoint. Each tool is a
method on the `DiscordTools` impl in `src/mcp/tools.rs` annotated
with `#[tool(description = "...")]`. The `#[tool_router]` macro on
the impl block discovers them automatically. Distinct from AI tools
above — different protocol, different consumer, different
registration mechanism. See
[Adding an MCP Tool](../development/adding-an-mcp-tool.md).

### Webhook (chargeback)

The HTTP endpoint exposed by the bot when `minecraft.chargeback =
true`, listening on the same port as the MCP server. The companion
Minecraft plugin POSTs to it when a chargeback is detected; the bot
verifies the signature, applies the configured restricted role, and
posts an interactive alert to the staff channel. See
[Minecraft Chargeback](../features/minecraft-chargeback.md).

## See also

- [Architecture Overview](../architecture/index.md) — how the
  components defined here fit together.
- [FAQ](faq.md) — the same vocabulary in context.
- [Codebase Tour](../development/codebase-tour.md) — every term
  here mapped to a file.
