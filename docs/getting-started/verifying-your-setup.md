# Verifying Your Setup

Once you have run `docker compose up -d` and the logs look reasonable, this
page is the post-deploy checklist. Run through it once. If every command
matches its expected output, your bot is healthy. If something is off, the
bottom of the page points at what to read next.

All commands assume you are in the repo root.

## Docker-level checks

The containers should all be running and healthy.

```bash
docker compose ps
```

You should see three services: `postgres`, `bot`, and `mcp-gateway`. Each
should report `running` in the **STATUS** column. `postgres` and `bot` also
report `healthy` once their healthchecks settle (a few seconds for
postgres, up to a minute for the bot the first time).

If any service is `restarting`, look at its logs to find out why.

Next, the bot's recent logs:

```bash
docker compose logs bot --tail 20
```

A healthy startup ends with a sequence like this, in roughly this order:

```
INFO discord_bot::db: Database initialized (schema: example).
INFO discord_bot: Instance config loaded: Example Bot (prefix: !)
INFO discord_bot: Starting bot...
INFO serenity::gateway::bridge::shard_runner: [ShardRunner ShardInfo { id: ShardId(0), total: 1 }] Running
INFO discord_bot::events::ready: Example Bot is connected! (ID: 123456789012345678)
INFO discord_bot::mcp: MCP server listening on 0.0.0.0:9090
```

Exact wording can shift slightly between releases, but you should see the
database schema initialize, the instance config load, a shard enter
`Running`, a line stating that the bot "is connected" along with its
user ID, and the MCP server bind to port 9090. If the last log line is
hours old and there is nothing about errors, the bot is sitting idle.
That is fine.

And postgres:

```bash
docker compose logs postgres --tail 5
```

You should see `database system is ready to accept connections`.

## Discord-level checks

Open Discord. The bot should appear in your server's member list with a
green dot. A grey dot means the bot is not running or cannot reach Discord;
the Docker-level checks above will tell you which.

Try a prefix command in any channel the bot can see:

```
!m help
```

You should get a help embed immediately. If your `config.toml` sets a
different prefix, use that instead. discord-bot-rs uses prefix commands
only — there are no slash commands to sync.

If you configured an AI API key, @mention the bot and ask it something. You
should get a reply within a few seconds. If you did not configure an AI
key, mentions are silently ignored — that is expected, not a bug.

## Database-level checks

The bot uses one PostgreSQL database with a separate schema per instance.
Confirm your instance's schema exists:

```bash
docker compose exec postgres psql -U discord_bot -d discord_bot -c '\dn'
```

You should see at least one schema beyond the built-in `public`,
`information_schema`, and `pg_*` schemas. The name should match your
instance's `DB_SCHEMA` from `.env`.

If the schema is missing, the bot did not finish its first migration —
check `docker compose logs bot` for migration errors.

## MCP-level checks

If you have not exposed the MCP port (the default), this section does not
apply. The MCP server inside the bot still runs, but it is only reachable
from inside the Docker network.

If you have mapped the port to your host, confirm it responds:

```bash
curl -i http://localhost:9090/mcp
```

You should get an HTTP response. The exact body depends on whether you
configured authentication; the important thing is that you get a response
rather than a connection refused. See
[MCP Exposure](../deployment/mcp-exposure.md) for how to expose it safely.

## What "good" looks like

A healthy bot has all of the following true at the same time:

- All Compose services report `running`, and `postgres` and `bot` report
  `healthy`.
- The bot's recent logs contain a "is connected! (ID: ...)" line and have no errors.
- The bot has a green dot in your Discord server.
- `!m help` replies when invoked in a channel the bot can read.
- Your instance's PostgreSQL schema exists.

If those five things are true, you are done. From here, treat any future
log errors as the diagnostic signal — the bot logs to stdout, which Compose
captures and `docker compose logs` reads.

## What to do if something fails

If any of the checks above fail, two pages have most of the answers:

- [Monitoring](../deployment/monitoring.md) covers reading logs, watching
  healthchecks, and what each service's failure modes look like.
- [FAQ](../reference/faq.md) collects the issues that come up repeatedly.

If neither helps, open an issue on
<https://github.com/MrMcEpic/discord-bot-rs> with the output of
`docker compose ps` and the last 50 lines of `docker compose logs bot`.
