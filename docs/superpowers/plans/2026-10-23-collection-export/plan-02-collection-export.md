# Collection export: Postman collection serializer Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** The Export dialog can save a Postman Collection v2.1 JSON file for a collection, with correct URLs, headers, bodies, auth and variables, and a report of everything skipped or blanked.

**Architecture:** `rocket-export` gains a typed `CollectionExportTree` and a pure `postman_collection` serializer. `CollectionExportService` builds the tree through `CollectionRepository` (strict reads) and a new Tauri command returns the JSON bytes. Scripts and environments come in plan 3.

**Tech Stack:** Rust (serde_json), Tauri 2, React, TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-23-collection-export-design.md`

## Global Constraints

- `info.schema` is exactly `https://schema.getpostman.com/json/collection/v2.1.0/collection.json`; `info._postman_id` is a new UUID v4; `info.name` is the collection name; `info.description` is the collection docs.
- Items: folders first, then requests; each group ordered by `seq` (missing `seq` last) then by name; sibling name collisions get ` (2)`, ` (3)`.
- `url.raw` holds only enabled query pairs; `url.query` lists every pair with `"disabled": true` on disabled ones; path params are `:id` in `raw` and `path` plus `url.variable`. A URL with no `scheme://` gets `raw`, `query` and `variable` only, with no `protocol`, `host` or `path`.
- Collection and folder headers are copied onto each request (outermost first, inner key wins, disabled never shadows), using `rocket_collection::inherited_headers`.
- Auth key names follow the table in the spec (section 3). `Inherit` omits `auth`; `None` emits `{"type":"noauth"}`; OAuth1 and Wsse emit `noauth` and a `Skipped` note.
- Literal credentials, sensitive header values, URL passwords and secret variable values are blanked with the same rules as plan 1 (`{{variable}}` references are kept).
- Request examples, gRPC, WebSocket, opaque items and flows are never exported; each gets a `Skipped` note. `ScriptFile` items are ignored without a note.
- Same layering, serde and cargo rules as plan 1. Cargo commands per crate with `-j4`.

## Review Focus

1. A disabled query param stays disabled and is absent from `raw`: pinned in Task 1 by `disabled_query_param_stays_disabled_and_out_of_raw`.
2. `{{baseUrl}}/users/:id` (no scheme) exports without a broken `host` or `path` split: pinned in Task 1 by `url_without_scheme_keeps_only_raw_query_and_variable`.
3. Two siblings both named `List` become `List` and `List (2)`: pinned in Task 1 by `duplicate_sibling_names_get_suffix`.
4. A request with `auth: inherit` under a folder with bearer auth exports no `auth` of its own, and the folder carries the bearer block: pinned in Task 2 by `inherit_omits_auth_and_folder_carries_it`.
5. A collection with a failing request read fails the whole export instead of silently dropping the request: pinned in Task 3 by `build_tree_fails_when_a_request_cannot_be_read`.

## File Structure

- Create `crates/rocket-export/src/{tree.rs,postman.rs,postman_auth.rs}`, `crates/rocket-export/tests/postman_golden.rs`, `crates/rocket-export/tests/fixtures/postman-golden.json`
- Modify `crates/rocket-export/Cargo.toml` (add `rocket-collection`, `uuid`), `crates/rocket-export/src/{lib.rs,scrub.rs}`
- Modify `crates/rocket-app/src/collection_export_service.rs`
- Modify `src-tauri/src/commands/export.rs`, `src-tauri/src/lib.rs`
- Modify `src/lib/tauri-api.ts`, `src/components/export/ExportCollectionDialog.tsx` and its test

---

### Task 1: Tree types, items, URLs, headers and bodies

**Files:**
- Create: `crates/rocket-export/src/tree.rs`, `crates/rocket-export/src/postman.rs`
- Modify: `crates/rocket-export/Cargo.toml`, `crates/rocket-export/src/lib.rs`

**Interfaces:**
- Consumes: `ExportReport`, `ExportNote`, `ExportAction` (plan 1).
- Produces:
  - `struct CollectionExportTree { name: String, settings: CollectionSettings, root: ExportFolder }`
  - `struct ExportFolder { name: String, settings: FolderSettings, items: Vec<ExportItem> }`
  - `enum ExportItem { Request(Box<Request>), GraphQl(Box<GraphQlRequest>), Folder(ExportFolder), Skipped { path: String, kind: String } }`
  - `struct PostmanOptions { preserve_scripts: bool }` (`Default`: false)
  - `fn postman_collection(tree: &CollectionExportTree, opts: &PostmanOptions) -> DomainResult<(serde_json::Value, ExportReport)>`
  - `fn postman_url(url: &str, query: &[QueryParam], path_params: &[PathParam]) -> serde_json::Value`

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.** Then read `crates/rocket-collection/src/{folder.rs,request.rs,graphql_request.rs,folder_settings.rs}`, `crates/rocket-shared/src/types.rs` (`Header`, `PathParam`, `Body`, `FormDataEntry`) and `src/lib/curl-generator.ts` (`buildUrlWithQuery` as the reference for how Rocket joins `url` and `queryParams`).

