# patchworks

**Git-style diffs for SQLite databases.**

Patchworks is a native desktop tool that treats SQLite databases the way `git diff` treats source code. Open one database to inspect it. Open two to see exactly what changed — every schema modification, every row added, removed, or altered — then generate the SQL to reconcile them.

No cloud. No account. No daemon. One binary, your databases, the truth.

> **Status: Phase 3 — responsiveness and large-database hardening.** Phases 0-2 (MVP, schema-object fidelity, quality rails) are complete. The desktop app ships inspection, diffing, snapshots, and SQL export today. See [BUILD.md](BUILD.md) for the full execution plan.

## Why patchworks?

Every team that ships software backed by SQLite eventually hits the same wall: "what actually changed in this database?" Maybe it's a production config store that drifted. Maybe it's a mobile app's local database after a migration. Maybe it's two copies of the same file and nobody remembers which one is current.

The existing options are grim — hex editors, ad-hoc scripts, manually eyeballing `sqlite3` output. Patchworks replaces all of that with a purpose-built comparison engine and a fast native UI.

| Capability | Status |
|---|---|
| Inspect SQLite schema, tables, and views | Shipped |
| Browse rows with pagination and sortable columns | Shipped |
| Schema-level diff (tables, indexes, triggers) | Shipped |
| Row-level diff with streaming merge comparison | Shipped |
| Snapshot a database to `~/.patchworks/` for later comparison | Shipped |
| Generate SQL migration (left to right) | Shipped |
| Foreign-key-safe export with trigger preservation | Shipped |
| Background processing with progress indicators | Shipped |
| Headless CLI for inspect, diff, and export | Planned |
| Plugin/extension system | Planned |
| Shared snapshot registries | Planned |

## Usage

```bash
# Install from crates.io
cargo install patchworks

# Launch empty — open databases from the UI
patchworks

# Inspect a single database
patchworks app.db

# Diff two databases
patchworks left.db right.db

# Snapshot a database for later comparison
patchworks --snapshot app.db
```

`rusqlite` ships with the `bundled` feature — no system SQLite required.

### Build from source

```bash
git clone git@github.com:dunamismax/patchworks.git
cd patchworks
cargo build --release
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
│   ├── mod.rs         # Module root
│   ├── inspector.rs   # Schema and row reading with pagination
│   ├── differ.rs      # High-level diff coordination with progress
│   ├── snapshot.rs    # Local snapshot store (~/.patchworks/)
│   └── types.rs       # Core data types
├── diff/              # Comparison algorithms and export
│   ├── mod.rs         # Module root
│   ├── schema.rs      # Schema diffing
│   ├── data.rs        # Streaming row-level diffing
│   └── export.rs      # SQL migration generation
├── state/             # UI-facing workspace state
│   ├── mod.rs         # Module root
│   └── workspace.rs   # Active databases, selections, loading flags
└── ui/                # egui rendering layer
    ├── mod.rs         # Module root
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

For deep architectural details, see [`ARCHITECTURE.md`](ARCHITECTURE.md).

## Repository layout

```text
.
├── BUILD.md              # execution manual — phases, decisions, progress
├── README.md             # public-facing project description
├── ARCHITECTURE.md       # deep architectural documentation
├── AGENTS.md             # agent-facing architecture memo
├── CONTRIBUTING.md       # setup, conventions, how to contribute
├── CHANGELOG.md          # release history
├── LICENSE               # MIT
├── Cargo.toml            # package manifest and dependency source of truth
├── Cargo.lock            # locked dependency graph
├── deny.toml             # cargo-deny dependency policy
├── .gitignore            # Rust exclusions
├── .github/
│   └── workflows/
│       └── ci.yml        # CI entrypoint
├── src/                  # application and library code
├── tests/                # integration tests and fixtures
│   ├── diff_tests.rs
│   ├── proptest_invariants.rs
│   ├── snapshot_tests.rs
│   ├── support/
│   └── fixtures/
└── benches/              # Criterion benchmarks
    ├── diff_hot_paths.rs
    └── query_hot_paths.rs
```

## Roadmap

| Phase | Name | Status |
|-------|------|--------|
| 0 | Repo baseline, workflow, and packaging truth | **Done** |
| 1 | Desktop inspection, diff, snapshot, and export MVP | **Done** |
| 2 | Schema-object fidelity and quality rails | **Done** |
| 3 | Responsiveness and large-database hardening | **In progress** |
| 4 | Headless CLI and automation surface | Planned |
| 5 | Packaging, platform confidence, release discipline | Planned |
| 6 | Product polish and UX refinement | Planned |
| 7 | Advanced diff intelligence | Planned |
| 8 | Migration workflow management | Planned |
| 9 | Plugin and extension architecture | Planned |
| 10 | Team features and shared snapshot registries | Planned |
| 11 | CI/CD integration and automation ecosystem | Planned |
| 12 | Multi-engine exploration and long-term platform evolution | Planned |

See [BUILD.md](BUILD.md) for the full phase breakdown with tasks, exit criteria, risks, and decisions.

## Current limits

Patchworks is honest about what it can and cannot do today:

- **GUI-first.** No headless CLI for inspect/diff/export yet (Phase 4 on the roadmap).
- **Views are inspect-only.** They are not diffed or exported.
- **No explicit cancel.** Long-running jobs show progress but can only be superseded, not interrupted.
- **Memory-bounded exports are WIP.** Very large migrations still materialize significant data in memory.
- **Best-effort on live databases.** Stable files give the best results; WAL-backed or actively changing databases are handled but not guaranteed.

## Design principles

1. **Correctness over cleverness.** A heavier migration that is semantically correct beats a minimal one that breaks edge cases.
2. **SQLite-native.** Preserve the nuance of SQLite (rowid, WITHOUT ROWID, WAL, PRAGMA behavior) instead of flattening into generic database abstractions.
3. **Honest scope.** Never describe future work as present capability.
4. **Desktop-first, automation-ready.** The GUI is the primary surface today, but every backend capability must be usable without a window.
5. **Single binary, zero config.** `cargo install patchworks` should be all anyone needs.

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
- [Build Plan](BUILD.md)
- [Architecture](ARCHITECTURE.md)
- [Contributing](CONTRIBUTING.md)
- [Changelog](CHANGELOG.md)
