# OpenAPI sync UX 3: Change the spec link Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A connected collection can move to another spec link without Disconnect, keeping the baseline, skips, backups and interval.

**Architecture:** `OpenApiSyncService` gains `change_link_preview` and `change_link_confirm`, modelled on Connect's preview/confirm (`openapi_connect.rs`). Two thin commands, TS wrappers and hooks, and a form in the connection dialog.

**Tech Stack:** Rust (`rocket-app`, `src-tauri`), React + TypeScript, react-query, shadcn/ui, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-28-openapi-sync-ux-3-design.md` (section 1)

## File Structure

| File | Change |
|---|---|
| `crates/rocket-app/src/{openapi_change_link.rs (new),openapi_change_link_tests.rs (new),lib.rs}` | `ChangeLinkPreview`, `ChangeLinkOutcome`, service methods |
| `src-tauri/src/commands/openapi_sync.rs`, `src-tauri/src/lib.rs` | DTOs and two commands |
| `src/lib/{tauri-api.ts,queries/openapi-sync-queries.ts}`, `openapi-sync/syncErrors.ts` | Types, wrappers, hooks, sentences |
| `src/components/collections/openapi-sync/{ChangeLinkForm.tsx (new),OpenApiSyncConnectionDialog.tsx}` | Form in the Spec link section |

## Review Focus

1. A link that matches almost no operations must warn before commit. Pinned by `preview_counts_matched_added_and_removed`.
2. A token must never be sent to, or kept for, a different repo. Pinned by `a_different_repo_deletes_the_token` and `the_same_repo_keeps_the_token`.
3. Baseline, skips, backups, interval and `spec_sha256` must survive. Pinned by `confirm_keeps_baseline_skips_backups_and_interval`.
4. A stale preview or a running apply must change nothing. Pinned by `a_changed_spec_gives_stale_spec` and `confirm_refuses_while_an_apply_runs`.
5. The old link's etag must not be sent to the new link. Pinned by `confirm_clears_validators`.

---

### Task 1: Service preview and confirm

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Interfaces:**
- Consumes: `fetch_spec_bundle`, `parse_spec_url`, `ParsedSpec::{parse,operation_views}`, `source_record`'s remote-ref helpers, the pending staging used by `check` (`PENDING_REL`, `save_pending` path), `grant_consent_for_link`, `collection_lock`, `recover_if_idle`, `secrets.delete(source_id, TOKEN_KEY)`, `deps.tx`.
- Produces: `ChangeLinkPreview { spec_sha256: String, title: String, version: Option<String>, matched: usize, added: usize, removed: usize, same_spec: bool }`; `ChangeLinkOutcome { pending: bool, token_kept: bool, warnings: Vec<String> }`; `OpenApiSyncService::change_link_preview(&self, collection: &str, link: &str, token: Option<&str>) -> DomainResult<ChangeLinkPreview>` and `change_link_confirm(&self, collection: &str, link: &str, token: Option<&str>, spec_sha256: &str) -> DomainResult<ChangeLinkOutcome>`. Fixed errors: `NEEDS_LINK`, `NOT_CONNECTED`, `SAME_LINK` ("This is already the spec link."), `STALE_SPEC`.

- [ ] **Step 1: Read** `openapi_connect.rs` (`preview`, `confirm`), `check` staging of the pending copy, and `source_record`.
- [ ] **Step 2: Write the failing tests** in `openapi_change_link_tests.rs` (`use super::*` of the apply-tests world): `preview_counts_matched_added_and_removed`, `preview_refuses_the_current_link`, `preview_refuses_an_unconnected_collection`, `preview_writes_nothing`, `confirm_keeps_baseline_skips_backups_and_interval`, `confirm_clears_validators`, `confirm_stages_a_differing_spec_as_pending`, `confirm_with_an_identical_spec_leaves_nothing_pending`, `confirm_grants_consent_for_the_typed_link`, `the_same_repo_keeps_the_token`, `a_different_repo_deletes_the_token`, `a_https_link_deletes_the_token`, `a_changed_spec_gives_stale_spec`, `confirm_refuses_while_an_apply_runs`.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-app change_link`. Expected: module not found.
- [ ] **Step 4: Implement** both methods. Confirm order: lock, `recover_if_idle`, reload source, re-fetch and compare hash, one transaction (source with new `location`, validators cleared, refs from the bundle, pending fields and copy per the spec), then consent, then token deletion; failures after commit are warnings.
- [ ] **Step 5: Run again.** `cargo test -j4 -p rocket-app change_link`, `cargo check -j4`. Expected: green.

