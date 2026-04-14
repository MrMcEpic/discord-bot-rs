# Adding a Command

This page walks through adding a new command to the bot from scratch.
By the end you'll have a working `!m echo <text>` command, understand
where it's registered, and know the handful of gotchas that catch
first-time contributors.

Before you start, read the [Codebase Tour](codebase-tour.md) — at least
the sections on `main.rs`, `commands/`, and the `Data` struct. This
page assumes you have a local build working (see
[Building Locally](building-locally.md)).

## One thing you need to know up front

**This bot has no slash commands.** Every user-facing command is a
prefix command, and every prefix command is a subcommand of a single
parent command named `m`. So the user types `!m play`, `!m wordle`,
`!m help`, and so on, and the framework dispatches to a subcommand
registered on that parent.

There are two consequences:

1. Your `#[poise::command(...)]` attribute should use
   `prefix_command`, not `slash_command`.
2. "Register the command" means adding it to the `subcommands(...)`
   list on the parent `m` command in
   [`src/commands/mod.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/commands/mod.rs).
   It does **not** mean touching `src/main.rs`. `main.rs` only ever
   registers the single parent `m`; every real command is reached
   through it.

Forgetting step 2 is the single most common mistake: you write the
function, `cargo check` passes, the bot boots, and your command
silently doesn't exist because nothing ever wired it into the parent.

## Choose a file

Commands are grouped by area under
[`src/commands/`](https://github.com/MrMcEpic/discord-bot-rs/tree/master/src/commands).
The existing files are `admin.rs`, `connections.rs`, `help.rs`,
`minecraft.rs`, `moderation.rs`, `music.rs`, `stocks.rs`, and
`wordle.rs`.

If your command fits an existing category, add it to the matching file.
If it doesn't — say, you're adding a brand-new feature — create a new
file and add `pub mod yourfeature;` at the top of
[`src/commands/mod.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/commands/mod.rs)
before the subcommands list.

For this walkthrough we'll add an `echo` command. We'll put it in a
new file, `src/commands/echo.rs`, so you can see the full wiring.

## Write the function

Here's the complete command:

```rust
// src/commands/echo.rs
use crate::error::BotError;
use crate::Context;

/// Echo a message back to the channel
#[poise::command(prefix_command, rename = "echo", aliases("say"))]
pub async fn echo(
    ctx: Context<'_>,
    #[description = "Text to echo"]
    #[rest]
    text: String,
) -> Result<(), BotError> {
    if text.trim().is_empty() {
        ctx.say("Give me something to echo.").await?;
        return Ok(());
    }

    ctx.say(&text).await?;
    Ok(())
}
```

A few things to notice:

- **The `Context<'_>` alias** comes from `crate::Context`, defined at
  the bottom of `src/main.rs` as `poise::Context<'_, Data, BotError>`.
  Use the alias.
- **`prefix_command`** tells poise this is a message-content command,
  not a slash command.
- **`rename = "echo"`** makes the invoked name `echo` instead of the
  function name. In this case they match, so `rename` is redundant —
  but most commands in the codebase use it for clarity and to keep
  the Rust function name free of reserved words (for example,
  `loop_cmd` renamed to `loop`).
- **`aliases("say")`** lets users type `!m say` as an alternative.
  Aliases are short.
- **`#[description = "..."]`** on parameters is required for any
  command that might ever have a generated help page. Provide it for
  every parameter.
- **`#[rest]`** on a `String` parameter tells poise to take the rest
  of the message as one argument instead of splitting on whitespace.
  Without `#[rest]`, `!m echo hello world` would fail with "too many
  arguments." With `#[rest]`, it works.
- **Return `Result<(), BotError>`** — always. All errors convert
  through `BotError`'s `From` impls, so `?` works on serenity,
  sqlx, reqwest, and serde_json errors.

## Register the command

