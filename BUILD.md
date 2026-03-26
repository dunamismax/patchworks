# BUILD.md

## Purpose

This file is the execution manual for `patchworks`.

It should answer, at a glance:

- what patchworks does right now
- what is shipped and verified
- what is actively being built next
- what is planned but not yet built
- what decisions, risks, and open questions shape the repo

This is a working plan for a product being built from scratch in Python and Go. When code and docs disagree, fix them together in the same change.

---

## Mission

Build the definitive SQLite comparison and migration tool - the `git diff` of the database world.

Patchworks exists because there is no trustworthy, purpose-built tool for understanding what changed between two SQLite databases. Not a hex editor. Not a shell script. A real tool - correct, fast, and well-built - that a developer or operator can point at two database files and immediately understand the delta.

### Long-term vision

1. **Today**: A CLI tool that inspects, diffs, snapshots, and exports SQL migrations between SQLite databases.
2. **Next**: Advanced diff intelligence with semantic awareness, three-way merge, and migration workflow management.
3. **After that**: A local web UI (FastAPI + htmx) for interactive browsing and visual diff review.
4. **Long-term**: CI/CD integration, plugin architecture, and optional team workflows with shared snapshot registries.

The through-line is unchanged: SQLite-specific correctness first. Every new feature earns its place by being trustworthy before being powerful.

---

## Current execution posture

The project is at Phase 7 - three-way merge.

- **Stack decision:** Python is the primary language. Go is reserved for hot paths if Python's performance becomes a bottleneck on large databases.
- **Phases 0–6 are complete and verified.** Core inspection, diffing, snapshots, SQL export, CLI surface, and advanced diff intelligence are all shipped and tested (202+ tests passing).
- **Discipline:** Roadmap boxes are not aspiration theater. Check them only after code lands and the relevant verification is recorded.

If a future pass changes the real priorities, update this section first rather than letting the roadmap drift silently.

---

## Locked decisions

| ID | Decision | Rationale |
|----|----------|-----------|
| L-001 | Python is the primary language | CLI, orchestration, diff logic, export generation, and all user-facing tooling live in Python |
| L-002 | Go is deferred | Only introduced when profiling shows Python is the bottleneck on a specific hot path |
| L-003 | `uv` for package and environment management | Team standard per python-tech-stack.md |
| L-004 | `ruff` for linting and formatting | Replaces Black + isort + flake8 per team standard |
| L-005 | Pyright for type checking | Team standard; type hints on all public functions |
| L-006 | `pytest` for testing | Team standard with `pytest-cov` for coverage |
| L-007 | `pyproject.toml` is the single project manifest | No setup.py, setup.cfg, or Poetry |
| L-008 | stdlib `sqlite3` for database access | No external SQLite driver needed; Python bundles SQLite |
| L-009 | `argparse` for CLI | stdlib, no external dependency; subcommand dispatch via `add_subparsers` |
| L-010 | Snapshot state under `~/.patchworks/` | Same location as previous release; local machine state, not project state |
| L-011 | SQL export favors correctness over minimality | Modified tables rebuilt via temporary replacement; semantic fidelity over minimal output |
| L-012 | CLI-first, web UI later | The CLI is the primary surface; FastAPI + htmx web UI is a later phase |
| L-013 | No desktop GUI | CLI is the primary surface; local web UI via FastAPI + htmx is a later phase |

---

## Source-of-truth mapping

