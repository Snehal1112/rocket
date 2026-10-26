# OpenAPI Remote References Implementation Plan (Plan 5b of the OpenAPI series)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A spec that takes parts of itself from other files (`$ref: https://.../definitions.yaml#/parameters/limit`, or `$ref: definitions.yaml#/...` next to the main file) imports completely, and the sync keeps working: a change in a referenced file shows up as a change of the spec. A reference Rocket cannot follow says so in plain words, once per file, not once per use.

**Architecture:** A pure bundler in `rocket-import` finds the remote references in the main spec, fetches those files through the existing safe `SpecFetcher`, and inlines them into one self-contained text (each file under a root key `x-rocket-remote`, every `$ref` rewritten to a local pointer). The rest of the pipeline (parser, converter, spec hash, pending copy, baseline, merge, apply) then runs unchanged on that one text. A spec without remote references comes out byte for byte as it went in, so its hash and every existing collection stay as they are. The source file remembers one validator pair (ETag, Last-Modified) per fetched file so a check stays cheap.

**Tech Stack:** Rust (`rocket-import`, `rocket-collection`, `rocket-app`, `rocket-http` types), TypeScript (import dialog), `serde_yaml`, `url` if already in the tree.

**Spec:** `docs/superpowers/specs/2026-10-25-openapi-import-and-sync-design.md` (fetch rules: https only, redirect cap, body cap, token only to its own origin; "If a fetch or parse fails, keep the last good copy"). Series index: `00-plan-index.md`.

## Global Constraints

- "Reject `http`, `file` and other schemes; cap redirects (about 5); cap body size (25 MB); set a timeout; never send the token to a different host after a redirect." Every referenced file goes through `SpecFetcher`, so these hold by construction.
- "If a fetch or parse fails, keep the last good copy, show the reason, and apply nothing." A referenced file that cannot be fetched during a **check** is a failed check, never a changed spec (otherwise its parameters would show as removed).
- The token is sent only to a referenced file that is in the same GitHub repository as the main spec. Every other file is fetched without a token.
- A reference with a query string, a user name, a non-https scheme, or a host that is a loopback, private, link-local or unspecified address literal (`redirect_target_allowed`) is not followed. The source file is committed, so no URL that may carry a secret is ever stored in it.
- Limits: at most 20 referenced files, nesting depth 3, 25 MB in total for the main spec plus all files. Over a limit, the rest stays unresolved with one note.
- A spec with no remote reference is returned unchanged (same bytes). Persisted structs use snake_case keys with no serde renaming; new fields have `#[serde(default)]` and are skipped when empty. Never unwrap errors in production paths. Cargo uses `-j4` and `-p <crate>`, never `--workspace`.
- Log lines and warnings may show a file name or host, never a query string, a token or a local path.
- Commits go through `dev-workflow-skills:1-git-commit` with explicit paths. Task 2 changes TypeScript: the commit gate runs `yarn tsc`, which fails here, so that commit is made by the user with `HARNESS_GATE=0`.
- Tasks that touch `rocket-import`, `rocket-collection` or collection files start with: "Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`."

## Rulings made while planning (open to your objection at plan review)

1. **Inline into one text, instead of a multi-file bundle.** The spec hash, the pending copy, the baseline and apply all assume one text. Inlining keeps them untouched; the price is that the stored pending copy is a rewritten spec, not the author's bytes.
2. **Where a file lands.** Remote file number `i` (in first-encounter order) becomes `x-rocket-remote.<i>` at the root of the main document. A `$ref` to `URL#/a/b` becomes `#/x-rocket-remote/<i>/a/b`; a `#/a/b` inside file `i` becomes `#/x-rocket-remote/<i>/a/b`. The order is deterministic, so the same inputs give the same bytes and the same hash.
3. **Relative references are followed for links.** `defs.yaml#/x` is resolved against the main spec's raw address (for GitHub: `raw.githubusercontent.com/{owner}/{repo}/{ref}/{path}`, so the repository token still works for a private repository). A file import has no base address, so a relative reference there stays unresolved with a clear note, and so does an absolute one (`import_openapi_file` has no fetcher). Fetching from a file import is a later decision.
4. **Fetch failure at import is a warning, at check is a failure.** An import with one unreachable file still creates the collection, with that reference unresolved and one note. A check with one unreachable file fails with code `ref_unavailable` and changes nothing.
5. **Cheap checks.** The source file keeps `remote_refs: [{url, etag, last_modified}]`. A check sends the main spec's validators first. If the main spec answers 304 and every file answers 304, nothing changed. If anything changed, the main spec is fetched again without validators and bundled again. A source written before this plan has no `remote_refs`; its first check behaves as before. A collection imported before this plan whose spec has remote references is fixed by importing it again (documented, not migrated).
6. **The converter version does not change.** The converter output for a given resolved spec is the same; only the spec text differs.

