# Building Locally

This page is for the case where you want to skip Docker and run the bot
directly with `cargo`. That's the right setup for active development —
the rebuild loop is faster, you get full IDE integration, and the
process is yours to attach a debugger to. If you just want a bot to talk
to in Discord, use the [Quickstart](../getting-started/quickstart.md)
instead — Docker is the easier path.

By the end of this page you'll have the main crate compiling, a
PostgreSQL database it can talk to, an instance directory pointing at
your bot, and `cargo run` producing a live Discord bot.

## What you need installed

Three groups of dependencies: the Rust toolchain, the system libraries
the bot links against, and the runtime tools it shells out to.

### Rust toolchain

Install [rustup](https://rustup.rs) and use a stable toolchain:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default stable
```

The crate targets Rust 2021 edition and tracks the latest stable
release. Anything from the last few stable versions should compile.

### System libraries

The Discord client links against Opus and libsodium for voice, and the
build itself needs `cmake` for some transitive C dependencies. On
Debian/Ubuntu:

```bash
sudo apt-get install -y cmake libopus-dev libsodium-dev libssl-dev pkg-config
```

On macOS with Homebrew:

```bash
brew install cmake opus libsodium openssl pkg-config
```

If you forget one of these, `cargo build` fails partway through with a
linker error mentioning the missing library. The fix is always to
install it and run `cargo build` again — there's no need to
`cargo clean`.

### Runtime tools

The music feature shells out to `ffmpeg` and `yt-dlp` at runtime. They
don't need to be present at build time, but the bot will fail to play
anything without them:

```bash
sudo apt-get install -y ffmpeg
pip install --user yt-dlp
```

Make sure both are on your `PATH`. `which ffmpeg && which yt-dlp`
should print two paths.

## PostgreSQL

The bot needs a PostgreSQL instance to connect to. The easiest option
is the bundled Compose service, even when the bot itself is not in
Docker:

```bash
docker compose up -d postgres
```

This starts PostgreSQL 17 on `localhost:5432` with the credentials the
default `DATABASE_URL` expects. Stop and remove it later with
`docker compose down`.

If you'd rather use a system-installed Postgres, create a database and
user that match your `DATABASE_URL`:

```sql
CREATE USER discord_bot WITH PASSWORD 'discord_bot_pass';
CREATE DATABASE discord_bot OWNER discord_bot;
```

The bot creates its own schema (`CREATE SCHEMA IF NOT EXISTS "<name>"`)
on startup, so you don't need to do that yourself. The user just needs
permission to create schemas in the database.

## Discord application

You need a Discord bot token. If you don't have one, follow the
[Prerequisites](../getting-started/prerequisites.md) page to create the
application, enable the **Message Content Intent** and **Server Members
Intent**, and invite the bot to a test server. Save the token, the
client ID, and the guild ID — you'll paste all three into your
environment in a moment.

## Set up an instance directory

Each running bot needs an instance directory containing `config.toml`,
`personality.txt`, and a `.env` file. The `instances/example/`
directory is the canonical starter. For local work, copy it:

```bash
cp -r instances/example instances/local
cp instances/local/.env.example instances/local/.env
```

Edit `instances/local/.env` and fill in:

```bash
DISCORD_TOKEN=<your token>
CLIENT_ID=<your application id>
GUILD_ID=<your test server id>
DATABASE_URL=postgresql://discord_bot:discord_bot_pass@localhost:5432/discord_bot
DB_SCHEMA=local
```

If you want AI chat, add `DEEPSEEK_API_KEY=...` and/or
`GEMINI_API_KEY=...`. If you want stock trading, add
`FINNHUB_API_KEY=...`. Anything you leave unset just disables the
corresponding feature; the bot still boots.

`config.toml` ships with sensible defaults — change `bot_name` and
`command_prefix` if you want, and leave every feature flag in
`[features]` set to `false` for your first run.

## Build and run

The bot reads its instance directory from the `CONFIG_DIR` environment
variable (default: the current directory). Point it at the directory
you just set up:

```bash
CONFIG_DIR=instances/local cargo run
```

The first build is slow — ten minutes or so on a laptop, longer on a
small VPS. `cargo` is downloading and compiling about 400
dependencies. Subsequent rebuilds with no changes take seconds; an
incremental change in one file usually takes 5–20 seconds to
rebuild.

When the build finishes you should see startup logs that end with
something like:

```
INFO discord_bot::db: Database initialized (schema: local).
INFO discord_bot: Instance config loaded: Example Bot (prefix: !)
INFO discord_bot: Starting bot...
INFO discord_bot::events::ready: Example Bot is connected! (ID: ...)
```

The bot now appears online in your test server. `!m help` should
respond.

Press `Ctrl+C` to stop. There's no graceful shutdown sequence — the
process exits immediately and the next start picks up from a clean
slate (with persisted Postgres state intact).

## Speeding up the inner loop

A few things help once you're iterating:

- **`cargo check`** is much faster than `cargo build` and is enough to
  catch type errors. Use it while writing code, then `cargo run` when
  you're ready to test in Discord.
- **`sccache`** (`cargo install sccache && export RUSTC_WRAPPER=sccache`)
  caches incremental builds across `cargo clean`s and across branches.
  It's especially useful if you switch between branches with different
  dependency sets.
- **`mold`** is a faster linker than the default GNU ld. Install it,
  then add a `.cargo/config.toml` snippet pointing the linker at
  `mold`. On Linux this can cut link time from 10 seconds to under 1.
- **The dev profile is unoptimized.** That's deliberate — debug builds
  are smaller and faster to produce. Don't reach for `--release`
  unless you're profiling. The bot is plenty fast in debug.

## Building the gateway crate

The `mcp-gateway/` directory is a separate crate. It builds
independently and is not part of `cargo run` for the main crate. To
build or test it, either `cd mcp-gateway` and run cargo there, or use
`--manifest-path` from the repo root:

```bash
cargo check --manifest-path mcp-gateway/Cargo.toml
cargo test --manifest-path mcp-gateway/Cargo.toml
```

CI runs format, clippy, build, and test on both crates separately. Do
the same locally before opening a PR — see
[Contributing Workflow](contributing-workflow.md).

## Building the docs

The mdBook lives under `docs/` and is built with
[mdBook](https://rust-lang.github.io/mdBook/). Install once:

```bash
cargo install mdbook
```

Then build or live-preview from the repo root:

```bash
mdbook build         # writes static HTML to ./book
mdbook serve         # live-reloading preview on http://localhost:3000
```

The published site is generated by GitHub Actions from `master`; you
don't need to commit `book/`.

## Common build problems

- **`error: linker 'cc' not found`** — install `build-essential` (or
  the equivalent toolchain on your platform). On macOS, install Xcode
  Command Line Tools (`xcode-select --install`).
- **`failed to find tool. Is `cmake` installed?`** — exactly what it
  says: install cmake.
- **`could not find native library 'opus'`** — install `libopus-dev`
  on Debian, or `opus` via Homebrew on macOS.
- **`Failed to connect to database`** at startup — check that
  PostgreSQL is running, the credentials match `DATABASE_URL`, and
  that the user can connect from your host. `psql "$DATABASE_URL"`
  is the quickest way to confirm.
- **`<KEY> must be set in .env`** at startup — the loader couldn't
  find a required variable. Either you didn't set it, or `cargo run`
  isn't picking up `.env`. The bot reads `.env` from the current
  working directory; running from the repo root with `CONFIG_DIR`
  pointed at your instance is the standard pattern.
- **`<KEY> has placeholder value`** — you copied `.env.example` but
  didn't fill in real values. Edit `.env` and replace any
  `your-...` placeholders.

## Next steps

- [Adding a Command](adding-a-command.md) — write your first command
  and watch it appear after the next `cargo run`.
- [Adding a Feature Module](adding-a-feature-module.md) — for changes
  bigger than a single command.
- [Debugging](debugging.md) — tracing, `RUST_LOG`, and what to do when
  things hang.
- [Contributing Workflow](contributing-workflow.md) — fork → PR → merge.
