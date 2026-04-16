# Deployment Overview

This section is the operations manual for `discord-bot-rs`. It covers
how to get a bot running on a real server, how to add a second one
later, how to back up the database, when to expose the MCP port, and
how to keep the whole thing alive long-term.

If you have not got a bot up locally yet, start with
[Quickstart](../getting-started/quickstart.md) and come back here once
you are ready to put something on a host that lives longer than your
laptop.

## What ships in the box

The repo includes everything you need to deploy a single instance:

- A multi-stage [`Dockerfile`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/Dockerfile)
  that builds the bot binary on `rust:bookworm` and ships a
  `debian:bookworm-slim` runtime image with `ffmpeg`, `yt-dlp`,
  Node.js (for `yt-dlp`'s JS challenges), and the Opus / libsodium
  shared libraries the voice stack needs.
- A separate [`mcp-gateway/Dockerfile`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/mcp-gateway/Dockerfile)
  for the gateway service.
- A top-level [`docker-compose.yml`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/docker-compose.yml)
  that wires the bot, a `postgres:17` container, and the gateway
  together with health checks and named volumes.
- An `instances/example/` directory used as a fully-documented
  reference for `config.toml`, `.env.example`, and `personality.txt`.

There are also pre-built images on GitHub Container Registry —
`ghcr.io/mrmcepic/discord-bot-rs:0.5.0` and `:latest`, plus
`ghcr.io/mrmcepic/discord-bot-rs-mcp-gateway` — for hosts where you
do not want to build from source. They are amd64-only at the moment.

## Recommended path: Docker Compose

[Docker Compose](docker-compose.md) is the path the repo is designed
around and the path most operators should use. It gets you the bot,
Postgres, and the MCP gateway with one command, with sensible
defaults for restart policy, health checks, persistent volumes, and
network isolation. Almost every other page in this section assumes
you are running under Compose.

The defining choice in the Compose file is that the `bot` service is
**generic** — it points at a configurable `INSTANCE_DIR`. The
default is `./instances/example`, but you select your own with:

```bash
INSTANCE_DIR=./instances/mybot docker compose up -d
```

That single switch lets the same Compose file run any instance you
have configured under `instances/`. To run more than one bot at a
time you copy the `bot` block in the Compose file, give it a unique
service name, and point it at a different directory — see
[Multi-Instance Deployment](multi-instance-deployment.md) for the
recipe.

## Other deployment shapes

You are not locked into Compose. The bot binary and the gateway are
both standalone executables, and you can run them however your
infrastructure prefers:

- **Plain Docker** — `docker run` the published images directly,
  bring your own Postgres, manage networking yourself.
- **Kubernetes** — wrap the same images in a Deployment and a
  StatefulSet for Postgres. There is no Helm chart in-tree, but the
  shape is straightforward enough that you can write one in an hour.
- **Bare metal** — `cargo build --release`, install `ffmpeg`,
  `yt-dlp`, `libopus`, `libsodium`, and Node.js, run the binary as a
  systemd unit, point it at a system Postgres. The build
  dependencies are listed in the Dockerfile.

The rest of this section is written against Compose because that is
where the hardening, health-check, and upgrade workflows are best
defined. If you are running under one of the alternatives, the
configuration knobs and operational concerns are the same — only the
mechanics of "restart this container" change.

## Pages in this section

| Page                                                     | When to read                                                          |
|----------------------------------------------------------|-----------------------------------------------------------------------|
| [Docker Compose](docker-compose.md)                      | Setting up your first deployment, or whenever you change the stack.   |
| [PostgreSQL Setup](postgres-setup.md)                    | Choosing bundled vs external Postgres, planning backups, migrations.  |
| [Multi-Instance Deployment](multi-instance-deployment.md) | Adding a second bot to an existing host.                              |
| [MCP Exposure](mcp-exposure.md)                          | Connecting an MCP client from outside the host.                       |
| [Upgrading](upgrading.md)                                | Pulling a new version, planning around breaking changes.              |
| [Monitoring](monitoring.md)                              | Health checks, log aggregation, what failure looks like.              |
| [Production Checklist](production-checklist.md)          | One-pass hardening sweep before you stop watching the logs.           |

If something on a page surprises you, the architecture pages —
especially [Multi-Instance Model](../architecture/multi-instance-model.md)
and [MCP Gateway Routing](../architecture/mcp-gateway-routing.md) —
explain why the deployment shape looks the way it does.
