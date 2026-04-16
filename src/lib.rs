//! Library facade for `discord-bot`.
//!
//! The bot itself is a binary (`src/main.rs`) that owns the full Discord
//! client lifecycle, but several modules are pure data/SQL helpers that we
//! also want to exercise from integration tests living under `tests/`. A
//! crate that only has a `[[bin]]` target can't be linked from
//! `tests/*.rs`, so we expose those modules here as well. `main.rs` keeps
//! its own `mod` declarations and is otherwise unchanged; the duplicate
//! compilation cost for these specific modules is negligible compared to
//! the rest of the binary.
//!
//! The surface intentionally stays narrow: only modules that are useful
//! to test in isolation (no Discord, no Songbird, no MCP) live here. Add
//! more if you need them, but resist the urge to mirror `main.rs` in full.

pub mod db;
pub mod stocks;