| File | Owns |
|------|------|
| `README.md` | Public-facing project description, honest status |
| `BUILD.md` | Implementation map, phase tracking, decisions, working rules |
| `ARCHITECTURE.md` | Deep architectural documentation |
| `CONTRIBUTING.md` | Setup, conventions, how to contribute |
| `CHANGELOG.md` | Release history |
| `pyproject.toml` | Package manifest, dependency graph, tool configuration |
| `.python-version` | Pinned Python version |
| `.pre-commit-config.yaml` | Pre-commit hook configuration |
| `.github/workflows/ci.yml` | CI entrypoint |
| `src/patchworks/__init__.py` | Package root |
| `src/patchworks/__main__.py` | CLI entrypoint |
| `src/patchworks/cli.py` | CLI subcommand dispatch and implementations |
| `src/patchworks/db/inspector.py` | SQLite inspection, paging, and summary loading |
| `src/patchworks/db/differ.py` | Diff orchestration |
| `src/patchworks/db/snapshot.py` | Snapshot persistence and local store behavior |
| `src/patchworks/db/migration.py` | Migration chain persistence and conflict detection |
| `src/patchworks/db/types.py` | Core data types |
| `src/patchworks/diff/schema.py` | Schema diff rules |
| `src/patchworks/diff/data.py` | Streaming row-level diffing |
| `src/patchworks/diff/export.py` | SQL migration generation |
| `src/patchworks/diff/semantic.py` | Semantic diff awareness (renames, type shifts) |
| `src/patchworks/diff/merge.py` | Three-way merge and conflict detection |
| `src/patchworks/diff/migration.py` | Migration generation, validation, rollback, squashing |
| `tests/` | All test modules |

**Invariant:** If docs, code, and CLI output ever disagree, the next change must reconcile all three. When docs and code disagree, code and tests win - update docs immediately.

---

## Working rules

1. **Read `BUILD.md` and `README.md`** before making substantial changes.
2. **Keep the layering honest.** `db/` and `diff/` own data inspection, comparison, snapshots, and export logic. `cli.py` is a thin dispatch layer. No business logic in the CLI module.
3. **Do not claim flows are verified** unless they were actually exercised and recorded.
4. **When behavior changes** around snapshots, SQL export, diff correctness, or live-database handling, update this file in the same pass.
5. **Prefer streaming and bounded-memory approaches** when touching inspection, diff, or export hot paths.
6. **Keep `README.md` aligned with recorded reality.** Do not market future work as present capability.
7. **Type hints on all public functions.** Pyright must pass clean.
8. **Correctness over cleverness.** A heavier migration that is semantically correct beats a minimal one that breaks edge cases.
9. **SQLite-native.** Preserve the nuance of SQLite (`rowid`, `WITHOUT ROWID`, WAL, PRAGMA behavior) instead of flattening into generic database abstractions.
10. **Every phase gets verification before the checkbox.** No checking boxes on vibes.

---

## Tracking conventions

| Term | Meaning |
|------|---------|
| **done** | Implemented and verified |
| **checked** | Verified by command or test output |
| **in progress** | Actively being worked on |
| **queued** | Intentionally next, not started yet |
| **planned** | Real roadmap work, not yet queued |
| **exploratory** | Needs sharper scope or decisions before building |
| **blocked** | Cannot proceed without a decision or dependency |
| **risk** | Plausible failure mode that could distort the design |
| **decision** | A durable call with consequences |

### Progress log format

`YYYY-MM-DD: scope - outcome. Verified with: <commands>. Next: <follow-up>.`

### Decision log format

`YYYY-MM-DD: decision - rationale - consequence.`

---

## Quality gates

### Standard gate (all phases)

```bash
uv sync
ruff check .
ruff format --check .
pyright
pytest
```

### Extended gate (before releases)

```bash
uv sync
ruff check .
ruff format --check .
pyright
pytest --cov=patchworks --cov-report=term-missing
patchworks --help
patchworks inspect --help
patchworks diff --help
```

### Go gate (when Go components exist)

```bash
cd go/
go build ./...
go test ./...
go vet ./...
golangci-lint run
govulncheck ./...
```

Do not record these as passed unless they were actually run. If a gate is temporarily unavailable, document why.

---

## Dependency strategy

**Python dependencies** are managed through `pyproject.toml` with `uv`.

Principles:

- Every dependency must justify itself against implementing the functionality directly
- The standard library covers more than most people think - prefer it
- `sqlite3` is stdlib; no external SQLite driver
- Separate runtime and dev dependencies
- Lock dependencies with `uv lock`; commit the lockfile
- Prefer packages with type stubs or inline types

**Go dependencies** (when applicable) are managed through `go.mod`.

Principles:

- Standard library first
- `modernc.org/sqlite` for SQLite if Go needs direct database access
- No CGO dependencies

---

## Current priority stack

### Priority 1 - Scaffold and core engine

Get the Python project bootstrapped and build the core inspection and diff engine.

### Priority 2 - CLI surface