- [ ] **Step 2: Write the failing tests in `postman.rs`.** Helpers: `req(name: &str, url: &str) -> Request` built with `Request::new(name, HttpMethod::Get, url)`, and `tree_with(items: Vec<ExportItem>) -> CollectionExportTree` (name `demo`, default settings). Example:

```rust
#[test]
fn disabled_query_param_stays_disabled_and_out_of_raw() {
    let mut r = req("List", "https://api.example.com/users");
    r.query_params = vec![Header::new("page", "1"), Header::disabled("debug", "x")];
    let (doc, _) = postman_collection(
        &tree_with(vec![ExportItem::Request(Box::new(r))]),
        &PostmanOptions::default(),
    )
    .expect("export builds");
    let url = &doc["item"][0]["request"]["url"];
    assert_eq!(url["raw"], "https://api.example.com/users?page=1");
    assert_eq!(url["query"][1]["key"], "debug");
    assert_eq!(url["query"][1]["disabled"], true);
    assert_eq!(url["host"], serde_json::json!(["api", "example", "com"]));
    assert_eq!(url["path"], serde_json::json!(["users"]));
}
```

| Test name | Input | Expected |
|---|---|---|
| `info_block_has_v2_1_schema_and_uuid` | any tree, collection docs `hello` | `info.schema` equals the constant, `_postman_id` parses as a UUID, `description == "hello"` |
| `url_without_scheme_keeps_only_raw_query_and_variable` | `{{baseUrl}}/users/:id`, path param `id=42` | no `protocol`/`host`/`path` keys; `variable == [{"key":"id","value":"42"}]` |
| `path_param_uses_colon_and_variable` | `https://x.test/users/:id` with path param | `path == ["users", ":id"]`, `variable` has `id` |
| `duplicate_sibling_names_get_suffix` | two requests `List`, two folders `Users` | `List`, `List (2)`, `Users`, `Users (2)` |
| `items_order_folders_first_then_seq_then_name` | requests seq 2, seq 1, none; one folder | folder, seq 1, seq 2, none |
| `folder_and_collection_headers_are_copied_onto_requests` | collection header `A=1`, folder header `A=2`, request header `B=3` | request headers `A=2`, `B=3`; disabled folder header does not shadow |
| `raw_body_sets_language_and_content_type` | json, xml, text, sparql bodies | `options.raw.language` is json, xml, text, text; a `Content-Type` header is added only when absent |
| `urlencoded_and_formdata_bodies` | rows incl. a disabled row and a file entry | `urlencoded` rows keep `disabled`; file entry is `{"type":"file","src":...}` |
| `binary_body_uses_file_mode` | `Body{mode: Binary, file_path: "a.bin"}` | `body == {"mode":"file","file":{"src":"a.bin"}}` |
| `graphql_request_uses_graphql_body` | `GraphQlRequest` with query and variables `{"a":1}` | `body.graphql.query` equals the query; `variables` is the string `{"a":1}` |
| `skipped_items_are_reported_and_absent` | `ExportItem::Skipped{path:"ws/chat", kind:"websocket"}` | not in `item`; one `Skipped` note with that path |
| `request_method_and_description` | method POST, description `docs` | `request.method == "POST"`, `request.description` set |

- [ ] **Step 3: Run `cargo test -j4 -p rocket-export postman`.** Expected: compile errors `cannot find type 'CollectionExportTree' in this scope`, zero passing.

- [ ] **Step 4: Implement the types and functions above.** Add `rocket-collection` and `uuid` (workspace) to `Cargo.toml`. Build values with `serde_json::json!`; sort with a stable sort on `(seq.unwrap_or(u32::MAX), name)`; the recursion carries a `Vec<FolderSettings>` chain so `inherited_headers(&tree.settings.headers, &chain)` is computed per request. Use the Rust header-scrub helper added in Task 2; until then pass values through.

- [ ] **Step 5: Run `cargo test -j4 -p rocket-export postman` and `cargo check -j4 -p rocket-export`.** Expected: 12 tests pass, no warnings.

### Task 2: Auth mapping, typed scrub, variables and the golden file

**Files:**
- Create: `crates/rocket-export/src/postman_auth.rs`, `crates/rocket-export/tests/postman_golden.rs`, `crates/rocket-export/tests/fixtures/postman-golden.json`
- Modify: `crates/rocket-export/src/{postman.rs,scrub.rs,lib.rs}`

