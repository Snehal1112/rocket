# Response Examples, Plan 2: Backend Commands Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add list, add, rename and delete commands for the saved examples of one HTTP request, and show example names in the sidebar's request summaries.

**Architecture:** New `CollectionRepository` methods (default bodies return an error, like `save_request_script`) implemented in a new `crates/rocket-infra/src/fs_collection/examples.rs`. Each call locks the collection mutex, reads the request file, changes only its `examples` list and writes it back atomically. `CollectionService` wraps them and publishes `DomainEvent::RequestSaved`. Four thin Tauri commands call the service. `RequestSummary` gains `example_names` so the sidebar can nest examples without extra file reads.

**Tech Stack:** Rust (`rocket-collection`, `rocket-infra`, `rocket-app`, `src-tauri`), `serde_yaml`, TypeScript bindings in `src/lib/tauri-api.ts`.

**Spec:** `docs/superpowers/specs/2026-10-23-response-examples-design.md`

## Global Constraints

- Real values stay on disk. No masking at save time. Examples stay out of exports (no export change).
- Only the `examples` list of the request file changes. Every other field, including `runtime.variables`, auth, scripts, docs, settings and the top-level `uid`, must survive byte-for-meaning (parse the file again and compare).
- Example names: trimmed, 1 to 120 characters, no control characters, unique per request, compared case-insensitively. Bad name is `DomainError::InvalidInput`, duplicate is `DomainError::AlreadyExists`. (Orchestrator's call: names are display text, never used as file names.)
- One example, serialized as YAML, is at most 1 MiB (1_048_576 bytes). Larger is `InvalidInput`. (Orchestrator's call.)
- Only HTTP request files hold examples through these commands. A GraphQL, gRPC or WebSocket file is `InvalidInput`.
- `camelCase` only on IPC DTOs. `HttpRequestExample` already has it. Do not add it to persistence structs.
- Commands stay thin: validate, call the service, return. No `unwrap` in production paths.
- Run `cargo test -j4 -p <crate> <test_name>` and `cargo check -j4 -p <crate>` per crate. Never `--workspace`.

## Review Focus

- Deleting the last example removes the `examples` key from the file (an empty list must not be written), and a later `save_request` from the UI still keeps other data.
- Two examples named `200 OK` and `200 ok`: the second add fails with `AlreadyExists`, and nothing is written.
- A request file with variables, auth, scripts and docs survives add, rename and delete unchanged apart from `examples`.
- A name such as `a: b # c` or `- x` or a multi-line name round-trips through YAML as the same string.
- Renaming an example to its own current name succeeds (no false duplicate). Renaming to a name used by another example fails.
- Adding to a request path that does not exist is `NotFound`. A path with `..` is rejected by the existing path validation.
- The summary load still reads each file once. No extra syscalls per request.

---

### Task 1: Repository methods and infra implementation

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-collection/src/repository.rs` (trait methods, and the test stub impl at the bottom if it needs them)
- Modify: `crates/rocket-collection/src/request_summary.rs`
- Create: `crates/rocket-infra/src/fs_collection/examples.rs`
- Modify: `crates/rocket-infra/src/fs_collection/mod.rs` (declare `mod examples;`, delegate trait methods)
- Modify: `crates/rocket-infra/src/fs_collection/tree.rs` (`load_request_summary` fills `example_names`)
- Modify: every `RequestSummary { .. }` literal: `crates/rocket-app/src/collection_export_service.rs`, `mcp_read_views.rs`, `mcp_tool_service.rs`, and the four in `tree.rs` (add `example_names: Vec::new()` except the HTTP one)
- Test: `crates/rocket-infra/src/fs_collection/examples.rs` (a `#[cfg(test)] mod tests`, `tempfile` fixtures like `variables.rs` tests)

**Interfaces:**
- Produces on trait `CollectionRepository` (all with default bodies returning `DomainError::Internal("... not supported by this repository")`):
  - `fn list_request_examples(&self, collection: &str, request_path: &str) -> DomainResult<Vec<HttpRequestExample>>`
  - `fn add_request_example(&self, collection: &str, request_path: &str, example: HttpRequestExample) -> DomainResult<()>`
  - `fn rename_request_example(&self, collection: &str, request_path: &str, old_name: &str, new_name: &str) -> DomainResult<()>`
  - `fn delete_request_example(&self, collection: &str, request_path: &str, name: &str) -> DomainResult<()>`
- Produces on `RequestSummary`: `pub example_names: Vec<String>` with `#[serde(default, skip_serializing_if = "Vec::is_empty")]` (wire name `exampleNames` if the struct already uses camelCase; check the struct's serde attributes and follow them).
- `HttpRequestExample` comes from `rocket_shared::action`.

- [ ] **Step 1: Write the failing tests** in `examples.rs`. Each builds a request `.yml` in a `tempfile` collection through `FsCollectionRepo` (copy the fixture setup from the `save_request_variables` tests), then calls the new trait methods:
  - `add_then_list_returns_the_example`: add `{name: "200 OK", request: {url: "https://x.io", method: "GET"}, response: {status: 200}}`, list returns one item with the same name and values.
  - `add_keeps_every_other_field`: a file with variables, bearer auth, a pre-request script, docs and a `uid`; after add, `get_request` and `get_request_variables` equal the values before.
  - `duplicate_name_is_rejected_case_insensitively_and_writes_nothing`: add `200 OK`, add `200 ok` returns `AlreadyExists`, the file bytes are unchanged.
  - `bad_names_are_rejected`: empty, spaces only, 121 characters, and a name with a newline each return `InvalidInput`.
  - `oversized_example_is_rejected`: a response body `data` of 1_100_000 characters returns `InvalidInput`.
  - `rename_changes_only_that_name`: two examples, rename the first, the second and the order are unchanged. Rename to its own name succeeds. Rename to the other's name returns `AlreadyExists`. Renaming a missing name returns `NotFound`.
  - `delete_removes_the_example_and_the_key_when_last`: delete one of two leaves one. Delete the last, then read the file text and assert it does not contain `examples:`.
  - `awkward_names_round_trip`: names `a: b # c`, `- x`, `'quoted'` come back identical from `list_request_examples`.
  - `non_http_files_are_rejected`: a GraphQL request file returns `InvalidInput` from add.
  - `missing_request_is_not_found`: add on an absent path returns `NotFound`.
  - In `tree.rs` tests (or `tree_walk_tests.rs`): `summaries_list_example_names_in_order`: a request file with two examples gives `example_names == ["200 OK", "401"]` from `get_summaries`.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -j4 -p rocket-infra examples`
Expected: FAIL (methods and module do not exist; compile errors count).

- [ ] **Step 3: Add the trait methods and `RequestSummary.example_names`** with the signatures above. Fix all `RequestSummary` literals so `cargo check -j4 -p rocket-collection -p rocket-infra -p rocket-app` compiles.

- [ ] **Step 4: Implement `examples.rs`** as `pub(super)` functions taking `repo: &FsCollectionRepo`, following `save_request_script` in `variables.rs`: `Collection::validate_name`, `repo.collection_mutex(..)` lock, `resolve_request_path`, `fs::read_to_string`, parse `OcHttpRequest` (parse failure that matches another kind is `InvalidInput`), change `oc.examples` only (`Option<Vec<OcHttpRequestExample>>`; set to `None` when it becomes empty), `serde_yaml::to_string`, `atomic_write`. Convert between `HttpRequestExample` (domain, opaque `Value`) and `OcHttpRequestExample` with the same mapping as `conversions/request.rs` (`serde_yaml::to_value` and `from_value`). Put the name and size checks in one helper `validate_example(&HttpRequestExample) -> DomainResult<()>`. If a parse-modify-serialize round trip of `OcHttpRequest` drops or reorders fields the `add_keeps_every_other_field` test checks, switch to editing the parsed `serde_yaml::Value` of the whole file (replace only the `examples` key) and keep the other keys as parsed.
  Fill `example_names` in `load_request_summary` for HTTP files by adding `#[serde(default)] examples: Vec<MinExample>` to `MinReq` with `struct MinExample { name: String }`.

- [ ] **Step 5: Run to verify pass**

Run: `cargo test -j4 -p rocket-infra examples` and `cargo test -j4 -p rocket-infra tree_walk`
Expected: PASS.

- [ ] **Step 6: Run `cargo check -j4 -p rocket-collection`, `cargo check -j4 -p rocket-infra`, `cargo check -j4 -p rocket-app`**
Expected: no errors.

### Task 2: Service, Tauri commands and TypeScript bindings

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/collection_service.rs`
- Modify: `src-tauri/src/commands/collections.rs`
- Modify: `src-tauri/src/lib.rs` (register the four commands next to `commands::collections::get_request_variables`)
- Modify: `src/lib/tauri-api.ts`
- Modify: `crates/rocket-app/CLAUDE.md` (one row of detail under CollectionService)
- Test: `crates/rocket-app/src/collection_service.rs` (existing `tests` module, or a new `collection_service_examples_tests.rs` if the file is already large)

**Interfaces:**
- Consumes: the four repository methods from Task 1.
- Produces on `CollectionService`:
  - `pub fn list_request_examples(&self, collection: &str, request_path: &str) -> DomainResult<Vec<HttpRequestExample>>`
  - `pub fn add_request_example(&self, collection: &str, request_path: &str, example: HttpRequestExample) -> DomainResult<()>`
  - `pub fn rename_request_example(&self, collection: &str, request_path: &str, old_name: &str, new_name: &str) -> DomainResult<()>`
  - `pub fn delete_request_example(&self, collection: &str, request_path: &str, name: &str) -> DomainResult<()>`
  Add, rename and delete publish `DomainEvent::RequestSaved { collection, path: request_path }` after the repo call succeeds. List publishes nothing.
- Produces Tauri commands `list_request_examples`, `add_request_example`, `rename_request_example`, `delete_request_example`, with arguments `collection: String`, `request_path: String` plus `example: HttpRequestExample` / `old_name`, `new_name` / `name`, and `svc: State<'_, CollectionService>`, returning `Result<_, DomainError>`.
- Produces in `tauri-api.ts`:
  - `export interface RequestExample { name: string; description?: unknown; request?: ExampleRequestSnapshot; response?: ExampleResponseSnapshot }`
  - `export interface ExampleRequestSnapshot { url?: string; method?: string; headers?: { name: string; value: string }[]; params?: { name: string; value: string; type?: 'query' | 'path' }[]; body?: { type: string; data: string | { name: string; value: string }[] } }`
  - `export interface ExampleResponseSnapshot { status?: number; statusText?: string; headers?: { name: string; value: string }[]; body?: { type: string; data: string } }`
  - `export const listRequestExamples = (collection: string, requestPath: string) => invoke<RequestExample[]>('list_request_examples', { collection, requestPath })`, and `addRequestExample(collection, requestPath, example)`, `renameRequestExample(collection, requestPath, oldName, newName)`, `deleteRequestExample(collection, requestPath, name)` in the same style as `getRequestVariables`.
  - `exampleNames?: string[]` on `RequestSummary`.

- [ ] **Step 1: Write the failing service tests** using the same in-memory or `FsCollectionRepo` + `RecordingEventPublisher` setup the neighbouring `save_request_script` tests use:
  - `add_example_publishes_request_saved`: after `add_request_example`, the recorded events contain one `RequestSaved` with the collection and path.
  - `failed_add_publishes_nothing`: a duplicate name returns the error and records no event.
  - `list_publishes_nothing`.
  - `rename_and_delete_publish_request_saved`.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -j4 -p rocket-app example`
Expected: FAIL (methods missing).

- [ ] **Step 3: Implement the four service methods** (delegate to `self.repo`, publish on success) and the four commands plus `lib.rs` registration. Keep command bodies to one service call each.

- [ ] **Step 4: Add the TypeScript types and four functions** to `src/lib/tauri-api.ts` as specified above. If a `tauri-api` mock or contract test lists all commands, add the four names there.

- [ ] **Step 5: Run to verify pass**

Run: `cargo test -j4 -p rocket-app example`, `cargo check -j4 -p rocket-app`, `cargo check -j4 -p rocket` (the `src-tauri` package; confirm its name in `src-tauri/Cargo.toml`), `yarn tsc --noEmit`
Expected: PASS, no errors.

- [ ] **Step 6: Add the CollectionService detail row** in `crates/rocket-app/CLAUDE.md` (what the four methods do, the event, the limits from Global Constraints).
