# Contributing Workflow

This page describes the end-to-end flow of contributing a change to
discord-bot-rs, from the moment you have an idea to the moment your
change ships in a release. It complements the top-level
[CONTRIBUTING.md](https://github.com/MrMcEpic/discord-bot-rs/blob/master/CONTRIBUTING.md),
which is the canonical short version — read that first. This page adds
the in-the-weeds detail you don't want to bury in a root-level file.

## Before you start

**Read the ground rules.** Be kind, keep PRs focused, and if you're
planning anything substantial — a new feature, a module reshuffle, a
dependency bump with downstream impact — open an issue first so
nobody spends a week on an approach that gets rejected at review.

**Check existing issues and PRs** before opening a duplicate. Search
[issues](https://github.com/MrMcEpic/discord-bot-rs/issues) and
[pull requests](https://github.com/MrMcEpic/discord-bot-rs/pulls).

**Bugs** go through the
[bug report template](https://github.com/MrMcEpic/discord-bot-rs/issues/new?template=bug_report.yml):
version/commit, reproduction steps, redacted logs, deployment method.
Filling it out honestly is worth more than any clever fix — half the
time the steps you write reveal the bug.

**Feature requests** go through the
[feature template](https://github.com/MrMcEpic/discord-bot-rs/issues/new?template=feature_request.yml).
Describe the problem before the proposal.

**Security issues** go through
[SECURITY.md](https://github.com/MrMcEpic/discord-bot-rs/blob/master/SECURITY.md)
— don't open a public issue for anything that could be a vulnerability.

## Fork and branch

The project follows the standard GitHub fork + feature-branch flow.

1. Fork [MrMcEpic/discord-bot-rs](https://github.com/MrMcEpic/discord-bot-rs).
2. Clone your fork locally.
3. Add the upstream remote:
   ```bash
   git remote add upstream https://github.com/MrMcEpic/discord-bot-rs.git
   ```
4. Create a feature branch off `master`:
   ```bash
   git checkout -b fix/wordle-expiry-bug master
   ```

Branch names don't have a strict format, but descriptive ones like
`feat/stock-alerts`, `fix/music-skip-deadlock`, or `docs/add-mcp-guide`
are easier to review than `patch-1`.

## Local setup

Follow [Building Locally](building-locally.md) to get a working
`cargo run`. The short version: install the Rust stable toolchain,
make sure Docker and Docker Compose are available for Postgres, and
run `cargo check` plus
`cargo check --manifest-path mcp-gateway/Cargo.toml` to pull down
dependencies and confirm the tree compiles.

If you don't have a Discord application yet, follow the
[Prerequisites](../getting-started/prerequisites.md) page; you'll need
it to test your changes live.

## Make the change

Follow the posture of the file you're editing — the existing code is
the style guide. When in doubt:

- **Add a test if reasonable.** The crate ships with ~120 automated
  tests (92 unit in main, 10 in the gateway, 18 Postgres-backed
  integration tests under `tests/`). Pure logic and SQL queries are
  well-covered; Discord-context handlers and the voice pipeline
  aren't, so PRs that move the needle there are particularly welcome.
  For a bug fix, a test that reproduces the bug is the best comment
  on the diff. See [Testing](testing.md) for the patterns.
- **Keep the PR focused.** One logical change per PR. A refactor and
  a feature and a docs rewrite are three PRs, not one.
- **Update the docs.** If you changed a command, update the
  [command list](../reference/command-list.md). If you changed a
  feature's behaviour, update its page under
  [`docs/features/`](https://github.com/MrMcEpic/discord-bot-rs/tree/master/docs/features).
  New config options go in
  [instance-config.md](../configuration/instance-config.md).
- **Add a CHANGELOG entry.** User-visible changes go under
  `[Unreleased]` in
  [CHANGELOG.md](https://github.com/MrMcEpic/discord-bot-rs/blob/master/CHANGELOG.md)
  in one of `Added`, `Changed`, `Fixed`, or `Removed`.

## Commit style

The project leans toward conventional-style prefixes but doesn't
enforce them with a hook. Common prefixes:

- `feat:` — a new user-visible feature or command
- `fix:` — a bug fix
- `docs:` — documentation only
- `chore:` — build, CI, deps, or repo housekeeping
- `refactor:` — code change with no behaviour change
- `test:` — adding or fixing tests

One logical change per commit. If you're tempted to write "and also"
in the commit message, split it with `git reset` or `git add -p`.

Don't force-push to `master` on your fork — that's fine on your own
feature branches, but never anywhere shared.

## Pre-PR checklist

Before you push and open a PR, run through:

- [ ] `cargo fmt`
- [ ] `cargo fmt --check` (same thing, but catches files you forgot
      to stage)
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test --bins` (minimum — unit tests, no Postgres needed)
- [ ] `cargo test` with a `DATABASE_URL` pointing at a Postgres
      (full — runs the integration tests under `tests/`). Easy
      throwaway: `docker run -d --rm -p 5433:5432 -e POSTGRES_USER=test
      -e POSTGRES_PASSWORD=test -e POSTGRES_DB=test postgres:17`,
      then `DATABASE_URL=postgres://test:test@localhost:5433/test
      cargo test`. See [Testing](testing.md) for the long version.
- [ ] For changes touching the gateway crate, the same three commands
      again with `--manifest-path mcp-gateway/Cargo.toml` or from
      inside `mcp-gateway/` (the gateway has no DB-backed tests, so
      `cargo test` is enough)
- [ ] Docs updated if behaviour changed
- [ ] CHANGELOG entry under `[Unreleased]`
- [ ] Manual test in a live Discord server

CI will run the first four for you, so skipping them locally just
means you find out about failures from a bot instead of a shell. It's
faster to catch them yourself.

## Open the pull request

Push your branch and open a PR against `master`:

```bash
git push -u origin fix/wordle-expiry-bug
```

Use the
[PR template](https://github.com/MrMcEpic/discord-bot-rs/blob/master/.github/PULL_REQUEST_TEMPLATE.md).
It asks for:

- **Summary** — one to three sentences on what and why.
- **Changes** — a bullet list of the main edits.
- **Testing** — the testing checkboxes (fmt, clippy, test, manual) and
  a short description of what you manually verified.
- **Related issues** — `Closes #123` for issues this PR fully fixes,
  `Refs #456` for related context.
- **Breaking changes** — default is `None`. If yours breaks an
  existing config, command, or behaviour, describe the migration.
- **Checklist** — four housekeeping items; tick them honestly.

Mark the PR as **draft** if it isn't ready for review yet. Draft PRs
still get CI, so you can push through a broken state until it's green
without fielding review comments prematurely.

## CI checks

When you push, CI runs the
[`ci.yml`](https://github.com/MrMcEpic/discord-bot-rs/blob/master/.github/workflows/ci.yml)
workflow, which has four jobs:

- **check-main** — `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo check --all-targets`, `cargo test` on the main crate. The
  job stands up a `postgres:17` service container with a health
  check and exports `DATABASE_URL` so the integration tests under
  `tests/` run for real against a live database.
- **check-gateway** — the same four commands inside `mcp-gateway/`
  (no Postgres service; the gateway's tests are pure).
- **docker-main** — builds the top-level `Dockerfile` (no push).
- **docker-gateway** — the same for `mcp-gateway/Dockerfile`.

A red check fails the PR. Common failures:

- **`cargo fmt --check` differs** — run `cargo fmt` and push.
- **Clippy flags something** — read the lint and fix it, or add a
  targeted `#[allow(...)]` with a comment if it's a false positive.
- **Compile error on Linux that didn't happen locally** — usually a
  missing system library. `check-main` installs `cmake`,
  `libopus-dev`, and `libsodium-dev`.
- **Test flake** — rare; push an empty commit or ask for a re-run.

Re-run a workflow by pushing any commit or re-opening the PR.

## Review process

Reviews usually come within a few days. If a week goes by with no
response, comment on the PR to bump it — it's almost certainly been
missed, not ignored.

During review:

- **Respond by committing, not force-pushing.** Follow-up commits make
  it easy for the reviewer to see exactly what changed between passes.
- **Don't squash your own history** unless asked. The maintainer
  squashes at merge time.
- **Mark resolved conversations** once you've addressed them.

## Merge

The default merge strategy is **squash merge**. Your feature branch
becomes one commit on `master`, titled after the PR title (and — since
the title follows a conventional prefix — grouped cleanly in the
changelog).

This means the shape of your individual commits matters less than the
shape of the PR description and title. A messy WIP history is fine as
long as the squashed commit message is tidy.

## After the merge

Once your PR is merged:

```bash
git checkout master
git fetch upstream
git merge --ff-only upstream/master
git push origin master
git branch -d fix/wordle-expiry-bug
```

Then rebase any other in-flight feature branches onto the new `master`.
The merge UI also offers a button to delete the remote branch.

## Release cadence

Releases are cut as-needed — usually every few weeks, sooner for
security fixes. Every merged PR ends up in the next release's
[CHANGELOG.md](https://github.com/MrMcEpic/discord-bot-rs/blob/master/CHANGELOG.md)
entry, and the release workflow publishes a tagged build. If your
change is urgent, say so in the PR description.

## Reference

- [CONTRIBUTING.md](https://github.com/MrMcEpic/discord-bot-rs/blob/master/CONTRIBUTING.md),
  [CODE_OF_CONDUCT.md](https://github.com/MrMcEpic/discord-bot-rs/blob/master/CODE_OF_CONDUCT.md),
  [SECURITY.md](https://github.com/MrMcEpic/discord-bot-rs/blob/master/SECURITY.md)
- [Building Locally](building-locally.md),
  [Adding a Command](adding-a-command.md),
  [Testing](testing.md)
- [CI workflow](https://github.com/MrMcEpic/discord-bot-rs/blob/master/.github/workflows/ci.yml),
  [PR template](https://github.com/MrMcEpic/discord-bot-rs/blob/master/.github/PULL_REQUEST_TEMPLATE.md),
  [issue templates](https://github.com/MrMcEpic/discord-bot-rs/tree/master/.github/ISSUE_TEMPLATE)
