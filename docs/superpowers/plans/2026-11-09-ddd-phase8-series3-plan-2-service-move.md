# Phase 8 series 3, plan 2: the sync service moves to `rocket-openapi` (D-81, part 2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `OpenApiSyncService` and every `impl` of it (apply, changes, delete, reset, skips, spec view, spec source, connect, change link) with their tests live in `rocket-openapi`. `rocket-app` keeps `OpenApiSyncScheduler` and the lock interplay tests. No behavior change.

**Architecture:** One atomic move, because `impl` blocks must sit in the crate that owns the type. Then an audit of visibility and dependencies, then docs and closing D-81.

**Tech Stack:** Rust: `rocket-openapi`, `rocket-app`, `src-tauri` (package `rocket`), `rocket-collection`.

**Spec:** `docs/superpowers/specs/2026-11-09-ddd-phase8-series3-openapi-sync-home-design.md` (section "Plan 2"). Series index: `docs/superpowers/plans/2026-11-09-ddd-phase8-index.md`. Run after plan 1.

## Global Constraints

- Behavior is identical: bodies, doc comments, error texts, lock order and JSON shapes do not change. Tests move with the code, with their `#[path]` chain.
- No re-export, alias or shim at an old location. Search every target over whole directories: `rg -n '<old name>' crates src-tauri` (includes `tests` and benches). Run `cargo check -j4 -p <pkg> --tests` for every package whose files changed (`rocket-app` about 6 minutes).
- `rocket-openapi` must not depend on `rocket-app`, `rocket-acp`, `rocket-network` or `rocket-git`; `rocket-app` must not gain a new dependency. Every new dependency of `rocket-openapi` beyond the spec's expected list is reported.
- Visibility: an item becomes `pub` only if `rocket-app` (the scheduler, `openapi_sync_lock_tests.rs`) or `src-tauri` needs it; report the list. Nothing else changes visibility.
- Format: `rustfmt --edition 2021 --check <file>` on every touched `.rs` file; hand-fix drift in place.
- Production code has no `unwrap`. Every `cargo` command takes `-j4`, names a package with `-p`, and never uses `--workspace` or `--all`. `cargo tree` takes no `-j`. Long runs go to a log file under the session scratchpad.
- Commits: conventional commits, one per task, explicit `git add <paths>` then plain `git commit`. Do not edit any `CLAUDE.md`, the roadmap or the index before task 3.
- Code comments are short, full sentences that end with a period.

## Review Focus

- A sync test lost in the move: the sum of `rocket-openapi` and `rocket-app` sync test counts equals the baseline.
- The lock order changed: the sync files still take `collection_lock` first (now from `rocket_collection::collection_lock`) and `openapi_sync_lock_tests.rs` passes unchanged in `rocket-app`.
- A command in `src-tauri/src/commands/openapi_sync.rs` or `import.rs` changed behavior or IPC shape (names, DTOs): the `src-tauri` tests pass and `git diff` of those files shows only import lines.
- A missed importer in `src-tauri/tests`, `crates/*/tests` or `crates/rocket-infra`.
- The new crate pulls in `rocket-app` (`cargo tree` check).

---

### Task 1: Move the service and its impl files

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (this task touches the OpenAPI source model and collection files).

