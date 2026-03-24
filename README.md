# patchworks

[![CI](https://github.com/dunamismax/patchworks/actions/workflows/ci.yml/badge.svg)](https://github.com/dunamismax/patchworks/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/patchworks.svg)](https://crates.io/crates/patchworks) [![docs.rs](https://docs.rs/patchworks/badge.svg)](https://docs.rs/patchworks) [![MSRV](https://img.shields.io/badge/MSRV-1.75-blue.svg)](https://github.com/dunamismax/patchworks/blob/main/Cargo.toml) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**Git-style diffs for SQLite databases.**

Patchworks is a native desktop tool that treats SQLite databases the way `git diff` treats source code. Open one database to inspect it. Open two to see exactly what changed — every schema modification, every row added, removed, or altered — then generate the SQL to reconcile them.

No cloud. No account. No daemon. One binary, your databases, the truth.

> **Status:** v0.3.0 published on crates.io. SQLite inspection, schema diffing, row diffing, local snapshot storage, SQL export, schema browser with DDL preview, collapsible diff sections, table search/filter, keyboard shortcuts, light/dark/system themes, and recent files are available in both the desktop GUI and headless CLI. View diff/export remains ahead.

## Why patchworks?

Every team that ships software backed by SQLite eventually hits the same wall: "what actually changed in this database?" Maybe it's a production config store that drifted. Maybe it's a mobile app's local database after a migration. Maybe it's two copies of the same file and nobody remembers which one is current.

The existing options are grim — hex editors, ad-hoc scripts, manually eyeballing `sqlite3` output. Patchworks replaces all of that with a purpose-built comparison engine and a fast native UI.

## What Ships Today

| Capability | Status |
|---|---|
| Inspect SQLite schema, tables, and views | ✓ |
| Browse rows with pagination and sortable columns | ✓ |
| Schema browser with DDL preview (tables, views, indexes, triggers) | ✓ |
| Schema-level diff (tables, indexes, triggers) | ✓ |
| Row-level diff with streaming merge comparison | ✓ |
| Collapsible diff sections with summary statistics | ✓ |
| Snapshot a database to `~/.patchworks/` for later comparison | ✓ |
| Generate SQL migration (left → right) | ✓ |
| Foreign-key-safe export with trigger preservation | ✓ |
| Background processing with progress indicators | ✓ |
| Table name search/filter in file panels | ✓ |
| Keyboard shortcuts for views and diff (⌘1-6, ⌘D) | ✓ |
| Light/dark/system theme support | ✓ |
| Recent files with quick reopen | ✓ |

## Install

```bash
# From crates.io
cargo install patchworks

# From source
git clone git@github.com:dunamismax/patchworks.git
cd patchworks
cargo build --release
```

`rusqlite` ships with the `bundled` feature — no system SQLite required.

## Usage

### Desktop GUI

```bash
# Launch empty — open databases from the UI
patchworks

# Inspect a single database
patchworks app.db

# Diff two databases
patchworks left.db right.db
```

### Headless CLI

```bash
# Inspect a database (schema, tables, columns, views, indexes, triggers)
patchworks inspect app.db
patchworks inspect app.db --format json

# Diff two databases
patchworks diff left.db right.db
patchworks diff left.db right.db --format json

# Generate a SQL migration
patchworks export left.db right.db
patchworks export left.db right.db -o migration.sql

# Manage snapshots
patchworks snapshot save app.db --name "before-migration"
patchworks snapshot list
patchworks snapshot list --source app.db --format json
patchworks snapshot delete <uuid>
```

### Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success, no differences |
| 1 | Error (bad path, SQLite failure, etc.) |
| 2 | Differences found (useful for CI gates) |

## How it works

Patchworks reads SQLite databases through `rusqlite` in read-only mode, extracts schema metadata from `sqlite_master`, and performs a streaming merge comparison across shared tables. The diff engine:

- Detects added, removed, and modified tables and columns at the schema level
- Streams row-level comparisons using primary keys (falls back to `rowid` with warnings when keys diverge)
- Tracks indexes and triggers through the full diff pipeline
- Generates SQL migrations that rebuild modified tables via temporary replacement, guard `PRAGMA foreign_keys`, and drop/recreate triggers around DML

The UI is built on [egui](https://github.com/emilk/egui) — immediate-mode, GPU-accelerated, cross-platform. All heavy work (inspection, table loading, diffing) runs on background threads with staged progress reporting.

## Architecture

```text
src/
├── main.rs            # CLI entrypoint and subcommand dispatch
├── cli.rs             # Headless CLI command implementations
├── app.rs             # Application coordinator and background task management
├── lib.rs             # Library root
├── error.rs           # Shared error model
├── db/                # SQLite inspection, snapshots, diff orchestration
│   ├── inspector.rs   # Schema and row reading with pagination
│   ├── differ.rs      # High-level diff coordination with progress
│   ├── snapshot.rs    # Local snapshot store (~/.patchworks/)
│   └── types.rs       # Core data types
├── diff/              # Comparison algorithms and export
│   ├── schema.rs      # Schema diffing
│   ├── data.rs        # Streaming row-level diffing
│   └── export.rs      # SQL migration generation
├── state/             # UI-facing workspace state
│   ├── workspace.rs   # Active databases, selections, loading flags
│   └── recent.rs      # Recent-files persistence
└── ui/                # egui rendering layer
    ├── workspace.rs   # Main workspace layout and view switching
    ├── table_view.rs  # Table browsing with pagination
    ├── schema_browser.rs # Full schema browser with DDL preview
    ├── diff_view.rs   # Row diff rendering with collapsible sections
    ├── schema_diff.rs # Schema diff rendering
    ├── sql_export.rs  # SQL export preview and save
    ├── snapshot_panel.rs # Snapshot management
    ├── file_panel.rs  # File selection with search/filter
    ├── dialogs.rs     # Modal dialogs
    └── progress.rs    # Progress indicators
```

See [`ARCHITECTURE.md`](ARCHITECTURE.md) for deep technical details.

## Operational guidance

### Live and WAL-mode databases

Patchworks opens databases in **read-only mode** and will read from WAL-mode databases. However, concurrent writes by other processes during inspection or diff can produce inconsistent results — you may see partial transactions or mid-write state.

**For reliable results:**

- Operate on quiescent database files (no other writers active)
- Use `patchworks snapshot save <db>` to capture a stable copy before comparing
- If you must inspect a live database, treat the output as advisory rather than authoritative
- Encrypted databases are not supported

### Large databases

The diff engine streams row comparisons and the export path writes one statement at a time to bound memory. However, the GUI preview path collects the full export into a `String`, so very large migrations displayed in the desktop UI may use significant memory. For large exports, prefer the CLI with file output:

```bash
patchworks export left.db right.db -o migration.sql
```

## Known limits

- **Views are inspect-only.** They are not diffed or exported.
- **No explicit cancel.** Long-running jobs show progress but can only be superseded, not interrupted.
- **CI badge covers Linux and macOS.** Other platforms are untested.

## Design principles

1. **Correctness over cleverness.** A heavier migration that is semantically correct beats a minimal one that breaks edge cases.
2. **SQLite-native.** Preserve the nuance of SQLite (rowid, WITHOUT ROWID, WAL, PRAGMA behavior) instead of flattening into generic database abstractions.
3. **Honest scope.** Never describe future work as present capability.
4. **Desktop and CLI.** The GUI and headless CLI share the same backend truth layer. Every inspection, diff, and export capability is available in both.
5. **Single binary, zero config.** `cargo install patchworks` is all you need.

## Verification

```bash
cargo build
cargo test
cargo nextest run
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo bench --no-run
cargo deny check
```

## Snapshot storage

Patchworks maintains a local store in your home directory:

- Metadata: `~/.patchworks/patchworks.db`
- Snapshots: `~/.patchworks/snapshots/<uuid>.sqlite`

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for setup, conventions, and how to get involved.

## License

MIT — see [LICENSE](LICENSE).

## Links

- [Crates.io](https://crates.io/crates/patchworks)
- [Architecture](ARCHITECTURE.md)
- [Contributing](CONTRIBUTING.md)
- [Changelog](CHANGELOG.md)