**Interfaces:**
- Consumes: Task 1 types.
- Produces:
  - `fn scrub_auth(auth: &Auth, path: &str, notes: &mut Vec<ExportNote>) -> Auth` (blank credential fields, keep references)
  - `fn scrub_header_values(headers: Vec<Header>, path: &str, notes: &mut Vec<ExportNote>) -> Vec<Header>`
  - `fn scrub_url(url: &str, path: &str, notes: &mut Vec<ExportNote>) -> String`
  - `fn postman_auth(auth: &Auth) -> PostmanAuth` where `enum PostmanAuth { Omit, Value(serde_json::Value), Skipped(&'static str) }`

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (auth sections).** Then read `crates/rocket-shared/src/types.rs:225-310` (`Auth`, `OAuth1Auth`), `crates/rocket-shared/src/oauth2.rs` (`OAuth2Flow` and its structs), and the spec's auth table (section 3).

- [ ] **Step 2: Write the failing tests.** In `postman_auth.rs` (`#[cfg(test)]`), one case per row, example:

```rust
#[test]
fn bearer_maps_to_token_key() {
    let out = postman_auth(&Auth::Bearer { token: "{{t}}".into() });
    assert_eq!(
        out,
        PostmanAuth::Value(serde_json::json!({
            "type": "bearer",
            "bearer": [{"key": "token", "value": "{{t}}", "type": "string"}]
        }))
    );
}
```

| Test name | Input | Expected |
|---|---|---|
| `basic_digest_ntlm_keys` | Basic, Digest, Ntlm | key arrays `username`, `password` (`domain` for ntlm) |
| `apikey_maps_in_header_or_query` | placement `header`, `query` | `in` is `header`, `query` |
| `awsv4_keys` | AwsSigV4 with session token | `accessKey`, `secretKey`, `sessionToken`, `service`, `region` |
| `oauth2_grant_type_names` | the four flows, authorization code with and without PKCE | `client_credentials`, `password_credentials`, `authorization_code`, `authorization_code_with_pkce`, `implicit` |
| `oauth2_places_token_and_client_auth` | placement body and basic header; token in header and url | `client_authentication` `body`/`header`; `addTokenTo` `header`/`queryParams` |
| `inherit_none_and_unsupported` | Inherit, None, OAuth1, Wsse | `Omit`, noauth value, `Skipped("oauth1")`, `Skipped("wsse")` |
| `scrub_auth_blanks_literals_and_keeps_references` | bearer `abc`, basic password `{{pw}}` | `""`, `{{pw}}` kept; one `Blanked` note for the first |
| `scrub_header_values_blanks_sensitive_literals` | `Authorization: abc`, `X-Id: abc`, `Cookie: {{c}}` | only the first is blanked |
| `scrub_url_blanks_password` | `https://u:p@h/x`, `https://u:{{p}}@h/x` | `https://u:@h/x`, unchanged |

In `postman.rs` add: `inherit_omits_auth_and_folder_carries_it`, `collection_variables_keep_values_and_blank_secrets` (collection variables `a=1` and secret `s=top`; `variable[]` has `a` value `1` and `s` value `""`), `collection_and_folder_auth_blocks`, `examples_are_skipped_with_note`, `oauth1_request_gets_noauth_and_note`. In `tests/postman_golden.rs`:

```rust
const GOLDEN: &str = include_str!("fixtures/postman-golden.json");

#[test]
fn full_collection_matches_golden() {
    let (doc, _) = postman_collection(&full_tree(), &PostmanOptions::default()).expect("export builds");
    let mut actual = serde_json::to_string_pretty(&doc).expect("json");
    // _postman_id is random, so the fixture stores a fixed marker.
    actual = actual.replace(doc["info"]["_postman_id"].as_str().expect("id"), "<uuid>");
    assert_eq!(actual.trim(), GOLDEN.trim());
}
```

`full_tree()` builds one collection using every body mode, every supported auth type, folder auth, collection variables, a disabled header and a skipped websocket item.

- [ ] **Step 3: Run `cargo test -j4 -p rocket-export` with the golden fixture file created empty.** Expected: the new unit tests fail to compile (`cannot find function 'postman_auth'`); after the stubs compile, `full_collection_matches_golden` fails and prints the actual JSON.

- [ ] **Step 4: Implement the functions.** Wire `scrub_auth`, `scrub_header_values` and `scrub_url` into `postman_collection` for every request and for collection and folder auth. After the unit tests pass, paste the printed actual JSON into `tests/fixtures/postman-golden.json` (with `<uuid>` in place of the id), re-run, and confirm it passes on unchanged code.

- [ ] **Step 5: Run `cargo test -j4 -p rocket-export` and `cargo check -j4 -p rocket-export`.** Expected: all tests pass (about 35 in total for the crate), no warnings.

### Task 3: Export service, command and dialog option

