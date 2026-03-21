# BUILD.md

**This is the primary operational handoff document for Patchworks. It is a living document, and every future agent or developer who touches this repository is responsible for keeping it accurate, current, and up to date.** If you verify a workflow, change behavior, add tooling, or discover a bug, update this file in the same pass.

## Last Verified Baseline

- Verified on: 2026-03-21
- Repo path: `/Users/sawyer/github/patchworks`
- Branch: `main`
- Base commit at start of this pass: `8824de772ce03e46b2de9fd95d194cfff326b730`
- Host used for verification: macOS arm64 (`Darwin 25.4.0`)
- Last full code review: 2026-03-20
- This verification was performed against the working tree rooted at the commit above before the changes from this pass were committed.

## Completed In This Pass

- Added crates.io-facing package metadata in `Cargo.toml`, including repository, homepage, documentation, keywords, and categories.
- Updated `README.md` so the packaged readme uses a relative `LICENSE` link and documents `cargo package --allow-dirty` as the local publishability check.
- Verified package contents and local package validation with `cargo package --allow-dirty --list`, `cargo package --allow-dirty`, and `cargo test`.

## Project Baseline

Patchworks is a single-crate Rust desktop application for inspecting and diffing SQLite databases. The current product is a Phase 1 `egui`/`eframe` desktop app with a small CLI wrapper.

What it currently does:

- Open zero, one, or two SQLite database files in a native desktop UI.
- Inspect tables and views from a SQLite file.
- Track indexes and triggers from `sqlite_master` for schema comparison and SQL export.
- Browse table rows with pagination and sortable columns.
- Diff two databases at schema level and row level.
- Save snapshots of a database into a local Patchworks store under `~/.patchworks/`.
- Compare a live database against a saved snapshot.
- Generate SQL intended to transform the left database into the right database, including tracked index and trigger definitions.
- Preview SQL in the UI, copy it to the clipboard, or save it to disk.

What it does not currently do:

- It does not provide headless CLI subcommands for diffing, inspection, SQL export, snapshot listing, or snapshot cleanup.
- It does not diff or export views; views are inspected and listed only.
- It does not include packaging, installer, or release automation in this repo.
- It does not include Docker, migrations, or environment-file driven setup.

## Major Components And Entry Points

- `Cargo.toml`
  Single Rust package manifest and dependency source of truth.
- `src/main.rs`
  Binary entrypoint. Parses CLI args, supports `--snapshot`, otherwise launches the `egui` app.
- `src/lib.rs`
  Re-exports the crate modules.
- `src/error.rs`
  Shared error types using `thiserror`. Defines `PatchworksError` and a crate-level `Result` alias.
- `src/app.rs`
  Top-level app coordinator. Loads left/right databases, computes diffs, saves snapshots, refreshes visible tables, and wires UI actions to backend modules.
- `src/db/`
  Database-facing backend code.
  - `inspector.rs`: read-only SQLite inspection, paging, sorting, row loading.
  - `differ.rs`: high-level orchestration for schema diff, row diff, and SQL export.
  - `snapshot.rs`: snapshot persistence in `~/.patchworks/patchworks.db` plus copied `.sqlite` files under `~/.patchworks/snapshots/`.
  - `types.rs`: shared data structures for summaries, table pages, diffs, and snapshots.
- `src/diff/`
  Diff and export algorithms.
  - `schema.rs`: table/column schema diffing.
  - `data.rs`: streaming row diffing across shared tables.
  - `export.rs`: SQL export generation.
- `src/state/`
  UI state containers for panes, diff mode, selected tables, and active view.
- `src/ui/`
  `egui` rendering layer for file panels, table views, schema diff, row diff, snapshots, and SQL export.
- `tests/`
  Integration and unit tests. These are the best executable specification of current behavior.
  - `tests/diff_tests.rs`
  - `tests/proptest_invariants.rs`
  - `tests/snapshot_tests.rs`
  - `tests/support/mod.rs`
  - `tests/fixtures/create_fixtures.sql`
  - `src/app.rs` also now includes focused unit tests for background diff task coordination.
- `benches/`
  Criterion benchmarks for the main diff and query hot paths.
  - `benches/diff_hot_paths.rs`
  - `benches/query_hot_paths.rs`
- `.github/workflows/ci.yml`
  GitHub Actions CI entrypoint for format, lint, test, benchmark compile, and dependency-policy checks.
- `deny.toml`
  `cargo-deny` configuration for advisories, source policy, allowed licenses, and duplicate-version warnings.

