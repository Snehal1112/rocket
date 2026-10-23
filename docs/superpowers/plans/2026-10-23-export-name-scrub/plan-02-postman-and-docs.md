# Export name-based scrub: Postman, dialog and docs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Postman export applies the same name rule as the Bruno ZIP (query pairs, header rows, form rows, OAuth URL fields, variables, environments), the OAuth1 and report-note gaps are closed, and the dialog and docs state the new coverage.

**Architecture:** Typed counterparts of the YAML helpers in `crates/rocket-export/src/scrub.rs`, called from `postman.rs`, `postman_auth.rs` and `postman_env.rs`. The dialog text and two docs are updated last, once the behavior is final.

**Tech Stack:** Rust (serde_json), React, TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-23-export-name-scrub-design.md`. Requires plan 1 (`plan-01-shared-rule-and-bruno.md`) to be merged: it provides `is_sensitive_name` and the note conventions.

## Global Constraints

- Same rules as plan 1: the name rule matches the last word only, the allow-list wins, a lone `{{ref}}` is kept, notes are `Blanked` with detail `<name> (by name)`, identifiers stay, bodies/scripts/descriptions are not scanned, no panicking unwrap calls, cargo per crate with `-j4`, never a workspace-wide run.
- Decision 4 of the spec: a secret variable holding a lone `{{ref}}` keeps the reference in the Postman collection, folder and request variables and in Postman environments.
- The golden file `crates/rocket-export/tests/fixtures/postman-golden.json` changes only by pasting actual output after an intended change, and the diff must contain only the new blanked values.
- Dialog: shadcn and lucide only; `yarn tsc --noEmit` and `yarn check` clean. Run `yarn install` first if `node_modules` is missing.

## Review Focus

1. `https://x.test/p?access_token=abc&token_type=Bearer` exports `access_token=` blanked and `token_type=Bearer` kept in both `url.raw` and `url.query`: pinned in Task 4 by `query_pairs_are_scrubbed_by_name_in_raw_and_query`.
2. A `query_params` table row `api_key=abc` is blanked in `url.query` and `url.raw`: pinned in Task 4 by `query_table_rows_are_scrubbed_by_name`.
3. OAuth2 `accessTokenUrl` with userinfo password or `client_secret=x` is blanked in the Postman auth block: pinned in Task 4 by `oauth_urls_are_scrubbed`.
4. An OAuth1 or WSSE request exports `noauth` and a `Skipped` note but no `Blanked` note: pinned in Task 5 by `skipped_auth_has_no_blanked_note`.
5. A Postman environment secret holding `{{ref}}` keeps it, a hydrated literal is blanked, and a non-secret `api_token` is blanked by name: pinned in Task 5 by `environment_secret_ref_is_kept_and_names_are_scrubbed`.

---

### Task 4: Postman rows, URLs and variables

**Files:**
- Modify: `crates/rocket-export/src/scrub.rs` (`scrub_url`, `scrub_auth`, new row helpers), `crates/rocket-export/src/postman.rs` (`request_item`, `graphql_item`, `variables_json`), `crates/rocket-export/tests/postman_golden.rs`, `crates/rocket-export/tests/fixtures/postman-golden.json`

