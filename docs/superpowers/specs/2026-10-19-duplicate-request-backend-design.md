# Duplicate request through a backend command

## Goal

Duplicating an HTTP request from the sidebar works through a new backend `duplicate_request` command that copies the source found by its full path, gives the copy a fresh uid, and names it uniquely inside the source's folder without ever overwriting a file.

## Overview

"Duplicate" in the sidebar context menu fails with `Internal error: save_request: empty uid on request for 'auth/Login copy' in collection 'api-4'; callers must construct via Request::new()`. The frontend builds the copy itself and sends `uid: ''`. The Rust `save_request` has refused an empty uid since commit `1d972c7d` (2026-05-04), and nothing on this path generates one.

The same function has two more defects, found while tracing the first:
- It finds the source by the last path segment or the display name, so a same-named request in another folder can be copied instead.
- It picks the copy name by scanning the collection's root items only, and `save_request` overwrites an existing file. Duplicating `auth/Login` twice overwrites `auth/Login copy.yml`.

Moving the logic into the backend fixes all three in one place, keeps the empty-uid guard, and removes business logic from the component.

## Current state

| Piece | Where | Today |
|---|---|---|
| Duplicate handler | `src/components/layout/CollectionsSidebar.tsx:231-285` `handleDuplicate` | Reads the whole collection with `getCollection`, finds the source with `findRequest`, picks a name from root items, calls `saveRequest` with `uid: ''`. |
| Menu entry | `RequestNode.tsx:356-359` and `:433`, only when `kind === 'http'` | Calls `onDuplicate(collectionName, path, name)`. |
| Callback type | `RequestNode.tsx:90`, `FolderNode.tsx:64` | `(collection, path, name) => Promise<void>`. |
| Save command | `src-tauri/src/commands/collections.rs:189` `save_request` | Pass-through. No uid logic. |
| Repo guard | `crates/rocket-infra/src/fs_collection/requests.rs:85` | Rejects an empty uid. `create_request_exclusive` (`:140`) does too and also refuses an existing file with `AlreadyExists`. |
| No-overwrite create | `CollectionService::create_request_exclusive` (`collection_service.rs:424`) | Used only by assistant proposals. No IPC command. |
| Existence check | `CollectionRepository::path_exists` (`repository.rs:204`), `fs_collection/paths.rs:99` | Compares names case-insensitively with `fold_name`, so a case variant counts as taken. |
| uid helper | `rocket_collection::generate_uid` (`lib.rs:46`) | The helper `Request::new` uses. |
| Kind lookup | `CollectionRepository::request_kind` (`repository.rs:107`), `RequestKind::{Http, GraphQl, Grpc, WebSocket}` | Tells HTTP from the other kinds. |
| `seq` | `Request.seq: Option<u32>` (`request.rs:33`), written as `info.seq` | Informational. The sidebar order comes from `_order.yml` and the entry sort (`tree.rs:143`), not from `seq`. |
| Tests | none for Duplicate, none for an empty-uid HTTP save | |

Gaps: no backend duplicate, no unique-name rule per folder, no test.

## Non-goals

- Duplicating GraphQL, WebSocket or gRPC requests. The menu entry exists only for HTTP, and the command refuses other kinds.
- Duplicating folders or whole collections.
- Placing the copy right after the source in `_order.yml`. The copy appears where the default sort puts it.
- Changing the empty-uid guards or `save_request`.
- Opening the copy in a tab after it is created.

## Design

Flow:

```
sidebar Duplicate -> duplicateRequest(collection, path)            (IPC)
  -> CollectionService::duplicate_request(collection, path)
       kind must be Http -> get_request(path) -> build copy -> pick a free name
       -> create_request_exclusive (retry the next name on AlreadyExists)
       -> RequestSaved event -> sidebar refresh (existing listener)
  <- the saved copy (Request)
```

### 1. `CollectionService::duplicate_request`

```rust
pub fn duplicate_request(&self, collection: &str, path: &str) -> DomainResult<Request>
```