**Files:**
- Move (all from `crates/rocket-app/src/` to `crates/rocket-openapi/src/`): `openapi_sync_service.rs` to `service.rs`; `openapi_sync_apply.rs` to `apply.rs`; `openapi_sync_changes.rs` to `changes.rs`; `openapi_sync_delete.rs` to `delete.rs`; `openapi_sync_reset.rs` to `reset.rs`; `openapi_sync_skips.rs` to `skips.rs`; `openapi_sync_spec_view.rs` to `spec_view.rs`; `openapi_sync_spec_source.rs` to `spec_source.rs`; `openapi_connect.rs` to `connect.rs`; `openapi_change_link.rs` to `change_link.rs`; and their tests: `openapi_sync_apply_tests.rs` to `apply_tests.rs`, `openapi_sync_plan_tests.rs` to `plan_tests.rs`, `openapi_sync_changes_tests.rs` to `changes_tests.rs`, `openapi_sync_spec_view_tests.rs` to `spec_view_tests.rs`, `openapi_sync_spec_source_tests.rs` to `spec_source_tests.rs`, `openapi_sync_reset_tests.rs` to `reset_tests.rs`, `openapi_sync_skips_tests.rs` to `skips_tests.rs`, `openapi_file_source_tests.rs` to `file_source_tests.rs`, `openapi_sync_delete_tests.rs` to `delete_tests.rs`, `openapi_connect_tests.rs` to `connect_tests.rs`, `openapi_change_link_tests.rs` to `change_link_tests.rs`, `openapi_sync_consent_tests.rs` to `consent_tests.rs`, `openapi_sync_disconnect_tests.rs` to `disconnect_tests.rs`. Update the `#[path = "..."]` attributes in `apply_tests.rs` to the new names.
- Stay in `rocket-app`: `openapi_sync_scheduler.rs`, `openapi_sync_scheduler_tests.rs`, `openapi_sync_lock_tests.rs`.
- Modify: `crates/rocket-openapi/Cargo.toml` and `lib.rs`, `crates/rocket-app/src/lib.rs` (drop the moved modules and the `pub use openapi_*` blocks), `crates/rocket-app/src/openapi_sync_scheduler.rs` and `openapi_sync_lock_tests.rs` (imports), `crates/rocket-app/src/openapi_sync_service.rs`'s test that uses `crate::test_doubles::NullCollectionRepo` (see step 3), `src-tauri/Cargo.toml` if needed, and every `src-tauri` importer: `commands/import.rs`, `commands/openapi_sync.rs`, `commands/workspaces.rs`, `lib.rs`, plus any other hit of `rg -n 'rocket_app::.*(OpenApiSync|CheckOutcome|SyncStatusView|SyncTarget|safe_message|ApplyDeps|RendererFactory|DeleteTarget|ResetReport|SkippedRow|ChangeLink|Connect|request_digest)' crates src-tauri`.

**Interfaces:**
- Produces: crate-root exports of `rocket_openapi`: `OpenApiSyncService`, `CheckOutcome`, `SyncStatusView`, `SyncTarget`, `safe_message`, `ApplyDeps`, `RendererFactory`, `DeleteTarget`, `ResetReport`, `SkippedRow`, `ChangeLinkOutcome`, `ChangeLinkPreview`, `request_digest`, `ConnectAmbiguous`, `ConnectOperationRow`, `ConnectOutcome`, `ConnectPair`, `ConnectPreview`, `ConnectRequestRow`, `ConnectRow` (the same names `rocket-app/src/lib.rs` re-exports today, minus the scheduler names and minus the `view` names that plan 1 moved). Modules stay public as `rocket_openapi::{service, apply, ...}` where `src-tauri` or the scheduler import by path.
- Consumes: `rocket_collection::collection_lock` and `rocket_openapi::{events, view}` from plan 1.

- [ ] **Step 1: Baseline.** Record the test counts: `cargo test -j4 -p rocket-app openapi` (all sync tests, with the scheduler and lock tests) and the `src-tauri` sync tests (`cargo test -j4 -p rocket openapi`). Note which files hold them.
- [ ] **Step 2: Move.** `git mv` every file above, add `pub mod` lines in `rocket-openapi/src/lib.rs` plus the root re-exports, fix `use crate::...` paths inside the moved files (`crate::openapi_sync_service` to `crate::service`, and so on; `crate::collection_lock` is already `rocket_collection::collection_lock`). Add dependencies the compiler asks for. Dev-dependencies: `rocket-infra` (the tests use `FsOpenApi*`), plus whatever else the tests use; report each.
- [ ] **Step 3: Fix the leftovers.** The test at about line 1143 of `service.rs` that uses `crate::test_doubles::NullCollectionRepo::new().on_lock_key(...)` moves into `crates/rocket-app/src/openapi_sync_lock_tests.rs` (a `rocket-app` test using the now-public builder `with_lock_keys`). Make `pub` only what `rocket-app` and `src-tauri` need. `rocket-app/src/lib.rs` drops the moved `pub mod`/`pub use` lines and keeps `pub mod openapi_sync_scheduler;` and `mod openapi_sync_lock_tests;`. `src-tauri` imports from `rocket_openapi` and gets `rocket-openapi.workspace = true` if plan 1 did not add it.
- [ ] **Step 4: Verify.** `cargo test -j4 -p rocket-openapi` passes; `cargo test -j4 -p rocket-app openapi` passes (scheduler and lock tests); the sum of both equals the baseline from step 1 (the moved lock-interplay test counts once). `cargo check -j4 --tests` passes for `rocket-openapi`, `rocket-app` and `rocket`; `cargo test -j4 -p rocket openapi` passes with the baseline count. `rg -n 'openapi_sync_service|openapi_connect|openapi_change_link|openapi_sync_apply' crates src-tauri --glob '*.rs'` shows no stale path. `cargo tree -p rocket-openapi -e normal --depth 1` lists no `rocket-app`, `rocket-acp`, `rocket-network` or `rocket-git`.
- [ ] **Step 5: Format and commit.** `rustfmt --edition 2021 --check` on every touched `.rs` file, then commit: `refactor(ddd): move OpenAPI sync service to rocket-openapi`.