### Task 2: Commands, wrappers and hooks

**Interfaces:**
- Consumes: Task 1; the Connect command, wrapper and hook patterns (`openapi_connect_preview`, `previewOpenApiConnect`, `usePreviewOpenApiConnect`), `invalidateSync`.
- Produces: commands `openapi_sync_change_link_preview` and `openapi_sync_change_link_confirm` (camelCase DTOs: `specSha256`, `sameSpec`, `tokenKept`); TS types `OpenApiChangeLinkPreview`, `OpenApiChangeLinkOutcome`; wrappers `previewOpenApiChangeLink(collection, link, token?)`, `confirmOpenApiChangeLink(collection, link, token, specSha256)`; hooks `usePreviewChangeLink(collection)`, `useConfirmChangeLink(collection)` (the confirm hook calls `invalidateSync`); `KNOWN` entries for `SAME_LINK` and `NOT_CONNECTED`.

- [ ] **Step 1: Read** `connect_error` and the Connect DTO tests in `src-tauri/src/commands/openapi_sync.rs`, and `syncErrors.ts`.
- [ ] **Step 2: Write the failing tests:** `change_link_preview_dto_uses_camel_case_keys`, `change_link_errors_pass_only_fixed_sentences` (Rust); `change_link_wrappers_call_the_commands` (`tauri-api.openapi-sync.test.ts`); `confirm_change_link_refreshes_the_sync_queries` (`openapi-sync-queries.test.tsx`); `syncErrors.test.ts` cases for the two sentences.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket change_link`, `yarn test openapi-sync`.
- [ ] **Step 4: Implement** the commands (register in `lib.rs`), wrappers, hooks and sentences.
- [ ] **Step 5: Run again.** `cargo check -j4`, `yarn tsc --noEmit`, `yarn check`. Expected: green.

### Task 3: Change link form

**Interfaces:**
- Consumes: Task 2 hooks, `isGithubLocation`, `ConnectOpenApiDialog`'s link/token inputs as the pattern, `syncErrorMessage`.
- Produces: `ChangeLinkForm({ collection, currentLocation, onDone }: { collection: string; currentLocation: string; onDone: () => void })`. A **Change link** button in the Spec link section reveals the form. Steps: enter link (token field only for Github links), **Check link** runs the preview and shows "N of M endpoints match. A new spec will be reviewed in Spec updates." (or "The spec is the same."), a warning "Only N of M endpoints match this spec. Check that this is the right link." when `matched * 2 < total`, then **Change link** confirms. An epoch ref drops late results; the dialog cannot close while confirming; success closes the form and shows "Link changed."

- [ ] **Step 1: Read** `ConnectOpenApiDialog.tsx` (epoch, `requestClose`) and `OpenApiSyncConnectionDialog.tsx`.
- [ ] **Step 2: Write the failing tests** (`ChangeLinkForm.test.tsx`, plus one in the dialog test): `checking_a_link_shows_the_match_counts`, `a_low_match_shows_the_warning`, `the_token_field_shows_for_github_links_only`, `confirm_calls_the_hook_with_the_previewed_hash`, `a_stale_error_asks_to_check_again`, `cannot_close_while_confirming`, `the_dialog_shows_change_link_in_the_spec_link_section`.
- [ ] **Step 3: Run to see them fail.** `yarn test ChangeLinkForm`.
- [ ] **Step 4: Implement** the form and mount it.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`. Expected: green.

Next plan: docs/superpowers/plans/2026-10-28-openapi-sync-ux-3/plan-02-openapi-sync-ux-3.md
