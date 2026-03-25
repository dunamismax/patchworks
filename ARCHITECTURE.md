# Patchworks Architecture

Technical architecture for patchworks. Written for a developer picking up the repo cold.

## Stack overview

- **Python** (3.12+) is the primary language. CLI, orchestration, diff logic, export generation, and all user-facing tooling live in Python.
- **Go** is reserved for performance-critical hot paths. It will only be introduced when profiling shows Python is the bottleneck on a specific operation. Until then, Go does not exist in this repo.
- **SQLite** is accessed through Python's stdlib `sqlite3` module in read-only mode. If Go components are added later, they use `modernc.org/sqlite` (no CGO).
- **FastAPI + htmx** powers the local web UI in a later phase. The CLI ships first.

## Directory layout

```text
src/patchworks/
  __init__.py              Package root
  __main__.py              CLI entrypoint (python -m patchworks)
  cli.py                   Subcommand dispatch and implementations (argparse)
  db/
    __init__.py
    inspector.py           Schema and row reading with pagination
    differ.py              High-level diff orchestration
    snapshot.py            Local snapshot store (~/.patchworks/)
    migration.py           Migration chain persistence and conflict detection
    types.py               Core data types
  diff/
    __init__.py
    schema.py              Schema-level diffing
    data.py                Streaming row-level diffing
    export.py              SQL migration generation
    semantic.py            Semantic diff awareness (renames, type shifts)
    merge.py               Three-way merge and conflict detection
    migration.py           Migration generation, validation, squashing
tests/
  test_inspector.py
  test_differ.py
  test_schema_diff.py
  test_data_diff.py
  test_export.py
  test_snapshot.py
  test_cli.py
  test_merge.py
  test_migration.py
  test_semantic.py
go/                        (future, only when profiling justifies it)
pyproject.toml             Single project manifest
.python-version            Pinned Python version
.pre-commit-config.yaml    Pre-commit hooks (ruff, pyright)
```

When the Go acceleration layer exists, it lives under `go/` with its own `go.mod` and standard Go layout (`cmd/`, `internal/`). Python calls Go components via subprocess, ctypes, or local HTTP - the integration approach will be decided based on the specific hot path being optimized.

## Key abstractions

### Data types (`db/types.py`)

Core types that flow through the entire pipeline:

- `DatabaseSummary` - complete schema metadata for one database (tables, views, indexes, triggers)
- `TableInfo`, `ColumnInfo`, `IndexInfo`, `TriggerInfo`, `ViewInfo` - individual schema objects
- `SchemaDiff` - result of comparing two `DatabaseSummary` objects
- `TableDataDiff`, `RowDiff`, `CellChange` - row-level diff results
- `DatabaseDiff` - complete diff result combining schema and row diffs

### Inspector (`db/inspector.py`)

Reads SQLite databases in read-only mode (`file:...?mode=ro` URI). Extracts schema metadata from `sqlite_master`. Provides paginated row access with deterministic primary-key or `rowid` tie-breakers. Exposes a streaming `for_each_row()` iterator for bounded-memory access to large tables.

### Differ (`db/differ.py`)

Orchestrates the full diff pipeline: runs schema comparison, then streams row-level diffs for each shared table. Delegates to `diff/schema.py` for schema-level work and `diff/data.py` for row-level work.

### Schema diff (`diff/schema.py`)

Compares two `DatabaseSummary` objects. Detects added, removed, and modified tables, indexes, and triggers.

### Data diff (`diff/data.py`)

Streaming row-level comparison using primary key matching. Falls back to `rowid` when primary keys diverge (with warnings). Tracks per-cell changes within modified rows. Does not materialize full tables in memory.

### SQL export (`diff/export.py`)

Generates SQL that transforms one database into another. Uses temporary-table rebuild for schema-changed tables. Guards `PRAGMA foreign_keys`. Drops and recreates triggers around migration DML to prevent left-side triggers from firing during migration. Streaming `write_export()` writes one statement at a time to any file-like object for bounded-memory operation.

### Snapshot store (`db/snapshot.py`)

Local snapshot management under `~/.patchworks/`:

```text
~/.patchworks/
  patchworks.db              Metadata database (SQLite)
  snapshots/
    <uuid>.sqlite            Full database copies
```

Snapshots are full file copies with metadata (source path, timestamp, label) tracked in the metadata database.

### Semantic diff (`diff/semantic.py`)

Heuristic detection of table renames (via column similarity scoring), column renames (via property matching), and compatible type shifts (using SQLite type affinity rules). All heuristic detections carry confidence scores.

