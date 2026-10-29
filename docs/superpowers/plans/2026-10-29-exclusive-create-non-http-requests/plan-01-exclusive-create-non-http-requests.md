# Exclusive Create for Non-HTTP Requests, Backend Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** The backend can create a GraphQL, gRPC or WebSocket request without ever replacing an existing file, behind three new Tauri commands.

**Architecture:** Three trait methods with default bodies in `rocket-collection`. `rocket-infra` gets one shared exclusive write helper (temp file plus `hard_link`, with a `create_new` fallback) and three thin creates. `rocket-app` copies the `create_request` service pattern per type. `src-tauri` exposes three thin commands.

**Tech Stack:** Rust, `rocket-collection`, `rocket-infra`, `rocket-app`, Tauri.

**Spec:** `docs/superpowers/specs/2026-10-29-exclusive-create-non-http-requests-design.md`

## File Structure

- `crates/rocket-collection/src/repository.rs`: three trait methods with defaults, and a default test in its tests module.
- `crates/rocket-infra/src/fs_collection/requests.rs`: `write_new_file_exclusive`, `fallback_create_new_with`, three create functions.
- `crates/rocket-infra/src/fs_collection/mod.rs`: wire the three.
- `crates/rocket-infra/src/shared_path_collection_repo.rs`: forward the three.
- `crates/rocket-infra/src/fs_collection/tests.rs`: infra tests.
- `crates/rocket-app/src/collection_service.rs`, `crates/rocket-app/src/test_doubles.rs`: service methods, hooks and tests.
- `src-tauri/src/commands/collections.rs`, `src-tauri/src/lib.rs`: commands and registration.

## Global Constraints

- Do not change `save_graphql_request`, `save_grpc_request`, `save_websocket_request` or `create_request_exclusive` in any layer. HTTP create keeps its code and behaviour.
- New trait methods have default bodies returning `DomainError::Internal`, so other implementors compile unchanged. `SharedPathCollectionRepo` must forward all three.
- No panicking unwrap calls in production code. Tests use `.expect("...")` and `expect_err`. No `git` CLI.
- Cargo commands are per crate: `cargo check -j4 -p <crate>` and `cargo test -j4 -p <crate> <filter>`. The `src-tauri` package is `rocket`. Never run a bare or workspace-wide cargo command.
- Edit only the files listed. No formatting, import-order or clippy edits elsewhere. If `cargo fmt` would change a file you did not otherwise touch, leave it.
- The exclusive create does not call `create_dir_all`: a missing parent folder is an error.

## Review Focus

1. A name that differs only in case is refused by the service (`path_exists` is case-insensitive): `create_<type>_request_refuses_a_name_that_exists_even_in_another_case` (Task 3).
2. A filesystem without hard links still creates the file, and never replaces one: `exclusive_create_falls_back_when_hard_link_is_unsupported` and `fallback_create_new_maps_a_clash_to_already_exists_and_keeps_the_file` (Task 1).
3. The fallback never deletes a file it did not create: `fallback_create_new_maps_a_clash_to_already_exists_and_keeps_the_file` (Task 1).
4. A refused create publishes no event and writes nothing: `create_<type>_request_refuses_a_name_that_exists_even_in_another_case` asserts no event (Task 3).
5. `SharedPathCollectionRepo` forwards the new methods, otherwise the default hides the real repo: `shared_path_repo_forwards_the_exclusive_creates` (Task 2).

---

