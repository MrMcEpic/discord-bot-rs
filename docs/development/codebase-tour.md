# Codebase Tour

This page is the contributor's map. It walks the codebase module by
module, tells you what each file is responsible for, and names the
function or type you should open first if you're trying to understand
the area. Read it once before your first contribution; after that it
becomes a lookup table.

If you want the higher-level picture of how the parts talk to each other,
read the [Architecture Overview](../architecture/index.md) first — this
page assumes you already have that context and goes one level deeper.

## Repository layout

```
discord-bot-rs/
├── src/                    # the main bot crate
├── mcp-gateway/            # a second crate: the multi-instance MCP router
├── instances/              # per-instance config directories
│   └── example/            # config.toml + personality.txt starter
├── docs/                   # this mdBook
├── theme/                  # mdBook theme overrides
├── .github/                # CI workflows and issue/PR templates
├── Cargo.toml              # workspace root for the main crate
├── Dockerfile              # bot container build
├── docker-compose.yml      # bot + postgres default stack
└── README.md
```

The two crates (`src/` and `mcp-gateway/`) are built and tested
independently — CI runs format, clippy, build, and test on each. A
third top-level artifact is the mdBook under `docs/`, built by a
separate workflow and published to GitHub Pages.

## The main crate (`src/`)

Every path below is relative to `src/`.

### `main.rs` — entry point and shared state

[`src/main.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/main.rs)
is the one file you have to read before anything else. It does four
things:

1. **Declares every top-level module** — the `mod` declarations at the
   top are the ground truth for which directories under `src/` actually
   compile.
2. **Defines the `Data` struct** — this is the shared application state
   that poise hands to every command and event handler. It holds the
   `sqlx::PgPool`, a `reqwest::Client`, the loaded `Config`, the
   `personality` and `bot_name` strings, every `Option<T>` feature
   config (`auto_role_config`, `minecraft_config`, `join_role_config`,
   `welcome_config`), per-guild state `DashMap`s (`guild_players`,
   `track_handles`, `now_playing_msgs`, `idle_timers`,
   `connections_games`, `wordle_games`), a `RateLimiters` bundle, and
   one-shot startup flags (`mcp_started`, `started_at`).
3. **Builds the poise framework** in `main()` — loads the instance
   config, constructs the parent `m` command (pushing the optional
   `verify` subcommand if Minecraft verify is enabled), registers the
   event handler, and wires the prefix from `instance_cfg.command_prefix`.
4. **Spawns long-running background tasks** — the tempban unban checker
   (30s loop), the auto-role time promotion checker (60s loop), and the
   donator sync poller (interval from config). Each is a
   `tokio::spawn` that owns its own clones of `http` and the DB pool.

When you add a new feature that needs startup state, this is the file
where you both extend `Data` and spawn the task, then pass the `Data`
reference through into the module that needs it.

### `config.rs` — environment variables

[`src/config.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/config.rs)
is a single `Config` struct and a `Config::load()` function. It reads
`.env` via `dotenvy`, panics fast on missing required vars
(`DISCORD_TOKEN`, `CLIENT_ID`, `GUILD_ID`), and exposes the optional
API keys (`DEEPSEEK_API_KEY`, `GEMINI_API_KEY`, `FINNHUB_API_KEY`,
`MC_VERIFY_URL`, `MC_VERIFY_SECRET`) as `Option<String>`. The MCP
settings (`MCP_PORT`, `MCP_BIND_ADDR`, `MCP_AUTH_TOKEN`) and the
database settings (`DB_SCHEMA`, `DATABASE_URL`) are plain `String`
fields with non-`None` defaults. The `get_env_or_throw` helper also
panics on `your-...` placeholder values, so the bot refuses to boot
with an unedited `.env.example`.

### `instance_config.rs` — parsing `config.toml`

