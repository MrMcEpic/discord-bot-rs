# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- (track new changes here until the next release)

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

[Unreleased]: https://github.com/MrMcEpic/discord-bot-rs/compare/v0.4.6...HEAD
[0.4.6]: https://github.com/MrMcEpic/discord-bot-rs/releases/tag/v0.4.6
