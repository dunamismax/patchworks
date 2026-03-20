# BUILD.md

**This is the primary operational handoff document for Patchworks. It is a living document, and every future agent or developer who touches this repository is responsible for keeping it accurate, current, and up to date.** If you verify a workflow, change behavior, add tooling, or discover a bug, update this file in the same pass.

## Last Verified Baseline

- Verified on: 2026-03-20
- Repo path: `/Users/sawyer/github/patchworks`
- Branch: `main`
- Base commit at start of this pass: `704e06ab2c0f0793ea8a700c5286bf212cb3d6e2`
- Host used for verification: macOS arm64 (`Darwin 25.4.0`)
- Last full code review: 2026-03-20
- This verification was performed against the working tree rooted at the commit above before the changes from this pass were committed.

## Completed In This Pass

- Added Criterion benchmarks for the main diff and query hot paths in `benches/diff_hot_paths.rs` and `benches/query_hot_paths.rs`.
- Added `proptest`-backed invariant coverage in `tests/proptest_invariants.rs` for schema diff classification, row-diff accounting, and SQL patch generation round-trips.
- Added checked-in dependency policy via `deny.toml` for `cargo-deny`, including the font licenses required by `egui`'s bundled font crate.
- Added GitHub Actions CI in `.github/workflows/ci.yml` for format, clippy, nextest, doc-test, bench-compile, and dependency-policy checks.
- Updated `Cargo.toml` dev dependencies and bench targets for `criterion` and `proptest`.
- Reviewed and updated `README.md`, `AGENTS.md`, and this file so the documented workflows match the commands verified in this pass.

## Project Baseline

Patchworks is a single-crate Rust desktop application for inspecting and diffing SQLite databases. The current product is a Phase 1 `egui`/`eframe` desktop app with a small CLI wrapper.

What it currently does:

- Open zero, one, or two SQLite database files in a native desktop UI.
- Inspect tables and views from a SQLite file.
- Browse table rows with pagination and sortable columns.
- Diff two databases at schema level and row level.
- Save snapshots of a database into a local Patchworks store under `~/.patchworks/`.
- Compare a live database against a saved snapshot.
- Generate SQL intended to transform the left database into the right database.
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
  - Schema diff covers tables and columns only.
  - Row diff is computed only for tables that exist on both sides.
  - Row diff prefers a shared primary key and falls back to `rowid` with warnings.
  - Large-table warning threshold is `100_000` rows.
  - Equality for row diffs now follows `compare_sql_values()` rather than `SqlValue`'s derived `PartialEq`.
- SQL export behavior:
  - Added tables are created and fully seeded from the right database.
  - Removed tables are dropped.
  - Modified tables are rebuilt from the right-side schema and reseeded from the right database.
  - Unchanged-schema tables get incremental `DELETE`/`INSERT`/`UPDATE` statements.
  - Rowid-fallback deletes use the stored row identity instead of guessing from the first visible column.
  - Missing primary-key columns or row values during export are now treated as an explicit invalid state.
- Quality workflow behavior:
  - Benchmarks now exist for paged table reads, isolated row diffs, and end-to-end diff generation.
  - Property tests now exercise schema diff invariants, row-diff accounting invariants, and SQL export round-tripping.
  - The repo now includes GitHub Actions CI and a checked-in `cargo-deny` policy.
  - `cargo nextest run` is the preferred fast local test command; `cargo nextest run --all-targets` also discovers the Criterion bench binaries and is slower than needed for routine CI.
- UI behavior:
  - Starting the app with two database paths computes a diff automatically.
  - Opening a right-side database from the toolbar loads it, but the user must click `Diff`.
  - The tabular diff display mode is now labeled `Grid`, which matches the current rendering.
  - SQL export can be copied to the clipboard or saved through a native save dialog.
  - Views are listed in the side panel, but remain inspect-only.
- Table paging behavior:
  - The app now reuses already-inspected `TableInfo` values when refreshing visible table pages.
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

### Verified Commands

The following commands were run successfully in `/Users/sawyer/github/patchworks` on 2026-03-20:

```bash
cargo build
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo nextest run
cargo bench --no-run
cargo deny check
cargo run -- --help
```

Observed `cargo run -- --help` output:

- Usage: `patchworks [OPTIONS] [FILES]...`
- Files: zero, one, or two database files to open in the UI
- Supported option: `--snapshot <SNAPSHOT>`

Verified CLI snapshot smoke test:

```bash
tmp_db="/tmp/patchworks-cli-smoke-$RANDOM.sqlite"
rm -f "$tmp_db"
sqlite3 "$tmp_db" "CREATE TABLE demo (id INTEGER PRIMARY KEY, name TEXT); INSERT INTO demo (name) VALUES ('sample');"
cargo run -- --snapshot "$tmp_db"
rm -f "$tmp_db"
```

Observed result of the snapshot smoke test:

- `cargo run -- --snapshot ...` completed successfully.
- The command created the default Patchworks store under `~/.patchworks/`.
- I cleaned up the temporary snapshot metadata row and copied database after verification.

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

- `cargo install patchworks` from crates.io is not valid as of 2026-03-20 because the crate is not published there.

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

- `README.md` now includes the verified `nextest`, benchmark, and `cargo-deny` workflows alongside the build/test commands.
- `README.md` still documents that views are inspect-only, snapshot state is stored in `~/.patchworks/`, and the crate is not published on crates.io.
- `AGENTS.md` now mentions the benchmark, property-test, CI, and dependency-policy workflows.
- There is still no real screenshot committed to the repo.

Config and state files that affect behavior:

- There is no checked-in runtime config file.
- GitHub Actions CI now lives in `.github/workflows/ci.yml`.
- Dependency policy is configured in `deny.toml`.
- User-local snapshot state lives outside the repo in `~/.patchworks/`.
- Build output goes to `target/`.

## Code Review Findings

Full review of every source file completed 2026-03-20. Build, test, nextest, clippy, fmt, bench compile, and `cargo-deny` all pass clean after the changes in this pass.

### Overall Assessment

The codebase is solid for a Phase 1 product. Code is well-organized, cleanly separated into layers (`db`, `diff`, `state`, `ui`), and follows Rust conventions. Error handling is consistent (`thiserror` in the library, `anyhow` in `main`). The streaming merge-diff approach in `data.rs` is still a strong architectural choice. No `unsafe` code was found.

### Remaining Findings

1. **Diff computation still blocks the UI thread** (`src/app.rs`). `compute_diff()` runs synchronously on the main `egui` thread, so large diffs will still freeze the UI. This is the biggest remaining product risk.

2. **SQL export still materializes the entire migration as one `String`** (`src/db/differ.rs`, `src/diff/export.rs`). Very large exports can consume substantial memory.

3. **`load_all_rows()` still materializes whole tables during export seeding** (`src/db/inspector.rs`). Added or rebuilt tables with millions of rows can still OOM.

4. **`SnapshotStore` still opens a fresh SQLite connection for each operation** (`src/db/snapshot.rs`). The code is simple and correct, but it is less efficient than keeping a persistent connection around.

5. **`list_snapshots()` still relies on canonicalized source paths** (`src/db/snapshot.rs`). If a database is moved or opened through a path that no longer canonicalizes to the stored location, the snapshot list will come back empty without a tailored explanation.

6. **`main.rs` still rewrites `-snapshot` to `--snapshot` manually.** The behavior works, but the workaround remains undocumented in the CLI definition itself.

7. **WAL-mode or actively changing databases are still only best-effort.** The repo has no explicit handling or user-facing documentation for encrypted databases, active writers, or WAL consistency edge cases.
