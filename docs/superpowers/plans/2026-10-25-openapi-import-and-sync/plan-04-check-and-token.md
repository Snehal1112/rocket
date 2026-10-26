# OpenAPI Spec Check, Sync Settings and Token Implementation Plan (Plan 4 of the OpenAPI series)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A collection imported from a link can re-check its spec on demand: a conditional fetch that spots a real change, keeps the last good copy, records the interval and last check, and works for private GitHub repositories through a token kept in the OS keychain (also usable at import time).

**Architecture:** The source file gains sync state (interval, last check, status, pending copy hash, a stable `source_id`). A new `OpenApiSyncService` in `rocket-app` does one check: conditional fetch, validate, compare hashes, store a pending copy, update the state. The token lives in the keychain under the collection's `source_id`. The import path learns to take a token. No scheduler and no review UI yet (Plans 7 to 9 call this service).

**Tech Stack:** Rust (`rocket-collection`, `rocket-infra`, `rocket-import`, `rocket-app`, `src-tauri`), React + TypeScript, shadcn/ui, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-25-openapi-import-and-sync-design.md`. Series index: `00-plan-index.md`.

## Global Constraints

- Link rules (Plan 2): https only, at most 5 redirects, 30 s timeout, 25 MB cap (`DEFAULT_MAX_SPEC_BYTES`). The token goes only to the original origin, never after a redirect to another origin (already enforced by `ReqwestSpecFetcher`).
- "If a fetch or parse fails, keep the last good copy, show the reason, and apply nothing." A failed check never changes `spec_sha256`, the operations list or any request.
- A token, or any secret, is never written to a file in the collection, a log, a debug print or an error message. Tokens are used for GitHub locations only.
- "One token per collection, stored under that collection's id." The id is `source_id`, stored in the source file, so renaming or copying the collection folder keeps the token bound to it.
- Intervals are a fixed list: off, 1 hour, 1 day, 7 days, monthly (30 days). The default for a new import is off, so nothing is fetched in the background until the user chooses.
- Persisted structs use snake_case keys and no serde renaming; camelCase only on IPC DTOs. Old source files (version 1 without the new keys) must still load.
- Rust: never unwrap errors in production paths, `-j4` and `-p <crate>` on cargo commands, never `--workspace`. Frontend: shadcn/ui primitives and lucide icons only, Bun commands (`bun run test`, `bunx tsc --noEmit`, `bun run check`).
- Commits go through `dev-workflow-skills:1-git-commit` with explicit paths. Frontend commits hit the yarn commit gate; the user runs those with `HARNESS_GATE=0`.
- Tasks that touch `rocket-import`, `rocket-collection`, `rocket-infra` or collection files start with: "Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`."

## Review Focus

- A 304 after an earlier check found a change must not hide that change: the pending copy and the "changed" status stay until the review applies or dismisses them (Task 2).
- A 200 that is not a spec (captive portal, error page, truncated file) is a failure: no "changed" status, no pending copy, no new validators (Task 2).
- The token never appears in the source file, the pending file, any message, debug output or request sent to a non-GitHub location (Tasks 2 and 3).
- A source file from Plan 2 or 3 (no `source_id`, no sync keys) still loads; a renamed or copied collection folder keeps its token (Tasks 1 and 2).
- A last-check time in the future (clock moved back) or an unreadable one makes the collection due, not stuck (Task 1).

---

