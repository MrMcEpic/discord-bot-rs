# Docker Compose Deployment

Docker Compose is the default deployment path. The repo ships a
top-level [`docker-compose.yml`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/docker-compose.yml)
that brings up the bot, a `postgres:17` database, and the MCP gateway
as a single coordinated stack. This page covers everything in that
file: what each service does, what the environment variables mean,
how the health checks compose, how to use a custom instance
directory, what volumes persist, and what to look at first when
something is broken.

If you have not run the stack at all yet, work through
[Quickstart](../getting-started/quickstart.md) first. This page
assumes you have done that and want a deeper look at the moving
parts.

## The three services

```yaml
services:
  postgres:    # PostgreSQL 17, bundled
  bot:         # the discord-bot binary
  mcp-gateway: # routes MCP requests to one or more bots
volumes:
  pgdata:      # persistent storage for postgres
```

`postgres` and `bot` are both required for a working deployment.
`mcp-gateway` is only needed if you want an MCP client (Claude Code,
Cursor, etc.) to be able to drive the bot programmatically — but
since the gateway is harmless when nobody connects to it, it is
included by default and you can ignore it until you need it.

## The `postgres` service

```yaml
postgres:
  image: postgres:17
  restart: unless-stopped
  environment:
    POSTGRES_USER: discord_bot
    POSTGRES_PASSWORD: discord_bot_pass
    POSTGRES_DB: discord_bot
  volumes:
    - pgdata:/var/lib/postgresql/data
  healthcheck:
    test: ["CMD-SHELL", "pg_isready -U discord_bot"]
    interval: 5s
    timeout: 5s
    retries: 5
```

The official `postgres:17` image, started with the `discord_bot` user
and `discord_bot_pass` password, owning a `discord_bot` database. The
data lives in a named Docker volume called `pgdata`, which means
`docker compose down` does **not** wipe the database — only
`docker compose down -v` (or an explicit `docker volume rm`) does.

The health check uses `pg_isready` so the bot's `depends_on:
condition: service_healthy` clause actually waits for Postgres to be
accepting connections, not just for the container to be running.

`restart: unless-stopped` means the container comes back after
reboots and after crashes, but not after you have explicitly stopped
it with `docker compose stop`.

Whether you should keep the bundled Postgres or point at an external
one is covered in [PostgreSQL Setup](postgres-setup.md). Short
version: bundled is fine for a single host running a handful of
bots; switch to external when you have other apps that need the
same database server, or when you want managed backups.

## The `bot` service

```yaml
bot:
  build:
    context: .
    dockerfile: Dockerfile
  restart: unless-stopped
  env_file: ${INSTANCE_DIR:-./instances/example}/.env
  environment:
    CONFIG_DIR: /config
  volumes:
    - ${INSTANCE_DIR:-./instances/example}:/config
  tmpfs:
    - /tmp:size=500M
  depends_on:
    postgres:
      condition: service_healthy
  healthcheck:
    test: ["CMD-SHELL", "curl -s -o /dev/null --connect-timeout 2 http://localhost:9090/mcp"]
    interval: 10s
    timeout: 5s
    retries: 12
