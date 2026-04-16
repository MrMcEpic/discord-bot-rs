# Testing

This page is honest about where the test suite is today and what kinds
of tests are most useful to add. The short version: coverage is
minimal, the project compiles cleanly without it, and any well-scoped
unit test is a welcome contribution.

## Where the project is today

A truthful inventory of automated coverage as of the public launch:

- **Main crate (`src/`)** — zero unit tests. `cargo test` passes
  because there's nothing to fail. The crate compiles, clippy passes
  with `-D warnings`, and CI is green; what's missing is asserts.
- **`mcp-gateway/` crate** — ten unit tests in
  [`mcp-gateway/src/routing.rs`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/mcp-gateway/src/routing.rs).
  They cover the `Router::resolve` decision tree (explicit instance
  vs guild lookup vs neither, unknown instance, unknown guild,
  guild-map updates, override semantics). Read those tests before
  writing your first one — they're the canonical example of the
  project's test style.
- **Integration tests** — none. There's no harness that boots the
  bot against a fake Discord, no end-to-end flow against a real
  Postgres in CI.
- **Doc tests** — none worth mentioning.

The reason coverage is thin is straightforward: the codebase grew
fast, the modules that benefit most from tests (pure logic) are small,
and the modules that have the most lines (Discord I/O, voice
playback, AI calls) are the hardest to test without elaborate mocks.
This isn't a defence; it's an explanation. If you want to write tests
for any of it, the door is wide open.

## What's worth testing

Three categories give the best return on effort.

### Pure logic

Anything that takes data in and returns data out, with no I/O, is
trivially testable and tends to host the bugs. Good candidates:

- **`src/util/duration.rs`** — `parse_duration`, `format_duration_ms`,
  `format_track_duration`. The rules are tight ("3d", "2h30m" not
  supported, capped at 365 days, returns `None` on overflow) and
  every consumer of these functions assumes they're correct. Even one
  test per function would help.
- **`src/wordle/game.rs`** — guess scoring (correct/present/absent),
  win/loss detection, `is_valid_word`. All pure.
- **`src/connections/game.rs`** — selection validation, mistake
  counting, "all four found" detection. Pure.
- **`src/autorole.rs`** — `meets_criteria(activity, config)` is a
  small pure decision. Worth a handful of cases (just-old-enough,
  just-enough-messages, both-required, either-required).
- **`src/ai/sanitize.rs`** — strips role markers and prompt-injection
  attempts. Tests would document the threat model.
- **`src/ai/split.rs`** — splits over the 2000-char limit without
  breaking code fences or multi-byte boundaries. Tests for the edge
  cases (a code fence that crosses 2000 chars, an emoji at byte
  1999, etc.) are very high-value.

If you're looking for a first contribution and don't have a feature
in mind, picking one of these and writing five-to-ten test cases is
genuinely useful work.

### Database queries

