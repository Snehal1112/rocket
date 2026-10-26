# Sync actions backend and consent Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the token and opt-in security gap with a machine-local consent record, then expose the write actions (Check now, interval, token, skip, apply, rename links) as thin Tauri commands that are safe to call from the UI. The UI for them is Plan 10.

**Architecture:** A small consent store under `~/.rocket-api/` records, per canonical collection root and `source_id`, the exact spec location the user approved. `check`, the token read and the scheduler refuse to run (or send the keychain token) unless the record matches the source's current location. Action commands call the existing `OpenApiSyncService` methods through `spawn_blocking`. Apply takes the same per-collection lock as editor saves and publishes an event so the UI reloads.

**Tech Stack:** Rust (`rocket-collection`, `rocket-infra`, `rocket-app`, `src-tauri`), `rocket-shared` events.

**Spec:** `docs/superpowers/specs/2026-10-25-openapi-import-and-sync-design.md`. Carry-forward: `00-plan-index.md` sections for Plans 4, 6, 7 and 8 (the token item, the opt-in stopgap, the sync lock vs editor saves, `spawn_blocking`).

## Global Constraints

- Tasks touching `rocket-collection`, `rocket-infra`, `rocket-app` or collection files start with: "Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`."
- Cargo: `-j4` and `-p <crate>`, never `--workspace`. No unwrap calls in production paths. camelCase renames on IPC DTOs only. New persisted fields use `#[serde(default)]`.
- A secret never crosses IPC and never sits in a collection folder. The keychain token is sent only to the location the user approved on this machine.
- Commands stay thin: validate, call the service, map the result. Blocking work (`check`, `apply`, file reads) runs through `tokio::task::spawn_blocking`; never hold a lock or a `FsFileTx` across an `.await`.
- Commits go through `dev-workflow-skills:1-git-commit` with explicit paths. Gates: `cargo check -j4`, targeted `cargo test -j4 -p <crate> <name>`.

## Decisions (rulings, designer's call under the user's delegation)

- Consent replaces `opt_in_root`. A background check, Check now and any token read need a consent record whose `location` equals the source's current normalized location (`display_url`). `opt_in_root` stays readable for old files but is ignored.
- Consent is granted by the user acting on this machine: at import (the user typed the link), by "Set interval" or "Set token" in the tab, or by an explicit "Trust this spec link" button. A cloned or restored folder has no record, so it never checks and never sends a token until the user approves the location shown.
- A changed location (the committed source file now points elsewhere) invalidates the match: the status says `consent: "location_changed"` and nothing runs until the user approves the new location.
- The consent store is a separate small file (`~/.rocket-api/openapi-sync.yml`), not the collection trust store, because trust models "the file asks, the user allows", not "the user asked".

## Review Focus

- Folder copied with a valid-looking `openapi-local.json`: nothing checks, no token is read from the keychain.
- Source file edited to point a GitHub `source_id` at another repo: the token is not sent; status says `location_changed`.
- Two collections that share a `source_id` (folder copy): consent is per canonical root, so approving one does not approve the other.
- Apply while an editor save runs, and the reverse: no lost write, no deadlock.
- An apply that fails halfway or is stale reports it and publishes the event once.
- `check` waiting behind a long apply does not block a tokio worker.

---