Expose all core capabilities through a CLI with subcommands, JSON output, and CI-friendly exit codes.

### Priority 3 - Full feature set

Snapshots, export, merge, and migration workflows.

---

## Phase dashboard

| Phase | Name | Status |
|-------|------|--------|
| 0 | Scaffold and bootstrap | **Done** |
| 1 | Core SQLite inspection engine | **Done** |
| 2 | Schema and row diffing | **Done** |
| 3 | Snapshot management | **Done** |
| 4 | SQL export and migration generation | **Done** |
| 5 | CLI surface | **Done** |
| 6 | Advanced diff intelligence | **Done** |
| 7 | Three-way merge | **Done** |
| 8 | Migration workflow management | **Planned** |
| 9 | Local web UI | **Planned** |
| 10 | Go acceleration layer | **Exploratory** |
| 11 | CI/CD integration and automation | **Planned** |
| 12 | Plugin and extension architecture | **Exploratory** |
| 13 | Team features and shared snapshot registries | **Exploratory** |

---

### Phase 0 - Scaffold and bootstrap
**Status: done**

Bootstrap the Python project structure, tooling, and CI pipeline.

- [x] Run `uv init` with `src/` layout and `pyproject.toml`
- [x] Pin Python version in `.python-version` (3.12+)
- [x] Add `ruff` and `pyright` to dev dependencies
- [x] Configure `ruff` in `pyproject.toml` (linting + formatting)
- [x] Configure `pyright` in `pyproject.toml`
- [x] Set up `pytest` with `pytest-cov`
- [x] Set up `.pre-commit-config.yaml` with ruff and pyright hooks
- [x] Create package structure: `src/patchworks/` with `__init__.py`, `__main__.py`
- [x] Create `src/patchworks/db/` and `src/patchworks/diff/` subpackages
- [x] Create `tests/` directory with initial test file
- [x] Add CI workflow (`.github/workflows/ci.yml`): `uv sync` → `ruff` → `pyright` → `pytest`
- [x] Add CI matrix for Linux and macOS
- [x] Update `ARCHITECTURE.md` for Python + Go structure
- [x] Update `CONTRIBUTING.md` for Python toolchain
- [x] Verify `patchworks --help` works as an installed CLI entry point

Exit criteria:

- [x] `uv sync && ruff check . && ruff format --check . && pyright && pytest` passes
- [x] CI runs green on both Linux and macOS
- [x] `uv run patchworks --help` produces output

---

### Phase 1 - Core SQLite inspection engine
**Status: done**

Build the SQLite reading layer that all other features depend on.

- [x] Implement `db/types.py`: core data types (`DatabaseSummary`, `TableInfo`, `ColumnInfo`, `IndexInfo`, `TriggerInfo`, `ViewInfo`)
- [x] Implement `db/inspector.py`: read schema from `sqlite_master`
- [x] Inspect tables, views, indexes, and triggers with full metadata
- [x] Read table rows with pagination and configurable page size
- [x] Add `for_each_row()` streaming iterator for bounded-memory row access
- [x] Support sorted pagination with deterministic primary-key or `rowid` tie-breakers
- [x] Open databases in read-only mode (`file:...?mode=ro` URI)
- [x] Handle WAL-mode databases
- [x] Add comprehensive tests for schema reading, row pagination, edge cases (empty db, no tables, WITHOUT ROWID)

Exit criteria:

- [x] `inspect_database()` returns a complete `DatabaseSummary` for any valid SQLite file
- [x] Row pagination is deterministic and bounded
- [x] Tests cover normal, empty, WAL-mode, and WITHOUT ROWID databases

---

### Phase 2 - Schema and row diffing
**Status: done**

Build the comparison engine.

- [x] Implement `diff/schema.py`: schema-level diffing (added, removed, modified tables, indexes, triggers)
- [x] Implement `diff/data.py`: streaming row-level diff using primary key matching
- [x] Fall back to `rowid` when primary keys diverge, with warnings
- [x] Track per-cell changes within modified rows
- [x] Implement diff result types (`SchemaDiff`, `TableDataDiff`, `RowDiff`, `CellChange`)
- [x] Implement `db/differ.py`: high-level diff orchestration combining schema and row diffs
- [x] Add integration tests for schema diffs, row diffs, mixed changes, edge cases

