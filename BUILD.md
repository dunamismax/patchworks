# BUILD.md

## Purpose

This file is the execution manual for `patchworks`.

It keeps the repo honest while the project grows from its post-MVP state into a complete SQLite lifecycle platform. At any point it should answer:

- what patchworks is trying to become
- what exists right now
- what is explicitly not built yet
- what the next correct move is
- what must be proven before stronger claims are made

This is a living document. When code and docs disagree, fix them together in the same change.

---

## Mission

Build the definitive SQLite comparison and migration tool — the `git diff` of the database world.

Patchworks exists because there is no trustworthy, purpose-built tool for understanding what changed between two SQLite databases. Not a hex editor. Not a shell script. A real tool — correct, fast, and native — that a developer or operator can point at two database files and immediately understand the delta.

### Long-term vision

1. **Today**: A desktop app that inspects, diffs, snapshots, and exports SQL migrations between SQLite databases.
2. **Near-term**: A headless CLI that makes the same engine scriptable for automation, CI pipelines, and pre-commit hooks.
3. **Mid-term**: An intelligent diff engine with semantic understanding, migration chain management, and conflict resolution.
4. **Long-term**: A plugin-extensible platform with team collaboration features, shared snapshot registries, and deep CI/CD integration.

The through-line: SQLite-specific correctness first. Every feature earns its place by being trustworthy before being powerful.

---

## Repo snapshot

**Current phase: Phase 3 — responsiveness and large-database hardening**