[`src/instance_config.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/instance_config.rs)
loads the per-instance `config.toml` — `bot_name`, `command_prefix`,
the `features` sub-table (feature flags), and optional typed config
sections (`AutoRoleConfig`, `MinecraftConfig` with its nested
`DonatorSyncConfig` and `ChargebackConfig`, `JoinRoleConfig`,
`WelcomeConfig`). It also resolves the instance directory via
`CONFIG_DIR` (default `.`) and loads `personality.txt` and the
optional welcome prompt from that directory. Everything here is
`Deserialize` + a small number of `default_*` functions for fields
that have sane defaults.

### `error.rs` — the `BotError` enum

[`src/error.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/error.rs)
defines `BotError`, the project-wide error type. Five variants —
`Serenity`, `Sqlx`, `Reqwest`, `SerdeJson`, `Other(String)` — with
`From` impls so every fallible call site can use `?`. It implements
`Display` and `std::error::Error`, which is enough for poise to accept
it as the `E` in `Context<'_, Data, E>`. Commands that return
`Err(...)` surface through poise's `on_error` handler in `main.rs`.

### `commands/` — the command tree

Every user-facing prefix command lives under
[`src/commands/`](https://github.com/MrMcEpic/discord-bot-rs/tree/master/src/commands).
The key file is
[`commands/mod.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/commands/mod.rs):
it declares the parent `m` command and lists every subcommand with
`#[poise::command(prefix_command, subcommands(...))]`. This is the one
place you register commands — `main.rs` only ever pushes the single
parent `m` into the framework's `commands` vec. **There are no slash
commands anywhere in this codebase.** Every command is
`prefix_command` only, usually with a `rename` and short `aliases`.

The individual files group commands by area:

- **`admin.rs`** — `setlog`, `djrole`, `djmode`. Server-admin settings
  that live in the `guild_settings` table.
- **`moderation.rs`** — `ban`, `unban`, `banlist`, `nuke`. Tempbans go
  through `db::queries::create_tempban`, which returns the expiry
  timestamp; the `main.rs` background task later calls
  `http.remove_ban` when expired bans are found.
- **`music.rs`** — `play`, `playlist`, `skip`, `stop`, `pause`,
  `resume`, `queue`, `nowplaying`, `remove`, `loop_cmd`, `shuffle`.
  All of them call into `music::voice` and
  `music::player::GuildPlayer` through `Data`.
- **`connections.rs`**, **`wordle.rs`** — thin wrappers over the
  corresponding game module. Each creates a `Game` struct, sends the
  initial embed + buttons, and inserts the game into the channel map
  on `Data`.
- **`stocks.rs`** — `stock` parent with `buy`, `sell`, `portfolio`,
  `price`, `leaderboard`, `history`, `reset` subcommands. The module
  refuses to run without `FINNHUB_API_KEY` via `require_finnhub_key`.
- **`minecraft.rs`** — the `verify` subcommand, which is only pushed
  into the parent `m` at startup when `features.minecraft` and
  `minecraft.verify` are both enabled in `config.toml`.
- **`help.rs`** — renders the help embed, dynamically showing the
  moderation and admin sections only if the invoking member has the
  matching permissions.

If you're adding a command, the [Adding a Command](adding-a-command.md)
page has a full worked example; start there rather than copying blindly
from any of these files.

### `events/` — gateway event dispatcher

[`events/mod.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/events/mod.rs)
is the non-command event handler. It's one big `match` over
`poise::serenity_prelude::FullEvent`:

- **`Ready`** → `ready::handle_ready` plus a one-shot MCP server start
  guarded by `data.mcp_started.swap(true)` so gateway reconnects don't
  re-launch the HTTP server.
- **`VoiceStateUpdate`** → `voice_state::handle_voice_state_update`,
  which triggers the "auto-leave when the channel is empty" behaviour.
- **`Message`** → `handle_message`, the largest branch. It does three
  things in order: bumps the `member_activity` row for auto-role,
  intercepts 5-letter messages as Wordle guesses in channels with an
  active game, and — if the message mentions the bot or replies to a
  bot message — hands off to `ai::deepseek::handle_mention`.
- **`InteractionCreate` (Component)** → routes to one of the button
  handlers by prefix: `music_*`, `game_*` (Connections), `cb_*`
  (chargeback buttons). Music buttons enforce "you must be in the same
  voice channel" and the DJ mode check.
- **`GuildMemberAddition`** → `member_join::handle_member_join`, which
  applies the join role and (if enabled) generates a welcome message
  through the AI pipeline.

`events/ready.rs` is tiny (it just logs and sets a presence string).
`events/voice_state.rs` checks whether the bot is alone in its voice
channel and cancels playback / disconnects. `events/member_join.rs`
handles both static join roles and the AI-generated welcome flow with
its own rate limit (`last_welcome`).

When adding a new reactive behaviour — for example, a new button prefix
— the routing lives here.

### `ai/` — the AI chat pipeline

[`src/ai/`](https://github.com/MrMcEpic/discord-bot-rs/tree/master/src/ai)
is the @mention pipeline. It's the single largest subtree in the
codebase and the one where you should start by reading the
[AI Pipeline](../architecture/ai-pipeline.md) architecture page rather
than the files.

- **`deepseek.rs`** — by far the biggest file in the project. It owns
  `handle_mention`, the message-history builder, the DeepSeek HTTP
  call, the Gemini fallback path, and every tool implementation the
  bot exposes to the LLM. If a tool call resolves to playing music,
  creating a tempban, or starting a game, the dispatch lives here.
- **`tools.rs`** — JSON schema definitions for the tool set the bot
  advertises to DeepSeek/Gemini: `web_search`, `play_song`, `skip`,
  `stop`, `pause`, `resume`, `show_queue`, `now_playing`, `shuffle`,
  `set_loop`, `remove_from_queue`, `tempban`, `unban`, `nuke`,
  `stock_buy`, `stock_sell`, `stock_price`, `stock_portfolio`,
  `stock_leaderboard`, `connections_start`, `wordle_start`, and a few
  others. Predicate helpers (`is_search_tool`, `is_moderation_tool`,
  ...) are used by `deepseek.rs` to route each tool call.
- **`dsml.rs`** — parses "DSML" (DeepSeek Markup Language) tool-call
  blocks embedded in model output, for models that emit structured
  tool calls in prose rather than in the OpenAI-style `tool_calls`
  array.
- **`sanitize.rs`** — strips role markers, DeepSeek control tokens, and
  Llama-style `[INST]` / `<<SYS>>` markers from user input so users
  can't trivially inject prompts.
- **`split.rs`** — splits responses over Discord's 2000-character
  message limit without breaking code fences or multi-byte chars.
- **`search.rs`** — DuckDuckGo scraper, implemented via `curl` rather
  than `reqwest` because DDG returns a non-result page when it sees
  `reqwest`'s default headers.
- **`confirmation.rs`** — wraps moderation tool calls with a "react to
  confirm" button flow, with a 30-second timeout, so the AI can't
  tempban someone without explicit human sign-off.

### `music/` — the music player

[`src/music/`](https://github.com/MrMcEpic/discord-bot-rs/tree/master/src/music)
has four files and is straightforward if you read them in order.

- **`player.rs`** — the `GuildPlayer` struct: a `VecDeque<Track>`, a
  `current: Option<Track>`, a `paused: bool`, a `LoopMode`, and a
  `MAX_QUEUE_LENGTH` of 100. All the queue operations (`enqueue`,
  `advance`, `skip_current`, `shuffle`, `remove`) live here. It has
  zero I/O; just data.
- **`track.rs`** — wraps `yt-dlp` as a subprocess. `resolve_track` and
  `resolve_tracks` spawn yt-dlp with the project's cookie jar and
  node-runtime path, parse the JSON output, and return `Track` values.
  It also exposes `ytdlp_user_args` which `voice.rs` passes into
  songbird's `YoutubeDl` input source.
- **`voice.rs`** — the songbird wrapper. `join_channel`, `play_track`,
  `stop_playback`, `leave_channel`, and the `TrackEndHandler` that
  advances the queue when a track finishes. `PlaybackContext` is the
  cloned state bundle passed into event handlers.
- **`embeds.rs`** — builds the "now playing", "added to queue", and
  "queue" embeds, plus the two button rows (`music_controls`). Button
  IDs are `music_pauseresume`, `music_skip`, `music_stop`,
  `music_shuffle`, `music_loop`, `music_queue`; this is what
  `events/mod.rs` dispatches on.

### `wordle/`, `connections/`, `stocks/` — the games

Each game module has the same shape:

- **`game.rs`** — the pure-Rust game state (guess list, selected
  words, mistakes remaining, game-over / won predicates).
- **`api.rs`** — the upstream fetch (NYT Wordle JSON, NYT Connections
  JSON, or Finnhub stock quotes, with a 60-second `stock_price_cache`
  table in front of the Finnhub calls).
- **`embeds.rs`** — the Discord rendering (emoji grid for Wordle, the
  4×4 button grid for Connections, portfolio/leaderboard/transaction
  history for stocks).

Wordle lives in
[`src/wordle/`](https://github.com/MrMcEpic/discord-bot-rs/tree/master/src/wordle)
and also ships a `words.txt` wordlist compiled in via `include_str!`.
Connections is in
[`src/connections/`](https://github.com/MrMcEpic/discord-bot-rs/tree/master/src/connections).
Stocks is in
[`src/stocks/`](https://github.com/MrMcEpic/discord-bot-rs/tree/master/src/stocks)
and, unlike the word games, has no `game.rs` — it has no in-memory
session state because every holding is persisted to Postgres.

### `minecraft/` — the Minecraft integration

[`src/minecraft/`](https://github.com/MrMcEpic/discord-bot-rs/tree/master/src/minecraft)
glues the bot to an external Minecraft server's HTTP API. Three files:

- **`api.rs`** — the wire types (`VerifyRequest`, `VerifyResponse`) and
  the `verify()` HTTP call that posts a code + Discord ID to the MC
  server's verify endpoint.
- **`donator_sync.rs`** — `fetch_donators` pulls the current donator
  list from the MC server; `sync_roles` reconciles it against Discord
  role state, adding and removing supporter/premium roles. This is
  driven by the `check_interval` loop spawned in `main.rs`.
- **`chargeback.rs`** — an `axum::Router` that listens for webhook
  posts from the MC server when a chargeback happens. The router
  verifies the secret, looks up the Discord user by UUID, posts a
  message to the staff channel with restrict/ignore buttons, and
  handles those button clicks (the `cb_*` custom ID prefix dispatched
  in `events/mod.rs`). The router is optional and is only mounted
  under the MCP axum app when `minecraft.chargeback = true`.

### `mcp/` — the embedded MCP server

[`src/mcp/`](https://github.com/MrMcEpic/discord-bot-rs/tree/master/src/mcp)
is the in-process Model Context Protocol server. Two files:

- **`mod.rs`** — sets up `rmcp`'s `StreamableHttpService`, wraps it in
  an axum app with a bearer-token middleware, optionally mounts the
  chargeback webhook router under the same app, and binds on
  `MCP_BIND_ADDR:MCP_PORT`.
- **`tools.rs`** — the `DiscordTools` struct with a `ToolRouter` and
  one `#[tool]` method per exposed capability: listing channels,
  reading messages, sending messages, managing roles, and so on. Every
  method wraps the serenity HTTP call with a 10-second `timeout` and
  converts errors via a pair of helpers (`api_err`, `timeout_err`).

If you're adding an MCP tool, [Adding an MCP Tool](adding-an-mcp-tool.md)
walks through the macros and registration.

### `db/` — Postgres and queries

[`src/db/`](https://github.com/MrMcEpic/discord-bot-rs/tree/master/src/db)
has three files:

- **`mod.rs`** — `init_pool`. Connects once, runs
  `CREATE SCHEMA IF NOT EXISTS "{schema}"`, then creates a
  `PgPoolOptions` with an `after_connect` that sets `search_path` on
  every new connection. This is how multi-instance schema isolation
  works: every instance uses the same `DATABASE_URL` but its own
  `DB_SCHEMA`. After the pool is built, `mod.rs` runs every `CREATE
  TABLE IF NOT EXISTS` statement for the bot's schema.
- **`models.rs`** — one `FromRow` struct per table: `Tempban`,
  `GuildSettings`, `MemberActivity`, `StockPortfolio`, `StockHolding`,
  `StockTransaction`, `StockPriceCache`.
- **`queries.rs`** — every query function. `get_guild_settings`,
  `upsert_guild_setting`, `create_tempban`, `get_expired_bans`,
  `mark_unbanned`, `get_stock_portfolio`, `buy_stock`, `sell_stock`,
  and so on. Queries use the raw `sqlx::query*` helpers with `bind`
  rather than the compile-time-checked `query!` / `query_as!` macros,
  so the crate builds without a live database at compile time.

### `autorole.rs` — the role-promotion rule

[`src/autorole.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/autorole.rs)
is a single small module: `meets_criteria(activity, config)` evaluates
age and message-count thresholds against an `AutoRoleConfig`, and
`try_promote` adds the target role, removes the source role, and marks
the row `promoted = true` in `member_activity`. It's called from two
places: the background time-check loop in `main.rs`, and the
`handle_message` hot path in `events/mod.rs` so promotion happens
immediately on the message that tips the count over.

### `util/` — small helpers

- **`util/duration.rs`** — `parse_duration` (`"3d"`, `"2h"`, `"30m"`,
  capped at 365 days) and `format_duration_ms` / `format_track_duration`.
  Used by tempbans, auto-role `min_age`, and the music "now playing"
  line.
- **`util/ratelimit.rs`** — the `SlidingWindowLimiter` (a
  `DashMap<String, Vec<Instant>>`) and `RateLimiters`, which bundles
  four limiters: `ai` (10/60s), `music` (15/30s), `moderation` (5/60s),
  and `stocks` (10/30s). Every limiter is keyed by a stringified user
  ID.

## The `mcp-gateway` crate

[`mcp-gateway/`](https://github.com/MrMcEpic/discord-bot-rs/tree/master/mcp-gateway)
is a small separate crate that sits in front of one or more running bot
instances and exposes a single MCP endpoint to an outside AI client.
Five files:

- **`main.rs`** — builds `GatewayConfig`, constructs one
  `BackendClient` per configured instance, wires them into a
  `GatewayState`, mounts the axum router, binds, and spawns a 5-minute
  background task that refreshes the guild map.
- **`config.rs`** — reads `GATEWAY_PORT` (default `9100`), the
  optional `MCP_AUTH_TOKEN` bearer secret, and the required
  `INSTANCES` env var of the form `name1=url1,name2=url2` into a list
  of `Instance { name, url }`.
- **`backend.rs`** — the per-instance MCP client. Opens a Streamable
  HTTP SSE session to a bot's MCP endpoint and exposes `initialize`,
  `list_tools`, `call_tool`, `list_guilds`, and `health_check` for the
  gateway server to call.
- **`routing.rs`** — the guild-to-instance router. Keeps a
  `HashMap<guild_id, instance_name>` so that the gateway can look at a
  tool call's `guild_id` and forward it to the right backend.
- **`server.rs`** — the axum handlers. `POST /mcp` accepts a JSON-RPC
  envelope, authenticates with the optional bearer token, resolves the
  target backend via the router, forwards the call, and streams the
  response back. There's also a cached `tools/list` aggregation so
  the client sees the union of every backend's tools, plus a
  gateway-only `list_instances` tool.

[MCP Gateway Routing](../architecture/mcp-gateway-routing.md) has the
sequence diagram.

## Cross-cutting patterns

A few conventions hold across the whole codebase. If you're not sure
how to do something, follow the pattern that already exists.

- **Every command takes `Context<'_>`** — that's the type alias for
  `poise::Context<'a, Data, BotError>` at the bottom of
  [`src/main.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/main.rs).
  You access shared state through `ctx.data()`, which hands you back a
  `&Data`.
- **Every DB query is in `db/queries.rs`** — commands do not build SQL
  inline. If you need a new query, add a function there and call it
  from the command.
- **Every long-running background task is a `tokio::spawn`** in
  `main()`, cloning the `Arc`-friendly parts of `Data` it needs
  (`db.clone()`, `http.clone()`). Tasks log via `tracing::info!`
  on startup and `tracing::warn!` / `tracing::error!` on failure,
  and they never panic — errors are logged and the loop keeps going.
- **Every optional feature is gated twice**: once by the
  `features.<name>` flag in `config.toml`, and once by the presence of
  the feature's `Option<Config>` on `Data`. The event handler and the
  command code both early-return when the feature is disabled.
- **Locking discipline is flat**: `DashMap<GuildId, Arc<Mutex<T>>>` for
  per-guild state. Look up the entry, clone the `Arc`, acquire the
  `Mutex`, do the work, drop the guard. Never hold a `DashMap` entry
  across an `.await`.
- **No `unwrap()` in hot paths.** Startup code in `main.rs` is
  allowed to `.expect()` on missing env vars and pool creation;
  everything downstream returns `BotError`.

## Where to look for…

Use this table as a lookup when you can't remember where something
lives.

| Looking for | Start here |
|---|---|
| Adding a new command | [`commands/mod.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/commands/mod.rs) and [Adding a Command](adding-a-command.md) |
| Adding a new event handler branch | [`events/mod.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/events/mod.rs) |
| Adding a new AI tool for DeepSeek | [`ai/tools.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/ai/tools.rs) and the dispatch in `ai/deepseek.rs` |
| Adding a new MCP tool | [`mcp/tools.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/mcp/tools.rs) and [Adding an MCP Tool](adding-an-mcp-tool.md) |
| Adding a whole new feature module | [Adding a Feature Module](adding-a-feature-module.md) |
| Adding a DB table | [`db/mod.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/db/mod.rs) (CREATE TABLE), [`db/models.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/db/models.rs) (struct), [`db/queries.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/db/queries.rs) (functions) |
| Changing rate limits | [`util/ratelimit.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/util/ratelimit.rs) (`RateLimiters::new`) |
| Changing the default personality | The instance's `personality.txt`, not the code |
| Adding a required env var | [`config.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/config.rs), then propagate through `Data` |
| Adding a per-instance TOML option | [`instance_config.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/instance_config.rs), then `main.rs` wiring |
| Changing the music queue size | `MAX_QUEUE_LENGTH` in [`music/player.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/music/player.rs) |
| Fixing Wordle validation | `is_valid_word` and `words.txt` in `wordle/game.rs` |
| Discord permission checks | `ctx.author_member()` + `member_permissions` — see `commands/help.rs` for the pattern |

## Code style conventions

Things you'll notice reading the code, and things reviewers will ask
for on PRs:

- **Hard tabs, width 4.** `rustfmt.toml` enforces this. Run
  `cargo fmt` before opening a PR.
- **`?` over `unwrap`** in fallible code. `expect` is only acceptable
  at startup in `main.rs`.
- **Errors convert through `BotError`** via `From` impls; commands and
  event handlers return `Result<(), BotError>`.
- **`tracing` over `println!`.** All logs go through `tracing::info!`,
  `warn!`, `error!` so they're structured and filterable via
  `RUST_LOG`.
- **No `async` closures inside `DashMap` callbacks.** Look up, clone
  the `Arc`, drop the entry guard, then `.await` on the clone.
- **Feature-gated startup is verbose by design** — `main.rs` logs each
  optional feature's activation at `info!` and warns loudly if the
  TOML config is missing. Copy that pattern for new features.

## Next steps

- [Building Locally](building-locally.md) — run the crate with `cargo`
  outside of Docker.
- [Adding a Command](adding-a-command.md) — the shortest path from
  "I want a new command" to "PR ready."
- [Adding a Feature Module](adding-a-feature-module.md) — when a
  feature is big enough to deserve its own directory under `src/`.
- [Adding an MCP Tool](adding-an-mcp-tool.md) — exposing a new
  capability to external MCP clients.
- [Testing](testing.md) — what the test surface looks like today and
  where to add coverage.
