# Exclusive create for GraphQL, gRPC and WebSocket requests

Ticket: https://github.com/Snehal1112/rocket/issues/113

## Goal

Creating a GraphQL, gRPC or WebSocket request with a name that already exists must be refused with an "already exists" error, never replace the file, and Bruno and Postman imports must stop overwriting files within one import.

## Overview

Creating an HTTP request goes through `create_request`, which refuses an existing name (commit `e8a44106`). Creating the other three types in `CreateRequestDialog` still calls the overwriting `save_*` commands. A name that matches an existing file replaces it, and the old content is lost except for stored variables. The same bug class shows up in two importers, where two source items can map to one file.

Checks done on current `main`:

- The three saves end in `atomic_write`, which writes a temp file and renames it over the target (`crates/rocket-infra/src/atomic_write.rs`). They read the old file only to keep `runtime.variables`.
- No UI check exists for these types, and no commit since `e8a44106` changed these paths.
- A prior known issue: the `hard_link` write used by `create_request_exclusive` fails on exFAT, FAT and some SMB shares (`Nl-knowledge-base/rocketapi/known-issues-gotchas.md:173-175`). Creation of the three types works there today, so a hard-link-only create would be a regression.

## Current state

| Area | Today | Where |
|---|---|---|
| Dialog | GraphQL, gRPC, WebSocket call `saveGraphQlRequest`, `saveGrpcRequest`, `createWebSocketItem`; no existing-name check | `src/components/request/CreateRequestDialog.tsx:91-146`, `src/lib/websocket-create.ts:11-26` |
| Error toast | `createRequestErrorMessage` maps "already exists" to a message; the dialog catch already uses it | `src/lib/create-request-error.ts`, `CreateRequestDialog.tsx:174-177` |
| Wrappers | `save_graphql_request`, `save_grpc_request`, `save_websocket_request` | `src/lib/tauri-api.ts:941-954` |
| Commands | thin wrappers over the service saves; `create_request` is HTTP only | the `save_graphql_request`, `save_grpc_request`, `save_websocket_request` and `create_request` commands in `src-tauri/src/commands/collections.rs` (about `:115-197`), `src-tauri/src/lib.rs:998-1003` |
| Service | `save_*` under the collection lock; `create_request` does `path_exists` then `create_request_exclusive_unlocked` | `crates/rocket-app/src/collection_service.rs:204-264,497-543` |
| Trait | `path_exists` (case-insensitive, `fold_name`); `create_request_exclusive` takes only `&Request` and has a default returning `Internal` | `crates/rocket-collection/src/repository.rs:199-229` |
| Infra | `create_request_exclusive` writes a hidden temp file, then `hard_link`; typed saves use `atomic_write` | `crates/rocket-infra/src/fs_collection/requests.rs:133-190,259,332,437`, `fs_collection/mod.rs:289-304` |
| Forwarding | `SharedPathCollectionRepo` forwards each method explicitly | `crates/rocket-infra/src/shared_path_collection_repo.rs:195-202` |
| Test double | `NullCollectionRepo` has a hook field, `on_*` builder and impl per exclusive method | `crates/rocket-app/src/test_doubles.rs:788,822,959,1039` |
| Importers | Bruno saves `rel_path.with_extension("yml")` with no clash check; Postman uses `sanitize_postman_filename` with no uniqueness | `crates/rocket-import/src/importer.rs:395-436,708`, `:1274` |

## Non-goals

- "Save to collection" (`SaveToCollectionDialog.tsx:75`, through `saveTabRequest`). It can overwrite all four types and is a separate ticket.
- Changing any `save_*` command. Tab save, auto-save, rename and docs edits rely on overwrite.
- The HTTP `create_request_exclusive` behaviour, including its exFAT limitation.
- `duplicate_request`, `rename_request` and `move_item_no_replace`.

## Design

Flow: `Dialog -> create_<type>_request command -> CollectionService (lock, path_exists, exclusive create, publish, re-read) -> repo exclusive write`.

### 1. Trait methods in `rocket-collection`

Three methods, each with a default body returning `DomainError::Internal`, like `create_request_exclusive`, so only the implementors below change:

