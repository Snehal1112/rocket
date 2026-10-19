# Duplicate Request Backend, Plan 1 (backend) Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** A backend `duplicate_request` command copies an HTTP request by its full path with a fresh uid and a unique name in the source's folder, and never overwrites a file.

**Architecture:** `CollectionService::duplicate_request` reads the source through the repository trait, builds the copy, probes names with `path_exists` and creates the file with `create_request_exclusive`, retrying on `AlreadyExists`. A thin Tauri command exposes it.

**Tech Stack:** Rust, the real `FsCollectionRepo` in end-to-end tests.

**Spec:** `docs/superpowers/specs/2026-10-19-duplicate-request-backend-design.md`

## Global Constraints

- Only HTTP requests are duplicated. Any other kind returns `DomainError::InvalidInput("Only HTTP requests can be duplicated")` and writes nothing.
- The copy copies every field of the source except `uid` (`generate_uid()`), `name`, `seq` (`None`) and `file_name` (`None`).
- Names: suffix `" copy"` for the first try and `" copy {n}"` from `n = 2`, up to 1000 tries, then `DomainError::Conflict`. The file stem comes from the source file name with `.yml`, `.yaml` or `.json` removed, and the copy is always written as `.yml`. The display name is `{source.name}{suffix}`.
- A candidate is skipped when `path_exists` is true or returns an error. `AlreadyExists` from `create_request_exclusive` moves on to the next name. Other errors are returned.
- The empty-uid guards in `rocket-infra` are not changed.
- No unwrap calls in production paths. Tests use `.expect("...")` and `expect_err`.
- Commands stay thin. Cargo: `cargo check -j4 -p <crate>` and `cargo test -j4 -p <crate> <filter>`, never a workspace command.

## Review Focus

1. A source whose display name holds `/` or `:` must still produce a valid file path (test `duplicate_request_uses_the_source_file_stem_for_the_path`).
2. A legacy `Foo.json` source gives `Foo copy.yml` (test `duplicate_request_of_a_legacy_json_source_writes_a_yml_copy`).
3. A source in the collection root gives a path with no leading `/` (tests `duplicate_request_in_the_root_has_no_leading_slash` and `duplicating_a_request_in_the_root_has_no_leading_slash`).
4. A case variant such as `login copy.yml` counts as taken (test `a_case_variant_counts_as_taken`).
5. Two duplicates started at once end up as two different files, never one overwritten (test `two_duplicates_from_two_threads_get_different_files`).

## File Structure

- Modify `crates/rocket-app/src/test_doubles.rs`: three hooks on `NullCollectionRepo`.
- Modify `crates/rocket-app/src/collection_service.rs`: `duplicate_request` and its unit tests.
- Create `crates/rocket-app/src/duplicate_request_e2e_tests.rs`, declared in `lib.rs` as `#[cfg(test)] mod duplicate_request_e2e_tests;`.
- Modify `src-tauri/src/commands/collections.rs` and `src-tauri/src/lib.rs`: the command and its registration.

