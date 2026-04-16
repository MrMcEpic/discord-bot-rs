# Production Checklist

A one-pass hardening sweep to do before you stop watching the logs.
Each item is a yes/no — if the answer is "no" or "not sure", read
the linked page and decide. If you can answer "yes" to every item,
the deployment is in reasonable shape.

The order is roughly secrets first, then network, then data, then
operations.

## Secrets

- [ ] **`DISCORD_TOKEN` is unique to this bot user and is not
      committed anywhere.** If it ever ended up in `git`, in a chat
      message, or in a screenshot, regenerate it in the Discord
      developer portal. Tokens are full credentials.
      → [Secrets Management](../configuration/secrets-management.md)

- [ ] **`.env` files are not in git.** The repo's `.gitignore`
      already excludes `instances/*/.env`. Confirm with `git
      status` after creating the file — it should not appear.

- [ ] **No required env var is using its placeholder value.** The
      bot rejects values starting with `your-` at startup, but the
      check is best-effort. Open each `instances/*/.env` and
      confirm.
      → [Environment Variables](../configuration/environment-variables.md)

- [ ] **API keys (`DEEPSEEK_API_KEY`, `GEMINI_API_KEY`,
      `FINNHUB_API_KEY`) are scoped to this deployment.** Do not
      reuse the same DeepSeek key across staging and production —
      separate billing and rate-limit blast radius.

- [ ] **`MCP_AUTH_TOKEN` is set on every bot whose
      `MCP_BIND_ADDR` is not loopback.** This is now enforced at
      startup — the bot refuses to boot if the bind is
      non-loopback and the token is empty. The bundled Compose
      `.env.example` ships with `MCP_BIND_ADDR=0.0.0.0` (so the
      gateway sidecar can reach it), so a Compose deploy without
      a token will fail to start.
      → [MCP Exposure](mcp-exposure.md)

- [ ] **`MCP_GATEWAY_AUTH_TOKEN` is set on the gateway service.**
      The gateway refuses to start at all without it — there is
      no loopback escape hatch. Generate with `openssl rand -hex
      32`.
      → [MCP Exposure](mcp-exposure.md)

- [ ] **The Postgres password is not the default `discord_bot_pass`
      if Postgres is exposed beyond the Compose network.** On the
      default localhost-only setup, the default is fine. If you
      bind Postgres to a host port or use external Postgres,
      rotate it.

- [ ] **`MC_VERIFY_SECRET` matches the value configured on the
      Minecraft companion plugin.** A mismatch makes verification
      and donator sync silently fail.

## Discord configuration

- [ ] **The bot's role permissions are minimum-necessary.** Audit
      the role's permissions in the Discord server settings.
      Administrator is rarely required and turns the MCP endpoint
      into an "anything-goes" interface. Grant only the
      permissions the features you have enabled actually need.

- [ ] **The bot's role is positioned correctly in the role
      hierarchy.** It must be above any role it needs to assign,
      remove, or modify (auto-role, join role, donator sync). Drag
      it up if necessary.

- [ ] **Privileged intents are enabled in the Discord developer
      portal.** Specifically, *Server Members Intent* and *Message
      Content Intent*. Without them the bot cannot read prefix
      commands or react to member joins.

- [ ] **The bot is in every guild whose `GUILD_ID` you have
      configured.** A `GUILD_ID` for a guild the bot is not in
      causes silent feature failure.

## Network

- [ ] **The MCP gateway is bound to `127.0.0.1:9100` on the host,
      not `0.0.0.0`.** The default Compose file is correct; only
      change it if you have read [MCP Exposure](mcp-exposure.md)
      and are using one of the safe patterns.

- [ ] **The Postgres port is not published unless you need it.**
      The default Compose file does not publish it. Adding `ports:
      ["5432:5432"]` exposes the database to the host and possibly
      the network. Only do it if a backup or admin tool needs it,
      and prefer `127.0.0.1:5432:5432`.

