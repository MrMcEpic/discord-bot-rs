# Concurrency Model

discord-bot-rs is a single-process async application. It handles many
simultaneous guilds, commands, and background tasks on one Tokio
runtime, without global locks. This page explains how it stays correct
under concurrent load: which data structures it leans on, where mutex
boundaries sit, and which patterns to copy when you're adding a new
feature.

The design rule is simple: **locks are the last tool, not the first.**
Where a feature can get away with a lock-free concurrent map, it does.
Where it needs serialised access within a single guild or channel, it
uses a narrow `tokio::Mutex` around the minimum amount of state. No
feature in the codebase holds a mutex across a network call, and there
is no global `RwLock<HashMap>`-style state anywhere.

## Tokio runtime

`main.rs` starts the app with `#[tokio::main]`, which gives it a
multi-threaded runtime using one worker thread per CPU by default.
Everything the bot does — gateway events, HTTP requests, DB queries,
yt-dlp subprocesses, MCP server, axum webhook router, background
workers — runs as async tasks on this single runtime. There are no
other runtimes, no threads spawned by `std::thread::spawn`, and no
blocking I/O outside of `spawn_blocking` (which isn't currently used
anywhere).

The result is that scheduling decisions are centralised: Tokio can
starve a slow task without blocking the rest, and `cargo run` boots
into a fully functional bot without any threading ceremony.

## The `Data` struct as shared state

Poise's framework gives every command and event handler a reference to
a user-defined state object. In this project that's `Data`, defined at
the top of
[`src/main.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/main.rs).
It's built once at startup, wrapped in an `Arc` by poise, and handed
out to every handler via `poise::Context`. Cloning an `Arc<Data>` is
a single atomic refcount bump, so passing it into a spawned task is
free.

Inside `Data`, the read-only fields (`db`, `http_client`, `config`,
`personality`, `bot_name`, all the optional feature configs) are
accessed concurrently without any locking — they're either `Arc`-
shared resources (the pool, the HTTP client) or immutable owned
strings. The interesting parts are the mutable per-guild and
per-channel maps.

## `DashMap` for per-guild state

Six of `Data`'s fields are DashMap-based:

| Field | Shape | Feature |
|---|---|---|
| `guild_players` | `DashMap<GuildId, Arc<Mutex<GuildPlayer>>>` | Music |
| `track_handles` | `DashMap<GuildId, TrackHandle>` | Music |
| `now_playing_msgs` | `DashMap<GuildId, Arc<Mutex<Option<MessageId>>>>` | Music |
| `idle_timers` | `DashMap<GuildId, Arc<Mutex<Option<JoinHandle<()>>>>>` | Music |
| `connections_games` | `DashMap<ChannelId, Arc<Mutex<ConnectionsGame>>>` | Games |
| `wordle_games` | `DashMap<ChannelId, Arc<Mutex<WordleGame>>>` | Games |

[DashMap](https://github.com/xacrimon/dashmap) is a sharded concurrent
hash map — internally it splits keys across a fixed number of shards,
each with its own `RwLock`. Lookups of different keys hit different
shards and don't block each other. This is the shape of the workload
here: two guilds' music commands land on different DashMap shards and
run concurrently; even two lookups inside the same guild's DashMap
won't block because the inner value is `Arc<Mutex<T>>` and the outer
map only holds the `Arc`.

Why not `Arc<RwLock<HashMap<GuildId, T>>>`? Because every write — a
user starts a song in guild A — would have to take the write lock on
the outer map, and every read from guild B would either have to wait
or hold a read lock that blocks guild C's write. DashMap eliminates
that global contention by design.

## Per-guild `Mutex<T>` inside DashMap

DashMap gives concurrent key-level access. Once a handler has its
guild's value in hand, it needs a way to serialise access inside that
guild — because a music player is a single state machine and you
don't want the `skip` button to race with the `play` command. The
pattern is to store `Arc<Mutex<T>>` as the value:

```rust
let player_arc = data.guild_players
    .entry(guild_id)
    .or_insert_with(|| Arc::new(Mutex::new(GuildPlayer::new(guild_id))))
    .value()
    .clone();

// Drop the DashMap entry guard before awaiting the inner mutex
let mut player = player_arc.lock().await;
player.enqueue(track);
```

Two important details:

1. **Release the DashMap guard before the await.** `entry(...).value()`
   returns a guard that holds the DashMap shard's lock. Holding it
   while you `.await` on the inner mutex would hold up other handlers
   that need the same shard. The idiom is to clone the inner `Arc`
   out and let the guard drop.
2. **Use `tokio::sync::Mutex`, not `std::sync::Mutex`.** Tokio mutexes
   are designed to be held across `.await` points; std mutexes are
   not. A handler that's holding a `std::sync::Mutex` across an
   `.await` can deadlock the whole runtime if Tokio happens to
   schedule the task back onto the thread that's blocked on the same
   mutex. Every mutex in this codebase is a tokio mutex.

This pattern gives fine-grained concurrency: two guilds can run music
commands in parallel, two channels can run Wordle games in parallel,
and within one guild the music player is still serialised. No feature
module has to coordinate with another, because they use different
DashMaps.

## Idle timers

The music idle timer pattern in
[`src/music/voice.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/music/voice.rs)
is a good example of how to manage cancellable background work in this
style. When a track ends and the queue is empty, the track-end handler
calls `start_idle_timer`, which:

1. Spawns a task that sleeps 300 seconds, then leaves the voice
   channel and cleans up.
2. Stores the task's `JoinHandle` inside `Data::idle_timers` at the
   guild's entry.

When the next track starts — or the user calls `!m stop` — the code
calls `cancel_idle_timer`, which takes the handle out of the mutex and
calls `.abort()` on it. Cancellation is atomic: either the task
already ran and there's nothing to abort, or it was sleeping and the
`.abort()` drops its future.

The `idle_timers` DashMap's value type is
`Arc<Mutex<Option<JoinHandle<()>>>>`. The `Option` is there because a
guild can be in the map without having an active timer (the slot
exists but is empty). The `Mutex` protects the slot from the "start a
new timer while the old one is being cancelled" race.

## Rate limiting

[`src/util/ratelimit.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/src/util/ratelimit.rs)
implements a sliding-window limiter using — unsurprisingly — a
DashMap:

```rust
pub struct SlidingWindowLimiter {
    buckets: DashMap<String, Vec<Instant>>,
    max_requests: usize,
    window: Duration,
}
```

The key is arbitrary (in practice, `user_id.to_string()`). The value
is a vector of timestamps. When `check` is called, it prunes
timestamps older than the window, then either returns `0` (allowed,
append the current timestamp) or the seconds until the oldest
timestamp expires (rate limited).

Because `DashMap::entry` gives unique access to one slot, two
concurrent `check` calls for the *same* user serialise naturally
through the entry guard. Two calls for *different* users land on
different shards and don't block each other.

`Data::rate_limiters` holds four of these:

- **`ai`** — 10 requests per 60 seconds, used by the AI chat
  pipeline.
- **`music`** — 15 requests per 30 seconds. Defined but not currently
  enforced.
- **`moderation`** — 5 requests per 60 seconds. Enforced only by the
  AI pipeline's moderation tool execution path. The prefix
  `!m ban` / `!m unban` / `!m nuke` commands do not currently consult
  this limiter (Discord-side permission checks are the only gate).
- **`stocks`** — 10 requests per 30 seconds. Defined but not currently
  enforced.

## Database pool concurrency

Sqlx's `PgPool` is itself concurrent: it holds a bounded number of
connections, hands them out to tasks that need them, and queues
waiters when the pool is saturated. A handler that awaits a query
yields the task to Tokio until a connection is free; no thread is
blocked. That means running 50 simultaneous commands on 5 Postgres
connections is fine — 45 of them will be parked, waiting their turn,
while other tasks on the runtime continue unhindered.

Because the `after_connect` hook sets `search_path` per connection
(see [Multi-Instance Model](multi-instance-model.md)), every query
transparently lands in the right schema without per-query
parameterisation. There is no per-query lock; sqlx handles
concurrency through the pool.

## Voice concurrency

Songbird runs its own audio processing inside the Tokio runtime. It
spawns internal tasks for gateway traffic, UDP packets, Opus
encoding, and track event dispatch. The bot's main event handlers
only talk to songbird through its API: `manager.join`,
`handler.play_input`, `handler.stop`, and `track_handle.add_event`.
All of those are non-blocking control calls. The actual audio
pipeline runs on background tasks owned by songbird, so a slow
handler on the main runtime doesn't stutter playback.

## What NOT to do

A few patterns are actively avoided:

- **Don't use `std::sync::Mutex` in async code.** As explained above,
  holding one across an `.await` can deadlock the runtime. The only
  place in the codebase that uses `std::sync` primitives is
  `AtomicBool`, which is lock-free.
- **Don't hold a DashMap entry guard across an `.await`.** Clone the
  `Arc` out and release the guard first. This keeps shard-level
  contention short and avoids mysterious stalls.
- **Don't invent a global `RwLock<HashMap<GuildId, T>>`.** Use
  DashMap. If you find yourself wanting a global lock, reconsider the
  shape of your state: it probably should be keyed by guild or
  channel.
- **Don't block in a handler.** Anything that would normally block
  (reading a file, running a subprocess, parsing a big input) should
  either be async (`tokio::fs`, `tokio::process`) or wrapped in
  `tokio::task::spawn_blocking`. The `yt-dlp` integration goes
  through `tokio::process::Command`, which is the right pattern.
- **Don't share `!Send` state across tasks.** Every tokio task must
  be `Send`, so anything held across `.await` inside a task must be
  too. Tokio's mutex guards satisfy this; `Rc` and `RefCell` do not.

## Cross-links

- [Data Flow](data-flow.md) — the lifecycle that these patterns are
  serving.
- [Music Pipeline](music-pipeline.md) — the most elaborate example of
  the patterns on this page.
- [AI Pipeline](ai-pipeline.md) — the other heavy user of the rate
  limiter and per-user state.
- [Multi-Instance Model](multi-instance-model.md) — why none of this
  contention crosses instance boundaries.