### Three-way merge (`diff/merge.py`)

Diffs two derived databases against a common ancestor. Merges non-conflicting row and schema changes. Surfaces conflicts with enough context for manual resolution.

### Migration management (`db/migration.py`, `diff/migration.py`)

Ordered migration sequences with generation, validation, rollback, and squashing. Persisted under `~/.patchworks/patchworks.db`.

## Data flow

### Inspect

```
CLI: patchworks inspect <db>
  -> cli.py dispatches to db/inspector.py
  -> inspector opens database read-only
  -> reads sqlite_master for schema metadata
  -> returns DatabaseSummary
  -> cli.py formats as human-readable or JSON
```

### Diff

```
CLI: patchworks diff <left> <right>
  -> cli.py dispatches to db/differ.py
  -> differ calls inspector on both databases
  -> differ calls diff/schema.py to compare schemas
  -> differ calls diff/data.py to stream row-level diffs per shared table
  -> returns DatabaseDiff (schema diff + all table data diffs)
  -> cli.py formats output
  -> exit code: 0 = no differences, 2 = differences found
```

### Export

```
CLI: patchworks export <left> <right>
  -> runs the diff pipeline (same as above)
  -> passes diff results to diff/export.py
  -> export generates SQL:
      PRAGMA foreign_keys=OFF
      For each removed table: DROP TABLE
      For each added table: CREATE TABLE + INSERT rows
      For each modified table:
        DROP affected triggers
        If schema changed: temp-table rebuild
          CREATE TABLE _patchworks_new_<name> (right schema)
          INSERT INTO _patchworks_new_<name> SELECT ... FROM <name>
          DROP TABLE <name>
          ALTER TABLE _patchworks_new_<name> RENAME TO <name>
        INSERT/DELETE/UPDATE rows
        Recreate indexes from right schema
        Recreate triggers from right schema
      PRAGMA foreign_keys=ON
  -> writes to stdout or file via -o flag
```

### Snapshot

```
CLI: patchworks snapshot save <db>
  -> copies database file to ~/.patchworks/snapshots/<uuid>.sqlite
  -> records metadata in ~/.patchworks/patchworks.db
```

## CLI layer

The CLI uses `argparse` from the standard library. Subcommand dispatch happens in `cli.py`. The CLI is a thin dispatch layer - it calls the same backend functions that any future web UI will use.

Subcommands: `inspect`, `diff`, `export`, `snapshot` (save/list/delete), `merge`, `migrate` (generate/validate/list/show/apply/delete/squash/conflicts).

All read-oriented commands support `--format human|json`. Export supports `-o/--output <file>`. Exit codes: 0 = success/no differences, 1 = error, 2 = differences found.

## Web UI (later phase)

FastAPI serves Jinja2 templates with htmx for dynamic interaction. The web UI calls the same `inspect_database`, `diff_databases`, `write_export`, and `SnapshotStore` functions as the CLI. No forked logic between surfaces. Launched via `patchworks serve`.

## SQLite interaction model

- All access is read-only (`file:...?mode=ro` URI).
- Schema metadata comes from `sqlite_master`.
- Row reads use parameterized queries with pagination.
- Sorted pagination includes a deterministic tie-breaker (primary key or `rowid`).
- WAL-mode databases are readable but concurrent writes during inspection can produce inconsistent results. Snapshot-based workflows provide stronger guarantees.
- Encrypted databases are not supported.

## Key invariants

1. **Diff + export + apply = identity.** Applying the generated SQL to the left database must produce the right database's state.
2. **Schema objects survive migration.** Indexes and triggers from the right database must be present after export application.
3. **Foreign key safety.** Generated SQL must not violate FK constraints when applied to a database with `PRAGMA foreign_keys=ON`.
4. **Triggers do not fire during migration.** Left-side triggers are dropped before DML; right-side triggers are created after.
5. **Deterministic pagination.** The same query with the same sort returns the same page regardless of insertion order.
6. **Streaming, bounded memory.** Diff and export operations do not materialize full tables.

## Toolchain

| Tool | Purpose |
|------|---------|
| `uv` | Package and environment management |
| `ruff` | Linting and formatting |
| `pyright` | Type checking |
| `pytest` | Testing (with `pytest-cov` for coverage) |
| `pre-commit` | Local quality gates |
| `go test` | Go tests (when Go components exist) |
| `golangci-lint` | Go linting (when Go components exist) |