Steps:
1. `repo.request_kind(collection, path)`. Any kind but `Http` returns `DomainError::InvalidInput("Only HTTP requests can be duplicated")`. A missing request returns the repo's `NotFound`.
2. `source = repo.get_request(collection, path)`. The source is found by its full path only.
3. Split `path` into its folder and file stem (the file name without `.yml`, `.yaml` or the legacy `.json`, or the whole name when it has no extension). The copy lives in the same folder and is always written as `.yml`, so duplicating a legacy `.json` request also moves the copy to the current format.
4. For `n` in `1..=1000`, with the suffix `" copy"` for `n == 1` and `" copy {n}"` otherwise:
   - file path: `{folder}/{stem}{suffix}.yml` (no leading `/` when the folder is empty);
   - display name: `{source.name}{suffix}`;
   - skip when `repo.path_exists(collection, file path)` is true or errors;
   - build the copy: `Request { uid: generate_uid(), name, seq: None, file_name: None, ..source.clone() }`;
   - call `self.create_request_exclusive(collection, file path, &copy)`. On `AlreadyExists` continue with the next `n`. Any other error is returned.
5. After 1000 tries return `DomainError::Conflict("Could not find a free name for the copy")`.

The result is the request returned by `create_request_exclusive`, which re-reads the saved file. `RequestSaved` is published by that call, so the sidebar refreshes through the existing path.

Example: `auth/Login.yml` (display name `Login`) gives `auth/Login copy.yml` (`Login copy`), then `auth/Login copy 2.yml` (`Login copy 2`). The file stem comes from the source file, not the display name, so a display name with characters that are illegal in file names cannot break the path.

Every field except `uid`, `name`, `seq` and `file_name` is copied as it is: URL, headers, body, auth, scripts, tests, assertions, variables, docs and settings.

### 2. IPC command

`duplicate_request(collection: String, path: String, svc: State<'_, CollectionService>) -> Result<Request, DomainError>` in `src-tauri/src/commands/collections.rs`, registered in `generate_handler!`. It calls `svc.duplicate_request(&collection, &path)` and nothing else.

### 3. Frontend

- `src/lib/tauri-api.ts`: `export const duplicateRequest = (collection: string, path: string) => invoke<Request>('duplicate_request', { collection, path })`.
- `CollectionsSidebar.tsx`: `handleDuplicate(collection, path)` calls `duplicateRequest` and nothing else. A failure shows `toast.error('Could not duplicate the request: <message>')` and logs with `console.error`. The `findRequest`, name-picking and `saveRequest` code is removed.
- The callback type becomes `(collection: string, path: string) => Promise<void>` in `RequestNode.tsx`, `FolderNode.tsx` and the call sites, because the display name is no longer needed.

### 4. Docs

Add a done row `R-64` at the end of section 8 of `.claude/roadmap.md`: "Duplicate request through the backend `duplicate_request` command", with the commit range left to the driver, and list the follow-ups below.

## Error handling and edge cases

| Case | Behaviour |
|---|---|
| Source path does not exist | `NotFound` from the repo, toast in the UI. |
| Source is GraphQL, WebSocket or gRPC | `InvalidInput`, nothing is written. The menu hides the entry for them anyway. |
| `Login copy.yml` already exists | The copy becomes `Login copy 2.yml`. |
| `login copy.yml` exists (different case) | `path_exists` is case-insensitive, so the name counts as taken and `copy 2` is used. |
| Two duplicates race for the same name | `create_request_exclusive` fails with `AlreadyExists` for the loser, which retries the next name. |
| `path_exists` returns an error | That candidate is skipped, so an unreadable listing never reads as free. |
| Two requests share a display name in different folders | The one at the given full path is copied. |
| Source in the collection root | The folder is empty and the copy path has no leading `/`. |
| Source has `seq` set | The copy has `seq` unset. |
| Source display name has `/` or `:` | The file name uses the source file stem, so the path stays valid. The display name keeps the characters. |
| Source is a legacy `Foo.json` | The copy is `Foo copy.yml`. |
| `path` has no extension | The same stem rule applies. |
| `Foo copy.json` exists but `Foo copy.yml` does not | The name is free, since the check compares whole file names. Accepted. |
| 1000 names taken | `Conflict`. |
| Any write error | Returned as is, no partial file (exclusive create links the finished file into place). |

## Testing

