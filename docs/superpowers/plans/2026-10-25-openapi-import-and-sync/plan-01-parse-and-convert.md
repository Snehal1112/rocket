# OpenAPI Parse and Convert Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `ImportService::import_openapi_text` turns an OpenAPI 3.0, 3.1 or Swagger 2.0 document (JSON or YAML text) into a new collection with folders by tag or by path, and returns what is needed to track each request later.

**Architecture:** A pure reader in `rocket-import/src/openapi/` normalises all three spec versions into one small neutral model. A converter maps each operation to a `rocket_collection::Request`. The import service lays out folders and writes through `CollectionRepository`, like `import_wsdl` does. No network, no scheduler, no UI in this plan.

**Tech Stack:** Rust, `rocket-import`, `serde_yaml` and `serde_json` (already dependencies), `sha2` (workspace dependency, new for this crate).

**Spec:** `docs/superpowers/specs/2026-10-25-openapi-import-and-sync-design.md`

## Global Constraints

- Supported inputs: OpenAPI 3.0, 3.1 and Swagger 2.0, as JSON or YAML text. Swagger 1.x and unknown versions are rejected with a clear error.
- Layout: by tag (default) or by URL path. Tag names with spaces and slashes become safe folder names.
- Request name: spec `summary`, else `operationId`, else `METHOD /path`. Cut to 100 characters.
- `servers[0].url` becomes the collection variable `baseUrl`. Every request URL starts with `{{baseUrl}}`. Server variables use their defaults.
- Secrets: no secret value from a spec is ever imported. Each security scheme becomes an empty secret collection variable and the request auth references it as `{{<variable>}}`.
- At most 5,000 operations per import. The rest are reported, not imported.
- Only local `#/...` `$ref` values are followed. A remote `$ref` becomes a report warning.
- All disk writes go through `CollectionRepository`. No raw YAML writes, no shelling out, no unwrapping of errors in production paths. `#[serde(rename_all = "camelCase")]` on IPC DTOs only.
- Cargo commands use `-j4 -p rocket-import`.

## Review Focus

- A schema that refers to itself (a tree or a linked list): the reader and the example generator stop at a depth cap instead of looping.
- An operation with no tags in tag layout: it goes in a folder named `untagged`.
- Two operations with the same `operationId`, or the same method and path twice: each still gets its own request and a unique `key` (`-2`, `-3` suffix).
- A tag or path segment such as `a/b`, `..`, `CON`, `environments`, or an empty string: it becomes a safe, unique folder name that is not a reserved collection name.
- A spec with 5,001 or more operations, or none at all: the first is capped with a report line, the second creates an empty collection and a report line, neither fails.

---

