# BUILD.md

**This is the primary operational handoff document for Patchworks. It is a living document, and every future agent or developer who touches this repository is responsible for keeping it accurate, current, and up to date.** If you verify a workflow, change behavior, add tooling, or discover a bug, update this file in the same pass.

## Last Verified Baseline

- Verified on: 2026-03-20
- Repo path: `/Users/sawyer/github/patchworks`
- Branch: `main`
- Commit: `720e849311b2b034cdaefa10c6b14b6927c12018`
- Host used for verification: macOS arm64 (`Darwin 25.4.0`)

## Project Baseline

Patchworks is a single-crate Rust desktop application for inspecting and diffing SQLite databases. The current product is a Phase 1 egui/eframe desktop app with a small CLI wrapper.

What it currently does:

- Open zero, one, or two SQLite database files in a native desktop UI.
- Inspect tables and views from a SQLite file.
- Browse table rows with pagination and sortable columns.
- Diff two databases at schema level and row level.
- Save snapshots of a database into a local Patchworks store under `~/.patchworks/`.
- Compare a live database against a saved snapshot.
- Generate SQL intended to transform the left database into the right database.
- Preview SQL in the UI, copy it to clipboard, or save it to disk.

What it does not currently do:

- It does not provide a headless CLI subcommand for diffing or SQL export.
- It does not diff or export views; views are inspected and listed only.
- It does not include packaging, installer, or release automation in this repo.
- It does not include migrations, seed scripts, Docker, CI config, or environment-file driven setup.

## Major Components And Entry Points

- `Cargo.toml`
  Single Rust package manifest and dependency source of truth.
- `src/main.rs`
  Binary entrypoint. Parses CLI args, supports `--snapshot`, otherwise launches the egui app.
- `src/lib.rs`
  Re-exports the crate modules.
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
  egui rendering layer for file panels, table views, schema diff, row diff, snapshots, SQL export, and file dialogs.
- `tests/`
  Integration tests. These are the best executable specification of current behavior.
  - `tests/diff_tests.rs`
  - `tests/snapshot_tests.rs`
  - `tests/fixtures/create_fixtures.sql`

## Current Implemented State

The implemented system is cohesive and passes its current automated checks, but it is still lightweight and intentionally narrow in scope.

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
- UI behavior:
  - Opening a right-side database from startup computes a diff automatically.
  - Opening a right-side database from the toolbar loads it, but the user must click `Diff`.
  - SQL export can be copied to the clipboard or saved through a native save dialog.

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

Verified CLI snapshot smoke test:

```bash
tmp_db=/tmp/patchworks-cli-smoke2-$$.sqlite
rm -f "$tmp_db"
sqlite3 "$tmp_db" "CREATE TABLE demo (id INTEGER PRIMARY KEY, name TEXT); INSERT INTO demo (name) VALUES ('sample');"
cargo run -- --snapshot "$tmp_db"
```

Observed result of the smoke test:

- `cargo run -- --snapshot ...` completed successfully.
- The command created the default Patchworks store under `~/.patchworks/`.
- I cleaned up the temporary smoke-test snapshot rows and copied databases after verification.

### Unverified But Likely Commands

These commands are supported by the code, but I did not fully verify the GUI interaction path in this review session:

```bash
cargo run
cargo run -- path/to/database.sqlite
cargo run -- left.sqlite right.sqlite
```

Likely local install command:

```bash
cargo install --path .
```

Important doc correction:

- `README.md` currently says `cargo install patchworks`.
- On 2026-03-20, `cargo info --registry crates-io patchworks` failed with `could not find patchworks in registry`.
- Treat the README installation command as stale/wrong until someone publishes the crate and re-verifies it.

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
  Good high-level product description, but not reliable for install instructions.
- `AGENTS.md`
  Secondary agent memory for Codex sessions. Useful for architecture/convention notes, but it is not the primary operational handoff.

Areas where docs or behavior diverge:

