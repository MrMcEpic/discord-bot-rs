# Upgrading

`discord-bot-rs` is shipped as Docker images on GitHub Container
Registry (`ghcr.io/mrmcepic/discord-bot-rs` and
`ghcr.io/mrmcepic/discord-bot-rs-mcp-gateway`) and as source on
[GitHub](https://github.com/MrMcEpic/discord-bot-rs). This page
covers how to move from one version to the next, what to expect
from the database when you do, and how breaking changes are
communicated.

## Versioning

The project uses [SemVer](https://semver.org/). The version is
visible in the `Cargo.toml` and on the `ghcr.io` image tags. Tag
suffixes:

- **`:0.5.0`** — a specific version. Pin this in production.
- **`:latest`** — whatever the most recent release is. Convenient
  for development; do not use in production.

While the project is in `0.x`, the **minor version** is the
breaking-change boundary. Going from `0.5.x` to `0.5.y` is always
safe; going from `0.5.x` to `0.6.0` may require a manual step.
Once it reaches `1.0`, the major version takes over that role.

The published images are currently amd64-only. If you are on
arm64 (Apple Silicon, Graviton, Ampere), build from source with
`docker compose build`.

## Reading the release notes

Every release ships with a [CHANGELOG entry](https://github.com/MrMcEpic/discord-bot-rs/blob/master/CHANGELOG.md)
that lists what changed. Before pulling, read the entries for
every version between yours and the new one. The changelog
distinguishes:

- **Added** — new features. Generally safe to pick up.
- **Changed** — behaviour changes. Read carefully.
- **Fixed** — bug fixes. Almost always wanted.
- **Deprecated** — features that still work but will be removed.
- **Removed** — features that are gone. Check whether you used
  them.
- **Migration required** — explicit flag for any release that
  needs a manual database step before the bot will boot.

If the release notes do not mention "Migration required", the
upgrade is the default flow below.

## Default upgrade flow

For a deployment that uses pre-built images:

```bash
# Bring the new images down
docker compose pull

# Restart only the services whose images changed
docker compose up -d
```

Compose detects that the bot and gateway images now have new
digests and recreates the containers in place. Postgres is not
upgraded by this — it stays on whatever `postgres:` tag is in your
Compose file.

For a deployment that builds from source:

```bash
git fetch
git checkout v0.6.0   # or whatever release tag
docker compose build
docker compose up -d
```

This rebuilds the bot and gateway images locally, then recreates
the containers.

For either path, the bot's startup migration step
([`migrate`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/db/mod.rs))
runs every boot. New tables and indexes from the upgrade get
created automatically. Existing tables are untouched.

## Watching for problems on the first boot after upgrade

Tail the logs immediately after the upgrade:

```bash
docker compose logs -f bot mcp-gateway
```

Things you want to see:

- `Database initialized (schema: <yours>)` — schema is in good
  shape.
- `Instance config loaded: <name> (prefix: ...)` — config still
  parses cleanly (a syntax change in `config.toml` between
  versions would surface here).
- `<botname> is connected!` — Discord connection is up.
- `Initialized backend <name>` per bot in the gateway logs.
- Any health check transitioning to `healthy` in
  `docker compose ps`.

Things that mean roll back:

- Any `panic` from the bot during startup. The bot is in a hard
  crash loop.
- `Failed to connect to database` — the connection string broke
  or Postgres rejected the credentials.
- A new required env var the upgrade introduced and your `.env`
  is missing. The release notes will name it.

If you need to roll back, redeploy the previous image tag:

```bash
docker compose pull   # implicit in `up -d` after editing image tag
# Edit docker-compose.yml: image: ghcr.io/mrmcepic/discord-bot-rs:0.5.0
docker compose up -d bot
```

For source builds, `git checkout` the previous tag and rebuild.

## Database migrations

The migration story today is intentionally simple: the
[`migrate`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/db/mod.rs)
function in `src/db/mod.rs` runs a flat list of `CREATE TABLE IF
NOT EXISTS` and `CREATE INDEX IF NOT EXISTS` statements every time
the bot starts. It is idempotent. There is no `alembic`-style
migration directory and no version table.

What this means for upgrades:

- **Adding a new table or index in a release is transparent.** The
  next boot creates it.
- **Renaming or dropping a column, changing a type, adding a NOT
  NULL constraint** requires a manual `psql` step before the
  upgrade. Releases that need this will say "Migration required"
  in the changelog and include the SQL.
- **The bundled Postgres major version may change.** If a release
  bumps the `postgres:17` image to `postgres:18`, the `pgdata`
  volume needs to be migrated using `pg_upgrade` or by dumping
  and reloading. The release notes will spell this out — Postgres
  major upgrades are not something to do casually.

A typical "Migration required" upgrade looks like:

```bash
# 1. Stop the bot so the schema is quiet
docker compose stop bot mcp-gateway

# 2. Take a backup
docker compose exec -T postgres pg_dump -U discord_bot \
  --schema=mybot discord_bot > pre-upgrade-mybot.sql

# 3. Apply the SQL from the release notes
docker compose exec -T postgres psql -U discord_bot discord_bot < release-notes-migration.sql

# 4. Pull the new images
docker compose pull

# 5. Bring everything back up
docker compose up -d
```

The bot's startup `migrate` step then handles any new-table
additions on top.

## Multi-instance considerations

When you run multiple bot instances against one Postgres, **every
instance shares the same database but lives in its own schema**.
Migrations are per-schema. If a release adds a new table, the
table is created inside the schema of whichever bot instance boots
first, and again inside each other instance's schema as they boot.
There is no way for instance A to step on instance B's tables.

You can also upgrade instances one at a time:

```bash
# Pull new images
docker compose pull

# Recreate just bot1 with the new image
docker compose up -d bot1

# bot2 keeps running on the old image until you choose to upgrade it
```

This is useful for canary upgrades, but be aware: if the new
release introduces SQL that the old version rejects (a new column
the old code does not know how to handle, or a column rename), a
mixed-version deployment can be unstable. The safest path is to
upgrade every instance together.

## Upgrading the gateway

The gateway is upgraded the same way as the bot — pull the new
`mcp-gateway` image, `docker compose up -d mcp-gateway`. The
gateway has no persistent state of its own; restarting it loses
nothing. MCP clients reconnect automatically the next time they
make a request.

The gateway and the bots do not have to be on matching versions in
the strictest sense, but you should aim to keep them in step. Tool
schema changes in the bot are not picked up by the gateway until
the gateway re-fetches the catalog (it does this on startup). If
a bot release adds a new tool, restart the gateway after the bot
upgrade to refresh the catalog.

## Upgrading Postgres

Patch versions of `postgres:17` (e.g. `17.0` to `17.4`) are
handled by Docker pulling the new image; the data on `pgdata` is
forward-compatible within a major version.

Major-version Postgres upgrades (17 to 18, etc.) require
`pg_upgrade` or a dump-and-reload, because the on-disk format
changes. The simplest dump-and-reload:

```bash
# 1. Dump everything from old Postgres
docker compose exec -T postgres pg_dumpall -U discord_bot > pg-dump.sql

# 2. Stop everything
docker compose down

# 3. Move the old volume aside (do not delete yet)
docker volume rename discord-bot-rs_pgdata discord-bot-rs_pgdata_v17

# 4. Edit docker-compose.yml, change image: postgres:17 -> postgres:18

# 5. Bring up the new Postgres
docker compose up -d postgres

# 6. Restore
docker compose exec -T postgres psql -U discord_bot < pg-dump.sql

# 7. Start the rest
docker compose up -d

# 8. Once you have verified the bot works, drop the old volume
docker volume rm discord-bot-rs_pgdata_v17
```

The bot does not care which Postgres major version it is talking
to as long as the connection works.

## Rebuilding from source

If you contribute changes locally or want a custom build:

```bash
git pull
docker compose build
docker compose up -d
```

`docker compose build` rebuilds both the `bot` and `mcp-gateway`
images from the local Dockerfiles. The build leverages BuildKit's
cargo cache mount, so incremental builds (small source changes)
take well under a minute. A clean build from a cold cache takes
3–8 minutes depending on the host.

## Cross-references

- [CHANGELOG](https://github.com/MrMcEpic/discord-bot-rs/blob/master/CHANGELOG.md) —
  the canonical source for what changed in each release.
- [PostgreSQL Setup](postgres-setup.md) — backups before upgrade.
- [Docker Compose](docker-compose.md) — the underlying stack.
- [Production Checklist](production-checklist.md) — what to verify
  after every upgrade.
