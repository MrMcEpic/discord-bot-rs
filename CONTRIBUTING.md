# Contributing to discord-bot-rs

Thanks for your interest! This doc covers how to get set up, submit changes, and what to expect.

## Ground Rules

- Be kind. We follow the [Contributor Covenant](CODE_OF_CONDUCT.md).
- Small focused PRs are easier to merge than large sprawling ones.
- If you're planning something substantial, [open an issue first](https://github.com/MrMcEpic/discord-bot-rs/issues/new/choose) so we can discuss the approach before you spend time on it.

## Contributor License

By submitting a pull request or other contribution to this project, you grant the project maintainer a perpetual, worldwide, non-exclusive, royalty-free, irrevocable license to use, reproduce, modify, adapt, publish, distribute, and sublicense your contribution as part of this project, including the right to distribute the project under different terms in the future. You represent that you are legally entitled to grant this license — for example, that the work is your own and not subject to a prior obligation (an employer agreement, a third-party license) that conflicts with these terms.

This protects both parties: you know what you're signing up for, and the project has the flexibility to evolve its licensing as the ecosystem changes.

## Development Setup

You need:

- Rust stable (install via [rustup](https://rustup.rs))
- Docker and Docker Compose
- `git`

Clone the repo and build:

```bash
git clone https://github.com/MrMcEpic/discord-bot-rs.git
cd discord-bot-rs
cargo check
cargo check --manifest-path mcp-gateway/Cargo.toml
```

For running locally without Docker, you'll need a PostgreSQL database and a Discord application. See [docs/development/building-locally.md](https://mrmcepic.github.io/discord-bot-rs/development/building-locally.html) for the full path.

The full test suite also needs Postgres: the integration tests under `tests/` use `#[sqlx::test]` to clone a fresh per-test database from `$DATABASE_URL`. Unit tests (`cargo test --bins`) don't need it. The throwaway pattern: `docker run -d --rm -p 5433:5432 -e POSTGRES_USER=test -e POSTGRES_PASSWORD=test -e POSTGRES_DB=test postgres:17`, then run `cargo test` with `DATABASE_URL=postgres://test:test@localhost:5433/test`. See [docs/development/testing.md](https://mrmcepic.github.io/discord-bot-rs/development/testing.html) for the full breakdown.

## Code Style

This project uses standard Rust formatting and clippy lints:

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
```

CI runs both. `clippy -D warnings` means your PR won't merge if clippy flags anything. If you think a warning is a false positive, use a targeted `#[allow(...)]` attribute with a comment explaining why — don't globally suppress.

Rust indentation is hard tabs, width 4. The `rustfmt.toml` in the repo enforces this; running `cargo fmt` applies it.

Unit tests live in a `#[cfg(test)] mod tests` block at the bottom of the file they cover. Postgres-backed integration tests live under `tests/*.rs` and use `#[sqlx::test(migrations = "./migrations")]` to clone a per-test database. The test runner links against the minimal `src/lib.rs` facade — if you need to test a module that isn't exposed yet, add it there.

## Commits

- Use descriptive commit messages. The repo leans toward conventional-ish prefixes (`feat:`, `fix:`, `docs:`, `chore:`, `refactor:`) but doesn't enforce them.
- One logical change per commit. If you're tempted to write "and also" in the commit message, split it.
- Don't force-push to shared branches (`master`). Force-push on your own branches is fine.

## Pull Requests

1. Fork the repo
2. Create a feature branch from `master`
3. Make your changes with tests (where applicable)
4. Run the pre-PR commands locally:
   - `cargo fmt`
   - `cargo clippy --all-targets -- -D warnings`
   - `cargo test --bins` (minimum — unit tests, no DB)
   - `cargo test` with `DATABASE_URL` set (full — runs the integration tests in `tests/`; CI runs this against a `postgres:17` service)
5. Push your branch and open a PR
6. Fill out the PR template

Expect a review within a few days. If you haven't heard back in a week, feel free to ping the PR.

## Adding a Feature

See [docs/development/adding-a-command.md](https://mrmcepic.github.io/discord-bot-rs/development/adding-a-command.html) and [docs/development/adding-a-feature-module.md](https://mrmcepic.github.io/discord-bot-rs/development/adding-a-feature-module.html) for the codebase walkthrough.

## Reporting Bugs

Use the [bug report template](https://github.com/MrMcEpic/discord-bot-rs/issues/new?template=bug_report.yml). Please include logs (redact secrets), your config.toml (redact IDs if you prefer), and reproduction steps.

## Reporting Security Issues

**Do not open a public issue.** See [SECURITY.md](SECURITY.md) for the responsible disclosure process.

## Questions

Open a discussion or a draft PR. Both are fine.