- `README.md` installation instructions are stale/wrong for the current state of distribution.
- `README.md` screenshot is still a placeholder.
- `README.md` roadmap is aspirational, not implementation proof.
- `AGENTS.md` is secondary to `BUILD.md` and should stay aligned with it, but it does not replace direct verification.

Config and state files that affect behavior:

- There is no checked-in runtime config file.
- User-local snapshot state lives outside the repo in `~/.patchworks/`.
- Build output goes to `target/`.

## Current Gaps And Known Issues

Concrete issues and risks found by inspection:

- SQL export appears incorrect for removed rows when a table has no shared primary key and diffing falls back to `rowid`.
  - In `src/diff/data.rs`, rowid fallback stores synthetic row identity in `RowModification.primary_key`, but `removed_rows` only store display/data columns.
  - In `src/diff/export.rs`, `where_clause()` uses `row[0]` as `rowid` for delete statements when `primary_key == ["rowid"]`.
  - That means deletes for rowid-fallback tables can target the first shared column value instead of the actual rowid.
  - This path is not covered by the existing SQL export test.
- Views are inspected (`DatabaseSummary.views`) and listed in the side panel, but there is no view diff model and no SQL export support for views.
- GUI behavior is only partially automated:
  - There are no UI tests.
  - File dialog behavior is untested.
  - Clipboard/save actions are untested.
- The CLI is thin:
  - No headless `diff`, `inspect`, `export`, `list-snapshots`, or `prune-snapshots` commands exist.
  - Automation currently has to use the library or the GUI.
- Snapshot storage always uses `~/.patchworks` via `directories::BaseDirs`, not XDG-style config/data directories.
- Modified-schema SQL export is correctness-first, not minimal. It rebuilds modified tables instead of generating the smallest possible migration.
- Row diffs only exist for shared tables. Added/removed tables only appear in schema diff.
- There is no release, packaging, or CI definition in the repository.
- There are no repo-local migration or seeding workflows outside the test fixtures.

Test coverage gaps:

- No regression test covers SQL export on a no-primary-key table with rowid fallback.
- No test covers view handling because views are not part of the current diff/export surface.
- No end-to-end GUI smoke test exists.

## Next-Pass Priorities

### Highest Impact

1. Fix the rowid-fallback SQL export bug and add a regression test in `tests/diff_tests.rs`.
2. Update `README.md` so installation and usage instructions match reality, then keep `BUILD.md` and `README.md` aligned.
3. Decide whether views are in scope for Phase 1.5/2. Either implement view diff/export support or document clearly that views are inspect-only.

### Quick Wins

1. Add a dedicated integration test for SQL export when diffing tables without primary keys.
2. Replace `README.md`'s `cargo install patchworks` with a verified local workflow.
3. Add a short note in `README.md` or `BUILD.md` that snapshot state is written to `~/.patchworks/`.
4. Add a manual QA section for the GUI flow once someone verifies the full desktop interaction path.

### Deeper Follow-Up Work

1. Add headless CLI subcommands for inspect/diff/export so Patchworks is useful in automation and CI.
2. Add snapshot listing and cleanup/pruning commands.
3. Add UI smoke coverage or scripted desktop verification.
4. Revisit SQL export minimality and dependency handling for more complex schema changes.

## Next-Agent Checklist

Follow this in order after opening the repo:

1. Read this file first.
2. Confirm you are on the expected commit/branch or note the new baseline here.
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
6. If you plan to touch SQL export, start by reproducing the rowid-fallback gap with a failing test before editing implementation.
7. If you plan to touch the GUI, verify manually whether `cargo run -- left.sqlite right.sqlite` opens and behaves as expected on your desktop session.
8. Update `BUILD.md` after any successful verification, bug discovery, workflow change, or doc correction.

## Safe Starting Points

Safest first changes for a new contributor:

- Add the missing SQL export regression test for rowid fallback.
- Correct the stale install instructions in `README.md`.
- Add explicit documentation for snapshot store side effects and locations.
