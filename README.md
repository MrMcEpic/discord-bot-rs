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
