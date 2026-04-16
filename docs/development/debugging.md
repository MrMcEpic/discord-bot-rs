# Debugging

This page is the bag of tricks for the moments when the bot is doing
something you didn't expect — silently failing a command, getting
stuck in voice, refusing to start, or crashing under specific load.
The tools are mostly the standard Rust ones (`tracing`, `RUST_LOG`,
the test harness, a profiler), but there are a few project-specific
patterns worth knowing.

## Logging

The bot uses [`tracing`](https://crates.io/crates/tracing) end to end.
Every log call goes through one of `tracing::info!`, `tracing::warn!`,
`tracing::error!`, or `tracing::debug!`, and `tracing_subscriber` is
initialised in `main.rs` with `tracing_subscriber::fmt::init()`. There
is no `println!` in the codebase — if you find one, replace it.

`tracing_subscriber::fmt::init()` reads the `RUST_LOG` environment
variable to decide which spans and events get emitted. The default is
`info` for everything, which is the right level for production but
hides most of the detail you want when debugging.

### Useful `RUST_LOG` settings

```bash
# Default: info from every crate (including serenity, sqlx, hyper).
cargo run

# Bot at debug, everything else at info — the typical dev setting.
RUST_LOG=discord_bot=debug,info cargo run

# Bot at trace (very loud), serenity quiet — useful when isolating
# bot logic from gateway noise.
RUST_LOG=discord_bot=trace,serenity=warn,info cargo run

# Just one module at debug.
RUST_LOG=discord_bot::ai=debug,info cargo run

# Music subsystem only.
RUST_LOG=discord_bot::music=debug,songbird=debug,info cargo run

# Database queries.
RUST_LOG=discord_bot::db=debug,sqlx=debug,info cargo run
```

The format is `<crate>=<level>` separated by commas, with a bare level
acting as the default for unmatched crates. `info`, `debug`, `trace`,
`warn`, and `error` are the levels — `trace` includes everything,
`error` only fatal stuff.

A common pattern when chasing a bug: start with
`RUST_LOG=discord_bot=debug,info`, reproduce, and grep for the
relevant module to see what fires.

### What's already logged

`main.rs` is verbose at startup — every feature flag's activation,
the database init, the instance config name and prefix, and each
background task's start are logged at `info`. If your bot doesn't
boot, the last `info` line before silence is your strongest hint.

Each module logs its hot paths at `debug`:

- `ai/deepseek.rs` logs the inbound message, tool calls and their
  results, and the final reply.
- `music/voice.rs` logs joins, leaves, track starts, and track-end
  events.
- `db/mod.rs` logs schema creation and migration progress.
- `mcp/mod.rs` logs the listen address.

`warn` is reserved for "unexpected but recoverable" — the donator
sync poll failed, an auto-role time check skipped a member, a
chargeback webhook arrived with a bad signature. `error` is reserved
for "this command failed and I'm reporting back to the user" plus the
single fallback in `on_error` for framework-level errors.

### Reading logs in Docker

When the bot runs under Compose, every log line goes to stdout, which
Docker captures:

```bash
docker compose logs -f bot                  # follow live
docker compose logs --since 10m bot         # last 10 minutes
docker compose logs bot 2>&1 | grep WARN    # filter
```

To raise the log level inside a Compose-deployed container, add
`RUST_LOG` to the bot service's `environment:` block in
`docker-compose.yml`:

```yaml
bot:
  environment:
    RUST_LOG: discord_bot=debug,info
```

Then `docker compose up -d bot` to restart. There's no live reload
of `RUST_LOG` — the subscriber is initialised once at startup.

## Common issues

A few classes of failure show up often enough to be named.

### "The bot doesn't come online."

Usually one of three causes. In rough order of frequency:

1. **Bad token.** Look for `Invalid Token` or `WebSocket close`
   in the logs near startup. Generate a new token in the Discord
   developer portal, paste it into `.env`, restart.
2. **Privileged intents disabled.** The bot needs **Message Content
   Intent** (to read prefix commands) and **Server Members Intent**
   (for member joins, auto-role, welcome). Both are toggled on the
   Bot page in the developer portal. Logs say `Disallowed intents`.
3. **The process started but hung on database init.** Watch for
   `Database initialized (schema: ...)`. If it never appears,
   Postgres is unreachable; check `DATABASE_URL` and the network.

### "A command silently does nothing."

Two flavours:

- **The command isn't registered.** You wrote a `#[poise::command]`
  function but didn't add `"<module>::<function>"` to the
  `subcommands(...)` list in `src/commands/mod.rs`. The command
  compiles, the bot boots, the user types it, nothing happens. Add
  the entry, restart.
- **The command panicked or returned an `Err`.** Poise's `on_error`
  in `main.rs` will reply `Error: <message>` and log
  `Command error: <error>`. If you see neither in the channel nor
  in the logs, you have a different bug — likely an early `return
  Ok(())` before any user-visible output, or a dropped future.

When in doubt: reproduce with `RUST_LOG=discord_bot=debug,info`.

### "AI chat doesn't reply."

Mention the bot, get nothing. The pipeline is in `src/ai/deepseek.rs`
and `src/ai/handle_mention`; the code logs at `info` when a request
comes in and at `error` when it fails. Possible causes:

- **No API key.** `DEEPSEEK_API_KEY` and `GEMINI_API_KEY` are both
  unset. The pipeline silently returns. Set at least one.
- **Rate limit hit.** The bot allows 10 AI calls per user per 60s.
  Eleventh call drops silently. Wait or restart.
- **DeepSeek/Gemini outage.** The logs will say so. The fallback
  path (DeepSeek → Gemini) only fires when DeepSeek returns an
  error response; if both are down, the bot is sad too.
- **A tool call hung.** Music searches via yt-dlp can stall when
  YouTube changes; the AI may be waiting on the tool. Tail
  `discord_bot::music=debug` and look for the offending track.

### "Music doesn't play."

The music pipeline involves `yt-dlp`, `ffmpeg`, and `songbird`. Each
can fail independently:

- **`yt-dlp` not on `PATH` or out of date.** YouTube breaks yt-dlp
  every few weeks; `pip install -U yt-dlp` is the fix more often
  than not.
- **`ffmpeg` not on `PATH`.** The Docker image has it; bare-metal
  setups need `apt install ffmpeg`.
- **The bot can't join voice.** Check that the Voice channel
  permissions allow the bot to **Connect** and **Speak**. Logs say
  `Failed to join voice channel`.
- **The track resolves but never plays.** Tail
  `RUST_LOG=discord_bot::music=debug,songbird=debug`. Look for an
  ffmpeg subprocess error — usually a codec mismatch or a stream
  yt-dlp couldn't extract.

### "Database connection issues."

Two patterns:

- **Cold start.** `Failed to connect to database` at startup. Check
  Postgres is up and `DATABASE_URL` is correct. `psql "$DATABASE_URL"`
  is the fastest test.
- **Hot disconnect.** `pool acquire timed out` mid-run. The
  Postgres process restarted or the network blipped; sqlx will
  reconnect automatically on the next query.

### "The bot is using a lot of CPU / memory."

Voice playback dominates. A bot in three voice channels with three
ffmpeg pipelines uses meaningfully more RAM than an idle bot. If
you're seeing growth without an obvious cause:

- Check `docker compose logs bot | grep "leaving voice"` — make sure
  the auto-leave-on-empty logic is firing. If channels stay joined
  with nobody in them, that's a leak.
- The `ai` rate limiter and the duration parser have unbounded
  internal `Vec`s with sliding-window pruning. Pruning happens on
  next access, so if a user makes one call then disappears, their
  entries linger until they call again. Not a correctness issue —
  bounded by the number of distinct users who've called once.
- For real heap profiling, see the Profiling section below.

### "Multi-instance: one bot has the wrong data."

Almost always `DB_SCHEMA` collision. Two instances with the same
`DB_SCHEMA` write to the same tables; their state intermixes. There
is no defensive check for this — the schemas just have to be
distinct. Fix the `.env`, restart both instances, and clean up the
mixed-up data manually.

## Stuck or hung

When the bot stops responding entirely:

1. **Is the process alive?** `ps aux | grep discord-bot` or
   `docker compose ps bot`. If exited, the logs will say why.
2. **Is the gateway connected?** Logs include heartbeats at `debug`
   level. A long gap means the gateway link is dropped; serenity
   normally reconnects automatically.
3. **Is the runtime stuck on a `.await`?** Most often a misuse of
   `DashMap`: holding an entry across `.await`. The fix is "look
   up, clone the inner `Arc`, drop the guard, await."
4. **Send `SIGQUIT` to dump a stack trace.** On Linux,
   `kill -QUIT <pid>` produces a thread dump from `tokio-console`
   if it's running, or simply terminates the process otherwise.

## Profiling

When you actually need numbers (you usually don't), the Rust
ecosystem has good tools:

- **`cargo flamegraph`** for CPU profiles. Install with
  `cargo install flamegraph`, run with
  `cargo flamegraph --bin discord-bot`. Produces an SVG you can open
  in a browser.
- **`tokio-console`** for runtime introspection. Add
  `console-subscriber` to dependencies, swap
  `tracing_subscriber::fmt::init()` for `console_subscriber::init()`,
  and run `tokio-console` in another terminal. Lets you see live
  task counts, busy/idle times, and detect deadlocks.
- **`heaptrack`** (Linux) for memory growth. Run with
  `heaptrack ./target/release/discord-bot`, kill the process when
  done, open the resulting file in `heaptrack_gui`.

These are heavier than the `RUST_LOG` flow and overkill for most
debugging — reach for them when a slow query or a runaway allocation
is real, not just suspected.

## Reproducing in the test harness

If you can extract the bug into a pure function — a duration parser
that returns `None` when it should return `Some`, a sanitiser that
keeps a marker it should strip — write a unit test that reproduces
it. The test stays in the repo as a regression guard. See
[Testing](testing.md) for the project's test posture.

## Reporting bugs

If you've debugged something to the point of needing help, file a
[bug report](https://github.com/MrMcEpic/discord-bot-rs/issues/new?template=bug_report.yml).
Include the version (or commit SHA), the deployment method (Docker
or bare metal), the `RUST_LOG` setting that produced your logs, and
the redacted log lines that show the failure. The template asks for
all of this; filling it out honestly speeds up triage by a factor of
ten.

## Next steps

- [Testing](testing.md) — the test posture and how to add a
  regression test for the bug you just fixed.
- [Building Locally](building-locally.md) — when you need a fresh
  local build to reproduce a deploy-only issue.
- [FAQ](../reference/faq.md) — the same questions, answered shorter.
