# Postman Import — Design Spec

> **Type:** Spec (reference only — never executed directly)
> **Date:** 2026-05-02
> **Status:** Approved

---

## Overview

RocketAPI imports Postman collections exported as JSON (Collection Format v2.0 and v2.1), converting them into OpenCollection `.yml` files. The importer lives entirely inside the existing `rocket-import` crate, following the same patterns as the Bruno importer. All disk I/O is delegated to the existing `FsCollectionRepo` and `FsEnvironmentRepo` — no raw YAML is written by any new code.

---

## Goals

| Goal | Priority |
|---|---|
| Import Postman Collection v2.1 JSON into the active workspace | Must have |
| Import Postman Collection v2.0 JSON (legacy — `url` as plain string) | Must have |
| Convert HTTP requests (all methods) | Must have |
| Convert nested folder hierarchy → OpenCollection folder hierarchy | Must have |
| Convert collection-level variables → `CollectionSettings.variables` | Must have |
| Convert collection-level auth | Must have |
| Convert query params, path variables, headers | Must have |
| Convert request body (raw JSON/text/XML, urlencoded, formdata text entries) | Must have |
| Import a separate Postman environment JSON file | Must have |
| Auto-rename on collection name conflict (append `-1`, `-2`) | Must have |
| Skip unsupported features, surface them in `ImportReport.skipped` | Must have |
| Extend `ImportDialog` with Bruno / Postman source-type toggle | Must have |

---

## Out of Scope

- Pre-request and test scripts (Postman JavaScript — incompatible, skipped with report entry)
- OAuth2 auth (skipped with report entry — not yet supported in RocketAPI)
- gRPC, WebSocket, GraphQL request types (skipped with report entry)
- Binary file body mode (skipped with report entry)
- Postman workspace API export
- formdata file-type entries (skipped individually; text entries are imported)

---

## Postman JSON Format

### Collection v2.1 top-level shape

```json
{
  "info": {
    "name": "My API",
    "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
  },
  "item": [ ... ],
  "variable": [ { "key": "baseUrl", "value": "http://localhost", "type": "string" } ],
  "auth": { "type": "bearer", "bearer": [{ "key": "token", "value": "{{token}}" }] }
}
```

### Item — request or folder

```json
// Request
{
  "name": "Get Users",
  "request": {
    "method": "GET",
    "url": {
      "raw": "{{baseUrl}}/users",
      "query": [{ "key": "page", "value": "1", "disabled": false }],
      "variable": [{ "key": "id", "value": "123" }]
    },
    "header": [{ "key": "Accept", "value": "application/json", "disabled": false }],
    "auth": { "type": "bearer", "bearer": [...] },
    "body": {
      "mode": "raw",
      "raw": "{\"name\": \"Alice\"}",
      "options": { "raw": { "language": "json" } }
    }
  }
}

// Folder
{ "name": "Auth", "item": [ ... ], "auth": { ... } }
```

### v2.0 differences

- `info.schema` contains `v2.0.0`
- `url` is a plain string (not an object)

### Variable syntax

Postman uses `{{variableName}}` — identical to RocketAPI. No translation needed.

### Environment JSON

```json
{
  "name": "Local",
  "values": [
    { "key": "baseUrl", "value": "http://localhost:3000", "enabled": true }
  ]
}
```

---

## Architecture

### Module layout (all inside `rocket-import`)

```
crates/rocket-import/src/
  postman/
    mod.rs          — re-exports PostmanCollection, parse_postman_json,
                      PostmanEnvironment, parse_postman_environment
    ast.rs          — serde Deserialize structs mirroring Postman JSON
    parser.rs       — parse_postman_json(path) → ImportResult<PostmanCollection>
    env_parser.rs   — parse_postman_environment(path) → ImportResult<PostmanEnvironment>
  converter/
    postman.rs      — pure conversion functions: PostmanAST → domain types
  importer.rs       — extend ImportService with two new public methods
  error.rs          — extend with NotAPostmanCollection + JsonParseError
```

No new crate. No new Cargo workspace member.