### Task 1: Consent store and gated check

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-collection/src/openapi_consent.rs` (trait and types), `crates/rocket-infra/src/fs_openapi_consent_repo.rs`
- Modify: `crates/rocket-collection/src/lib.rs`, `crates/rocket-infra/src/lib.rs`, `crates/rocket-app/src/openapi_sync_service.rs` (gate in `check`, `stored_token`, `due_targets`, `status`, `set_interval`, `set_token`), `crates/rocket-app/src/openapi_sync_scheduler.rs` (use consent, drop `opt_in_root`), `crates/rocket-collection/src/openapi_local.rs` (keep `opt_in_root` readable and ignored), `src-tauri/src/commands/import.rs` (`make_sync_service` wires the store; import grants consent)
- Test: `crates/rocket-app/src/openapi_sync_consent_tests.rs`, repo tests beside the repo

**Interfaces:**
- Produces: `trait OpenApiConsentStore: Send + Sync { fn get(&self, root: &Path, source_id: &str) -> DomainResult<Option<OpenApiConsent>>; fn grant(&self, root: &Path, source_id: &str, location: &str) -> DomainResult<()>; fn revoke(&self, root: &Path, source_id: &str) -> DomainResult<()>; }`, `OpenApiConsent { source_id, location, granted_at }`; `ConsentState { Granted, Missing, LocationChanged }` and `OpenApiSyncService::consent_state(&self, root: &Path) -> DomainResult<ConsentState>`; `OpenApiSyncService::grant_consent(&self, root: &Path) -> DomainResult<()>` (records the source's current location); `SyncStatusView` gains `consent: ConsentState`; `CheckOutcome`/`check` return `Failed { code: "consent_required" | "location_changed", .. }` without any network or keychain access when the state is not Granted.
- The store keys by canonical root (follow `fs_trust_store.rs`: canonicalize, atomic write, fail closed on a corrupt file by treating every lookup as Missing and refusing to overwrite it).

- [ ] **Step 1: Write failing tests** (`openapi_sync_consent_tests.rs`, fake store and fetcher with a request recorder, fake secret store with a read counter): `check_without_consent_makes_no_request_and_reads_no_token`; `check_with_location_changed_makes_no_request_and_reads_no_token`; `check_with_consent_sends_the_token_to_the_approved_location`; `consent_is_per_root_so_a_copied_folder_is_missing`; `due_targets_skips_collections_without_consent`; `opt_in_root_alone_no_longer_turns_checks_on`; `set_interval_grants_consent_for_the_current_location`; repo tests: grant, get, revoke, corrupt file fails closed and is not overwritten.
- [ ] **Step 2: Run them to see them fail** (`cargo test -j4 -p rocket-app consent`).
- [ ] **Step 3: Implement the trait, the fs repo and the service gate.** The gate sits at the top of `check` and inside the one place that reads the keychain token (`stored_token`), so no path can read the token without it. `status()` reports `consent` and does not read the keychain when it is not Granted (`has_token` is then `false`). Scheduler `due_targets` filters on consent and no longer reads `opt_in_root`. Import (`import_openapi_url`) grants consent for the typed link; file imports have no remote location and need none.
- [ ] **Step 4: Run the tests until green, then `cargo check -j4`, `cargo test -j4 -p rocket-app openapi_sync` and `cargo test -j4 -p rocket-infra openapi_consent`; commit.**

---

### Task 2: Action commands

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src-tauri/src/commands/openapi_sync.rs` (new commands and DTOs), `src-tauri/src/lib.rs` (register), `crates/rocket-app/src/openapi_sync_apply.rs` (`plan` gains `links`), `crates/rocket-app/src/openapi_sync_service.rs` (skip methods if missing)
- Test: DTO and mapping tests in `src-tauri/src/commands/openapi_sync.rs`, service tests in `crates/rocket-app/src/openapi_sync_plan_tests.rs`

