# Error Handling

This page describes how errors flow through the bot: where they start,
where they get turned into user-visible messages, and where they're
logged and swallowed. The design is deliberately minimal — one error
type, one `on_error` hook, and a handful of rules about who panics and
who doesn't.

## The `BotError` type

Every fallible function in this codebase returns `Result<T, BotError>`,
where `BotError` is a plain hand-written enum defined in
[`src/error.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/error.rs):

```rust
#[derive(Debug)]
pub enum BotError {
    Serenity(serenity::Error),
    Sqlx(sqlx::Error),
    Reqwest(reqwest::Error),
    SerdeJson(serde_json::Error),
    Other(String),
}
```

Each variant wraps one upstream error type. The fifth variant, `Other`,
is an escape hatch for ad-hoc string errors that don't correspond to a
specific upstream — `BotError::Other("Not in a guild".into())` is a
common pattern when a command can't proceed because of a missing
argument or a precondition failure.

Conversions live right next to the definition as `From` impls:

```rust
impl From<serenity::Error> for BotError { ... }
impl From<sqlx::Error> for BotError { ... }
impl From<reqwest::Error> for BotError { ... }
impl From<serde_json::Error> for BotError { ... }
impl From<String> for BotError { ... }
```

These `From` impls are the reason command handlers can use `?`
everywhere. `let expires_at = create_tempban(...).await?;` turns a
`sqlx::Error` into a `BotError::Sqlx` and bubbles up, without the
handler having to know what `create_tempban` can fail with. The enum
implements `std::error::Error` and `Display`, so errors also format
sensibly when logged.

There's no `thiserror`, no `anyhow`, no derived `From`. The enum is
small enough that hand-writing the impls is cleaner than pulling in a
macro dependency, and the explicitness makes it obvious what kinds of
errors the bot actually handles.

## Where errors are surfaced

**Command errors.** Poise ties every handler's return value to its
framework error hook. A command returning `Err(BotError::Sqlx(...))`
or `Err(BotError::Other("...".into()))` raises a
`FrameworkError::Command`, which the hook turns into a user-facing
reply and a tracing log. That hook lives in `main.rs` and now uses
`BotError::user_message()` to keep operator-only details out of
chat:

```rust
on_error: |error| Box::pin(async move {
    match error {
        poise::FrameworkError::Command { error, ctx, .. } => {
            // Full error (including upstream sqlx/reqwest text) goes
            // to logs only.
            tracing::error!("Command error: {error}");
            // Sanitised, per-variant copy goes to the user.
            let _ = ctx.say(error.user_message()).await;
        }
        other => {
            tracing::error!("Framework error: {other}");
        }
    }
})
```

The split between `Display` and `user_message()` is deliberate:

- **`Display`** still produces the verbose form (`"Database error:
  <sqlx message>"`, `"HTTP error: <reqwest message>"`, ...) and is
  what gets logged. It carries every byte of upstream context an
  operator might want to grep for.
- **`user_message()`** returns a fixed, generic per-variant string
  ("Something went wrong talking to the database. Please try again
  later.", "Couldn't reach an external service. Please try again.",
  ...) — except for `Other(s)`, which is treated as already-curated
  copy and passed through verbatim. That last case is what makes
  short messages like `"Not in a guild"` and validation errors
  still surface naturally.

The user sees the short, friendly form. The operator sees the full
upstream chain in the logs and can correlate by timestamp.

Other `FrameworkError` variants (permission denied, argument parse
failure, missing subcommand) are logged but not replied to. The
default poise behaviour is to post a short notice for some of these;
the current hook is deliberately minimal, because command errors are
rare and usually only interesting to operators.

**Event handler errors.** Event handlers (message handler, voice
state, component interactions, member join) don't return `Result` in
the usual sense. The top-level `event_handler` function returns
`Result<(), BotError>`, but it never actually returns `Err` — every
sub-handler uses `let _ = ...` to swallow individual errors and
continues. This is because an event handler has no good place to post
an error: the "user" who triggered the event might be a raw gateway
event like a voice state update, not a chat message, so there's
nothing to reply to.

Instead, event handlers emit `tracing::error!` or `tracing::warn!`
at the site of the failure. For example, the auto-role promotion
path inside `handle_message` spawns a task that logs via
`tracing::warn!("Auto-role promotion failed for {}: {}", ...)`. The
user sees nothing; the operator sees the error in the logs.

**Background task errors.** `main.rs` spawns several long-running
tasks (rate-limiter cleanup, tempban unban sweep, auto-role
time-based check, donator sync). Each iteration body runs inside the
`run_supervised` helper, which wraps it in
`AssertUnwindSafe(...).catch_unwind()`. A panic inside one iteration
is caught and logged via `tracing::error!` with the task name and
panic payload, and the outer loop continues to the next iteration —
"a background task should never take the bot down" is now enforced
by the wrapper, not just by convention. Recoverable errors inside the
body still use the same `tracing::warn!` / `tracing::error!` pattern
and continue. See
[Concurrency Model: background task supervision](concurrency-model.md#background-task-supervision)
for the JoinSet plumbing and graceful-shutdown story.

**The AI pipeline.** `handle_mention` doesn't return a `Result` at
all. It uses pattern matching and explicit `return` statements to
exit on failure paths, and posts its own user-visible messages for
things like "Something went wrong talking to the AI." This is by
design: the pipeline has too many recoverable states (classifier
failure, vision failure, censored response, search failure) for the
`?` operator to express naturally, so it handles each one explicitly.
The tool dispatch paths used to compose user-facing replies as
`message.reply(format!("Database error: {e}"))` (and similar for
`reqwest`/`serde_json`/MCP errors), which leaked the same internal
detail the command path now hides. Those reply sites have all been
swept to use the same generic copy as `BotError::user_message()`,
with the full upstream error logged via `tracing::error!` carrying
the failing tool name and guild ID for operator diagnostics.

## Debug vs production

What gets logged and what gets shown to users is split on purpose:

- **Logs** (`tracing::error!`, `tracing::warn!`): the full
  `Display` form of `BotError`, including every byte of upstream
  context. These go to stderr and whatever log aggregator the
  operator has set up. `tracing_subscriber::fmt::init()` in `main.rs`
  is the default config — override with `RUST_LOG` to raise or lower
  the level.
- **User messages** (`ctx.say(...)`, `message.reply(...)`): the
  output of `BotError::user_message()`. A failed DB query becomes
  `"Something went wrong talking to the database. Please try again
  later."`; a flaky upstream API becomes `"Couldn't reach an external
  service. Please try again."`. The user knows something is broken
  and that retrying is reasonable, but no SQL fragment, hostname, or
  serde path leaks to chat.

The split matters because upstream error messages can contain
information that shouldn't be in Discord — file paths, internal
hostnames, table names, partial stack traces from dependencies. The
old `format!("Error: {e}")` path leaked all of that whenever a
handler bubbled up a `BotError::Sqlx` or `BotError::Reqwest`; the
`user_message()` mapping closes that gap. The same swept the
`BotError::Other(format!("...{e}"))` pattern out of the AI tool
dispatch paths so a failing DeepSeek call can't smuggle a raw HTTP
body into chat by way of `Other`.

## Panics

Panics are reserved for one specific case: **startup config validation**.
The `get_env_or_throw` helper in
[`src/config.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/config.rs)
panics if a required environment variable is missing or contains a
placeholder value:

```rust
fn get_env_or_throw(key: &str) -> String {
    let val = env::var(key).unwrap_or_else(|_| panic!("{key} must be set in .env"));
    if val.starts_with("your-") {
        panic!("{key} has placeholder value — set it in .env");
    }
    val
}
```

This is used for `DISCORD_TOKEN`, `CLIENT_ID`, and `GUILD_ID` — the
three variables without which the bot literally cannot connect. A
missing value there is a deployment error that the operator needs to
see immediately, in the clearest possible way, before the process
starts doing real work. The panic message ends up in the process
output and the operator fixes it.

The `database_url`, MCP bind config, and AI API keys do **not** panic.
They fall back to defaults or stay unset, and the features that need
them either disable themselves or warn at first use.

**Optional config is never a panic.** When `config.toml` has a
feature enabled but its corresponding `[feature_name]` section is
missing, `main.rs` warns through `tracing::warn!(...)` and skips the
feature. For example:

```rust
let auto_role_config = if instance_cfg.features.auto_role {
    match &instance_cfg.auto_role {
        Some(cfg) => { /* log, enable */ Some(cfg.clone()) }
        None => {
            tracing::warn!("Auto-role feature enabled but [auto_role] config section missing");
            None
        }
    }
} else {
    None
};
```

The same pattern repeats for the minecraft donator-sync config, the
chargeback config, the join-role config, and the welcome prompt file.
Missing optional config is always a warning plus a disabled feature,
never a crash. Operators can ship a bot with half its features
half-configured and it'll still start — the log just tells them what
they missed.

Runtime panics elsewhere — inside a command handler, an event handler,
or a background task — are considered bugs. If one happens, Tokio will
catch the task panic and log it, and the rest of the runtime will keep
going. The user whose command triggered the panic sees nothing, which
is unpleasant but better than the process exiting.

## Cross-links

- [Data Flow](data-flow.md) — the shape of the call chain that
  produces these errors in the first place.
- [Debugging](../development/debugging.md) — how to read the logs
  and track a failure back to its source.
- [Environment Variables](../configuration/environment-variables.md) —
  the required variables whose absence produces a startup panic.