### Task 1: Sync state in the source file

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-collection/src/openapi_source.rs` (new fields, `SyncInterval`, `SyncStatus`, pending-copy methods on the trait).
- Modify: `crates/rocket-infra/src/fs_openapi_source_repo.rs` (implement the pending-copy methods).
- Modify: `crates/rocket-import/src/openapi/source_record.rs` (set `source_id`, interval off).
- Modify: every other implementer of `OpenApiSourceRepository` (find them with grep; the `FailingSources` fake in `crates/rocket-import/src/openapi/entry.rs` tests is one).
- Modify: `crates/rocket-import/CLAUDE.md` (one paragraph on the new fields).

**Interfaces:**
- Consumes: `OpenApiSource`, `OpenApiSourceRepository::{load, save, delete}`, `atomic_write`.
- Produces:
  - New `OpenApiSource` fields, all `#[serde(default)]` and skipped when empty: `source_id: String`, `sync_interval: SyncInterval`, `last_checked_at: Option<String>` (RFC 3339 UTC), `last_status: Option<SyncStatus>`, `pending_sha256: Option<String>`.
  - `enum SyncInterval { Off, Hourly, Daily, Weekly, Monthly }` (serde snake_case, `Default` is `Off`) with `fn as_secs(self) -> Option<u64>` (`None` for Off; 3600, 86400, 604800, 2_592_000), `fn parse(&str) -> Option<SyncInterval>` for the snake_case names, and `fn is_due(self, last_checked_at: Option<&str>, now: chrono::DateTime<chrono::Utc>) -> bool`.
  - `enum SyncStatus { Unchanged, Changed, Failed { code: String } }` (serde tagged by `state`, snake_case). `code` is a short machine word such as `not_found`, `rate_limited`, `not_a_spec`.
  - `OpenApiSource::new_source_id() -> String` (a v4 UUID string).
  - Trait additions: `fn save_pending(&self, collection_root: &Path, spec: &[u8]) -> DomainResult<()>`, `fn load_pending(&self, collection_root: &Path) -> DomainResult<Option<Vec<u8>>>`, `fn clear_pending(&self, collection_root: &Path) -> DomainResult<()>` (a missing file is not an error). The file is `<collection>/.rocket/openapi-pending.spec`, written with `atomic_write`.

- [ ] **Step 1: Write the failing tests** (in the existing test modules of `openapi_source.rs` and `fs_openapi_source_repo.rs`):
  - `old_source_file_loads_with_defaults`: a YAML with only the Plan 2 keys loads with `source_id == ""`, `sync_interval == Off`, no last check, no status, no pending hash.
  - `sync_fields_round_trip_with_snake_case_keys`: save and load a source with all new fields set (`last_status: Failed { code: "not_found" }`); the file text contains `sync_interval:` and `last_checked_at:` and does not contain `token`.
  - `interval_seconds`: Off is `None`; the other four are 3600, 86400, 604800, 2_592_000. `parse("daily") == Some(Daily)`, `parse("Daily") == None`, `parse("") == None`.
  - `is_due_follows_the_interval`: with a fixed `now`: Off is never due; no last check is due; hourly with a check 59 min ago is not due and 60 min ago is due; monthly at 29 days is not due and at 30 days is due.
  - `is_due_when_the_last_check_is_unreadable_or_in_the_future`: `Some("not a date")` and a time one hour after `now` are both due.
  - `pending_copy_round_trips_and_clears`: `save_pending` then `load_pending` returns the same bytes; `clear_pending` then `load_pending` is `None`; `clear_pending` on a missing file is `Ok`.
  - `pending_copy_does_not_break_the_collection_loader`: a collection with a pending file next to the source file still loads (copy the pattern of `collection_with_source_file_loads_in_strict_mode`).
  - In `source_record.rs`: `record_gets_a_fresh_source_id_and_interval_off` (two records get different non-empty ids).
- [ ] **Step 2: Run to verify they fail.** Run: `cargo test -j4 -p rocket-collection openapi_source` and `cargo test -j4 -p rocket-infra fs_openapi_source_repo`. Expected: compile errors for the missing items.
- [ ] **Step 3: Implement** the Interfaces block. `is_due` compares `now - last` against `as_secs`; any parse failure or a negative difference counts as due. Update the fakes with simple in-memory or error-returning bodies.
- [ ] **Step 4: Run to verify they pass.** Run the two commands above, then `cargo test -j4 -p rocket-import openapi` and `cargo check -j4`. Expected: all pass.
- [ ] **Step 5: Commit** with `dev-workflow-skills:1-git-commit`. Message: `feat(import): keep sync state in the OpenAPI source file`.

---