## Review Focus

- A file the spec points at is never read from the local disk, never fetched over http or from a private or loopback address literal, and the token never leaves the main spec's repository (Tasks 1 and 3).
- A cyclic reference between two files, a file that references itself, a huge fan-out and a 1,000-reference spec stop at the limits with a note, not a hang or an unbounded fetch (Task 1).
- A transient failure of one referenced file never turns into "these parameters were removed" (Task 3).
- A spec with no remote reference gives the same hash, pending copy and baseline as before this plan (Tasks 1 and 3).
- The import dialog tells the user what was not loaded and what that means for their requests (Task 2).

---

### Task 1: The bundler

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-import/src/openapi/bundle.rs` (declared in `openapi/mod.rs`, public items re-exported from `lib.rs`).
- Modify: `crates/rocket-import/CLAUDE.md` (a "Remote references" section).

**Interfaces:**
- Consumes: `rocket_http::{SpecFetcher, SpecFetchRequest, SpecFetchOutcome, SpecFetchFailure, redirect_target_allowed, DEFAULT_MAX_SPEC_BYTES}`; `parse_spec_url`, `fetch_target`, `SpecLocation` (`source_url.rs`).
- Produces:
  - `pub struct RemoteRefState { pub url: String, pub etag: Option<String>, pub last_modified: Option<String> }`.
  - `pub struct Bundle { pub text: String, pub refs: Vec<RemoteRefState>, pub notes: Vec<String>, pub incomplete: bool }`. `text` is the main text unchanged when no remote reference was found; `incomplete` is true when any reference could not be fetched or hit a limit; `notes` are plain sentences (one per file or limit, no query strings).
  - `pub fn find_remote_refs(main_text: &str, base: Option<&str>) -> Vec<String>`: the absolute, fragment-free, deduplicated URLs the main text refers to, in first-encounter order, after the rules in Global Constraints (references to skip are not listed here; they are noted by `bundle`).
  - `pub async fn bundle_spec(fetcher: &dyn SpecFetcher, main_text: &str, location: &SpecLocation, token: Option<&str>, previous: &[RemoteRefState]) -> ImportResult<Bundle>`. It resolves relative references against the location's raw address, fetches files breadth first (depth 3, 20 files, 25 MB total), parses each as YAML or JSON (not a map or not text: that file is unresolved with a note), and calls the inlining step. `previous` is used only to send validators; a 304 for a file in `previous` is reported through a second function below, so `bundle_spec` always fetches fresh bodies.
  - `pub async fn refs_unchanged(fetcher: &dyn SpecFetcher, location: &SpecLocation, token: Option<&str>, previous: &[RemoteRefState]) -> Result<bool, SpecFetchFailure>`: true when every file in `previous` answers 304 with its stored validators; false on the first changed file; the first failure is returned.
  - `pub fn inline_refs(main: &mut serde_yaml::Value, files: &[(String, serde_yaml::Value)])` (pure, deterministic) following ruling 2.
  - Token rule: a file is fetched with the token only when `parse_spec_url(file_url)` gives a `Github` location with the same owner and repo as `location`, through `fetch_target(&that_location, true)`; otherwise without.

- [ ] **Step 1: Write the failing tests** (a fake `SpecFetcher` that serves a map of URL to body and records every request):
  - Finding: `finds_absolute_and_relative_refs_in_order`; `ignores_local_refs`; `skips_query_userinfo_http_and_private_hosts_with_a_note`; `deduplicates_urls_and_drops_fragments`.
  - Inlining: `inlines_a_file_and_rewrites_pointers_inside_it` (a `#/a/b` inside the file points into the inlined copy); `file_to_file_refs_resolve_against_each_files_own_address`; `a_cycle_between_two_files_terminates`; `same_inputs_give_the_same_bytes`; `a_spec_without_remote_refs_is_returned_unchanged` (byte equal).
  - Fetching: `the_users_case_parameters_limit_offset_sort_resolve` (main spec plus a definitions file with `parameters.limit`, `offset`, `sort`; after `parse_openapi` on the bundle the operation has the three query parameters and there is no "remote $ref" warning); `token_goes_only_to_the_same_repository`; `other_hosts_get_no_token`; `a_failed_file_is_unresolved_with_one_note_and_incomplete_true`; `limits_stop_the_fetching` (21 files, depth 4, 26 MB total: fewer fetches than references, `incomplete` true, one note per limit); `a_thousand_references_to_one_file_fetch_it_once`.
  - `refs_unchanged`: `all_304_is_unchanged`; `one_200_is_changed`; `a_failure_is_returned`.