Exit criteria:

- [x] Schema diffs detect all object-level changes between two databases
- [x] Row diffs are correct for added, removed, and modified rows with per-cell detail
- [x] Streaming comparison does not materialize full tables in memory

---

### Phase 3 - Snapshot management
**Status: done**

Local snapshot store for capturing database state.

- [x] Implement `db/snapshot.py`: `SnapshotStore` managing `~/.patchworks/patchworks.db`
- [x] Create metadata SQLite database with `snapshots` table
- [x] Save snapshots: copy database file to `~/.patchworks/snapshots/<uuid>.sqlite`, record metadata
- [x] List snapshots with optional source filter
- [x] Delete snapshots (metadata + file)
- [x] Support snapshot naming via `--name` flag
- [x] Add tests for save, list, delete, and edge cases

Exit criteria:

- [x] Snapshots are saved, listed, and deleted correctly
- [x] Snapshot metadata is queryable and consistent with files on disk

---

### Phase 4 - SQL export and migration generation
**Status: done**

Generate SQL that transforms one database into another.

- [x] Implement `diff/export.py`: SQL migration generation from diff results
- [x] Use temporary-table rebuild for schema-changed tables
- [x] Guard `PRAGMA foreign_keys` in generated SQL
- [x] Drop and recreate affected triggers around migration DML
- [x] Implement streaming `write_export()` that writes one statement at a time to any file-like object
- [x] Implement convenience `export_as_sql()` that returns a string (for preview use)
- [x] Use `for_each_row()` for bounded-memory table seeding during export
- [x] Add integration tests: round-trip export application, foreign-key safety, trigger preservation, large tables

Exit criteria:

- [x] Generated SQL transforms the left database into the right database when applied
- [x] Export handles schema changes, added/removed tables, and row-level changes correctly
- [x] Streaming export path has bounded memory usage

---

### Phase 5 - CLI surface
**Status: done**

Expose all capabilities through subcommands.

- [x] Implement `cli.py` with argparse subcommand dispatch
- [x] `patchworks inspect <db>` - schema, tables, columns, views, indexes, triggers
- [x] `patchworks diff <left> <right>` - schema and row diffs
- [x] `patchworks export <left> <right>` - SQL migration output
- [x] `patchworks snapshot save <db>` with `--name`
- [x] `patchworks snapshot list` with optional `--source` filter
- [x] `patchworks snapshot delete <uuid>`
- [x] Add `--format human|json` on inspect, diff, and snapshot list
- [x] Add `-o/--output` on export for file output
- [x] Define exit codes: 0 = success/no differences, 1 = error, 2 = differences found
- [x] Add `__main__.py` entry point so `python -m patchworks` works
- [x] Register console script in `pyproject.toml` so `patchworks` works after install
- [x] Add CLI integration tests

Exit criteria:

- [x] All core capabilities are accessible via CLI subcommands
- [x] JSON output is machine-readable and stable
- [x] Exit codes are consistent and documented
- [x] CLI calls the same backend functions as any future UI surface - no forked logic

---

### Phase 6 - Advanced diff intelligence
**Status: done**

Richer diff analysis beyond raw deltas.

- [x] Implement `diff/semantic.py`: semantic diff awareness
- [x] Detect table renames via column similarity scoring
- [x] Detect column renames via property matching (type, nullable, pk, default)
- [x] Detect compatible type shifts using SQLite type affinity rules
- [x] Add confidence scores for heuristic detections
- [x] Add diff filtering by change type (added/removed/modified) and by table
- [x] Add aggregate diff summary statistics (table counts, row counts, cell changes, schema object counts)
- [x] Add data-type-aware comparison (integer 1 vs real 1.0, text "42" vs integer 42)
- [x] Add diff annotations for triage workflows (pending/approved/rejected/needs-discussion/deferred)
- [x] Wire semantic analysis and filtering into CLI output
- [x] Add tests for all semantic detection paths and edge cases

Exit criteria:

- [x] Semantic renames and type shifts are detected with confidence scores
- [x] Diff output can be filtered and summarized
- [x] Data-type-aware comparison distinguishes cosmetic from semantic differences