```rust
fn create_graphql_request_exclusive(&self, collection: &str, path: &str, request: &GraphQlRequest) -> DomainResult<String>;
fn create_grpc_request_exclusive(&self, collection: &str, path: &str, request: &GrpcRequest) -> DomainResult<String>;
fn create_websocket_request_exclusive(&self, collection: &str, path: &str, request: &WebSocketRequest) -> DomainResult<String>;
```

Each returns the collection-relative path of the new file and fails with `AlreadyExists` when the file is there. The request types are `GraphQlRequest`, `GrpcRequest` and `WebSocketRequest`, the same ones the `save_*` methods take (`repository.rs:96,125,144`). The converters in `requests.rs` are `graphql_to_oc`, `grpc_to_oc` and `websocket_to_oc_websocket`, and the typed re-reads are `get_graphql_request`, `get_grpc_request` and `get_websocket_request`.

### 2. Infra write (`rocket-infra`)

- Extract the write step of `create_request_exclusive` into a private helper `write_new_file_exclusive(dir, final_name, bytes) -> DomainResult<()>` in `requests.rs`. It writes a hidden temp file with `create_new` and `sync_all`, then `hard_link`s it to the final name and removes the temp file.
- A `hard_link` error of kind `AlreadyExists` maps to `DomainError::AlreadyExists("{collection}/{path}")`.
- Any other `hard_link` error (an unsupported filesystem) falls back to `fallback_create_new(final_path, bytes)`: `OpenOptions::new().write(true).create_new(true).open(final)`, write all bytes, `sync_all`. If the write fails after the open succeeded, remove the file. If the open itself fails with `AlreadyExists`, map it to `AlreadyExists` and delete nothing: the existing file is not ours. This keeps the create exclusive and working on exFAT, FAT and SMB.
- Test seam: `write_new_file_exclusive` takes the link step as a parameter (`link: impl Fn(&Path, &Path) -> io::Result<()>`), and production passes `std::fs::hard_link`. Tests pass a closure that returns `ErrorKind::Unsupported` to drive the fallback, and `fallback_create_new` is also tested directly.
- Unlike the saves, the exclusive create does not call `create_dir_all`: a missing parent folder is an error, as for HTTP create. The dialog always creates inside an existing folder.
- The HTTP `create_request_exclusive` keeps its current code and behaviour (no fallback). It does not call the new helper.
- Three functions, one per type, in `requests.rs`: validate the collection name, take the repo mutex, reject an empty uid with `Internal`, normalise with `request_filename_for`, `validate_path`, convert with the same `*_to_oc` function the matching `save_*` uses, `serde_yaml::to_string`, then call the helper. No existing file means no variables to preserve.
- Wire the three in `fs_collection/mod.rs` next to `:297` and forward them in `shared_path_collection_repo.rs` next to `:202`.

### 3. Service (`rocket-app`)

Three methods `create_graphql_request`, `create_grpc_request`, `create_websocket_request` in `collection_service.rs`, each copying `create_request` (`:528-543`): compute `request_filename_for(path)`, take `self.locked(collection, ...)`, call `self.repo.path_exists` and return `AlreadyExists("{collection}/{file}")` when true, call the exclusive repo method, publish `DomainEvent::RequestSaved { collection, path }`, and return the re-read item through the matching `get_*_request`. `NullCollectionRepo` gains, per type, a hook field of a type matching that request (the existing `SaveRequestHook` is typed for `Request`), an `on_create_*_exclusive` builder and an impl, plus the `on_get_graphql_request`, `on_get_grpc_request` and `on_get_websocket_request` hooks if they do not exist yet. `dup_service` (`collection_service.rs:1377`) is HTTP-only, so the new tests use a new small fixture, or `dup_service` is extended.

### 4. Tauri commands

`create_graphql_request`, `create_grpc_request`, `create_websocket_request` in `src-tauri/src/commands/collections.rs`, each `#[tauri::command(async)]` with `(collection, path, request, svc)` returning the saved type, one line `svc.create_*`. Register them in `lib.rs` next to `:998`. Errors stay `DomainError` values.

### 5. Frontend