### Task 2: `OpenApiSyncService` and the token

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-infra/src/secret_store.rs` (add `KeyringSecretStore::new_openapi_sync()` with service `com.rocketapi.openapi-sync`, and extend the distinct-namespace test).
- Modify: `crates/rocket-import/src/openapi/entry.rs` (make the failure text a public function taking `token_sent: bool`; `describe_fetch_failure` keeps working for the import path) and `crates/rocket-import/src/openapi/mod.rs` plus `lib.rs` (export `check_spec_text`, `fetch_failure_message`).
- Modify: `crates/rocket-app/Cargo.toml` (move `rocket-import` from dev-dependencies to dependencies).
- Create: `crates/rocket-app/src/openapi_sync_service.rs`; declare and re-export in `crates/rocket-app/src/lib.rs`.

**Interfaces:**
- Consumes: Task 1 types and trait methods; `fetch_target`, `parse_spec_url`; `SpecFetcher`; `SecretStore` (`get`, `set`, `delete`); `chrono`.
- Produces:
  - `rocket_import::check_spec_text(bytes: &[u8]) -> ImportResult<()>`: Ok when the bytes are UTF-8 and parse as a supported spec (same parser as the import, no collection written).
  - `rocket_import::fetch_failure_message(failure: &SpecFetchFailure, token_sent: bool) -> String`. With `token_sent` true, `Unauthorized` and `NotFound` say the token was rejected or lacks Contents read access; with false they keep the Plan 3 text.
  - `pub enum CheckOutcome { Unchanged, Changed { spec_sha256: String }, Failed { message: String, retry_after_secs: Option<u64> } }`.
  - `pub struct SyncStatusView { pub location: String, pub interval: SyncInterval, pub last_checked_at: Option<String>, pub last_status: Option<SyncStatus>, pub has_pending: bool, pub has_token: bool }`.
  - `OpenApiSyncService::new(fetcher: Arc<dyn SpecFetcher>, sources: Box<dyn OpenApiSourceRepository>, secrets: Arc<dyn SecretStore>) -> Self`, `with_clock(self, clock: Arc<dyn Fn() -> chrono::DateTime<chrono::Utc> + Send + Sync>) -> Self` (tests set a fixed time; the default is `Utc::now`).
  - `async fn check(&self, collection_root: &Path) -> DomainResult<CheckOutcome>`
  - `fn status(&self, collection_root: &Path) -> DomainResult<SyncStatusView>`
  - `fn set_interval(&self, collection_root: &Path, interval: SyncInterval) -> DomainResult<()>`
  - `fn set_token(&self, collection_root: &Path, token: &str) -> DomainResult<()>`, `fn clear_token(&self, collection_root: &Path) -> DomainResult<()>`. A GitHub location is required; an `Https` location gives an error saying tokens work with GitHub links only. When the source file has an empty `source_id` (a Plan 2 or 3 file), `set_token` first generates and saves one.
  - Keychain entry: `scope_id = source_id`, `key = "token"`.

Check rules (the core of this task):
1. No source file is an error. Read the token only for a `Github` location; an `Https` location sends none.
2. Send `if_none_match` and `if_modified_since` from the stored validators, `max_bytes = DEFAULT_MAX_SPEC_BYTES`.
3. `NotModified`: set `last_checked_at`; status is `Changed` if `pending_sha256` is set, else `Unchanged`. The outcome follows the status. Nothing else changes.
4. `Fetched` with a body that fails `check_spec_text`: `Failed` (code `not_a_spec`), set `last_checked_at` and the failed status, keep the old validators, pending copy and hashes.
5. `Fetched` whose SHA-256 equals `spec_sha256`: `Unchanged`, store the new validators, clear any pending copy and `pending_sha256`.
6. `Fetched` whose hash equals `pending_sha256`: `Changed`, store the validators, rewrite nothing.
7. `Fetched`, valid and new: `save_pending(body)`, set `pending_sha256`, store the validators, status `Changed`, outcome `Changed { spec_sha256 }`. `spec_sha256` and `operations` stay as imported.
8. A fetch failure: `Failed { message: fetch_failure_message(f, token_sent), retry_after_secs }` (only `RateLimited` has a value); set `last_checked_at` and the failed status (code from the variant); change nothing else.
9. Every path that changes state saves the source file once, at the end.

- [ ] **Step 1: Write the failing tests** (`#[tokio::test]` in `openapi_sync_service.rs`; a `FakeFetcher` that returns a queue of canned results and records every `SpecFetchRequest`; an in-memory `SecretStore`; the real `FsOpenApiSourceRepo` from the dev-dependency; the fixture is `crates/rocket-import/tests/fixtures/openapi/orders-3.0.json` and a changed copy with one extra path built in the test; a fixed clock):
  - `not_modified_records_the_check_and_sends_the_validators`: request carries the stored `if_none_match`; outcome `Unchanged`; `last_checked_at` equals the clock.
  - `same_body_with_new_etag_is_unchanged_and_updates_the_validators`; also clears a leftover pending copy.
  - `changed_valid_spec_becomes_pending`: outcome `Changed`; `load_pending` equals the body; `spec_sha256` and `operations` are exactly what they were.
  - `a_second_different_spec_replaces_the_pending_copy`.
  - `not_modified_after_a_change_keeps_the_change`: first a `Changed` check, then `NotModified`; the outcome is `Changed`, the pending copy is still there.
  - `html_body_fails_and_keeps_everything`: outcome `Failed`; the pending copy and validators are as before; status is `Failed { code: "not_a_spec" }`.
  - `fetch_failures_map_to_messages_and_retry_after`: `NotFound`, `RateLimited { retry_after_secs: Some(60) }` (outcome carries `Some(60)`), `Network("x")`; each sets `last_checked_at` and a failed status and changes nothing else.
  - `github_token_is_sent_as_a_bearer_to_the_contents_api`: after `set_token`, the recorded request has `bearer_token == Some(token)` and an `api.github.com` URL with the raw `Accept`.
  - `https_location_never_sends_a_token_and_refuses_to_store_one`: `set_token` errors; a check sends `bearer_token == None`.
  - `token_never_reaches_the_collection_folder`: after `set_token` and a check, no file under the collection root contains the token text, and `format!("{:?}", status)` does not contain it.
  - `unauthorized_with_a_token_names_the_token`: `fetch_failure_message(&Unauthorized, true)` contains `"token"`; `fetch_failure_message(&Unauthorized, false)` contains `"private"`. A check whose fetcher answers `Unauthorized` after `set_token` returns a message containing `"token"`.
  - `token_follows_a_copied_collection_folder`: copy the folder to a new path; `status(new_root).has_token` is true.
  - `set_token_adds_a_source_id_to_an_older_file`: source saved with `source_id: ""`; after `set_token` the loaded source has a non-empty id and `has_token` is true.
  - `set_interval_persists_and_clear_token_forgets`.
  - `status_reports_pending_and_token_flags`.
  - In `secret_store.rs`: the new namespace is distinct from the other four.
  - In `entry.rs`: `check_spec_text` accepts the fixture and rejects `<html></html>` and invalid UTF-8.
