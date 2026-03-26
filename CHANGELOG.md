# Changelog

All notable changes to Patchworks are documented here. This project uses [Keep a Changelog](https://keepachangelog.com/) conventions and [Semantic Versioning](https://semver.org/).

## [1.0.0] - 2026-03-26

Production release. Git-style diffs for SQLite databases.

### Added

#### Core engine
- SQLite inspection engine reading schema from `sqlite_master` in read-only mode
- Full metadata extraction: tables, views, indexes, triggers, columns, primary keys
- Paginated row browsing with deterministic primary-key or `rowid` tie-breakers
- Streaming `for_each_row()` iterator for bounded-memory access to large tables
- WAL-mode database support
- WITHOUT ROWID table support

#### Diffing
- Schema-level diff: added, removed, and modified tables, indexes, triggers, views
- Streaming row-level diff using primary key matching with `rowid` fallback
- Per-cell change tracking within modified rows
- Data-type-aware comparison (integer 1 vs real 1.0, text "42" vs integer 42)
- Diff filtering by change type (added/removed/modified) and by table
- Aggregate diff summary statistics

#### Semantic analysis
- Table rename detection via column similarity scoring with confidence scores
- Column rename detection via property matching (type, nullable, pk, default)
- Compatible type shift detection using SQLite type affinity rules
- Diff annotations for triage workflows (pending/approved/rejected/needs-discussion/deferred)

#### Three-way merge
- Merge engine diffing two derived databases against a common ancestor
- Non-conflicting row and schema change merging
- Conflict detection: row conflicts, schema conflicts, delete-modify conflicts, table-delete conflicts

#### SQL export
- Migration generation transforming left database into right database
- Temporary-table rebuild for schema-changed tables
- `PRAGMA foreign_keys` guards in generated SQL
- Trigger drop/recreate around migration DML
- Streaming `write_export()` for bounded-memory operation
- File output via `-o/--output`

#### Snapshot management
- Local snapshot store under `~/.patchworks/`
- Save, list, delete snapshots with optional source filtering
- Snapshot naming via `--name` flag

#### Migration workflows
- Generate forward and reverse migrations from database diffs
- Validate migrations by applying to temporary copies
- Apply migrations with `--dry-run` safety
- Rollback support
- Squash sequential migrations into a single migration
- Conflict detection between migrations targeting the same objects
- Full persistence in `~/.patchworks/patchworks.db`

#### CLI
- `patchworks inspect` -- schema, tables, columns, views, indexes, triggers
- `patchworks diff` -- schema and row diffs with `--summary`, `--semantic`, `--change-type`, `--table` flags
- `patchworks export` -- SQL migration output to stdout or file
- `patchworks snapshot save|list|delete` -- local snapshot management
- `patchworks merge` -- three-way merge with conflict reporting
- `patchworks migrate generate|validate|list|show|apply|delete|squash|conflicts` -- full migration workflow
- `patchworks serve` -- local web UI server
- `--format human|json` on all read-oriented commands
- CI-friendly exit codes: 0 (success/no differences), 1 (error), 2 (differences found)
- `--version` flag

#### Web UI
- FastAPI + Jinja2 + htmx local web interface
- Schema browser with table/view/index/trigger listing and DDL preview
- Table row browser with pagination
- Diff viewer with collapsible sections and summary statistics
- SQL export preview and download
- Snapshot management panel
- Light/dark theme support

#### Infrastructure
- Python 3.12+ with `uv` package management
- `ruff` linting and formatting
- `pyright` type checking
- `pytest` with 317 tests
- CI workflow for Linux and macOS
- Pre-commit hooks

[1.0.0]: https://github.com/dunamismax/patchworks/releases/tag/v1.0.0