### Task 1: Trait methods and the exclusive write helper

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-collection/src/repository.rs` (next to `create_request_exclusive`, `:218-229`; tests module with `MinimalRepo`, `:475`)
- Modify: `crates/rocket-infra/src/fs_collection/requests.rs` (new helpers, near `create_request_exclusive` `:133-190`)
- Test: `crates/rocket-infra/src/fs_collection/tests.rs`

**Interfaces:**
- Produces:
  - Trait methods (each with a default body `Err(DomainError::Internal("exclusive <type> creation is not available".into()))`):
    - `fn create_graphql_request_exclusive(&self, collection: &str, path: &str, request: &GraphQlRequest) -> DomainResult<String>`
    - `fn create_grpc_request_exclusive(&self, collection: &str, path: &str, request: &GrpcRequest) -> DomainResult<String>`
    - `fn create_websocket_request_exclusive(&self, collection: &str, path: &str, request: &WebSocketRequest) -> DomainResult<String>`
    - Doc comment: saves a new request, fails with `AlreadyExists` when the file is there, returns the collection-relative path.
  - In `requests.rs`:
    - `pub(super) fn write_new_file_exclusive(file_path: &Path, bytes: &[u8], label: &str, link: impl Fn(&Path, &Path) -> std::io::Result<()>) -> DomainResult<()>`. Writes a hidden temp file `.new-request.tmp.{pid}_{nanos:08x}` in the same directory (`create_new` and `sync_all`; a write error removes the temp file and returns `DomainError::Io`). Calls `link(&tmp, file_path)`, always removes the temp file, maps `ErrorKind::AlreadyExists` to `DomainError::AlreadyExists(label.to_string())`, and on any other link error calls `fallback_create_new_with`.
    - `pub(super) fn fallback_create_new_with(file_path: &Path, label: &str, write: impl FnOnce(&mut std::fs::File) -> std::io::Result<()>) -> DomainResult<()>`. Opens `file_path` with `create_new(true)`; an `AlreadyExists` open error returns `AlreadyExists(label)` and deletes nothing. After a successful open it runs `write`; if `write` fails it removes the file it just created and returns `DomainError::Io`.
    - `pub(super) fn fallback_create_new(file_path: &Path, bytes: &[u8], label: &str) -> DomainResult<()>`: `fallback_create_new_with` with a writer that does `write_all(bytes)` then `sync_all()`.

- [ ] **Step 1: Read** `requests.rs:128-192` (the HTTP exclusive create, which stays as it is), `repository.rs:199-232` and its tests module, and `fs_collection/tests.rs:3217-3300` (`setup()` and `create_request_exclusive_never_replaces_a_file`).

- [ ] **Step 2: Write the failing tests.**

In `repository.rs` tests, using `MinimalRepo` as the neighbouring tests do:

```rust
#[test]
fn new_exclusive_creates_default_to_unavailable() {
    let repo = MinimalRepo;
    let gql = GraphQlRequest::new("g", "https://e.example/graphql");
    assert!(matches!(
        repo.create_graphql_request_exclusive("c", "g.yml", &gql).expect_err("default"),
        DomainError::Internal(_)
    ));
}
```

(Add the same for gRPC and WebSocket, building the request the way the existing `save_*` tests do.)

In `fs_collection/tests.rs`, use `tempfile::tempdir()` directly (these test the helper, not a repo):

| Test | Setup | Assertion |
|---|---|---|
| `write_new_file_exclusive_links_a_new_file` | empty temp dir, `link` = `std::fs::hard_link` | `Ok`, file content equals the bytes, no `.new-request.tmp.*` left in the dir |
| `write_new_file_exclusive_never_replaces_a_file` | file `a.yml` already holds `old` | `expect_err` is `AlreadyExists` with the label, content still `old`, no temp left |
| `exclusive_create_falls_back_when_hard_link_is_unsupported` | `link` closure returns `Err(ErrorKind::Unsupported.into())` | `Ok`, file holds the bytes, no temp left |
| `fallback_create_new_maps_a_clash_to_already_exists_and_keeps_the_file` | file already holds `old`, call `fallback_create_new` | `AlreadyExists`, content still `old` |
| `fallback_create_new_removes_the_file_when_the_write_fails` | `fallback_create_new_with` and a `write` closure returning `Err(ErrorKind::Other.into())` | `Io` error and the file does not exist |

- [ ] **Step 3: Run** `cargo test -j4 -p rocket-collection new_exclusive_creates` and `cargo test -j4 -p rocket-infra write_new_file`. Expected: compile errors, `no method named create_graphql_request_exclusive` and `cannot find function write_new_file_exclusive in this scope`.

- [ ] **Step 4: Implement** the three trait methods with their defaults, then the helpers in `requests.rs` as specified. Do not touch `create_request_exclusive`.

- [ ] **Step 5: Run** `cargo test -j4 -p rocket-collection`, `cargo test -j4 -p rocket-infra write_new_file`, `cargo test -j4 -p rocket-infra fallback_create_new`, `cargo test -j4 -p rocket-infra exclusive_create_falls_back`, and `cargo check -j4 -p rocket-infra`. Expected: all pass (one new test in rocket-collection per type, five in rocket-infra).

---

### Task 2: Three exclusive creates in rocket-infra

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-infra/src/fs_collection/requests.rs` (next to `save_graphql_request` `:259`, `save_websocket_request` `:332`, `save_grpc_request` `:437`)
- Modify: `crates/rocket-infra/src/fs_collection/mod.rs` (next to `create_request_exclusive`, `:297-304`)
- Modify: `crates/rocket-infra/src/shared_path_collection_repo.rs` (next to `:202`)
- Test: `crates/rocket-infra/src/fs_collection/tests.rs`