### Task 1: Reader and neutral model

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-import/src/openapi/mod.rs`, `crates/rocket-import/src/openapi/model.rs`, `crates/rocket-import/src/openapi/reader.rs`
- Create: `crates/rocket-import/tests/fixtures/openapi/orders-3.0.json`, `orders-3.1.yaml`, `store-2.0.yaml`, `recursive-3.0.yaml`
- Modify: `crates/rocket-import/src/lib.rs` (add `pub(crate) mod openapi;`), `crates/rocket-import/src/error.rs` (add `NotAnOpenApiSpec(String)`)
- Test: unit tests in `reader.rs`

**Interfaces:**
- Produces, in `model.rs`:
  - `pub struct OpenApiDoc { pub title: String, pub version: String, pub servers: Vec<String>, pub operations: Vec<Operation>, pub security_schemes: Vec<SecurityScheme>, pub warnings: Vec<String> }`
  - `pub struct Operation { pub key: String, pub method: HttpMethod, pub path: String, pub summary: Option<String>, pub operation_id: Option<String>, pub description: Option<String>, pub tags: Vec<String>, pub parameters: Vec<Parameter>, pub request_body: Option<RequestBodySpec>, pub security: Vec<String>, pub deprecated: bool }`
  - `pub struct Parameter { pub name: String, pub location: ParamLocation, pub required: bool, pub example: Option<serde_json::Value>, pub schema: Option<serde_json::Value>, pub description: Option<String> }` and `pub enum ParamLocation { Path, Query, Header, Cookie }`
  - `pub struct RequestBodySpec { pub content_type: String, pub schema: Option<serde_json::Value>, pub example: Option<serde_json::Value> }`
  - `pub struct SecurityScheme { pub name: String, pub kind: SecuritySchemeKind }` and `pub enum SecuritySchemeKind { Bearer, Basic, ApiKey { param_name: String, in_query: bool }, OAuth2, Unsupported(String) }`
- Produces, in `reader.rs`: `pub fn parse_openapi(text: &str) -> ImportResult<OpenApiDoc>`
- `Operation.key` is `operationId` when present, else `"{METHOD} {path}"` (upper-case method). Duplicate keys get `-2`, `-3` appended in document order.

- [ ] **Step 1: Write the failing tests** in `reader.rs` `#[cfg(test)]`:
  - `parses_3_0_json_operations`: `orders-3.0.json` gives 3 operations; `GET /orders` has `key == "listOrders"`; the path `/orders/{id}` has a Path parameter `id` with `required == true`.
  - `parses_3_1_yaml_and_2_0_yaml_to_same_shape`: `orders-3.1.yaml` and `store-2.0.yaml` both yield `servers[0] == "https://api.example.com/v1"` (for 2.0 built from `schemes`, `host`, `basePath`) and a JSON request body spec on `POST /orders` (for 2.0 from `consumes` and an `in: body` parameter).
  - `local_ref_is_resolved_and_remote_ref_warns`: a `$ref` to `#/components/parameters/Page` (or `#/parameters/Page` in 2.0) resolves; a `$ref: "https://x/y.yaml#/A"` leaves the item out and adds one entry to `warnings`.
  - `self_referencing_schema_terminates`: `recursive-3.0.yaml` parses and the body schema is present.
  - `duplicate_keys_get_suffix`: two operations with `operationId: dup` give keys `dup` and `dup-2`.
  - `rejects_non_openapi_and_swagger_1`: `{"swagger":"1.2"}` and `{"hello":1}` return `ImportError::NotAnOpenApiSpec`.
  - `trace_method_is_kept`: a `trace` operation becomes `HttpMethod::Trace`.
- [ ] **Step 2: Run to verify failure:** `cargo test -j4 -p rocket-import openapi::reader` → FAIL (module missing).
- [ ] **Step 3: Implement `parse_openapi(text: &str) -> ImportResult<OpenApiDoc>`** in `reader.rs`. Parse with `serde_yaml` into `serde_json::Value` (YAML accepts JSON). Detect the version from `openapi` (3.x) or `swagger == "2.0"`. Resolve local `$ref` with a visited set and a depth cap of 32, leaving the unresolved schema as `None` past the cap. Merge path-level parameters into each operation. Skip non-operation keys (`parameters`, `summary`, `servers`, `$ref`) under a path. Add no dependencies.
- [ ] **Step 4: Add the fixtures** listed above (small hand-written specs: orders with list, create and get-by-id; one tag `Order Management`; one bearer scheme).
- [ ] **Step 5: Run to verify pass:** `cargo test -j4 -p rocket-import openapi::reader` → PASS.
- [ ] **Step 6: Commit** with `dev-workflow-skills:1-git-commit`, paths: `crates/rocket-import/src/openapi`, `crates/rocket-import/src/lib.rs`, `crates/rocket-import/src/error.rs`, `crates/rocket-import/tests/fixtures/openapi`. Message: `feat(import): read OpenAPI 3.0, 3.1 and Swagger 2.0`.

---

