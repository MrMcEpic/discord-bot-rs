# Environment Variables

This page is the canonical reference for every environment variable the bot reads. The source of truth is `Config::load()` in [`src/config.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/config.rs); if you find a variable in the code that isn't on this page, please open an issue.

## Overview

The bot reads its environment from two places:

1. A `.env` file in the current working directory, parsed at startup by [`dotenvy`](https://crates.io/crates/dotenvy). The file is plain `KEY=VALUE` lines; comments start with `#`. Empty values are treated as if the variable were unset for every optional key here, so `DEEPSEEK_API_KEY=` means "no DeepSeek key."
2. The process environment itself. Anything Docker Compose passes via `env_file` or an `environment:` block also works, and overrides the same key in the file.

Inside Docker Compose the standard pattern is `env_file: instances/yourbot/.env`, which loads the file once when the container starts. There is no live reload — changing a value requires restarting the container.

The variables fall into seven groups: Discord core (required), database (required), AI providers (optional), Finnhub (optional), Minecraft (optional), the embedded MCP server (optional), and the MCP gateway (optional, only used by the gateway service).

## Discord core (required)

| Name             | Required | Default | Description                                                  |
|------------------|----------|---------|--------------------------------------------------------------|
| `DISCORD_TOKEN`  | yes      | —       | Bot token from the Discord Developer Portal                  |
| `CLIENT_ID`      | yes      | —       | Application (client) ID for the bot user                     |
| `GUILD_ID`       | yes      | —       | Snowflake of the guild this instance is bound to             |

All three are validated at startup. If any is missing the bot panics with `<KEY> must be set in .env`. There is also a placeholder check: if a value still starts with the literal string `your-` (as in the shipped `.env.example`), the bot panics with a hint that you forgot to fill it in.

### `DISCORD_TOKEN`

Created in the Discord Developer Portal under your application's *Bot* page. Format is `MTxxxxxxxxxxxxxxxxxxxxx.G_xxxxxxxxxxxxxxxxxxxxxxxxxxx` or similar — there is no fixed length, but if it doesn't start with the right prefix Discord will reject it. Treat this like a password: it grants full control over the bot user. See [Secrets Management](secrets-management.md) for rotation.

### `CLIENT_ID`

The application's snowflake ID, also from the Developer Portal (top of the *General Information* page). The bot uses this for command registration. It is not a secret in the same way the token is — leaking it doesn't compromise the bot — but you should still keep it with the rest of your config.

### `GUILD_ID`

The snowflake of the Discord server this instance manages. The bot uses this as the default guild for its MCP tools, event handling, and auto-role checks. Right-click your server icon in Discord with Developer Mode enabled to copy it.

## Database (required)

| Name           | Required | Default                                                                | Description                                              |
|----------------|----------|------------------------------------------------------------------------|----------------------------------------------------------|
| `DATABASE_URL` | yes      | `postgresql://discord_bot:discord_bot_pass@localhost:5432/discord_bot` | PostgreSQL connection string                             |
| `DB_SCHEMA`    | yes      | `public`                                                               | Postgres schema this instance reads and writes           |

### `DATABASE_URL`

Standard `postgresql://user:password@host:port/database` connection string, parsed by [`sqlx`](https://crates.io/crates/sqlx). The default points at the bundled Compose service; if you're using `docker-compose.yml` from this repo as-is, keeping it pointed at `postgres:5432` (the service name on the Compose network) works. Outside Docker, point it at wherever your Postgres lives.

The default is technically a fallback rather than a hard requirement — the loader uses it if the variable is unset. But you should always set it explicitly so that misconfigurations fail loudly instead of silently connecting to a fictional `localhost:5432`.

### `DB_SCHEMA`

The Postgres schema name this instance owns. The default is `public`, but for any real deployment you should set this to a unique value per instance — `mybot1`, `mybot2`, and so on. At connection time the bot runs `SET search_path TO "<schema>"` on every new pool connection (see `src/db/mod.rs`), so all queries land in that schema and instances can't see each other's tables. See [Multiple Instances](multiple-instances.md) and [Multi-Instance Model](../architecture/multi-instance-model.md) for the full picture.

## AI providers (optional)

| Name               | Required | Default | Description                                |
|--------------------|----------|---------|--------------------------------------------|
| `DEEPSEEK_API_KEY` | no       | unset   | API key for DeepSeek chat completions      |
| `GEMINI_API_KEY`   | no       | unset   | API key for Google Gemini chat completions |

Both are independently optional. If both are unset the AI chat feature is disabled — mention the bot and you'll get nothing back. If only one is set it is used. If both are set the bot uses DeepSeek as primary and falls back to Gemini on error. Empty strings (`DEEPSEEK_API_KEY=`) are normalized to "unset."

### `DEEPSEEK_API_KEY`

Get one at [platform.deepseek.com](https://platform.deepseek.com/). DeepSeek's chat models are the cheapest of the supported providers and are recommended as the primary. See the [AI Chat](../features/ai-chat.md) feature page for model selection details.

### `GEMINI_API_KEY`

Get one in Google AI Studio. Used as a fallback when DeepSeek is set, or as the only provider when DeepSeek is not.

## Finnhub (optional)

| Name              | Required | Default | Description                                |
|-------------------|----------|---------|--------------------------------------------|
| `FINNHUB_API_KEY` | no       | unset   | API key for the [Stocks](../features/games-stocks.md) feature |

Required only if you use the virtual stock trading game. Free tier keys are available at [finnhub.io](https://finnhub.io/) and have generous limits. If unset, stocks-related commands return a not-configured message.

## Minecraft integration (optional)

| Name               | Required                            | Default | Description                                  |
|--------------------|-------------------------------------|---------|----------------------------------------------|
| `MC_VERIFY_URL`    | when any minecraft sub-feature is on| unset   | Base URL of the companion plugin's HTTP API  |
| `MC_VERIFY_SECRET` | when any minecraft sub-feature is on| unset   | Shared secret for HMAC requests              |

These are unset by default; the bot only needs them when `features.minecraft = true` in `config.toml` and at least one Minecraft sub-feature (`verify`, `donator_sync`, `chargeback`) is enabled. The companion plugin lives on the Minecraft server and exposes verification and donator-tier endpoints; both URL and secret are required for any of those calls to work.

See [Minecraft Verify](../features/minecraft-verify.md), [Minecraft Donator Sync](../features/minecraft-donator-sync.md), and [Minecraft Chargeback](../features/minecraft-chargeback.md).

## MCP server (optional)

The bot embeds a [Model Context Protocol](../features/mcp-server.md) server so external tools can drive Discord operations programmatically. These three variables control where it listens and how it authenticates.

| Name              | Required | Default     | Description                                       |
|-------------------|----------|-------------|---------------------------------------------------|
| `MCP_PORT`        | no       | `9090`      | TCP port the MCP server listens on                |
| `MCP_BIND_ADDR`   | no       | `127.0.0.1` | Bind address for the MCP server                   |
| `MCP_AUTH_TOKEN`  | no       | empty       | Required bearer token for MCP requests            |

### `MCP_PORT`

The port the in-process MCP server binds to. Must be a number; an unparseable value panics at startup. Default `9090` is fine for a single-instance setup. When running multiple instances on the same host you can either keep all internal ports the same and let Docker isolate them, or assign different host ports if you expose them.

### `MCP_BIND_ADDR`

The address to bind to. Default `127.0.0.1` keeps the MCP server reachable only from inside the container. Set it to `0.0.0.0` to listen on all interfaces — for example, when the [MCP gateway](../architecture/mcp-gateway-routing.md) needs to reach this instance over the Docker network. Pair any external exposure with `MCP_AUTH_TOKEN`. See [MCP Exposure](../deployment/mcp-exposure.md) for the threat model.

### `MCP_AUTH_TOKEN`

Bearer token required on all MCP requests. The default is an empty string, which disables auth entirely. **Always set this to a long random value when binding to anything other than `127.0.0.1`.** A leaked or empty token on a public address gives anyone full programmatic control of the bot.

## MCP gateway (optional)

| Name                     | Required        | Default | Description                                     |
|--------------------------|-----------------|---------|-------------------------------------------------|
| `MCP_GATEWAY_AUTH_TOKEN` | gateway service | unset   | Bearer token clients use to talk to the gateway |

This variable is read by the separate `mcp-gateway` service in `docker-compose.yml`, not by the bot binary itself. It is the token that external MCP clients (Claude Code, etc.) present when calling the gateway, which then proxies the request to the appropriate per-instance MCP server. See [MCP Gateway Routing](../architecture/mcp-gateway-routing.md).

## A note on placeholder detection

`Config::load()` rejects any required variable whose value still starts with the literal string `your-` — for example, `DISCORD_TOKEN=your-discord-bot-token`. This catches the easy mistake of copying `.env.example` to `.env` and forgetting to actually fill it in. If you see `<KEY> has placeholder value — set it in .env` at startup, that's the check firing.