**Interfaces:**
- Consumes: `write_new_file_exclusive`, the three trait methods from Task 1.
- Produces: `pub(super) fn create_graphql_request_exclusive(repo: &FsCollectionRepo, collection: &str, path: &str, request: &GraphQlRequest) -> DomainResult<String>` and the same for gRPC (`GrpcRequest`) and WebSocket (`WebSocketRequest`). Each: `Collection::validate_name(collection)?`, lock `repo.collection_mutex(collection)`, reject an empty `uid` with `DomainError::Internal`, `request_filename_for(path)`, `repo.validate_path(...)`, convert with `graphql_to_oc`, `grpc_to_oc` or `websocket_to_oc_websocket`, `serde_yaml::to_string`, call `write_new_file_exclusive(&file_path, yaml.as_bytes(), &format!("{collection}/{path}"), std::fs::hard_link)`, return the collection-relative path as `create_request_exclusive` does. No variables are preserved (the file is new). `FsCollectionRepo` and `SharedPathCollectionRepo` implement the three trait methods by delegating.

- [ ] **Step 1: Read** `save_graphql_request`, `save_grpc_request`, `save_websocket_request` and `create_request_exclusive` in `requests.rs`, and the tests `save_graphql_request_rejects_an_empty_uid` (`tests.rs:2014`) and `save_grpc_request_rejects_an_empty_uid` (`:2502`) for how each request is built.

- [ ] **Step 2: Write the failing tests.** Example for GraphQL (copy the shape of `create_request_exclusive_never_replaces_a_file`):

```rust
#[test]
fn create_graphql_request_exclusive_never_replaces_a_file() {
    let (_dir, repo) = setup();
    repo.create("col").expect("create collection");
    let first = GraphQlRequest::new("Orders", "https://e.example/graphql");
    repo.create_graphql_request_exclusive("col", "orders.yml", &first)
        .expect("first create");
    let before = std::fs::read_to_string(repo.collection_path("col").join("orders.yml"))
        .expect("read");
    let second = GraphQlRequest::new("Other", "https://e.example/other");
    let err = repo
        .create_graphql_request_exclusive("col", "orders.yml", &second)
        .expect_err("second create");
    assert!(matches!(err, DomainError::AlreadyExists(_)), "{err}");
    let after = std::fs::read_to_string(repo.collection_path("col").join("orders.yml"))
        .expect("read");
    assert_eq!(before, after);
}
```

| Test | Setup | Assertion |
|---|---|---|
| `create_grpc_request_exclusive_never_replaces_a_file` | same shape with a `GrpcRequest` built as the gRPC save tests build it | `AlreadyExists`, content unchanged |
| `create_websocket_request_exclusive_never_replaces_a_file` | same shape with a `WebSocketRequest` | `AlreadyExists`, content unchanged |
| `create_<type>_request_exclusive_rejects_an_empty_uid` (3 tests) | request with `uid` set to an empty string | `Internal`, no file written |
| `create_<type>_request_exclusive_writes_a_readable_file` (3 tests) | create then `get_graphql_request` / `get_grpc_request` / `get_websocket_request` | name and URL round-trip |
| `shared_path_repo_forwards_the_exclusive_creates` | build `SharedPathCollectionRepo` over a temp workspace the way its existing tests do; create one GraphQL request through it | `Ok` (the default would give `Internal`) |

- [ ] **Step 3: Run** `cargo test -j4 -p rocket-infra create_graphql_request_exclusive`. Expected: `Internal("exclusive GraphQL creation is not available")` from the default, because `FsCollectionRepo` does not implement it yet.

- [ ] **Step 4: Implement** the three create functions and the delegating trait impls in `mod.rs` and `shared_path_collection_repo.rs`.

- [ ] **Step 5: Run** `cargo test -j4 -p rocket-infra create_` , `cargo test -j4 -p rocket-infra shared_path_repo_forwards`, then `cargo test -j4 -p rocket-infra`, and `cargo check -j4 -p rocket-infra`. Expected: all pass, and the existing `save_*` and `create_request_exclusive_never_replaces_a_file` tests are unchanged.

---

