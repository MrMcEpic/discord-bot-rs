# Monitoring

The bot is small and quiet. There is no metrics endpoint, no
Prometheus exporter, and no built-in alerting. What it gives you
is structured logs, container health checks, and a database whose
state you can query directly. This page is about how to make those
things into a passable monitoring story.

## Health checks

The Compose stack defines health checks on two services:

**Postgres** runs `pg_isready` every 5 seconds. The bot's
`depends_on: postgres: condition: service_healthy` clause uses this
so the bot does not start until the database is accepting
connections.

**Bot** runs `curl` against its own embedded MCP server every 10
seconds:

```yaml
healthcheck:
  test: ["CMD-SHELL", "curl -s -o /dev/null --connect-timeout 2 http://localhost:9090/mcp"]
  interval: 10s
  timeout: 5s
  retries: 12
```

This is a liveness check. The MCP server starts as a side effect of
the bot reaching its run loop, so if the check is passing, the bot
process is alive, has loaded its config, has connected to Postgres,
and is past startup. If the check is failing for 12 consecutive
intervals (2 minutes), Compose marks the container `unhealthy` and
the gateway's `depends_on` clause stops it from being considered
ready.

The bot health check is also what the gateway depends on for its
own startup ordering. A failed bot health check means the gateway
will not (re-)route to that backend until it recovers.

There is no health check on `mcp-gateway` itself. It is stateless
and loud — if it is down, every MCP call fails immediately and
that is the signal.

The minimum viable monitoring is therefore `docker compose ps`:

```text
NAME                       STATUS
discord-bot-rs-bot-1       Up 3 hours (healthy)
discord-bot-rs-postgres-1  Up 3 hours (healthy)
discord-bot-rs-mcp-gw-1    Up 3 hours
```

If `bot` or `postgres` shows `unhealthy`, something is broken. If
the gateway shows as `Restarting`, the bot is unhealthy and the
gateway crashed waiting for it.

For automated alerting, run `docker compose ps --format json` from
cron or a small script and alert when any service is anything
other than `running` and (where applicable) `healthy`.

## Logs

