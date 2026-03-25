# Contributing to Patchworks

Patchworks is an open-source project and contributions are welcome. This document covers how to get set up, what the conventions are, and how to make your contribution count.

## Getting started

### Prerequisites

- Python 3.12+
- [uv](https://docs.astral.sh/uv/) for package and environment management
- Go (latest stable) - only needed if working on Go acceleration components

### Clone and build

```bash
git clone git@github.com:dunamismax/patchworks.git
cd patchworks
uv sync
```

### Run the full verification suite

```bash
uv sync
ruff check .
ruff format --check .
pyright
pytest
```

Every PR should pass all of these.

### Go verification (when Go components exist)

```bash
cd go/
go build ./...
go test ./...
go vet ./...
golangci-lint run
govulncheck ./...
```

## Project structure

```
src/patchworks/
  __init__.py       Package root
  __main__.py       CLI entrypoint
  cli.py            Subcommand dispatch (keep thin)
  db/               SQLite inspection, snapshots, diff orchestration
  diff/             Schema diffing, row diffing, SQL export, merge, semantic analysis
tests/              All test modules
go/                 (future) Go acceleration components
```

Read [`ARCHITECTURE.md`](ARCHITECTURE.md) for detailed technical documentation.

## Code conventions

### Python style

- **Python 3.12+.** Use modern syntax and type hints.
- **Type hints on all public functions.** Pyright must pass clean.
- **`ruff check`** is the lint authority. Fix warnings, do not suppress them without justification.
- **`ruff format`** is the formatting authority. Run `ruff check --fix` first for import sorting.
- **No `assert` outside tests** for control flow. Use explicit error handling.

### Go style (when applicable)

- **`gofmt`** for formatting.
- **`golangci-lint`** for linting.
- **`modernc.org/sqlite`** for SQLite access. No CGO.

### Architecture rules

- `cli.py` dispatches. It does not own persistence or business logic.
- `db/` and `diff/` own data inspection, comparison, snapshots, and export logic.
- The CLI and any future web UI call the same backend functions. No forked logic between surfaces.

### Diff and export correctness

- Prefer streaming and bounded-memory approaches when touching inspection, diff, or export hot paths.
- SQL export favors correctness over minimal output. Do not optimize for smaller migrations at the expense of semantic fidelity.
- Test edge cases: WITHOUT ROWID tables, foreign key enforcement, empty tables, tables with only BLOB columns, tables with duplicate sort values.

## Testing

### Running tests

```bash
# Full suite
pytest

# With coverage
pytest --cov=patchworks --cov-report=term-missing

# Specific test file
pytest tests/test_inspector.py

# Specific test
pytest tests/test_inspector.py::test_schema_reading
```

### Writing tests

- Tests go in `tests/`.
- Use `pytest` fixtures for test database setup and teardown.
- Use temporary directories for test databases.

### What to test

- Every new diff or export behavior needs at least one test demonstrating correct output.
- If you fix a bug, add a regression test that would have caught it.

## Submitting changes

### Before opening a PR

1. Run the full verification suite (all commands above).
2. Update `BUILD.md` if your change affects:
   - User-visible behavior
   - Verification workflow
   - Known limits or risks
   - Phase status or checklist items
3. Update `README.md` if your change affects the public-facing product description.
4. Add a progress-log entry to `BUILD.md` if the change is substantive.

### PR expectations

- Keep PRs focused. One concern per PR.
- Describe what changed and why in the PR description.
- If the change is large, explain the approach and any alternatives considered.
- Include test coverage for new behavior.
- Do not mix refactoring with feature work in the same PR.

### Commit messages

- Use imperative mood: "Add streaming export" not "Added streaming export".
- First line is a concise summary (50 chars or less preferred).
- Body explains the why, not the what (the diff shows the what).

## Documentation

- `BUILD.md` is the canonical execution plan and handoff document. Keep it current.
- `README.md` is the public-facing product summary. Keep it honest.
- `AGENTS.md` is the agent-facing architecture memo. Keep it concise.
- `ARCHITECTURE.md` is the deep technical architecture document.
- `CHANGELOG.md` tracks release history.
- When docs and code disagree, code and tests win. Fix the docs immediately.

## Questions?

Open an issue on GitHub. For substantial proposals, open a discussion first to align on approach before writing code.