- `src/lib/tauri-api.ts`: `createGraphQlRequest`, `createGrpcRequest`, `createWebSocketRequest`, each `invoke` of the new command with the same payload shape as the matching `save*` wrapper.
- `CreateRequestDialog.tsx` GraphQL (`:92`) and gRPC (`:117`) branches use the new wrappers. `createWebSocketItem` (`websocket-create.ts:25`) uses `createWebSocketRequest`.
- The existing catch shows `createRequestErrorMessage(err, trimmedName)`. No new UI.

### 6. Importers (`rocket-import`)

Within one import run, a second item that would write to an already used file gets a numeric suffix instead of replacing it.

- One run-wide `&mut HashSet<String>` is threaded through the recursive `walk_requests` (`importer.rs:323`) and Postman's `write_postman_items` (`:683`). Both importers target a fresh collection (`collection_repo.create` runs first), so only files this run wrote can clash.
- Bruno `walk_requests` (`:395-436`): the key is `rocket_collection::fold_name` of the full relative output path including the folder, with the `.yml` name. When the key is taken, take the extension-less relative path as the stem and call `rocket_collection::candidate_filename(stem, n)` with `n` starting at 2 (`a 2.yml`, `a 3.yml`, and so on) until a key is free, then save under that name. This covers case variants such as `Get.bru` and `get.bru`. The walk also accepts `.yml` and `.yaml` sources, but the spec does not assume `bru::parse_file` parses them, so the test fixture uses case variants.
- Postman (`:704-708`): the same set, keyed by the folded `request_filename_for(path)` with the folder prefix, so two items with the same sanitized name in one folder get distinct files. A folder and a request with one name do not clash (folders are directories).
- Known and out of scope: the Bruno HTTP branch does `let _ = repo.save_request(...)` and counts a failed save as imported (`:412-414`), so tests assert file names, not the imported count.
- The request `name` stored inside the file is unchanged. A renamed file counts as imported, with no new report field.
- OpenAPI and WSDL imports already deduplicate with `unique_segment` and are not changed.

## Error handling and edge cases

| Case | Behaviour |
|---|---|
| Name exists exactly | `AlreadyExists`, toast "A request named ... already exists here", no write, no event |
| Name exists in another case (`Foo` vs `foo`) | `AlreadyExists` from `path_exists` (case-insensitive) |
| Folder missing | Same error as HTTP create (the parent folder must exist) |
| Empty uid in the payload | `Internal`, as the saves do |
| Filesystem without hard links | Falls back to `create_new`, still exclusive |
| Crash during the fallback write | May leave a truncated new file; a write error removes it (only a file this call created), a hard crash is accepted |
| Two creates race on the same name | The collection lock and repo mutex serialise in-process; an exact-name race is closed by the link or `create_new` |
| Tab save, auto-save, rename, docs edit | Unchanged, still overwrite |
| Bruno `Get.bru` and `get.bru` in one folder | Second becomes `get 2.yml` |
| Two Postman items with one sanitized name | Second gets a numeric suffix |

## Testing

- `rocket-infra` (`fs_collection/tests.rs`, fixture `setup()`): `create_graphql_request_exclusive_never_replaces_a_file`, `create_grpc_request_exclusive_never_replaces_a_file`, `create_websocket_request_exclusive_never_replaces_a_file`, `exclusive_create_falls_back_when_hard_link_is_unsupported` (the injected link closure returns `ErrorKind::Unsupported`), `fallback_create_new_maps_a_clash_to_already_exists_and_keeps_the_file`, `fallback_create_new_removes_the_file_when_the_write_fails`.
- `rocket-app` (`collection_service.rs` tests, a new fixture built on `NullCollectionRepo`, patterned on `dup_service`): per type, `create_<type>_request_refuses_a_name_that_exists_even_in_another_case` and `create_<type>_request_writes_a_new_name_and_publishes`.
- `rocket-import`: `bruno_import_keeps_both_files_when_case_variants_clash`, `bruno_import_numbers_a_third_clash`, `postman_import_keeps_both_items_with_one_sanitized_name`.
- Frontend (append to the four existing files, mocks extended with the new wrappers): `CreateRequestDialog.test.tsx`, `CreateRequestDialog.grpc.test.tsx`, `CreateRequestDialog.websocket.test.tsx` each get a test that an "already exists" rejection shows the toast and opens no tab; `websocket-create.test.ts` gets `uses the exclusive create`.
- Commands: `cargo test -j4 -p rocket-collection`, `-p rocket-infra`, `-p rocket-app <filter>`, `-p rocket-import <filter>`, then `yarn tsc --noEmit`, `yarn check`, `yarn test --run <patterns>`.

