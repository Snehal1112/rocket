# Phase 8 series 3, plan 1: the lock moves down and the crate appears (D-81, part 1)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `collection_lock` lives in `rocket-collection`, and a new crate `rocket-openapi` exists with the sync events and view DTOs. No behavior change.

**Architecture:** Move the lock first (it must sit below `rocket-app` and `rocket-openapi`), then create the crate and move the two leaf modules. No re-export at the old locations.

**Tech Stack:** Rust: `rocket-collection`, `rocket-openapi` (new), `rocket-app`, `src-tauri` (package `rocket`).

**Spec:** `docs/superpowers/specs/2026-11-09-ddd-phase8-series3-openapi-sync-home-design.md` (section "Plan 1"). Series index: `docs/superpowers/plans/2026-11-09-ddd-phase8-index.md`.

## Global Constraints

- Behavior is identical: bodies, doc comments, error texts and JSON shapes do not change. Tests move with the code.
- No re-export, alias or shim at an old location. Search every target over whole directories: `rg -n '<old name>' crates src-tauri` (includes `tests` and benches). After each task run `cargo check -j4 -p <pkg> --tests` for every package whose files changed (`rocket-app` takes about 6 minutes; `rocket` is the `src-tauri` package).
- `rocket-collection` gains no new dependency in task 1. `rocket-openapi` must not depend on `rocket-app`; `cargo tree -p rocket-openapi -e normal --depth 1` is checked.
- Format: `rustfmt --edition 2021 --check <file>` on every touched `.rs` file; hand-fix drift in place.
- Production code has no `unwrap`. Every `cargo` command takes `-j4`, names a package with `-p`, and never uses `--workspace` or `--all`. `cargo tree` takes no `-j`. Long runs go to a log file under the session scratchpad.
- Commits: conventional commits, one per task, explicit `git add <paths>` then plain `git commit` (never `-A`, `.` or `--only` with deleted paths). Do not edit the roadmap or the index before plan 2 task 3.
- Code comments are short, full sentences that end with a period.

## Review Focus

- The lock order or fairness changed: the ticket lock tests move unchanged and pass.
- A user of `collection_lock` was missed (`execution_service`, `collection_service`, `collection_trust`, `network/sync_collections` and its tests, the sync files, `src-tauri`).
- The crate pulls in `rocket-app` or a heavy dependency by accident (`cargo tree` check).
- `OpenApiSyncEvent` wire shape changes: the `wire_contract_*` tests and the `tauri_event_bus` test stay green.

---

### Task 1: Move `collection_lock` to `rocket-collection`

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (this task touches `rocket-collection`).

**Files:**
- Move: `crates/rocket-app/src/collection_lock.rs` to `crates/rocket-collection/src/collection_lock.rs`
- Modify: `crates/rocket-collection/src/lib.rs` (add `pub mod collection_lock;`), `crates/rocket-app/src/lib.rs` (drop `pub(crate) mod collection_lock;`), every importer found by `rg -n 'collection_lock' crates src-tauri`

**Interfaces:**
- Produces: `rocket_collection::collection_lock::{CollectionLock, CollectionGuard, collection_lock, with_collection_lock}`, all `pub` (they are `pub(crate)` today). Signatures and bodies unchanged.
- Consumes: nothing.

- [ ] **Step 1: Baseline.** `cargo test -j4 -p rocket-app collection_lock` and note the count and names of the lock's own tests (they move).
- [ ] **Step 2: Move.** `git mv` the file, add `pub mod collection_lock;` to `rocket-collection/src/lib.rs`, change `pub(crate)` to `pub` on the items other crates call, drop the module line in `rocket-app/src/lib.rs`. Replace every `crate::collection_lock::` with `rocket_collection::collection_lock::` (use a `use` line, keep the existing import style of each file).
- [ ] **Step 3: Verify.** `cargo test -j4 -p rocket-collection collection_lock` passes with the same test count as the baseline. `cargo check -j4 --tests` passes for `rocket-collection`, `rocket-app` and `rocket`. `rg -n 'crate::collection_lock' crates src-tauri` has no hit. `cargo tree -p rocket-collection -e normal --depth 1` lists no new dependency.
- [ ] **Step 4: Format and commit.** `rustfmt --edition 2021 --check` on the touched files, then commit: `refactor(ddd): move collection lock to collection`.

