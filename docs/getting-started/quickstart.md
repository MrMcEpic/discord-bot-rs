# Quickstart

This page is the fast path: about ten minutes from `git clone` to a running
bot, assuming you have already gathered everything from
[Prerequisites](prerequisites.md). If you have not, do that first. If you
want a slower walkthrough with more context, see
[First Bot Tutorial](first-bot-tutorial.md).

By the end of this page you will have a bot online in your Discord server,
running locally in Docker, talking to a bundled PostgreSQL container.

## Step 1: Clone the repo

```bash
git clone https://github.com/MrMcEpic/discord-bot-rs.git
cd discord-bot-rs
```

The repo is small (a few hundred kilobytes) and has no submodules.

## Step 2: Create your instance directory

Each bot is configured by a directory under `instances/`. The `example`
directory is the canonical reference and is also the directory the bundled
`docker-compose.yml` points at by default. Copy it to a new name for your own
bot:

```bash
cp -r instances/example instances/mybot
cp instances/mybot/.env.example instances/mybot/.env
```

`mybot` is just a label. Use whatever name you like. The name does not have
to match the bot's display name in Discord.

## Step 3: Fill in `.env`

Open `instances/mybot/.env` in your editor of choice. The required fields
are at the top:

```bash
DISCORD_TOKEN=...     # from the Discord developer portal (Bot page)
CLIENT_ID=...         # the application ID from General Information
GUILD_ID=...          # right-click your server in Discord, Copy Server ID
```

Paste in the values you saved during Prerequisites.

The next block is the database. If you are using the bundled PostgreSQL, the
default works as-is:

```bash
DATABASE_URL=postgresql://discord_bot:discord_bot_pass@postgres:5432/discord_bot
DB_SCHEMA=mybot
```

Change `DB_SCHEMA` to match your instance name. Each instance gets its own
PostgreSQL schema, so you can run several bots side by side without their
data colliding.

If you want AI chat, paste in `DEEPSEEK_API_KEY` and/or `GEMINI_API_KEY`. If
you want stock trading, paste in `FINNHUB_API_KEY`. Anything you leave blank
just disables the corresponding feature.

One pair of values is mandatory for the bundled stack: `MCP_AUTH_TOKEN`
inside the instance `.env` (read by the bot container) and
`MCP_GATEWAY_AUTH_TOKEN` inside a `.env` at the repo root (read by Docker
Compose and forwarded to the `mcp-gateway` service). The bot and gateway
both refuse to start with an empty token on a non-loopback bind, and the
gateway uses its token as the bearer when reaching the bot, so the two
values must be **identical**. Generate one and install it in both places:

```bash
TOKEN=$(openssl rand -hex 32) && \
  sed -i.bak "s|^MCP_AUTH_TOKEN=.*|MCP_AUTH_TOKEN=$TOKEN|" instances/mybot/.env && \
  rm instances/mybot/.env.bak && \
  echo "MCP_GATEWAY_AUTH_TOKEN=$TOKEN" >> .env
```

See [MCP Exposure](../deployment/mcp-exposure.md) for the security model.

Save the file. It is gitignored, so it will not end up in any commit you
make.

## Step 4: Review `config.toml`

Open `instances/mybot/config.toml`. The fields you most likely want to
change are at the top:

```toml
bot_name = "My Bot"
command_prefix = "!"
personality_file = "personality.txt"
```

`bot_name` is what the bot calls itself in help text and welcome messages.
`command_prefix` is the prefix for text-based commands (the default `!` gives
you `!m help`, `!m play`, etc.). discord-bot-rs uses prefix commands only —
there are no slash commands.

Below that is a `[features]` block with feature flags:

```toml
[features]
minecraft = false
auto_role = false
join_role = false
welcome = false
```

Leave them all `false` for your first run. You can turn things on later once
the bot is up. See [Instance Config](../configuration/instance-config.md) for
what each flag does.

## Step 5: Tweak `personality.txt` (optional)

`instances/mybot/personality.txt` is the system prompt for AI chat. The
default is a starting template; you can customize tone and behavior here. If
you are not sure what to put, skip this for now and try the defaults first.
You can edit and restart the bot at any time.

## Step 6: Start the stack

From the repo root:

```bash
INSTANCE_DIR=./instances/mybot docker compose up -d
```

The first run takes a few minutes because Docker has to build the bot image.
Subsequent starts are seconds.

`INSTANCE_DIR` tells Compose which instance directory to mount into the
container. If you skip the variable, it defaults to `./instances/example`,
which is fine for trying things out but you will want it pointing at your
real instance long-term.

The stack starts three services:

- `postgres` — bundled PostgreSQL 17.
- `bot` — the Discord bot itself.
- `mcp-gateway` — a small HTTP router for the embedded MCP server (safe to
  ignore for now; see [MCP Server](../features/mcp-server.md) when you are
  curious).

## Step 7: Watch the logs

```bash
docker compose logs -f bot
```

You should see output that ends with something like:

```
INFO discord_bot::db: Database initialized (schema: example).
INFO discord_bot: Instance config loaded: Example Bot (prefix: !)
INFO discord_bot: Starting bot...
INFO discord_bot::events::ready: Example Bot is connected! (ID: ...)
INFO discord_bot::mcp: MCP server listening on 0.0.0.0:9090
```

Press `Ctrl+C` to stop tailing. The bot keeps running.

## Step 8: Test it in Discord

Open your Discord server. The bot should appear in the member list with a
green dot.

- Type `!m help` (or whatever prefix you set). You should see a list of
  commands. discord-bot-rs uses prefix commands only — there are no slash
  commands.
- @mention the bot in a channel. If you set an AI API key, it should reply.
  If you did not, the mention is ignored.

That is the minimum success criterion. For a more thorough check, run
through [Verifying Your Setup](verifying-your-setup.md).

## Troubleshooting

A few common failures and what to do about them.

**The bot never comes online (no green dot in the member list).**

The token is wrong, or the privileged intents are off. Check
`docker compose logs bot` for an error like "Invalid Token" or "Disallowed
intents." Go back to Prerequisites and double-check both the token and the
two privileged intents on the Bot page of the developer portal.

**Postgres reports unhealthy in `docker compose ps`.**

Run `docker compose logs postgres`. The most common cause is the disk being
full or the PostgreSQL data volume being corrupted from an interrupted
shutdown. `docker compose down` followed by `docker compose up -d` fixes most
transient issues. If the volume is genuinely broken, `docker volume rm
discord-bot-rs_pgdata` and start fresh (this destroys all bot data).

**`!m help` does not get a reply.**

Check that the bot has **Read Messages**, **Send Messages**, and **Read
Message History** permission in that channel. Some channels inherit
deny-by-default overrides that block bot replies. The bot needs
**Message Content Intent** enabled in the Discord developer portal to
read your prefix commands at all — if you forgot that during
prerequisites, re-enable it and restart the bot.

**The bot is online but @mentioning it does nothing.**

AI chat needs at least one of `DEEPSEEK_API_KEY` or `GEMINI_API_KEY` in
`.env`. If neither is set, AI chat is silently disabled. Set one, then
restart with `docker compose restart bot` and watch the logs for any API
errors on the next mention.

## Next steps

Once your bot is up and replying:

- [Verifying Your Setup](verifying-your-setup.md) — run through the
  post-deploy checklist.
- [Features](../features/index.md) — see what each feature does and how to
  configure it.
- [Production Checklist](../deployment/production-checklist.md) — when you
  are ready to leave it running long-term.
