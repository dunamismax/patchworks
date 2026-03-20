# Patchworks Memory

## Project

- Name: Patchworks
- Tagline: Git-style visual diffs for SQLite databases
- Stack: Rust 2021, egui/eframe, rusqlite, serde, clap, tracing

## Phase 1 Goals

- Inspect SQLite databases and browse tables
- Compare two database files at schema and row level
- Save snapshots to a local Patchworks store
- Export diffs as SQL migration scripts
- Ship as a single desktop binary with a lightweight egui interface

## Architecture Notes

- `src/db` owns inspection, types, snapshot storage, and high-level diff orchestration
- `src/diff` owns schema diffing, streaming row diffs, and SQL export
- `src/state` holds UI-facing workspace state
- `src/ui` renders panels and views without owning persistence
- `src/app.rs` coordinates UI events with the backend modules

## Current Conventions

- Public types and functions should carry doc comments
- Use `thiserror` in library code and `anyhow` in `main.rs`
- Avoid `unwrap()` outside tests
- Prefer keeping diff logic streaming-friendly rather than materializing full tables

## Final Phase 1 State

- The crate now includes a reusable library surface plus the desktop binary entrypoint
- Database inspection reads tables, views, columns, primary keys, row counts, and paginated row data
- Schema diffing detects added, removed, and modified tables and columns
- Data diffing uses a merge-style streaming compare for shared tables and falls back to `rowid` when necessary
- SQL export generates transactional migration scripts and rebuilds modified tables from the right-side schema
- Snapshot storage copies databases into `~/.patchworks/snapshots/` and tracks metadata in `~/.patchworks/patchworks.db`
- The egui app shell includes file loading, table browsing, diff views, schema diff, snapshots, and SQL export preview
- CLI modes support empty launch, one-file inspect, two-file diff, and snapshot creation

## Verification

- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`

## Known Caveats

- SQL export currently prioritizes correctness over minimality, so modified schemas are rebuilt from the right-hand database definition
- The diff UI is functional but intentionally lightweight and can be refined further in Phase 2
- Snapshot storage currently uses a Unix-style `~/.patchworks` home for consistency with the Phase 1 spec