- [ ] **Per-bot MCP ports are not published.** The Compose file
      does not publish them by default; the gateway reaches them
      over the internal network. The only port published to the
      host should be the gateway's.

- [ ] **External MCP access uses TLS or a tunnel.** Plain HTTP on
      a public IP leaks bearer tokens. Use Tailscale / WireGuard /
      SSH tunnel / TLS-terminating reverse proxy.
      → [MCP Exposure](mcp-exposure.md)

- [ ] **The host firewall blocks anything you are not deliberately
      exposing.** Even with Docker's port bindings, having `ufw`
      or equivalent in deny-by-default mode prevents accidents.

## Database

- [ ] **`DB_SCHEMA` is set to a unique value per instance.** Two
      instances on the same `DB_SCHEMA` will trample each other.
      Match it to the instance directory name.
      → [PostgreSQL Setup](postgres-setup.md)

- [ ] **The `pgdata` volume is on persistent storage.** Default
      Docker named volumes live under `/var/lib/docker/volumes`
      on the host's root disk. If your root disk is ephemeral
      (some cloud setups), bind-mount to persistent storage
      instead.

- [ ] **Backups are scheduled.** A `pg_dump` cron job, a
      filesystem snapshot policy, or an external Postgres with
      managed backups. Pick one and verify it runs.
      → [PostgreSQL Setup: Backups](postgres-setup.md#backups)

- [ ] **You have tested a restore.** A backup you have not
      restored is a wish. Restore into a throwaway database and
      check the bot can read its own data.

- [ ] **Backup retention matches your tolerance for lost data.**
      Default the retention to "longer than you would notice a
      problem" — typically 30 days at minimum.

- [ ] **You know which schemas exist.** `\dn` in `psql` lists
      them. Stale schemas from removed instances waste space; drop
      them with `DROP SCHEMA "<name>" CASCADE;` once you are sure.

- [ ] **You have read the migrations directory before upgrading.**
      The bot now uses `sqlx::migrate!` against `migrations/`,
      applied automatically on startup against each instance's
      schema (tracked in a per-schema `_sqlx_migrations` table).
      No operator action is required for ordinary releases — but
      a release that ships a destructive or long-running
      migration will be flagged in the CHANGELOG, and you should
      take a backup before applying it.
      → [Database Schema: Migrations](../architecture/database-schema.md#migrations)

## Configuration hygiene

- [ ] **Each instance has its own directory under `instances/`.**
      One directory per Discord identity. No sharing of `.env` or
      `config.toml` between bots.

- [ ] **`config.toml` reflects the features you actually use.**
      Feature flags off for anything you do not want. Each
      enabled feature requires its config section (`[auto_role]`,
      `[minecraft]`, etc.) — the bot warns at startup if a flag
      is on but the section is missing.
      → [Instance Config](../configuration/instance-config.md)

- [ ] **`personality.txt` reads how you want the bot to sound.**
      The example default is functional but generic. Edit it for
      production bots.

- [ ] **The `command_prefix` does not collide with another bot in
      the same server.** If two bots share `!`, both will respond
      to every `!cmd`.

## Operations

- [ ] **`restart: unless-stopped` is set on every service.** The
      default Compose file already does this. Confirm if you
      hand-edited.

- [ ] **The host has a reboot policy that brings Docker back up.**
      `systemctl enable docker` on systemd hosts. Otherwise
      `restart: unless-stopped` does nothing on a host reboot.

- [ ] **You have a documented upgrade process.** Knowing whether
      you do `docker compose pull` (image-based) or `git pull &&
      docker compose build` (source-based) saves panic later. Keep
      the bot's image tag pinned to a specific version, not
      `:latest`.
      → [Upgrading](upgrading.md)

- [ ] **You read the CHANGELOG before pulling a new release.**
      Releases occasionally need manual database migrations. The
      changelog flags them.

- [ ] **Disk space is monitored.** Postgres data, container logs,
      and Docker images all grow. `df -h /var/lib/docker` and
      Postgres's `pgdata` volume size should be on whatever
      monitoring you have. A full disk wedges everything.

- [ ] **Log rotation is in place.** Docker's default JSON file
      driver has no rotation; logs grow indefinitely until they
      fill the disk. Either set `max-size` and `max-file` on the
      logging driver, or use `journald` (which rotates by
      default).

- [ ] **Health checks have somewhere to alert from.** A `cron` job
      that runs `docker compose ps --format json` and pages on
      anything not `healthy` is the minimum viable. Better: a
      proper monitoring agent (Healthchecks.io, Uptime Kuma,
      Datadog, etc.) hitting a wrapper script.
      → [Monitoring](monitoring.md)

- [ ] **Rate limiters need no operator action.** All four
      per-user limiters (ai / music / moderation / stocks) are
      now wired into their respective command paths and clean up
      stale entries automatically — there is nothing to schedule
      or prune by hand. Previously only the AI limiter was
      enforced; the rest were defined but unused.

## MCP-specific (if exposed)

- [ ] **`MCP_GATEWAY_AUTH_TOKEN` is at least 32 random bytes.**
      `openssl rand -hex 32` is the easiest way to generate one.
      Short or guessable tokens are not tokens.

- [ ] **The bearer token is rotated when an operator leaves.**
      There is no per-client revocation, so rotating the shared
      token and redistributing is the only mechanism.

- [ ] **MCP clients are configured with the production token, not
      a staging one.** Rotating staging because it leaked into a
      test log should not affect production.

- [ ] **Your reverse proxy passes the `Authorization` header
      through.** Some proxies strip auth headers by default.

- [ ] **Reverse proxy timeouts are long enough for SSE.** MCP
      uses Server-Sent Events; default 60-second proxy timeouts
      kill streams. See [MCP Exposure](mcp-exposure.md).

## Multi-instance

- [ ] **Every instance has a distinct `DB_SCHEMA`.** Already
      mentioned but worth repeating — it is the most-common
      misconfiguration in multi-instance setups.

- [ ] **Every instance has a distinct `DISCORD_TOKEN`.** Two bots
      on one token will conflict on the gateway connection.

- [ ] **The gateway's `INSTANCES` lists every backend.** Missing
      a backend means the gateway cannot route to it.
      → [Multi-Instance Deployment](multi-instance-deployment.md)

- [ ] **The gateway's `depends_on` lists every backend.** A
      missing backend means the gateway might start before that
      bot is ready.

- [ ] **Each instance's prefix is sensible.** Two bots in the same
      Discord server need different prefixes.

## Final smoke test

After every configuration change:

- [ ] **Startup logs are clean.** No `panic`, no
      `Failed to ...`, no unexpected `WARN`.
      → [Monitoring: Log lines worth knowing](monitoring.md#log-lines-worth-knowing)

- [ ] **`docker compose ps` shows everything `healthy`.**

- [ ] **The bot is online in Discord.** Green dot, responds to
      `!m help`.

- [ ] **An end-to-end command works.** Try a music command (`!m
      play test`), a moderation command (`!m banlist`), or
      whatever your most-used feature is. If it returns a
      sensible response, the wiring is correct.

If anything on this list is unanswered or "no", fix it before you
walk away from the deployment. The defaults are reasonable; the
defaults are not "production-grade with no thought required."

## Cross-references

- [Docker Compose](docker-compose.md) — the underlying stack.
- [Environment Variables](../configuration/environment-variables.md) —
  every variable, what it does, what is required.
- [Secrets Management](../configuration/secrets-management.md) —
  rotation and storage.
- [PostgreSQL Setup](postgres-setup.md) — backups, schemas,
  external Postgres.
- [MCP Exposure](mcp-exposure.md) — the network and auth side of
  the MCP endpoint.
- [Monitoring](monitoring.md) — logs, health checks, alerts.
- [Upgrading](upgrading.md) — how to move forward without
  breaking things.
