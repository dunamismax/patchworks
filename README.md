# patchworks

[![CI](https://github.com/dunamismax/patchworks/actions/workflows/ci.yml/badge.svg)](https://github.com/dunamismax/patchworks/actions/workflows/ci.yml) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**Git-style diffs for SQLite databases.**

Patchworks is a CLI tool that treats SQLite databases the way `git diff` treats source code. Open one database to inspect it. Open two to see exactly what changed - every schema modification, every row added, removed, or altered - then generate the SQL to reconcile them.

No cloud. No account. No daemon. Your databases, the truth.

> **Status:** v1.0.0 shipped. See [BUILD.md](BUILD.md) for the full roadmap and future plans.

## Why patchworks?

Every team that ships software backed by SQLite eventually hits the same wall: "what actually changed in this database?" Maybe it's a production config store that drifted. Maybe it's a mobile app's local database after a migration. Maybe it's two copies of the same file and nobody remembers which one is current.

The existing options are grim - hex editors, ad-hoc scripts, manually eyeballing `sqlite3` output. Patchworks replaces all of that with a purpose-built comparison engine.

## Feature set

| Capability | Status |
|---|---|
| Inspect SQLite schema, tables, and views | Shipped |
| Browse rows with pagination | Shipped |
| Schema-level diff (tables, indexes, triggers) | Shipped |
| Row-level diff with streaming comparison | Shipped |
| Snapshot a database to `~/.patchworks/` for later comparison | Shipped |
| Generate SQL migration (left to right) | Shipped |
| Foreign-key-safe export with trigger preservation | Shipped |
| Semantic diff awareness (renames, type shifts) | Shipped |
| Three-way merge with conflict detection | Shipped |
| Migration workflow management (generate, validate, apply, squash) | Shipped |
| Diff annotations for triage workflows | Shipped |
| Data-type-aware comparison rules | Shipped |
| Machine-readable JSON output | Shipped |
| CI-friendly exit codes | Shipped |
| Local web UI for interactive browsing | Shipped |

## Tech stack

- **Python** (3.12+) - CLI, orchestration, diff logic, export generation, all user-facing tooling
- **Go** - reserved for performance-critical components if profiling justifies it
- **uv** - package and environment management
- **ruff** - linting and formatting
- **Pyright** - type checking
- **pytest** - testing
- **sqlite3** - stdlib SQLite access (read-only)
- **FastAPI + htmx** - local web UI for interactive browsing and diff review

## Install

```bash
# From source (recommended during development)
git clone git@github.com:dunamismax/patchworks.git
cd patchworks
uv sync
uv run patchworks --help
```

```bash
# Install as a tool
uv tool install .
patchworks --help
```

## Usage

### Inspect a database

```bash
patchworks inspect app.db
patchworks inspect app.db --format json
```

### Diff two databases

```bash
patchworks diff left.db right.db
patchworks diff left.db right.db --format json
```

### Generate a SQL migration

```bash
patchworks export left.db right.db
patchworks export left.db right.db -o migration.sql
```

### Manage snapshots

```bash
patchworks snapshot save app.db --name "before-migration"
patchworks snapshot list
patchworks snapshot list --source app.db --format json
patchworks snapshot delete <uuid>
```

### Three-way merge

```bash
patchworks merge ancestor.db left.db right.db
patchworks merge ancestor.db left.db right.db --format json
```

### Migration workflows

```bash
patchworks migrate generate left.db right.db --name "add-users-table"
patchworks migrate validate <id>
patchworks migrate list
patchworks migrate show <id>
patchworks migrate apply <id> target.db
patchworks migrate apply <id> target.db --dry-run
patchworks migrate delete <id>
patchworks migrate squash --source left.db --name "combined"
patchworks migrate conflicts
```

### Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success, no differences |
| 1 | Error (bad path, SQLite failure, etc.) |
| 2 | Differences found (useful for CI gates) |

## How it works

Patchworks reads SQLite databases through Python's `sqlite3` module in read-only mode, extracts schema metadata from `sqlite_master`, and performs a streaming comparison across shared tables. The diff engine:

- Detects added, removed, and modified tables and columns at the schema level
- Streams row-level comparisons using primary keys (falls back to `rowid` with warnings when keys diverge)
- Tracks indexes and triggers through the full diff pipeline
- Generates SQL migrations that rebuild modified tables via temporary replacement, guard `PRAGMA foreign_keys`, and drop/recreate triggers around DML

## Architecture

```text
src/patchworks/
├── __init__.py            # Package root
├── __main__.py            # CLI entrypoint
├── cli.py                 # Subcommand dispatch and implementations
├── db/                    # SQLite inspection, snapshots, diff orchestration
│   ├── inspector.py       # Schema and row reading with pagination
│   ├── differ.py          # High-level diff coordination
│   ├── snapshot.py        # Local snapshot store (~/.patchworks/)
│   ├── migration.py       # Migration chain persistence
│   └── types.py           # Core data types
├── diff/                  # Comparison algorithms and export
│   ├── schema.py          # Schema diffing
│   ├── data.py            # Streaming row-level diffing
│   ├── export.py          # SQL migration generation
│   ├── semantic.py        # Semantic diff awareness (renames, type shifts)
│   ├── merge.py           # Three-way merge and conflict detection
│   └── migration.py       # Migration generation, validation, squashing
├── web/                   # Local web UI (FastAPI + htmx)
│   ├── app.py             # Application factory
│   ├── routes.py          # Web UI routes
│   ├── templates/         # Jinja2 templates
│   └── static/            # CSS and static assets
tests/
├── test_cli.py
├── test_diff.py
├── test_export.py
├── test_inspector.py
├── test_merge.py
├── test_migration.py
├── test_semantic.py
├── test_snapshot.py
└── test_web.py
```

See [`ARCHITECTURE.md`](ARCHITECTURE.md) for deep technical details.

## Operational guidance

### Live and WAL-mode databases

Patchworks opens databases in **read-only mode** and will read from WAL-mode databases. However, concurrent writes by other processes during inspection or diff can produce inconsistent results.

**For reliable results:**

- Operate on quiescent database files (no other writers active)
- Use `patchworks snapshot save <db>` to capture a stable copy before comparing
- If you must inspect a live database, treat the output as advisory
- Encrypted databases are not supported

### Large databases

The diff engine streams row comparisons to bound memory. For large exports, use file output:

```bash
patchworks export left.db right.db -o migration.sql
```

## Known limits

- **Views are inspect-only.** They are not diffed or exported.
- **Encrypted databases are not supported.**
- **CI covers Linux and macOS.** Other platforms are untested.

## Design principles

1. **Correctness over cleverness.** A heavier migration that is semantically correct beats a minimal one that breaks edge cases.
2. **SQLite-native.** Preserve the nuance of SQLite (rowid, WITHOUT ROWID, WAL, PRAGMA behavior) instead of flattening into generic database abstractions.
3. **Honest scope.** Never describe future work as present capability.
4. **CLI-first.** The CLI is the primary surface. Every capability is available headless before it appears in any UI.
5. **One install, zero config.** `uv tool install .` and you're done.

## Verification

```bash
uv sync
ruff check .
ruff format --check .
pyright
pytest
```

## Snapshot storage

Patchworks maintains a local store in your home directory:

- Metadata: `~/.patchworks/patchworks.db`
- Snapshots: `~/.patchworks/snapshots/<uuid>.sqlite`

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for setup, conventions, and how to get involved.

## License

MIT - see [LICENSE](LICENSE).

## Links

- [Architecture](ARCHITECTURE.md)
- [Build Plan](BUILD.md)
- [Contributing](CONTRIBUTING.md)
- [Changelog](CHANGELOG.md)