## Current Implemented State

The implemented system is cohesive, lightweight, and intentionally narrow in scope.

- CLI surface today:
  - `patchworks`
  - `patchworks <db>`
  - `patchworks <left.db> <right.db>`
  - `patchworks --snapshot <db>`
- Snapshot behavior:
  - Snapshot metadata is stored in `~/.patchworks/patchworks.db`.
  - Snapshot database copies are stored in `~/.patchworks/snapshots/<uuid>.sqlite`.
  - The store schema is created lazily by `SnapshotStore::ensure_schema()`.
- Diff behavior:
  - Schema diff still centers on table/column changes in the UI, but the inspected model now also tracks indexes and triggers from `sqlite_master` so export planning can preserve them.
  - Row diff is computed only for tables that exist on both sides.
  - Row diff prefers a shared primary key and falls back to `rowid` with warnings.
  - Large-table warning threshold is `100_000` rows.
  - Equality for row diffs now follows `compare_sql_values()` rather than `SqlValue`'s derived `PartialEq`.
  - Diff requests now run on a background thread and are applied back into the workspace on the next UI update tick.
- SQL export behavior:
  - Added tables are created and fully seeded from the right database.
  - Removed tables are dropped.
  - Modified tables are rebuilt from the right-side schema and reseeded from the right database.
  - Unchanged-schema tables get incremental `DELETE`/`INSERT`/`UPDATE` statements.
  - Changed indexes are dropped and recreated from the right-side schema definitions.
  - Triggers on affected tables are dropped before migration DML and recreated from the right side after data changes finish so export application does not accidentally execute left-side trigger logic.
  - Rowid-fallback deletes use the stored row identity instead of guessing from the first visible column.
  - Missing primary-key columns or row values during export are now treated as an explicit invalid state.
- Quality workflow behavior:
  - Benchmarks now exist for paged table reads, isolated row diffs, and end-to-end diff generation.
  - Property tests now exercise schema diff invariants, row-diff accounting invariants, and SQL export round-tripping.
  - The repo now includes GitHub Actions CI and a checked-in `cargo-deny` policy.
  - `cargo nextest run` is the preferred fast local test command; `cargo nextest run --all-targets` also discovers the Criterion bench binaries and is slower than needed for routine CI.
- UI behavior:
  - Starting the app with two database paths queues a diff automatically without blocking the UI thread.
  - Toolbar-initiated diffs now run in the background and show a spinner while work is in flight.
  - Loading a new left-side or right-side database clears any prior diff result and drops the receiver for an older in-flight diff task.
  - Opening a right-side database from the toolbar loads it, but the user must click `Diff`.
  - The tabular diff display mode is now labeled `Grid`, which matches the current rendering.
  - SQL export can be copied to the clipboard or saved through a native save dialog.
  - Views are listed in the side panel, but remain inspect-only.
- Table paging behavior:
  - The app now reuses already-inspected `TableInfo` values when refreshing visible table pages.
  - User-selected sorts now append a deterministic primary-key / `rowid` tie-breaker so duplicate sort values page stably across boundaries.
  - The public `read_table_page()` helper still performs its own lookup for callers that only have a path and table name.

## Verified Build And Run Workflow

### Prerequisites

- Rust toolchain with `cargo`
- `rustfmt`
- `clippy`
- `cargo-nextest` for the fast local/CI test runner
- `cargo-deny` for dependency policy checks
- A desktop environment if you want to launch the GUI

Notes:

- No `.env` file, runtime secret, or external service dependency was found.
- `rusqlite` is built with `bundled`, so a system SQLite library is not required for the app build.
- `sqlite3` was only used for a local smoke test; it is not a project dependency.

### Verified Commands For This Pass

The following commands were run successfully in `/Users/sawyer/github/patchworks` on 2026-03-21:

```bash
cargo metadata --no-deps --format-version 1
cargo package --allow-dirty --list
cargo package --allow-dirty
cargo test
```

