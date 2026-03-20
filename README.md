# Patchworks

> ⚠️ Work in Progress

Git-style visual diffs for SQLite databases.

Patchworks is a desktop tool for opening two SQLite databases and seeing exactly what changed between them. It focuses on schema changes, row-by-row data diffs, snapshot history, and SQL export, all inside a Rust-native application with an egui interface and a CLI entrypoint for common workflows.

## Features

- Open one SQLite database for inspection
- Open two SQLite databases and compute schema and data diffs
- Browse tables with paging and sortable columns
- Save named snapshots of database state
- Compare a live database against a saved snapshot
- Export diffs as SQL migration scripts
- Run as a single Rust binary with no external SQLite dependency

## Screenshot

![Patchworks screenshot placeholder](https://placehold.co/1200x700?text=Patchworks+Screenshot+Coming+Soon)

## Installation

```bash
cargo install patchworks
```

## Usage

```bash
patchworks
patchworks app.db
patchworks before.db after.db
patchworks --snapshot app.db
```

## Roadmap

- SQLite session extension support for live changeset recording
- Branch-like snapshot trees
- Conflict resolution UI for merging diverged databases
- Row-level undo and redo
- Column-level type migration suggestions
- Database health checks for integrity, WAL state, and freelist analysis