```

The interesting parts:

**`INSTANCE_DIR` is the only deploy-time switch you need.** Both
`env_file` and `volumes` interpolate `${INSTANCE_DIR:-./instances/example}`,
so a single environment variable on the host shell selects which
instance directory feeds this container. The whole point of the
generic `bot` service is that you can run any instance configured
under `instances/` without editing the Compose file:

```bash
INSTANCE_DIR=./instances/mybot docker compose up -d
```

If `INSTANCE_DIR` is unset, the default points at the
fully-documented `instances/example` directory, which is intended as
a reference rather than a real bot but will boot if you fill in its
`.env`.

**`/config` is where the bot reads its configuration.** Inside the
container, `CONFIG_DIR=/config` and the instance directory is
mounted at `/config`, so the bot finds `config.toml`,
`personality.txt`, the optional welcome prompt, and any
`cookies.txt` for music there. The `.env` is loaded separately by
Compose's `env_file` directive, not by the bot reading
`/config/.env` — that is why the env file path and the volume mount
both reference the same `INSTANCE_DIR`.

**`/tmp` is a 500 MB tmpfs.** `yt-dlp` and `ffmpeg` write transient
files here during music playback. Putting `/tmp` on tmpfs avoids
hammering the host disk and ensures it is wiped on container
restart.

**The bot waits for Postgres.** `depends_on: condition:
service_healthy` is the strict version — the bot container will not
start its main process until the Postgres health check is passing.
This avoids the otherwise-common race where the bot tries to open a
connection before Postgres is accepting them and crashes.

**The health check hits the embedded MCP server.** The bot's MCP
server starts on port 9090 inside the container as a side effect of
the bot reaching its run loop. If the health check fails, the bot is
either not started yet, deadlocked, or has the MCP feature disabled
on a non-default port. The health check is also what `mcp-gateway`
waits for before it starts.

The Dockerfile itself is multi-stage: a `rust:bookworm` builder
compiles a release binary, and the runtime image is
`debian:bookworm-slim` with `ffmpeg`, `yt-dlp`, Node.js (for
`yt-dlp`'s JS challenge solving), and the runtime libraries the
voice stack needs (`libopus`, `libsodium`, `libssl3`).

## The `mcp-gateway` service

```yaml
mcp-gateway:
  build:
    context: ./mcp-gateway
    dockerfile: Dockerfile
  restart: unless-stopped
  ports:
    - "127.0.0.1:9100:9100"
  environment:
    GATEWAY_PORT: "9100"
    INSTANCES: "${INSTANCES:-bot=http://bot:9090}"
    MCP_AUTH_TOKEN: "${MCP_GATEWAY_AUTH_TOKEN:-}"
    RUST_LOG: "info"
  depends_on:
    bot:
      condition: service_healthy
```

The gateway is a tiny axum server that fronts every bot's embedded
MCP endpoint and presents a single tool catalog to clients. The full
design is in [MCP Gateway Routing](../architecture/mcp-gateway-routing.md);
operationally, the things that matter:

**It binds to `127.0.0.1:9100` on the host.** The gateway port is
deliberately localhost-only by default. To make it reachable from
outside the host, see [MCP Exposure](mcp-exposure.md) — the safe
patterns are SSH tunnels, WireGuard / Tailscale, or a TLS-terminating
reverse proxy.

**`INSTANCES` is the routing table.** The format is comma-separated
`name=url` pairs. The default is `bot=http://bot:9090` — one
backend, called `bot`, reached over the internal Compose network.
For multiple bots you override it on the host shell:

```bash
INSTANCES="bot1=http://bot1:9090,bot2=http://bot2:9090" docker compose up -d
```

The names here are also the routing keys clients use when calling
tools that need to specify which bot to act on. See
[Multi-Instance Deployment](multi-instance-deployment.md) for the
end-to-end pattern.

**`MCP_GATEWAY_AUTH_TOKEN` is the shared secret for the whole MCP
fabric.** The gateway refuses to start if it is empty — there is no
loopback escape hatch, because the gateway's whole job is to be
reachable from outside its own container. The same value is:

- checked against the `Authorization: Bearer <token>` header on
  every inbound request from an MCP client, and
- forwarded as `Authorization: Bearer <token>` on every outbound
  request from the gateway to a backend bot.

For that to work, each bot's `MCP_AUTH_TOKEN` must be set to the
same value as `MCP_GATEWAY_AUTH_TOKEN`. A mismatch shows up as the
gateway logging `401 Unauthorized` from the backend at startup.
Generate one secret with `openssl rand -hex 32` and use it in both
places.

**It depends on the bot's health check.** `depends_on: condition:
service_healthy` ensures the gateway never starts before there is at
least one backend to talk to. With multiple bot services you would
list each in the `depends_on` block — the gateway will fail to
register guilds for instances that are not up.

## Common operations

```bash
# Start everything in the background
docker compose up -d

# Start with a specific instance directory
INSTANCE_DIR=./instances/mybot docker compose up -d

# Tail the bot logs
docker compose logs -f bot

# Tail everything
docker compose logs -f

# Restart just the bot (after editing config.toml or .env)
docker compose restart bot

# Stop everything but keep data
docker compose down

# Stop and wipe the database (destructive)
docker compose down -v