- [ ] **Step 2: Run to verify they fail.** Run: `cargo test -j4 -p rocket-import openapi::bundle`. Expected: compile errors for the missing items.
- [ ] **Step 3: Implement** the Interfaces block. Reuse `parse_spec_url` and `fetch_target`; do not parse URLs by hand where the `url` crate is already a dependency of the workspace (check before adding one).
- [ ] **Step 4: Run to verify they pass.** Run the command, `cargo test -j4 -p rocket-import openapi`, then `cargo check -j4`. Expected: all pass, no warnings.
- [ ] **Step 5: Commit** with `dev-workflow-skills:1-git-commit`. Message: `feat(import): bundle remote OpenAPI references`.

---

### Task 2: Import with remote references, and a clear dialog

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-import/src/openapi/entry.rs` (`import_openapi_url`), `crates/rocket-import/src/openapi/source_record.rs` (spec hash over the bundle, `remote_refs`), `crates/rocket-collection/src/openapi_source.rs` (`remote_refs`), `crates/rocket-import/src/report.rs` and `crates/rocket-import/src/importer.rs` (a dedicated reason), `crates/rocket-import/src/openapi/reader.rs` (the unresolved-reference warning text).
- Modify: `src/lib/tauri-api.ts` (the reason type), `src/components/import/ImportCollectionDialog.tsx` (summary line, label, grouping).
- Modify: `crates/rocket-import/CLAUDE.md`.

**Interfaces:**
- Produces:
  - `OpenApiSource.remote_refs: Vec<RemoteRefState>`-shaped list (`url`, `etag`, `last_modified`; `#[serde(default, skip_serializing_if = "Vec::is_empty")]`), defined in `rocket-collection` (the type `RemoteRefState` moves there and `rocket-import` re-exports it).
  - `import_openapi_url` bundles the fetched text (`bundle_spec`, `previous: &[]`), imports the bundled text, writes `spec_sha256` from the bundled bytes, stores `remote_refs` from `Bundle.refs`, and adds `Bundle.notes` to the result's warnings. A bundle that is `incomplete` still imports.
  - `SkipReason::NotFollowed(String)` serialized as `notFollowed`, used by the reader's warnings instead of `UnsupportedRequestType`; the unresolved-reference text reads "a `$ref` to <file name or host> was not loaded" (no query, no path beyond the file name).
  - Dialog: the summary line says "N requests imported" and, separately, "M notes" (the word "skipped" only for items that were really not imported); `notFollowed` shows as "Not loaded"; notes of one cause are grouped into one row ("3 parameters come from another file that Rocket did not load: definitions.yaml. The requests were imported without them.") with the full address in a tooltip; the raw enum name is never shown. Use shadcn primitives and lucide icons only.

- [ ] **Step 1: Write the failing tests.**
  - Rust (`entry.rs`, fake fetcher): `import_follows_a_remote_parameter_file` (the screenshot's spec: both requests import with `limit`, `offset`, `sort`, no skipped items, `spec_sha256` equals the hash of the bundled text, `remote_refs` holds one entry with the file's validators); `import_with_an_unreachable_ref_still_creates_the_collection_with_one_note`; `a_spec_without_remote_refs_has_the_same_hash_as_before` (hash equals the raw text hash, `remote_refs` empty, source file bytes carry no `remote_refs` key); `a_file_import_keeps_remote_refs_unresolved_with_the_new_wording`; `the_source_file_never_holds_a_query_string`.
  - Source: `remote_refs_round_trip_and_are_skipped_when_empty`.
  - Frontend (`yarn test` pattern for the dialog): `groups_not_followed_notes_by_cause`, `summary_does_not_call_notes_skipped`, `never_shows_the_raw_enum_name`.
