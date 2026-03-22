# Patchworks

Patchworks is a GUI-first Rust desktop app for inspecting and diffing SQLite databases. It can open zero, one, or two database files, browse tables and views, compare schema and row-level changes, save snapshots, and generate SQL intended to move the left database toward the right database.

## What works today

- Inspect SQLite tables and views in a native `egui` desktop UI
- Browse table rows with pagination and sortable columns
- Diff two databases at schema level and row level
- Save snapshots of a live database and compare against them later
- Preview SQL export in the UI, copy it to the clipboard, or save it to disk
- Preserve tracked indexes and triggers in generated SQL migrations
- Create a snapshot from the CLI with `patchworks --snapshot <db>`

## Current limits and operating truth

- Patchworks is still a GUI-first tool; there is no headless CLI for `inspect`, `diff`, or SQL export yet.
- Views are inspect-only in the current phase; they are not diffed or exported.
- Indexes and triggers are preserved in generated SQL, but they are not surfaced in dedicated UI panels yet.
- Snapshot state lives under `~/.patchworks/` on the local machine.
- Best results come from stable SQLite files; live, WAL-backed, or actively changing databases are still best-effort.
- Long-running database opens, table refreshes, and diffs now show staged progress, but there is still no explicit cancel control; superseded requests are dropped instead of being cooperatively interrupted.
- Very large exports and snapshot seeds can still materialize substantial data in memory.

## Crates.io

- <https://crates.io/crates/patchworks>

## Install

```bash
cargo install patchworks
```

`rusqlite` is built with the `bundled` feature, so a system SQLite library is not required.

## Quick start

```bash
patchworks
patchworks app.db
patchworks left.db right.db
patchworks --snapshot app.db
```

If you're working from a local checkout instead of an installed binary:

```bash
cargo run
cargo run -- app.db
cargo run -- left.db right.db
cargo run -- --snapshot app.db
```

## Contributor workflow

Common local verification commands:

```bash
cargo build
cargo test
cargo nextest run
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo bench --no-run
cargo deny check
cargo run -- --help
```

For the full project handoff, phase tracking, and source-of-truth notes, see [`BUILD.md`](BUILD.md).

## Snapshot storage

Patchworks creates a local store in your home directory:

- Metadata database: `~/.patchworks/patchworks.db`
- Snapshot database copies: `~/.patchworks/snapshots/<uuid>.sqlite`

## Behavior notes

- Starting the app with two database paths computes a diff on launch.
- Opening databases and refreshing visible table pages run in the background with inline loading indicators.
- Diff requests also run in the background, so the UI stays responsive while large comparisons complete.
- Opening a right-side database from the toolbar loads it, but you still need to click `Diff`.
- Row diffs are only computed for tables that exist on both sides.
- Sorted pagination adds a primary-key or `rowid` tie-breaker so duplicate sort values page deterministically.
- SQL export favors correctness over minimal migrations when a table schema changes.
- Generated SQL drops and recreates tracked triggers after migration DML so export application does not accidentally fire left-side trigger logic.

## Repository layout

- `src/main.rs`: CLI entrypoint and desktop startup
- `src/app.rs`: app coordinator and workspace actions
- `src/db/`: inspection, diff orchestration, snapshots, and shared types
- `src/diff/`: schema diff, row diff, and SQL export logic
- `src/ui/`: `egui` rendering layer
- `tests/`: integration tests and SQLite fixtures
- `benches/`: Criterion benchmarks for query and diff hot paths
- `.github/workflows/ci.yml`: GitHub Actions CI
- `deny.toml`: `cargo-deny` dependency policy
- `BUILD.md`: living build and handoff document for future passes

## License

MIT. See [LICENSE](LICENSE).
