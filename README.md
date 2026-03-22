# Patchworks

Patchworks is a Rust desktop app for inspecting and diffing SQLite databases. It can open zero, one, or two database files, show schema and row-level changes, save snapshots, and generate SQL intended to transform the left database into the right database.

## Current Scope

- Inspect SQLite tables and views in a native `egui` desktop UI
- Browse table rows with pagination and sortable columns
- Diff two databases at schema level and row level
- Save snapshots of a live database and compare against them later
- Preview SQL export in the UI, copy it to the clipboard, or save it to disk
- Preserve tracked indexes and triggers in generated SQL migrations
- Create a snapshot from the CLI with `patchworks --snapshot <db>`

Current limitations:

- Views are inspect-only in the current phase; they are not diffed or exported.
- Indexes and triggers are preserved in generated SQL, but they are not surfaced in dedicated UI panels yet.
- There is no headless CLI for `diff`, `inspect`, or SQL export yet.
- Snapshot state is stored under `~/.patchworks/`.
- Very large exports and snapshot seeds still materialize substantial data in memory.
- Live / WAL-backed / actively changing databases are still best-effort.
- Long-running background loads and diffs now expose staged progress, but there is still no explicit cancel control; superseded requests are dropped instead of being cooperatively interrupted.

## Crates.io

- <https://crates.io/crates/patchworks>

## Install

```bash
cargo install patchworks
```

## Basic Use

```bash
patchworks
patchworks app.db
patchworks left.db right.db
patchworks --snapshot app.db
```

## Requirements

- Rust toolchain with `cargo`
- `rustfmt`
- `clippy`
- `cargo-nextest` for the preferred local test runner
- `cargo-deny` for dependency policy checks
- A desktop session if you want to launch the GUI

`rusqlite` is built with the `bundled` feature, so a system SQLite library is not required.

## Verified Local Workflow

These commands were re-verified on 2026-03-20 in `/Users/sawyer/github/patchworks`:

```bash
cargo build
cargo test
cargo nextest run
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo bench --no-run
cargo deny check
cargo run -- --help
```

Benchmarks can then be executed locally with:

```bash
cargo bench
```

If you're working from a local checkout instead of an installed binary, the equivalent commands are:

```bash
cargo run
cargo run -- app.db
cargo run -- left.db right.db
cargo run -- --snapshot app.db
```

For a local publishability check before release, run:

```bash
cargo package --allow-dirty
```

## Snapshot Storage

Patchworks creates a local store in your home directory:

- Metadata database: `~/.patchworks/patchworks.db`
- Snapshot database copies: `~/.patchworks/snapshots/<uuid>.sqlite`

## Behavior Notes

- Starting the app with two database paths computes a diff on launch.
- Opening databases and refreshing visible table pages now run in the background with inline loading indicators.
- Diff requests now run in the background, so the UI stays responsive while large comparisons complete.
- Opening a right-side database from the toolbar loads it, but you still need to click `Diff`.
- Long-running database opens, table refreshes, and diffs now show staged progress in the UI and status bar.
- Row diffs are only computed for tables that exist on both sides.
- Sorted pagination now adds a primary-key / `rowid` tie-breaker so duplicate sort values page deterministically.
- SQL export favors correctness over minimal migrations when a table schema changes.
- Generated SQL now drops and recreates tracked triggers after data changes so migration DML does not accidentally fire left-side trigger logic.

## Repository Layout

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
