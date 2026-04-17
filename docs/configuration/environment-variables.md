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

| Name               | Required | Default | Description                                            |
|--------------------|----------|---------|--------------------------------------------------------|
| `DEEPSEEK_API_KEY` | no       | unset   | API key for DeepSeek chat completions                  |
| `GEMINI_API_KEY`   | no       | unset   | API key for Google Gemini chat completions             |
| `GROK_API_KEY`     | no       | unset   | API key for xAI Grok (used as a CENSORED-cascade alt)  |

All three are independently optional. If `DEEPSEEK_API_KEY` and `GEMINI_API_KEY` are both unset the AI chat feature is disabled — mention the bot and you'll get nothing back. If only one is set it is used. If both are set the bot uses DeepSeek as primary text and Gemini for image vision. `GROK_API_KEY` is only used when an instance opts into the CENSORED cascade via `[ai.fallback]` in its `config.toml`. Empty strings (`DEEPSEEK_API_KEY=`) are normalized to "unset."

**Custom providers may name any env var.** The `api_key_env` field of an `[ai.providers.<name>]` block in instance `config.toml` names the env var the bot will read for that provider's bearer token. The default-registry providers' `api_key_env` values map to the env vars listed above. See [AI Providers](ai-providers.md) for the schema and worked examples (custom env-var names, multiple providers, etc.).

### `DEEPSEEK_API_KEY`

Get one at [platform.deepseek.com](https://platform.deepseek.com/). DeepSeek's chat models are the cheapest of the supported providers and are recommended as the primary. See the [AI Chat](../features/ai-chat.md) feature page for model selection details.

### `GEMINI_API_KEY`

Get one in Google AI Studio. Used as the vision provider when image attachments are present in the prompt or replied-to message. Also valid as a CENSORED-cascade fallback if listed in `[ai.fallback]`.

### `GROK_API_KEY`

Get one at [console.x.ai](https://console.x.ai/). Optional; needed only if an instance lists `"grok"` in its `[ai.fallback] on_censored` config. Grok is a less-restrictive alternative the bot can replay a refused conversation through when DeepSeek hits its content-moderation block. See the [AI Chat](../features/ai-chat.md) feature page for the cascade story.

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

| Name              | Required              | Default     | Description                                       |
|-------------------|-----------------------|-------------|---------------------------------------------------|
| `MCP_PORT`        | no                    | `9090`      | TCP port the MCP server listens on                |
| `MCP_BIND_ADDR`   | no                    | `127.0.0.1` | Bind address for the MCP server                   |
| `MCP_AUTH_TOKEN`  | when bind is non-loopback | empty   | Bearer token for MCP requests; hard-required on any non-loopback bind |

### `MCP_PORT`

The port the in-process MCP server binds to. Must be a number; an unparseable value panics at startup. Default `9090` is fine for a single-instance setup. When running multiple instances on the same host you can either keep all internal ports the same and let Docker isolate them, or assign different host ports if you expose them.

### `MCP_BIND_ADDR`

The address to bind to. Two defaults are in play here, and the difference matters:

- **`Config::load()` fallback (no value set anywhere):** `127.0.0.1`. Loopback only.
- **Shipped `instances/example/.env.example`:** `0.0.0.0`. The bundled Compose stack has the `mcp-gateway` sidecar reach the bot over the Docker bridge network at `http://bot:9090`, which requires the bot to bind on a non-loopback interface inside its own container.

Pick based on shape:

- **Single-host, no gateway, you only want loopback access:** set `MCP_BIND_ADDR=127.0.0.1` (or simply unset it and let the fallback apply).
- **Bundled Docker Compose with the gateway sidecar:** keep `MCP_BIND_ADDR=0.0.0.0` from the example. Each bot lives in its own container, so "all interfaces" means "the container's interface on the Compose bridge network" — not the host's public IP. Pair this with `MCP_AUTH_TOKEN`; the bot now refuses to start without one (see below).

See [MCP Exposure](../deployment/mcp-exposure.md) for the threat model and the deploy shapes in detail.

### `MCP_AUTH_TOKEN`

Bearer token required on all MCP requests. The default is an empty string, which disables auth entirely.

**This is now a hard startup requirement when `MCP_BIND_ADDR` is anything other than a loopback address.** If `MCP_AUTH_TOKEN` is empty *and* the bind address is non-loopback, the bot logs an error and refuses to start. The check is in `src/mcp/mod.rs`. The intent is to prevent the easy mistake of binding `0.0.0.0` for a Compose deploy and forgetting to set a token — which would otherwise hand programmatic control of the bot to anyone who could reach the port.

Loopback binds with no token are still allowed (and are the right answer for a single-host bot with no gateway). Comparison against the configured token is constant-time (via the `subtle` crate) so the auth path doesn't leak token contents through timing.

See [MCP Exposure](../deployment/mcp-exposure.md) for the full security model and the two deploy shapes.

## MCP gateway (optional)

| Name                     | Required        | Default | Description                                     |
|--------------------------|-----------------|---------|-------------------------------------------------|
| `MCP_GATEWAY_AUTH_TOKEN` | gateway service | unset   | Bearer token clients use to talk to the gateway |

This variable is read by the separate `mcp-gateway` service in `docker-compose.yml`, not by the bot binary itself. It is the token that external MCP clients (Claude Code, etc.) present when calling the gateway, which then proxies the request to the appropriate per-instance MCP server. See [MCP Gateway Routing](../architecture/mcp-gateway-routing.md).

**The gateway always binds non-loopback and treats this token as mandatory.** If `MCP_GATEWAY_AUTH_TOKEN` is empty the gateway refuses to start at all — there is no loopback escape hatch like the bot has, because the gateway's whole job is to be reachable from outside its container.

**The same value must be set as each bot's `MCP_AUTH_TOKEN`.** The gateway uses `MCP_GATEWAY_AUTH_TOKEN` for *both* inbound auth (checking client bearers) and outbound auth (as the `Authorization: Bearer` header it forwards to every backend bot). A bot with a different `MCP_AUTH_TOKEN` will 401 the gateway at startup. Generate one secret with `openssl rand -hex 32` and use it in both the gateway environment and every bot's `.env`.

## A note on placeholder detection

`Config::load()` rejects any required variable whose value still starts with the literal string `your-` — for example, `DISCORD_TOKEN=your-discord-bot-token`. This catches the easy mistake of copying `.env.example` to `.env` and forgetting to actually fill it in. If you see `<KEY> has placeholder value — set it in .env` at startup, that's the check firing.
