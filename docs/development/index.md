# Development

This section is for people who want to read, extend, debug, or contribute to discord-bot-rs.

## Where to start

- **New to the codebase?** Start with the [Codebase Tour](codebase-tour.md). It walks every module in `src/` and explains responsibilities, key types, and entry points. ~3000 words; treat it as your map.
- **Want to run the bot without Docker?** [Building Locally](building-locally.md) covers the cargo workflow, system dependencies, and how to point the binary at a local PostgreSQL.
- **Want to write code?** [Contributing Workflow](contributing-workflow.md) covers fork-and-PR, the pre-PR checklist, what CI runs, and how reviews work. Pair it with the top-level [CONTRIBUTING.md](https://github.com/MrMcEpic/discord-bot-rs/blob/master/CONTRIBUTING.md), which has the inbound-license clause and dev-setup essentials.
- **Stuck on a bug?** [Debugging](debugging.md) covers `RUST_LOG`, common failure modes, and where to look when the bot misbehaves.

## How-to guides

When you have a specific change in mind:

- [Adding a Command](adding-a-command.md) — every user-facing command in this bot is a prefix subcommand of the parent `m` command. This guide walks through writing a new one and registering it correctly. The #1 gotcha is forgetting the entry in `src/commands/mod.rs`.
- [Adding a Feature Module](adding-a-feature-module.md) — the bigger version of "adding a command." Covers creating a new top-level module under `src/`, wiring its config into `InstanceConfig`, hooking event handlers, and integrating with the `Data` struct.
- [Adding an MCP Tool](adding-an-mcp-tool.md) — the MCP server in `src/mcp/` exposes Discord management tools to clients like Claude Code. This guide shows how to add a new tool, including the schema, handler, and `#[tool]` macro.

## Testing

[Testing](testing.md) describes the current state honestly: limited automated coverage today, with a clear path for adding more. The mcp-gateway crate has unit tests for routing; the main crate compiles in CI but has no test suite to speak of yet. Contributions of tests are very welcome.

## Architecture context

These dev pages assume you've at least skimmed the [Architecture Overview](../architecture/index.md). If you haven't, start there — it has the top-level component diagram, the `Data` struct's role, and the multi-instance model. The architecture pages are reference material; the dev pages are how-to.

## Tooling expectations

Every PR runs through CI: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo check`, `cargo test`, and a `docker build` of both Dockerfiles. Run these locally before pushing and you'll save yourself a round trip:

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
cd mcp-gateway && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test
```

Rust formatting is hard tabs, width 4 (`rustfmt.toml`). Don't fight the formatter.