### Task 3: Service methods, test double hooks and Tauri commands

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/collection_service.rs` (next to `create_request`, `:528-543`; tests near `:1377-1445`)
- Modify: `crates/rocket-app/src/test_doubles.rs` (`NullCollectionRepo`, hook fields near `:788`, constructors near `:822`, builders near `:959`, impl near `:1039`)
- Modify: `src-tauri/src/commands/collections.rs` (next to `create_request`, `:189-197`)
- Modify: `src-tauri/src/lib.rs` (next to `commands::collections::create_request`, `:998`)

**Interfaces:**
- Consumes: the three repo trait methods from Task 1.
- Produces:
  - `pub fn create_graphql_request(&self, collection: &str, path: &str, request: &GraphQlRequest) -> DomainResult<GraphQlRequest>` and `create_grpc_request` (`GrpcRequest`) and `create_websocket_request` (`WebSocketRequest`) on `CollectionService`. Each: `let file_path = rocket_collection::request_filename_for(path);` then under `self.locked(collection, ...)`: `self.repo.path_exists(collection, &file_path)?` true returns `DomainError::AlreadyExists(format!("{collection}/{file_path}"))`; otherwise call the repo exclusive method with `&file_path`, publish `DomainEvent::RequestSaved { collection, path: actual_path }`, return `self.repo.get_<type>_request(collection, &actual_path)`.
  - `NullCollectionRepo`: per type, a hook field typed for that request (the existing `SaveRequestHook` is typed for `Request`), an `on_create_<type>_request_exclusive` builder, an impl of the trait method, and the matching `on_get_<type>_request` hook if it does not exist yet. Unset hooks return `Internal`, like the existing ones.
  - Tauri commands `create_graphql_request`, `create_grpc_request`, `create_websocket_request`, each `#[tauri::command(async)] pub fn name(collection: String, path: String, request: <Type>, svc: State<'_, CollectionService>) -> Result<<Type>, DomainError>` calling `svc.create_<type>_request(&collection, &path, &request)`. Registered in `lib.rs` next to `create_request`.

- [ ] **Step 1: Read** `create_request` and `create_request_exclusive_unlocked` (`collection_service.rs:497-543`), the tests `create_request_refuses_a_name_that_exists_even_in_another_case` (`:1418`) and `create_request_writes_a_new_name_and_publishes` (`:1435`), `dup_service` (`:1377`, HTTP-only, so build a small fixture for the other types in the same style), and `NullCollectionRepo` in `test_doubles.rs`.

- [ ] **Step 2: Write the failing tests** in `collection_service.rs`. Example for GraphQL (build the fixture like `dup_service` with `NullCollectionRepo::strict()`, `.on_path_exists(..)`, the new hooks and a recording event publisher):

```rust
#[test]
fn create_graphql_request_refuses_a_name_that_exists_even_in_another_case() {
    let (svc, events, writes) = graphql_create_service(|_| true);
    let request = GraphQlRequest::new("Orders", "https://e.example/graphql");
    let err = svc
        .create_graphql_request("col", "Orders", &request)
        .expect_err("name is taken");
    assert!(matches!(err, DomainError::AlreadyExists(_)), "{err}");
    assert_eq!(*writes.lock().expect("writes"), 0);
    assert!(events.recorded().is_empty());
}
```

| Test | Setup | Assertion |
|---|---|---|
| `create_grpc_request_refuses_a_name_that_exists_even_in_another_case` | same fixture for gRPC | `AlreadyExists`, no write, no event |
| `create_websocket_request_refuses_a_name_that_exists_even_in_another_case` | same for WebSocket | same |
| `create_<type>_request_writes_a_new_name_and_publishes` (3 tests) | `path_exists` returns false, exclusive hook returns `col/Fresh.yml`, get hook returns the request | `Ok`, one `RequestSaved` event with that path |

- [ ] **Step 3: Run** `cargo test -j4 -p rocket-app create_graphql_request`. Expected: `no method named create_graphql_request found for struct CollectionService`.

- [ ] **Step 4: Implement** the three service methods, the test double hooks, the three commands and their registration.

- [ ] **Step 5: Run** `cargo test -j4 -p rocket-app create_graphql_request`, `cargo test -j4 -p rocket-app create_grpc_request`, `cargo test -j4 -p rocket-app create_websocket_request`, `cargo test -j4 -p rocket-app collection_service`, `cargo check -j4 -p rocket-app` and `cargo check -j4 -p rocket`. Expected: all pass. The driver runs the bare `src-tauri` check itself.

Next plan: docs/superpowers/plans/2026-10-29-exclusive-create-non-http-requests/plan-02-exclusive-create-non-http-requests.md
