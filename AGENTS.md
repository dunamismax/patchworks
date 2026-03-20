# AGENTS.md

## Purpose

This file is the secondary agent-oriented project memory for Patchworks. `BUILD.md` is the primary operational handoff document and must stay authoritative for setup, verification, and next-pass priorities. Use this file for concise architecture and convention notes that help the next Codex session ramp up quickly.

## Project

- Name: Patchworks
- Tagline: Git-style visual diffs for SQLite databases
- Stack: Rust 2021, egui/eframe, rusqlite, serde, clap, tracing

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
- The egui app shell includes file loading, table browsing, diff views, schema diff, snapshots, and SQL export preview.
- CLI modes support empty launch, one-file inspect, two-file diff, and snapshot creation.

## Verification Snapshot

- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`

## Known Caveats

- SQL export currently prioritizes correctness over minimality, so modified schemas are rebuilt from the right-hand database definition.
- The diff UI is functional but intentionally lightweight and can be refined further.
- Snapshot storage currently uses `~/.patchworks`.
- `BUILD.md` currently tracks at least one concrete SQL export risk around rowid-fallback deletes; read it before touching export logic.