- [ ] **Step 2: Run to verify they fail.** Run: `cargo test -j4 -p rocket-import openapi::entry` and `cargo test -j4 -p rocket-collection openapi_source`. Expected: compile errors or failures for the missing behavior.
- [ ] **Step 3: Implement** the Interfaces block. The Tauri command layer needs no change except the new reason in the DTO if the DTO maps reasons explicitly.
- [ ] **Step 4: Run to verify they pass.** Run both commands, `cargo test -j4 -p rocket-import openapi`, `cargo check -j4`, then `yarn tsc --noEmit` and `yarn test ImportCollectionDialog` if yarn works here (otherwise the user runs them). Expected: all pass, no warnings.
- [ ] **Step 5: Commit** the Rust part with `dev-workflow-skills:1-git-commit` (`feat(import): follow remote refs when importing a link`); the user commits the TypeScript part with `HARNESS_GATE=0`.

---

### Task 3: Checks and sync with remote references

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/openapi_sync_service.rs` (`check`), `crates/rocket-collection/src/openapi_source.rs` (nothing new beyond Task 2 unless the failure code needs a mapping), `crates/rocket-import/CLAUDE.md`, `docs/roadmaps/roadmap.md` (R-69, R-70), the plan index.

**Interfaces:**
- Consumes: Task 1 `bundle_spec`, `refs_unchanged`, `RemoteRefState`; Task 2 `OpenApiSource.remote_refs`.
- Produces: `check` behaves as follows (the existing rules 1 to 8 stay; these refine rules 2, 3 and 4):
  - Fetch the main spec with its validators. On 304 and an empty `remote_refs`: as before. On 304 and a non-empty list: `refs_unchanged`; true gives the old 304 behavior, false fetches the main spec again without validators and continues as a fresh fetch; an error is a failed check with the usual codes.
  - A fresh body goes through `bundle_spec`. `Bundle.incomplete` makes the check fail with code `ref_unavailable` and the message "A file the spec refers to could not be loaded." followed by the note, and keeps everything as it was (no pending copy, no hash change). Otherwise the bundled text is what is hashed, compared with `spec_sha256` and `pending_sha256`, and saved as the pending copy; `remote_refs` and the main validators are stored with the other check fields.
  - The token and the keychain rules are unchanged; the token reaches `bundle_spec` and is subject to its same-repository rule.

- [ ] **Step 1: Write the failing tests** (the existing fake fetcher and fixtures in `openapi_sync_service.rs`, extended to serve several URLs):
  - `a_changed_referenced_file_makes_the_spec_changed` (main 304, one file 200: the check is Changed, the pending copy is the new bundle, the pending hash matches its bytes, apply of the item adds the new parameter and keeps the user's edits);
  - `all_304_is_unchanged_with_no_main_refetch`;
  - `an_unreachable_file_fails_the_check_and_changes_nothing` (code `ref_unavailable`, source and pending bytes unchanged, `consecutive_failures` incremented);
  - `a_spec_without_remote_refs_behaves_as_before` (the existing check tests still pass unchanged);
  - `the_token_is_not_sent_to_a_file_in_another_repository`;
  - `refs_are_fetched_again_only_when_something_changed`.
- [ ] **Step 2: Run to verify they fail.** Run: `cargo test -j4 -p rocket-app openapi_sync`. Expected: failures for the missing behavior.
- [ ] **Step 3: Implement** the Interfaces block.
- [ ] **Step 4: Run to verify they pass.** Run: `cargo test -j4 -p rocket-app openapi_sync`, `cargo test -j4 -p rocket-import openapi`, `cargo test -j4 -p rocket-collection openapi_`, `cargo check -j4`. Expected: all pass, no warnings.
- [ ] **Step 5: Commit** with `dev-workflow-skills:1-git-commit`. Message: `feat(import): sync specs that use remote refs`. Then update `00-plan-index.md` (a P5b row, done) and R-69 and R-70.

---

## Self-review notes

- Spec coverage: https only, caps, token origin (Global Constraints, Task 1); keep the last good copy on a fetch failure (Task 3, ruling 4); the user's reported case (Task 1 test `the_users_case_...`, Task 2 test `import_follows_a_remote_parameter_file`); the confusing dialog text (Task 2).
- Not here: remote references in a **file** import (ruling 3: a later decision); migrating collections imported before this plan (re-import); following references to local disk paths (never, on purpose); DNS rebinding (host names are not resolved, as in the existing redirect rule).
- Known limits: a spec whose referenced file is huge counts against the 25 MB total; a referenced file that is not YAML or JSON is left unresolved; `$ref` forms other than `url#/pointer` (`$id`, `$anchor`, JSON Schema dynamic references) are not followed.
