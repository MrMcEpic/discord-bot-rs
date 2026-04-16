# FAQ

The questions that come up most often, grouped by topic. Each answer
points at the deeper page if you need the full story.

## Getting it running

### My bot doesn't come online — there's no green dot.

Three common causes, in order of frequency:

1. **The token is wrong.** Logs show `Invalid Token` or a WebSocket
   close. Generate a new token in the Discord Developer Portal under
   your application → Bot → Reset Token, paste it into your `.env`,
   restart the bot.
2. **Privileged intents are off.** The bot needs **Message Content
   Intent** (to read prefix commands) and **Server Members Intent**
   (for joins, auto-role, welcome). Both are toggles on the same Bot
   page. Logs show `Disallowed intents`.
3. **The bot panicked at startup.** Look for the line right before
   the process exits. The `Config::load()` panics with a clear
   message when a required env var is missing or still has its
   placeholder `your-...` value.

See [Quickstart Troubleshooting](../getting-started/quickstart.md#troubleshooting)
for the full list and [Debugging](../development/debugging.md) for
log-level controls.

### `!m help` doesn't get a reply, but the bot is online.

Two possibilities:

- **Permissions in that channel.** The bot needs **View Channel**,
  **Send Messages**, and **Read Message History**. Some channels
  inherit deny-by-default overrides that block bot replies. Check
  channel-level overrides for the bot's role.
- **The prefix is wrong.** Default is `!`, but `command_prefix` in
  `config.toml` may have been changed. The startup logs print
  `Instance config loaded: <name> (prefix: <prefix>)` — check that.

### AI chat doesn't reply when I @mention the bot.

AI chat needs at least one of `DEEPSEEK_API_KEY` or `GEMINI_API_KEY`.
If neither is set, mentions are silently ignored. Set one in your
`.env`, restart, and try again.

If you have a key set and it still doesn't reply, tail the logs with
`RUST_LOG=discord_bot::ai=debug,info` — the pipeline logs every
inbound mention and every API call. Common downstream causes:

- Rate limit hit (10 calls per user per 60s). Wait a minute.
- DeepSeek/Gemini outage. The logs will show the upstream error.
- The bot lacks **Send Messages** in the channel you mentioned it in.

See [AI Chat](../features/ai-chat.md) for the full feature page.

### Music doesn't play.

The pipeline is `yt-dlp` → `ffmpeg` → `songbird`. Each can fail:

- **`yt-dlp` outdated** — YouTube breaks yt-dlp every few weeks.
  `pip install -U yt-dlp` in the bot's environment, restart. The
  Dockerfile pins to `pip install yt-dlp`; rebuilding the image
  pulls the latest.
- **`ffmpeg` missing on `PATH`** — the Docker image bundles it;
  bare-metal needs `apt install ffmpeg`.
- **Bot can't join voice** — check the voice channel's permissions
  for **Connect** and **Speak** on the bot's role.
- **DJ mode is on** — `!m djmode` toggles a setting where only
  members with the configured DJ role can use music commands. Check
  with `!m djmode` again to see the current state.

### Database connection issues.

`Failed to connect to database` at startup means the connection
string in `.env` is wrong or the database isn't reachable. Quick
sanity check: `psql "$DATABASE_URL"` from the same host. If that
works and the bot still can't connect, double-check the bot is
loading the right `.env` (`CONFIG_DIR` must point at the directory
holding it).

If you see `pool acquire timed out` mid-run, Postgres restarted or
the network blipped. sqlx reconnects on the next query — usually no
action needed, but check Postgres health if it keeps happening.

## Multi-instance and operations

### How do I run more than one bot on the same machine?

The model is "one process per bot, one schema per process, all
sharing one Postgres." Concretely: copy `instances/example/` to a
new directory, fill in a different `DISCORD_TOKEN`, `CLIENT_ID`,
`GUILD_ID`, and `DB_SCHEMA`, then add a second `bot` service to
`docker-compose.yml` pointing at the new directory.

The full recipe is in
[Multiple Instances](../configuration/multiple-instances.md). The
key constraint: `DB_SCHEMA` **must be unique** per instance. There's
no defensive check, and two instances on the same schema will
corrupt each other's data.

### How do I upgrade?

Pull the latest image (or rebuild from source), then restart the
bot service:

```bash
git pull
docker compose pull bot
docker compose up -d bot
```

The bot is forward-compatible across schema migrations — `migrate`
runs `CREATE TABLE IF NOT EXISTS` for every table at startup. If a
release introduces a destructive migration (column rename, table
drop) the changelog and release notes will say so explicitly.

For multi-instance deployments, restart instances one at a time so
you keep at least one bot online while you upgrade the others.

See [Upgrading](../deployment/upgrading.md) for the deeper version
of this answer including rollback steps.

### How do I back up the bot?

Two things to back up:

1. **The Postgres data volume.** All persistent state — tempbans,
   guild settings, stock portfolios, member activity, reminders if
   you've added them — lives there. `pg_dump` against your
   `DATABASE_URL`, store the dump somewhere safe, automate with cron
   or a managed backup service. Per-instance dumps are
   `pg_dump --schema=<DB_SCHEMA>`.
2. **Your `instances/` directories.** They contain `.env` (with
   secrets), `config.toml`, `personality.txt`, and any prompt files
   you've added. `tar` them up and store alongside the database
   backups. They're small.

Source code is on GitHub; you don't need to back that up.

### How do I expose the MCP server externally?

Default behaviour is `MCP_BIND_ADDR=127.0.0.1`, which only the bot's
own host can reach. To expose it:

1. Set `MCP_AUTH_TOKEN=<long random value>` — without auth on a
   public address, anyone with network access can drive the bot.
2. Set `MCP_BIND_ADDR=0.0.0.0` (or your specific interface).
3. Open the port (`MCP_PORT`, default `9090`) in your firewall.
4. If you're running multiple instances, use the included
   `mcp-gateway` to route requests by instance name.

[MCP Exposure](../deployment/mcp-exposure.md) walks through the
threat model. Don't skip the auth token.

### How do I enable Minecraft features?

Three steps:

1. Install the companion plugin on your Minecraft server (it owns
   the verification and donator-tier endpoints the bot calls).
2. Set `MC_VERIFY_URL` and `MC_VERIFY_SECRET` in the bot's `.env`.
3. In `config.toml`, set `features.minecraft = true` and toggle the
   sub-features you want under `[minecraft]`: `verify`,
   `donator_sync`, `chargeback`. Each sub-feature has its own
   `[minecraft.*_config]` table — see
   [`instances/example/config.toml`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/instances/example/config.toml)
   for every option.

Restart the bot. The startup logs print which sub-features
activated and which are enabled-but-misconfigured.

[Minecraft Verify](../features/minecraft-verify.md),
[Donator Sync](../features/minecraft-donator-sync.md), and
[Chargeback Alerts](../features/minecraft-chargeback.md) have the
deeper details.

## Design and conventions

### Why prefix commands and no slash commands?

Three reasons, in order:

1. **Iteration speed.** Prefix commands are immediate — change a
   handler, rebuild, type the command, see the result. Slash
   commands have global registration delays and per-guild quotas
   that turn the dev loop into "edit, push, wait, test."
2. **No global state in Discord.** Slash commands live as objects
   in Discord's API, attached to your application. Prefix commands
   live entirely in your code. That makes the bot easier to fork,
   self-host, and run with custom variants.
3. **Subcommand parsing was already solved.** Poise gives the bot a
   parent `m` command with a typed subcommand tree, which gives
   users an interface that reads like `!m play`, `!m wordle`,
   `!m stocks portfolio` — close enough to slash UX to be familiar
   without inheriting the registration pain.

The prefix is configurable per-instance (`command_prefix` in
`config.toml`); pick whatever you want.

### What's the license, really?

[AGPL-3.0-or-later](https://github.com/MrMcEpic/discord-bot-rs/blob/master/LICENSE).
The shape of it for a bot host:

- You can run this bot for any purpose, commercial or not.
- You can modify it for your own use without telling anyone.
- If you run a modified version that interacts with users over a
  network — which a Discord bot does by definition — the AGPL
  obligates you to make your modified source available to those
  users on request.
- You can't take this code, modify it, run it as a service, and
  refuse to share the source. That's the entire point of AGPL over
  GPL.

Contributing back is welcome but not required by the license; the
license only requires that derivative network services share their
own source. See [CONTRIBUTING.md](https://github.com/MrMcEpic/discord-bot-rs/blob/master/CONTRIBUTING.md)
for the inbound license terms.

### Can I commercialize this?

Yes, with the AGPL constraint above. You can run a paid hosted
version of this bot, charge money for support, or sell custom
features built on top — the license imposes no fee, no royalty, and
no commercial restriction. What it does require: anyone using your
hosted version can request the source code of the version you're
running, and you have to provide it. If you're building proprietary
extensions that you don't want to release, the AGPL probably isn't
compatible with your business model and you should look elsewhere.

### How do I contribute?

The short version: fork, branch, change, test, PR. The long version
is in [Contributing Workflow](../development/contributing-workflow.md).
The pre-PR checklist is `cargo fmt`,
`cargo clippy --all-targets -- -D warnings`, `cargo test`, and a
manual run-through in a test Discord server.

If you're new to the codebase, start with
[Codebase Tour](../development/codebase-tour.md) and pick a small
issue from
[GitHub Issues](https://github.com/MrMcEpic/discord-bot-rs/issues)
to land your first PR.

## Misc

### What models does the AI use?

The default is DeepSeek's `deepseek-chat` model when
`DEEPSEEK_API_KEY` is set, falling back to Google's Gemini
(`gemini-2.0-flash`) when DeepSeek errors out and `GEMINI_API_KEY`
is set. Both are remote API calls; the bot has no local model.
DeepSeek is recommended as the primary because it's the cheapest of
the supported providers.

### Can I change the bot's personality?

Yes — every instance has a `personality.txt` next to its
`config.toml`. The contents are used as the system prompt for AI
chat. Edit, restart the bot, talk to it. See
[Personality Files](../configuration/personality.md) for tips on
writing one that holds up.

### Where does the music come from?

Anything `yt-dlp` can resolve — YouTube videos, playlists,
SoundCloud, Bandcamp, direct links to media files, and a few
hundred other sources. yt-dlp does the resolution; ffmpeg pipes the
output to songbird in OGG/Opus passthrough so the bot doesn't
re-encode. The pipeline is in `src/music/track.rs` and `voice.rs`.

### Why does the bot use so much disk space?

It doesn't, unless logging is misconfigured. The bot itself stores
nothing on disk other than `personality.txt` and any cookies
yt-dlp keeps in its cache. All persistent state is in Postgres.

If your container is growing, check `docker logs --no-color bot |
wc -l` — Docker keeps logs forever by default. Set a size limit in
`docker-compose.yml` under the bot service:

```yaml
logging:
  driver: json-file
  options:
    max-size: "10m"
    max-file: "3"
```

## Still stuck?

Log a [GitHub Issue](https://github.com/MrMcEpic/discord-bot-rs/issues/new/choose).
Bug report template asks for the version, reproduction steps, and
redacted logs — those three together usually let someone help you
on the first reply.
