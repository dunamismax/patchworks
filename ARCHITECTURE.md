# Patchworks Architecture

This document describes the technical architecture of Patchworks in enough detail for a new contributor or agent to understand the codebase, make informed changes, and avoid breaking the invariants that keep the tool trustworthy.

## Design philosophy

Patchworks is built on three architectural principles:

1. **Layered separation.** UI rendering, application state, and data logic live in distinct modules with clear dependency directions. `ui/` depends on `state/`, which depends on `db/` and `diff/`. Nothing flows backwards.

2. **Background-first I/O.** Every operation that touches SQLite runs on a background thread. The UI thread never blocks on database I/O. Results flow back through `mpsc` channels and are applied on the next UI tick.

3. **Correctness over performance.** The diff engine and SQL export prioritize semantic correctness. A heavier migration that preserves all schema objects and handles edge cases correctly is preferred over a minimal one that might break.

## Module map

```
patchworks (single crate)
│
├── src/main.rs           CLI entrypoint
├── src/lib.rs            Library module exports
├── src/app.rs            Application coordinator
├── src/error.rs          Shared error types
│
├── src/db/               Data layer
│   ├── mod.rs            Module declarations
│   ├── inspector.rs      SQLite schema/row reading
│   ├── differ.rs         Diff orchestration with progress
│   ├── snapshot.rs       Snapshot persistence
│   └── types.rs          Core data types
│
├── src/diff/             Comparison engine
│   ├── mod.rs            Module declarations
│   ├── schema.rs         Schema diffing
│   ├── data.rs           Streaming row-level diffing
│   └── export.rs         SQL migration generation
│
├── src/state/            UI state
│   ├── mod.rs            Module declarations
│   └── workspace.rs      Workspace, pane, and diff state
│
└── src/ui/               Rendering layer
    ├── mod.rs            Module declarations
    ├── workspace.rs      View switcher
    ├── file_panel.rs     File open dialogs
    ├── table_view.rs     Table browsing
    ├── diff_view.rs      Row diff display
    ├── schema_diff.rs    Schema diff display
    ├── sql_export.rs     SQL preview/copy/save
    ├── snapshot_panel.rs Snapshot management
    ├── progress.rs       Progress indicators
    └── dialogs.rs        Common dialogs
```

## Data flow

### Startup

```
main.rs
  ├─ --snapshot <db>  → snapshot::save_snapshot() → exit
  ├─ no args          → PatchworksApp::new(empty) → eframe::run_native()
  ├─ one arg          → PatchworksApp::new(left=db) → eframe::run_native()
  └─ two args         → PatchworksApp::new(left=db, right=db) → eframe::run_native()
```

### Database loading (background)

```
User opens file → app.rs::load_left/right()
  → spawns thread: inspector::inspect_database()
  → thread sends DatabaseSummary through mpsc channel
  → app.rs::poll_background_work() picks it up on next UI tick
  → updates WorkspaceState.left/right
  → triggers initial table page load (also backgrounded)
```

### Table browsing (background)

```
User selects table or changes page/sort
  → app.rs::request_table_refresh()
  → spawns thread: inspector::read_table_page()
  → thread sends TablePage through mpsc channel
  → app.rs::poll_background_work() applies it
  → UI re-renders with new data
```

### Diff computation (background)

```
User clicks Diff
  → app.rs::request_diff()
  → spawns thread: differ::diff_databases_with_progress()
    → schema::diff_schema()
    → data::diff_all_tables() (streaming merge per shared table)
    → export::export_diff_as_sql()
  → progress callbacks update DiffState.progress through channel
  → completed DatabaseDiff sent through channel
  → app.rs::poll_background_work() applies result
```

### SQL export pipeline

```
export::export_diff_as_sql(schema_diff, data_diffs, left_summary, right_summary)
  → PRAGMA foreign_keys=OFF
  → For each removed table: DROP TABLE
  → For each added table: CREATE TABLE + INSERT all rows
  → For each modified table:
    → DROP affected triggers
    → If schema changed: temp-table rebuild
      → CREATE TABLE _patchworks_new_<name> (right schema)
      → INSERT INTO _patchworks_new_<name> SELECT ... FROM <name>
      → DROP TABLE <name>
      → ALTER TABLE _patchworks_new_<name> RENAME TO <name>
    → INSERT/DELETE/UPDATE rows
    → Recreate indexes from right schema
    → Recreate triggers from right schema
  → PRAGMA foreign_keys=ON
```

## Core types

### `DatabaseSummary`
Represents the complete schema metadata for one database: tables, views, indexes, triggers, and their properties.