### Dependency rule compliance

```
rocket-import → rocket-collection (CollectionRepository trait)
rocket-import → rocket-environment (EnvironmentRepository trait)
rocket-import → rocket-shared (Auth, Header, etc.)
rocket-import → rocket-infra (FsCollectionRepo, FsEnvironmentRepo — test only via new_with_workspace_path)
```

This is identical to how the Bruno importer is wired. The dependency arrows are unchanged.

### Data flow

```
Postman JSON file
  → postman::parse_postman_json / parse_postman_environment
  → PostmanCollection / PostmanEnvironment (AST)
  → converter::postman::{convert_auth, convert_headers, convert_query_params,
                          convert_path_variables, convert_body, convert_collection_variables,
                          convert_request_item}
  → domain types (Request, CollectionSettings, Environment)
  → self.collection_repo.save_request / save_settings / create_folder
  → self.env_factory.make(col_name).save(env)
  → ~/.rocket-api/workspaces/<ws>/collections/<name>/
```

### Postman AST structs (`postman/ast.rs`)

```rust
pub(crate) struct PostmanCollection {
    pub info: PostmanInfo,          // name + schema
    pub item: Vec<PostmanItem>,
    pub variable: Vec<PostmanVariable>,
    pub auth: Option<PostmanAuth>,
}

// PostmanItem is #[serde(untagged)]: Folder (has `item`) tried first, then Request
pub(crate) enum PostmanItem {
    Folder(PostmanFolder),
    Request(PostmanRequestItem),
}

// PostmanUrl is #[serde(untagged)]: Object tried first, then String
pub(crate) enum PostmanUrl {
    Object(PostmanUrlObject),   // v2.1
    String(String),             // v2.0
}
impl PostmanUrl {
    pub fn raw(&self) -> &str { ... }
    pub fn query_params(&self) -> &[PostmanQueryParam] { ... }
    pub fn path_variables(&self) -> &[PostmanPathVariable] { ... }
}
```

### Conversion rules

| Postman | OpenCollection |
|---|---|
| `info.name` | Collection name |
| `item[]` (folder) | `create_folder` via repo |
| `item[]` (request) | `save_request` via repo |
| `variable[]` | `CollectionSettings.variables` |
| `auth.type = "bearer"` | `Auth::Bearer { token }` |
| `auth.type = "basic"` | `Auth::Basic { username, password }` |
| `auth.type = "apikey"` | `Auth::ApiKey { key, value, in_ }` |
| `auth.type = "noauth"` | `Auth::None` |
| `auth.type = "oauth2"` | `None` + `SkipReason::UnsupportedAuthType("oauth2")` |
| `header[]` | `http.headers[]` (disabled → `enabled: false`) |
| `url.query[]` | `http.params[]` with `param_type: "query"` |
| `url.variable[]` | `http.params[]` with `param_type: "path"` |
| `body.mode = "raw" + language = "json"` | `Body::Raw { body_type: "json", data }` |
| `body.mode = "raw" + language = "xml"` | `Body::Raw { body_type: "xml", data }` |
| `body.mode = "raw"` (other) | `Body::Raw { body_type: "text", data }` |
| `body.mode = "urlencoded"` | `Body::FormUrlEncoded { data }` |
| `body.mode = "formdata"` text entries | `Body::MultipartForm { data }` |
| `body.mode = "formdata"` file entries | Skipped individually — `SkipReason::UnsupportedRequestType("formdata-file-entry")` |
| `body.mode = "file"` | `None` + `SkipReason::UnsupportedRequestType("file-body")` |
| `request.description` | `request.description` (string) |
| env `values[]` | `Variable::new(key, value)` with `.enabled = v.enabled` |

### ImportService additions

```rust
// In crates/rocket-import/src/importer.rs
pub fn import_postman_collection(
    &self,
    json_path: &Path,
    workspace_id: &str,
) -> ImportResult<ImportReport>

pub fn import_postman_environment(
    &self,
    json_path: &Path,
    collection_name: &str,
    workspace_id: &str,
) -> ImportResult<ImportReport>
```

