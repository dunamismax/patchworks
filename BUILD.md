# BUILD.md

## Purpose

This file is the execution manual for `patchworks`.

It should answer, at a glance:

- what patchworks does right now
- what is shipped and verified
- what is actively being hardened next
- what is planned but not yet built
- what decisions, risks, and open questions shape the repo

This is not a release tombstone. It is the working plan for a shipped product that still has room to grow. When code and docs disagree, fix them together in the same change.

---

## Mission

Build the definitive SQLite comparison and migration tool — the `git diff` of the database world.

Patchworks exists because there is no trustworthy, purpose-built tool for understanding what changed between two SQLite databases. Not a hex editor. Not a shell script. A real tool — correct, fast, and native — that a developer or operator can point at two database files and immediately understand the delta.

### Long-term vision

1. **Today**: A desktop app that inspects, diffs, snapshots, and exports SQL migrations between SQLite databases.
2. **Next**: A headless CLI that makes the same engine scriptable for automation, CI pipelines, and pre-commit hooks.
3. **After that**: A more intelligent diff and migration engine with stronger semantic understanding, validation, and conflict handling.
4. **Long-term**: A plugin-extensible platform with optional team workflows, shared snapshot registries, and deeper CI/CD integration.

The through-line is unchanged: SQLite-specific correctness first. Every new feature earns its place by being trustworthy before being powerful.

---

## Current release posture

**Patchworks v0.3.0 is released, and the project is still active.** The desktop app and headless CLI both ship inspection, diffing, snapshots, and SQL export. CI now covers both Linux and macOS. Install paths (`cargo install --path .` and `cargo install patchworks`) are verified. Phase 6 (product polish) is complete — the desktop app now has a schema browser, table search, keyboard shortcuts, theme support, recent files, and improved diff UX. The next job is advanced diff intelligence (Phase 7).

## Current execution posture

Patchworks is in the healthy middle state between prototype and finished platform.

- **Shipped baseline:** v0.3.0 exists on crates.io with both desktop GUI and headless CLI.
- **Hardening complete:** Phase 3 landed streaming export, bounded-memory table seeding, WAL regression coverage, and explicit trust-boundary documentation.
- **CLI complete:** Phase 4 landed headless subcommands for inspect, diff, export, and snapshot management with JSON output and CI-friendly exit codes.
- **Platform confidence complete:** Phase 5 landed macOS CI, verified install paths, tightened operational guidance in README, and recorded packaging decisions.
- **UX polish complete:** Phase 6 landed schema browser, table search/filter, keyboard shortcuts, theme support, recent files, collapsible diff sections, and diff summary statistics.
- **Active lane:** advanced diff intelligence (Phase 7).
- **Discipline:** roadmap boxes are not aspiration theater. Check them only after code lands and the relevant verification is recorded.

If a future pass changes the real priorities, update this section first rather than letting the roadmap drift silently.

---

## Repo snapshot

**Status: Released (v0.3.0), active roadmap continuing**

