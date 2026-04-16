# Testing

This page describes the test suite as it stands today: how it's
structured, how to run it, what it covers, and where the gaps still
are. Coverage grew substantially during the v0.5.0 hardening cycle —
the crate went from zero tests to a little over a hundred — so the
tone of this page is no longer "we wish we had tests." It's "here's
how to run them and where to add more."

## Current coverage

A truthful inventory as of `v0.5.0`:

- **Main crate unit tests — 92.** Live alongside the code they cover
  in `#[cfg(test)] mod tests` blocks at the bottom of each file.
  Split across `src/util/duration.rs`, `src/util/ratelimit.rs`,
  `src/ai/dsml.rs`, `src/ai/sanitize.rs`, `src/ai/split.rs`,
  `src/error.rs`, `src/wordle/game.rs`, `src/connections/game.rs`,
  `src/autorole.rs`, and the `parse_duration_secs` helper on the MCP
  tool surface.
- **`mcp-gateway/` unit tests — 10.** In
  [`mcp-gateway/src/routing.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/mcp-gateway/src/routing.rs),
  covering the `Router::resolve` decision tree (explicit instance,
  guild lookup, unknown instance, guild-map updates, override
  semantics). The canonical example of the project's test style for
  pure async logic.
- **Main crate integration tests — 18.** Under
  [`tests/`](https://github.com/MrMcEpic/discord-bot-rs/tree/master/tests)
  as four files (`db_stocks.rs`, `db_autorole.rs`, `db_moderation.rs`,
  `db_settings.rs`) driven by `#[sqlx::test]`. They require a running
  Postgres — see below.
- **Doc tests — none worth mentioning.**

Total: **120 automated tests.** CI runs them all on every push and PR.

Two of the integration tests deserve their own call-out because they
exist to pin specific regressions:

- `db_stocks::stocks_reset_sell_race_does_not_mint_money` — ten
  iterations of a concurrent `sell_stock` + `reset_portfolio` race.
  Confirms the `FOR UPDATE` row-lock fix (Tier 1.2) still holds; if
  the lock ever regresses, this test mints money and turns red.
- `db_autorole` — sixteen parallel tasks all trying to claim a role
  for the same user. Verifies the atomic-claim path (Tier 2.x)
  doesn't double-assign.

If you touch the stock-trading SQL layer or autorole flow, run these
tests before opening a PR.

## How unit tests are structured

Every module that has pure logic worth testing carries its tests in
the same file, under `#[cfg(test)] mod tests`. That's the whole
pattern — there's no `tests/` subdirectory inside `src/`, no separate
crate for fixtures, no shared helpers (yet). When you add a new
function worth testing, add the tests to the same file.

```rust
// src/util/duration.rs

pub fn parse_duration(input: &str) -> Option<i64> { /* ... */ }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_units() {
        assert_eq!(parse_duration("30s"), Some(30_000));
        assert_eq!(parse_duration("5m"), Some(300_000));
        assert_eq!(parse_duration("2h"), Some(7_200_000));
    }

    #[test]
    fn rejects_unknown_units() {
        assert_eq!(parse_duration("3y"), None);
        assert_eq!(parse_duration(""), None);
    }
}
```

For async tests, use `tokio::test` the way the gateway's routing
tests do:

```rust
#[tokio::test]
async fn resolve_explicit_instance() {
    let router = test_router();
    let result = router.resolve(Some("bot_b"), None).await.unwrap();
    // ...
}
```

## How integration tests work

The four files under `tests/` use `sqlx`'s test macro:

```rust
#[sqlx::test(migrations = "./migrations")]
async fn buy_stock_decrements_cash_and_creates_holding(pool: PgPool) {
    queries::get_or_create_portfolio(&pool, "test-guild", "test-user").await.unwrap();
    let total = queries::buy_stock(&pool, "test-guild", "test-user", "AAPL", d("2"), d("100"))
        .await
        .unwrap();
    assert_eq!(total, d("200"));
    // ...
}
```

`#[sqlx::test(migrations = "./migrations")]` does three things per
test: clones a fresh database from the `DATABASE_URL` target, applies
every file under `./migrations/` into it, and passes the resulting
`PgPool` into the test function. Tests run in parallel against
independent databases, so there's no ordering coupling or teardown
work to write.

The tests link against the bot's own library crate — a minimal
`src/lib.rs` facade that exposes `pub mod db;` and `pub mod stocks;`.
The binary (`src/main.rs`) is unchanged; the library exists purely so
`tests/*.rs` can call `discord_bot::db::queries::*` without reaching
into private modules. If you need another module testable, add it to
`src/lib.rs` — but keep the surface narrow (no Discord context, no
Songbird, no MCP handlers).

## Running tests locally

The unit tests don't touch the database. The integration tests do.
So there are two useful commands:

```bash
# Unit tests only, no Postgres needed:
cargo test --bins

# Full suite, requires a Postgres reachable at $DATABASE_URL:
cargo test
```

The easiest way to get a throwaway Postgres for the full suite:

```bash
docker run -d --rm --name dbrs-test-pg -p 5433:5432 \
    -e POSTGRES_USER=test \
    -e POSTGRES_PASSWORD=test \
    -e POSTGRES_DB=test \
    postgres:17

DATABASE_URL=postgres://test:test@localhost:5433/test cargo test
```

Stop it with `docker stop dbrs-test-pg` when done. `#[sqlx::test]`
creates a fresh per-test database, so the container can be reused
across `cargo test` invocations — nothing accumulates.

The gateway crate runs independently:

```bash
cargo test --manifest-path mcp-gateway/Cargo.toml
```

Other useful invocations:

```bash
cargo test util::duration          # run tests matching a name
cargo test --test db_stocks        # run one integration file
cargo test -- --nocapture          # show println! output
```

## How CI runs tests

[`ci.yml`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/.github/workflows/ci.yml)'s
`check-main` job stands up a `postgres:17` service container with a
health check, then exports `DATABASE_URL` before running `cargo test`.
Both unit and integration tests run in one invocation. If the
container isn't healthy when the test step starts, the job fails
outright — we don't fall through to running unit tests only.

The `check-gateway` job runs `cargo test` inside `mcp-gateway/` with
no services; the gateway's tests are pure and don't need a DB.

## What's tested

- Pure data transforms: `parse_duration` / `format_duration_ms` /
  `format_track_duration`, `parse_duration_secs` (MCP-side), token
  bucket arithmetic in `util::ratelimit`, DSML parsing, AI message
  splitting across the 2000-char boundary, prompt-injection scrub in
  `ai::sanitize`, `error::user_message` fallout.
- Wordle game state: guess scoring (correct/present/absent), win/loss
  detection, `is_valid_word`.
- Connections game state: selection validation, mistake counting,
  full-category detection.
- Autorole: both the pure `meets_criteria` decision and the atomic
  DB claim.
- Stock trading SQL: buy, sell (partial and full), portfolio reset,
  transaction log, and the concurrency-sensitive reset/sell race.
- Moderation SQL: warnings, history queries, expiry sweeps.
- Instance-settings SQL: round-trip reads/writes of guild settings.
- Gateway routing: the `Router::resolve` decision tree.

## What isn't tested

Being honest about the gaps:

- **Discord-context-dependent handlers.** Anything that needs a
  `Context` or `CommandInteraction` from poise/Serenity. Mocking the
  framework is more code than the handler; the pattern is to extract
  the inner decision as a free function and test that instead.
- **The `songbird` voice pipeline.** Requires a real voice gateway or
  a fixture-heavy mock that doesn't exist.
- **Live external API calls** — DeepSeek, Gemini, Finnhub, NYT.
  These belong in manual smoke tests, not CI. The cost of flake is
  worse than the cost of a missed regression.
- **`mcp-gateway` `backend.rs` / `server.rs`.** The router is tested;
  the request-parse and `tools/list` aggregation paths aren't yet.
  Good first-PR territory.

## Known quirks pinned by tests (not bugs, yet)

Several tests encode present behaviour that's arguably wrong but
hasn't been changed to avoid bundling a fix into a "just add tests"
PR. If you're going to fix one of these, write the test-change and
the code-change in the same PR so the intent is clear:

- `parse_duration("0s")` returns `Some(0)` — a zero-length duration.
  Consumers treat it as "no timeout," which may not be what the user
  typing `0s` meant.
- `parse_duration_secs` (MCP tool helper) silently accepts negative
  values and can overflow on large inputs; the test pins the current
  saturating behaviour.
- `sanitize_content` strips role markers and prompt-injection
  attempts but does **not** scrub bot tokens or other high-entropy
  secrets that slip into AI context. The test suite documents the
  current threat model rather than an aspirational one.
- `format_duration_ms` doesn't clamp negative inputs — it renders
  them with a leading minus. Fine for the display sites that guard
  against negatives upstream, dubious as a general-purpose helper.
- `ConnectionsGame::AlreadyGuessed` is dead-code today (no call path
  constructs it). A test asserts it exists so nobody deletes it
  during a cleanup before the feature that was going to produce it
  lands.
- `submit_guess` with fewer than four tiles selected is a no-op
  rather than an error. Tests pin the no-op behaviour; change it
  deliberately if needed.

## Adding tests

**For pure logic**, drop a `#[cfg(test)] mod tests` block at the
bottom of the file and add `#[test]` functions. If the code under
test is async, use `#[tokio::test]`. No ceremony.

**For new SQL queries**, add a file under `tests/` named for the
module (e.g. `tests/db_my_feature.rs`). Pattern:

```rust
use sqlx::PgPool;
use discord_bot::db::queries;

#[sqlx::test(migrations = "./migrations")]
async fn my_query_does_the_thing(pool: PgPool) {
    let result = queries::my_query(&pool, "guild", "user").await.unwrap();
    assert_eq!(result, /* ... */);
}
```

If the module you want to test isn't reachable through
`discord_bot::…` yet, add it to `src/lib.rs`. Keep the library
surface narrow: only modules that genuinely benefit from
Postgres-backed integration testing belong there.

For **race tests**, follow `stocks_reset_sell_race_does_not_mint_money`
as a template — set up the scenario, spawn two `tokio::spawn` tasks,
await both, then assert the invariant on the final state regardless
of which task won.

## Test naming

`snake_case` names that say what's expected, not what's being called.
`resolve_unknown_guild_fails` beats `test_resolve_3`.
`buy_stock_rejects_insufficient_funds` beats `test_buy_2`. Your future
self reads test names when CI fails.

## Manual testing

Automation still doesn't cover most of the bot — anything that needs
a live Discord connection, voice pipeline, or external API. The
manual loop:

1. Start a local instance with `CONFIG_DIR=instances/local cargo run`.
2. Exercise the change in your test Discord server.
3. Tail the logs (`RUST_LOG=discord_bot=debug,info cargo run`) and
   confirm there's no warning or error you didn't expect.

The PR template's **Testing** section asks you to list what you
manually verified. "Tested `!m play` and `!m skip` against a real
voice channel" is more useful than "tested music."

## Next steps

- [Debugging](debugging.md) — when a test fails and you don't know
  why, start there.
- [Contributing Workflow](contributing-workflow.md) — the pre-PR
  checklist tells you which `cargo test` invocation to run when.
