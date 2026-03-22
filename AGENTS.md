# AGENTS.md

## Purpose

Secondary agent-oriented project memory for Patchworks. `BUILD.md` is the primary operational handoff document and must stay authoritative for setup, verification, and next-pass priorities. This file is for fast ramp-up.

## Project

- Name: Patchworks
- Tagline: Git-style visual diffs for SQLite databases
- Stack: Rust 2021, egui/eframe, rusqlite (bundled), serde, clap, tracing, criterion, proptest
- Package: crates.io `patchworks` (`0.1.0`)
- License: MIT

## Vision

Patchworks is evolving from a desktop inspection tool into a complete SQLite lifecycle platform: desktop GUI today, headless CLI next, then intelligent diffs, migration management, plugins, team collaboration, and CI/CD integration. See BUILD.md phases 0-12 for the full roadmap.

## Architecture

```
src/main.rs       → CLI entrypoint, thin
src/app.rs        → Application coordinator, background task management
src/db/           → SQLite inspection, snapshots, diff orchestration, types
src/diff/         → Schema diffing, streaming row diffs, SQL export
src/state/        → UI-facing workspace state
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

## Current State (2026-03-22)

**Released and frozen (v0.1.0).** Phases 0-3 complete. All quality gates pass.

What works: inspect, browse, diff (schema + rows), snapshots, SQL export with FK safety and trigger preservation, background processing with progress.

Known limits: no headless CLI, views are inspect-only, no explicit cancel, large exports are memory-resident, best-effort on live/WAL databases.

## Known Caveats

- SQL export correctness > minimality.
- No explicit cancel — stale work is dropped.
- Live/WAL/encrypted databases are best-effort.
- `~/.patchworks/` is local machine state, not project state.
- `cargo bench --no-run` is slow (bundled SQLite compiled in release mode).
- BUILD.md is the primary handoff — read it before touching export logic or verification workflows.
