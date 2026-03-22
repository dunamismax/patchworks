# patchworks

**Git-style diffs for SQLite databases.**

Patchworks is a native desktop tool that treats SQLite databases the way `git diff` treats source code. Open one database to inspect it. Open two to see exactly what changed — every schema modification, every row added, removed, or altered — then generate the SQL to reconcile them.

No cloud. No account. No daemon. One binary, your databases, the truth.

## Why patchworks?

Every team that ships software backed by SQLite eventually hits the same wall: "what actually changed in this database?" Maybe it's a production config store that drifted. Maybe it's a mobile app's local database after a migration. Maybe it's two copies of the same file and nobody remembers which one is current.

The existing options are grim — hex editors, ad-hoc scripts, manually eyeballing `sqlite3` output. Patchworks replaces all of that with a purpose-built comparison engine and a fast native UI.

## Features

| Capability | Status |
|---|---|
| Inspect SQLite schema, tables, and views | ✓ |
| Browse rows with pagination and sortable columns | ✓ |
| Schema-level diff (tables, indexes, triggers) | ✓ |
| Row-level diff with streaming merge comparison | ✓ |
| Snapshot a database to `~/.patchworks/` for later comparison | ✓ |
| Generate SQL migration (left → right) | ✓ |
| Foreign-key-safe export with trigger preservation | ✓ |
| Background processing with progress indicators | ✓ |

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

```bash
# Launch empty — open databases from the UI
patchworks

# Inspect a single database
patchworks app.db

# Diff two databases
patchworks left.db right.db

# Snapshot a database for later comparison
patchworks --snapshot app.db
```

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
├── main.rs            # CLI entrypoint
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
│   └── workspace.rs   # Active databases, selections, loading flags
└── ui/                # egui rendering layer
    ├── workspace.rs   # Main workspace layout
    ├── table_view.rs  # Table browsing with pagination
    ├── diff_view.rs   # Row diff rendering
    ├── schema_diff.rs # Schema diff rendering
    ├── sql_export.rs  # SQL export preview and save
    ├── snapshot_panel.rs # Snapshot management
    ├── file_panel.rs  # File selection
    ├── dialogs.rs     # Modal dialogs
    └── progress.rs    # Progress indicators
```

See [`ARCHITECTURE.md`](ARCHITECTURE.md) for deep technical details.

## Known limits

- **GUI-first.** No headless CLI for inspect/diff/export yet.
- **Views are inspect-only.** They are not diffed or exported.
- **No explicit cancel.** Long-running jobs show progress but can only be superseded, not interrupted.
- **Large exports are memory-resident.** Very large migrations materialize significant data in memory.
- **Best-effort on live databases.** Stable files give the best results; WAL-backed or actively changing databases are handled but not guaranteed.

## Design principles

1. **Correctness over cleverness.** A heavier migration that is semantically correct beats a minimal one that breaks edge cases.
2. **SQLite-native.** Preserve the nuance of SQLite (rowid, WITHOUT ROWID, WAL, PRAGMA behavior) instead of flattening into generic database abstractions.
3. **Honest scope.** Never describe future work as present capability.
4. **Desktop-first, automation-ready.** The GUI is the primary surface today, but every backend capability is designed to be usable without a window.
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