**Package:** crates.io [`patchworks`](https://crates.io/crates/patchworks) (`0.3.0`)
**Primary surfaces:** native Rust desktop app via `egui`/`eframe` and headless CLI

What exists:

- Opens zero, one, or two SQLite database files in a native desktop UI
- Inspects tables and views with row browsing, pagination, and sortable columns
- Dedicated schema browser panel showing tables, views, indexes, and triggers with DDL preview
- Table name search/filter in file panels
- Computes schema diffs and row diffs between two databases
- Collapsible diff sections with summary statistics and per-table change indicators
- Saves snapshots into a local Patchworks store under `~/.patchworks/`
- Compares a live database against a saved snapshot
- Generates SQL export that transforms the left database into the right database
- Preserves tracked indexes and triggers in generated SQL
- Guards `PRAGMA foreign_keys` and uses temporary-table rebuilds for schema-changed tables
- Runs inspection, table loading, and diffing on background threads with staged progress
- Keyboard shortcuts: ⌘1-6 for views, ⌘D for diff
- Theme support: dark, light, and system-following
- Recent-files memory with quick reopen from toolbar menu
- Headless CLI subcommands: `inspect`, `diff`, `export`, `snapshot save/list/delete`
- Machine-readable JSON output (`--format json`) on inspect, diff, and snapshot list
- CI-friendly exit codes: 0 = success, 1 = error, 2 = differences found
- File output for exports (`-o/--output`)

What does **not** exist yet:

- View diffing or export support
- Explicit cancel control for long-running background jobs
- Formal desktop packaging or installer automation beyond Cargo packaging
- Strong guarantees for heavily changing live databases or actively-written WAL-backed files (read-only access works, but concurrent writes during inspection can produce inconsistent results)

### Recorded verification baseline

- Verified on: 2026-03-24
- Repo path: `/Users/sawyer/github/patchworks`
- Branch: `main`
- Host: macOS arm64 (`Darwin 25.4.0`)
- Release verification: build, test (58 tests), clippy, fmt, bench-compile, deny, and CLI help were recorded passing
- Install verification: both `cargo install --path .` and `cargo install patchworks` (from crates.io) recorded passing on macOS arm64

This baseline is still useful, but it is not permission to stop verifying. Any later change that touches product behavior should record its own narrower proof.

---

## Source-of-truth mapping

| File | Owns |
|------|------|
| `README.md` | Public-facing project description, honest status |
| `BUILD.md` | Implementation map, phase tracking, decisions, working rules |
| `AGENTS.md` | Agent-facing architecture memo |
| `ARCHITECTURE.md` | Deep architectural documentation |
| `CONTRIBUTING.md` | Setup, conventions, how to contribute |
| `CHANGELOG.md` | Release history and unreleased doc/product deltas |
| `Cargo.toml` | Package manifest, dependency graph, crate publishing posture |
| `deny.toml` | Dependency policy |
| `.github/workflows/ci.yml` | CI entrypoint |
| `src/main.rs` | CLI entrypoint, subcommand dispatch, and startup behavior |
| `src/cli.rs` | Headless CLI command implementations |
| `src/app.rs` | Top-level app orchestration, background task wiring, UI-to-backend coordination |
| `src/error.rs` | Shared error model |
| `src/db/inspector.rs` | SQLite inspection, paging, and summary loading |
| `src/db/differ.rs` | Diff orchestration and SQL export entrypoints |
| `src/db/snapshot.rs` | Snapshot persistence and local store behavior |
| `src/db/types.rs` | Shared database and diff data types |
| `src/diff/schema.rs` | Schema diff rules |
| `src/diff/data.rs` | Row diff rules and invariants |
| `src/diff/export.rs` | SQL export generation |
| `src/state/workspace.rs` | UI-facing workspace state |
| `src/state/recent.rs` | Recent-files persistence |
| `src/ui/` | Rendering and interaction surfaces |
| `src/ui/schema_browser.rs` | Schema browser with DDL preview |
| `tests/cli_tests.rs` | CLI command behavior and CLI/GUI parity expectations |
| `tests/diff_tests.rs` | Diff and export behavior expectations |
| `tests/proptest_invariants.rs` | Property-based invariant checks |
| `tests/snapshot_tests.rs` | Snapshot behavior expectations |
| `benches/diff_hot_paths.rs` | Diff performance tracking |
| `benches/query_hot_paths.rs` | Query performance tracking |

**Invariant:** If docs, code, and CLI output ever disagree, the next change must reconcile all three. When docs and code disagree, code and tests win — update docs immediately rather than carrying mismatched narratives.

---

## Working rules

1. **Read `BUILD.md`, `README.md`, and `AGENTS.md`** before making substantial changes.
2. **Keep the layering honest.** `ui/` renders and handles interaction. `state/` stores UI-facing state. `db/` and `diff/` own data inspection, comparison, snapshots, and export logic. `main.rs` stays thin.
3. **Do not claim flows are verified** unless they were actually exercised and recorded.
4. **When behavior changes** around snapshots, SQL export, diff correctness, performance limits, or live-database handling, update this file in the same pass.
5. **Prefer streaming and bounded-memory approaches** when touching inspection, diff, or export hot paths.
6. **Keep `README.md` aligned with recorded reality.** Do not market future work as present capability.
7. **If a task changes user-visible behavior or repo workflow,** add a progress-log entry and, when appropriate, a decision-log entry.
8. **If new work reveals an architectural or product ambiguity,** record it under open questions or decisions instead of leaving it implicit.
9. **Correctness over cleverness.** A heavier migration that is semantically correct beats a minimal one that breaks edge cases.
10. **SQLite-native.** Preserve the nuance of SQLite (`rowid`, `WITHOUT ROWID`, WAL, PRAGMA behavior) instead of flattening into generic database abstractions.
11. **Release is a baseline, not a finish line.** Do not write docs as if v0.1.0 exhausted the product's purpose.

---

## Tracking conventions

Use this language consistently in docs, commits, and issues:

| Term | Meaning |
|------|---------|
| **done** | Implemented and verified |
| **checked** | Verified by command or test output |
| **in progress** | Actively being worked on or still the current lane |
| **queued** | Intentionally next, but not started in code yet |
| **planned** | Real roadmap work, not yet queued for the next pass |
| **exploratory** | Worth investigating, but still needs sharper scope or decisions |
| **blocked** | Cannot proceed without a decision or dependency |
| **risk** | Plausible failure mode that could distort the design |
| **decision** | A durable call with consequences |

Checkboxes describe landing state. Unchecked work should be specific enough to build, not vague aspiration. The progress log is append-first: preserve verification context, then update the current-state sections so the plan stays usable.

### Progress log format

- `YYYY-MM-DD: scope - outcome. Verified with: <commands or audit summary>. Next: <follow-up>.`

### Decision log format

- `YYYY-MM-DD: decision - rationale - consequence.`

---

## Quality gates

### Standard gate (all phases)

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

### Scenario-specific checks

Use these when the change touches the relevant area:

- Packaging and crates.io metadata changes:
  - `cargo metadata --no-deps --format-version 1`
  - `cargo package --allow-dirty --list`
  - `cargo package --allow-dirty`
- Snapshot CLI behavior:
  - `cargo run -- --snapshot <db>`
- GUI launch smoke test:
  - `cargo run`
  - `cargo run -- app.db`
  - `cargo run -- left.db right.db`

Do not record these as passed unless they were actually run in the repo. If a gate is temporarily unavailable, document why. Never silently skip.

### Currently recorded verified commands

Re-verified on 2026-03-24 (v0.3.0 release):

- `cargo build`
- `cargo test` (58 tests)
- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo bench --no-run`
- `cargo deny check`
- `cargo run -- --help`
- `cargo install --path .`
- `cargo install patchworks` (from crates.io)

Still recorded from the 2026-03-21 packaging pass:

- `cargo metadata --no-deps --format-version 1`
- `cargo package --allow-dirty --list`
- `cargo package --allow-dirty`
- `cargo run -- --snapshot <db>`

Supported by the codebase but not fully re-verified in the recorded passes above:

- `cargo run`
- `cargo run -- app.db`
- `cargo run -- left.db right.db`
- `cargo run -- inspect <db>`
- `cargo run -- diff <left> <right>`
- `cargo run -- export <left> <right>`
- `cargo run -- snapshot save <db>`
- `cargo run -- snapshot list`
- `cargo run -- snapshot delete <id>`
- `cargo install --path .`
- `cargo install patchworks`

---

## Dependency strategy

Patchworks is a native Rust application. Dependencies are managed through `Cargo.toml` with policy enforcement via `cargo-deny` (`deny.toml`).

**Principles:**

- Every dependency must justify itself against implementing the functionality directly
- `cargo deny check` must pass as part of the standard quality gate
- Development-only dependencies stay in `[dev-dependencies]`
- The `rusqlite` `bundled` feature is used to ship SQLite without requiring a system install

---

## Current priority stack

### Priority 1 — Advanced diff intelligence

The desktop UX is polished. The next step is making the diff engine smarter.

Near-term intelligence work:

- column-level change highlighting within modified rows
- diff filtering by change type and by table
- diff statistics and summary views beyond the current aggregate bar
- semantic diff awareness for column renames and compatible type shifts

### Priority 2 — Error recovery polish (Phase 6 residual)

One Phase 6 goal was deferred: clearer error recovery with retry affordances, richer diagnostic detail, and user-facing failure states. Current error display is adequate but could be more actionable.

### Priority 3 — Migration workflow management

After the diff engine is intelligent and the UX covers error recovery well, begin building migration chains.

If a code pass does not obviously move one of these priorities, it should say why.

---

## Phase dashboard

| Phase | Name | Status |
|-------|------|--------|
| 0 | Repo baseline, workflow, and packaging truth | **Done** |
| 1 | Desktop inspection, diff, snapshot, and export MVP | **Done** |
| 2 | Schema-object fidelity and quality rails | **Done** |
| 3 | Responsiveness and large-database hardening | **Done** |
| 4 | Headless CLI and automation surface | **Done** |
| 5 | Packaging, platform confidence, and release discipline | **Done** |
| 6 | Product polish and UX refinement | **Done** |
| 7 | Advanced diff intelligence | **Planned** |
| 8 | Migration workflow management | **Planned** |
| 9 | Plugin and extension architecture | **Exploratory** |
| 10 | Team features and shared snapshot registries | **Exploratory** |
| 11 | CI/CD integration and automation ecosystem | **Planned** |
| 12 | Long-term platform evolution | **Exploratory** |
| 13 | Tauri 2 desktop shell | **Planned** |

Phases 0-6 are the shipped foundation. Phase 7 (advanced diff intelligence) is the next active build step.

---

### Phase 0 — Repo baseline, workflow, and packaging truth
**Status: done**

Goals:
- [x] Establish the single-crate Rust package as the repo's active implementation surface
- [x] Record the local build, test, lint, benchmark-compile, and dependency-policy workflow
- [x] Add checked-in CI entrypoints and `cargo-deny` policy
- [x] Align crate metadata for crates.io and docs.rs
- [x] Ensure the packaged README uses a relative `LICENSE` link and passes local package validation

Exit criteria:
- [x] A contributor can see the real package/workflow surface from the root docs
- [x] `cargo package --allow-dirty` is a recorded success path in repo history

---

### Phase 1 — Desktop inspection, diff, snapshot, and export MVP
**Status: done**

Goals:
- [x] Support empty launch, one-file inspect, two-file diff, and snapshot CLI startup paths
- [x] Inspect SQLite tables and views in the desktop UI
- [x] Browse table rows with pagination and sortable columns
- [x] Compute schema-level diffs
- [x] Compute row-level diffs for shared tables
- [x] Save and compare snapshots through the local Patchworks store
- [x] Generate SQL export, preview it in the UI, copy it, and save it to disk

Exit criteria:
- [x] Patchworks is useful as a desktop inspection and comparison tool for normal local workflows
- [x] Snapshot creation and SQL export are part of the real product, not just planned scope

---

### Phase 2 — Schema-object fidelity and quality rails
**Status: done**

Goals:
- [x] Track indexes and triggers from `sqlite_master` through inspection and export planning
- [x] Recreate changed indexes from the right-side schema during export generation
- [x] Drop and recreate affected triggers around migration DML so left-side trigger logic is not accidentally executed
- [x] Add deterministic sorted-pagination tie-breakers using primary-key or `rowid` fallback
- [x] Add integration tests for schema diff, data diff, rowid fallback, snapshot behavior, and SQL export behavior
- [x] Add property tests for schema and row-diff invariants
- [x] Add Criterion benchmark entrypoints for query and diff hot paths
- [x] Add background diff task coordination tests in `src/app.rs`

Exit criteria:
- [x] The repo has meaningful executable coverage for core diff and export behavior
- [x] Generated SQL preserves more of the actual SQLite schema story than table-only export logic

---

### Phase 3 — Responsiveness and large-database hardening
**Status: done**

Shipped already:
- [x] Move database inspection off the UI thread
- [x] Move table-page refresh work off the UI thread or otherwise bound its impact on interactivity
- [x] Add progress reporting for long-running background inspection, table-load, and diff jobs
- [x] Decide whether explicit diff cancellation belongs in the current architecture
- [x] Refactor SQL export to a streaming `Write`-based API (`write_export`) that flushes one statement at a time
- [x] Replace full-table materialization during export seeding with row-at-a-time streaming via `for_each_row`
- [x] Add regression coverage for WAL-mode databases (inspection, diffing, and export application)
- [x] Add regression coverage for streaming export (parity with in-memory, file-based round-trip, large-table seeding)
- [x] Add regression coverage for edge cases (empty databases, table-added-from-empty)
- [x] Document the live/WAL trust boundary explicitly in README and BUILD.md
- [x] Decide that large-table diffing does not need table-level parallelism (see decision-0013)
- [x] Decide that `SnapshotStore` stays simple with per-operation connections (see decision-0014)

Exit criteria:
- [x] Export generation has a bounded-memory story: `write_export` streams to any `Write` sink with one row at a time; the convenience `export_diff_as_sql` still collects to `String` for GUI preview
- [x] The trust boundary for live/WAL-backed databases is explicit in tests (WAL test) and docs (README, BUILD.md)
- [x] Remaining performance tradeoffs are conscious decisions: single-threaded diff order and per-operation snapshot connections are documented decisions, not accidental leftovers

Residual limits acknowledged:
- The in-memory convenience path (`export_diff_as_sql`) used for GUI preview still collects the full export into a `String`; very large migrations shown in-UI may still use significant memory
- Background workers do not yet support cooperative interruption or user-facing cancel (deferred to a future cancellable job abstraction per decision-0011)
- Row-level diff results (`TableDataDiff`) are still fully materialized in memory; streaming the diff result itself would require a deeper architectural change

---

### Phase 4 — Headless CLI and automation surface
**Status: done**

Goals:
- [x] Design the command shape for `patchworks inspect`, `patchworks diff`, `patchworks export`, and `patchworks snapshot ...`
- [x] Add a headless inspect command
- [x] Add a headless diff command
- [x] Add a headless SQL export command
- [x] Add snapshot listing and cleanup commands
- [x] Decide which machine-readable output formats belong in the first CLI expansion (human + JSON via `--format`)
- [x] Define and document exit code conventions early instead of retrofitting them later (0 = ok, 1 = error, 2 = diff found)
- [x] Keep the CLI on the same backend logic rather than forking separate comparison code
- [x] Add CLI-focused fixtures or golden-output checks that prove parity with the GUI-backed truth layer

Exit criteria:
- [x] Patchworks can participate in scripted workflows, not only interactive desktop sessions
- [x] CLI and GUI paths share the same diff and export truth layer
- [x] The first CLI contract is small enough to support, but real enough to build automation on top of

Risks addressed:
- **mitigated:** CLI calls the same `diff_databases`, `inspect_database`, `write_export`, and `SnapshotStore` functions as the GUI — no forked logic
- **mitigated:** exit codes and output formats are documented and tested from day one

---

### Phase 5 — Packaging, platform confidence, and release discipline
**Status: done**

Goals:
- [x] Re-verify `cargo install --path .` and/or `cargo install patchworks` explicitly and record the result
- [x] Add at least one macOS CI build smoke path in addition to Linux
- [x] Decide whether the project needs release archives, installers, or desktop packaging beyond Cargo install
- [x] Tighten README and BUILD guidance around live databases, WAL mode, and other operational caveats
- [x] Define what the next release-quality support bar actually is (`0.1.x` polish vs `0.2.0` scope)
- [x] Decide whether platform-specific smoke tests need minimal fixture databases for launch-and-open scenarios

Exit criteria:
- [x] Installation expectations are documented from actual verification, not assumption
- [x] CI covers the platforms most likely to matter for a desktop SQLite tool
- [x] The project can describe its release posture without hand-waving

---

### Phase 6 — Product polish and UX refinement
**Status: done**

Goals:
- [x] Decide whether views should stay inspect-only or gain diff/export support (reaffirmed: inspect-only per decision-0004)
- [x] Decide whether indexes and triggers need dedicated UI panels instead of export-only preservation (browsable via schema browser; dedicated diff panels deferred)
- [x] Add a dedicated schema browser panel (tables, views, indexes, triggers with DDL preview)
- [x] Add search and filter across table names in file panels
- [x] Refine diff UX: collapsible sections, per-table change indicators, summary statistics bar
- [x] Add keyboard shortcuts for core workflows (⌘1-6 for views, ⌘D to trigger diff)
- [x] Add theme support (light/dark/system)
- [x] Add recent-files workspace memory so users can quickly reopen previous sessions
- [ ] Add clearer error recovery: retry affordances, diagnostic detail, and user-facing failure states (deferred — current error display is adequate for the shipped feature set)

Exit criteria:
- [x] The app feels like a durable tool, not just a technically correct prototype
- [x] UX scope follows proven correctness and performance improvements instead of papering over unfinished core behavior

---

### Phase 7 — Advanced diff intelligence
**Status: planned**

Goals:
- [ ] Add column-level change highlighting within modified rows
- [ ] Add diff filtering by change type and by table
- [ ] Add diff statistics and summary views
- [ ] Add semantic diff awareness for table renames, column renames, and compatible type shifts where defensible
- [ ] Add conflict detection for two databases compared against a common ancestor snapshot
- [ ] Add three-way merge support with explicit conflict surfacing
- [ ] Add diff annotations for triage workflows
- [ ] Add data-type-aware comparison rules that distinguish cosmetic differences from semantic ones

Exit criteria:
- [ ] Patchworks provides actionable intelligence about changes, not just raw deltas
- [ ] Three-way merge works correctly for non-conflicting cases and clearly exposes conflicts when manual judgment is required

---

### Phase 8 — Migration workflow management
**Status: planned**

Goals:
- [ ] Add migration chain support: generate, store, and replay ordered migration sequences
- [ ] Add migration validation by applying generated SQL to a copy and verifying the result matches the target
- [ ] Add rollback generation for reversible migrations where possible
- [ ] Add migration squashing for sequential migrations
- [ ] Add migration history tracking in the Patchworks store
- [ ] Add migration conflict detection when multiple migrations target the same objects
- [ ] Add a `patchworks migrate` CLI command with safety checks
- [ ] Add `--dry-run` mode for migration operations
- [ ] Decide whether custom pre/post migration hooks belong in core or later extensions

Exit criteria:
- [ ] Users can manage ordered database migrations through Patchworks rather than ad-hoc SQL files alone
- [ ] Migrations can be validated before application and tracked after application

---

### Phase 9 — Plugin and extension architecture
**Status: exploratory**

Goals:
- [ ] Design a plugin surface for alternate diff formatters (HTML, Markdown, JSON)
- [ ] Design a plugin surface for export targets (Alembic, Flyway, Liquibase)
- [ ] Design a plugin surface for custom inspectors or app-specific validation rules
- [ ] Evaluate discovery and loading mechanisms
- [ ] Add built-in reference plugins only after the core API shape is stable enough to deserve them
- [ ] Define plugin API stability and versioning expectations
- [ ] Add plugin author documentation only after the extension contract stops moving weekly

Exit criteria:
- [ ] Third-party developers can extend Patchworks without forking
- [ ] The extension surface has a stability story instead of accidental API leakage

---

### Phase 10 — Team features and shared snapshot registries
**Status: exploratory**

Goals:
- [ ] Design a snapshot registry protocol for optional push/pull workflows
- [ ] Add snapshot naming, tagging, and annotation
- [ ] Add snapshot comparison across machines
- [ ] Add snapshot retention policies and integrity verification
- [ ] Add access-control primitives if remote/shared registries become real
- [ ] Keep local-only workflows first-class instead of turning shared state into a requirement

Exit criteria:
- [ ] Teams can share snapshots through an optional registry without breaking the local-first model
- [ ] Registry integrity and trust boundaries are explicit

---

### Phase 11 — CI/CD integration and automation ecosystem
**Status: planned**

Goals:
- [ ] Add a `patchworks check` command for CI gates
- [ ] Add GitHub Actions integration examples using the future headless CLI
- [ ] Add pre-commit hook support
- [ ] Add machine-readable output formats across CLI commands
- [ ] Add a `--format` flag with stable human/json/jsonl behavior where appropriate
- [ ] Stabilize and document exit code conventions
- [ ] Evaluate `patchworks watch` mode for file-change monitoring
- [ ] Add GitOps workflow documentation after the CLI contract exists

Exit criteria:
- [ ] Patchworks can be dropped into CI with a small, documented command surface
- [ ] Machine-readable output and exit codes are stable enough for automation to depend on

---

### Phase 12 — Long-term platform evolution
**Status: exploratory**

Goals:
- [ ] Evaluate multi-engine support (DuckDB, libSQL, or other embedded databases) without sacrificing SQLite-first correctness
- [ ] Evaluate embedded scripting (Lua, Rhai, WASM)
- [ ] Evaluate a TUI mode as a middle ground between GUI and CLI
- [ ] Evaluate remote database support (SSH, HTTP, cloud storage)
- [ ] Evaluate schema visualization and ERD generation
- [ ] Evaluate integration with existing migration frameworks
- [ ] Evaluate performance profiling integration as part of normal developer workflow

Exit criteria:
- [ ] Each exploration produces a build, defer, or reject decision with rationale
- [ ] Any accepted capability follows the same phase-gated discipline as the shipped desktop core

---

### Phase 13 — Tauri 2 desktop shell
**Status: planned**

Patchworks currently ships as a native egui/eframe desktop app. Tauri 2 would provide a second desktop shell using the React + Vite browser-facing stack while keeping the Rust core untouched. This unifies with the broader React + Vite lane used across other products and opens a path to richer UI capabilities (rich text diff rendering, syntax-highlighted SQL preview, responsive layouts) without replacing the headless CLI or the existing egui surface.

Goals:
- [ ] scaffold a Tauri 2 shell alongside the existing egui app (both shells consume the same Rust core crates)
- [ ] expose `inspect_database`, `diff_databases`, `write_export`, and `SnapshotStore` as Tauri commands
- [ ] build the diff and inspection views using React + TanStack + shadcn/ui
- [ ] add syntax-highlighted SQL export preview in the browser shell
- [ ] keep the egui surface available as the lightweight/portable option
- [ ] keep the headless CLI available for automation and CI
- [ ] add Tauri-specific packaging for macOS (.dmg) and Linux (AppImage) as the first real installer story
- [ ] decide whether the Tauri shell replaces the egui surface long-term or coexists as a separate distribution

Exit criteria:
- [ ] the Tauri shell provides the same core capabilities as the egui desktop app (inspect, diff, export, snapshots)
- [ ] packaging produces installable artifacts beyond `cargo install`
- [ ] the Rust core crates remain shell-agnostic

---

## Decisions

### decision-0001: GUI-first desktop tool
**Date:** 2026-03-20

The shipped product is the desktop app with a small snapshot CLI surface. Headless automation work stays explicit future scope instead of implied capability.

### decision-0002: Snapshot state under `~/.patchworks/`
**Date:** 2026-03-20

Snapshot metadata and copied database files live under `~/.patchworks/`. This keeps user-local state outside the repo. Snapshot behavior is local machine state, not project state.

### decision-0003: SQL export favors correctness over minimality
**Date:** 2026-03-20

Modified tables are rebuilt from the right-side schema when needed. Exports may be heavier, but semantic fidelity takes priority at this stage.

### decision-0004: Views remain inspect-only
**Date:** 2026-03-20

This keeps the current scope focused on table-centered diffing and export. Any view diff or export support must be added deliberately later.

### decision-0005: Sorted pagination with deterministic tie-breakers
**Date:** 2026-03-20

Sorted pagination appends a primary-key or `rowid` tie-breaker to preserve deterministic page boundaries across duplicate sort values.

### decision-0006: Indexes and triggers tracked from `sqlite_master`
**Date:** 2026-03-20

Indexes and triggers are tracked and preserved in generated SQL. Schema fidelity can advance ahead of UI completeness without requiring dedicated panels first.

### decision-0007: Background diff execution
**Date:** 2026-03-20

Diff computation runs on a background thread. Later responsiveness work extended that handoff model to inspection and visible-table refresh work.

### decision-0008: Crate metadata aligned for crates.io
**Date:** 2026-03-21

`cargo package --allow-dirty` is a recorded success path. Future docs changes must keep packaged links and publishability checks honest.

### decision-0009: Foreign-key safety in SQL export
**Date:** 2026-03-22

SQL export now prioritizes SQLite foreign-key safety over preserving original `CREATE TABLE` header text byte-for-byte. Schema-changed tables are rebuilt via a temporary replacement table and the generated migration batch guards `PRAGMA foreign_keys`.

### decision-0010: Background worker architecture
**Date:** 2026-03-22

Inspection and visible-table refresh run on background worker threads coordinated from `src/app.rs`. Stale results are dropped by replacing their receivers. Future responsiveness work should build on this shape instead of reintroducing synchronous database reads in the render loop.

### decision-0011: No explicit cancellation yet
**Date:** 2026-03-22

Explicit cancellation does not belong in the current background-task model yet. The app reports staged progress and safely supersedes stale work by dropping receivers. Any future cancel control should wait for a cancellable job abstraction rather than bolting partial interruption onto fire-and-forget threads.

### decision-0012: Release status does not imply roadmap freeze
**Date:** 2026-03-24

v0.1.0 is the first stable baseline, not the end of the repo's purpose. BUILD.md should track the next credible build sequence after release rather than reading like the project is done forever.

### decision-0013: Single-threaded table-level diffing is sufficient
**Date:** 2026-03-24

Large-table diffing stays single-threaded at the table level. The streaming merge is already I/O-bound against SQLite reads; parallelizing across tables would add ordering and progress-reporting complexity without clear throughput gains. If profiling later shows CPU-bound bottlenecks, this decision can be revisited with evidence.

### decision-0014: SnapshotStore keeps per-operation connections
**Date:** 2026-03-24

`SnapshotStore` continues opening a fresh SQLite connection for each operation rather than holding a persistent connection. The snapshot metadata database is tiny and accessed infrequently; a persistent connection would add lifetime complexity without measurable benefit. Revisit only if contention evidence appears.

### decision-0016: Cargo install is the distribution story for now
**Date:** 2026-03-24

Desktop packaging (DMG, AppImage, etc.) is deferred. `cargo install patchworks` and `cargo install --path .` are both verified working. The project does not yet have enough users or enough platform-specific behavior to justify installer automation. Revisit when the user base or platform requirements make Cargo install insufficient.

### decision-0017: CI covers Linux and macOS
**Date:** 2026-03-24

CI now runs the full quality gate on both `ubuntu-latest` and `macos-latest`. This is a native desktop tool with an egui/eframe GUI and platform-specific windowing — macOS coverage is essential. Windows CI is deferred until demand or a contributor appears.

### decision-0018: Platform-specific GUI smoke tests are deferred
**Date:** 2026-03-24

Platform-specific launch-and-open smoke tests with fixture databases are not worth the complexity yet. The headless CLI tests already exercise the full backend truth layer. GUI smoke tests would require either headless rendering or manual verification, neither of which scales in CI. Revisit if platform-specific rendering bugs become a pattern.

### decision-0019: Views remain inspect-only (reaffirmed)
**Date:** 2026-03-24

View diffing and export support is not justified by current usage or the diff engine's capabilities. Views are browsable in the schema browser panel. Revisit when users request it or when semantic diff awareness is stronger.

### decision-0020: Schema browser instead of dedicated index/trigger diff panels
**Date:** 2026-03-24

Indexes and triggers are browsable in the schema browser with full DDL preview. The schema diff view already surfaces added, removed, and modified indexes and triggers. Dedicated diff UI panels would add complexity without proven demand.

### decision-0021: Error recovery improvements deferred
**Date:** 2026-03-24

Phase 6 deferred explicit retry affordances and richer error diagnostics. Current error display (colored labels with error text in panes and status bar) is adequate for the shipped feature set. Revisit when user feedback reveals specific pain points.

### decision-0015: CLI shares the GUI's backend truth layer
**Date:** 2026-03-24

Headless CLI subcommands call the same `inspect_database`, `diff_databases`, `write_export`, and `SnapshotStore` functions as the desktop GUI. No separate comparison or export logic exists for the CLI. Integration tests explicitly verify CLI/GUI parity. This eliminates the risk of behavioral divergence between surfaces.

---

## Risks

### Active risks

- Background inspection, table loading, and diff execution report coarse staged progress but do not support cooperative interruption or a user-facing cancel control
- The GUI preview path (`export_diff_as_sql`) still collects full export into a `String`; very large migrations displayed in-UI may use significant memory even though the file-export path (`write_export`) streams
- Live databases, WAL-backed databases, encrypted databases, and actively changing sources are handled on a best-effort basis with read-only access; concurrent writes during inspection can produce inconsistent results
- CI covers Linux and macOS but not Windows
- Snapshot matching depends on canonicalized paths and can behave awkwardly if files move
- Row-level diff results are still fully materialized in memory; very large diffs with millions of changed rows could exhaust memory before export begins

### Strategic risks

- Feature breadth expanding faster than test coverage could erode the correctness trust that defines Patchworks
- A plugin system introduces API stability obligations that constrain future refactoring
- Shared snapshot registries introduce network, auth, and security concerns absent in the local-only model
- Multi-engine support could dilute SQLite-specific correctness if abstraction boundaries are drawn wrong
- The diff engine is currently synchronous and single-threaded at the table level; parallelizing across tables may help on large multi-table diffs, but it introduces ordering and progress-reporting complexity

---

## Open questions

| Question | Phase | Impact |
|----------|-------|--------|
| Should the project add desktop packaging (DMG, AppImage) beyond Cargo install for broader reach? | 6 | Distribution |
| Should the first CLI ship human-readable output only, or stabilize JSON/JSONL immediately? | 4, 11 | Automation contract |
| Should the plugin system use compiled Rust dynamic libraries, WASM, or both? | 9 | Architecture |
| Should three-way merge be a core feature or a plugin? | 7, 9 | Scope |
| How should migration chain state be stored? | 8 | Storage architecture |
| Is multi-engine support worth the abstraction cost? | 12 | Product identity |
| Should the shared snapshot registry be a separate service or embedded? | 10 | Architecture |
| Should the GUI preview path move to streaming or lazy rendering for very large exports? | 6 | UX and memory |

---

## Immediate next moves

If somebody picks this repo up for the next substantive pass, the most credible sequence is:

1. **Begin Phase 7 advanced diff intelligence**
   - Column-level change highlighting within modified rows.
   - Diff filtering by change type and by table.
   - Diff statistics and summary views beyond the current aggregate bar.
2. **Error recovery polish (Phase 6 residual)**
   - Retry affordances for failed loads, better diagnostic detail in error states.
3. **Only then widen into migration workflow or plugin work**
   - Migration chains and plugin extensibility should compound on a polished and trustworthy diff engine.

If priorities change, replace this list with the new order rather than letting stale direction linger.

---

## Progress log

### 2026-03-20

- Completed a full repository review and recorded the local workflow baseline for build, lint, test, bench-compile, dependency policy, and CLI help behavior. Verified with: `cargo build`, `cargo test`, `cargo nextest run`, `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo bench --no-run`, `cargo deny check`, `cargo run -- --help`. Next: tighten diff and export correctness and document the real product limits honestly.

- Implemented index and trigger inspection, carried those schema objects through diff and export planning, added deterministic sorted-pagination tie-breakers, and landed regression coverage for schema-object preservation and trigger-safe export behavior. Verified with: `cargo test --test diff_tests`, `cargo fmt --all --check`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`. Next: keep responsiveness, large-export memory use, and live-database caveats as the main follow-up priorities.

- Re-reviewed the full repository after the follow-up implementation and confirmed the strongest remaining risks were synchronous inspection, unbounded export memory growth, Linux-only CI, simple snapshot-store connection management, and best-effort handling of live or WAL-backed databases. Verified with: `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, plus source review across `src/`, `tests/`, `.github/workflows/ci.yml`, and `deny.toml`. Next: prioritize async loading and streaming export design before feature breadth.

### 2026-03-21

- Added crates.io-facing metadata in `Cargo.toml`, updated `README.md` for packaged-readme correctness and local publishability guidance, and recorded the successful local package validation path. Verified with: `cargo metadata --no-deps --format-version 1`, `cargo package --allow-dirty --list`, `cargo package --allow-dirty`, `cargo test`. Next: keep BUILD, README, and future release workflow aligned with the actual package and install story.

- Reframed `BUILD.md` into a phase-based execution plan with clearer source-of-truth mapping, architecture flow, quality gates, risks, and next moves while preserving recorded verification history. Verified with: repo document and source-tree audit of `BUILD.md`, `README.md`, `AGENTS.md`, `Cargo.toml`, `src/`, `tests/`, and `benches/`. Next: update this plan in lockstep with the next real code or verification pass.

### 2026-03-22

- Hardened row diff and SQL export around SQLite edge cases: row diff no longer assumes `rowid` exists when shared primary keys diverge, SQL export now uses a temporary-table rebuild path plus foreign-key guarding so exports apply cleanly when `PRAGMA foreign_keys=ON`, and added regression coverage for WITHOUT ROWID fallback and FK-enforced export application. Verified with: `cargo test --test diff_tests`, `cargo fmt --all --check`, `cargo build`, `cargo test`, `cargo nextest run`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo bench --no-run`, `cargo deny check`, `cargo run -- --help`. Next: keep Phase 3 focused on async inspection and page loading, explicit progress reporting, and bounded-memory export generation.

- Moved database inspection and visible-table refresh onto background workers, added pane and table loading state in the UI, and landed app-level regression coverage for background load application and stale table-refresh dropping. Verified with: `cargo test --lib`, `cargo test`, `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings`. Next: add progress reporting and decide whether explicit cancellation belongs in the current background-task architecture.

- Added staged progress reporting for background database opens, visible-table refreshes, and diff computation; documented that stale jobs are superseded rather than explicitly cancelled; and landed regression coverage for emitted diff progress plus worker-to-UI progress application. Verified with: `cargo test --lib`, `cargo fmt --all --check`, `cargo test`, `cargo nextest run`, `cargo clippy --all-targets --all-features -- -D warnings`. Next: keep Phase 3 focused on bounded-memory SQL export and seeding plus sharper live and WAL-backed database guidance.

- Captured the v0.1.0 release baseline, updated README/BUILD/AGENTS/CHANGELOG around the shipped surface, and re-verified the core quality gates. Verified with: `cargo build`, `cargo test`, `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`. Next: turn the release baseline into an active next-pass plan instead of letting the repo read as finished forever.

### 2026-03-24

- Reframed `BUILD.md` from a release-tombstone tone into an active post-release execution manual, reopened Phase 3 as the live hardening lane, sharpened Phases 4-5 as the next credible build sequence, and aligned `AGENTS.md` plus `CHANGELOG.md` with the same active-roadmap posture. Verified with: doc audit of `BUILD.md`, `README.md`, `AGENTS.md`, and `CHANGELOG.md`, plus a targeted negative-text search confirming the old closed-out status language is gone from those docs. Next: choose whether the next code pass should attack bounded-memory export first or define the CLI contract first.

---

## Decision log

- 2026-03-20: Patchworks remains a GUI-first SQLite tool for now - the shipped product is the desktop app with a small snapshot CLI surface - headless automation work stays explicit future scope instead of implied capability.
- 2026-03-20: Snapshot state lives under `~/.patchworks/` - this keeps user-local metadata and copied snapshot databases outside the repo - snapshot behavior must be documented as local machine state, not project state.
- 2026-03-20: SQL export favors correctness over minimal migration output - modified tables are rebuilt from the right-side schema when needed - exports may be heavier, but semantic fidelity takes priority at this stage.
- 2026-03-20: Views remain inspect-only - this keeps the current scope focused on table-centered diffing and export - any view diff or export support must be added deliberately later.
- 2026-03-20: Sorted pagination appends a primary-key or `rowid` tie-breaker - this preserves deterministic page boundaries across duplicate sort values - the app should prefer stable browsing over a superficially simpler sort implementation.
- 2026-03-20: Indexes and triggers are tracked from `sqlite_master` and preserved in generated SQL - this improves migration fidelity without requiring dedicated UI panels first - schema fidelity can advance ahead of UI completeness.
- 2026-03-20: Diff computation runs on a background thread while inspection later followed the same task-handoff model - this was the lowest-friction responsiveness win already landed - future performance work should preserve the separation between render and database work.
- 2026-03-21: Crate metadata and packaged README content were aligned for crates.io and local packaging - this makes `cargo package --allow-dirty` a recorded success path - future docs changes must keep packaged links and publishability checks honest.
- 2026-03-22: SQL export now prioritizes SQLite foreign-key safety over preserving the original `CREATE TABLE` header text byte-for-byte - schema-changed tables are rebuilt via a temporary replacement table and the generated migration batch guards `PRAGMA foreign_keys` around the operation - inspection normalizes simple table-header quoting so semantic comparisons stay stable even though SQLite rewrites renamed table definitions.
- 2026-03-22: Inspection and visible-table refresh now run on background worker threads coordinated from `src/app.rs` - this keeps `ui/` presentation-focused while allowing stale page-refresh results to be dropped by replacing their receivers - future responsiveness work should build on this task-handoff shape instead of reintroducing synchronous database reads in the render loop.
- 2026-03-22: Explicit cancellation does not belong in the current background-task model yet - the app now reports staged progress and safely supersedes stale work by dropping receivers, but the detached worker threads do not have cooperative cancellation checkpoints across inspection, diff, and export - any future cancel control should wait for a cancellable job abstraction rather than bolting partial interruption onto the current fire-and-forget threads.
- 2026-03-24: Release is the first trustworthy baseline, not the end of the roadmap - BUILD.md, AGENTS.md, and CHANGELOG.md should keep the project readable as active post-v0.1.0 work - future documentation passes must preserve real open work instead of flattening the repo into a finished artifact.
- 2026-03-24: Large-table diffing stays single-threaded at the table level - the streaming merge is I/O-bound against SQLite reads and parallelizing across tables would add ordering and progress-reporting complexity without clear throughput gains - revisit only with profiling evidence of CPU-bound bottlenecks.
- 2026-03-24: SnapshotStore keeps per-operation connections rather than a persistent connection - the snapshot metadata database is tiny and accessed infrequently - a persistent connection would add lifetime complexity without measurable benefit.
- 2026-03-24: Headless CLI subcommands reuse the same backend functions as the GUI rather than implementing separate comparison or export logic - this eliminates the risk of CLI/GUI divergence - integration tests explicitly verify parity between the two surfaces.
- 2026-03-24: Cargo install is the distribution story for now - desktop packaging (DMG, AppImage) is deferred until user demand justifies installer automation - both `cargo install --path .` and `cargo install patchworks` are verified working on macOS arm64.
- 2026-03-24: CI now covers both Linux and macOS via a build matrix - Windows CI is deferred until demand or a contributor appears.
- 2026-03-24: Platform-specific GUI smoke tests are deferred - the headless CLI tests exercise the full backend truth layer and GUI smoke tests would require headless rendering or manual verification - revisit if platform-specific rendering bugs become a pattern.
- 2026-03-24: Views remain inspect-only (reaffirmed decision-0004) - view diffing and export support is not justified by current usage - revisit when users request it or when the diff engine's semantic understanding is stronger.
- 2026-03-24: Indexes and triggers are browsable via the schema browser panel rather than dedicated diff UI panels - the schema diff view already surfaces index/trigger changes - dedicated panels are additional complexity without proven demand.
- 2026-03-24: Error recovery improvements (retry affordances, richer diagnostics) are deferred from Phase 6 - current error display is adequate for the shipped feature set - revisit when user feedback or usage patterns reveal pain points.

### 2026-03-24 (Phase 3 completion)

- Refactored SQL export to a streaming `Write`-based API (`write_export`) that flushes one SQL statement at a time instead of accumulating a `Vec<String>`. The existing `export_diff_as_sql` is now a thin convenience wrapper that collects to `String` for GUI preview. Added `for_each_row` to `inspector.rs` as the bounded-memory alternative to `load_all_rows` — table seeding during export no longer materializes the full table in memory. Added 6 new integration tests: WAL-mode database inspection and diffing, streaming export parity with in-memory export, file-based streaming export round-trip, large-table (5000+ rows) streaming export verification, empty database diff, and table-added-from-empty diff. Documented the live/WAL trust boundary explicitly in README.md. Made two deferred decisions explicit: single-threaded table-level diffing is sufficient (decision-0013), and SnapshotStore keeps per-operation connections (decision-0014). Verified with: `cargo build`, `cargo test` (34 tests), `cargo nextest run` (34 passed), `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo bench --no-run`, `cargo deny check`, `cargo run -- --help`. Next: Phase 4 CLI contract design and Phase 5 platform confidence.

### 2026-03-24 (Phase 4 completion — v0.2.0 release)

- Implemented the full Phase 4 headless CLI surface: `patchworks inspect`, `patchworks diff`, `patchworks export`, and `patchworks snapshot save/list/delete`. All CLI commands share the same backend truth layer as the GUI — `inspect_database`, `diff_databases`, `write_export`, and `SnapshotStore` are called directly, not forked. Added `--format human|json` on inspect, diff, and snapshot list. Added `-o/--output` on export for file output. Defined exit code conventions: 0 = success/no differences, 1 = error, 2 = differences found. Added `src/cli.rs` with 7 unit tests and `tests/cli_tests.rs` with 18 integration tests including CLI/GUI parity proofs. Restructured `main.rs` from flat args to clap subcommands while preserving backward compatibility (`--snapshot <db>` and bare file arguments still work). Added `list_all_snapshots()` and `delete_snapshot()` to `SnapshotStore`. Added `Json` variant to `PatchworksError` for structured output support. Verified with: `cargo build`, `cargo test` (58 tests), `cargo nextest run` (58 passed), `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo bench --no-run`, `cargo deny check`, `cargo run -- --help`. Published as v0.2.0 to crates.io. Next: Phase 5 platform confidence and Phase 6 product polish.

### 2026-03-24 (Phase 5 completion — v0.3.0 release)

- Completed Phase 5: packaging, platform confidence, and release discipline. Verified both `cargo install --path .` and `cargo install patchworks` (from crates.io) on macOS arm64 — both install successfully, binary launches, `--help` and `--version` produce correct output. Added macOS CI build smoke path alongside Linux in `.github/workflows/ci.yml` using a build matrix. Tightened README with dedicated "Operational guidance" section covering live/WAL-mode databases and large database handling. Recorded three decisions: Cargo install is sufficient for now (decision-0016), CI covers Linux + macOS (decision-0017), platform-specific GUI smoke tests are deferred (decision-0018). Normalized git remote to dual-push SSH (GitHub + Codeberg). Verified with: `cargo build`, `cargo test` (58 tests), `cargo clippy --all-targets --all-features -- -D warnings`, `cargo fmt --all --check`, `cargo bench --no-run`, `cargo deny check`, `cargo install --path .`, `cargo install patchworks`. Published as v0.3.0 to crates.io. Next: Phase 6 product polish.

---

### 2026-03-24 (Phase 6 completion — UX polish)

- Completed Phase 6 product polish and UX refinement. Added a dedicated schema browser panel (`src/ui/schema_browser.rs`) showing tables, views, indexes, and triggers with full DDL preview in collapsible sections. Added table name search/filter in file panels. Added keyboard shortcuts: ⌘1-6 for view switching, ⌘D for diff. Added theme support with dark/light/system selector in toolbar. Added recent-files persistence (`src/state/recent.rs`) with quick L/R reopen from a toolbar menu — stores up to 20 files in `~/.patchworks/recent.json`. Refactored diff view with collapsible sections (removed, added, modified each grouped with row counts), added aggregate summary statistics bar, and per-table change indicators in the table selector. Improved schema diff view with summary bar covering index and trigger changes, collapsed unchanged tables by default. Improved file panel to show filename with tooltip, index/trigger counts. Made SQL export preview read-only with line/statement counts. Decided views remain inspect-only (reaffirmed decision-0004). Decided indexes/triggers are browsable via the schema browser rather than dedicated diff panels. Deferred error recovery improvements — current error display is adequate for the shipped feature set. Verified with: `cargo build`, `cargo test` (58 tests), `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo bench --no-run`, `cargo deny check`, `cargo run -- --help`. Next: Phase 7 advanced diff intelligence.

---

*Update this log only with things that actually happened.*
