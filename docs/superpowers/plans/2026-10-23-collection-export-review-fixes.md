# Collection export: review fixes (Important items) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the five Important findings from the plan-1 and plan-2 code reviews of the collection export: wrong query handling in the Postman URL, folder `None` auth, case-sensitive header override, dropped folder and request variables, and the dialog text that over-promises what the scrub covers.

**Architecture:** All Postman fixes are inside `crates/rocket-export/src/postman.rs` (pure logic). The dialog fix is one string plus a test. No new crates, files or IPC changes.

**Tech Stack:** Rust (serde_json), React, TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-23-collection-export-design.md`. Source reviews: plan-1 review (Important #1) and plan-2 review (Important #1 to #4). Minor findings are tracked in GitHub issues, not here.

## Global Constraints

- Same layering, serde and cargo rules as `plan-01-collection-export.md`: domain logic only in `rocket-export`, no panicking unwrap calls in production paths, tests use `.expect("...")`, cargo per crate with `-j4`, never a workspace-wide run.
- Runtime source of truth for URL and query: `src/lib/execute-request.ts` (the url bar holds the query; a typed query is sent once, either in the url or as params; rows need `enabled && key`). `curl-generator.ts` is not the reference.
- `rocket_collection::resolve_folder_auth` treats `None`, `Some(Auth::None)` and `Some(Auth::Inherit)` on a folder as "no folder auth". The export must match.
- `{{variable}}` references are kept; secret literals are blanked with the existing rules.
- The golden file `crates/rocket-export/tests/fixtures/postman-golden.json` changes only by pasting actual output after an intended change (see `crates/rocket-export/CLAUDE.md`).
- This worktree has no `node_modules`. Run `yarn install` once before any `yarn` command.

## Review Focus

1. A Rocket-saved request whose query lives only in `url` (`https://x/users?page=1`, empty `query_params`) exports `url.query == [{"key":"page","value":"1"}]`: pinned in Task 1 by `rocket_shaped_url_query_becomes_url_query_rows`.
2. A pair present in both the url and `query_params` appears once in `raw` and once in `query`: pinned in Task 1 by `typed_and_table_duplicate_pair_appears_once`.
3. A query row with an empty key is dropped, as the runtime does: pinned in Task 1 by `empty_key_query_rows_are_skipped`.
4. A folder with `Auth::None` under a collection with bearer auth exports no `auth` on the folder: pinned in Task 2 by `folder_with_none_auth_omits_auth_block`.
5. A collection `Authorization` header and a request `authorization` header produce one header: pinned in Task 2 by `request_header_overrides_inherited_case_insensitively`.

---

### Task 1: Postman URL query from the url bar

**Files:**
- Modify: `crates/rocket-export/src/postman.rs` (`postman_url`, tests module)
- Modify: `crates/rocket-export/tests/fixtures/postman-golden.json`

**Interfaces:**
- Consumes: `QueryParam`, `PathParam` (unchanged).
- Produces: `pub fn postman_url(url: &str, query: &[QueryParam], path_params: &[PathParam]) -> Value` keeps its signature. New behavior: `url.query` lists the url's own enabled query pairs first, then every `query_params` row that is not already one of them; `raw` holds each enabled pair once.

- [ ] **Step 1: Write the failing tests in `postman.rs`.**

```rust
#[test]
fn rocket_shaped_url_query_becomes_url_query_rows() {
    let url = postman_url("https://x.test/users?page=1&sort=name", &[], &[]);
    assert_eq!(url["raw"], "https://x.test/users?page=1&sort=name");
    assert_eq!(
        url["query"],
        serde_json::json!([
            {"key": "page", "value": "1"},
            {"key": "sort", "value": "name"}
        ])
    );
}

#[test]
fn typed_and_table_duplicate_pair_appears_once() {
    let q = vec![Header::new("page", "1"), Header::disabled("debug", "x")];
    let url = postman_url("https://x.test/users?page=1", &q, &[]);
    assert_eq!(url["raw"], "https://x.test/users?page=1");
    assert_eq!(url["query"].as_array().expect("query array").len(), 2);
    assert_eq!(url["query"][1]["key"], "debug");
    assert_eq!(url["query"][1]["disabled"], true);
}

#[test]
fn empty_key_query_rows_are_skipped() {
    let q = vec![Header::new("", "orphan"), Header::new("a", "1")];
    let url = postman_url("https://x.test/p?=skip&b=2", &q, &[]);
    assert_eq!(url["raw"], "https://x.test/p?b=2&a=1");
    assert_eq!(url["query"].as_array().expect("query array").len(), 2);
}
```

Reuse the row construction of the existing `disabled_query_param_stays_disabled_and_out_of_raw` test for the `QueryParam` values.

- [ ] **Step 2: Run `cargo test -j4 -p rocket-export postman_url rocket_shaped typed_and_table empty_key`.** Expected: the three new tests fail (`query` is missing or `raw` has a duplicate pair).

- [ ] **Step 3: Rework the query part of `postman_url`.** Split `url` at the first `?`, drop a `#fragment` from the query text, split the query on `&`, split each pair on the first `=` (no `=` means an empty value), skip empty keys and empty segments. Build `rows` as: typed pairs (enabled), then each `query` row with a non-empty key that does not equal an already-listed `(key, value)` pair (keeping its `enabled` flag). `raw` is `base` plus `?` plus the enabled rows joined as `key=value` with `&`. Keep the existing scheme, host, port, path and variable logic. Emit `query` only when `rows` is not empty, using `header_json`.