---

### Task 2: Audit visibility and dependencies

**Files:**
- Modify: `crates/rocket-openapi/Cargo.toml`, `crates/rocket-openapi/src/*.rs` (visibility only), `crates/rocket-app/src/*.rs` only if an unused import remains.

**Interfaces:**
- Produces: nothing new.
- Consumes: the crate from task 1.

- [ ] **Step 1: Unused dependencies.** For each dependency of `rocket-openapi`, `rg -n '<crate>::' crates/rocket-openapi/src` shows a use (or a derive/macro use); remove any that has none. For `rocket-app`, check that `rocket-import` is still used (`rg -n 'rocket_import' crates/rocket-app/src`); remove the dependency only if nothing uses it, and report.
- [ ] **Step 2: Visibility.** List the `pub` items in `rocket-openapi` that nothing outside the crate uses (`rg` each name in `crates` and `src-tauri`). Lower to `pub(crate)` only the ones that were `pub(crate)` before the move. Report items that were `pub` before and are unused; do not change them.
- [ ] **Step 3: Lints.** `cargo clippy -j4 -p rocket-openapi --tests` and `cargo clippy -j4 -p rocket-app --tests` show no warning in the moved files (a warning in a file this series did not touch is reported, not fixed). `rustfmt --edition 2021 --check` on every `.rs` file in `crates/rocket-openapi`.
- [ ] **Step 4: Commit** only if files changed: `chore(ddd): trim rocket-openapi dependencies`. If nothing changed, report that.

---

### Task 3: Docs and closing D-81

**Files:**
- Modify: `crates/rocket-openapi/CLAUDE.md` (module map for every module, the sync design notes moved from `rocket-app/CLAUDE.md`), `crates/rocket-app/CLAUDE.md`, root `CLAUDE.md` (crate table), `.claude/docs-index.md`, `docs/features/openapi-sync*.md` and other docs naming moved files, `docs/roadmaps/ddd-roadmap.md` (row D-81 and decisions log), `docs/superpowers/plans/2026-11-09-ddd-phase8-index.md` (series 3 row), `docs/superpowers/specs/2026-11-09-ddd-phase8-series3-openapi-sync-home-design.md` (status line)

- [ ] **Step 1: Find stale statements.** `rg -n 'openapi_sync_|openapi_connect|openapi_change_link|collection_lock|OpenApiSyncService' .claude docs crates --glob '*.md' --glob '!docs/superpowers/**'`; judge each hit against the code.
- [ ] **Step 2: Update.** Move the sync service notes (rocket-app/CLAUDE.md "OpenAPI sync scheduler" sections keep only the supervisor loop, delays and start/stop; the service rules, consent gate, remote refs, reset/view/connect notes go to `crates/rocket-openapi/CLAUDE.md`) and point to the new paths. `rocket-collection/CLAUDE.md` gets a line for `collection_lock`. Roadmap D-81 `done (<date>, sync service, apply, events and view in rocket-openapi; scheduler stays in rocket-app; collection_lock moved to rocket-collection)` and a decisions-log line. Index series 3 row: plans listed and `done (<date>) on branch worktree-ddd-phase8-series1`. Spec status: `approved 2026-11-09, implemented`.
- [ ] **Step 3: Verify.** Every path named in the changed docs exists (`ls`), and the `rg` from step 1 shows no stale statement.
- [ ] **Step 4: Commit:** `docs(ddd): close D-81 and update guides`.
