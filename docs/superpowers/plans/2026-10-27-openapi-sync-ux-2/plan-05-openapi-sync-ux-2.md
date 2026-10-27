# OpenAPI sync UX 2: Connect backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A collection without a source can be matched to an OpenAPI spec link, previewed without writing anything, and connected in one transaction.

**Architecture:** A pure matcher in `rocket-import` pairs requests with operations. `rocket-app` gets an `OpenApiConnectService` with `preview` (fetch, bundle, parse, match, write nothing) and `confirm` (one file transaction, then consent, interval and token in that order). Thin Tauri commands map results to fixed sentences.

**Tech Stack:** Rust (`rocket-import`, `rocket-app`, `src-tauri`).

**Spec:** `docs/superpowers/specs/2026-10-27-openapi-sync-ux-design.md` (section 9)

## File Structure

| File | Change |
|---|---|
| `crates/rocket-import/src/openapi/matching.rs (new)`, `openapi/mod.rs` | `normalise_path`, `match_requests`, result types |
| `crates/rocket-import/src/openapi/entry.rs` | Extract the fetch, bundle and parse part of `import_openapi_url` (`entry.rs:147-237`) into a function the preview reuses; the import keeps its behaviour |
| `crates/rocket-app/src/openapi_connect.rs (new)`, `openapi_connect_tests.rs (new)` | `preview`, `confirm`, `request_digest` |
| `src-tauri/src/commands/{openapi_sync.rs,import.rs}`, `src-tauri/src/lib.rs` | Commands `openapi_connect_preview`, `openapi_connect_confirm`; DTOs |

## Global Notes

- 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.
- Matching: normalise a request URL (drop a leading `{{var}}` or the server origin, drop the query, drop a trailing slash, treat `:id`, `{id}` and `{{var}}` segments as the same path parameter) and compare `METHOD` + path with each operation's template. One request to one operation: **Matched**. A request equal to several operations, or an operation matched by several requests: **Ambiguous**. No path match but the same method and a name equal to the operation summary or operationId (case-insensitive): **Suggested**. Operations with no request: **In spec only** (default Add). Requests with no operation: **Yours only** (default Keep). GraphQL and gRPC requests are ignored.
- Connect accepts a link only (GitHub or https). A token goes to GitHub links only. Consent is granted only for the link the user typed, and last.
- The preview is stateless. The confirm sends the rows, the link and the digest back; the digest is the sha256 over uid, request path, method and url of every HTTP request.
- Fixtures: reuse the fetcher double, `world()` helpers and the import fixtures of the existing `openapi_sync_*_tests.rs` and `entry.rs` tests (the pets spec `v1()`).
- Commands: `cargo test -j4 -p <crate> <filter>`, `cargo check -j4`.

---

### Task 1: Matching

**Interfaces:**
- Consumes: the parsed operations (`ParsedSpec::operation_views`, plan 02), request `method`, `url`, `name`, `uid`, `request_path`.
- Produces: `normalise_path(url: &str, origins: &[String]) -> String`; `match_requests(requests: &[MatchRequest], operations: &[SpecOperationView], origins: &[String]) -> MatchResult`; `MatchRequest { uid, request_path, method, url, name }`; `MatchResult { matched: Vec<(uid, key)>, suggested: Vec<(uid, key)>, in_spec_only: Vec<key>, yours_only: Vec<uid>, ambiguous: Vec<AmbiguousGroup> }`; `AmbiguousGroup { uids: Vec<String>, keys: Vec<String> }`.

- [ ] **Step 1: Read** how `entry.rs` builds `base_url` and the request url shape (`{{baseUrl}}/pets/{id}` style), and the `Request` method type.
- [ ] **Step 2: Write the failing tests** (`matching.rs`): `matches_get_orders_with_variable_prefix`, `treats_colon_and_brace_params_as_equal`, `ambiguous_when_two_requests_match_one_operation`, `ambiguous_when_one_request_matches_two_operations`, `suggests_by_name_only_when_path_differs`, `a_name_match_with_a_different_method_is_not_suggested`, `ignores_graphql_and_grpc_requests`, `drops_query_and_trailing_slash`, `drops_the_server_origin`, `unmatched_sides_land_in_spec_only_and_yours_only`, `zero_matches_still_returns_both_lists`.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-import matching`.
- [ ] **Step 4: Implement** the pure functions; no I/O, no regex built per call in a loop.
- [ ] **Step 5: Run again.** `cargo test -j4 -p rocket-import openapi`. Expected: green.

### Task 2: Preview

**Interfaces:**
- Consumes: Task 1, the extracted fetch-bundle-parse function, consent rules, `request_digest`.
- Produces: `request_digest(requests: &[MatchRequest]) -> String`; `OpenApiConnectService::preview(&self, collection: &str, link: &str, token: Option<&str>) -> DomainResult<ConnectPreview>` with `ConnectPreview { spec_title, spec_version, digest, matched, suggested, in_spec_only, yours_only, ambiguous }` (rows carry what the UI shows: method, path, name, summary, request path). Errors: a collection that already has a source is `Conflict`; a failed fetch is the fixed sentence of the import path ("The spec could not be fetched."), and a spec past the limits shows the existing fixed limit sentence. Nothing is written, no consent is granted, no token is stored. A missing or empty link is `InvalidInput`.

- [ ] **Step 1: Read** `import_openapi_url` (`entry.rs:147-237`) and the command `import_openapi_url` in `src-tauri/src/commands/import.rs` for the fixed sentences and the token rule.
- [ ] **Step 2: Write the failing tests:** `preview_matches_the_pets_collection`, `preview_writes_nothing` (the directory tree and mtimes are identical before and after), `preview_stores_no_token_and_grants_no_consent`, `preview_refuses_a_collection_that_has_a_source`, `preview_failure_gives_the_fixed_sentence` (the fetcher double fails with a message holding a path; it is absent), `preview_rejects_a_token_for_a_non_github_link`, `the_digest_changes_when_a_request_url_changes`, `the_digest_ignores_graphql_requests`. In `openapi_sync.rs`: `connect_preview_dto_uses_camel_case_keys`.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-app openapi_connect`.
- [ ] **Step 4: Implement** the service and the command (requires the typed link; the command does not read the keychain).
- [ ] **Step 5: Run again.** `cargo test -j4 -p rocket-app openapi_connect`, `cargo check -j4`. Expected: green.

