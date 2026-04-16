# Adding a Feature Module

A "feature module" is what this project calls a top-level directory
under `src/` — `wordle/`, `music/`, `connections/`, `minecraft/`, and
so on. Use this page when your change is bigger than a command: when
you're introducing new state, new background work, new database
tables, or a meaningful new event handler. If all you want is a single
new command in an existing area, the [Adding a Command](adding-a-command.md)
page is the shorter path.

This page walks through building a small feature module called
`reminders` — store reminders in the database, fire them via a
background task, and expose `!m remind` and `!m reminders` commands.
By the end you'll have touched every part of the codebase a real
feature touches and you'll know the conventions for each one.

Read the [Codebase Tour](codebase-tour.md) first if you haven't —
this page assumes you're familiar with `Data`, `BotError`, and the
shape of `commands/mod.rs`.

## What a feature module looks like

Open the `wordle/` directory: `mod.rs`, `game.rs`, `api.rs`,
`embeds.rs`, plus a `words.txt` data file. That's the canonical
shape:

- **`mod.rs`** declares the public submodules and re-exports the
  types other modules need.
- **`game.rs`** (or whatever name fits) holds the pure-Rust state and
  logic — no I/O, no Discord types beyond what's needed for IDs.
- **`api.rs`** wraps any external HTTP calls.
- **`embeds.rs`** builds Discord embeds and component rows.

You don't have to copy that exact split — the music module has
`player.rs`, `track.rs`, `voice.rs`, and `embeds.rs` instead — but
keep the same posture: separate the data, the I/O, and the rendering.

## Step 1: Create the directory

```bash
mkdir src/reminders
touch src/reminders/{mod.rs,model.rs,scheduler.rs}
```

We'll put the reminder data type in `model.rs` and the background
firing logic in `scheduler.rs`. Most features end up with three to
five files; resist the urge to make `mod.rs` itself long.

`src/reminders/mod.rs` is the single re-export and submodule
declaration:

```rust
pub mod model;
pub mod scheduler;

pub use model::Reminder;
```

## Step 2: Wire the module into `main.rs`

Open `src/main.rs`. At the very top, with the other `mod` lines, add
yours in alphabetical order:

```rust
mod ai;
mod autorole;
mod commands;
mod config;
mod connections;
mod db;
mod error;
mod events;
mod instance_config;
mod mcp;
mod minecraft;
mod music;
mod reminders;     // <-- new
mod stocks;
mod util;
mod wordle;
```

Until this line exists, nothing in `src/reminders/` actually compiles.
Forgetting to add it is the most common mistake on a fresh module:
your IDE may show your code as fine, but `cargo build` reports the
files don't exist as a module. The cure is one line.

## Step 3: Decide what state you need on `Data`

Open the `Data` struct in `src/main.rs`. Anything your feature needs
to share across commands, events, and background tasks goes here.
Three kinds of state are common:

- **Per-guild or per-channel state** — use a
  `DashMap<GuildId, Arc<Mutex<T>>>` (see `wordle_games`,
  `connections_games`, `guild_players`).
- **Loaded config** — if your feature is opt-in via `config.toml`,
  add an `Option<YourConfig>` field next to `auto_role_config`,
  `minecraft_config`, etc.
- **One-shot startup flags** — `AtomicBool`s for things that should
  run once. The MCP server uses `mcp_started` for this.

For reminders, the firing logic is in a single background task and
the data lives in Postgres, so we don't need any per-guild map. We do
want optional config — say, a maximum number of pending reminders per
user. So extend `Data`:

```rust
pub struct Data {
    // ... existing fields ...
    pub reminders_config: Option<instance_config::RemindersConfig>,
}
```

And construct it in `setup`:

```rust
Ok(Data {
    // ... existing fields ...
    reminders_config: instance_cfg.reminders.clone(),
})
```

