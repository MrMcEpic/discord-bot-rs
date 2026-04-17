# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **MCP message reactions**: `add_reaction` and `remove_reaction` tools accept unicode emoji, Discord custom-emoji format (`<:name:id>` / `<a:name:id>`), or a bare custom-emoji snowflake. `remove_reaction` targets the bot's own reaction only. (Closes #6)
- **MCP moderation rounding**: `set_nickname` (set/clear member nickname), `get_bans` (list active bans with paging), `remove_timeout` (lift an active timeout). Round out the existing moderation suite. (Closes #11)
- Brings the bot tool catalog to 29.

## [0.8.0] - 2026-04-16

### Added
- **`search_messages` MCP tool** — composable channel-message search. Filters: `author_id`, `author_name` (case-insensitive substring), `content` (case-insensitive substring), `after` and `before` (each accepts ISO 8601 dates like `2026-07-03` or numeric snowflakes). Pages backward from `before` (or now) until `limit` matches are found, the `after` boundary is reached, or the `max_pages` safety cap is hit. Returns matching messages plus a summary line stating how many were scanned and whether the search was truncated. Brings the bot tool catalog to 24.

### Added
- **`get_recent_messages` MCP tool** — read recent channel messages, newest first, with `limit` (1-100) and `before` for pagination. Brings the bot tool catalog to 23.

### Fixed
- **`mcp-gateway`: outgoing `Host` header override.** The bundled Compose deploy could never proxy MCP requests because rmcp's `StreamableHttpService` rejects non-loopback `Host` headers (DNS-rebinding protection); the gateway was sending `Host: <service-name>:9090` and getting back `403 Forbidden`. Gateway now sends `Host: localhost:9090` on every outgoing POST.
- **`mcp-gateway`: periodic tool list refresh.** The 5-minute background refresh only re-synced guild maps; the cached tool list was filled once at startup and never updated, so adding a new tool to a backend bot left it invisible to clients until the gateway itself was restarted. Now both `refresh_guild_map` and `refresh_tool_list` run on the periodic loop.

## [0.6.1] - 2026-04-16

Dependency-update sweep — applies the upgrades Dependabot was suggesting on the closed PR queue. No functional changes.

### Changed
- `mcp-gateway`: `reqwest` 0.12 → 0.13 (main crate stays on 0.12 until songbird 0.6 rebases off it)
- `toml` 0.8 → 1.1
- `rand` 0.8 → 0.10 (call sites migrated to `rand::rng()` and `RngExt::random_range`)
- All within-semver updates picked up by `cargo update` (notably `tokio` 1.50 → 1.52 in the gateway)
- GitHub Actions: `docker/login-action` v3 → v4, `softprops/action-gh-release` v2 → v3, `actions/upload-pages-artifact` v3 → v5

## [0.6.0] - 2026-04-16

First post-launch polish release. Focused on hardening, correctness fixes, and a real test suite. No breaking changes for existing deployments whose MCP server was already bound to `127.0.0.1` or already had `MCP_AUTH_TOKEN` set; see **Changed** for the bundled-Compose impact.

### Added
- `sqlx::migrate!` against a `migrations/` directory replaces the in-code `CREATE TABLE` block; per-schema `_sqlx_migrations` table tracks applied versions
- Panic recovery and graceful shutdown for supervised background tasks (`tokio::task::JoinSet` + `futures::FutureExt::catch_unwind`)
- Per-user cleanup task for every rate-limiter bucket; music, stocks, and moderation limiters are now enforced (previously defined but unwired)
- 92 unit tests (up from 37) and 18 Postgres-backed integration tests via `#[sqlx::test]`; CI runs both against a `postgres:17` service
- mdBook Mermaid preprocessor; the seven architecture diagrams now render instead of appearing as code blocks
- Remaining 25 documentation stub pages filled in (getting-started, configuration, features, architecture, deployment, development, reference)
- Quickstart step for generating the MCP auth token — the bundled Compose stack now requires it to start (see Changed)

### Changed
- `mcp-gateway` forwards its `MCP_AUTH_TOKEN` to each backend as the `Authorization: Bearer` header. Combined with the bot-side startup guard, the bundled Compose deploy is now a shared-secret model: `MCP_AUTH_TOKEN` in the instance `.env` and `MCP_GATEWAY_AUTH_TOKEN` in a repo-root `.env` must hold the same value.
- Bot refuses to start when `MCP_BIND_ADDR` is non-loopback *and* `MCP_AUTH_TOKEN` is empty; gateway refuses to start unconditionally without `MCP_AUTH_TOKEN`. Both comparisons are constant-time via the `subtle` crate.
- Stock portfolio columns moved from `DOUBLE PRECISION` to `NUMERIC(18, 4)`; Rust side uses `rust_decimal::Decimal` end-to-end so cents no longer drift over many fractional-share trades (migration in `20260414000001_stocks_decimal.sql`)
- Apache-style contributor grant replaces the prior inbound-license language in `CONTRIBUTING.md`
- Contribution terms + recent-change reconciliation pass across the docs

### Fixed
- **MCP:** 64 KiB body cap (`RequestBodyLimitLayer`); spec-compliant JSON-RPC parse error response; channel-targeting tools verify the channel belongs to the resolved guild before acting
- **Gateway:** dead pending-dispatcher code removed; startup panics instead of silently accepting empty auth
- **Stocks:** reset/buy/sell race closed with row-level locking; text-based confirmations replaced with a button that expires after one click
- **Autorole:** atomic-claim UPDATE prevents two concurrent event handlers from double-promoting the same member
- **Music:** Now Playing embed lifecycle consolidated so orphans don't accumulate after skip or loop transitions; skip no longer races the natural `TrackEnd` to advance twice; yt-dlp invocations have both a timeout and `kill_on_drop`
- **Welcome:** per-user rate limit replaces a global `Mutex` that serialised every join system-wide
- **Games:** Wordle and Connections refuse to overwrite an in-progress game; dates are validated and cross-checked against NYT's `print_date`; Wordle dictionary is a `HashSet` (was a `binary_search` on an unsorted `Vec`)
- **AI:** tool-search capped at three rounds (prompt said three, code allowed five); DSML closing tag accepts optional `/`; DSML strip scoped so prose isn't mangled; user-visible errors never leak raw upstream text
- **Chargeback:** staff role IDs moved from a hardcoded list into `config.toml`
- **Error surface:** `BotError` Display impls stop echoing internal error text; `format!("Database error: {e}")` wraps removed so sqlx's built-in context reaches logs
- **Docker:** build context now copies `migrations/` so `sqlx::migrate!` can compile-embed them
- **Polish:** `mistakes_dots` underflow guard; several places stop cloning an entire `Guild` to read one field

## [0.5.0] - 2026-04-14

Public launch on GitHub. No functional changes from 0.4.6 beyond repo hygiene, documentation, release automation, and cleanup of maintainer-specific state.

### Added
- Public repository launch with full documentation at <https://mrmcepic.github.io/discord-bot-rs/>
- AGPL-3.0-or-later license
- Hosted mdBook documentation covering getting started, configuration, features, architecture, deployment, and development (30+ pages of real content plus ~25 stubs)
- `CONTRIBUTING.md` with dev setup and inbound-license clause
- `SECURITY.md` with vulnerability reporting process
- `CODE_OF_CONDUCT.md` (Contributor Covenant v2.1)
- GitHub Actions CI: fmt, clippy (deny warnings), check, test, Docker build — for both the main crate and mcp-gateway
- GitHub Actions docs workflow: mdBook build and Pages deployment (landing page + book)
- GitHub Actions release workflow: multi-arch Docker image builds published to `ghcr.io`
- Dependabot config for cargo, GitHub Actions, and Docker ecosystems
- Issue and PR templates plus dependabot grouped updates
- Single-file HTML landing page at the Pages root
- Seven Mermaid architecture diagrams (top-level components, multi-instance topology, data flow, AI pipeline, music pipeline, MCP gateway routing, database ER diagram)

### Changed
- Generic parameterized `bot` service in `docker-compose.yml` (was hardcoded `examplebot`/`secondbot`)
- `INSTANCE_DIR` env var controls which instance directory the bot mounts
- `INSTANCES` env var controls MCP gateway routing table
- Example instance (`instances/example/`) expanded into a fully documented config reference
- Rust sources formatted with `hard_tabs = true, tab_spaces = 4` (one-time reformat)
- Help command now uses the per-instance `bot_name` from `config.toml` instead of a hardcoded string
- README rewritten as a showcase landing with badges, feature cards, architecture diagram, and documentation links

### Removed
- Hardcoded `OWNER_ID` constant and the check-and-return branches that protected a specific user from `!m ban` and AI tempban tool calls
- `version_info.txt` and its `include_str!` reference in the AI system prompt
- `ecosystem.config.cjs` (stale PM2 leftover from a previous deployment)
- `instances/examplebot/` and `instances/secondbot/` (maintainer's private configs, moved to a private fork)
- `docs/superpowers/` internal design specs and plans (moved to private fork)
- Hardcoded test-fixture instance names and guild IDs in `mcp-gateway` tests

### Fixed
- 62 pre-existing clippy warnings across both crates now pass `cargo clippy --all-targets -- -D warnings`

## [0.4.6] - 2026-04-14

Bootstrap entry. This release represents the project state at the time of public launch. Features are grouped by subsystem.

### Added

**Core**
- Multi-instance architecture: one binary runs any number of bot instances, each with its own config directory, database schema, personality, and feature set
- Per-instance PostgreSQL schema isolation
- Graceful shutdown and idle-timer based voice disconnect

**AI Chat**
- DeepSeek V3.2 primary provider (`deepseek-chat`)
- Google Gemini fallback provider
- @mention activation in any channel the bot can read
- 30-minute / 10-message conversation context window
- Personality file as system prompt (`personality.txt` per instance)
- Response sanitization and AI tool support (web search, confirmation workflow)

**Music**
- yt-dlp + ffmpeg audio pipeline with OGG/Opus passthrough (256 kbps)
- Queue with loop, shuffle, skip, and previous
- Interactive button controls on the "now playing" embed
- Auto-leave on empty voice channel

**Games**
- Wordle (daily word guess)
- Connections (group-by-category)
- Virtual stock trading with real-time Finnhub data

**Moderation**
- Slash commands for ban, kick, timeout, role assignment

**Member Join**
- Optional join role assignment for new members
- Optional AI-generated welcome message with rate limiting
- Auto-role promotion based on age-in-guild and message-count criteria

**Minecraft Module**
- `!m verify` command to link Discord to a Minecraft account via companion plugin
- Donator sync: polls MC server and syncs supporter/premium Discord roles
- Chargeback alerts: webhook-driven role stripping and interactive staff alert flow

**MCP Server**
- Embedded Model Context Protocol server for programmatic Discord management
- 22 tools covering guilds, channels, roles, members, messages
- Supports Claude Code via HTTP transport
- Optional bearer-token authentication

**MCP Gateway**
- Separate service that routes MCP requests across multiple bot instances
- Streamable HTTP SSE protocol support
- Per-instance session management

**Deployment**
- Docker Compose setup with bundled PostgreSQL
- Health checks for all services

### Known Issues
- AI context bleed: the message history builder can mix context from concurrent conversations in the same channel. Tracked in [#1](https://github.com/MrMcEpic/discord-bot-rs/issues/1).
- MCP OAuth: bearer-token auth works but Claude Code's HTTP transport expects OAuth 2.1. Currently running without auth on localhost only. Tracked in [#2](https://github.com/MrMcEpic/discord-bot-rs/issues/2).

[Unreleased]: https://github.com/MrMcEpic/discord-bot-rs/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/MrMcEpic/discord-bot-rs/releases/tag/v0.5.0
[0.4.6]: https://github.com/MrMcEpic/discord-bot-rs/releases/tag/v0.4.6
