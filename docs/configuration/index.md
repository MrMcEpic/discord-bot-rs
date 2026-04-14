# Configuration

discord-bot-rs splits its configuration across three files per instance, each with a clear job. Once you understand the split, every other page in this section is just filling in the field-by-field detail.

## The three-file model

Every instance lives in its own directory under `instances/` and contains the same three files:

- **`.env`** holds secrets and runtime connection details: the Discord token, the database URL, AI provider API keys, and the schema name. It is loaded by [`dotenvy`](https://crates.io/crates/dotenvy) at startup and exists only on the host (and inside the container at runtime).
- **`config.toml`** holds the bot's identity and feature surface: its display name, command prefix, which feature modules are enabled, and the IDs/settings each feature needs (role IDs, channel IDs, intervals). This file is checked in as part of your fork and versioned alongside the rest of the repo.
- **`personality.txt`** holds the free-form prose that becomes the system prompt for AI chat. It is plain text — no escaping, no structure — so editing it feels like editing a doc, not a config file.

## Why the split

Mixing secrets and structured config in one file is a recipe for accidentally committing tokens. Splitting them lets each format do what it does best.

`.env` is gitignored at the repo root, so secrets stay out of git history by default. Rotating a key is one line edit and a restart, with no risk of touching feature settings. `config.toml` is the opposite: checked in, typed, and reviewable in pull requests, so changes to feature behavior are visible and traceable. `personality.txt` is plain prose because system prompts are prose — putting them inside TOML would mean wrestling with multiline string escapes every time you wanted to tweak a sentence.

## What lives where

| What                  | File              | Example                                               |
|-----------------------|-------------------|-------------------------------------------------------|
| Discord bot token     | `.env`            | `DISCORD_TOKEN=MTxxxxxxxxxxxxxxxx...`                 |
| Database URL          | `.env`            | `DATABASE_URL=postgresql://user:pass@host:5432/db`    |
| Postgres schema name  | `.env`            | `DB_SCHEMA=mybot`                                     |
| AI provider API keys  | `.env`            | `DEEPSEEK_API_KEY=sk-...`                             |
| Bot display name      | `config.toml`     | `bot_name = "My Bot"`                                 |
| Command prefix        | `config.toml`     | `command_prefix = "!"`                                |
| Feature flags         | `config.toml`     | `[features]` `auto_role = true`                       |
| Role and channel IDs  | `config.toml`     | `[auto_role]` `from_role = "123456789012345678"`      |
| AI personality        | `personality.txt` | `You are a friendly assistant on this Discord server.`|

## How the files are loaded

At startup the bot does the following, in order:

1. Calls `dotenvy::dotenv()` to read the `.env` file from the current working directory, exporting each key into the process environment. This populates `DISCORD_TOKEN`, `DATABASE_URL`, and friends.
2. Determines `CONFIG_DIR` from the environment, defaulting to `.` (the current directory). Inside the Docker image this is set to `/config`, which Docker Compose mounts from the instance directory on the host.
3. Reads `config.toml` from `CONFIG_DIR` and parses it into the `InstanceConfig` struct. Failures here panic with a clear error pointing at the file.
4. Reads `personality.txt` (or whatever `personality_file` is set to) from `CONFIG_DIR`. An empty or missing file panics; the AI chat module needs a non-empty system prompt.
5. Logs the loaded `bot_name`, command prefix, and which feature modules are enabled, then connects to Discord and Postgres.

The mapping is mechanical: one instance directory on the host becomes `/config` inside the container, and everything the bot needs is in that directory. Nothing else is read.

## Configuration is per-instance

Each bot you run is a separate instance with its own `.env`, `config.toml`, and `personality.txt`. They share a Postgres database (with schema-level isolation per instance) and, optionally, a single MCP gateway, but otherwise they are wholly independent processes. See [Multiple Instances](multiple-instances.md) for the multi-bot recipe and [Multi-Instance Model](../architecture/multi-instance-model.md) for the architectural picture.

## Where to next

- [Environment Variables](environment-variables.md) — the canonical reference for every variable the bot reads from `.env`.
- [Instance Config](instance-config.md) — the canonical reference for every field in `config.toml`.
- [Personality Files](personality.md) — how to write a system prompt that does what you want.
- [Secrets Management](secrets-management.md) — keeping `.env` out of git, rotating tokens, and what to do if a secret leaks.
- [Multiple Instances](multiple-instances.md) — running more than one bot from a single repo and database.
