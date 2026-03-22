# Patchworks Build Plan

Last updated: 2026-03-22
Status: active development, post-MVP hardening
Primary surface: native Rust desktop app via `egui`/`eframe`
Package: crates.io `patchworks` (`0.1.0`)

## Purpose

This file is the canonical execution and handoff document for Patchworks.
Any agent or developer making meaningful changes to code, docs, workflow, or release posture should read it first and update it in the same pass.
`README.md` is the public-facing summary. `AGENTS.md` is secondary project memory. This file is where current truth, verified workflow, risks, and next work are tracked together.

## Mission

Build the definitive SQLite comparison and migration tool — the `git diff` of the database world.

Patchworks exists because there is no trustworthy, purpose-built tool for understanding what changed between two SQLite databases. Not a hex editor. Not a shell script. A real tool — correct, fast, and native — that a developer or operator can point at two database files and immediately understand the delta.

### Long-term vision

Patchworks will evolve from a desktop inspection tool into a complete SQLite lifecycle platform:

1. **Today**: A desktop app that inspects, diffs, snapshots, and exports SQL migrations between SQLite databases.
2. **Near-term**: A headless CLI that makes the same engine scriptable for automation, CI pipelines, and pre-commit hooks.
3. **Mid-term**: An intelligent diff engine with semantic understanding, migration chain management, and conflict resolution.
4. **Long-term**: A plugin-extensible platform with team collaboration features, shared snapshot registries, and deep CI/CD integration.

The through-line: SQLite-specific correctness first. Every feature earns its place by being trustworthy before being powerful.

### Operating principles

- **Correctness over cleverness.** A heavier migration that is semantically correct beats a minimal one that breaks edge cases.
- **SQLite-native.** Preserve the nuance of SQLite (rowid, WITHOUT ROWID, WAL, PRAGMA behavior) instead of flattening into generic database abstractions.
- **Honest scope.** Never describe future work as present capability. BUILD.md tracks what actually works.
- **Desktop-first, automation-ready.** The GUI is the primary surface today, but every backend capability must be usable without a window.
- **Single binary, zero config.** `cargo install patchworks` should be all anyone needs.

## Operating Truth

- Patchworks is GUI-first today. The only shipped CLI workflow beyond app launch is `--snapshot <db>`.
- `BUILD.md` is the source of truth for current verification state, risks, and next-pass priorities.
- `README.md` should stay useful and public-facing, but it should not carry the entire internal handoff burden.
- Best results come from stable SQLite files; live, WAL-backed, encrypted, or actively changing databases remain best-effort territory.
- SQL export currently favors correctness and SQLite-specific safety over minimal migration output.
- Responsiveness work is underway: inspection, visible-table refresh, and diff execution are now backgrounded with staged progress, but explicit cancellation does not exist yet.

## Current Repository Snapshot

### Active root

- `BUILD.md` is the primary execution plan and progress ledger.
- `README.md` is the public product summary and local workflow guide.
- `AGENTS.md` is the concise agent-facing architecture memo.
- `Cargo.toml` is the single-package manifest and dependency source of truth.
- `deny.toml` is the dependency-policy source of truth.
- `.github/workflows/ci.yml` is the checked-in CI entrypoint.
- `src/` contains the active application and library code.
- `tests/` contains integration tests, fixtures, and test support.
- `benches/` contains Criterion benchmark entrypoints.

### Product state today

What Patchworks currently does:

- Opens zero, one, or two SQLite database files in a native desktop UI.
- Inspects tables and views.
- Browses table rows with pagination and sortable columns.
- Computes schema diffs and row diffs between two databases.
- Saves snapshots into a local Patchworks store under `~/.patchworks/`.
- Compares a live database against a saved snapshot.
- Generates SQL intended to transform the left database into the right database.
- Preserves tracked indexes and triggers in generated SQL.
- Applies generated SQL safely even when the destination connection starts with `PRAGMA foreign_keys=ON` by guarding the migration batch and using a temporary-table rebuild path for schema-changed tables.
- Supports a small CLI surface for app launch plus `--snapshot <db>`.

What Patchworks does not currently do:

- It does not expose headless CLI commands for inspect, diff, SQL export, snapshot listing, or snapshot cleanup.
- It does not diff or export views; views are inspect-only.
- It does not yet provide an explicit cancel control for long-running background inspection, table-load, or diff jobs; the current model reports staged progress and supersedes stale requests by dropping their receivers.
- It does not yet stream very large exports; some large operations still materialize significant data in memory.
- It does not yet include installer or release automation beyond normal Cargo packaging expectations.
- It should not be described as strong against heavily changing live databases yet; that is still best-effort, not a hard guarantee.