---

### Phase 7 - Three-way merge
**Status: done**

Merge changes from two databases against a common ancestor.

- [x] Implement `diff/merge.py`: three-way merge engine
- [x] Diff both derived databases against the ancestor
- [x] Merge non-conflicting row and schema changes
- [x] Surface conflict types: row conflict, schema conflict, delete-modify conflict, table-delete conflict
- [x] Add `patchworks merge <ancestor> <left> <right>` CLI subcommand
- [x] Support `--format human|json` on merge output
- [x] Add tests for non-conflicting merges, each conflict type, and edge cases

Exit criteria:

- [x] Non-conflicting changes merge correctly
- [x] Conflicts are detected and clearly reported with enough context for manual resolution

---

### Phase 8 - Migration workflow management
**Status: planned**

Ordered migration sequences with validation and safety.

- [ ] Implement `db/migration.py`: `MigrationStore` in `~/.patchworks/patchworks.db`
- [ ] Implement `diff/migration.py`: generation, validation, rollback, squashing
- [ ] Generate forward migrations using the diff+export engine
- [ ] Generate reverse migrations for rollback
- [ ] Validate migrations by applying to a temporary copy and diffing against target
- [ ] Squash sequential migrations into a single migration
- [ ] Detect conflicts between migrations targeting the same objects
- [ ] Add `patchworks migrate` subcommand family: `generate`, `validate`, `list`, `show`, `apply`, `delete`, `squash`, `conflicts`
- [ ] Add `--dry-run` mode for generate, apply, and squash
- [ ] Add `--format human|json` on all migrate subcommands
- [ ] Add comprehensive tests for all migration operations

Exit criteria:

- [ ] Users can generate, validate, apply, rollback, and squash migrations via CLI
- [ ] `--dry-run` never modifies the target database
- [ ] Migration state is persisted and queryable

---

### Phase 9 - Local web UI
**Status: planned**

Interactive browser-based interface served locally via FastAPI + htmx.

- [ ] Add FastAPI to dependencies
- [ ] Implement `web/` package with routes, templates, and static assets
- [ ] Schema browser with table/view/index/trigger listing and DDL preview
- [ ] Table row browser with pagination
- [ ] Diff viewer with collapsible sections and summary statistics
- [ ] SQL export preview
- [ ] Snapshot management panel
- [ ] Add `patchworks serve` CLI subcommand to launch local web server
- [ ] Keep the web UI on the same backend truth layer as the CLI
- [ ] Add Jinja2 templates with htmx for dynamic interaction
- [ ] Add light/dark theme support

Exit criteria:

- [ ] The web UI provides interactive browsing and diff review equivalent to the former desktop app
- [ ] `patchworks serve` launches a local server that works without external dependencies
- [ ] The web UI calls the same backend functions as the CLI - no forked logic

---

### Phase 10 - Go acceleration layer
**Status: exploratory**

Optional Go components for performance-critical paths.

- [ ] Profile Python hot paths with realistic large databases (100k+ rows, 50+ tables)
- [ ] Identify bottlenecks where Go would provide material improvement
- [ ] Decide on integration approach: subprocess, shared library via ctypes, or local HTTP service
- [ ] Implement Go components under `go/` directory with standard Go toolchain
- [ ] Use `modernc.org/sqlite` for any Go-side database access (no CGO)
- [ ] Add Go CI gate alongside Python gate
- [ ] Maintain Python fallback for all Go-accelerated paths

Exit criteria:

- [ ] Profiling evidence justifies the added complexity
- [ ] Go components provide measurable speedup on identified bottlenecks
- [ ] Python fallback works identically, just slower

---

### Phase 11 - CI/CD integration and automation
**Status: planned**

- [ ] Add `patchworks check` command for CI gates
- [ ] Add GitHub Actions integration examples
- [ ] Add pre-commit hook support
- [ ] Stabilize JSON output contracts across all commands
- [ ] Evaluate `patchworks watch` mode for file-change monitoring
- [ ] Add GitOps workflow documentation

Exit criteria:

- [ ] Patchworks can be dropped into CI with a small, documented command surface
- [ ] Machine-readable output and exit codes are stable enough for automation

