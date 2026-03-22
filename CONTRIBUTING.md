# Contributing to Patchworks

Patchworks is an open-source project and contributions are welcome. This document covers how to get set up, what the conventions are, and how to make your contribution count.

## Getting started

### Prerequisites

- Rust stable toolchain (edition 2021)
- `cargo-nextest` (recommended for faster test execution)
- `cargo-deny` (for dependency policy checks)

No system SQLite is needed — `rusqlite` builds with the `bundled` feature.

### Clone and build

```bash
git clone git@github.com:dunamismax/patchworks.git
cd patchworks
cargo build
```

### Run the full verification suite

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

Every PR should pass all of these. If `cargo bench --no-run` is slow on first run, that's expected — it compiles bundled SQLite in release mode.

## Project structure

```
src/main.rs       → CLI entrypoint (keep thin)
src/app.rs        → Application coordinator
src/db/           → SQLite inspection, snapshots, diff orchestration
src/diff/         → Schema diffing, row diffing, SQL export
src/state/        → UI-facing workspace state
src/ui/           → egui rendering (presentation only)
tests/            → Integration tests, property tests, fixtures
benches/          → Criterion benchmarks
```

Read [`ARCHITECTURE.md`](ARCHITECTURE.md) for detailed module-level documentation.

## Code conventions

### Rust style

- **Edition 2021.** No nightly features.
- **`thiserror`** for library error types, **`anyhow`** only in `main.rs`.
- **No `unwrap()`** outside tests. Use `?` propagation or explicit error handling.
- **Doc comments** on all public types and functions.
- **`cargo fmt`** is the formatting authority. No manual style exceptions.
- **`cargo clippy`** with `-D warnings` is the lint authority. Fix warnings, don't suppress them without justification.

### Architecture rules

- `ui/` renders and handles interaction. It does not own persistence or business logic.
- `state/` stores UI-facing state. It does not perform I/O.
- `db/` and `diff/` own data inspection, comparison, snapshots, and export logic.
- `main.rs` stays thin — argument parsing and dispatch only.
- `app.rs` coordinates between modules. It is the only place that spawns background threads.

### Diff and export correctness

- Prefer streaming and bounded-memory approaches when touching inspection, diff, or export hot paths.
- SQL export favors correctness over minimal output. Do not optimize for smaller migrations at the expense of semantic fidelity.
- Test edge cases: WITHOUT ROWID tables, foreign key enforcement, empty tables, tables with only BLOB columns, tables with duplicate sort values.

## Testing

### Running tests

```bash
# Full suite
cargo test

# Faster parallel execution
cargo nextest run

# Specific test file
cargo test --test diff_tests

# Library unit tests only
cargo test --lib
```

### Writing tests

- Integration tests go in `tests/`.
- Use the fixture system in `tests/fixtures/create_fixtures.sql` for deterministic test data.
- Use `tests/support/mod.rs` helpers (`create_db`, `create_db_with_sql`) for test database setup.
- Property tests use `proptest` and go in `tests/proptest_invariants.rs`.
- Benchmarks use `criterion` and go in `benches/`.

### What to test

- Every new diff or export behavior needs at least one integration test demonstrating correct output.
- Property tests should cover invariants (e.g., "diff + export + apply = identity").
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
- Don't mix refactoring with feature work in the same PR.

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