**Files:**
- Modify: `crates/rocket-app/src/collection_export_service.rs`, `src-tauri/src/commands/export.rs`, `src-tauri/src/lib.rs`, `src/lib/tauri-api.ts`, `src/components/export/ExportCollectionDialog.tsx`, `src/components/export/ExportCollectionDialog.test.tsx`

**Interfaces:**
- Consumes: `postman_collection`, `CollectionExportTree`, plan 1's `CollectionExportService`.
- Produces:
  - `CollectionExportService::with_collections(self, repo: Box<dyn CollectionRepository>) -> Self`
  - `fn build_tree(&self, collection: &str) -> DomainResult<CollectionExportTree>`
  - `fn export_postman(&self, collection: &str) -> DomainResult<ExportOutput>` (file name `<slug>.postman_collection.json`)
  - `pub fn slug(name: &str) -> String` in `crates/rocket-export/src/names.rs` (lowercase, each run of non-alphanumeric characters becomes one `-`, leading and trailing `-` trimmed, an empty result becomes `export`); test `slug_cases`: `My API` gives `my-api`, `  A/B  ` gives `a-b`, `!!!` gives `export`
  - Tauri: `export_collection_postman(collection: String, workspace_path: State<'_, Arc<Mutex<PathBuf>>>) -> Result<ExportResultDto, DomainError>`
  - TS: `exportCollectionPostman = (collection: string) => invoke<ExportResult>('export_collection_postman', { collection })`

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.** Then read the `CollectionRepository` signatures (`get_strict`, `get_settings`, `get_folder_settings`; folder paths are relative to the collection root and `""` is the root), `crates/rocket-app/src/test_doubles.rs` (`NullCollectionRepo` and its `on_*` hooks), and how `src-tauri/src/lib.rs` builds `SharedPathCollectionRepo::new(Arc::clone(&active_workspace_path))`.

- [ ] **Step 2: Write the failing tests.** In `collection_export_service.rs`, using `NullCollectionRepo` hooks (`on_get_strict`, `on_get_settings`, `on_get_folder_settings`):

```rust
#[test]
fn build_tree_fails_when_a_request_cannot_be_read() {
    let repo = NullCollectionRepo::strict().on_get_strict(|_| {
        Err(DomainError::Internal("bad request file".into()))
    });
    let svc = CollectionExportService::new(Box::new(EmptySource)).with_collections(Box::new(repo));
    let err = svc.build_tree("demo").expect_err("strict read error surfaces");
    assert!(err.to_string().contains("bad request file"));
}
```

| Test name | Input | Expected |
|---|---|---|
| `build_tree_reads_folder_settings_per_folder` | collection with folder `users` (`dir_name` `users-dir`) | `get_folder_settings` is called with `users-dir` and nested folders with `a/b` joined by `/` |
| `build_tree_maps_item_kinds` | Request, GraphQl, WebSocket, Grpc, OpaqueItem, ScriptFile, Summary | Request and GraphQl kept; WebSocket, Grpc and Opaque become `Skipped` with kind names; ScriptFile dropped; a Summary item is an `Err(Internal)` |
| `export_postman_names_the_file` | empty collection `My API` | `file_name == "my-api.postman_collection.json"`, valid JSON bytes |
| `export_postman_without_repository_is_an_error` | service without `with_collections` | `Err(DomainError::Internal(..))` |

Frontend: in the dialog test add `postman option calls exportCollectionPostman and saves a .json file` (select the Postman option, click Export; expects `exportCollectionPostman('demo')` and `saveBinaryFile` called with a name ending `.postman_collection.json` and a JSON filter) and `shows a note that Postman environments are not included yet` (when the Postman option is selected, the Include environments checkbox is hidden and the dialog says "Environments are exported in a later version"; plan 3 removes both).

- [ ] **Step 3: Run `cargo test -j4 -p rocket-app collection_export` and `yarn test ExportCollectionDialog`.** Expected: missing method errors in Rust; the new dialog tests fail because the Postman option does not exist.

- [ ] **Step 4: Implement.** `build_tree` walks `Collection.root` recursively, calling `get_settings` once and `get_folder_settings` per folder, mapping `CollectionItem` variants as in the table. `export_postman` serialises with `serde_json::to_vec_pretty`. The command builds the service per call with `SharedPathCollectionRepo::new(Arc::clone(&*workspace_path))` and registers in `generate_handler!`. The dialog's `Select` gains `Postman collection (v2.1)`; `exportCollectionPostman` is added to `tauri-api.ts`.

- [ ] **Step 5: Run `cargo test -j4 -p rocket-app collection_export`, `cargo check -j4 -p <src-tauri package>`, `yarn test ExportCollectionDialog`, `yarn tsc --noEmit`, `yarn check`.** Expected: 4 Rust tests and the updated dialog tests pass, no type, lint or Rust errors.

Next plan: docs/superpowers/plans/2026-10-23-collection-export/plan-03-collection-export.md
