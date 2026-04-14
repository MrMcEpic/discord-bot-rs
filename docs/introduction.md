# Introduction

discord-bot-rs is a multi-instance Discord bot framework written in Rust.
One binary runs any number of bot instances from a single shared codebase,
each with its own personality file, configuration, database schema, and
feature set. The goal is a batteries-included bot you can actually operate
in production: a real music player, real AI chat, real moderation, and a
small enough codebase that you can read the whole thing in an afternoon.

## What it does

The bot is built from independent feature modules that you enable or
disable per instance in `config.toml`. Out of the box you get:

- **AI chat** — @mention the bot and it replies in-character, using
  DeepSeek V3.2 as the primary provider and Google Gemini as a fallback,
  with a configurable personality prompt.
- **Music** — yt-dlp plus songbird plus ffmpeg, with a full queue, loop
  modes, shuffle, and interactive button controls on the now-playing
  embed.
- **Games** — the daily NYT Wordle, NYT Connections, and a virtual stock
  trading game backed by live Finnhub quotes.
- **Moderation** — tempbans with auto-unban, channel nukes, audit log
  routing, and a DJ-only mode for music commands.
- **Minecraft integration** — link a Discord account to a Minecraft
  account, sync donator roles from a game server, and route chargeback
  alerts to staff.
- **An embedded MCP server** — every instance exposes a set of Model
  Context Protocol tools so external AI clients can read channels, send
  messages, and manage roles through the bot.

## Who it's for

**Self-hosters** who want a bot they can stand up with one
`docker compose up` and grow into. The [Quickstart](getting-started/quickstart.md)
takes about ten minutes.

**Developers** who want to read and extend real Rust code — a working
serenity + poise + songbird + sqlx + axum stack with no magic. The
[Codebase Tour](development/codebase-tour.md) is the fastest way in.

**Community admins** running a Minecraft server, a Discord, and whatever
else, who want a single bot that handles account verification, donator
roles, and chargeback alerts without writing glue code.

## What this book covers

- [Getting Started](getting-started/index.md) — prerequisites, quickstart,
  first-bot tutorial, and verification.
- [Configuration](configuration/index.md) — `.env`, `config.toml`,
  personality files, secrets, and multi-instance layouts.
- [Features](features/index.md) — a page per feature, with activation,
  configuration, and troubleshooting.
- [Architecture](architecture/index.md) — how the pieces fit together.
- [Deployment](deployment/index.md) — Docker Compose, Postgres, upgrades,
  and the production checklist.
- [Development](development/index.md) — codebase tour, building locally,
  adding commands, and the contribution workflow.
- [Reference](reference/command-list.md) — command list, MCP tool
  catalog, FAQ, glossary.

## Where to start

If you want to **run** the bot, head to the [Quickstart](getting-started/quickstart.md).
If you want to **contribute**, start with the [Development Overview](development/index.md)
and then the [Codebase Tour](development/codebase-tour.md). If you're
**evaluating** the project for a team, the [Architecture Overview](architecture/index.md)
is the shortest path to "how does this actually work."

## License

discord-bot-rs is licensed under the [GNU Affero General Public License
v3.0 or later](https://github.com/MrMcEpic/discord-bot-rs/blob/master/LICENSE).
Contributions are accepted under the same license — see
[CONTRIBUTING.md](https://github.com/MrMcEpic/discord-bot-rs/blob/master/CONTRIBUTING.md)
for the inbound-license note.
