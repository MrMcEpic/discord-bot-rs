# Discord Bot RS

A multi-instance Discord bot written in Rust. Supports AI chat (DeepSeek/Gemini), music playback, virtual stock trading, word games, moderation, and Minecraft account verification.

## Quick Start

```bash
git clone https://github.com/MrMcEpic/discord-bot-rs.git
cd discord-bot-rs

# Create your instance from the example
cp -r instances/example instances/mybot

# Configure your instance
cp instances/mybot/.env.example instances/mybot/.env
# Edit instances/mybot/.env with your Discord token and API keys
# Edit instances/mybot/config.toml for bot name, prefix, and features
# Edit instances/mybot/personality.txt for AI personality

# Update docker-compose.yml to point to your instance
# (replace "examplebot" service with your instance name and path)

# Start
docker compose up -d
```

## Instance Configuration

Each bot instance has its own config directory under `instances/`:

```
instances/mybot/
├── .env              # Secrets: Discord token, API keys, DB schema
├── config.toml       # Bot name, command prefix, feature toggles
└── personality.txt   # AI system prompt / personality
```

### .env

| Variable | Required | Description |
|----------|----------|-------------|
| `DISCORD_TOKEN` | Yes | Discord bot token |
| `CLIENT_ID` | Yes | Discord application client ID |
| `GUILD_ID` | Yes | Discord server ID |
| `DATABASE_URL` | Yes | PostgreSQL connection string |
| `DB_SCHEMA` | Yes | Postgres schema name (isolates data per instance) |
| `DEEPSEEK_API_KEY` | No | DeepSeek API key for AI chat |
| `GEMINI_API_KEY` | No | Google Gemini API key (fallback AI) |
| `FINNHUB_API_KEY` | No | Finnhub API key for stock trading |
| `MC_VERIFY_URL` | No | Minecraft verification plugin URL |
| `MC_VERIFY_SECRET` | No | Shared secret for MC verification |
| `MCP_PORT` | No | MCP server port (default: 9090) |
| `MCP_BIND_ADDR` | No | MCP server bind address (default: 127.0.0.1) |
| `MCP_AUTH_TOKEN` | No | Bearer token for MCP auth (empty = no auth) |

### config.toml

```toml
bot_name = "My Bot"
command_prefix = "!"
personality_file = "personality.txt"

[features]
minecraft = false
```

### personality.txt

Free-form text that defines the bot's AI personality. This becomes the system prompt for AI conversations. See `instances/example/personality.txt` for a starting template.

## MCP Server (Discord Management API)

The bot embeds an MCP (Model Context Protocol) server that exposes Discord server management tools. Claude Code or any MCP client can connect to it for programmatic server management.

### Available Tools (22)

| Category | Tools |
|----------|-------|
| **Guilds** | `list_guilds` |
| **Server** | `get_guild_info`, `send_message`, `delete_messages` |
| **Channels** | `list_channels`, `create_channel`, `delete_channel`, `edit_channel`, `move_channel`, `set_channel_permissions` |
| **Roles** | `list_roles`, `create_role`, `delete_role`, `edit_role` |
| **Members** | `list_members`, `get_member`, `assign_role`, `remove_role`, `ban_member`, `unban_member`, `kick_member`, `timeout_member` |

All tools accept an optional `guild_id` parameter to target any server the bot is in. If omitted, defaults to the configured `GUILD_ID`.

### Connecting Claude Code

Add to `~/.claude.json` (user scope) or project `.mcp.json`:

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

### Docker Port Exposure

To expose the MCP port, add to the service in `docker-compose.yml`:

```yaml
ports:
  - "127.0.0.1:9090:9090"  # localhost only
```

## Known Issues / TODO

- **AI context bleed**: The bot's message history builder can still mix context from concurrent conversations in the same channel. Currently mitigated with a 30-minute window and 10-message limit, but needs a proper conversation-thread-based approach.
- **MCP authentication**: Bearer token auth is implemented but Claude Code's HTTP transport expects OAuth. Currently running without auth (localhost-only). Needs OAuth 2.1 support or a workaround.

## Adding Another Instance

1. Create a new instance directory:
   ```bash
   cp -r instances/example instances/newbot
   ```
2. Configure `.env`, `config.toml`, and `personality.txt`
3. Add a service to `docker-compose.yml`:
   ```yaml
   newbot:
     build:
       context: .
       dockerfile: Dockerfile
     restart: unless-stopped
     env_file: instances/newbot/.env
     environment:
       CONFIG_DIR: /config
     volumes:
       - ./instances/newbot:/config:ro
     tmpfs:
       - /tmp:size=500M
     depends_on:
       postgres:
         condition: service_healthy
   ```
4. Start it:
   ```bash
   docker compose up -d newbot
   ```

Each instance auto-creates its database schema on first startup.

## Tech Stack

- **Rust** with [poise](https://github.com/serenity-rs/poise) / [serenity](https://github.com/serenity-rs/serenity)
- **PostgreSQL** via [sqlx](https://github.com/launchbadge/sqlx) (per-instance schema isolation)
- **Docker Compose** for deployment
- **DeepSeek V3.2 / Gemini** for AI chat
- **Songbird** + yt-dlp + ffmpeg for music
- **rmcp** + axum for MCP server (Discord management API)