Open
[`src/commands/mod.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/commands/mod.rs).
It looks like this:

```rust
pub mod admin;
pub mod connections;
pub mod help;
pub mod minecraft;
pub mod moderation;
pub mod music;
pub mod stocks;
pub mod wordle;

use crate::error::BotError;
use crate::Data;

#[poise::command(
    prefix_command,
    subcommands(
        "music::play",
        "music::playlist",
        // ... many more ...
        "help::help",
    )
)]
pub async fn m(_ctx: poise::Context<'_, Data, BotError>) -> Result<(), BotError> {
    Ok(())
}
```

Make two edits. First, add your new module at the top of the file:

```rust
pub mod echo;
```

Second, add your command to the `subcommands(...)` list:

```rust
subcommands(
    // ... existing entries ...
    "echo::echo",
    "help::help",
)
```

The string is `"<module>::<function>"`. Order in the list doesn't
affect functionality, but match the grouping of nearby commands if you
can — it keeps the help output tidy.

That's it. `cargo check`, rebuild, restart the bot, and `!m echo
hello` works.

## Parameters

Poise supports most things you'd expect from an argument parser.

### Optional parameters

Wrap the type in `Option<T>`:

```rust
pub async fn echo(
    ctx: Context<'_>,
    #[description = "Text to echo (defaults to a greeting)"]
    text: Option<String>,
) -> Result<(), BotError> {
    let text = text.unwrap_or_else(|| "hello!".into());
    ctx.say(&text).await?;
    Ok(())
}
```

### Discord types

Poise auto-parses serenity model types: `serenity::all::Member`,
`User`, `Role`, `Channel`. See `src/commands/moderation.rs` — the
`ban` command takes `target: serenity::all::Member` directly:

```rust
pub async fn ban(
    ctx: Context<'_>,
    #[description = "User to ban"] target: serenity::all::Member,
    #[description = "Duration (e.g. 3d, 2h, 1w)"] duration_str: String,
    #[description = "Reason"]
    #[rest]
    reason: Option<String>,
) -> Result<(), BotError> { /* ... */ }
```

`!m ban @user 3d flood` gets parsed into three typed arguments.

### Integers and booleans

`i64`, `u64`, `bool` all work out of the box. For enum inputs, define
an `enum` and derive `poise::ChoiceParameter`.

### Permission gates

Use the `required_permissions` attribute to restrict the command:

```rust
#[poise::command(
    prefix_command,
    rename = "setlog",
    required_permissions = "ADMINISTRATOR"
)]
```

See `src/commands/admin.rs` and `src/commands/moderation.rs` for the
pattern. Users who lack the permission get a clean "missing
permissions" error, and you don't have to check in the function body.

## Reading `Data`

Every command has access to the shared `Data` struct through
`ctx.data()`, which returns `&Data`. A few common patterns:

```rust
let db = &ctx.data().db;            // sqlx::PgPool
let http = &ctx.data().http_client; // reqwest::Client
let bot_name = &ctx.data().bot_name;
let personality = &ctx.data().personality;

// Feature configs are Option<T>:
if let Some(cfg) = &ctx.data().auto_role_config {
    // feature is enabled
}
```

For per-guild state (music players, active games), use the matching
`DashMap` on `Data`. See `get_or_create_player` in
`src/commands/music.rs` for the standard lookup-or-insert pattern.

## Responding to the user

Poise gives you a few ways to reply:

- **`ctx.say("...")`** — the simplest: send a text message to the
  current channel.
- **`ctx.reply("...")`** — like `say`, but uses Discord's reply
  feature so the message shows as a reply to the invoker.
- **`ctx.send(poise::CreateReply::default().embed(e).components(v))`**
  — the full builder. Use this when you want an embed, buttons,
  ephemeral flag, or anything beyond text.

For an **ephemeral** reply (only the invoker sees it):

```rust
ctx.send(
    poise::CreateReply::default()
        .content("This is only visible to you.")
        .ephemeral(true),
).await?;
```

Note that ephemeral replies only work in certain contexts in prefix
commands; if yours doesn't render ephemerally, fall back to plain
replies or DMs.

## Slow commands: defer first

If your command takes more than about three seconds — say, it hits an
HTTP API or spawns a subprocess — call `ctx.defer_or_broadcast()`
before the slow work:

```rust
pub async fn play(
    ctx: Context<'_>,
    #[description = "Song name or URL"]
    #[rest]
    query: String,
) -> Result<(), BotError> {
    // ... cheap checks ...

    ctx.defer_or_broadcast().await?;

    // ... resolve the track, join voice, start playback ...
}
```

For prefix commands, `defer_or_broadcast` sends a typing indicator so
the channel knows the bot is working on it. See the `play` command in
`src/commands/music.rs` for the pattern.

## Feature-gating your command

If your command belongs to an optional feature, check the feature flag
before doing work:

```rust
let cfg = match ctx.data().auto_role_config.as_ref() {
    Some(c) => c,
    None => {
        ctx.say("Auto-role isn't enabled on this instance.").await?;
        return Ok(());
    }
};
```

If the feature is **conditionally registered** at startup — like the
Minecraft `verify` subcommand, which is only pushed into `m` when
`features.minecraft` and `minecraft.verify` are both true — follow the
same pattern as in `src/main.rs`:

```rust
let mut m_cmd = commands::m();
if instance_cfg.features.minecraft {
    if let Some(ref mc) = instance_cfg.minecraft {
        if mc.verify {
            m_cmd.subcommands.push(commands::minecraft::verify());
        }
    }
}
```

This keeps the command completely absent from `!m help` on instances
where the feature is off.

## Rate limiting

The bot has a `RateLimiters` bundle on `Data` with four sliding-window
limiters: `ai`, `music`, `moderation`, and `stocks`. If your command
should be rate limited, pick the closest bucket (or add a new one in
`src/util/ratelimit.rs`) and check it at the top of the function:

```rust
let cooldown = ctx
    .data()
    .rate_limiters
    .music
    .check(&ctx.author().id.to_string());
