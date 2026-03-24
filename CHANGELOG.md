# Changelog

All notable changes to Patchworks are documented here. This project uses [Keep a Changelog](https://keepachangelog.com/) conventions and will adopt [Semantic Versioning](https://semver.org/) once it reaches 1.0.

## [Unreleased]

### Added
- Streaming SQL export API (`write_export`) that writes one statement at a time to any `Write` sink for bounded-memory large migrations
- Row-at-a-time table seeding via `for_each_row` — export no longer materializes entire tables in memory
- WAL-mode database regression test covering inspection, diffing, and export application
- Streaming export tests: parity with in-memory export, file-based round-trip, and large-table (5000+ rows) verification
- Edge-case regression tests for empty databases and table-added-from-empty diffs
- Explicit live/WAL trust boundary documentation in README.md

### Changed
- Reframed `BUILD.md` as an active post-release execution manual instead of a closed-out project memo
- Aligned `AGENTS.md` with the active roadmap posture after v0.1.0
- Phase 3 (responsiveness and large-database hardening) marked complete
- SQL export internals refactored from `Vec<String>` accumulation to streaming `Write` output

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

[Unreleased]: https://github.com/dunamismax/patchworks/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/dunamismax/patchworks/releases/tag/v0.1.0