Cheap to clone (it's a small struct), cheap to read, no synchronisation
overhead. If you needed a `DashMap` for live state, you'd add it the
same way — initialised with `Arc::new(DashMap::new())`.

## Step 4: Add a `config.toml` section (if needed)

If your feature is opt-in, `instance_config.rs` is where you teach the
bot to read it. Add a feature flag to the `Features` struct:

```rust
#[derive(Debug, Deserialize, Default)]
pub struct Features {
    #[serde(default)]
    pub minecraft: bool,
    #[serde(default)]
    pub auto_role: bool,
    #[serde(default)]
    pub join_role: bool,
    #[serde(default)]
    pub welcome: bool,
    #[serde(default)]
    pub reminders: bool,    // <-- new
}
```

And the typed config struct alongside the others:

```rust
#[derive(Debug, Deserialize, Clone)]
pub struct RemindersConfig {
    #[serde(default = "default_max_reminders")]
    pub max_per_user: i64,
}

fn default_max_reminders() -> i64 {
    20
}
```

Then add the optional field to `InstanceConfig`:

```rust
pub struct InstanceConfig {
    // ... existing fields ...
    pub reminders: Option<RemindersConfig>,
}
```

Update `instances/example/config.toml` with a commented-out example
block so users discover the option:

```toml
# [features]
# reminders = true
#
# [reminders]
# max_per_user = 20
```

In `main.rs`, follow the existing "feature gated twice" pattern when
loading the config:

```rust
let reminders_config = if instance_cfg.features.reminders {
    match &instance_cfg.reminders {
        Some(cfg) => {
            tracing::info!("Reminders module enabled (max_per_user={})", cfg.max_per_user);
            Some(cfg.clone())
        }
        None => {
            tracing::warn!("Reminders feature enabled but [reminders] config section missing");
            None
        }
    }
} else {
    None
};
```

This is verbose by design — `main.rs` logs every feature's activation
at `info!` and warns loudly when the config is half-set. Copy the
pattern; reviewers will ask for it.

## Step 5: Add a database table

If your feature persists anything, the table belongs in `src/db/`.

In `src/db/mod.rs`, add a `CREATE TABLE IF NOT EXISTS` to `migrate`:

```rust
sqlx::query(
    "CREATE TABLE IF NOT EXISTS reminders (
        id SERIAL PRIMARY KEY,
        guild_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        channel_id TEXT NOT NULL,
        message TEXT NOT NULL,
        fire_at TIMESTAMPTZ NOT NULL,
        fired BOOLEAN NOT NULL DEFAULT FALSE,
        created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
    )",
)
.execute(pool)
.await?;

sqlx::query(
    "CREATE INDEX IF NOT EXISTS idx_reminders_pending ON reminders (fire_at) WHERE fired = FALSE",
)
.execute(pool)
.await?;
```

In `src/db/models.rs`, add a `FromRow` struct:

```rust
#[derive(Debug, sqlx::FromRow)]
pub struct Reminder {
    pub id: i32,
    pub guild_id: String,
    pub user_id: String,
    pub channel_id: String,
    pub message: String,
    pub fire_at: chrono::DateTime<chrono::Utc>,
    pub fired: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
```

In `src/db/queries.rs`, add the functions your feature needs:

```rust
pub async fn create_reminder(
    pool: &PgPool,
    guild_id: &str,
    user_id: &str,
    channel_id: &str,
    message: &str,
    fire_at: chrono::DateTime<chrono::Utc>,
) -> Result<i32, sqlx::Error> {
    let row: (i32,) = sqlx::query_as(
        "INSERT INTO reminders (guild_id, user_id, channel_id, message, fire_at)
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(guild_id)
    .bind(user_id)
    .bind(channel_id)
    .bind(message)
    .bind(fire_at)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

pub async fn get_due_reminders(pool: &PgPool) -> Result<Vec<crate::db::models::Reminder>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM reminders WHERE fired = FALSE AND fire_at <= NOW()")
        .fetch_all(pool)
        .await
}

pub async fn mark_reminder_fired(pool: &PgPool, id: i32) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE reminders SET fired = TRUE WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
```

Notice the project uses the runtime `query` / `query_as` helpers with
`bind`, **not** the compile-time `query!` / `query_as!` macros. The
choice is deliberate: it lets the crate build without a live database
at compile time. Stick with it.

## Step 6: Implement the feature logic

`src/reminders/model.rs` is just a small re-export and any helpers
that don't need DB access:

```rust
pub use crate::db::models::Reminder;

impl Reminder {
    pub fn human_when(&self) -> String {
        crate::util::duration::format_duration_ms(
            (self.fire_at - chrono::Utc::now()).num_milliseconds().max(0),
        )
    }
}
```

`src/reminders/scheduler.rs` holds the firing loop:

```rust
use serenity::all::*;
use std::sync::Arc;
use std::time::Duration;

use crate::db;

pub async fn fire_due_reminders(http: Arc<Http>, db: sqlx::PgPool) {
    match db::queries::get_due_reminders(&db).await {
        Ok(due) => {
            for r in due {
                let Ok(channel_id) = r.channel_id.parse::<u64>() else { continue };
                let _ = ChannelId::new(channel_id)
                    .say(
                        &http,
                        format!("<@{}>: {}", r.user_id, r.message),
                    )
                    .await;
                if let Err(e) = db::queries::mark_reminder_fired(&db, r.id).await {
                    tracing::warn!("Failed to mark reminder {} fired: {e}", r.id);
                }
            }
        }
        Err(e) => tracing::warn!("Reminder fetch failed: {e}"),
    }
}

pub fn spawn_loop(http: Arc<Http>, db: sqlx::PgPool, interval: Duration) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        tracing::info!("Reminder scheduler started ({}s interval).", interval.as_secs());
        loop {
            fire_due_reminders(http.clone(), db.clone()).await;
            tokio::time::sleep(interval).await;
        }
    });
}
```

## Step 7: Spawn the background task

Open `main()` in `src/main.rs`. Below the existing background-task
spawns (tempban unban checker, auto-role checker, donator sync), add
yours, gated on the feature flag:

```rust
if instance_cfg.features.reminders {
    reminders::scheduler::spawn_loop(
        client.http.clone(),
        db_clone.clone(),
        std::time::Duration::from_secs(30),
    );
}
```

Match the conventions of the existing tasks: clone the `Arc`-friendly
parts of state, log on startup, log warnings on per-iteration errors,
and never panic.

## Step 8: Add commands

Create `src/commands/reminders.rs` with one or two prefix subcommands:

```rust
use crate::error::BotError;
use crate::{db, Context};

#[poise::command(prefix_command, rename = "remind")]
pub async fn remind(
    ctx: Context<'_>,
    #[description = "When (e.g. 30m, 2h, 1d)"] when: String,
    #[description = "Reminder message"]
    #[rest]
    message: String,
) -> Result<(), BotError> {
    let Some(ms) = crate::util::duration::parse_duration(&when) else {
        ctx.say("Invalid duration. Try `30m`, `2h`, `1d`.").await?;
        return Ok(());
    };
    let fire_at = chrono::Utc::now() + chrono::Duration::milliseconds(ms);
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
    let id = db::queries::create_reminder(
        &ctx.data().db,
        &guild_id,
        &ctx.author().id.to_string(),
        &ctx.channel_id().to_string(),
        &message,
        fire_at,
    )
    .await?;
    ctx.say(format!("Reminder #{} set.", id)).await?;
    Ok(())
}
```

Then register the command. Open `src/commands/mod.rs` and add the
module declaration plus the subcommand entry:

```rust
pub mod reminders;
// ... existing pub mod lines ...

#[poise::command(
    prefix_command,
    subcommands(
        // ... existing entries ...
        "reminders::remind",
        "help::help",
    )
)]
pub async fn m(_ctx: poise::Context<'_, Data, BotError>) -> Result<(), BotError> {
    Ok(())
}
```

If your feature needs to be **conditionally registered** based on the
TOML config — like `minecraft::verify` — push it into `m_cmd.subcommands`
in `main.rs` after the parent is built:

```rust
let mut m_cmd = commands::m();
if instance_cfg.features.reminders {
    m_cmd.subcommands.push(commands::reminders::remind());
}
```

This keeps the command absent from `!m help` on instances where the
feature is off.

## Step 9: Hook into events (optional)

If your feature needs to react to gateway events — messages, member
joins, button clicks — open `src/events/mod.rs`. The handler is one
big `match` over `FullEvent` variants; add or extend the arm you need.
For a button-driven feature, define a custom-ID prefix
(e.g. `reminder_dismiss_<id>`) and route on it in the
`InteractionCreate` arm the same way `music_*`, `game_*`, and `cb_*`
are routed today.

For reminders the only reactive piece would be cancelling a fired
reminder via a button — small enough that you can leave it for a
follow-up PR.

## Step 10: Document the feature

User-visible features get:

- A page under `docs/features/yourfeature.md`. Match the shape of an
  existing feature page, like `docs/features/auto-role.md`.
- An entry in `docs/SUMMARY.md` under the Features section.
- An entry in `docs/configuration/instance-config.md` if you added a
  `config.toml` section.
- A row in `docs/reference/command-list.md` if you added a command.
- A `CHANGELOG.md` entry under `[Unreleased]` in `Added`.

Documentation is a load-bearing part of the change. PRs that ship a
feature without it tend to bounce in review.

## Step 11: Test the loop

Locally:

```bash
CONFIG_DIR=instances/local cargo run
```

In Discord:

- `!m remind 1m hello` should respond with `Reminder #1 set.` and
  fire a minute later.
- `!m remind banana hello` should respond with `Invalid duration.`
- `!m remind 1y hello` should respond with `Invalid duration.` (the
  parser caps at 365 days).

Add automated coverage where it's cheap. `parse_duration` is already
unit-tested; if your feature has pure logic — a scheduler that picks
which reminders to fire, a permission check, a duration formatter —
test it the same way. See [Testing](testing.md) for the project's
current posture and what kinds of tests are most welcome.

## Common gotchas

- **Forgot the `mod` line in `main.rs`.** Your code doesn't compile,
  with a confusing error. Add `mod reminders;` at the top.
- **Forgot to register the command.** The command compiles, the bot
  boots, and `!m remind` does nothing. Add `"reminders::remind"` to
  the `subcommands(...)` list in `commands/mod.rs`.
- **Held a `DashMap` entry across an `.await`.** Will deadlock under
  load. Look up the entry, clone the `Arc`, drop the guard, then
  await on the clone.
- **Panicked from a background task.** Tasks must never panic — log
  the error with `tracing::warn!` or `tracing::error!` and continue
  the loop. The bot stays up.
- **Missed the feature-gate "verbose" log on startup.** Reviewers
  will ask for it. Copy the pattern from auto-role or minecraft in
  `main.rs`.
- **Skipped the config struct because "the feature is small."** Once
  the feature has any tunable, put it in `instance_config.rs`.
  Hard-coded values become technical debt fast.

## Next steps

- [Adding a Command](adding-a-command.md) — for the next time you
  just need a new subcommand inside an existing area.
- [Adding an MCP Tool](adding-an-mcp-tool.md) — if your feature
  should also be reachable from an MCP client.
- [Testing](testing.md) — where to add tests for the new logic.
- [Contributing Workflow](contributing-workflow.md) — fork → PR →
  merge end-to-end.