- [ ] **Step 2: Run to verify they fail.** Run: `cargo test -j4 -p rocket-app openapi_sync_service`. Expected: compile errors for the missing items.
- [ ] **Step 3: Implement** the items above following the check rules in order. Keep `check` as one function with one early-return per rule; put hashing in a small helper. Read the token with the lenient `SecretStore::get`.
- [ ] **Step 4: Run to verify they pass.** Run: `cargo test -j4 -p rocket-app openapi_sync_service`, `cargo test -j4 -p rocket-infra secret_store`, `cargo test -j4 -p rocket-import openapi`, then `cargo check -j4`. Expected: all pass, no warnings.
- [ ] **Step 5: Commit** with `dev-workflow-skills:1-git-commit`. Message: `feat(import): check an OpenAPI source for changes`.

---

### Task 3: Import from a private GitHub link with a token

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-import/src/openapi/entry.rs` (`import_openapi_url` takes a token; `OpenApiImportResult` gains `source_id: Option<String>`).
- Modify: `src-tauri/src/commands/import.rs` (token argument, store it after a successful import, `token_arg`).
- Modify: `src/lib/tauri-api.ts` (`importOpenApiUrl` gets an optional fourth parameter; `OpenApiImportResult` unchanged on the TS side).
- Modify: `src/components/import/ImportCollectionDialog.tsx` and its test; update the tab description.
- Modify: `crates/rocket-import/CLAUDE.md` (token note in "Importing from a file or a link").

**Interfaces:**
- Consumes: Task 2 `OpenApiSyncService::set_token`, `fetch_failure_message`; Plan 3 command and dialog.
- Produces:
  - `ImportService::import_openapi_url(&self, fetcher, sources, link, group_by, token: Option<&str>, workspace_id)`: with a token and a `Github` location it uses `fetch_target(&location, true)` and sends the bearer; with a token and an `Https` location it returns `Err(ImportError::SpecUnavailable)` ("tokens work with GitHub links only") before any fetch. `OpenApiImportResult.source_id` is the id written to the source file for a link import, `None` for a file import.
  - Tauri `import_openapi_url(link, group_by, token: Option<String>, target_workspace_id, ...)`: `token_arg(Option<String>) -> Option<String>` trims and turns blank into `None`. After a successful import with a token, the command builds an `OpenApiSyncService` (fetcher state, `FsOpenApiSourceRepo`, `KeyringSecretStore::new_openapi_sync()`) and calls `set_token`. If that fails, the import still succeeds and `warnings` gains "The token could not be saved, so checks on this private repository will fail."
  - TS: `importOpenApiUrl(link: string, groupBy: OpenApiGroupBy, targetWorkspaceId: string, token?: string)` invoking `import_openapi_url` with `{ link, groupBy, targetWorkspaceId, token }` (`token` omitted when undefined).
  - Dialog: in Link mode, a password `Input` labelled "Access token (optional)" with the helper text "Only for private GitHub repositories. Kept in your system keychain, never in the collection." Tab description: "Import an OpenAPI 3.0, 3.1 or Swagger 2.0 spec (JSON or YAML) from a link or a file. GitHub file links work, and private repositories need a token."

- [ ] **Step 1: Write the failing tests:**
  - Rust, `entry.rs`: `github_link_with_a_token_uses_the_contents_api_and_a_bearer` (recorded request: `api.github.com` URL, `bearer_token == Some("t0k")`, raw `Accept`); `https_link_with_a_token_is_refused_before_any_fetch` (the fake fetcher is never called, no collection folder); `token_never_reaches_the_source_file_or_the_result` (no file under the collection root, no warning and no report text contains `t0k`); `unauthorized_with_a_token_says_the_token_was_rejected`; `link_import_returns_the_source_id_written_to_the_file`.
  - Rust, `import.rs`: `token_arg_trims_and_drops_blank_values` (`None`, `Some("  ")`, `Some(" abc ")` give `None`, `None`, `Some("abc")`).
  - TS API: `importOpenApiUrl` passes `token` when given and leaves the key out when not.
  - Dialog: `shows a token field in Link mode and hides it in File mode`; `the token field is a password input`; `sends the trimmed token with the link`; `sends no token when the field is blank`; `clears the token when the dialog closes and reopens`; `does not show the token on the done screen`.
- [ ] **Step 2: Run to verify they fail.** Run: `cargo test -j4 -p rocket-import openapi::entry`, `cargo test -j4 -p rocket --lib token_arg` (read the package name from `src-tauri/Cargo.toml`), `bun run test ImportCollectionDialog`, `bun run test tauri-api.openapi`. Expected: FAIL.
- [ ] **Step 3: Implement** the Interfaces block. The command stays thin: `token_arg`, call the service, store the token, map to the DTO. The dialog keeps the token in state, resets it in `handleClose` and `switchImportSource`, and never renders it back.
- [ ] **Step 4: Run to verify they pass.** Run the four commands above, then `cargo check -j4`, `bunx tsc --noEmit`, `bun run check`. Expected: all pass.
- [ ] **Step 5: Commit** with `dev-workflow-skills:1-git-commit` (the frontend part needs `HARNESS_GATE=0` from the user). Message: `feat(import): import a private GitHub spec with a token`. Then update `00-plan-index.md` (P4 row done) and `docs/roadmaps/roadmap.md` R-70.

---

## Self-review notes

- Spec coverage: conditional requests with ETag/Last-Modified plus a body-hash fallback (Task 2 rules 3 to 7); keep the last good copy on failure (rules 4 and 8); distinct failure states (rule 8 and `SyncStatus`); token in the keychain, one per collection, never in files or logs (Tasks 2 and 3); interval list and last check time (Task 1). The pending copy gives Plan 5 its "new spec" input and lets the "changes found" notice survive a restart (Plan 7). Not here: the scheduler and catch-up (P7), the diff and merge (P5), apply (P6), the tab and settings UI (P8, P9), commands for check, status, interval and token (added with the UI in P9).
- Ruling: the default interval is off, so a freshly imported collection makes no background requests until the user opts in. The spec's "never silent" anti-goal outweighs convenience.
- Deviations from the index outline: the token is first used at import time (Task 3), because a private repository could not be imported at all without it. The index's "sync settings" are the Task 1 fields; they have no UI until P9.
- Deferred: a token shared between collections; non-GitHub credentialed hosts (spec non-goal); host-name resolution for redirect targets.