### `TableInfo` / `ViewInfo` / `SchemaObjectInfo`
Individual schema objects with their names, DDL, and (for tables) column details and row counts.

### `SqlValue`
Enum representing SQLite values: Null, Integer, Real, Text, Blob. Used throughout the diff pipeline for row comparison.

### `SchemaDiff`
The result of comparing two `DatabaseSummary` objects: lists of added, removed, and modified tables plus schema objects.

### `TableDataDiff`
Row-level diff for a single table: added rows, removed rows, modified rows (with before/after), statistics, and warnings.

### `DatabaseDiff`
The complete diff result: left summary, right summary, schema diff, all table data diffs, and the generated SQL export.

## Background task model

Patchworks uses a simple fire-and-forget threading model:

- `std::thread::spawn` for all background work (no async runtime).
- `std::sync::mpsc` channels for result delivery.
- Each task type has a dedicated channel and receiver stored in `PatchworksApp`.
- `poll_background_work()` runs on every `egui` update tick, checking all receivers.
- Stale tasks are superseded by replacing the receiver — the old thread runs to completion but its result is dropped.

### Why not async?

The workload is CPU-bound (SQLite reads, diff computation) rather than I/O-bound. An async runtime would add complexity without improving throughput. The current model is simple, correct, and sufficient for the foreseeable future.

### Cancellation strategy

There is no cooperative cancellation today. Background threads run to completion even if their results are no longer needed. This is acceptable because:

1. Database reads are fast for normal-sized databases.
2. Diff computation is the only potentially long operation, and progress is reported.
3. The cost of a wasted computation is small compared to the complexity of safe cancellation.

If cancellation becomes necessary (Phase 3 hardening for very large databases), it should be implemented as cooperative checkpoints in the diff engine, not as thread interruption.

## SQLite interaction model

- All database access uses `rusqlite` with the `bundled` feature (no system SQLite dependency).
- Databases are opened in **read-only mode** (`SQLITE_OPEN_READ_ONLY`).
- Schema metadata is read from `sqlite_master`.
- Row reads use parameterized queries with pagination.
- Sorted pagination includes a deterministic tie-breaker (primary key or `rowid`) to ensure stable page boundaries.

### WAL and live database handling

- Patchworks does not acquire exclusive locks.
- WAL-backed and actively changing databases may produce inconsistent snapshots if rows are modified between reads.
- The tool documents this as "best-effort" behavior.
- Snapshot-based workflows (snapshot → compare later) provide stronger guarantees because the snapshot is a static copy.

## Snapshot store

```
~/.patchworks/
├── patchworks.db              Metadata database (SQLite)
└── snapshots/
    └── <uuid>.sqlite          Full database copies
```

- Each snapshot is a full copy of the source database (via SQLite backup API).
- Metadata (source path, timestamp, label) is tracked in `patchworks.db`.
- Snapshots are identified by UUID v4.
- The store uses one SQLite connection per operation (intentionally simple; persistent connection is a future consideration).

## Testing strategy

### Integration tests (`tests/`)

- `diff_tests.rs`: Core diff and export behavior including edge cases (WITHOUT ROWID, FK enforcement, rowid fallback).
- `proptest_invariants.rs`: Property-based tests for schema classification invariants, row-diff accounting, and SQL export round-trips.
- `snapshot_tests.rs`: Snapshot persistence and retrieval.

### Fixture system

- `tests/fixtures/create_fixtures.sql` contains named SQL blocks.
- `tests/support/mod.rs` provides `create_db()` and `create_db_with_sql()` helpers that load fixtures into temporary SQLite databases.

### Benchmarks (`benches/`)

- `diff_hot_paths.rs`: Row diff streaming performance (20,000 rows) and end-to-end diff.
- `query_hot_paths.rs`: Paged table read and sorted pagination performance.
- Sample size is intentionally low (10) because bundled SQLite compilation is slow in release mode.

## Key invariants

These must hold across all changes:

1. **Diff + export + apply = identity.** Applying the generated SQL export to the left database must produce the right database's state. Property tests verify this.
2. **Schema objects survive migration.** Indexes and triggers from the right database must be present after export application.
3. **Foreign key safety.** Generated SQL must not violate FK constraints when applied to a database with `PRAGMA foreign_keys=ON`.
4. **Triggers don't fire during migration.** Left-side triggers are dropped before DML and right-side triggers are created after.
5. **Deterministic pagination.** The same query with the same sort must return the same page regardless of insertion order.
6. **UI thread never blocks on I/O.** All database operations happen on background threads.