**Package:** crates.io [`patchworks`](https://crates.io/crates/patchworks) (`0.1.0`)
**Primary surface:** native Rust desktop app via `egui`/`eframe`

What exists:

- Opens zero, one, or two SQLite database files in a native desktop UI
- Inspects tables and views with row browsing, pagination, and sortable columns
- Computes schema diffs and row diffs between two databases
- Saves snapshots into a local Patchworks store under `~/.patchworks/`
- Compares a live database against a saved snapshot
- Generates SQL export that transforms left database into right database
- Preserves tracked indexes and triggers in generated SQL
- Guards `PRAGMA foreign_keys` and uses temporary-table rebuild for schema-changed tables
- Runs inspection, table loading, and diffing on background threads with staged progress
- Small CLI surface for app launch plus `--snapshot <db>`

What does **not** exist yet:

- Headless CLI commands for inspect, diff, SQL export, snapshot listing, or snapshot cleanup
- View diffing or export (views are inspect-only)
- Explicit cancel control for long-running background jobs
- Streaming export for very large migrations (some operations still materialize significant data in memory)
- Installer or release automation beyond Cargo packaging
- Guarantees for heavily changing live databases (still best-effort)

### Recorded verification baseline

- Verified on: 2026-03-22
- Repo path: `/Users/sawyer/github/patchworks`
- Branch: `main`
- Base commit: `e91a870e8a9ed3432a0540c90f309c232d3da98c`
- Host: macOS arm64 (`Darwin 25.4.0`)
- Last full code review: 2026-03-20

---

## Source-of-truth mapping

| File | Owns |
|------|------|
| `README.md` | Public-facing project description, honest status |
| `BUILD.md` | Implementation map, phase tracking, decisions, working rules |
| `AGENTS.md` | Agent-facing architecture memo |
| `ARCHITECTURE.md` | Deep architectural documentation |
| `CONTRIBUTING.md` | Setup, conventions, how to contribute |
| `CHANGELOG.md` | Release history |
| `Cargo.toml` | Package manifest, dependency graph, crate publishing posture |
| `deny.toml` | Dependency policy |
| `.github/workflows/ci.yml` | CI entrypoint |
| `src/main.rs` | CLI surface and startup behavior |
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
| `src/ui/` | Rendering and interaction surfaces |
| `tests/diff_tests.rs` | Diff and export behavior expectations |
| `tests/proptest_invariants.rs` | Property-based invariant checks |
| `tests/snapshot_tests.rs` | Snapshot behavior expectations |
| `benches/diff_hot_paths.rs` | Diff performance tracking |
| `benches/query_hot_paths.rs` | Query performance tracking |

**Invariant:** If docs, code, and CLI output ever disagree, the next change must reconcile all three. When docs and code disagree, code and tests win — update docs immediately rather than carrying mismatched narratives.

---

## Working rules

1. **Read BUILD.md, README.md, and AGENTS.md** before making substantial changes.
2. **Keep the layering honest.** `ui/` renders and handles interaction. `state/` stores UI-facing state. `db/` and `diff/` own data inspection, comparison, snapshots, and export logic. `main.rs` stays thin.
3. **Do not claim flows are verified** unless they were actually exercised and recorded.
4. **When behavior changes** around snapshots, SQL export, diff correctness, or live-database handling, update this file in the same pass.
5. **Prefer streaming and bounded-memory approaches** when touching inspection, diff, or export hot paths.
6. **Keep README.md aligned with recorded reality.** Do not market future work as present capability.
7. **If a task changes user-visible behavior or repo workflow,** add a progress-log entry and, when appropriate, a decision-log entry.
8. **If new work reveals an architectural or product ambiguity,** record it under open decisions instead of leaving it implicit.
9. **Correctness over cleverness.** A heavier migration that is semantically correct beats a minimal one that breaks edge cases.
10. **SQLite-native.** Preserve the nuance of SQLite (rowid, WITHOUT ROWID, WAL, PRAGMA behavior) instead of flattening into generic database abstractions.

---

## Tracking conventions

Use this language consistently in docs, commits, and issues:

| Term | Meaning |
|------|---------|
| **done** | Implemented and verified |
| **checked** | Verified by command or test output |
| **in progress** | Actively being worked on |
| **not started** | Intentional, not started |
| **blocked** | Cannot proceed without a decision or dependency |
| **risk** | Plausible failure mode that could distort the design |
| **decision** | A durable call with consequences |

Checkboxes mean landed work, not intention. The progress log is append-only. Preserve historical verification context instead of rewriting it into a cleaner fiction.

When new work lands, update: repo snapshot, phase dashboard, decisions (if architecture changed), and progress log with date and what was verified.

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

---

## Dependency strategy

Patchworks is a native Rust application. Dependencies are managed through `Cargo.toml` with policy enforcement via `cargo-deny` (`deny.toml`).

**Principles:**

- Every dependency must justify itself against implementing the functionality directly
- `cargo deny check` must pass as part of the standard quality gate
- Development-only dependencies stay in `[dev-dependencies]`
- The `rusqlite` `bundled` feature is used to ship SQLite without requiring a system install

---

## Phase dashboard

| Phase | Name | Status |
|-------|------|--------|
| 0 | Repo baseline, workflow, and packaging truth | **Done** |
| 1 | Desktop inspection, diff, snapshot, and export MVP | **Done** |
| 2 | Schema-object fidelity and quality rails | **Done** |
| 3 | Responsiveness and large-database hardening | **In progress** |
| 4 | Headless CLI and automation surface | Not started |
| 5 | Packaging, platform confidence, and release discipline | Not started |
| 6 | Product polish and UX refinement | Not started |
| 7 | Advanced diff intelligence | Not started |
| 8 | Migration workflow management | Not started |
| 9 | Plugin and extension architecture | Not started |
| 10 | Team features and shared snapshot registries | Not started |
| 11 | CI/CD integration and automation ecosystem | Not started |
| 12 | Long-term platform evolution | Not started |

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
- [x] `cargo package --allow-dirty` is recorded as successful in the repo history

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
- [x] Snapshot creation and SQL export are part of the actual product, not just planned scope

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
**Status: in progress**

Goals:
- [x] Move database inspection off the UI thread
- [x] Move table-page refresh work off the UI thread or otherwise bound its impact on interactivity
- [x] Add progress reporting for long-running background inspection, table-load, and diff jobs
- [x] Decide whether explicit diff cancellation belongs in the current architecture
- [ ] Refactor SQL export away from one giant in-memory `String` for very large migrations
- [ ] Reduce or eliminate full-table materialization during export seeding where practical
- [ ] Add regression coverage for large databases and live or WAL-backed cases
- [ ] Decide whether `SnapshotStore` should reuse a persistent SQLite connection

Exit criteria:
- [ ] Large-file inspection and diff paths no longer feel frozen even when work is expensive
- [ ] Export memory growth is materially better bounded than it is today
- [ ] Known live-database caveats are either handled better or documented with sharper precision

Risks:
- **risk:** SQL export still builds one large in-memory payload and can materialize large tables during seeding
- **risk:** live databases, WAL-backed databases, and actively changing sources are still only best-effort
- **risk:** background workers do not yet support cooperative interruption or user-facing cancel

---

### Phase 4 — Headless CLI and automation surface
**Status: not started**

Goals:
- [ ] Add a headless inspect command
- [ ] Add a headless diff command
- [ ] Add a headless SQL export command
- [ ] Add snapshot listing and cleanup commands
- [ ] Decide whether machine-readable output formats belong in the first CLI expansion
- [ ] Keep the CLI on the same backend logic rather than forking separate comparison code

Exit criteria:
- [ ] Patchworks can participate in scripted workflows, not only interactive desktop sessions
- [ ] CLI and GUI paths share the same truth layer for diff and export behavior

Risks:
- **risk:** CLI and GUI diverging on diff behavior if backend is forked rather than shared

---

### Phase 5 — Packaging, platform confidence, and release discipline
**Status: not started**

Goals:
- [ ] Re-verify `cargo install --path .` or `cargo install patchworks` explicitly and record the result
- [ ] Add at least one macOS CI build smoke path in addition to Linux
- [ ] Decide whether the project needs release archives, installers, or desktop packaging beyond Cargo install
- [ ] Tighten README and BUILD guidance around live databases, WAL mode, and other operational caveats
- [ ] Decide what counts as the first release-quality support bar

Exit criteria:
- [ ] Installation expectations are documented from actual verification, not assumption
- [ ] CI covers the platforms most likely to matter for a desktop SQLite tool

---

### Phase 6 — Product polish and UX refinement
**Status: not started**

Goals:
- [ ] Decide whether views should stay inspect-only or gain diff or export support
- [ ] Decide whether indexes and triggers need dedicated UI panels instead of export-only preservation
- [ ] Add a dedicated schema browser panel (tree view of tables, views, indexes, triggers with DDL preview)
- [ ] Add search/filter across table names, column names, and row data
- [ ] Refine the diff UX: syntax-highlighted SQL, collapsible sections, jump-to-change navigation
- [ ] Add keyboard shortcuts for core workflows (open file, switch panes, trigger diff, copy export)
- [ ] Add a theme system (light/dark at minimum; respect system preference)
- [ ] Add a recent-files list or workspace memory so users can quickly reopen previous sessions
- [ ] Reassess whether the current CLI surface and GUI affordances match the product's real audience
- [ ] Add user-facing error recovery: clear error states, retry affordances, diagnostic info on failure

Exit criteria:
- [ ] The app feels like a polished tool, not a prototype
- [ ] Any scope expansion is deliberate and documented rather than accidental drift
- [ ] Product polish work follows proven correctness and performance improvements instead of masking unfinished core behavior

---

### Phase 7 — Advanced diff intelligence
**Status: not started**

Goals:
- [ ] Add column-level change highlighting within modified rows (cell-level diff, not just row-level)
- [ ] Add diff filtering: show only additions, only deletions, only modifications, or filter by table
- [ ] Add diff statistics dashboard: summary counts, change heatmap by table, largest deltas
- [ ] Add semantic diff awareness: detect column renames (vs. drop+add), table renames, and column type changes that preserve data
- [ ] Add conflict detection: identify rows modified in both databases relative to a common ancestor snapshot
- [ ] Add three-way merge support: given a base snapshot and two diverged databases, identify conflicts and produce a merged migration
- [ ] Add diff annotations: let users mark changes as "expected", "investigate", or "reject" for triage workflows
- [ ] Add data-type-aware comparison: understand that `INTEGER` vs `INT` is cosmetic, `TEXT` vs `BLOB` is semantic

Exit criteria:
- [ ] Patchworks provides actionable intelligence about changes, not just raw deltas
- [ ] Three-way merge works correctly for non-conflicting changes and clearly surfaces conflicts for manual resolution

---

### Phase 8 — Migration workflow management
**Status: not started**

Goals:
- [ ] Add migration chain support: generate, store, and replay ordered migration sequences
- [ ] Add migration validation: dry-run a generated migration against a copy of the source database and verify the result matches the target
- [ ] Add migration rollback generation: produce a reverse migration alongside the forward migration
- [ ] Add migration squashing: combine multiple sequential migrations into a single equivalent migration
- [ ] Add migration history tracking: store which migrations have been applied to which databases (tracked in the Patchworks store)
- [ ] Add migration conflict detection: warn when two migrations target the same table or when ordering matters
- [ ] Add a `patchworks migrate` CLI command that applies a migration file to a database with safety checks
- [ ] Add `--dry-run` mode for all migration operations
- [ ] Add migration templates: let users define custom pre/post migration hooks

Exit criteria:
- [ ] Users can manage a sequence of database migrations through Patchworks rather than ad-hoc SQL files
- [ ] Every migration can be validated before application and rolled back after
- [ ] Migration state is tracked persistently, not just in-memory

---

### Phase 9 — Plugin and extension architecture
**Status: not started**

Goals:
- [ ] Design and implement a plugin trait/interface for custom diff formatters (HTML, Markdown, JSON)
- [ ] Design and implement a plugin trait for custom export targets (Alembic, Flyway, Liquibase)
- [ ] Design and implement a plugin trait for custom inspectors (application-specific table semantics, data validation rules)
- [ ] Add a plugin discovery and loading mechanism
- [ ] Add built-in reference plugins (JSON, HTML, Markdown)
- [ ] Define plugin API stability guarantees and versioning
- [ ] Add plugin documentation and development guide

Exit criteria:
- [ ] Third-party developers can extend Patchworks output formats and inspection logic without forking
- [ ] The plugin API has a stability contract and documentation
- [ ] At least three built-in plugins demonstrate the architecture

---

### Phase 10 — Team features and shared snapshot registries
**Status: not started**

Goals:
- [ ] Design a snapshot registry protocol: push/pull snapshots to/from a shared store
- [ ] Add `patchworks push` and `patchworks pull` commands for snapshot exchange
- [ ] Add snapshot naming, tagging, and annotation
- [ ] Add snapshot comparison across machines
- [ ] Add snapshot retention policies
- [ ] Add snapshot integrity verification
- [ ] Add access control primitives for shared registries

Exit criteria:
- [ ] A team of developers can share database snapshots through a common registry
- [ ] Snapshot integrity is verifiable end-to-end
- [ ] The shared registry is optional — Patchworks remains fully functional as a local-only tool

---

### Phase 11 — CI/CD integration and automation ecosystem
**Status: not started**

Goals:
- [ ] Add a `patchworks check` command for CI gates (exit 0 if identical, exit 1 if different)
- [ ] Add GitHub Actions integration
- [ ] Add pre-commit hook support
- [ ] Add machine-readable output formats (JSON, JSONL) across all CLI commands
- [ ] Add `--format` flag across all CLI commands (human, json, jsonl)
- [ ] Stabilize and document exit code conventions
- [ ] Add `patchworks watch` mode
- [ ] Add webhook/notification support
- [ ] Add GitOps workflow documentation

Exit criteria:
- [ ] Patchworks can be dropped into a CI pipeline with a single `patchworks check` command
- [ ] Machine-readable output is stable and documented
- [ ] At least one real GitHub Actions workflow demonstrates the integration

---

### Phase 12 — Long-term platform evolution
**Status: not started**

Goals:
- [ ] Evaluate multi-engine support (DuckDB, libSQL, other embedded databases)
- [ ] Evaluate embedded scripting (Lua, Rhai, WASM)
- [ ] Evaluate a TUI mode as a middle ground between GUI and CLI
- [ ] Evaluate remote database support (SSH, HTTP, cloud storage)
- [ ] Evaluate real-time collaboration
- [ ] Evaluate database schema visualization (ERD generation)
- [ ] Evaluate integration with existing migration frameworks (Diesel, SQLx, Alembic, Flyway)
- [ ] Evaluate performance profiling integration

Exit criteria:
- [ ] Each evaluation produces a decision document (build, defer, or reject) with rationale
- [ ] Any accepted capability follows the same phase-gated development discipline
- [ ] Platform evolution does not compromise SQLite-first correctness

---

## Decisions

### decision-0001: GUI-first desktop tool
**Date:** 2026-03-20

The shipped product is the desktop app with a small snapshot CLI surface. Headless automation work stays explicit future scope instead of implied capability.

### decision-0002: Snapshot state under ~/.patchworks/
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

### decision-0006: Indexes and triggers tracked from sqlite_master
**Date:** 2026-03-20

Indexes and triggers are tracked and preserved in generated SQL. Schema fidelity can advance ahead of UI completeness without requiring dedicated panels first.

### decision-0007: Background diff execution
**Date:** 2026-03-20

Diff computation runs on a background thread. Further UI-thread loading work extended this to inspection and table refresh.

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

---

## Risks

### Active risks

- Background inspection, table loading, and diff execution report coarse staged progress but do not support cooperative interruption or a user-facing cancel control
- SQL export still builds one large in-memory payload and can materialize large tables during seeding
- Live databases, WAL-backed databases, encrypted databases, and actively changing sources are still only best-effort
- CI is currently Linux-only even though this is a desktop app and macOS users likely matter
- Snapshot matching depends on canonicalized paths and can behave awkwardly if files move
- The current lightweight UI is usable, but product polish can easily outrun underlying scale and correctness work if the order slips

### Strategic risks

- Feature breadth expanding faster than test coverage could erode the correctness trust that defines Patchworks
- A plugin system introduces API stability obligations that constrain future refactoring
- Shared snapshot registries introduce network, auth, and security concerns absent in the local-only model
- Multi-engine support (Phase 12) could dilute SQLite-specific correctness if abstraction boundaries are drawn wrong
- The diff engine is currently synchronous and single-threaded at the table level; parallelizing across tables will be needed for large multi-table diffs but introduces ordering and progress-reporting complexity

---

## Open questions

| Question | Phase | Impact |
|----------|-------|--------|
| Should the next major investment go into async loading/performance or headless CLI? | 3-4 | Resource allocation |
| How far should live/WAL-backed database guarantees go before docs promise more than best-effort? | 3-5 | Trust boundary |
| Does large-export hardening require a streaming writer API or can the current shape be improved incrementally? | 3 | Architecture |
| Should `SnapshotStore` remain simple with one connection per operation or get a persistent connection? | 3 | Complexity tradeoff |
| Is the first release-quality support bar Cargo install only, or does the project need desktop packaging? | 5 | Distribution |
| Should the plugin system use compiled Rust dynamic libraries, WASM, or both? | 9 | Architecture |
| Should three-way merge be a core feature or a plugin? | 7 | Scope |
| How should migration chain state be stored? | 8 | Storage architecture |
| Is multi-engine support worth the abstraction cost? | 12 | Product identity |
| Should the shared snapshot registry be a separate service or embedded? | 10 | Architecture |

---

## Immediate next moves

1. Rework SQL export and seeding toward bounded-memory behavior for large databases.
2. Add sharper tests and docs for live or WAL-backed database behavior so the product's trust boundary is explicit.
3. Decide whether headless CLI work starts before or after the next responsiveness hardening slice lands.
4. Add a macOS CI build smoke path once the current hardening priorities are underway.
5. Revisit explicit cancellation only if the background-task model grows cooperative checkpoints or a cancellable job runner.
6. Begin designing the CLI command structure for Phase 4 (subcommand layout, output format conventions, exit codes).
7. Evaluate `clap` subcommand architecture for `patchworks inspect`, `patchworks diff`, `patchworks export`, `patchworks snapshot`.

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

---

## Decision log

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

---

*Update this log only with things that actually happened.*
