# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- (track new changes here until the next release)

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
- AI context bleed: the message history builder can mix context from concurrent conversations in the same channel. Tracked in a known-issue GitHub issue.
- MCP OAuth: bearer-token auth works but Claude Code's HTTP transport expects OAuth 2.1. Currently running without auth on localhost only. Tracked in a known-issue GitHub issue.

[Unreleased]: https://github.com/MrMcEpic/discord-bot-rs/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/MrMcEpic/discord-bot-rs/releases/tag/v0.5.0
[0.4.6]: https://github.com/MrMcEpic/discord-bot-rs/releases/tag/v0.4.6
