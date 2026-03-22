# Patchworks

Patchworks is a Rust desktop app for inspecting and diffing SQLite databases. It can open zero, one, or two database files, compare schema and row changes, save snapshots, and export SQL that moves the left database toward the right one.

## Current scope

- inspect SQLite tables and views
- diff schema and row data
- save snapshots under `~/.patchworks/`
- export SQL from the UI or use `--snapshot` from the CLI

## Quick start

```bash
cargo install patchworks
patchworks
patchworks app.db
patchworks left.db right.db
patchworks --snapshot app.db
```

## Notes

- views are inspect-only right now
- SQL export prefers correctness over minimal changes
- very large diffs still cost memory