---

### Task 2: Create `rocket-openapi` with the events and view DTOs

**Files:**
- Create: `crates/rocket-openapi/Cargo.toml`, `crates/rocket-openapi/src/lib.rs`, `crates/rocket-openapi/CLAUDE.md`
- Move: `crates/rocket-app/src/openapi_sync_events.rs` to `crates/rocket-openapi/src/events.rs`; `crates/rocket-app/src/openapi_sync_view.rs` to `crates/rocket-openapi/src/view.rs`
- Modify: root `Cargo.toml` (workspace member and `[workspace.dependencies]` entry `rocket-openapi = { path = "crates/rocket-openapi" }`, follow how `rocket-network` is listed), `crates/rocket-app/Cargo.toml`, `src-tauri/Cargo.toml`, `crates/rocket-app/src/lib.rs`, root `CLAUDE.md` (crate table row), every importer of the two modules (`rg -n 'openapi_sync_events|openapi_sync_view|OpenApiSyncEvent' crates src-tauri`)

**Interfaces:**
- Produces: `rocket_openapi::events::OpenApiSyncEvent` and `rocket_openapi::view::{BackupRow, BadgeView, ChangeRow, ChangeValue, ChangesView, OverviewView, PlanView, SpecOpRow, SpecSource, SpecSourceFormat, SpecTagGroup, SpecView, SpecViewState, UnchangedView}` (every public item of the two files; use `rg '^pub ' ` on `openapi_sync_view.rs` to confirm the list). `lib.rs` declares `pub mod events; pub mod view;`.
- Consumes: nothing from task 1 except that the workspace builds.

- [ ] **Step 1: Create the crate.** `Cargo.toml` with `rocket-shared.workspace = true`, `rocket-import.workspace`-style dependency (`rocket-import = { path = "../rocket-import" }` if it is not a workspace entry), `serde.workspace = true`; add only what the compiler asks for and report it. `CLAUDE.md`: role (the OpenAPI sync context: service, apply, events, view DTOs), dependency direction (may use `rocket-shared`, `rocket-collection`, `rocket-import`, `rocket-http`, `rocket-environment`; never `rocket-app`), module map with `events` and `view`, and the commands `cargo check -p rocket-openapi -j4` and `cargo test -p rocket-openapi -j4`.
- [ ] **Step 2: Move the two files.** `git mv`, then fix the module paths inside them. `rocket-app` and `rocket-openapi` both list each other's names correctly: `rocket-app/src/lib.rs` drops `pub mod openapi_sync_events;`, `pub mod openapi_sync_view;` and the `pub use openapi_sync_view::{...}` block. The files still in `rocket-app` that use these names import them from `rocket_openapi::events` and `rocket_openapi::view`. `src-tauri` imports from `rocket_openapi` (`tauri_event_bus.rs`, `commands/openapi_sync.rs`, any other hit).
- [ ] **Step 3: Verify.** `cargo test -j4 -p rocket-openapi` passes (the events' `wire_contract_*` tests and any view tests moved with the files; report the count). `cargo check -j4 --tests` passes for `rocket-openapi`, `rocket-app` and `rocket`. `cargo test -j4 -p rocket openapi_sync_events_emit_on_their_own_single_channel` passes. `cargo tree -p rocket-openapi -e normal --depth 1` lists no `rocket-app`.
- [ ] **Step 4: Format and commit.** `rustfmt --edition 2021 --check` on the touched files, then commit: `refactor(ddd): add rocket-openapi crate with events and view`.