### Task 2: Operation to request

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-import/src/converter/openapi.rs`
- Modify: `crates/rocket-import/src/converter/mod.rs` (add `pub(crate) mod openapi;`), `crates/rocket-import/Cargo.toml` (add `sha2.workspace = true`)
- Test: unit tests in `converter/openapi.rs`

**Interfaces:**
- Consumes: `Operation`, `Parameter`, `SecurityScheme`, `OpenApiDoc` from Task 1.
- Produces:
  - `pub struct ConvertedOperation { pub request: Request, pub skipped: Vec<SkipReason> }`
  - `pub fn convert_operation(op: &Operation, schemes: &[SecurityScheme], seq: u32) -> ConvertedOperation`
  - `pub fn request_name(op: &Operation) -> String`
  - `pub fn security_variable_name(scheme: &SecurityScheme) -> String` (the collection variable a scheme uses, for example `bearerAuth_token`)
  - `pub fn baseline_hash(request: &Request) -> String` (hex SHA-256 over a canonical form of: method, url, query params, path params, headers, body mode and content, form data, auth kind and placeholder; not scripts, tests, docs, examples, name or seq)
  - `pub fn sample_json(schema: &serde_json::Value, depth: usize) -> serde_json::Value` (uses `example`, then `default`, then the first `enum` value, else a typed placeholder such as `"string"`, `0`, `false`; objects and arrays recurse; depth cap 8)

- [ ] **Step 1: Write the failing tests:**
  - `url_uses_base_url_and_colon_path_params`: path `/orders/{id}` gives url `{{baseUrl}}/orders/:id` and `path_params == [PathParam { name: "id", .. }]`.
  - `query_and_header_params_become_rows`: a required query param is `enabled: true`, an optional one is `enabled: false`, both keep the spec example as value; `Cookie` params are skipped with a `SkipReason::UnsupportedRequestType`.
  - `json_body_uses_example_then_schema`: with `example` the body content equals it pretty-printed; with only a schema it equals `sample_json` output; `body.mode == BodyMode::Json` and a `Content-Type: application/json` header is not added twice.
  - `form_and_xml_bodies_map_modes`: `application/x-www-form-urlencoded` gives `BodyMode::FormUrlEncoded`, `multipart/form-data` gives `BodyMode::FormData` with `FormDataType::File` for binary fields, `application/xml` gives `BodyMode::Xml`.
  - `auth_references_variable_never_a_value`: bearer gives `Auth::Bearer { token: "{{bearerAuth_token}}" }`, basic gives username and password placeholders, apiKey gives `Auth::ApiKey` with the spec's parameter name, placement `header` or `query`, and a placeholder value; OAuth2 and unknown schemes give `Auth::None` plus a `SkipReason::UnsupportedAuthType`. No fixture value appears in the output.
  - `name_rule_and_length_cap`: summary wins over `operationId`, which wins over `METHOD /path`; a 300-character summary is cut to 100.
  - `baseline_hash_ignores_user_fields_but_not_imported_ones`: changing a script, test, doc, example, name or seq leaves the hash equal; changing url, a header value, body content or auth changes it.
  - `sample_json_stops_on_deep_schema`: a schema nested 50 levels returns at depth 8.
- [ ] **Step 2: Run to verify failure:** `cargo test -j4 -p rocket-import converter::openapi` → FAIL.
- [ ] **Step 3: Implement the functions above** in `converter/openapi.rs`. Model it on `converter/wsdl.rs::convert_operation` for building a `Request` (fresh `uid` from `rocket_collection::generate_uid`, `description` from the operation description when present, `tags` from the spec tags). Use `serde_json::to_string_pretty` for bodies. `QueryParam` is an alias of `Header`.
- [ ] **Step 4: Run to verify pass:** `cargo test -j4 -p rocket-import converter::openapi` → PASS.
- [ ] **Step 5: Commit** with `dev-workflow-skills:1-git-commit`, paths: `crates/rocket-import/src/converter`, `crates/rocket-import/Cargo.toml`. Message: `feat(import): convert OpenAPI operations to requests`.

---

### Task 3: Layout and `import_openapi_text`

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-import/src/importer.rs` (new method and types; reuse `unique_segment`, `resolve_collection_name`, `REQUEST_RESERVED`, `PORT_RESERVED`, `SERVICE_RESERVED`), `crates/rocket-import/src/lib.rs` (re-export `GroupBy`, `OpenApiImportOutcome`, `ImportedOperation`)
- Create: `crates/rocket-import/tests/openapi_integration_test.rs`