### Task 1: duplicate_request in CollectionService

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/test_doubles.rs` (`NullCollectionRepo`: `CollectionMethod` enum ~712, `on_*` builders ~819-905, trait impls ~955-990)
- Modify: `crates/rocket-app/src/collection_service.rs` (after `create_request_exclusive`, ~424; `mod tests` ~592)

**Interfaces:**
- Produces:
  - `CollectionService::duplicate_request(&self, collection: &str, path: &str) -> DomainResult<Request>`.
  - On `NullCollectionRepo`: `on_request_kind(self, f: impl Fn(&str, &str) -> DomainResult<RequestKind> + Send + Sync + 'static) -> Self`, `on_path_exists(self, f: impl Fn(&str, &str) -> DomainResult<bool> + Send + Sync + 'static) -> Self` and `on_create_request_exclusive(self, f: impl Fn(&str, &str, &Request) -> DomainResult<String> + Send + Sync + 'static) -> Self`. The matching trait methods consult the hook first, as `get_request` does with `on_get_request`.
- Consumes: `CollectionRepository::{request_kind, get_request, path_exists, create_request_exclusive}`, `rocket_collection::generate_uid`.

- [ ] **Step 1: Read** `collection_service.rs:424-436` (`create_request_exclusive`) and the test `move_item_emits_item_moved` (~1019), `test_doubles.rs:861-905` (`on_get_request`, `on_move_item`) and the `get_request` impl ~955, `repository.rs:100-110,200-230` and `request.rs:76-84` (`request_filename_for`).
- [ ] **Step 2: Write the failing tests** in `collection_service.rs` `mod tests`. Build the service like `move_item_emits_item_moved`, with `NullCollectionRepo` hooks: `on_request_kind` returns `RequestKind::Http`, `on_get_request` returns a fixture source `Request::new("Login", HttpMethod::Get, "http://x".to_string())` with `seq = Some(3)` and `file_name = Some("Login.yml".into())`, `on_path_exists` returns false, and `on_create_request_exclusive` records `(path, request)` into a `Mutex<Vec<_>>` and returns the path. Event assertions use `matches!`. Tests:
  - `duplicate_request_creates_a_copy_with_a_new_uid_and_name`: `duplicate_request("c","auth/Login.yml")` records the path `auth/Login copy.yml`, the request has `name == "Login copy"`, `uid` non-empty and different from the source uid.
  - `duplicate_request_clears_seq`: the recorded request has `seq == None` and `file_name == None`.
  - `duplicate_request_copies_every_other_field`: set headers, body, auth, scripts, tests and variables on the source. The recorded request equals the source with `uid`, `name`, `seq` and `file_name` replaced.
  - `duplicate_request_rejects_a_graphql_request`: `on_request_kind` returns `GraphQl`, the result is `Err(DomainError::InvalidInput(_))` and nothing was recorded.
  - `duplicate_request_of_a_missing_request_is_not_found`: `on_request_kind` returns `Err(DomainError::NotFound("x".into()))`, the same error is returned.
  - `duplicate_request_publishes_request_saved`: the publisher saw one `DomainEvent::RequestSaved { collection: "c", path: "auth/Login copy.yml" }`. This test needs `on_get_request` to also answer for the new path, so give the hook a closure that returns the fixture for any path.
  - `duplicate_request_skips_a_taken_name`: `on_path_exists` is true for `auth/Login copy.yml`, the recorded path is `auth/Login copy 2.yml` and the name is `Login copy 2`.
  - `duplicate_request_retries_the_next_name_when_the_file_appears_meanwhile`: `on_create_request_exclusive` returns `Err(DomainError::AlreadyExists("x".into()))` on the first call and the path on the second. The final path is `auth/Login copy 2.yml`.
  - `duplicate_request_gives_up_after_1000_names`: `on_path_exists` is always true, the result is `Err(DomainError::Conflict(_))` and `on_path_exists` was called 1000 times.
  - `duplicate_request_skips_a_candidate_when_the_existence_check_fails`: `on_path_exists` errors for the first candidate and is false after, the recorded path is `auth/Login copy 2.yml`.
  - `duplicate_request_uses_the_source_file_stem_for_the_path`: the source has `name = "A/B: C"` and file `auth/ab.yml`, the recorded path is `auth/ab copy.yml` and the name is `A/B: C copy`.
  - `duplicate_request_of_a_legacy_json_source_writes_a_yml_copy`: source path `auth/Foo.json`, recorded path `auth/Foo copy.yml`.
  - `duplicate_request_in_the_root_has_no_leading_slash`: source path `Login.yml`, recorded path `Login copy.yml`.
- [ ] **Step 3: Run** `cargo test -j4 -p rocket-app duplicate_request`. Expected: compile errors, `no method named 'on_request_kind'` and `no method named 'duplicate_request'`.
- [ ] **Step 4: Implement** the three hooks and the trait methods on `NullCollectionRepo` (a hook wins, otherwise the strict gate applies as for the other methods; add the three methods to `CollectionMethod` if the gate needs them). Implement `duplicate_request` following the Global Constraints: split the path at the last `/`, strip the extension, loop `n` in `1..=1000`, and delegate to `self.create_request_exclusive` so the event is published once.
- [ ] **Step 5: Run** `cargo test -j4 -p rocket-app duplicate_request` (expected: 13 passed), then `cargo test -j4 -p rocket-app collection_service` and `cargo check -j4 -p rocket-app` (clean).

### Task 2: Real-filesystem end-to-end tests

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-app/src/duplicate_request_e2e_tests.rs`
- Modify: `crates/rocket-app/src/lib.rs` (declare the module next to `flow_request_ref_e2e_tests`)

**Interfaces:**
- Consumes: `CollectionService::duplicate_request`, `rocket_infra::FsCollectionRepo`, `tempfile`, `RecordingPublisher`/`SharedPublisher` from `test_doubles.rs`.
- Produces: harness `fn world() -> (TempDir, CollectionService, Arc<RecordingPublisher>)` with collection `c`, built like the `world()` of `flow_request_ref_e2e_tests.rs` (real `FsCollectionRepo::new_standalone`, folder `auth`, requests saved with `Request::new`).

- [ ] **Step 1: Read** `flow_request_ref_e2e_tests.rs:1-60` (the harness to copy), `repository.rs` `get_request` and `request.rs` `Request::new`.
- [ ] **Step 2: Write the failing tests.** Seed `auth/Login.yml` (display name `Login`, URL `http://one`) and, in the same collection, `other/Login.yml` (display name `Login`, URL `http://two`). Tests:
  - `duplicating_a_request_in_a_folder_stays_in_that_folder`: `duplicate_request("c","auth/Login.yml")` returns a request whose file exists at `<base>/c/auth/Login copy.yml`, and nothing was written at the collection root.
  - `duplicating_twice_makes_copy_then_copy_2`: two calls give `Login copy.yml` and `Login copy 2.yml`, names `Login copy` and `Login copy 2`.
  - `duplicating_never_overwrites_an_existing_copy`: write `auth/Login copy.yml` with URL `http://keep` first. After one duplicate that file still holds `http://keep` and the new file is `Login copy 2.yml`.
  - `duplicating_uses_the_full_path_not_the_display_name`: `duplicate_request("c","other/Login.yml")` returns a copy whose URL is `http://two` and which sits in `other/`.
  - `a_case_variant_counts_as_taken`: write `auth/login copy.yml` first. The duplicate becomes `Login copy 2.yml`. On a case-sensitive filesystem the check still holds because `path_exists` folds case.
  - `the_copy_has_its_own_uid_and_is_listed_in_the_tree`: the returned uid differs from the source uid, and `get_collection("c")` lists both requests under `auth`. Emit check: the publisher saw `RequestSaved` for the copy path.
  - `duplicating_a_request_in_the_root_has_no_leading_slash`: seed `Solo.yml`, the copy is `<base>/c/Solo copy.yml`.
  - `two_duplicates_from_two_threads_get_different_files`: wrap the service in `Arc`, run `std::thread::scope` with two threads each calling `duplicate_request("c","auth/Login.yml")`, and assert both succeed, return different paths, and both files exist.
- [ ] **Step 3: Run** `cargo test -j4 -p rocket-app duplicate_request_e2e`. Expected: compile error until the module is declared, then all pass or fail only on real defects. If a test fails for a production reason, fix `duplicate_request` in Task 1's code and record it in the ledger.
- [ ] **Step 4: Fix** any production defect the tests expose, keeping Task 1's unit tests green.
- [ ] **Step 5: Run** `cargo test -j4 -p rocket-app duplicate_request` (expected: all unit and end-to-end tests pass, 21 in total) and `cargo check -j4 -p rocket-app`.

### Task 3: IPC command

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

This task is wiring only, with no new logic. The behaviour is pinned by Tasks 1 and 2, so its checks are a compile and a registration scan.

**Files:**
- Modify: `src-tauri/src/commands/collections.rs` (near `save_request`, ~189)
- Modify: `src-tauri/src/lib.rs` (`generate_handler!`)

**Interfaces:**
- Consumes: `CollectionService::duplicate_request`.
- Produces: Tauri command `duplicate_request(collection: String, path: String, svc: State<'_, CollectionService>) -> Result<Request, DomainError>`, which calls `svc.duplicate_request(&collection, &path)` and nothing else.

- [ ] **Step 1: Read** `commands/collections.rs:185-200` (the `save_request` command) and the `commands::collections::` entries in `generate_handler!` in `lib.rs`.
- [ ] **Step 2: Implement** the command next to `save_request` with the same imports, and register `commands::collections::duplicate_request` beside the other collection commands.
- [ ] **Step 3: Verify.** Run `cargo check -j4 -p rocket` (clean, this builds the whole app and takes a while). Run `grep -n "duplicate_request" src-tauri/src/commands/collections.rs src-tauri/src/lib.rs`. Expected: one definition and one registration. Run `cargo test -j4 -p rocket-app duplicate_request`. Expected: 21 passed.

Next plan: docs/superpowers/plans/2026-10-19-duplicate-request-backend/plan-02-duplicate-request-backend.md