**Interfaces:**
- Consumes: `is_sensitive_name`, `is_variable_reference`, `blank_note` (plan 1).
- Produces (all `pub` in `scrub.rs`, re-exported in `lib.rs` next to `scrub_header_values`):
  - `fn scrub_rows(rows: Vec<Header>, feature: &str, path: &str, notes: &mut Vec<ExportNote>) -> Vec<Header>` blanks the value of each row whose `key` matches the rule (disabled rows too).
  - `fn scrub_form_entries(entries: Vec<FormDataEntry>, path: &str, notes: &mut Vec<ExportNote>) -> Vec<FormDataEntry>` blanks text entries by `key`; file entries are untouched.
  - `scrub_url` keeps its signature and now also blanks the value of each `name=value` pair of the query whose name matches (feature `URL query`), besides the userinfo password.
  - `scrub_auth` also scrubs the URL fields of OAuth2 flows (`access_token_url`, `refresh_token_url`, `authorization_url`, `callback_url`) and the OAuth1 `callback_url` with the same logic as `scrub_url`.
  - `variables_json` blanks a variable when it is `secret` or its key matches the rule, and the value is a literal; features `Secret variable` and `Variable` (detail `<key> (by name)`).

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (auth and variables sections).** Then read plan 1's `sensitive.rs`, `crates/rocket-export/src/scrub.rs:255-360` (`scrub_auth`, `scrub_header_values`, `scrub_url`), `postman.rs` (`request_item`, `graphql_item`, `variables_json`, `postman_url`) and `postman_auth.rs:96-205` (where the OAuth2 URL fields are read).

- [ ] **Step 2: Write the failing tests.**
  - `query_pairs_are_scrubbed_by_name_in_raw_and_query` (in `postman.rs`): request url `https://x.test/p?access_token=abc&token_type=Bearer&a=1`. Expect `url.raw == "https://x.test/p?access_token=&token_type=Bearer&a=1"`, `url.query` rows `access_token` value `""`, `token_type` value `Bearer`, one `Blanked` note with detail `access_token (by name)`.
  - `query_table_rows_are_scrubbed_by_name`: `query_params` rows `api_key=abc` (enabled) and `debug=x` (disabled) plus url without a query. Expect the `api_key` value `""` in `url.query` and `raw` (`?api_key=`), `debug` unchanged.
  - `references_are_kept_in_query_rows`: `?access_token={{t}}` and row `api_key={{k}}` unchanged, no notes.
  - `form_entries_are_scrubbed_by_name`: urlencoded `client_secret=s`, `grant_type=password`; formdata text `password=p`, file entry `secret_file`. Expect only the first and the formdata `password` blanked.
  - `header_rows_use_the_name_rule`: header `X-Auth-Token: abc` blanked, `X-Request-Id: 1` kept.
  - `oauth_urls_are_scrubbed` (in `postman_auth.rs` or `postman.rs` tests): OAuth2 authorization-code with `access_token_url: https://u:pw@idp.test/token?client_secret=x&a=1`. Expect the Postman `accessTokenUrl` value `https://u:@idp.test/token?client_secret=&a=1`.
  - `variables_are_blanked_by_name_and_secret_refs_kept`: collection variables `api_token=abc` (plain), `plain_name=abc`, `s1` secret with `{{ref}}`, `s2` secret with `top`. Expect `api_token` and `s2` blanked, `plain_name` and `s1` unchanged; notes `Variable` for the first, `Secret variable` for `s2`.
  - Extend the existing golden `full_tree()` with one query param `api_key`, a header `X-Auth-Token`, a urlencoded `client_secret`, and a variable `api_token`, so the golden file covers each.

- [ ] **Step 3: Run `cargo test -j4 -p rocket-export postman`.** Expected: the new tests fail; the golden test fails once `full_tree()` is extended and prints the new JSON.

- [ ] **Step 4: Implement the helpers above.** In `request_item` and `graphql_item`: `scrub_url` on the url (already there), `scrub_rows` on `query_params` with feature `Query parameter`, `scrub_form_entries` on the body entries, `scrub_header_values` unchanged (rule from plan 1), all before `postman_url` and `body_json`. Reuse one private function for the URL query-pair blanking in both `scrub_url` and the OAuth URL fields. Adjust `variables_json` as described.

- [ ] **Step 5: Update the golden file** by pasting the actual output (with `<uuid>` for the id). Check the diff holds only the new blanked values and the added fixture entries. Run `cargo test -j4 -p rocket-export` and `cargo check -j4 -p rocket-export`. Expected: all pass, no warnings.

- [ ] **Step 6: Commit** `feat(export): blank secret-like rows by name in the Postman export`. Stage the changed `rocket-export` files and the fixture only.