- [ ] **Step 4: Run `cargo test -j4 -p rocket-export`.** Expected: the three new tests and all earlier tests pass except `full_collection_matches_golden`, which prints the new actual JSON.

- [ ] **Step 5: Regenerate the golden file.** Paste the printed actual JSON (with `<uuid>` in place of the id) into `tests/fixtures/postman-golden.json`. Check the diff: only `url.query` rows gain the typed pairs, for example `x=1` before `page=1`. Re-run `cargo test -j4 -p rocket-export` and `cargo check -j4 -p rocket-export`. Expected: all pass, no warnings.

- [ ] **Step 6: Commit** with the `dev-workflow-skills:1-git-commit` skill: `fix(export): keep the url query in Postman url.query`. Add explicit paths only (`postman.rs` and the golden fixture).

### Task 2: Folder `None` auth, header case, folder and request variables

**Files:**
- Modify: `crates/rocket-export/src/postman.rs` (`folder_items`, `merged_headers`, `variables_json`, `request_item`, `graphql_item`, tests module)
- Modify: `crates/rocket-export/tests/fixtures/postman-golden.json`

**Interfaces:**
- Consumes: `CollectionVariable` (fields `key`, `value`, `initial_value`, `secret`, `enabled`), `FolderSettings.variables`, `Request.variables`, `GraphQlRequest.variables`.
- Produces: `fn variables_json(vars: &[CollectionVariable], path: &str, report: &mut ExportReport) -> Vec<Value>` replacing the tree-based version (the collection call site passes `&tree.settings.variables` and `"(collection)"`). Folder objects and request items gain a `variable` array when their list is not empty.

- [ ] **Step 1: Write the failing tests in `postman.rs`.**
  - `folder_with_none_auth_omits_auth_block`: collection auth `Bearer { token: "{{t}}" }`, a folder with `auth: Some(Auth::None)` holding one request with `Auth::Inherit`. Assert `doc["item"][0]` has no `auth` key and `doc["auth"]["type"] == "bearer"`.
  - `request_header_overrides_inherited_case_insensitively`: collection header `Authorization=Bearer {{a}}`, request header `authorization=Bearer {{b}}`. Assert the request's `header` array has length 1 and its value is `Bearer {{b}}`.
  - `folder_and_request_variables_are_exported`: a folder with variables `a=1` and secret `s=top`, and a request with variable `r=2`. Assert the folder's `variable` has `a` value `1` and `s` value `""`, the request item's `variable` has `r` value `2`, and the report holds one `Blanked` note with `feature == "Secret variable"` and a path that starts with the folder name.

- [ ] **Step 2: Run `cargo test -j4 -p rocket-export folder_with_none request_header_overrides folder_and_request_variables`.** Expected: all three fail.

- [ ] **Step 3: Implement.**
  - In `folder_items`, treat `Some(Auth::None)` like `Inherit` for the folder's own `auth` block (skip `auth_value`). Request-level `Auth::None` keeps emitting `noauth`.
  - In `merged_headers`, compare keys with `eq_ignore_ascii_case`.
  - Change `variables_json` to the signature above, using `path` in the note. Call it for the collection, for each folder (`f.settings.variables`, path `child_path`) and for each request and GraphQL request (`r.variables`, the item path). Insert `variable` only when the result is not empty.

- [ ] **Step 4: Run `cargo test -j4 -p rocket-export` and update the golden file** the same way as Task 1 Step 5 if `full_collection_matches_golden` fails. If `full_tree()` has no folder or request variables, add one of each to it first so the golden file covers them. Expected: all pass, no warnings.

- [ ] **Step 5: Run `cargo check -j4 -p rocket-app`.** Expected: no errors (the tree builder is unchanged).

- [ ] **Step 6: Commit** with the `dev-workflow-skills:1-git-commit` skill: `fix(export): match Rocket folder auth, header and variable semantics in Postman`. Explicit paths only.

### Task 3: Dialog text states what is blanked

**Files:**
- Modify: `src/components/export/ExportCollectionDialog.tsx` (the `DialogDescription`, about lines 89-93)
- Modify: `src/components/export/ExportCollectionDialog.test.tsx`
- Modify: `docs/features/collection-export.md` (only if it repeats the old sentence)

**Interfaces:**
- No signature changes. Decision: the plan-1 review offered two fixes (exact text, or a wider scrub). This plan takes the text fix. A wider name-based scrub for params, other token headers and OAuth2 `additionalParameters` is tracked in the Minor GitHub issue for a separate decision.

- [ ] **Step 1: Run `yarn install`** (the worktree has no `node_modules`). Expected: completes without errors.

- [ ] **Step 2: Write the failing test** `states which values are blanked and which are exported as written`: render the dialog open and assert the description contains `Authorization`, `Cookie`, `X-Api-Key` and `URL passwords`, and contains `query parameters` followed by `exported as written`.

- [ ] **Step 3: Run `yarn test ExportCollectionDialog`.** Expected: the new test fails.

- [ ] **Step 4: Replace the description text** with: "Auth fields, secret variables, the Authorization, Proxy-Authorization, Cookie, Set-Cookie and X-Api-Key headers, and URL passwords are blanked. Variable references are kept. Other headers, query parameters, request bodies, scripts and descriptions are exported as written and are not scanned for secrets." Update `docs/features/collection-export.md` if it repeats the old claim.

- [ ] **Step 5: Run `yarn test ExportCollectionDialog`, `yarn tsc --noEmit` and `yarn check`.** Expected: all pass with no errors.

- [ ] **Step 6: Commit** with the `dev-workflow-skills:1-git-commit` skill: `fix(frontend): state exactly what the export dialog blanks`. Explicit paths only.
