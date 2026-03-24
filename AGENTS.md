# AGENTS.md

## Purpose

Secondary agent-oriented project memory for Patchworks. `BUILD.md` is the primary operational handoff document and must stay authoritative for setup, verification, and next-pass priorities. This file is for fast ramp-up.

## Project

- Name: Patchworks
- Tagline: Git-style visual diffs for SQLite databases
- Stack: Rust 2021, egui/eframe, rusqlite (bundled), serde, clap, tracing, criterion, proptest
- Package: crates.io `patchworks` (`0.3.0`)
- License: MIT

## Vision

Patchworks is evolving from a desktop inspection tool into a complete SQLite lifecycle platform: desktop GUI today, headless CLI next, then intelligent diffs, migration management, plugins, team collaboration, and CI/CD integration. See BUILD.md phases 0-12 for the full roadmap.

## Architecture

```
src/main.rs       → CLI entrypoint and subcommand dispatch
src/cli.rs        → Headless CLI command implementations
src/app.rs        → Application coordinator, background task management, keyboard shortcuts
src/db/           → SQLite inspection, snapshots, diff orchestration, types
src/diff/         → Schema diffing, streaming row diffs, SQL export
src/state/        → UI-facing workspace state, recent-files persistence
src/ui/           → egui rendering layer (presentation only)
tests/            → Integration tests, property tests, fixtures
benches/          → Criterion benchmarks
```

Key design decisions:
- `ui/` renders. `state/` stores. `db/` + `diff/` own data logic. `app.rs` coordinates.
- Background tasks use `mpsc` channels — no async runtime.
- Stale background work is superseded by dropping receivers, not cooperatively cancelled.
- SQL export favors correctness over minimality (temp-table rebuild, FK guards, trigger preservation).

## Conventions

- `thiserror` in library code, `anyhow` in `main.rs`.
- No `unwrap()` outside tests.
- Prefer streaming over materializing full tables.
- Keep `BUILD.md` current in the same pass as code changes.
- Public types and functions carry doc comments.

## Verification

```bash
cargo build
cargo test
cargo nextest run
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo bench --no-run
cargo deny check
cargo run -- --help
```

## Current State (2026-03-24)

**Released (v0.3.0) and still active.** Phases 0-6 are shipped. Phase 7 (advanced diff intelligence) is the next build step.

What works: inspect, browse, diff (schema + rows), snapshots, SQL export with FK safety and trigger preservation, background processing with progress, streaming bounded-memory export to file, headless CLI with subcommands for inspect/diff/export/snapshot, JSON output, CI-friendly exit codes, schema browser with DDL preview, table search/filter, keyboard shortcuts (⌘1-6 views, ⌘D diff), dark/light/system theme, recent files with quick reopen, collapsible diff sections with summary statistics.

Known limits: views are inspect-only, no explicit cancel, GUI preview path still collects full export in memory, best-effort on live/WAL databases (read-only access; concurrent writes may produce inconsistent results).

## Known Caveats

- SQL export correctness > minimality.
- No explicit cancel — stale work is dropped.
- Live/WAL/encrypted databases are best-effort (read-only; concurrent writes may cause inconsistent reads).
- `~/.patchworks/` is local machine state, not project state.
- `cargo bench --no-run` is slow (bundled SQLite compiled in release mode).
- BUILD.md is the primary handoff — read it before touching export logic or verification workflows.
- `write_export` is the bounded-memory path; `export_diff_as_sql` still collects to `String` for GUI preview.