# Pull a new bot image and re-create the container
docker compose pull bot && docker compose up -d bot

# Force a rebuild after editing the source
docker compose build bot && docker compose up -d bot

# See container health
docker compose ps
```

Restarting the bot is cheap (a few seconds) and safe — the database
holds all persistent state, and in-memory state like music queues
and rate-limit counters is intentionally disposable. See
[Database Schema: What's not stored](../architecture/database-schema.md#whats-not-stored)
if you are curious about that boundary.

## Networking

Compose creates a default bridge network for the project. Inside it,
services reach each other by service name: the bot connects to
Postgres at `postgres:5432`, the gateway connects to bots at
`http://bot:9090` (or `http://bot1:9090`, `http://bot2:9090` in a
multi-instance setup). None of these names exist on the host's DNS;
they only resolve inside the Compose network.

The only port published to the host is the gateway's
`127.0.0.1:9100`. Postgres and the bots are network-isolated by
default. If you want host access to Postgres for backups or psql,
add a `ports: ["127.0.0.1:5432:5432"]` block to the `postgres`
service, but think twice before binding it to `0.0.0.0`.

## Volumes and persistence

There is one named volume: `pgdata`. Everything the bot considers
worth keeping across restarts goes through Postgres: tempbans,
guild settings, stock portfolios, member-activity counters. See
[Database Schema](../architecture/database-schema.md) for the full
list.

Things that are **not** persisted: music queues, active games,
rate-limit counters, idle timers. These live on `Data` in the bot's
process and reset on every restart. That is by design — see
[Database Schema: What's not stored](../architecture/database-schema.md#whats-not-stored).

The instance directory itself (`instances/mybot/`) is bind-mounted
read-only-ish from the host. Anything you change in `config.toml`
or `personality.txt` takes effect after `docker compose restart bot`.
Configuration is not hot-reloaded.

## Resource limits

The Compose file does not set explicit CPU or memory limits. A
single bot under normal load uses 50–150 MB of RAM and very little
CPU outside of music transcoding, which is bursty. If you want to
cap things, add a `deploy.resources` block per service — start with
512 MB for the bot and 256 MB for Postgres on a small VPS, raise
either if you see OOMs in `docker compose ps`.

## Troubleshooting

**`bot` exits immediately with `<KEY> must be set in .env`.** A
required environment variable is missing. The bot validates
`DISCORD_TOKEN`, `CLIENT_ID`, and `GUILD_ID` at startup and panics
if any is empty or still has a `your-...` placeholder. Open
`instances/<name>/.env` and fill in the values. See
[Environment Variables](../configuration/environment-variables.md)
for the full required list.

**`bot` reports `Failed to connect to database` and restarts.**
Postgres is not yet healthy — usually a transient race. With
`depends_on: service_healthy` this should not happen on a clean
boot, but if it does, check `docker compose logs postgres` for disk
space, volume permissions, or a corrupted data directory.

**`mcp-gateway` logs `InstanceNotFound: bot`.** The `INSTANCES`
variable is wrong. The default points at a service named `bot`; if
you renamed it for a multi-instance setup but did not update
`INSTANCES`, the gateway has no backends to talk to.

**Health check is stuck on `unhealthy` for `bot`.** The MCP server
inside the container is not responding on port 9090. Either the bot
process has not finished starting (give it 30 seconds), it is
listening on a different `MCP_PORT`, or it has crashed. `docker
compose logs bot` will tell you which.

**Music playback fails with `node` errors.** The runtime image
includes Node.js because `yt-dlp` shells out to it for JavaScript
challenges. If you see `node: command not found`, you are running
an old image — pull or rebuild.

## Cross-references

- [Environment Variables](../configuration/environment-variables.md) —
  every variable the bot reads from `.env`.
- [Instance Config (config.toml)](../configuration/instance-config.md) —
  what lives in the per-instance config file.
- [PostgreSQL Setup](postgres-setup.md) — bundled vs external,
  backups.
- [Multi-Instance Deployment](multi-instance-deployment.md) — the
  copy-paste pattern for adding a second bot.
- [Monitoring](monitoring.md) — the health check, log aggregation,
  alerting.
