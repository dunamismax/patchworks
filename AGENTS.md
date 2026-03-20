# AGENTS.md

## Purpose

This file is the secondary agent-oriented project memory for Patchworks. `BUILD.md` is the primary operational handoff document and must stay authoritative for setup, verification, and next-pass priorities. Use this file for concise architecture and convention notes that help the next Codex session ramp up quickly.

## Project

- Name: Patchworks
- Tagline: Git-style visual diffs for SQLite databases
- Stack: Rust 2021, egui/eframe, rusqlite, serde, clap, tracing, criterion, proptest

## Product Scope

- Inspect SQLite databases and browse tables
- Compare two database files at schema and row level
- Save snapshots to a local Patchworks store
- Export diffs as SQL migration scripts
- Ship as a single desktop binary with a lightweight egui interface

## Architecture Notes

- `src/db` owns inspection, types, snapshot storage, and high-level diff orchestration.
- `src/diff` owns schema diffing, streaming row diffs, and SQL export.
- `src/state` holds UI-facing workspace state.
- `src/ui` renders panels and views without owning persistence.
- `src/app.rs` coordinates UI events with backend modules and the workspace state.
- `benches/` holds Criterion coverage for the main query and diff hot paths.
- `.github/workflows/ci.yml` is the checked-in CI entrypoint.
- `deny.toml` is the source of truth for dependency policy and allowed licenses.

## Current Conventions

- Public types and functions should carry doc comments.
- Use `thiserror` in library code and `anyhow` in `main.rs`.
- Avoid `unwrap()` outside tests.
- Prefer keeping diff logic streaming-friendly rather than materializing full tables.
- Keep `BUILD.md` current whenever workflows, verification status, or known issues change.

## Current Implemented State

- The crate includes a reusable library surface plus the desktop binary entrypoint.
- Database inspection reads tables, views, columns, primary keys, row counts, and paginated row data.
- Schema diffing detects added, removed, and modified tables and columns.
- Data diffing uses a merge-style streaming compare for shared tables and falls back to `rowid` when necessary.
- SQL export generates transactional migration scripts and rebuilds modified tables from the right-side schema.
- Snapshot storage copies databases into `~/.patchworks/snapshots/` and tracks metadata in `~/.patchworks/patchworks.db`.
- The egui app shell includes file loading, table browsing, background diff execution, schema diff, snapshots, and SQL export preview.
- CLI modes support empty launch, one-file inspect, two-file diff, and snapshot creation.
- Criterion benchmarks now cover paged table reads, row-diff streaming, and end-to-end diff generation.
- Proptest coverage now checks schema classification invariants, row-diff accounting invariants, and SQL export round-trips.
- The repo now includes GitHub Actions CI plus `cargo-deny` dependency policy checks.

## Verification Snapshot

- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo nextest run`
- `cargo bench --no-run`
- `cargo deny check`

## Known Caveats

- SQL export currently prioritizes correctness over minimality, so modified schemas are rebuilt from the right-hand database definition.
- The diff UI is functional but intentionally lightweight and can be refined further.
- Background diff execution is fire-and-forget today; there is no progress reporting or explicit cancellation beyond replacing the pending request.
- Snapshot storage currently uses `~/.patchworks`.
- `cargo bench --no-run` is slower than the rest of the local checks because the bundled SQLite C source is compiled in release mode for the bench profile.
- `BUILD.md` is still the primary operational handoff and should be read before touching export logic or verification workflows.
