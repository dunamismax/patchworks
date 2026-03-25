# AGENTS.md

## Purpose

Secondary agent-oriented project memory for Patchworks. `BUILD.md` is the primary operational handoff document and must stay authoritative for setup, verification, and next-pass priorities. This file is for fast ramp-up.

## Project

- Name: Patchworks
- Tagline: Git-style diffs for SQLite databases
- Stack: Python 3.12+, stdlib sqlite3, argparse, uv, ruff, pyright, pytest
- Go: reserved for performance-critical hot paths (not yet present)
- Web UI: FastAPI + htmx (later phase)
- License: MIT

## Vision

Patchworks is a CLI-first SQLite comparison and migration tool. The CLI is the primary surface. A local web UI via FastAPI + htmx replaces the former desktop GUI as a later phase. See BUILD.md phases 0-13 for the full roadmap.

## Architecture

```
src/patchworks/
  __init__.py       Package root
  __main__.py       CLI entrypoint
  cli.py            Subcommand dispatch (argparse) - thin layer, no business logic
  db/               SQLite inspection, snapshots, diff orchestration, migration persistence
  diff/             Schema diffing, streaming row diffs, SQL export, semantic analysis, merge
tests/              All test modules
go/                 (future) Go acceleration components
```

Key design decisions:
- `cli.py` dispatches. `db/` and `diff/` own data logic. No business logic in the CLI module.
- SQL export favors correctness over minimality (temp-table rebuild, FK guards, trigger preservation).
- Streaming, bounded-memory approach for diff and export operations.
- CLI and any future web UI share the same backend functions. No forked logic.

## Conventions

- Type hints on all public functions. Pyright must pass clean.
- Prefer streaming over materializing full tables.
- Keep `BUILD.md` current in the same pass as code changes.
- No `assert` outside tests for control flow.

## Verification

```bash
uv sync
ruff check .
ruff format --check .
pyright
pytest
```

## Current state (2026-03-25)

**Rewrite in progress.** The previous Rust implementation shipped through v0.3.0. The project is being rebuilt in Python with Go reserved for performance-critical paths. Currently at Phase 0 - scaffold and bootstrap.

Previous Rust capabilities that the rewrite targets for parity: inspect, browse, diff (schema + rows), snapshots, SQL export with FK safety and trigger preservation, headless CLI with subcommands, JSON output, CI-friendly exit codes, streaming bounded-memory export, semantic diff awareness, three-way merge, migration workflow management.

Known limits carried forward: views are inspect-only, best-effort on live/WAL databases (read-only access; concurrent writes may produce inconsistent results), encrypted databases not supported.

## Known caveats

- SQL export correctness > minimality.
- Live/WAL/encrypted databases are best-effort.
- `~/.patchworks/` is local machine state, not project state.
- BUILD.md is the primary handoff - read it before touching export logic or verification workflows.