### Recorded verification baseline

Most recent recorded verification baseline:

- Verified on: 2026-03-22
- Repo path: `/Users/sawyer/github/patchworks`
- Branch: `main`
- Base commit at start of that verification pass: `e91a870e8a9ed3432a0540c90f309c232d3da98c`
- Host used for that verification pass: macOS arm64 (`Darwin 25.4.0`)
- Last full code review recorded in this file: 2026-03-20

### Currently recorded verified commands

Re-verified on 2026-03-22:

- `cargo build`
- `cargo test`
- `cargo nextest run`
- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo bench --no-run`
- `cargo deny check`
- `cargo run -- --help`

Still recorded from the 2026-03-21 packaging pass, but not re-run in the 2026-03-22 diff/export hardening pass:

- `cargo metadata --no-deps --format-version 1`
- `cargo package --allow-dirty --list`
- `cargo package --allow-dirty`
- `cargo run -- --snapshot <db>`

Supported by the codebase but not fully re-verified in the recorded passes above:

- `cargo run`
- `cargo run -- app.db`
- `cargo run -- left.db right.db`
- `cargo install --path .`
- `cargo install patchworks`

## Quick Commands

Use these as the everyday fast path for local work:

```bash
cargo build
cargo test
cargo nextest run
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo bench --no-run
cargo deny check
cargo run -- --help
cargo run -- --snapshot app.db
cargo run -- app.db
cargo run -- left.db right.db
```

## Source Of Truth By Concern

- Package metadata, dependency graph, and crate publishing posture:
  - `Cargo.toml`
- CLI surface and startup behavior:
  - `src/main.rs`
- Top-level app orchestration, background load/task wiring, and UI-to-backend coordination:
  - `src/app.rs`
- Shared error model:
  - `src/error.rs`
- SQLite inspection, paging, and summary loading:
  - `src/db/inspector.rs`
- Diff orchestration and SQL export entrypoints:
  - `src/db/differ.rs`
- Snapshot persistence and local store behavior:
  - `src/db/snapshot.rs`
- Shared database and diff data types:
  - `src/db/types.rs`
- Schema diff rules:
  - `src/diff/schema.rs`
- Row diff rules and invariants:
  - `src/diff/data.rs`
- SQL export generation:
  - `src/diff/export.rs`
- UI-facing workspace state:
  - `src/state/workspace.rs`
- Rendering and interaction surfaces:
  - `src/ui/`
- Executable behavior expectations:
  - `tests/diff_tests.rs`
  - `tests/proptest_invariants.rs`
  - `tests/snapshot_tests.rs`
  - `tests/support/mod.rs`
  - `tests/fixtures/create_fixtures.sql`
- Performance hot-path tracking:
  - `benches/diff_hot_paths.rs`
  - `benches/query_hot_paths.rs`
- CI and dependency policy:
  - `.github/workflows/ci.yml`
  - `deny.toml`
- Public-facing repo narrative:
  - `README.md`

When docs and code disagree, code and tests win. Update docs immediately rather than carrying mismatched narratives.

## Architecture And Runtime Flow

### Module boundaries

- `src/main.rs`
  - Parses CLI arguments.
  - Supports the snapshot CLI path.
  - Otherwise launches the desktop application.
- `src/app.rs`
  - Coordinates file loading, visible table refresh, diff requests, snapshots, and SQL export actions.
  - Owns the background task handoff used to keep database inspection, visible-table refresh, and diff computation off the UI thread.
- `src/db/`
  - Owns SQLite inspection, snapshot persistence, shared backend types, and high-level diff/export orchestration.
- `src/diff/`
  - Owns schema comparison, row comparison, and SQL export generation rules.
- `src/state/`
  - Owns UI-facing workspace state and active selections.
- `src/ui/`
  - Owns `egui` rendering and interaction surfaces.
  - Should stay presentation-focused rather than absorbing backend logic.

### Primary runtime flow

1. `main.rs` parses arguments and decides whether to run snapshot mode or launch the GUI.
2. `app.rs` schedules left and right database inspection plus initial table-page loads through `db::inspector` background workers.
3. UI state in `state::workspace` tracks active databases, selected tables, loading flags, diff mode, and view state.
4. Visible-table selection, sorting, and pagination requests also flow through `db::inspector` background workers.
5. Diff requests flow through `db::differ`, which combines schema diffing, row diffing, and SQL export preparation.
6. Background load and diff results are applied back into app state on later UI update ticks.
7. `ui/*` modules render schema diff, row diff, table browsing, snapshot management, and SQL export views.
8. Snapshot actions persist metadata to `~/.patchworks/patchworks.db` and copied database files under `~/.patchworks/snapshots/`.

### Important current behavior constraints

- Diffing, database inspection, and visible-table refresh now run on background workers with staged progress updates surfaced into UI state.
- Row diffs only run for tables present on both sides.
- Row diff prefers shared primary keys and falls back to table-local row identity (`rowid` when available, otherwise each table's declared primary key) with warnings.
- Sorted pagination adds a deterministic primary-key or `rowid` tie-breaker.
- SQL export favors correctness over minimal, hand-tuned migration output.
- SQL export temporarily disables SQLite foreign-key enforcement for the generated migration batch, rebuilds schema-changed tables through a temporary replacement table, and then restores `PRAGMA foreign_keys=ON`; this keeps export application safe when the caller begins with foreign-key enforcement enabled.
- Trigger handling is intentionally conservative: affected triggers are recreated after migration DML to avoid firing left-side trigger logic during export application.
- Best results still come from stable SQLite files rather than databases undergoing heavy concurrent change.

## Working Rules

1. Read `BUILD.md`, `README.md`, and `AGENTS.md` before making substantial changes.
2. Keep the layering honest:
   - `ui/` renders and handles interaction.
   - `state/` stores UI-facing state.
   - `db/` and `diff/` own data inspection, comparison, snapshots, and export logic.
   - `main.rs` stays thin.
3. Do not claim GUI flows, install flows, or packaging flows are verified unless they were actually exercised and recorded.
4. When behavior changes around snapshots, SQL export, diff correctness, or live-database handling, update this file in the same pass.
5. Prefer streaming and bounded-memory approaches when touching inspection, diff, or export hot paths.
6. Keep `README.md` aligned with recorded reality; do not market future work as present capability.
7. If a task changes user-visible behavior or repo workflow, add a progress-log entry and, when appropriate, a decision-log entry.
8. If new work reveals an architectural or product ambiguity, record it under open decisions instead of leaving it implicit.

## Tracking Conventions

- Each phase has a `Status:` line using `not started`, `in progress`, `done`, or `blocked`.
- Checkboxes mean landed work, not intention.
- The progress log is append-only. Preserve historical verification context instead of rewriting it into a cleaner fiction.
- Decision-log entries capture choices that affect future implementation or user expectations.
- If a command was not actually run, do not imply that it was.
- If a command was verified in an earlier pass but not re-run, say that explicitly.

### Progress log format

- `YYYY-MM-DD: scope - outcome. Verified with: <commands or audit summary>. Next: <follow-up>.`

### Decision log format

- `YYYY-MM-DD: decision - rationale - consequence.`

## Quality Gates

### Recorded current standard gate

The strongest recorded local gate in this repo today is:

- `cargo build`
- `cargo test`
- `cargo nextest run`
- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo bench --no-run`
- `cargo deny check`
- `cargo run -- --help`

### Additional scenario-specific checks

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

Do not record these as passed unless they were actually run in the repo.

## Phase Dashboard

- Phase 0 - Repo baseline, workflow, and packaging truth. Status: done.
- Phase 1 - Desktop inspection, diff, snapshot, and export MVP. Status: done.
- Phase 2 - Schema-object fidelity and quality rails. Status: done.
- Phase 3 - Responsiveness and large-database hardening. Status: in progress.
- Phase 4 - Headless CLI and automation surface. Status: not started.
- Phase 5 - Packaging, platform confidence, and release discipline. Status: not started.
- Phase 6 - Product polish and UX refinement. Status: not started.
- Phase 7 - Advanced diff intelligence. Status: not started.
- Phase 8 - Migration workflow management. Status: not started.
- Phase 9 - Plugin and extension architecture. Status: not started.
- Phase 10 - Team features and shared snapshot registries. Status: not started.
- Phase 11 - CI/CD integration and automation ecosystem. Status: not started.
- Phase 12 - Long-term platform evolution. Status: not started.

## Detailed Phase Plan

### Phase 0 - Repo baseline, workflow, and packaging truth

Status: done

- [x] Establish the single-crate Rust package as the repo's active implementation surface.
- [x] Record the local build, test, lint, benchmark-compile, and dependency-policy workflow.
- [x] Add checked-in CI entrypoints and `cargo-deny` policy.
- [x] Align crate metadata for crates.io and docs.rs.
- [x] Ensure the packaged README uses a relative `LICENSE` link and passes local package validation.

Exit criteria:

- [x] A contributor can see the real package/workflow surface from the root docs.
- [x] `cargo package --allow-dirty` is recorded as successful in the repo history.

### Phase 1 - Desktop inspection, diff, snapshot, and export MVP

Status: done

- [x] Support empty launch, one-file inspect, two-file diff, and snapshot CLI startup paths.
- [x] Inspect SQLite tables and views in the desktop UI.
- [x] Browse table rows with pagination and sortable columns.
- [x] Compute schema-level diffs.
- [x] Compute row-level diffs for shared tables.
- [x] Save and compare snapshots through the local Patchworks store.
- [x] Generate SQL export, preview it in the UI, copy it, and save it to disk.

Exit criteria:

- [x] Patchworks is useful as a desktop inspection and comparison tool for normal local workflows.
- [x] Snapshot creation and SQL export are part of the actual product, not just planned scope.

### Phase 2 - Schema-object fidelity and quality rails

Status: done

- [x] Track indexes and triggers from `sqlite_master` through inspection and export planning.
- [x] Recreate changed indexes from the right-side schema during export generation.
- [x] Drop and recreate affected triggers around migration DML so left-side trigger logic is not accidentally executed.
- [x] Add deterministic sorted-pagination tie-breakers using primary-key or `rowid` fallback.
- [x] Add integration tests for schema diff, data diff, rowid fallback, snapshot behavior, and SQL export behavior.
- [x] Add property tests for schema and row-diff invariants.
- [x] Add Criterion benchmark entrypoints for query and diff hot paths.
- [x] Add background diff task coordination tests in `src/app.rs`.

Exit criteria:

- [x] The repo has meaningful executable coverage for core diff and export behavior.
- [x] Generated SQL preserves more of the actual SQLite schema story than table-only export logic.

### Phase 3 - Responsiveness and large-database hardening

Status: in progress

- [x] Move database inspection off the UI thread.
- [x] Move table-page refresh work off the UI thread or otherwise bound its impact on interactivity.
- [x] Add progress reporting for long-running background inspection, table-load, and diff jobs.
- [x] Decide whether explicit diff cancellation belongs in the current architecture.
- [ ] Refactor SQL export away from one giant in-memory `String` for very large migrations.
- [ ] Reduce or eliminate full-table materialization during export seeding where practical.
- [ ] Add regression coverage for large databases and live or WAL-backed cases.
- [ ] Decide whether `SnapshotStore` should reuse a persistent SQLite connection.

Exit criteria:

- [ ] Large-file inspection and diff paths no longer feel frozen even when work is expensive.
- [ ] Export memory growth is materially better bounded than it is today.
- [ ] Known live-database caveats are either handled better or documented with sharper precision.

### Phase 4 - Headless CLI and automation surface

Status: not started

- [ ] Add a headless inspect command.
- [ ] Add a headless diff command.
- [ ] Add a headless SQL export command.
- [ ] Add snapshot listing and cleanup commands.
- [ ] Decide whether machine-readable output formats belong in the first CLI expansion.
- [ ] Keep the CLI on the same backend logic rather than forking separate comparison code.

Exit criteria:

- [ ] Patchworks can participate in scripted workflows, not only interactive desktop sessions.
- [ ] CLI and GUI paths share the same truth layer for diff and export behavior.

### Phase 5 - Packaging, platform confidence, and release discipline

Status: not started

- [ ] Re-verify `cargo install --path .` or `cargo install patchworks` explicitly and record the result.
- [ ] Add at least one macOS CI build smoke path in addition to Linux.
- [ ] Decide whether the project needs release archives, installers, or desktop packaging beyond Cargo install.
- [ ] Tighten README and BUILD guidance around live databases, WAL mode, and other operational caveats.
- [ ] Decide what counts as the first release-quality support bar.

Exit criteria:

- [ ] Installation expectations are documented from actual verification, not assumption.
- [ ] CI covers the platforms most likely to matter for a desktop SQLite tool.

### Phase 6 - Product polish and UX refinement

Status: not started

- [ ] Decide whether views should stay inspect-only or gain diff or export support.
- [ ] Decide whether indexes and triggers need dedicated UI panels instead of export-only preservation.
- [ ] Add a dedicated schema browser panel (tree view of tables, views, indexes, triggers with DDL preview).
- [ ] Add search/filter across table names, column names, and row data.
- [ ] Refine the diff UX: syntax-highlighted SQL, collapsible sections, jump-to-change navigation.
- [ ] Add keyboard shortcuts for core workflows (open file, switch panes, trigger diff, copy export).
- [ ] Add a theme system (light/dark at minimum; respect system preference).
- [ ] Add a recent-files list or workspace memory so users can quickly reopen previous sessions.
- [ ] Reassess whether the current CLI surface and GUI affordances match the product's real audience.
- [ ] Add user-facing error recovery: clear error states, retry affordances, diagnostic info on failure.

Exit criteria:

- [ ] The app feels like a polished tool, not a prototype.
- [ ] Any scope expansion is deliberate and documented rather than accidental drift.
- [ ] Product polish work follows proven correctness and performance improvements instead of masking unfinished core behavior.

### Phase 7 - Advanced diff intelligence

Status: not started

This phase moves Patchworks from "shows you what changed" to "helps you understand why it changed and what to do about it."

- [ ] Add column-level change highlighting within modified rows (cell-level diff, not just row-level).
- [ ] Add diff filtering: show only additions, only deletions, only modifications, or filter by table.
- [ ] Add diff statistics dashboard: summary counts, change heatmap by table, largest deltas.
- [ ] Add semantic diff awareness: detect column renames (vs. drop+add), table renames, and column type changes that preserve data.
- [ ] Add conflict detection: identify rows modified in both databases relative to a common ancestor snapshot.
- [ ] Add three-way merge support: given a base snapshot and two diverged databases, identify conflicts and produce a merged migration.
- [ ] Add diff annotations: let users mark changes as "expected", "investigate", or "reject" for triage workflows.
- [ ] Add data-type-aware comparison: understand that `INTEGER` vs `INT` is cosmetic, `TEXT` vs `BLOB` is semantic.

Exit criteria:

- [ ] Patchworks provides actionable intelligence about changes, not just raw deltas.
- [ ] Three-way merge works correctly for non-conflicting changes and clearly surfaces conflicts for manual resolution.

### Phase 8 - Migration workflow management

Status: not started

This phase turns Patchworks from a one-shot diff tool into a migration lifecycle manager.

- [ ] Add migration chain support: generate, store, and replay ordered migration sequences.
- [ ] Add migration validation: dry-run a generated migration against a copy of the source database and verify the result matches the target.
- [ ] Add migration rollback generation: produce a reverse migration alongside the forward migration.
- [ ] Add migration squashing: combine multiple sequential migrations into a single equivalent migration.
- [ ] Add migration history tracking: store which migrations have been applied to which databases (tracked in the Patchworks store).
- [ ] Add migration conflict detection: warn when two migrations target the same table or when ordering matters.
- [ ] Add a `patchworks migrate` CLI command that applies a migration file to a database with safety checks.
- [ ] Add `--dry-run` mode for all migration operations.
- [ ] Add migration templates: let users define custom pre/post migration hooks (e.g., "always run VACUUM after migration").

Exit criteria:

- [ ] Users can manage a sequence of database migrations through Patchworks rather than ad-hoc SQL files.
- [ ] Every migration can be validated before application and rolled back after.
- [ ] Migration state is tracked persistently, not just in-memory.

### Phase 9 - Plugin and extension architecture

Status: not started

This phase opens Patchworks to community extension without compromising core correctness.

- [ ] Design and implement a plugin trait/interface for custom diff formatters (e.g., HTML report, Markdown, JSON).
- [ ] Design and implement a plugin trait for custom export targets (e.g., Alembic-style Python migrations, Flyway SQL, Liquibase XML).
- [ ] Design and implement a plugin trait for custom inspectors (e.g., application-specific table semantics, data validation rules).
- [ ] Add a plugin discovery and loading mechanism (compiled Rust plugins via dynamic linking, or WASM for sandboxed extensions).
- [ ] Add a built-in JSON diff output plugin as the reference implementation.
- [ ] Add a built-in HTML report plugin that generates a standalone diff report.
- [ ] Add a built-in Markdown report plugin for embedding diffs in documentation or PRs.
- [ ] Define plugin API stability guarantees and versioning.
- [ ] Add plugin documentation and a plugin development guide.

Exit criteria:

- [ ] Third-party developers can extend Patchworks output formats and inspection logic without forking.
- [ ] The plugin API has a stability contract and documentation.
- [ ] At least three built-in plugins demonstrate the architecture (JSON, HTML, Markdown).

### Phase 10 - Team features and shared snapshot registries

Status: not started

This phase enables Patchworks to work across a team, not just on one developer's machine.

- [ ] Design a snapshot registry protocol: push/pull snapshots to/from a shared store (local directory, S3, or custom backend).
- [ ] Add `patchworks push` and `patchworks pull` commands for snapshot exchange.
- [ ] Add snapshot naming, tagging, and annotation (beyond UUID-based storage).
- [ ] Add snapshot comparison across machines: "show me what changed since the last snapshot anyone on the team took."
- [ ] Add snapshot retention policies: auto-prune snapshots older than N days or exceeding N total.
- [ ] Add snapshot integrity verification: checksums and schema fingerprints to detect corruption or tampering.
- [ ] Add a simple built-in snapshot server (optional, for teams that want a lightweight central registry without S3).
- [ ] Add access control primitives for shared registries (read-only vs. read-write tokens).

Exit criteria:

- [ ] A team of developers can share database snapshots through a common registry.
- [ ] Snapshot integrity is verifiable end-to-end.
- [ ] The shared registry is optional — Patchworks remains fully functional as a local-only tool.

### Phase 11 - CI/CD integration and automation ecosystem

Status: not started

This phase makes Patchworks a first-class citizen in automated pipelines.

- [ ] Add a `patchworks check` command: exit 0 if two databases are identical, exit 1 with a summary if they differ (designed for CI gates).
- [ ] Add GitHub Actions integration: publish a `dunamismax/patchworks-action` that installs Patchworks and runs diff checks.
- [ ] Add pre-commit hook support: validate that a database migration file correctly transforms source to target.
- [ ] Add machine-readable output formats: JSON and JSONL for all CLI commands, suitable for piping into `jq` or downstream tools.
- [ ] Add `--format` flag across all CLI commands (human, json, jsonl).
- [ ] Add exit code conventions: document and stabilize exit codes for scripting (0 = identical, 1 = differences found, 2 = error).
- [ ] Add a `patchworks watch` mode: monitor a database file for changes and trigger a diff against a baseline on every write.
- [ ] Add webhook/notification support: post diff summaries to Slack, Discord, or arbitrary HTTP endpoints.
- [ ] Add GitOps workflow documentation: how to version-control SQLite databases and use Patchworks in a GitOps pipeline.

Exit criteria:

- [ ] Patchworks can be dropped into a CI pipeline with a single `patchworks check` command.
- [ ] Machine-readable output is stable and documented.
- [ ] At least one real GitHub Actions workflow demonstrates the integration.

### Phase 12 - Long-term platform evolution

Status: not started

This phase tracks longer-horizon capabilities that may reshape the product.

- [ ] Evaluate multi-engine support: could the diff engine work against DuckDB, libSQL, or other embedded databases?
- [ ] Evaluate embedded scripting: let users write custom diff rules or migration transforms in Lua, Rhai, or WASM.
- [ ] Evaluate a TUI (terminal UI) mode as a middle ground between full GUI and raw CLI.
- [ ] Evaluate remote database support: diff databases accessible over SSH, HTTP, or cloud storage without downloading full copies.
- [ ] Evaluate real-time collaboration: multiple users viewing and annotating the same diff session.
- [ ] Evaluate database schema visualization: ERD generation from inspected schema.
- [ ] Evaluate integration with existing migration frameworks (Diesel, SQLx, Alembic, Flyway) as import/export targets.
- [ ] Evaluate performance profiling integration: not just "what changed" but "what changed that might affect query performance."

Exit criteria:

- [ ] Each evaluation produces a decision document (build, defer, or reject) with rationale.
- [ ] Any accepted capability follows the same phase-gated development discipline as earlier work.
- [ ] Platform evolution does not compromise SQLite-first correctness.

## Open Decisions And Unresolved Scope

### Near-term (Phases 3-5)

- Should the next major investment go first into async loading and performance, or into a headless CLI surface that makes the existing diff engine scriptable?
- How far should Patchworks go on live or WAL-backed database guarantees before the docs promise more than best-effort behavior?
- Does large-export hardening require a streaming writer API that changes current UI and export plumbing, or can the current shape be improved incrementally?
- Should `SnapshotStore` remain intentionally simple with one connection per operation, or is a persistent connection worth the complexity once UI-thread loading is addressed?
- Is the first release-quality support bar Cargo install only, or does the project need stronger desktop packaging before it should be presented as broadly ready?

### Mid-term (Phases 6-9)

- Should the plugin system use compiled Rust dynamic libraries, WASM sandboxed extensions, or both?
- Should three-way merge be a core feature or a plugin?
- How should migration chain state be stored — in the Patchworks metadata DB, in a separate migration-specific store, or alongside the target database?
- Should the theme system use egui's built-in theming or a custom abstraction?

### Long-term (Phases 10-12)

- Is multi-engine support worth the abstraction cost, or should Patchworks stay SQLite-only and do it better than anyone?
- Should the shared snapshot registry be a separate service or embedded in the Patchworks binary?
- Does real-time collaboration belong in a desktop tool, or should that be a separate web-based product?
- Should Patchworks maintain its own migration format, or should it target existing framework formats (Diesel, SQLx, etc.) as first-class output?

## Risk Register

### Active risks (current phase)

- Background inspection, table loading, and diff execution now report coarse staged progress, but they still do not support cooperative interruption or a user-facing cancel control.
- SQL export still builds one large in-memory payload and can materialize large tables during seeding.
- Live databases, WAL-backed databases, encrypted databases, and other actively changing sources are still only best-effort.
- CI is currently Linux-only even though this is a desktop app and likely macOS users matter.
- Snapshot matching depends on canonicalized paths and can behave awkwardly if files move.
- The current lightweight UI is usable, but product polish can easily outrun underlying scale and correctness work if the order slips.

### Strategic risks (long-term)

- Feature breadth expanding faster than test coverage could erode the correctness trust that defines Patchworks.
- A plugin system introduces API stability obligations that constrain future refactoring.
- Shared snapshot registries introduce network, auth, and security concerns that are absent in the local-only model.
- Multi-engine support (Phase 12) could dilute SQLite-specific correctness if abstraction boundaries are drawn wrong.
- The diff engine is currently synchronous and single-threaded at the table level; parallelizing across tables will be needed for large multi-table diffs but introduces ordering and progress-reporting complexity.

## Immediate Next Moves

1. Rework SQL export and seeding toward bounded-memory behavior for large databases.
2. Add sharper tests and docs for live or WAL-backed database behavior so the product's trust boundary is explicit.
3. Decide whether headless CLI work starts before or after the next responsiveness hardening slice lands.
4. Add a macOS CI build smoke path once the current hardening priorities are underway.
5. Revisit explicit cancellation only if the background-task model grows cooperative checkpoints or a cancellable job runner.
6. Begin designing the CLI command structure for Phase 4 (subcommand layout, output format conventions, exit codes).
7. Evaluate `clap` subcommand architecture for `patchworks inspect`, `patchworks diff`, `patchworks export`, `patchworks snapshot`.

## Progress Log

- 2026-03-20: Completed a full repository review and recorded the local workflow baseline for build, lint, test, bench-compile, dependency policy, and CLI help behavior. Verified with: `cargo build`, `cargo test`, `cargo nextest run`, `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo bench --no-run`, `cargo deny check`, `cargo run -- --help`. Next: tighten diff and export correctness and document the real product limits honestly.
- 2026-03-20: Implemented index and trigger inspection, carried those schema objects through diff and export planning, added deterministic sorted-pagination tie-breakers, and landed regression coverage for schema-object preservation and trigger-safe export behavior. Verified with: `cargo test --test diff_tests`, `cargo fmt --all --check`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`. Next: keep responsiveness, large-export memory use, and live-database caveats as the main follow-up priorities.
- 2026-03-20: Re-reviewed the full repository after the follow-up implementation and confirmed the strongest remaining risks were synchronous inspection, unbounded export memory growth, Linux-only CI, simple snapshot-store connection management, and best-effort handling of live or WAL-backed databases. Verified with: `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, plus source review across `src/`, `tests/`, `.github/workflows/ci.yml`, and `deny.toml`. Next: prioritize async loading and streaming export design before feature breadth.
- 2026-03-21: Added crates.io-facing metadata in `Cargo.toml`, updated `README.md` for packaged-readme correctness and local publishability guidance, and recorded the successful local package validation path. Verified with: `cargo metadata --no-deps --format-version 1`, `cargo package --allow-dirty --list`, `cargo package --allow-dirty`, `cargo test`. Next: keep BUILD, README, and future release workflow aligned with the actual package and install story.
- 2026-03-21: Reframed `BUILD.md` into a phase-based execution plan with clearer source-of-truth mapping, architecture flow, quality gates, risks, and next moves while preserving recorded verification history. Verified with: repo document and source-tree audit of `BUILD.md`, `README.md`, `AGENTS.md`, `Cargo.toml`, `src/`, `tests/`, and `benches/`. Next: update this plan in lockstep with the next real code or verification pass.
- 2026-03-22: Hardened row diff and SQL export around SQLite edge cases: row diff no longer assumes `rowid` exists when shared primary keys diverge, SQL export now uses a temporary-table rebuild path plus foreign-key guarding so exports apply cleanly when `PRAGMA foreign_keys=ON`, and added regression coverage for WITHOUT ROWID fallback and FK-enforced export application. Verified with: `cargo test --test diff_tests`, `cargo fmt --all --check`, `cargo build`, `cargo test`, `cargo nextest run`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo bench --no-run`, `cargo deny check`, `cargo run -- --help`. Next: keep Phase 3 focused on async inspection and page loading, explicit progress reporting, and bounded-memory export generation.
- 2026-03-22: Moved database inspection and visible-table refresh onto background workers, added pane and table loading state in the UI, and landed app-level regression coverage for background load application and stale table-refresh dropping. Verified with: `cargo test --lib`, `cargo test`, `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings`. Next: add progress reporting and decide whether explicit cancellation belongs in the current background-task architecture.
- 2026-03-22: Added staged progress reporting for background database opens, visible-table refreshes, and diff computation; documented that stale jobs are superseded rather than explicitly cancelled; and landed regression coverage for emitted diff progress plus worker-to-UI progress application. Verified with: `cargo test --lib`, `cargo fmt --all --check`, `cargo test`, `cargo nextest run`, `cargo clippy --all-targets --all-features -- -D warnings`. Next: keep Phase 3 focused on bounded-memory SQL export and seeding plus sharper live and WAL-backed database guidance.

## Decision Log

- 2026-03-20: Patchworks remains a GUI-first SQLite tool for now - the shipped product is the desktop app with a small snapshot CLI surface - headless automation work stays explicit future scope instead of implied capability.
- 2026-03-20: Snapshot state lives under `~/.patchworks/` - this keeps user-local metadata and copied snapshot databases outside the repo - snapshot behavior must be documented as local machine state, not project state.
- 2026-03-20: SQL export favors correctness over minimal migration output - modified tables are rebuilt from the right-side schema when needed - exports may be heavier, but semantic fidelity takes priority at this stage.
- 2026-03-20: Views remain inspect-only - this keeps the current scope focused on table-centered diffing and export - any view diff or export support must be added deliberately later.
- 2026-03-20: Sorted pagination appends a primary-key or `rowid` tie-breaker - this preserves deterministic page boundaries across duplicate sort values - the app should prefer stable browsing over a superficially simpler sort implementation.
- 2026-03-20: Indexes and triggers are tracked from `sqlite_master` and preserved in generated SQL - this improves migration fidelity without requiring dedicated UI panels first - schema fidelity can advance ahead of UI completeness.
- 2026-03-20: Diff computation runs on a background thread while inspection still remains synchronous - this was the lowest-friction responsiveness win already landed - further UI-thread loading work was still required and remained active scope.
- 2026-03-21: Crate metadata and packaged README content were aligned for crates.io and local packaging - this makes `cargo package --allow-dirty` a recorded success path - future docs changes must keep packaged links and publishability checks honest.
- 2026-03-22: SQL export now prioritizes SQLite foreign-key safety over preserving the original `CREATE TABLE` header text byte-for-byte - schema-changed tables are rebuilt via a temporary replacement table and the generated migration batch guards `PRAGMA foreign_keys` around the operation - inspection normalizes simple table-header quoting so semantic comparisons stay stable even though SQLite rewrites renamed table definitions.
- 2026-03-22: Inspection and visible-table refresh now run on background worker threads coordinated from `src/app.rs` - this keeps `ui/` presentation-focused while allowing stale page-refresh results to be dropped by replacing their receivers - future responsiveness work should build on this task-handoff shape instead of reintroducing synchronous database reads in the render loop.
- 2026-03-22: Explicit cancellation does not belong in the current background-task model yet - the app now reports staged progress and safely supersedes stale work by dropping receivers, but the detached worker threads do not have cooperative cancellation checkpoints across inspection, diff, and export - any future cancel control should wait for a cancellable job abstraction rather than bolting partial interruption onto the current fire-and-forget threads.