---

### Phase 12 - Plugin and extension architecture
**Status: exploratory**

- [ ] Design plugin surface for alternate diff formatters (HTML, Markdown)
- [ ] Design plugin surface for export targets (Alembic, Flyway, Liquibase)
- [ ] Design plugin surface for custom inspectors or validation rules
- [ ] Evaluate discovery and loading mechanisms (entry points, importlib)
- [ ] Define plugin API stability and versioning expectations

Exit criteria:

- [ ] Third-party developers can extend patchworks without forking
- [ ] The extension surface has a stability story

---

### Phase 13 - Team features and shared snapshot registries
**Status: exploratory**

- [ ] Design a snapshot registry protocol for optional push/pull workflows
- [ ] Add snapshot naming, tagging, and annotation
- [ ] Add snapshot comparison across machines
- [ ] Add snapshot retention policies and integrity verification
- [ ] Keep local-only workflows first-class

Exit criteria:

- [ ] Teams can share snapshots without breaking the local-first model

---

## Decisions

Decisions are numbered and durable. New decisions append; old decisions are never deleted.

### decision-0001: Python-first architecture
**Date:** 2026-03-25

Python owns the CLI, orchestration, diff logic, export generation, and all user-facing tooling. Go is reserved for performance-critical hot paths only when profiling evidence justifies the added complexity. This keeps the codebase in one language for as long as possible.

### decision-0002: Snapshot state under `~/.patchworks/`
**Date:** 2026-03-25

Snapshot metadata and copied database files live under `~/.patchworks/`. Local machine state, not project state.

### decision-0003: SQL export favors correctness over minimality
**Date:** 2026-03-25

Modified tables are rebuilt via temporary replacement. Exports may be heavier, but semantic fidelity takes priority.

### decision-0004: Views remain inspect-only
**Date:** 2026-03-25

Views are browsable but not diffed or exported. Revisit when demand or diff engine capability justifies it.

### decision-0005: stdlib sqlite3 over external drivers
**Date:** 2026-03-25

Python's bundled `sqlite3` module is sufficient for read-only inspection and comparison. No external driver needed. If Go components are added later, they use `modernc.org/sqlite` (no CGO).

### decision-0006: argparse over click/typer
**Date:** 2026-03-25

`argparse` is stdlib and handles the subcommand structure without adding a dependency. The CLI surface is complex but not so complex that a framework pays for itself. Revisit if CLI ergonomics become a real friction point.

### decision-0007: No desktop GUI
**Date:** 2026-03-25

No desktop GUI. The CLI is the primary surface. A local web UI via FastAPI + htmx is a later phase.

### decision-0008: CLI and web UI share the same backend
**Date:** 2026-03-25

The CLI and any future web UI call the same `inspect_database`, `diff_databases`, `write_export`, and `SnapshotStore` functions. No forked logic between surfaces.

---

## Risks

### Active risks

- Python's `sqlite3` module may have performance limitations on very large databases (millions of rows)
- Streaming row comparison in Python needs bounded-memory correctness as the priority over raw speed
- The web UI phase adds FastAPI as a runtime dependency
- Live databases, WAL-backed databases, and actively changing sources are handled best-effort with read-only access; concurrent writes during inspection can produce inconsistent results

### Strategic risks

- Go acceleration layer may never be needed, but planning for it adds architectural complexity to the Python code
- Plugin system introduces API stability obligations that constrain future refactoring
- Multi-engine support could dilute SQLite-specific correctness if abstraction boundaries are drawn wrong

---

## Open questions

| Question | Phase | Impact |
|----------|-------|--------|
| Is `argparse` sufficient for the full subcommand tree, or should we switch to `click`? | 5 | CLI ergonomics |
| Should the web UI use WebSocket for progress reporting on long diffs? | 9 | UX |
| Is Go acceleration actually needed, or is Python fast enough for realistic workloads? | 10 | Architecture |
| Should the plugin system use Python entry points, importlib, or something else? | 12 | Extension surface |
| Should the snapshot registry be a separate service or embedded in the CLI? | 13 | Architecture |

---

## Resume checklist

If picking this repo up for the next pass:

1. Read `BUILD.md` (this file) and `README.md`
2. Run the quality gate: `uv sync && ruff check . && ruff format --check . && pyright && pytest`
3. Check the phase dashboard above for current status
4. Look at the current priority stack for what to build next
5. If a phase is in progress, check its checkboxes for what remains
6. Update this file in the same pass as any code change

---

## Immediate next moves

1. **Complete Phase 7 - three-way merge.** `diff/merge.py`, merge CLI subcommand, conflict detection and reporting.
2. **Build Phase 8 - migration workflow management.** `db/migration.py`, `diff/migration.py`, generate/validate/apply/squash migrations.
3. **Build Phase 9 - local web UI.** FastAPI + htmx for interactive browsing and diff review.

If priorities change, replace this list rather than letting stale direction linger.

---

## Progress log

*Update this log only with things that actually happened.*

### 2026-03-26

- Completed Phase 7: three-way merge engine (`diff/merge.py`), CLI `merge` subcommand with `--format human|json`, comprehensive tests for non-conflicting merges, row conflicts, schema conflicts, delete-modify conflicts, table-delete conflicts, and edge cases. Verified with: `uv run ruff check . && uv run ruff format --check . && uv run pyright && uv run pytest`. Next: Phase 8 migration workflow management.
- Updated BUILD.md: checked off all Phase 0–6 boxes against actual codebase. All 202+ tests passing. All quality gates green.

### 2026-03-25

- Completed Phase 6: advanced diff intelligence. `diff/semantic.py` with table rename detection, column rename detection, type shift detection with SQLite affinity rules, diff filtering, aggregate summary, data-type-aware comparison, annotations. CLI wired with `--summary`, `--semantic`, `--change-type`, `--table` flags. 202 tests passing.
- Completed Phase 5: CLI surface. `cli.py` with argparse subcommand dispatch for inspect, diff, export, snapshot (save/list/delete), merge (stub), migrate (stub), serve (stub). `--format human|json`, `-o/--output`, exit codes 0/1/2. CLI integration tests.
- Completed Phase 4: SQL export and migration generation. `diff/export.py` with streaming `write_export()` and convenience `export_as_sql()`. Temporary-table rebuild for schema changes, PRAGMA foreign_keys guards, trigger drop/recreate, bounded-memory row seeding via `for_each_row()`. Round-trip integration tests.
- Completed Phase 3: snapshot management. `db/snapshot.py` with `SnapshotStore` managing `~/.patchworks/patchworks.db`. Save, list, delete, source filtering, naming. Comprehensive tests.
- Completed Phase 2: schema and row diffing. `diff/schema.py` for schema-level diffs, `diff/data.py` for streaming row-level diffs with PK matching and rowid fallback. `db/differ.py` for orchestration. Integration tests for schema diffs, row diffs, mixed changes, edge cases.
- Completed Phase 1: core SQLite inspection engine. `db/types.py` with all data types, `db/inspector.py` with schema reading, row pagination, `for_each_row()` streaming, read-only mode, WAL support. Tests for normal, empty, WAL-mode, and WITHOUT ROWID databases.
- Completed Phase 0: scaffold and bootstrap. `uv init`, `pyproject.toml`, `.python-version`, ruff, pyright, pytest, pre-commit, package structure, CI workflow with Linux/macOS matrix. `patchworks --help` works.
- Created BUILD.md and README.md. Product vision and feature set defined. All phases have fresh unchecked boxes. No code yet - planning only. Next: Phase 0 scaffold and bootstrap.

---

## Decision log

- 2026-03-25: Python-first architecture - Python owns CLI, orchestration, diff logic, export generation, and user-facing tooling; Go deferred until profiling justifies it - keeps the codebase in one language as long as possible.
- 2026-03-25: stdlib sqlite3 over external drivers - Python's bundled sqlite3 is sufficient for read-only inspection - no external dependency needed.
- 2026-03-25: argparse over click/typer - stdlib handles the subcommand structure without adding a dependency - revisit if CLI ergonomics become friction.
- 2026-03-25: No desktop GUI - CLI is primary, FastAPI + htmx web UI is a later phase.
- 2026-03-25: Snapshot location, export correctness, views inspect-only, and backend sharing decisions established.