The bot uses [`tracing`](https://crates.io/crates/tracing) with the
default `tracing_subscriber::fmt::init()` in `main.rs`. Output goes
to stderr, which Docker captures into the container log stream.

Common operational queries:

```bash
# Tail everything across the stack
docker compose logs -f

# Just the bot
docker compose logs -f bot

# The last 200 lines, then exit
docker compose logs --tail 200 bot

# Filter to errors and warnings
docker compose logs bot 2>&1 | grep -E ' (ERROR|WARN) '

# Logs from a specific time window
docker compose logs --since 1h --until 30m bot
```

### Log levels

The default is `INFO`. Override with `RUST_LOG`:

```bash
# Set in the bot's .env
RUST_LOG=debug
```

`RUST_LOG=debug` is loud — useful when investigating a specific
incident, painful to leave on long-term. Per-module filters help:

```bash
RUST_LOG=info,discord_bot::music=debug,discord_bot::mcp=debug
```

This keeps everything else at INFO and only debugs music and MCP.
The module names follow the source tree
(`src/music/`, `src/mcp/`, etc.).

### Log lines worth knowing

A few lines you will see often, with what they mean:

- `Database initialized (schema: <name>)` — pool is up, migrations
  done. If you do not see this within a few seconds of boot, the
  database connection is broken.
- `Instance config loaded: <name> (prefix: ...)` — `config.toml`
  parsed without errors.
- `<botname> is connected!` — Discord gateway is up. The bot is
  fully operational from this point.
- `MCP server listening on 0.0.0.0:9090` — embedded MCP server
  started.
- `Tempban unban checker started (30s interval).` — background
  worker spawned.
- `Auto-role time checker started (60s interval).` — auto-role
  background worker spawned (only if enabled).
- `Donator sync checker started (<N>s interval).` — Minecraft
  donator sync started (only if enabled).

WARN-level lines worth paying attention to:

- `<feature> enabled but [<section>] config section missing` —
  a feature flag is on but its config section is absent. The
  feature is silently disabled until you fix the config.
- `Welcome feature enabled but no AI API key (DEEPSEEK_API_KEY or
  GEMINI_API_KEY) configured` — welcome messages need an AI
  provider; one is missing.
- `Donator sync: failed to fetch donators` — the Minecraft
  companion plugin is unreachable. Often transient (network blip,
  MC server restart); persistent failures mean MC_VERIFY_URL or
  MC_VERIFY_SECRET is wrong.
- `Auto-role time promotion failed for <user>` — Discord rejected
  a role grant. Usually a permissions issue; the bot's role needs
  to be above the role it is granting.

ERROR-level lines should always be investigated:

- `Command error: ...` — a user-facing command threw. The user
  also got an `Error: ...` message in Discord. Often this is
  user input the command cannot handle (bad time format, missing
  permission), occasionally it is a bug.
- `Framework error: ...` — poise reported a framework-level
  problem.
- `Client error: ...` printed at the very bottom of the log right
  before the bot exits — Serenity has lost the connection and
  cannot recover. Compose's `restart: unless-stopped` will bring
  the container back, but a recurring crash is worth digging into.

## Log aggregation

For a single host running a single bot, `docker compose logs` and
`grep` is sufficient. As soon as you have multiple hosts or
multiple instances, you want logs in a central place.

The simplest option is to point the Docker daemon at a syslog
endpoint, journald, or a log driver of your choice:

```yaml
# In the bot service
logging:
  driver: journald
  options:
    tag: "discord-bot"
```

journald gives you `journalctl -u discord-bot -f` and rotation for
free. Other drivers (`gelf`, `awslogs`, `loki`, `fluentd`, etc.)
are wired the same way — see the
[Docker logging docs](https://docs.docker.com/config/containers/logging/configure/).

For a structured-log workflow, consider Loki + Grafana: it
ingests the raw JSON-flavoured tracing output cleanly and lets you
build dashboards on log fields (per-guild error rates, music
command counts, etc.). The bot itself does not export metrics, so
Loki + log-derived metrics is the path to graphs.

## Common failure modes

### The bot is offline and the container is restarting

Check `docker compose logs bot --tail 100`. The most common causes:

- A required env var is missing or has a placeholder. The bot
  panics at startup with `<KEY> must be set in .env` or `<KEY>
  has placeholder value`.
- The Discord token is invalid. You will see a Serenity error
  about authentication shortly after `Starting bot...`.
- Postgres is down. The pool fails to initialise and the bot
  panics with `Failed to connect to database`.

### The bot is online but does not respond to commands

- **Wrong prefix.** Check `command_prefix` in `config.toml`
  matches what you are typing.
- **Missing permissions.** The bot needs Read Messages, Send
  Messages, and Read Message History in the channel.
- **Missing intents.** Discord requires you to enable Message
  Content Intent in the developer portal for the bot to read
  message text. Without it, prefix commands silently do nothing.
- **The bot crashed mid-handler.** Look for `Command error:` in
  the logs.

### Music does not play

- Check `docker compose logs bot | grep -E '(yt-dlp|ffmpeg|node)'`.
  A broken yt-dlp or missing Node.js (it is needed for some JS
  challenges) will show up here.
- If yt-dlp is failing on YouTube specifically, the bot may need
  cookies. See [Music feature page](../features/music.md).
- Voice-stack errors mention `songbird` or `opus` — typically a
  rare dependency mismatch in a custom build.

### MCP calls fail

- `docker compose logs mcp-gateway` first. If the gateway is up
  but the bot's MCP server is not responding, you will see
  health-check warnings.
- 401 Unauthorized responses mean the bearer token is wrong or
  missing.
- `InstanceNotFound` or `GuildNotFound` means the gateway's
  routing table cannot resolve the request — see
  [MCP Gateway Routing](../architecture/mcp-gateway-routing.md).

### Donator sync stops working

Most often the MC companion plugin is unreachable or its endpoint
returns a non-200. The bot logs `Donator sync error:` and the next
poll retries — there is no escalation.

### Auto-role does not promote

The auto-role worker logs `Auto-role time promotion failed` for
each failed grant. The bot needs its role to be above the role it
is granting in the Discord role hierarchy. Re-order roles in the
Discord server settings and the next sweep will succeed.

## Database introspection

Sometimes the fastest debugging is a `psql` session:

```bash
docker compose exec postgres psql -U discord_bot discord_bot
```

Useful queries:

```sql
-- Active tempbans across instances
SELECT * FROM "<schema>".tempbans WHERE unbanned = FALSE ORDER BY expires_at;

-- Top message-senders for the auto-role feature
SELECT * FROM "<schema>".member_activity ORDER BY message_count DESC LIMIT 20;

-- Recent stock trades
SELECT * FROM "<schema>".stock_transactions ORDER BY created_at DESC LIMIT 20;

-- Per-guild settings
SELECT * FROM "<schema>".guild_settings;
```

Replace `<schema>` with each instance's `DB_SCHEMA`. The
[Database Schema](../architecture/database-schema.md) page lists
every table.

## What is intentionally not monitored

A few things the bot does not track and you should not try to:

- **Per-command latency.** The Discord gateway is the rate
  limiter; latency is dominated by Discord's response time, not
  the bot's.
- **In-memory queues and caches.** Music queues, game state, rate
  limiters all reset on restart by design — they are not state
  worth watching.
- **The MCP gateway's per-request status.** It is a thin proxy;
  failures in it are visible as log lines.

## Cross-references

- [Docker Compose](docker-compose.md) — service definitions and
  health-check syntax.
- [Database Schema](../architecture/database-schema.md) — what
  lives in each table.
- [Production Checklist](production-checklist.md) — sets up the
  monitoring you actually need before going live.
- [Upgrading](upgrading.md) — log lines to watch for after a
  version change.
