# Patchworks

Patchworks is a Rust desktop app for inspecting and diffing SQLite databases. It can open zero, one, or two database files, show schema and row-level changes, save snapshots, and generate SQL intended to transform the left database into the right database.

## Current Scope

- Inspect SQLite tables and views in a native `egui` desktop UI
- Browse table rows with pagination and sortable columns
- Diff two databases at schema level and row level
- Save snapshots of a live database and compare against them later
- Preview SQL export in the UI, copy it to the clipboard, or save it to disk
- Create a snapshot from the CLI with `patchworks --snapshot <db>`

Current limitations:

- Views are inspect-only in the current phase; they are not diffed or exported.
- There is no headless CLI for `diff`, `inspect`, or SQL export yet.
- Snapshot state is stored under `~/.patchworks/`.
- The crate is not published on crates.io yet.

## Requirements

- Rust toolchain with `cargo`
- `rustfmt`
- `clippy`
- A desktop session if you want to launch the GUI

`rusqlite` is built with the `bundled` feature, so a system SQLite library is not required.

## Verified Local Workflow

These commands were re-verified on 2026-03-20 in `/Users/sawyer/github/patchworks`:

```bash
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo run -- --help
```

Launch and snapshot examples:

```bash
cargo run
cargo run -- app.db
cargo run -- left.db right.db
cargo run -- --snapshot app.db
```

Patchworks is not currently available via `cargo install patchworks` from crates.io.

## Snapshot Storage

Patchworks creates a local store in your home directory:

- Metadata database: `~/.patchworks/patchworks.db`
- Snapshot database copies: `~/.patchworks/snapshots/<uuid>.sqlite`

## Behavior Notes

- Starting the app with two database paths computes a diff on launch.
- Opening a right-side database from the toolbar loads it, but you still need to click `Diff`.
- Row diffs are only computed for tables that exist on both sides.
- SQL export favors correctness over minimal migrations when a table schema changes.

## Repository Layout

- `src/main.rs`: CLI entrypoint and desktop startup
- `src/app.rs`: app coordinator and workspace actions
- `src/db/`: inspection, diff orchestration, snapshots, and shared types
- `src/diff/`: schema diff, row diff, and SQL export logic
- `src/ui/`: `egui` rendering layer
- `tests/`: integration tests and SQLite fixtures
- `BUILD.md`: living build and handoff document for future passes

## License

MIT. See [LICENSE](/Users/sawyer/github/patchworks/LICENSE).