### Task 5: OAuth1 slot, report notes, environments, dialog and docs

**Files:**
- Modify: `crates/rocket-export/src/scrub.rs` (`scrub_auth` OAuth1 `access_token`), `crates/rocket-export/src/postman.rs` (`auth_value`), `crates/rocket-export/src/postman_env.rs`, `src/components/export/ExportCollectionDialog.tsx`, `src/components/export/ExportCollectionDialog.test.tsx`, `.claude/collection-export.md`, `crates/rocket-export/CLAUDE.md`

**Interfaces:**
- Consumes: Task 4 helpers.
- Produces: `scrub_auth` blanks OAuth1 `access_token` (a literal). `auth_value` emits the `Blanked` notes of `scrub_auth` only when the auth is exported, never for `Skipped` auths. `postman_environment` and `environment_notes` keep a lone `{{ref}}` in a secret variable, blank a hydrated literal, and blank a non-secret variable whose name matches the rule (note `Variable`, detail `<key> (by name)`).

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (environments section).** Then read `postman_env.rs:13-60` and its tests, `postman.rs:60-76` (`auth_value`), and the "Secret scrub" sections of `.claude/collection-export.md` and `crates/rocket-export/CLAUDE.md`.

- [ ] **Step 2: Write the failing tests.**
  - `oauth1_access_token_is_blanked` (typed scrub): OAuth1 with `access_token: Some("abc")`. Expect `None`/empty after scrub and one `Blanked` note.
  - `skipped_auth_has_no_blanked_note` (in `postman.rs`): a request with OAuth1 (literal `consumer_secret`) and one with WSSE (literal password). Expect `auth` is `{"type":"noauth"}`, one `Skipped` note each, and no `Blanked` note for either path.
  - `environment_secret_ref_is_kept_and_names_are_scrubbed` (in `postman_env.rs`): secret `A` = `{{ref}}`, secret `B` hydrated `hydrated-value`, plain `api_token=abc`, plain `HOST=x.test`. Expect `A` value `{{ref}}` with `type: secret`, `B` value `""`, `api_token` value `""`, `HOST` unchanged; `environment_notes` lists `B` and `api_token` but not `A`.
  - Dialog: update `states which values are blanked and which are exported as written` to assert the new text contains `query parameters`, `form fields`, `OAuth2`, `variables`, `ends in token`, and still contains `Authorization` and `URL passwords`, and `exported as written` for bodies, scripts and descriptions.

- [ ] **Step 3: Run `cargo test -j4 -p rocket-export` and `yarn test ExportCollectionDialog`.** Expected: the new tests fail.

- [ ] **Step 4: Implement.** Add the `access_token` slot to the OAuth1 branch of `scrub_auth`. In `auth_value`, collect `scrub_auth` notes in a local list and append them to the report only for `Omit` and `Value` outcomes. Change the environment value rule as described. Dialog text: "Auth fields, secret variables, URL passwords, and the values of headers, query parameters, form fields, OAuth2 parameters and variables whose name ends in token, secret, password, key or similar (for example Authorization, X-Api-Key, access_token) are blanked. Variable references are kept. Request bodies, scripts and descriptions are exported as written and are not scanned for secrets." Docs: update the "Secret scrub" lists and the known-losses lines in both files; replace the "secret values are always blanked, even a reference" wording with decision 4; add the name rule, its allow-list and the "userinfo with no `:` is not blanked" note.

- [ ] **Step 5: Run `cargo test -j4 -p rocket-export`, `cargo test -j4 -p rocket-app collection_export`, `cargo check -j4 -p rocket-export`, `yarn test ExportCollectionDialog`, `yarn tsc --noEmit` and `yarn check`.** Expected: all pass, no warnings. Update the Postman golden file only if a test shows an intended change.

- [ ] **Step 6: Commit** `feat(export): finish name-based scrub, dialog text and docs`. Stage the listed files only.