The `query` / `query_as` runtime helpers don't validate SQL until
they hit a live database. A test that boots an ephemeral Postgres
(via [`testcontainers`](https://crates.io/crates/testcontainers) or
similar), runs the migrations, and exercises each query function in
`src/db/queries.rs` would catch a class of bugs the type system
can't. We don't have this yet and the project would gladly accept it.

### Routing and decision logic in `mcp-gateway`

The gateway is the easiest crate to test because it's almost entirely
pure: parse a request, decide where to send it, forward, return. The
existing ten tests in `routing.rs` cover the resolver. The other
files (`backend.rs`, `server.rs`, `config.rs`) have decision logic
that would benefit from similar tests — particularly request parsing
and the `tools/list` aggregation in `server.rs`.

## What's not worth testing right now

A few areas where the cost-benefit is bad enough that adding tests
isn't recommended without buy-in from the maintainer:

- **The Serenity / poise dispatch path.** Mocking the framework is
  more code than the handlers. Test the inner functions that
  handlers call instead.
- **The `songbird` voice pipeline.** Same problem times ten —
  testing voice would require either a real voice gateway or a
  fixture-heavy mock layer that doesn't exist.
- **Live external API calls** (DeepSeek, Gemini, Finnhub, NYT
  Wordle/Connections). These belong in manual smoke tests, not
  CI. The cost of a flaky test is worse than the cost of a missed
  regression.

## How to add a test

Rust unit tests live alongside the code in a `mod tests` block at the
bottom of the file:

```rust
// src/util/duration.rs

pub fn parse_duration(input: &str) -> Option<i64> { /* ... */ }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_seconds_minutes_hours_days_weeks() {
        assert_eq!(parse_duration("30s"), Some(30_000));
        assert_eq!(parse_duration("5m"), Some(300_000));
        assert_eq!(parse_duration("2h"), Some(7_200_000));
        assert_eq!(parse_duration("3d"), Some(259_200_000));
        assert_eq!(parse_duration("1w"), Some(604_800_000));
    }

    #[test]
    fn rejects_unknown_units() {
        assert_eq!(parse_duration("3y"), None);
        assert_eq!(parse_duration("3"), None);
        assert_eq!(parse_duration(""), None);
    }

    #[test]
    fn caps_at_365_days() {
        assert_eq!(parse_duration("365d"), Some(365 * 86_400_000));
        assert_eq!(parse_duration("366d"), None);
        assert_eq!(parse_duration("53w"), None);
    }
}
```

For an async test (any function that uses `await`), use `tokio::test`
the way the gateway tests do:

```rust
#[tokio::test]
async fn resolve_explicit_instance() {
    let router = test_router();
    let result = router.resolve(Some("bot_b"), None).await.unwrap();
    // ...
}
```

Run them:

```bash
cargo test                                              # main crate
cargo test --manifest-path mcp-gateway/Cargo.toml       # gateway
cargo test util::duration                               # filter by name
cargo test -- --nocapture                               # show println! output
```

CI runs `cargo test` on both crates separately, so once your test
passes locally it'll pass in CI.

## Test naming

Match the existing convention in `mcp-gateway/src/routing.rs`:
descriptive `snake_case` names that say what's expected, not what's
being called. `resolve_unknown_guild_fails` beats `test_resolve_3`.
Your future self reads test names when CI fails.

## Mocking

The project has no mocking infrastructure today. Where it'd be
useful — Serenity HTTP, sqlx queries, reqwest calls — the right
answer for now is usually one of:

- **Refactor to extract pure logic.** Move the decision out of the
  handler into a free function that takes data and returns data,
  then test that.
- **Skip mocking entirely.** Cover the unit, smoke-test the
  integration manually.

If you want to introduce a mocking framework
([`mockall`](https://crates.io/crates/mockall),
[`wiremock`](https://crates.io/crates/wiremock) for HTTP), open an
issue first to discuss — it has long-term maintenance cost and the
project hasn't picked one yet.

## Manual testing

For everything not covered by automation — and right now that's most
of the bot — the manual loop is:

1. Start a local instance with `CONFIG_DIR=instances/local cargo run`.
2. Exercise the change in your test Discord server.
3. Tail the logs (`RUST_LOG=discord_bot=debug,info cargo run`) and
   confirm there's no warning or error you didn't expect.

The PR template's **Testing** section asks you to list what you
manually verified. Be specific — "tested `!m play` and `!m skip`"
is more useful than "tested music."

## Where to look first

When you want to add tests, in roughly this order of cost-benefit:

1. `src/util/duration.rs`
2. `src/ai/split.rs`
3. `src/ai/sanitize.rs`
4. `src/wordle/game.rs`
5. `src/connections/game.rs`
6. `src/autorole.rs`
7. `mcp-gateway/src/server.rs`

A PR that adds five tests to one of those files would be welcome. A
PR that adds the first integration test (Postgres in a container)
would be a milestone.

## Next steps

- [Debugging](debugging.md) — when a test fails and you don't know
  why, start there.
- [Contributing Workflow](contributing-workflow.md) — the pre-PR
  checklist includes `cargo test` on both crates.