### Task 3: Confirm

**Interfaces:**
- Consumes: Task 2, `baseline_hash` (the converter module), `CONVERTER_VERSION`, the spec-request builder used at import (same `base_url`, `group_by`), `OpenApiLocalState.skips`, plan 01 `save_applied`, `deps.tx`, the consent store and secret store used by `import_openapi_url` in the command layer.
- Produces: `OpenApiConnectService::confirm(&self, collection, link, token: Option<&str>, rows: Vec<ConnectRow>, group_by: GroupBy, interval: Option<SyncInterval>, digest: &str) -> DomainResult<ConnectOutcome>`; `ConnectRow { kind: Link | Add | Skip | Keep, key: Option<String>, uid: Option<String> }`; `ConnectOutcome { linked, added, skipped, warnings: Vec<String> }`. Order: recompute the digest (a mismatch is `Conflict` with the fixed sentence "The collection changed. Run the preview again."), then one transaction writes the source (one `SourceOperation` per Link and Add row), the baseline (every Link and Add key with its spec-converted request), the applied spec copy, the new requests of Add rows, and the skips (key and fingerprint) of Skip rows; after the commit grant consent for the typed link, then set the interval, then store the token. A failure after the commit never undoes the files; it adds a fixed warning ("Checks are not enabled yet." or "The token was not saved."). A failed commit writes nothing. A Link row's `baseline_hash` is `baseline_hash(&spec_request)` and its `request_path` is the request's current path.

- [ ] **Step 1: Read** `import_openapi_text` (how Add rows' requests are built), `source_record`, the skip fingerprint in `plan`, and the command wiring for consent and token in `import.rs`.
- [ ] **Step 2: Write the failing tests:**
  - `confirm_links_matched_rows_so_an_untouched_request_is_not_changed_by_you` (after confirm, `changes` lists nothing modified).
  - `confirm_links_an_edited_request_so_the_edit_reads_as_changed_by_you` (a request with a different header shows as modified).
  - `confirm_adds_the_spec_only_rows_as_new_requests`.
  - `a_skipped_spec_only_row_does_not_come_back_as_new_after_a_check` (run `check_with` the same spec; the plan has no New item for it).
  - `a_kept_row_changes_nothing`.
  - `confirm_rolls_back_when_the_commit_fails` (tx double failing on commit: no source, no baseline, no new request, no consent).
  - `confirm_refuses_a_stale_digest` (a request edited between preview and confirm; nothing written).
  - `consent_is_granted_last_and_only_for_the_typed_link` (the consent store double records the link and that the source file existed first).
  - `a_failed_consent_adds_a_warning_and_keeps_the_files` and `a_failed_token_save_adds_a_warning_and_keeps_the_files`.
  - `confirm_stores_the_applied_copy_so_view_spec_works`.
  - `confirm_refuses_a_collection_that_already_has_a_source`.
  - `confirm_with_zero_matches_still_succeeds`.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-app openapi_connect`.
- [ ] **Step 4: Implement** `confirm` under `collection_lock` with the order above, and the command with the DTOs; register both commands in `lib.rs`.
- [ ] **Step 5: Run again.** `cargo test -j4 -p rocket-app openapi_connect`, `cargo test -j4 -p rocket-import openapi`, `cargo check -j4`. Expected: green.

## Review Focus

1. A preview must never write, grant consent or save a token. Pinned by `preview_writes_nothing` and `preview_stores_no_token_and_grants_no_consent`.
2. A matched but edited request must show as "changed by you", not be overwritten or reported in sync. Pinned by `confirm_links_an_edited_request_so_the_edit_reads_as_changed_by_you`.
3. A skipped operation must not return as New on the next check. Pinned by `a_skipped_spec_only_row_does_not_come_back_as_new_after_a_check`.
4. Consent for a link the user did not type must be impossible. Pinned by `consent_is_granted_last_and_only_for_the_typed_link`.
5. A collection edited between preview and confirm must not be linked from stale rows. Pinned by `confirm_refuses_a_stale_digest`.

Next plan: docs/superpowers/plans/2026-10-27-openapi-sync-ux-2/plan-06-openapi-sync-ux-2.md