Still valid from the previous 2026-03-20 verification baseline, but not re-run in this pass:

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run
cargo build
cargo bench --no-run
cargo deny check
cargo run -- --help
```

The previous pass also verified a CLI snapshot smoke test with `cargo run -- --snapshot <db>`, including creation of the default store under `~/.patchworks/`.

### Unverified But Likely Commands

These commands are supported by the code, but I did not fully verify the GUI interaction path in this session:

```bash
cargo run
cargo run -- path/to/database.sqlite
cargo run -- left.sqlite right.sqlite
```

Likely local install command, still unverified in this pass:

```bash
cargo install --path .
```

Important packaging note:

- `patchworks` is now published on crates.io at <https://crates.io/crates/patchworks>.
- `cargo install patchworks` is the expected install path for end users; this exact install flow was not re-verified in this pass.
- `cargo package --allow-dirty` now completes successfully after adding repository/homepage/documentation metadata and ensuring the packaged README points at a relative `LICENSE` path.

## Source-Of-Truth Notes

Authoritative files:

- `Cargo.toml`
  Dependencies and crate metadata.
- `src/main.rs`
  True CLI surface and app startup behavior.
- `src/app.rs`
  Operational source of truth for how the UI loads databases, computes diffs, refreshes visible tables, and saves snapshots.
- `src/db/*.rs` and `src/diff/*.rs`
  Source of truth for inspection, diffing, snapshot storage, and SQL export behavior.
- `tests/diff_tests.rs`, `tests/proptest_invariants.rs`, and `tests/snapshot_tests.rs`
  Best executable specification of current expected behavior.
- `src/app.rs`
  Source of truth for the background diff task lifecycle and its focused unit-test coverage.
- `tests/support/mod.rs`
  Shared integration-test helpers for fixtures and temporary SQLite databases.
- `tests/fixtures/create_fixtures.sql`
  Canonical test data for schema diff, data diff, rowid fallback, SQL export, and snapshot tests.
- `benches/*.rs`
  Source of truth for the currently tracked diff and query hot-path benchmarks.
- `.github/workflows/ci.yml`
  Source of truth for the checked-in CI workflow.
- `deny.toml`
  Source of truth for dependency-policy expectations.

Useful but secondary docs:

- `README.md`
  High-level product description and local workflow guide. Keep it aligned with verified commands and current scope.
- `AGENTS.md`
  Secondary agent memory for Codex sessions. Useful for architecture and conventions, but it does not replace direct verification.

Current doc and behavior alignment notes:

- `Cargo.toml` now includes crates.io-facing metadata for repository, homepage, documentation, keywords, and categories.
- `README.md` now includes the verified `nextest`, benchmark, and `cargo-deny` workflows alongside the build/test commands, notes that diffs run in the background, documents `cargo package --allow-dirty` for local publishability checks, and uses a relative `LICENSE` link that will render correctly from the packaged crate.
- `README.md` now also calls out that views remain inspect-only, tracked indexes/triggers are preserved in generated SQL, stable sorted pagination uses a primary-key / `rowid` tie-breaker, and large/live-database caveats still apply.
- `AGENTS.md` now mentions the benchmark, property-test, CI, dependency-policy workflows, and the background diff behavior.
- There is still no real screenshot committed to the repo.

Config and state files that affect behavior:

- There is no checked-in runtime config file.
- GitHub Actions CI now lives in `.github/workflows/ci.yml`.
- Dependency policy is configured in `deny.toml`.
- User-local snapshot state lives outside the repo in `~/.patchworks/`.
- Build output goes to `target/`.

## Code Review Findings

Full review of every source file completed 2026-03-20. `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo fmt --all --check` pass clean after the changes in this pass. `cargo nextest run` remains valid from the previous 2026-03-20 verification baseline but was not re-run in this follow-up.

### Overall Assessment

The codebase is solid for a Phase 1 product. Code is well-organized, cleanly separated into layers (`db`, `diff`, `state`, `ui`), and follows Rust conventions. Error handling is consistent (`thiserror` in the library, `anyhow` in `main`). The streaming merge-diff approach in `data.rs` is still a strong architectural choice. No `unsafe` code was found.

### Remaining Findings

1. **SQL export still materializes the entire migration as one `String`** (`src/db/differ.rs`, `src/diff/export.rs`). Very large exports can consume substantial memory.

2. **`load_all_rows()` still materializes whole tables during export seeding** (`src/db/inspector.rs`). Added or rebuilt tables with millions of rows can still OOM.

3. **Background diff execution is still fire-and-forget with no progress or cancellation model** (`src/app.rs`, `src/ui/diff_view.rs`). The UI stays responsive now, but large diffs still offer only an indeterminate spinner and last-request-wins behavior.

4. **`SnapshotStore` still opens a fresh SQLite connection for each operation** (`src/db/snapshot.rs`). The code is simple and correct, but it is less efficient than keeping a persistent connection around.

5. **`list_snapshots()` still relies on canonicalized source paths** (`src/db/snapshot.rs`). If a database is moved or opened through a path that no longer canonicalizes to the stored location, the snapshot list will come back empty without a tailored explanation.

6. **`main.rs` still rewrites `-snapshot` to `--snapshot` manually.** The behavior works, but the workaround remains undocumented in the CLI definition itself.

7. **WAL-mode or actively changing databases are still only best-effort.** The repo has no explicit handling or user-facing documentation for encrypted databases, active writers, or WAL consistency edge cases.

### 2026-03-20 follow-up review addendum

This follow-up review re-read the full repository and re-ran `cargo test` plus `cargo clippy --all-targets --all-features -- -D warnings`; both passed clean.

#### Strengths

- **Architecture is clean and easy to reason about.** The `db` / `diff` / `state` / `ui` split is consistent, and `src/app.rs` stays mostly as coordinator code rather than mixing persistence and rendering logic.
- **Core diff correctness has better-than-average coverage for a Phase 1 app.** Integration tests, property tests, and SQL round-trip checks meaningfully exercise schema diffing, row diffing, rowid fallback, and export behavior.
- **Quality gates are solid for a single-crate desktop project.** CI covers fmt, clippy, nextest, doc tests, bench compilation, and `cargo-deny`, which is a strong baseline.

#### Highest-priority risks

1. **Database loading is still synchronous and can block the UI on large files.** `load_left()` / `load_right()` call `inspect_database()` directly from the app thread (`src/app.rs`), and `inspect_database()` performs `COUNT(*)` for every table (`src/db/inspector.rs`). Background diffing helped, but opening a large database can still freeze the interface before the user ever clicks `Diff`.

2. **Export memory use is still unbounded for large migrations.** `export_diff_as_sql()` builds one large `String`, and `append_create_and_seed()` calls `load_all_rows()` to materialize full tables before emitting inserts (`src/diff/export.rs`, `src/db/inspector.rs`). That is the biggest scalability risk in the current implementation.

3. **CI is Linux-only even though the product is a desktop app with native dialogs.** `.github/workflows/ci.yml` runs on `ubuntu-latest` only. That is good for core Rust logic, but it will not catch macOS-specific `eframe` / `rfd` build or runtime regressions.

4. **`SnapshotStore` still opens a fresh SQLite connection for each operation** (`src/db/snapshot.rs`). The code is simple and correct, but it is less efficient than keeping a persistent connection around.

5. **WAL-mode or actively changing databases are still only best-effort.** The repo has no explicit handling or user-facing documentation for encrypted databases, active writers, or WAL consistency edge cases.

#### Recommended next steps

- **First:** move database inspection and table-page refresh work off the UI thread, not just diff computation.
- **Second:** refactor SQL export toward a streaming writer / iterator so large tables do not require full row materialization plus one giant in-memory SQL blob.
- **Third:** add regression tests for live/WAL-backed database behavior and background loading once inspection becomes asynchronous.
- **Fourth:** add at least one macOS CI job for build smoke coverage, since that is a likely real user platform.
- **Fifth:** consider whether `SnapshotStore` should reuse a persistent SQLite connection once the UI-thread loading work is addressed.

#### Documentation note

`README.md` and this handoff doc now explicitly call out that views remain inspect-only, tracked indexes/triggers are preserved in generated SQL, and large/live-database caveats still apply. The next documentation gap is around best-effort behavior for active/WAL-backed databases and other live-database edge cases.

## 2026-03-20 Implementation Follow-Up

### Changed

- Implemented index/trigger inspection in `src/db/inspector.rs` and carried those schema objects through `DatabaseSummary` / `SchemaDiff` so export planning can see them.
- Updated `src/diff/export.rs` so changed indexes are recreated from the right-hand database and affected triggers are recreated only after migration DML finishes.
- Added deterministic tie-breakers for sorted pagination so duplicate sort values do not cause unstable page boundaries.
- Added regression coverage for schema-object preservation, trigger-safe migration application, and deterministic sorted paging.

### Verified

- `cargo test --test diff_tests`
- `cargo fmt --all --check`
- `cargo test`
- `cargo clippy --all-targets --all-features -- -D warnings`

### Remaining

- Database inspection and table refresh still happen on the UI thread.
- SQL export still builds one large in-memory string and still fully materializes seeded tables.
- Live/WAL-backed database handling remains best-effort and still needs clearer behavioral tests/documentation.