Both use `self.collection_repo` (already a field on `ImportService`) and `self.env_factory` (already a field) — no new constructor parameters.

### IPC commands (rocket-app)

```rust
#[tauri::command]
async fn import_postman_collection(path: String, target_workspace_id: String, ...) -> Result<ImportReport, String>

#[tauri::command]
async fn import_postman_environment(json_path: String, collection_name: String, target_workspace_id: String, ...) -> Result<ImportReport, String>
```

### Frontend changes

`ImportDialog.tsx` gains a source-type toggle (Bruno / Postman). When Postman is selected:
- File picker filters to `.json` files (not directory)
- Optional second picker for Postman environment JSON
- `handleImport` calls `importPostmanCollection` (and optionally `importPostmanEnvironment`)
- Dialog close resets source type back to Bruno

The existing `ImportReport` TypeScript type is shared — no new types needed.

---

## Error handling

| Situation | Result |
|---|---|
| File is not valid JSON | `ImportError::JsonParseError` |
| JSON has no Postman schema string | `ImportError::NotAPostmanCollection` |
| Per-request parse failure | Request skipped, `SkippedItem` added |
| Unsupported body mode | `None` body + `SkippedItem` |
| Unsupported auth type | `None` auth + `SkippedItem` |
| Collection name conflict | Auto-rename: `my-api-1`, `my-api-2` (via existing `resolve_collection_name`) |

---

## File Map

| File | Action |
|---|---|
| `crates/rocket-import/src/error.rs` | Modify — add `NotAPostmanCollection`, `JsonParseError` |
| `crates/rocket-import/src/postman/mod.rs` | Create |
| `crates/rocket-import/src/postman/ast.rs` | Create |
| `crates/rocket-import/src/postman/parser.rs` | Create |
| `crates/rocket-import/src/postman/env_parser.rs` | Create |
| `crates/rocket-import/src/converter/mod.rs` | Modify — add `postman` sub-module |
| `crates/rocket-import/src/converter/postman.rs` | Create |
| `crates/rocket-import/src/importer.rs` | Modify — add two public methods |
| `crates/rocket-import/src/lib.rs` | Modify — expose `postman` module |
| `crates/rocket-import/Cargo.toml` | Modify — add `serde_json` |
| `crates/rocket-import/tests/fixtures/postman/*.json` | Create (4 fixture files) |
| `crates/rocket-import/tests/postman_integration_test.rs` | Create |
| `crates/rocket-app/src/import_commands.rs` | Modify — add 2 Tauri commands |
| `crates/rocket-app/src/lib.rs` | Modify — register 2 new commands |
| `src/lib/tauri-api.ts` | Modify — add 2 typed bindings |
| `src/components/imports/ImportDialog.tsx` | Modify — source toggle + Postman pickers |

---

## Testing strategy

### Rust unit tests (inline in each module)

- `postman/ast.rs` — parse minimal JSON, parse folder with nested requests, parse bearer auth, parse collection variables, parse v2.0 string URL
- `postman/parser.rs` — parse from disk (3 fixtures), reject non-Postman JSON, reject invalid JSON
- `postman/env_parser.rs` — parse environment.json, reject invalid JSON
- `converter/postman.rs` — convert each auth type, headers, query params, path variables, all body modes

### Integration tests (`tests/postman_integration_test.rs`)

- Import minimal collection → correct request count and files
- Import full collection with folders → skipped items for file body
- Import v2.0 collection → correct request count
- Auto-rename on conflict
- Reject non-Postman JSON
- Import environment into existing collection

### Fixture files

```
crates/rocket-import/tests/fixtures/postman/
  minimal-collection.json   — 2 requests, no auth, no variables
  full-collection.json      — 2 folders (Users/Auth), auth, collection vars, all body types
  v2.0-collection.json      — 1 request, url as plain string
  environment.json          — 3 variables (1 disabled)
```

### Manual smoke test

After all three plans execute: import `full-collection.json` via File → Import → Postman, verify folder hierarchy, request fields, collection variables, and auto-rename on second import.
