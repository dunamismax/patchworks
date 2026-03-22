# Patchworks

**Git-style diffs for SQLite databases.**

Patchworks is a native desktop tool that treats SQLite databases the way `git diff` treats source code. Open one database to inspect it. Open two to see exactly what changed — every schema modification, every row added, removed, or altered — then generate the SQL to reconcile them.

No cloud. No account. No daemon. One binary, your databases, the truth.

## Why Patchworks exists

Every team that ships software backed by SQLite eventually hits the same wall: "what actually changed in this database?" Maybe it's a production config store that drifted. Maybe it's a mobile app's local database after a migration. Maybe it's two copies of the same file and nobody remembers which one is current.

The existing options are grim — hex editors, ad-hoc scripts, manually eyeballing `sqlite3` output. Patchworks replaces all of that with a purpose-built comparison engine and a fast native UI.

## What it does today

| Capability | Status |
|---|---|
| Inspect SQLite schema, tables, and views | Shipped |
| Browse rows with pagination and sortable columns | Shipped |
| Schema-level diff (tables, indexes, triggers) | Shipped |
| Row-level diff with streaming merge comparison | Shipped |
| Snapshot a database to `~/.patchworks/` for later comparison | Shipped |
| Generate SQL migration (left → right) | Shipped |
| Foreign-key-safe export with trigger preservation | Shipped |
| Background processing with progress indicators | Shipped |
| Headless CLI for inspect, diff, and export | Planned |
| Plugin/extension system | Planned |
| Shared snapshot registries | Planned |

## Install

```bash
cargo install patchworks
```

`rusqlite` ships with the `bundled` feature — no system SQLite required.

### Build from source

```bash
git clone git@github.com:dunamismax/patchworks.git
cd patchworks
cargo build --release
```

## Quick start

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

```
src/
├── main.rs          # CLI entrypoint
├── app.rs           # Application coordinator and background task management
├── db/              # SQLite inspection, snapshots, diff orchestration
│   ├── inspector.rs # Schema and row reading with pagination
│   ├── differ.rs    # High-level diff coordination with progress
│   ├── snapshot.rs  # Local snapshot store (~/.patchworks/)
│   └── types.rs     # Core data types
├── diff/            # Comparison algorithms and export
│   ├── schema.rs    # Schema diffing
│   ├── data.rs      # Streaming row-level diffing
│   └── export.rs    # SQL migration generation
├── state/           # UI-facing workspace state
└── ui/              # egui rendering layer
```

For deep architectural details, see [`ARCHITECTURE.md`](ARCHITECTURE.md).

## Current limits

Patchworks is honest about what it can and cannot do today:

- **GUI-first.** No headless CLI for inspect/diff/export yet (Phase 4 on the roadmap).
- **Views are inspect-only.** They are not diffed or exported.
- **No explicit cancel.** Long-running jobs show progress but can only be superseded, not interrupted.
- **Memory-bounded exports are WIP.** Very large migrations still materialize significant data in memory.
- **Best-effort on live databases.** Stable files give the best results; WAL-backed or actively changing databases are handled but not guaranteed.

## Roadmap

Patchworks has a 13-phase development plan tracked in [`BUILD.md`](BUILD.md):

- **Phases 0-2** (done): MVP — inspection, diffing, snapshots, SQL export, schema-object fidelity
- **Phase 3** (in progress): Responsiveness and large-database hardening
- **Phase 4**: Headless CLI and automation surface
- **Phase 5**: Packaging, platform confidence, release discipline
- **Phase 6**: Product polish and UX refinement
- **Phase 7**: Advanced diff intelligence
- **Phase 8**: Migration workflow management
- **Phase 9**: Plugin and extension architecture
- **Phase 10**: Team features and shared snapshot registries
- **Phase 11**: CI/CD integration and automation ecosystem
- **Phase 12**: Multi-engine exploration and long-term platform evolution

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for setup, conventions, and how to get involved.

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

## License

MIT. See [LICENSE](LICENSE).

## Links

- [Crates.io](https://crates.io/crates/patchworks)
- [Build Plan](BUILD.md)
- [Architecture](ARCHITECTURE.md)
- [Contributing](CONTRIBUTING.md)
- [Changelog](CHANGELOG.md)