**Interfaces:**
- Consumes: `parse_openapi`, `convert_operation`, `baseline_hash`, `security_variable_name` from Tasks 1 and 2.
- Produces:
  - `pub enum GroupBy { Tags, Paths }`
  - `pub struct ImportedOperation { pub key: String, pub request_path: String, pub uid: String, pub baseline_hash: String }`
  - `pub struct OpenApiImportOutcome { pub report: ImportReport, pub collection: String, pub operations: Vec<ImportedOperation> }`
  - `impl ImportService { pub fn import_openapi_text(&self, text: &str, source_label: &str, group_by: GroupBy, workspace_id: &str) -> ImportResult<OpenApiImportOutcome> }`
  - `pub(crate) const MAX_OPENAPI_OPERATIONS: usize = 5_000;`
- Layout rules: `Tags` puts an operation in the folder of its first tag, untagged operations in `untagged`. `Paths` uses the literal segments of the path before the first `{param}` segment as nested folders (`/api/v1/users/{id}` and `/api/v1/users` both go in `api/v1/users`); a path with no literal segment goes in the collection root. Folder and file names use `unique_segment`, so they are safe and unique. `seq` follows document order within a folder. The collection name is `info.title`, else the file stem of `source_label`, resolved with `resolve_collection_name`. After creating the collection, save `CollectionSettings` with the `baseUrl` variable and one empty secret variable per security scheme (`CollectionVariable { enabled: true, secret: true, value: "", initial_value: "" }`).

- [ ] **Step 1: Write the failing tests** in `openapi_integration_test.rs` (use `ImportService::new_with_workspace_path` on a `tempfile` dir, as the WSDL integration test does):
  - `tags_layout_makes_one_folder_per_first_tag`: `orders-3.0.json` gives one folder named like `unique_segment` returns for `Order Management`, with 3 requests; `report.imported == 3`.
  - `paths_layout_nests_literal_segments`: with `GroupBy::Paths` a request for `/orders/{id}` sits in `orders/`, and `/orders` also sits in `orders/`.
  - `untagged_operations_go_to_untagged_folder`.
  - `collection_variables_hold_base_url_and_empty_secrets`: `get_settings` returns `baseUrl == "https://api.example.com/v1"` and a secret variable for the bearer scheme with an empty value.
  - `outcome_lists_every_written_request`: `operations.len() == imported`, each `request_path` exists in the repo, each `uid` equals the saved request's `uid`, each `baseline_hash` equals `baseline_hash(&saved_request)`.
  - `cap_and_empty_spec_are_reported_not_fatal`: a generated spec with `MAX_OPENAPI_OPERATIONS + 1` operations imports exactly the cap and adds one skipped item saying how many were left; a spec with no paths creates an empty collection and one report line.
  - `name_collision_gets_suffix`: importing the same spec twice gives collections `Orders` and `Orders-1`.
  - `reserved_and_hostile_names_are_safe`: tags `environments`, `a/b`, `..`, `CON` produce folders that are not those names and do not escape the collection.
- [ ] **Step 2: Run to verify failure:** `cargo test -j4 -p rocket-import --test openapi_integration_test` → FAIL.
- [ ] **Step 3: Implement `import_openapi_text`** following the shape of `import_wsdl_with_limits` (only creating the collection is fatal; a folder or request that cannot be written becomes a skipped item). Collect `ImportedOperation` rows from the saved requests. Warnings from the reader become `SkippedItem { reason: SkipReason::UnsupportedRequestType(..) }`.
- [ ] **Step 4: Run to verify pass:** `cargo test -j4 -p rocket-import` → PASS, then `cargo check -j4`.
- [ ] **Step 5: Commit** with `dev-workflow-skills:1-git-commit`, paths: `crates/rocket-import/src/importer.rs`, `crates/rocket-import/src/lib.rs`, `crates/rocket-import/tests/openapi_integration_test.rs`. Message: `feat(import): import an OpenAPI spec as a collection`.
- [ ] **Step 6: Update the docs.** Add the OpenAPI rows to `crates/rocket-import/CLAUDE.md` (module map, public API, mappings) and mark P1 done in `00-plan-index.md`. Commit with the same skill: `docs(import): describe the OpenAPI importer`.