## Files changed

- `crates/rocket-collection/src/repository.rs`
- `crates/rocket-infra/src/fs_collection/requests.rs`, `fs_collection/mod.rs`, `fs_collection/tests.rs`, `shared_path_collection_repo.rs`
- `crates/rocket-app/src/collection_service.rs`, `test_doubles.rs`
- `src-tauri/src/commands/collections.rs`, `src-tauri/src/lib.rs`
- `crates/rocket-import/src/importer.rs`, `crates/rocket-import/CLAUDE.md` (note the dedup rule)
- `src/lib/tauri-api.ts`, `src/components/request/CreateRequestDialog.tsx`, `src/lib/websocket-create.ts`, the four frontend test files

## Constraints

- Do not change `save_graphql_request`, `save_grpc_request` or `save_websocket_request`, in any layer.
- New trait methods have default bodies, so no other implementor changes. `SharedPathCollectionRepo` must forward all three.
- No unwrap calls in production paths, no `git` CLI, DTO camelCase only on IPC DTOs, persistence structs unchanged.
- Cargo commands run per crate. The `src-tauri` check is run by the driver, not by the builder.
- Read `docs/superpowers/specs/opencollection-spec-reference.md` before any task that touches collection data.
- Frontend: shadcn/ui and lucide-react only.

## Acceptance criteria

- Creating a GraphQL, gRPC or WebSocket request with an existing name (any case) fails with an "already exists" toast, writes nothing and opens no tab.
- Creating one with a new name succeeds and publishes `RequestSaved`.
- The exclusive create never replaces an existing file, including when hard links are unsupported.
- Tab save, auto-save, rename and docs edit still overwrite.
- HTTP create is unchanged.
- A Bruno import with `Get.bru` and `get.bru` in one folder produces two files.
- A Postman import with two same-named items produces two files.
- `SharedPathCollectionRepo` forwards the new methods.
- `cargo check -j4` and `cargo test -j4` per touched crate, `yarn tsc --noEmit`, `yarn check` and the targeted tests pass.

## Out of scope

- Save-as-new from "Save to collection".
- Fixing the HTTP exFAT limitation.
- Reporting renamed import files in the import report.

## Follow-ups

- Make "Save to collection" refuse to replace an existing file for all four request types.
- Apply the `create_new` fallback to the HTTP `create_request_exclusive` so HTTP create works on exFAT, FAT and SMB.

## Decisions

- Does this job also fix "Save to collection" overwrite? Options: separate ticket; include it. Chosen: separate ticket. Decided by: user
- What does the new exclusive create do where hard links fail? Options: fall back to `create_new`; fail like HTTP; fix HTTP too. Chosen: fall back to `create_new` for the three new creates only. Decided by: user
- Are importer clashes in scope? Options: out of scope; include a dedup suffix. Chosen: include a dedup suffix for Bruno and Postman. Decided by: user
- One trait method per type, or one generic method? Chosen: one per type with default bodies, because the types differ and the defaults keep the change to three implementors. Decided by: call
- Where does the existing-name check live? Chosen: in the service, copying `create_request` (`path_exists` under the lock, then the exclusive write), because `hard_link` alone only catches an exact-case match on Linux. Decided by: call
- Share the write code with HTTP? Chosen: no. The HTTP method keeps its code, so HTTP behaviour cannot change in this job. Decided by: call
- How are renamed import files named and reported? Chosen: numeric suffix in the `candidate_filename` style, no new report field. Decided by: call
- How is the work split into plans? Chosen: plan 1 backend (trait, infra, service, test doubles, commands), plan 2 frontend, plan 3 importers. Plan 1 touches `rocket-infra` and `src-tauri`, so it will stop for a human approve. Decided by: call
- How is the fallback tested without a real exFAT disk? Chosen: the helper takes the link step as a parameter and tests inject a closure that fails with `ErrorKind::Unsupported`. Decided by: call
- Which Bruno fixture proves the dedup? Chosen: case variants (`Get.bru`, `get.bru`), because it is not verified that `bru::parse_file` accepts `.yml` sources. Decided by: call
