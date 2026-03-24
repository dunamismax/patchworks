# Changelog

All notable changes to Patchworks are documented here. This project uses [Keep a Changelog](https://keepachangelog.com/) conventions and will adopt [Semantic Versioning](https://semver.org/) once it reaches 1.0.

## [Unreleased]

## [0.3.0] - 2026-03-24

### Added
- **macOS CI** — CI now runs the full quality gate on both Linux and macOS via a build matrix
- Dedicated "Operational guidance" section in README covering live/WAL-mode databases and large database handling
- Verified install paths: both `cargo install --path .` and `cargo install patchworks` recorded passing on macOS arm64

### Changed
- CI workflow updated from single-platform (Linux) to multi-platform matrix (Linux + macOS)
- Phase 5 (Packaging, platform confidence, and release discipline) marked complete
- README "Known limits" section refined and expanded with operational guidance
- Git remote normalized to dual-push SSH (GitHub + Codeberg)

### Decisions
- Cargo install is the distribution story for now; desktop packaging deferred (decision-0016)
- CI covers Linux and macOS; Windows deferred (decision-0017)
- Platform-specific GUI smoke tests deferred; CLI tests cover the backend truth layer (decision-0018)

## [0.2.0] - 2026-03-24

### Added
- **Headless CLI subcommands** — Patchworks is no longer GUI-only:
  - `patchworks inspect <db>` — print schema summary (tables, columns, views, indexes, triggers)
  - `patchworks diff <left> <right>` — show schema and data changes between two databases
  - `patchworks export <left> <right>` — generate SQL migration to transform left into right
  - `patchworks snapshot save <db>` — save a snapshot of a database
  - `patchworks snapshot list` — list saved snapshots (optionally filtered by source)
  - `patchworks snapshot delete <id>` — delete a saved snapshot
- `--format human|json` flag on `inspect`, `diff`, and `snapshot list` for machine-readable output
- `-o/--output <file>` flag on `export` to write SQL migration directly to a file
- Exit code conventions: 0 = success/no differences, 1 = error, 2 = differences found (enables CI gating)
- JSON serialization error variant in `PatchworksError` to support structured CLI output
- `list_all_snapshots()` and `delete_snapshot()` methods on `SnapshotStore`
- `src/cli.rs` module with all headless command logic, sharing the same backend truth layer as the GUI
- 18 new CLI integration tests in `tests/cli_tests.rs` proving CLI/GUI parity
- 7 unit tests in `src/cli.rs` for command output behavior
- Backward compatibility: `--snapshot <db>` legacy flag still works, bare arguments still launch the GUI
- Streaming SQL export API (`write_export`) that writes one statement at a time to any `Write` sink for bounded-memory large migrations
- Row-at-a-time table seeding via `for_each_row` — export no longer materializes entire tables in memory
- WAL-mode database regression test covering inspection, diffing, and export application
- Streaming export tests: parity with in-memory export, file-based round-trip, and large-table (5000+ rows) verification
- Edge-case regression tests for empty databases and table-added-from-empty diffs
- Explicit live/WAL trust boundary documentation in README.md

### Changed
- CLI restructured from flat args to clap subcommands (backward-compatible with existing usage)
- Phase 4 (Headless CLI and automation surface) marked complete
- Phase 3 (responsiveness and large-database hardening) marked complete
- SQL export internals refactored from `Vec<String>` accumulation to streaming `Write` output
- Reframed `BUILD.md` as an active post-release execution manual

## [0.1.0-post] - 2026-03-22

### Added
- Background processing for database inspection, table loading, and diff computation
- Staged progress reporting for long-running operations
- Foreign-key-safe SQL export with PRAGMA guards and temporary-table rebuild
- Trigger drop/recreate around migration DML to prevent left-side trigger execution
- WITHOUT ROWID table handling in row diff (fallback with warnings)
- Property tests for schema/row-diff invariants and SQL export round-trips
- Criterion benchmarks for query and diff hot paths
- `ARCHITECTURE.md` deep technical documentation
- `CONTRIBUTING.md` contributor onboarding guide
- `CHANGELOG.md` release history tracking

### Changed
- SQL export now rebuilds schema-changed tables via temporary replacement for FK safety
- Row diff no longer assumes `rowid` exists when shared primary keys diverge
- README.md updated to release quality
- BUILD.md aligned to the v0.1.0 release-baseline framing
- All quality gates verified passing

## [0.1.0] - 2026-03-21

Initial release on crates.io.

### Added
- Native desktop GUI built on egui/eframe
- SQLite database inspection: tables, views, columns, row counts
- Table browsing with pagination and sortable columns
- Schema-level diff: detect added, removed, and modified tables
- Row-level diff with streaming merge comparison for shared tables
- Rowid fallback when primary keys diverge (with warnings)
- Index and trigger tracking from `sqlite_master`
- Index and trigger preservation in generated SQL migrations
- Local snapshot store at `~/.patchworks/`
- Snapshot creation from CLI: `patchworks --snapshot <db>`
- SQL export preview, copy-to-clipboard, and save-to-disk
- Deterministic sorted pagination with primary-key/rowid tie-breaker
- CLI modes: empty launch, single-file inspect, two-file diff
- Integration test suite with SQLite fixtures
- GitHub Actions CI
- `cargo-deny` dependency policy
- Published to crates.io as `patchworks`

### Known limitations
- GUI-first only; no headless CLI for inspect/diff/export
- Views are inspect-only (not diffed or exported)
- No explicit cancel for long-running operations
- Large exports materialize significant data in memory
- Best-effort handling of live/WAL-backed databases
- Linux-only CI

[Unreleased]: https://github.com/dunamismax/patchworks/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/dunamismax/patchworks/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/dunamismax/patchworks/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/dunamismax/patchworks/releases/tag/v0.1.0
