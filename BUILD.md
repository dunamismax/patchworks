# BUILD.md

**This is the primary operational handoff document for Patchworks. It is a living document, and every future agent or developer who touches this repository is responsible for keeping it accurate, current, and up to date.** If you verify a workflow, change behavior, add tooling, or discover a bug, update this file in the same pass.

## Last Verified Baseline

- Verified on: 2026-03-20
- Repo path: `/Users/sawyer/github/patchworks`
- Branch: `main`
- Commit: `c491702cf51e93f0ad9030caaf68f80a75088dbf`
- Host used for verification: macOS arm64 (`Darwin 25.4.0`)
- Verification in this file covers the current working tree on top of the commit above. Local changes from this pass are not committed yet.

## Completed In This Pass

- Fixed the SQL export bug for rowid-fallback deletes on tables without a shared primary key.
- Added regression coverage for that path in `tests/diff_tests.rs` with dedicated fixtures in `tests/fixtures/create_fixtures.sql`.
- Rewrote `README.md` so installation, usage, limitations, and snapshot storage match the code.
- Re-verified MIT licensing state:
  - `Cargo.toml` declares `license = "MIT"`.
  - `LICENSE` contains the MIT license text.
- Re-ran the verified build, test, lint, help, and snapshot smoke workflows.

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

Test coverage gaps:

- No test covers view handling because views are not part of the current diff/export surface.
- No end-to-end GUI smoke test exists.
- There is no coverage for clipboard or native file-dialog integration.

## Next-Pass Priorities

### Highest Impact

1. Add headless CLI subcommands for inspect, diff, and SQL export so Patchworks is useful in automation and CI.
2. Add snapshot listing and cleanup/pruning commands.
3. Add UI smoke coverage or a scripted manual desktop verification workflow.
4. Decide whether view diff/export moves into scope for the next phase; if so, design the model and tests instead of leaving views inspect-only.

### Quick Wins

1. Add a manual QA section for the GUI flow once someone verifies the full desktop interaction path.
2. Add a real screenshot to `README.md` after verifying a stable UI capture.
3. Consider a store-path override for snapshots to improve automation and test ergonomics.

### Deeper Follow-Up Work

1. Revisit SQL export minimality and dependency handling for more complex schema changes.
2. Evaluate platform-native data-directory behavior instead of always using `~/.patchworks`.
3. Decide whether no-primary-key diff/export flows need stronger user-facing warnings in the UI.

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
8. Update `BUILD.md` after any successful verification, bug discovery, workflow change, or doc correction.

## Safe Starting Points

Safest first changes for a new contributor:

- Add headless CLI subcommands for inspect/diff/export.
- Add snapshot list and prune workflows.
- Add manual QA notes or scripted smoke coverage for the current GUI path.
