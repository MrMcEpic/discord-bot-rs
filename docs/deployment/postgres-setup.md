# PostgreSQL Setup

The bot needs Postgres. The shipped `docker-compose.yml` includes a
`postgres:17` sibling service so a default deployment is fully
self-contained, but you are not required to use it — pointing the
bot at any reachable Postgres instance works the same way. This
page covers both shapes, the schema-per-instance model the bot uses,
how to back the database up, and how migrations work.

The architectural side of the schema model lives in
[Multi-Instance Model](../architecture/multi-instance-model.md) and
the table reference (plus the migration system) lives in
[Database Schema](../architecture/database-schema.md). This page is
about the operations.

## Bundled Postgres

The default. The Compose file declares:

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
```

A single `discord_bot` database lives inside a single Postgres
container. The data persists in a named Docker volume called
`pgdata`, which `docker compose down` does **not** delete — only
`docker compose down -v` does. The default credentials
(`discord_bot` / `discord_bot_pass`) are baked into the Compose
file and the example `.env`. Change them only if the database is
reachable from outside the host; on the internal Compose network
the credentials are not the threat.

This setup is suitable for any deployment where:

- Only the bot and its siblings need this Postgres.
- One host owns both the bot and the database.
- You are happy to handle backups with `docker compose exec` and a
  cron job.

It is not suitable when you already run a managed Postgres for
other applications, when you want point-in-time recovery, or when
you want to host the database off the bot's box for resilience. For
those cases, switch to external Postgres.

## External Postgres

To point the bot at an existing Postgres instead of the bundled one:

1. Create a database on the external server. Any name works — the
   bot does not care.
2. Create a user with `CREATE` and `USAGE` privileges on that
   database. The bot needs to be able to create schemas, create
   tables inside them, and read and write rows.
3. Set `DATABASE_URL` in your instance's `.env` to the new
   connection string:
   ```bash
   DATABASE_URL=postgresql://username:password@db.example.com:5432/discord_bot_db
   ```
4. Remove the `postgres` service from `docker-compose.yml`, or just
   leave it and ignore it.
5. Remove the `depends_on: postgres` block from the `bot` service —
   it has no local Postgres to wait for.
6. `docker compose up -d`.

The first time the bot connects, it runs `CREATE SCHEMA IF NOT
EXISTS "<DB_SCHEMA>"` and then applies every migration in
`migrations/` inside that schema via `sqlx::migrate!`. After that,
every connection in the pool is configured with `SET search_path TO
"<DB_SCHEMA>"` so all queries — and the migration runner's own
`_sqlx_migrations` tracking table — land there. There is no extra
setup step required on the external server beyond creating the
database and granting the user.

If you are adding a second instance later, give it its own
`DB_SCHEMA` and the bot will create a new schema in the same
database — see [Multi-Instance Deployment](multi-instance-deployment.md).

## The schema-per-instance model

Every instance's data lives inside one Postgres schema, named by the
`DB_SCHEMA` environment variable. At startup,
[`init_pool`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/db/mod.rs)
does three things in order:

1. Opens a one-off connection and runs `CREATE SCHEMA IF NOT
   EXISTS "<schema>"`.
2. Builds a connection pool with an `after_connect` hook that runs
   `SET search_path TO "<schema>"` on every new connection.
3. Calls `sqlx::migrate!("./migrations").run(&pool)` to apply every
   migration that hasn't been applied yet. Migration history is
   tracked per-instance in a `_sqlx_migrations` table inside the
   schema.

The `search_path` hook is the magic. Postgres resolves unqualified
table names by walking `search_path` in order, so once it is
pinned to the instance's schema, every `SELECT * FROM tempbans`
in the codebase implicitly becomes `SELECT * FROM
"<schema>".tempbans`. No feature module has to know the schema
name; the multi-tenancy boundary is invisible above the pool.

What this means operationally: **every instance gets its own
schema, every schema is independent**. Drop one schema and the
others are untouched. Back up one schema with `pg_dump --schema=`
and restore it without affecting the others. Two instances cannot
read or write each other's data even by accident — the connections
literally cannot see the other schema's tables.

If `DB_SCHEMA` is unset, the bot falls back to `public` — but the
shipped `instances/example/.env.example` already sets it to `example`,
so a fresh quickstart user gets a properly-isolated `example` schema
out of the box. **For any real deployment you should set this to a
unique value per instance**, typically matching your instance directory
name. `mybot1` and `mybot2`, not `public` and `public`.

## Backups

Backups are owned by you, not the bot. The bundled `postgres:17`
container does not run any backup tool out of the box. Pick one of
these patterns.

### `pg_dump` over `docker compose exec`

The simplest, single-host approach. From the host:

```bash
docker compose exec -T postgres pg_dump -U discord_bot \
  --schema=mybot discord_bot > backups/mybot-$(date +%F).sql