if cooldown > 0 {
    ctx.say(format!("Rate limited — try again in {cooldown}s.")).await?;
    return Ok(());
}
```

`check` returns `0` if the call is allowed and the number of seconds
until reset if not. See `ai/deepseek.rs` for real call sites.

## Testing it

Rebuild and run the bot locally (see
[Building Locally](building-locally.md)). The fast loop is:

```bash
cargo run
# in another terminal, wait for "Starting bot..."
```

In Discord, test the command you just added. Prefix commands are
immediate — there's no global sync delay the way slash commands have,
which is one reason this project uses them exclusively.

A manual test plan for `!m echo`:

- `!m echo hello world` → the bot replies with `hello world`.
- `!m say hello` (the alias) → same thing.
- `!m echo` (no text) → the bot replies with "Give me something to
  echo."
- `!m echo` with multi-word text containing punctuation → parses
  correctly because of `#[rest]`.

Before opening a PR, run:

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

CI runs the same commands; getting them green locally saves a round
trip.

## Common gotchas

- **Forgot the `subcommands` entry.** Your command compiles, the bot
  boots clean, and `!m echo` does nothing. Go back to
  `src/commands/mod.rs` and add `"echo::echo"` to the list.
- **Missed `#[rest]`.** Multi-word arguments fail with "too many
  arguments." Add `#[rest]` to the last `String` parameter.
- **Used `slash_command`.** Slash commands aren't enabled in this
  crate's framework builder, so your command silently never
  registers. Use `prefix_command`.
- **Returned `anyhow::Error` or a custom type.** Poise needs the error
  type to match `Data`'s error parameter — return
  `Result<(), BotError>`, and let `?` convert from whatever you're
  calling.
- **Held a `DashMap` entry across an `.await`.** This will deadlock or
  panic. Clone the inner `Arc` out, drop the entry guard, then await.
- **Didn't add a description.** Commands without
  `#[description = "..."]` on their parameters compile fine but look
  terrible in any generated help.

## Worked examples in the codebase

When in doubt, copy from a command that already works:

- **Simplest**:
  [`commands/help.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/commands/help.rs)
  — no parameters, renders an embed.
- **Parameters and permission gating**: the `ban` command in
  [`commands/moderation.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/commands/moderation.rs).
- **Defers for slow work**: the `play` command in
  [`commands/music.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/commands/music.rs).
- **Own subcommands**: `wordle` in
  [`commands/wordle.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/commands/wordle.rs)
  — `!m wordle` plays today's puzzle, `!m wordle random` picks one.
- **Conditionally registered**: `verify` in
  [`commands/minecraft.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/commands/minecraft.rs),
  pushed into `m` only when enabled in `config.toml`.

## Next steps

- [Adding a Feature Module](adding-a-feature-module.md) — if your
  command is the tip of an iceberg and you need to create a whole new
  `src/<yourfeature>/` directory with state, config, and event
  handlers, start there next.
- [Testing](testing.md) — how and where to add unit tests.
- [Contributing Workflow](contributing-workflow.md) — the end-to-end
  fork → PR → merge flow.