**Interfaces:**
- Consumes: Task 1 (`grant_consent`, `consent_state`), `OpenApiSyncService::{check, set_interval, set_token, clear_token, apply}`, `Selection`, `ApprovedItem`, `SkipRequest`, `VariableApproval`, `Choice`, `ApplyReport` (non-serde; map to DTOs), `plan_with_links`.
- Produces Tauri commands (each `collection: String` first arg, all through `spawn_blocking`): `openapi_sync_check_now` -> `CheckOutcomeDto { state: "unchanged"|"changed"|"failed", code?, message?, retryAfterSecs? }`; `openapi_sync_set_interval(collection, interval)`; `openapi_sync_set_token(collection, token)` and `openapi_sync_clear_token(collection)`; `openapi_sync_grant_consent(collection, location)` (the `location` must equal the source's current location string or the command refuses, so the UI can only approve what it showed); `openapi_sync_revoke_consent(collection)`; `openapi_sync_plan(collection, links: Vec<[String; 2]>)` (adds `links`, default empty); `openapi_sync_apply(collection, selection: SelectionDto) -> ApplyReportDto`. `SelectionDto { items: [{key, digest, choices: {field: "spec"|"yours"}}], skips: [{key, fingerprint}], variables: [{name, digest}], renameLinks: [[oldKey, newKey]] }` and `ApplyReportDto { applied: [{key, kind: "created"|"updated", requestPath}], stale: [String], notRewritable: [String], deleted: [String], detached: [String], skipped: usize, remaining: usize }` (camelCase). `SyncStatusDto` gains `consent: "granted"|"missing"|"locationChanged"`, `lastMessage?: String`.
- Persisted failure message: the last failed check stores a short safe `last_message` (no URL query, no token) in `OpenApiLocalState` (`#[serde(default)]`, skipped when empty) so the status and the tab can show why a check failed; cleared on success.

- [ ] **Step 1: Write failing tests**: a `SelectionDto` JSON round trip maps `"yours"`/`"spec"` to `Choice`, keeps `renameLinks` order, and rejects an unknown choice with a clear error; `ApplyReportDto` maps every list; `openapi_sync_grant_consent` refuses a location that differs from the source (pure mapping function tested); `plan` with `links` returns digests equal to those `apply` checks (apply the plan's own digests with the same links and get no `stale`); a failed check stores `last_message` and a later success clears it; `check_now` refuses with `consent_required` when not granted.
- [ ] **Step 2: Run to see failures, implement the DTOs, mapping and commands, run green.**
- [ ] **Step 3: Skip support:** a `SkipRequest` is applied by `apply` today; add a `skips`-only apply path only if `apply` with an empty `items` list cannot record skips (check first; do not add a second path if it can).
- [ ] **Step 4: `cargo check -j4`, `cargo test -j4 -p rocket openapi_sync`, `cargo test -j4 -p rocket-app openapi_sync`; commit.**

---

### Task 3: Shared save lock, apply event, lazy token read

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/openapi_sync_service.rs` (make `collection_lock` public), `crates/rocket-app/src/collection_service.rs` (take the lock in the mutating methods that write collection files), `crates/rocket-shared/src/events.rs` (new `DomainEvent::OpenApiSyncApplied { collection, applied, stale }` with the existing wire rules: tag camelCase, fields snake_case, contract test), `src-tauri/src/tauri_event_bus.rs` (Tauri event `openapi-sync-applied`), `src-tauri/src/commands/openapi_sync.rs` (publish after apply), `crates/rocket-app/src/openapi_sync_service.rs` (`status` reads the keychain only when asked)
- Test: `crates/rocket-app/src/openapi_sync_lock_tests.rs`, event contract test in `events.rs`

**Interfaces:**
- Consumes: Tasks 1 and 2, `collection_lock(&Path) -> Arc<Mutex<()>>` (process-wide std mutex by canonical root), `EventPublisher`.
- Produces: `CollectionService` mutating writes (`save_request`, `save_folder`, `save_settings`, create/rename/move/delete, import-in-place) hold `collection_lock(root)` for their whole load-modify-write; `SyncStatusView.has_token` becomes `Option<bool>` (`None` unless the caller passes `include_token: true`), so the overview, breadcrumb and notice never touch the keychain; `openapi_sync_status(collection, includeToken: Option<bool>)`; `OpenApiSyncApplied` published once per apply.

- [ ] **Step 1: Write failing tests**: `editor_save_waits_for_a_running_apply_and_the_reverse` (two threads, a gate, no lost write, finishes); `lock_order_is_collection_lock_then_tx_guard_everywhere` (a save inside apply's critical section on the same thread does not deadlock: use try-lock based assertion, document the rule); `status_without_include_token_never_reads_the_keychain` (read counter 0); `apply_publishes_one_event`; events.rs contract test pins `{"type":"openApiSyncApplied","collection":"api","applied":3,"stale":1}`.
- [ ] **Step 2: Run to see failures, implement, run green.** Keep the lock scope small; do not take it in read paths. If a `CollectionService` method already holds another lock, state the order in a comment (collection lock first).
- [ ] **Step 3: `cargo check -j4`, `cargo test -j4 -p rocket-app openapi_sync`, `cargo test -j4 -p rocket-app collection_service`, `cargo test -j4 -p rocket-shared events`; commit.**