```

`--schema=mybot` restricts the dump to a single instance's data, so
you can take per-instance backups on different schedules. Drop the
flag for a full database dump.

Schedule it from cron on the host:

```cron
0 3 * * * cd /opt/discord-bot && docker compose exec -T postgres pg_dump -U discord_bot --schema=mybot discord_bot | gzip > /var/backups/discord-bot/mybot-$(date +\%F).sql.gz
```

Restoring a per-schema dump:

```bash
gunzip -c mybot-2024-01-01.sql.gz | docker compose exec -T postgres psql -U discord_bot discord_bot
```

If you are restoring on top of an existing schema, drop it first
(`DROP SCHEMA "mybot" CASCADE;`) — `pg_dump` output assumes the
target objects do not already exist.

### Volume snapshots

For a host with a snapshotting filesystem (ZFS, btrfs, LVM) or a
cloud provider that snapshots block storage, take filesystem
snapshots of `pgdata`'s underlying volume. This captures the
database in a crash-consistent state, which Postgres can recover
from on restart. Snapshots are faster and cheaper than `pg_dump`
for large databases, but they restore the entire database, not a
single schema.

If you go this route, **stop the bot first** (`docker compose stop
bot mcp-gateway`), let any in-flight writes settle for a few
seconds, take the snapshot, then restart. For most deployments the
bot's write rate is so low that you can take live snapshots without
issue, but the safe order is bot-stopped-during-snapshot.

### External Postgres backups

If you switched to external Postgres (managed RDS, your own
Postgres host, etc.), use whatever backup story that infrastructure
already provides. AWS RDS automated backups, Postgres native
streaming replication, `pgbackrest`, `wal-g`, take your pick. The
bot writes such a small amount of data that any of these are
overkill in absolute terms, and that is fine.

### What to back up

Everything in the database is worth keeping. The full table list is
in [Database Schema](../architecture/database-schema.md). The
`tempbans`, `stock_*`, and `member_activity` tables would be
genuinely painful to lose; `guild_settings` is one row per guild and
trivial to recreate; `stock_price_cache` is throwaway. There is no
"don't bother backing this up" table — just take the whole schema.

The instance directory itself (`config.toml`, `.env`,
`personality.txt`) is **not** in the database. Back that up
through whatever you use for code or configuration — `git` is the
obvious answer.

## Migrations

The bot uses `sqlx::migrate!` against a top-level `migrations/`
directory. Every migration file is `<UTC-timestamp>_<description>.sql`,
sqlx applies them in timestamp order on every startup, and each
applied migration is recorded in a `_sqlx_migrations` table inside the
instance's schema. The migrations themselves are embedded in the
binary at build time, so no `DATABASE_URL` is required to compile.

What this means in practice:

- **Adding a new table or index** in a new bot release is
  transparent. The release ships a new migration file, the next
  startup applies it, no manual intervention.
- **Renaming or dropping a column, changing a type, adding a NOT
  NULL constraint** is also transparent — it just goes in a new
  migration file with the appropriate `ALTER TABLE` (and any backfill
  `UPDATE` it needs). Only changes that require manual coordination
  with running instances (e.g. multi-step zero-downtime migrations)
  will be flagged in release notes.
- **Restoring a backup from an older version** is safe: the
  migration step on the next boot replays any migrations the older
  dump was missing, in order. Because the init migration uses
  `IF NOT EXISTS`, it also tolerates restores from snapshots that
  predate the migration system.
- **Pre-migration databases** (anything that ran the bot before this
  system was introduced) are handled by the init migration's
  `IF NOT EXISTS` guards: it is a no-op against a database that
  already has the bootstrapped tables, and sqlx writes the
  `_sqlx_migrations` row afterwards so future migrations have a
  clean history.

Do not edit a migration file once it has shipped — sqlx checksums
the file contents and a mismatch on the next startup is a hard
failure. Land schema fixes in a new file with a later timestamp.

When a release does require operator action beyond "restart the
bot" (rare), it will be flagged in [Upgrading](upgrading.md) and the
[CHANGELOG](https://github.com/MrMcEpic/discord-bot-rs/blob/master/CHANGELOG.md).
The maintainer's policy is to avoid breaking schema changes within a
major version, but the project is young — read the release notes
before upgrading any version where the minor or major number
changed.

## Tuning

For a single bot, Postgres needs no tuning. The default `postgres:17`
configuration handles dozens of bots writing tens of operations per
second without breaking a sweat.

For larger deployments, the ones to think about:

- **`max_connections`** — the bot opens a small pool (a few
  connections) per instance. Default 100 is enough for ~30 bots
  without changes.
- **`shared_buffers`** — bumping from the 128 MB default to 25% of
  available RAM helps if the active dataset stops fitting in cache.
- **`work_mem`** — the bot does not run heavy joins or sorts;
  default is fine.
- **`autovacuum`** — leave on. The `tempbans` and `stock_*` tables
  see a lot of updates, autovacuum keeps them tidy.

If you are running the bundled Postgres and want to pass tuning
flags, use `command:` in the Compose file:

```yaml
postgres:
  image: postgres:17
  command: postgres -c max_connections=200 -c shared_buffers=512MB
```

## Connection security

On the internal Compose network, Postgres is reachable only from
other services in the project — no host port is published. The
default `discord_bot_pass` password is fine for that scope.

If you publish the Postgres port to the host (`ports: ["127.0.0.1:5432:5432"]`),
the password is what stops a process on the host from connecting.
Change it. If you publish to a non-loopback host port, also enable
TLS (`ssl=on` in `postgresql.conf`) and consider a different
authentication method in `pg_hba.conf` — `scram-sha-256` rather
than the default trust on the local socket.

## Cross-references

- [Environment Variables: `DATABASE_URL` and `DB_SCHEMA`](../configuration/environment-variables.md#database-required) —
  the variables that point the bot at Postgres.
- [Multi-Instance Model](../architecture/multi-instance-model.md) —
  why every instance gets its own schema.
- [Database Schema](../architecture/database-schema.md) — the
  table-by-table reference.
- [Multi-Instance Deployment](multi-instance-deployment.md) —
  adding a second instance against the same Postgres.
- [Upgrading](upgrading.md) — when manual migrations are needed.