- `rocket-app`, service with test doubles (`cargo test -j4 -p rocket-app duplicate_request`): `duplicate_request_creates_a_copy_with_a_new_uid_and_name`, `duplicate_request_clears_seq`, `duplicate_request_copies_every_other_field`, `duplicate_request_rejects_a_graphql_request`, `duplicate_request_of_a_missing_request_is_not_found`, `duplicate_request_publishes_request_saved`, `duplicate_request_skips_a_taken_name`, `duplicate_request_retries_the_next_name_when_the_file_appears_meanwhile`, `duplicate_request_gives_up_after_1000_names`, `duplicate_request_skips_a_candidate_when_the_existence_check_fails`, `duplicate_request_of_a_legacy_json_source_writes_a_yml_copy`.
- `rocket-app`, real filesystem (`cargo test -j4 -p rocket-app duplicate_request_e2e`): `duplicating_a_request_in_a_folder_stays_in_that_folder`, `duplicating_twice_makes_copy_then_copy_2`, `duplicating_never_overwrites_an_existing_copy`, `duplicating_uses_the_full_path_not_the_display_name`, `a_case_variant_counts_as_taken`, `the_copy_has_its_own_uid_and_is_listed_in_the_tree`.
- Frontend (`yarn test CollectionsSidebar --run`, `yarn test RequestNode --run`, `yarn test FolderNode --run`): `duplicate calls duplicateRequest with the collection and the path`, `duplicate failure shows an error toast and logs`, `RequestNode Duplicate passes the collection and path`.
- Checks: `cargo check -j4 -p rocket-app`, `cargo check -j4 -p rocket`, `yarn tsc --noEmit`, `yarn check`.

## Files changed

- `crates/rocket-app/src/collection_service.rs` (the method and its tests)
- `crates/rocket-app/src/test_doubles.rs` (hooks on `NullCollectionRepo` for `path_exists`, `request_kind`, `get_request` and `create_request_exclusive`, as needed)
- `crates/rocket-app/src/duplicate_request_e2e_tests.rs` (new), declared in `lib.rs`
- `src-tauri/src/commands/collections.rs`, `src-tauri/src/lib.rs`
- `src/lib/tauri-api.ts`, `src/components/layout/CollectionsSidebar.tsx`, `src/components/collections/RequestNode.tsx`, `src/components/collections/FolderNode.tsx`, and their tests
- `.claude/roadmap.md`

## Constraints

- Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.
- Commands stay thin. The logic lives in `CollectionService`, which holds the repository trait only.
- No `unwrap` calls in production paths. Tests use `.expect("...")`.
- The empty-uid guards in `rocket-infra` stay as they are.
- Cargo commands are per crate, `cargo check -j4 -p <crate>` and `cargo test -j4 -p <crate> <filter>`, never a workspace command.
- shadcn primitives and lucide icons only. No full destructure of Zustand state.
- `Request` already serializes with camelCase, so the command adds no DTO.

## Acceptance criteria

- Duplicating an HTTP request from the sidebar no longer fails with the empty uid error, and the copy has a new uid.
- The copy is created in the same folder as the source, named `<source> copy`, then `<source> copy 2`, and so on.
- An existing file is never overwritten, including a case variant of the name.
- The source is found by its full path. With two requests of the same display name in different folders, the clicked one is copied.
- The copy keeps every field of the source except uid, name, seq and file name.
- A GraphQL, WebSocket or gRPC request is refused with `InvalidInput` and nothing is written.
- A failure in the sidebar shows an error toast.
- The sidebar component holds no duplicate logic, only the command call.
- The tests in the Testing section exist and pass.
- `R-64` is in `.claude/roadmap.md`.

## Out of scope

- Duplicating non-HTTP requests and folders.
- Ordering the copy next to the source.
- Any change to the empty-uid guards.

## Follow-ups

- Duplicate for GraphQL, WebSocket and gRPC requests.
- Place the copy directly after the source in `_order.yml`.
- Duplicate a folder with everything below it.

## Decisions

- Where does the duplicate logic live? Options: fix only the uid in the frontend, fix all three problems in the frontend, a new backend duplicate_request command. Chosen: a new backend `duplicate_request` command that fixes all three problems. Decided by: user
- Only HTTP requests are duplicated, because the menu entry exists only for them. Decided by: call
- The file stem comes from the source file and the display name gets the same suffix, so illegal characters in a display name cannot break the path. Decided by: call
- The copy has `seq` unset, since the sidebar order does not use `seq`. Decided by: call
- Uniqueness uses `path_exists` (case-insensitive) plus an exclusive create with a retry, so a race never overwrites. Decided by: call
- Failures show an error toast, because the original failure was only visible in the console. Decided by: call
- The `name` argument is dropped from the `onDuplicate` callback. Decided by: call
- The copy is not opened in a tab. Decided by: call
- A legacy `.json` or extensionless source is handled with the same stem rule and the copy is written as `.yml`. Decided by: call
- Test doubles override `path_exists` and `request_kind` for the service tests, since the trait defaults make `path_exists` an error. Decided by: call
