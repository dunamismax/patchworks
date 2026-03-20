# BUILD.md

**This is the primary operational handoff document for Patchworks. It is a living document, and every future agent or developer who touches this repository is responsible for keeping it accurate, current, and up to date.** If you verify a workflow, change behavior, add tooling, or discover a bug, update this file in the same pass.

## Last Verified Baseline

- Verified on: 2026-03-20
- Repo path: `/Users/sawyer/github/patchworks`
- Branch: `main`
- Commit: `c491702cf51e93f0ad9030caaf68f80a75088dbf`
- Host used for verification: macOS arm64 (`Darwin 25.4.0`)
- Last full code review: 2026-03-20
- Verification in this file covers the current working tree on top of the commit above. Local changes from this pass are not committed yet.

## Completed In This Pass

- Fixed the SQL export bug for rowid-fallback deletes on tables without a shared primary key.
- Added regression coverage for that path in `tests/diff_tests.rs` with dedicated fixtures in `tests/fixtures/create_fixtures.sql`.
- Rewrote `README.md` so installation, usage, limitations, and snapshot storage match the code.
- Re-verified MIT licensing state:
  - `Cargo.toml` declares `license = "MIT"`.
  - `LICENSE` contains the MIT license text.
- Re-ran the verified build, test, lint, help, and snapshot smoke workflows.
- Performed full code review (see [Code Review Findings](#code-review-findings) below).

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
- It does not include CI config, Docker, migrations, or environment-file driven setup.

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
  Top-level app coordinator. Loads left/right databases, computes diffs, saves snapshots, and wires UI actions to backend modules.
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
  Integration tests. These are the best executable specification of current behavior.
  - `tests/diff_tests.rs`
  - `tests/snapshot_tests.rs`
  - `tests/fixtures/create_fixtures.sql`

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
- SQL export behavior:
  - Added tables are created and fully seeded from the right database.
  - Removed tables are dropped.
  - Modified tables are rebuilt from the right-side schema and reseeded from the right database.
  - Unchanged-schema tables get incremental `DELETE`/`INSERT`/`UPDATE` statements.
  - Rowid-fallback deletes now use the actual stored row identity instead of guessing from the first visible column.
- UI behavior:
  - Starting the app with two database paths computes a diff automatically.
  - Opening a right-side database from the toolbar loads it, but the user must click `Diff`.
  - SQL export can be copied to the clipboard or saved through a native save dialog.
  - Views are listed in the side panel, but remain inspect-only.

## Verified Build And Run Workflow

### Prerequisites

- Rust toolchain with `cargo`
- `rustfmt`
- `clippy`
- A desktop environment if you want to launch the GUI

Notes:

- No `.env` file, runtime secret, or external service dependency was found.
- `rusqlite` is built with `bundled`, so a system SQLite library is not required for the app build.
- `sqlite3` was only used for a local smoke test; it is not a project dependency.

### Verified Commands

The following commands were run successfully in `/Users/sawyer/github/patchworks` on 2026-03-20:

```bash
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
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
  Operational source of truth for how the UI actually loads databases, computes diffs, and saves snapshots.
- `src/db/*.rs` and `src/diff/*.rs`
  Source of truth for inspection, diffing, snapshot storage, and SQL export behavior.
- `tests/diff_tests.rs` and `tests/snapshot_tests.rs`
  Best executable specification of current expected behavior.
- `tests/fixtures/create_fixtures.sql`
  Canonical test data for schema diff, data diff, rowid fallback, SQL export, and snapshot tests.

Useful but secondary docs:

- `README.md`
  High-level product description and local workflow guide. Keep it aligned with verified commands and current scope.
- `AGENTS.md`
  Secondary agent memory for Codex sessions. Useful for architecture and conventions, but it does not replace direct verification.

Current doc and behavior alignment notes:

- `README.md` now matches the verified local workflow and explicitly says the crate is not published on crates.io.
- `README.md` now documents that views are inspect-only and snapshot state is stored in `~/.patchworks/`.
- There is still no real screenshot committed to the repo.

Config and state files that affect behavior:

- There is no checked-in runtime config file.
- User-local snapshot state lives outside the repo in `~/.patchworks/`.
- Build output goes to `target/`.

## Code Review Findings

Full review of every source file completed 2026-03-20. Build, test, clippy, and fmt all pass clean.

### Overall Assessment

The codebase is solid for a Phase 1 product. Code is well-organized, cleanly separated into layers (db, diff, state, ui), and follows Rust conventions. Error handling is consistent (thiserror in library, anyhow in main). The streaming merge-diff approach in `data.rs` is a good architectural choice. No security vulnerabilities, no panics outside tests, no unsafe code.

### Correctness Issues (Bugs / Latent Defects)

1. **`read_table_page` re-inspects the entire database on every page read** (`inspector.rs:80`). Each call to `read_table_page` calls `inspect_database` internally, which re-reads `sqlite_master` and counts every table. For databases with many tables or frequent pagination, this is needlessly expensive. The `TableInfo` is already available in the caller's `DatabasePaneState.summary`. Consider accepting a `&TableInfo` parameter instead.

2. **`count_rows` cast truncates on databases above 2^63 rows** (`inspector.rs:280`). The `i64` from SQLite is cast to `u64` with `as`. Negative values from a corrupted or adversarial database would wrap. Low-severity since SQLite databases realistically never approach this limit, but a `try_from` or `.max(0)` guard would be defensive.

3. **`where_clause` in `export.rs:174` silently uses index 0 if a primary key column is missing from the comparison column set.** The `unwrap_or(0)` fallback means a missing-column bug would generate silently wrong SQL (deleting/updating based on the wrong column value). This should be an error or at minimum a logged warning.

4. **`DiffDisplayMode::SideBySide` is defined and stored in state** (`workspace.rs:25-30`) **but the enum variant `SideBySide` doesn't produce a true side-by-side view.** The render in `diff_view.rs` shows removed/added/modified rows sequentially in a grid, not left-right aligned. The name is misleading relative to the actual rendering. Low-severity UX issue.

5. **Snapshot `list_snapshots` silently fails on uncanonicalized paths.** It calls `canonicalize()` on the source path and compares against stored canonical paths. If the source database was moved or is accessed via a symlink, no snapshots will be found. No error is raised, the list just comes back empty.

### Performance Concerns

6. **Diff computation is fully synchronous and blocks the UI thread** (`app.rs:86`). The `compute_diff` call in `PatchworksApp` runs on the main `egui` thread. For large databases, the entire GUI will freeze during diff computation. This should eventually move to a background thread with a progress indicator.

7. **SQL export materializes the entire migration script as a single `String`** (`differ.rs:32`, `export.rs:74`). For databases with millions of rows in added/modified tables, the in-memory SQL string could grow very large. A streaming writer would be more robust for large exports.

8. **`load_all_rows` in `inspector.rs` materializes every row of a table into memory at once** (`inspector.rs:125-142`). Used by `export.rs` for seeding added/modified tables. On a table with millions of rows, this will OOM. Should be streamed.

9. **The fixture parser in both test files is duplicated identically** (`diff_tests.rs:11-33` and `snapshot_tests.rs:9-31`). The `fixture_sql` and `create_db` helpers are copy-pasted between the two test files. Should be extracted to a shared test utility module.

### API / Design Issues

10. **`_left_path` is accepted but unused in `export_diff_as_sql`** (`export.rs:13`). The leading underscore suppresses the warning, but it's a public API parameter that does nothing. Either use it or remove it from the signature.

11. **`snapshot.rs` opens a new `Connection` for every operation** (`snapshot.rs:85`, `108`, `137`, `147`). Each of `save_snapshot`, `list_snapshots`, `load_snapshot_path`, and `ensure_schema` opens and closes a separate SQLite connection. Since `SnapshotStore` is long-lived within the app, holding a persistent connection (or using a connection pool/cache) would be cleaner and faster.

12. **`WorkspaceState` derives `Clone`** (`workspace.rs:77`) **but cloning it clones the entire diff result, all row data, and both database summaries.** This is a large struct. If `Clone` is only derived for `StartupOptions` forwarding or similar, consider whether it's actually needed on the full workspace.

13. **`SqlValue` compares `Real` values with `==` for row-diff equality** (`data.rs:161`). The `PartialEq` derive on `SqlValue` means `Real(f64)` uses bitwise float equality. `NaN != NaN` and `-0.0 != 0.0` could cause false positives in the diff. The `compare_sql_values` function handles this more carefully, but the diff loop uses `==` via `PartialEq`.

14. **`main.rs` manually maps `-snapshot` to `--snapshot`** (lines 26-28). This is a non-standard workaround. It would be cleaner to use clap's alias support (`#[arg(long, alias = "snapshot")]`) or document why macOS Finder or something sends single-dash long args.

### Robustness / Edge Cases

15. **No handling for encrypted or WAL-mode databases.** If a user opens a database in WAL mode with active writers, the read-only connection might see stale data or the backup in `copy_database_via_backup` might get an inconsistent snapshot. Not a bug per se, but worth documenting as a known limitation.

16. **`inspect_database` filters out `sqlite_%` names but not other internal objects.** Tables created by extensions (like `fts5` shadow tables) or triggers will show up in the inspection and diff. This could produce noisy diffs for databases using full-text search or other extensions.

17. **No limit on diff result size.** If two large databases differ significantly, `added_rows`, `removed_rows`, and `modified_rows` vectors in `TableDataDiff` can grow without bound, eventually consuming all available memory. A cap or pagination on diff results would help.

18. **`sql_export` preview uses a mutable binding for a read-only text edit** (`sql_export.rs:30-36`). The `let mut preview = sql.to_owned()` allocates a full copy of the SQL string every frame just to pass it to a read-only `TextEdit`. Using `TextEdit::multiline` with a non-mutable reference or a `code_editor` pattern would avoid the per-frame allocation.

### Test Coverage Observations

19. **No unit tests in any library module.** All 8 tests are integration tests in the `tests/` directory. The diff algorithms, schema comparison, SQL literal escaping, value comparison, and fixture parser would all benefit from targeted unit tests, particularly `sql_literal` (escaping edge cases), `compare_sql_values` (NaN, -0.0, mixed types), and `columns_match`.

20. **No test for schema-modified SQL export.** The `sql_export_recreates_target_state` test only covers unchanged-schema incremental changes. The `ALTER TABLE ... RENAME TO` / rebuild path for modified tables is untested.

21. **No test for empty databases.** Diffing two empty databases, or diffing against an empty right-side, is untested. The code looks like it would handle this correctly but it's unverified.

22. **No test for tables with composite primary keys.** All fixtures use single-column integer primary keys. Multi-column PKs exercise different code paths in `build_stream_sql`, `next_stream_row`, and `where_clause`.

23. **No test for blob-heavy or large-text columns in SQL export.** The `sql_literal` function handles blobs and text escaping, but only basic cases are exercised through the existing `data_left`/`data_right` fixtures.

## Current Gaps And Known Issues

Concrete issues and risks still present:

- Views are inspected (`DatabaseSummary.views`) and listed in the UI, but there is no view diff model and no SQL export support for views.
- GUI behavior is only partially automated:
  - There are no UI tests.
  - File dialog behavior is untested.
  - Clipboard and save actions are untested.
- The CLI is still thin:
  - No headless `diff`, `inspect`, `export`, `list-snapshots`, or `prune-snapshots` commands exist.
  - Automation currently has to use the library or the GUI.
- Snapshot storage always uses `~/.patchworks` via `directories::BaseDirs`, not XDG-style config/data directories.
- Modified-schema SQL export is correctness-first, not minimal. It rebuilds modified tables instead of generating the smallest possible migration.
- Row diffs only exist for shared tables. Added and removed tables only appear in schema diff.
- There is no release, packaging, or CI definition in the repository.
- No handling for encrypted databases, WAL-mode edge cases, or extension shadow tables (see code review items 15-16).

Test coverage gaps:

- No unit tests exist in any library module.
- No test covers view handling because views are not part of the current diff/export surface.
- No test covers schema-modified table SQL export (the ALTER/rebuild path).
- No test covers composite primary keys, empty databases, or blob/text escaping edge cases.
- No end-to-end GUI smoke test exists.
- There is no coverage for clipboard or native file-dialog integration.

## Next-Pass Priorities

### Highest Impact

1. **Fix the `where_clause` silent fallback** (review item 3). The `unwrap_or(0)` in `export.rs:174` could generate wrong SQL. Change to a proper error.
2. **Stop re-inspecting the database on every `read_table_page` call** (review item 1). Pass the already-loaded `TableInfo` instead of re-reading `sqlite_master`.
3. Add headless CLI subcommands for inspect, diff, and SQL export so Patchworks is useful in automation and CI.
4. Add snapshot listing and cleanup/pruning commands.
5. Add unit tests for `sql_literal`, `compare_sql_values`, `where_clause`, and `columns_match`.
6. Add a test for schema-modified SQL export and composite primary keys.

### Quick Wins

1. Remove the unused `_left_path` parameter from `export_diff_as_sql` or use it (review item 10).
2. Extract the duplicated `fixture_sql`/`create_db` test helpers into a shared module (review item 9).
3. Avoid per-frame SQL string clone in `sql_export.rs` (review item 18).
4. Add a manual QA section for the GUI flow once someone verifies the full desktop interaction path.
5. Add a real screenshot to `README.md` after verifying a stable UI capture.
6. Consider a store-path override for snapshots to improve automation and test ergonomics.
7. Document the `-snapshot` single-dash workaround in `main.rs` or replace it with a clap alias (review item 14).

### Deeper Follow-Up Work

1. Move diff computation off the UI thread (review item 6).
2. Stream SQL export and `load_all_rows` instead of materializing everything in memory (review items 7-8).
3. Address `SqlValue` float equality in diff comparisons (review item 13).
4. Revisit SQL export minimality and dependency handling for more complex schema changes.
5. Evaluate platform-native data-directory behavior instead of always using `~/.patchworks`.
6. Decide whether view diff/export moves into scope for the next phase.
7. Add diff result pagination or size caps (review item 17).
8. Consider holding a persistent connection in `SnapshotStore` (review item 11).

## Next-Agent Checklist

Follow this in order after opening the repo:

1. Read this file first.
2. Confirm you are on the expected branch and note any new commit baseline here.
3. Read `Cargo.toml`, `src/main.rs`, `src/app.rs`, `tests/diff_tests.rs`, and `tests/fixtures/create_fixtures.sql`.
4. Run the verified commands:

   ```bash
   cargo build
   cargo test
   cargo fmt --check
   cargo clippy --all-targets -- -D warnings
   cargo run -- --help
   ```

5. If you plan to touch snapshots, inspect and be aware of user-local state in `~/.patchworks/`.
6. If you plan to touch SQL export, start by running `cargo test sql_export_uses_rowid_for_removed_rows_without_primary_key`.
7. If you plan to touch the GUI, verify manually whether `cargo run -- left.sqlite right.sqlite` opens and behaves as expected on your desktop session.
8. Review the [Code Review Findings](#code-review-findings) section for known issues before starting work.
9. Update `BUILD.md` after any successful verification, bug discovery, workflow change, or doc correction.

## Safe Starting Points

Safest first changes for a new contributor:

- Fix the `unwrap_or(0)` in `export.rs:174` (silent wrong-SQL risk).
- Extract duplicated test helpers into a shared module.
- Add unit tests for `sql_literal`, `compare_sql_values`, and `columns_match`.
- Add headless CLI subcommands for inspect/diff/export.
- Add snapshot list and prune workflows.
- Remove the unused `_left_path` parameter from `export_diff_as_sql`.
- Add manual QA notes or scripted smoke coverage for the current GUI path.
