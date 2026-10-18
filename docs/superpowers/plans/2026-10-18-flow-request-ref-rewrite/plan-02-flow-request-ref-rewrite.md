# Flow Request Ref Rewrite, Plan 2 (CollectionService hook) Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** `CollectionService` rewrites and reports flow references after a move, and real-filesystem tests prove it end to end.

**Architecture:** `CollectionService` gets an optional `FlowReferenceUpdater`. After a successful `move_item` or `move_item_no_replace` it either rewrites (same collection) or looks up the dangling flows (other collection), and publishes one `FlowRequestPathsRewritten` event. Nothing is wired in the app yet, so runtime behaviour is unchanged until Plan 3.

**Tech Stack:** Rust, `tempfile`, the real `FsCollectionRepo` and `FsFlowRepo` in tests.

**Spec:** `docs/superpowers/specs/2026-10-18-flow-request-ref-rewrite-design.md`

## Global Constraints

- Plan 1 is merged: `FlowReferenceUpdater`, `FlowRewriteReport`, `DomainEvent::FlowRequestPathsRewritten`, `FlowRewriteFailure`, and `flow_references::test_support`.
- A same-collection rewrite needs a non-empty `dst_path` and `src_path != dst_path`, compared with one trailing `/` stripped. A move to another collection runs no rewrite.
- A failed rewrite or a failed lookup never fails the move. A failed repo move runs nothing and publishes nothing.
- Without an updater, `CollectionService` behaves exactly as before.
- No unwrap calls in production paths. Tests use `.expect("...")`.
- Cargo: `cargo check -j4 -p rocket-app` and `cargo test -j4 -p rocket-app <filter>`, never a workspace command.

## Review Focus

1. The move must succeed when the rewrite reports failures or the lookup errors: both return `Ok(())` (tests `move_item_succeeds_when_a_flow_rewrite_fails`, `move_item_to_another_collection_ignores_a_lookup_error`).
2. `("c","a.yml","c","")` and a move onto the same path must not rewrite (test `move_item_skips_the_rewrite_for_an_empty_destination_or_same_path`).
3. A trailing slash on a folder path, as the sidebar may send, must still match (test `move_item_strips_trailing_slashes_before_rewriting`).
4. A flow that cannot be parsed stays byte for byte as it was and the move succeeds (test `a_corrupt_flow_file_is_left_untouched_and_the_move_succeeds`).
5. A flow of a different collection that uses the same path string is never touched (test `a_flow_of_another_collection_is_untouched`).

## File Structure

- Modify `crates/rocket-app/src/collection_service.rs`: field, builder, hook, tests.
- Modify `crates/rocket-app/src/test_doubles.rs`: `NullCollectionRepo::on_move_item`.
- Create `crates/rocket-app/src/flow_request_ref_e2e_tests.rs`: real filesystem tests, declared in `lib.rs` as `#[cfg(test)] mod flow_request_ref_e2e_tests;`.

### Task 1: Same-collection rewrite in CollectionService

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/collection_service.rs` (struct line 14, builders 23-45, `move_item_no_replace` 373, `move_item` 436, `mod tests` 592)
- Modify: `crates/rocket-app/src/test_doubles.rs` (`NullCollectionRepo` `on_*` builders ~815, `move_item` impl ~982)

**Interfaces:**
- Consumes: `FlowReferenceUpdater`, `FlowRewriteReport`, `DomainEvent::FlowRequestPathsRewritten`.
- Produces:
  - `CollectionService::with_flow_references(mut self, updater: Arc<dyn FlowReferenceUpdater>) -> Self`.
  - private `fn after_move(&self, src_collection: &str, src_path: &str, dst_collection: &str, dst_path: &str)`, called by both move methods after the repo move succeeded.
  - `NullCollectionRepo::on_move_item(self, f: impl Fn(&str, &str, &str, &str) -> DomainResult<()> + Send + Sync + 'static) -> Self`, consulted by its `move_item` before the strict gate, as `get_settings` does.

- [ ] **Step 1: Read** `collection_service.rs:14-45,373-390,436-453,1019-1040` and the `NullCollectionRepo` hooks in `test_doubles.rs` (`on_get`, and `get_settings` ~990).
- [ ] **Step 2: Write the failing tests** in `collection_service.rs` `mod tests`. Add a local `RecordingUpdater` (`calls: Mutex<Vec<(String,String,String)>>`, `report: Mutex<FlowRewriteReport>`) implementing the trait, with `flows_using` returning `Ok(vec![])`. Build the service like `move_item_emits_item_moved` does, with `.with_flow_references(updater.clone())` and the existing `RecordingEventPublisher`. Event assertions use `matches!`, since the event has no `PartialEq`. Tests:
  - `move_item_rewrites_flows_in_the_same_collection`: `move_item("c","auth","c","identity")` gives `calls == [("c","auth","identity")]`. With a report of `rewritten = ["A"]`, one `FlowRequestPathsRewritten { collection: "c", from: "auth", to: "identity", flows == ["A"], failed empty }` is published besides `ItemMoved`.
  - `move_item_does_not_rewrite_when_the_move_fails`: the repo is built with `on_move_item(|..| Err(DomainError::NotFound("x".into())))`. The call returns `Err`, `calls` is empty, no event is published.
  - `move_item_no_replace_rewrites_flows`: the same as the first test through `move_item_no_replace`.
  - `move_item_succeeds_when_a_flow_rewrite_fails`: a report with one `failed` entry. The move returns `Ok(())` and the event carries the entry.
  - `move_item_publishes_no_flow_event_when_nothing_was_rewritten`: an empty report publishes only `ItemMoved`.
  - `move_item_without_an_updater_behaves_as_before`: no builder call, only `ItemMoved` is published.
  - `move_item_skips_the_rewrite_for_an_empty_destination_or_same_path`: `("c","a.yml","c","")` and `("c","a","c","a/")` leave `calls` empty.
  - `move_item_strips_trailing_slashes_before_rewriting`: `("c","auth/","c","identity/")` gives `calls == [("c","auth","identity")]`.
- [ ] **Step 3: Run** `cargo test -j4 -p rocket-app move_item`. Expected: compile error, `no method named 'with_flow_references'`.
- [ ] **Step 4: Implement.** Add `flow_refs: Option<Arc<dyn FlowReferenceUpdater>>` (None in both constructors), the builder, and `after_move`. For this task `after_move` handles only the same-collection case: strip one trailing `/` from both paths, return when there is no updater, the collections differ, `dst` is empty or the paths are equal, otherwise call `rewrite_request_paths` and publish the event when `rewritten` or `failed` is non-empty. Call `after_move` after the repo move succeeds, before `Ok(())`, in both methods. Add `on_move_item`.
- [ ] **Step 5: Run** `cargo test -j4 -p rocket-app move_item` (expected: all pass, including the existing `move_item_emits_item_moved`), then `cargo test -j4 -p rocket-app collection_service` and `cargo check -j4 -p rocket-app`.

### Task 2: Cross-collection moves report dangling flows

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/collection_service.rs` (`after_move`, `mod tests`)

**Interfaces:**
- Consumes: Task 1's `after_move` and `RecordingUpdater`, which gains `using: Mutex<DomainResult<Vec<String>>>` and `using_calls: Mutex<Vec<(String,String)>>`.
- Produces: the event for a move to another collection carries empty `flows` and `failed == [FlowRewriteFailure { flow, message: "moved out of the collection" }]`, one entry per flow that used the item.

- [ ] **Step 1: Read** Task 1's `after_move` and the spec's section 3 and edge-case table.
- [ ] **Step 2: Write the failing tests.**
  - `move_item_to_another_collection_reports_dangling_flows`: `using = Ok(["A"])`, `move_item("c","a.yml","d","a.yml")` leaves `calls` empty, `using_calls == [("c","a.yml")]`, and one event is published with `collection: "c"`, `from: "a.yml"`, `to: "a.yml"`, empty `flows` and the failure above.
  - `move_item_to_another_collection_ignores_a_lookup_error`: `using = Err(DomainError::Internal("x".into()))`. The move returns `Ok(())` and no rewrite event is published.
  - `move_item_to_another_collection_with_no_users_publishes_no_flow_event`: `using = Ok(vec![])`.
  - `move_item_to_another_collection_with_an_empty_destination_still_reports`: `("c","a.yml","d","")` (the "move to collection" menu) still calls the lookup and publishes the event.
- [ ] **Step 3: Run** `cargo test -j4 -p rocket-app move_item_to_another_collection`. Expected: the first and fourth tests fail, `using_calls` is empty and no `FlowRequestPathsRewritten` was published.
- [ ] **Step 4: Implement** the other-collection branch of `after_move`: call `flows_using(src_collection, src_path)`, ignore an `Err`, and publish the event only for a non-empty list. The destination is not inspected in this branch.
- [ ] **Step 5: Run** `cargo test -j4 -p rocket-app move_item` (expected: all pass) and `cargo check -j4 -p rocket-app`.

### Task 3: Real-filesystem end-to-end tests

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-app/src/flow_request_ref_e2e_tests.rs`
- Modify: `crates/rocket-app/src/lib.rs` (`#[cfg(test)] mod flow_request_ref_e2e_tests;`, next to the other test modules)

**Interfaces:**
- Consumes: `CollectionService::with_flow_references`, `FlowService`, `rocket_infra::{FsCollectionRepo, FsFlowRepo}` (a dev-dependency), `tempfile`, `rocket_flow::validate`.
- Produces: a harness `fn world() -> (TempDir, CollectionService, Arc<FlowService>, Arc<RecordingPublisher>)` that wires real repositories under one temp base directory, building the repos and the request files as `execution_service/folder_chain_e2e_tests.rs` does, and `RecordingPublisher` from `test_doubles.rs:1391`. The `FlowService` is shared as an `Arc` so the test reads flows back through it.

- [ ] **Step 1: Read** `execution_service/folder_chain_e2e_tests.rs` (how it builds a collection with nested requests on disk), `crates/rocket-infra/src/fs_flow_repo.rs:212-240` (`setup`, `sample`), `flow_references.rs` and `test_doubles.rs:1380-1420`.
- [ ] **Step 2: Write the failing tests.** Each saves flows through `FlowService::save` with Saved nodes built by `flow_with`-style code, then moves through `CollectionService`:
  - `moving_a_request_rewrites_the_saved_flow_on_disk`: collection `c` with request `auth/login.yml` and flow `Checkout` using it. `move_item("c","auth/login.yml","c","identity/login.yml")` leaves the flow reading `identity/login.yml`, the flow file at `<base>/c/flows/` changed on disk, `rocket_flow::validate(&flow)` is `Ok`, and `get_request("c","identity/login.yml")` resolves, so the path a run would use exists. One `FlowRequestPathsRewritten` event names `Checkout`.
  - `renaming_a_folder_rewrites_every_flow_below_it`: requests `auth/login.yml` and `auth/sub/me.yml`, flows `A` (first) and `B` (both). `move_item("c","auth","c","identity")` rewrites both flows, and the event lists `["A","B"]`.
  - `a_corrupt_flow_file_is_left_untouched_and_the_move_succeeds`: next to a good flow, write `<base>/c/flows/broken.yml` with the bytes `nodes: [oops`. After the move the good flow is rewritten, `broken.yml` is byte for byte the same, and the call returned `Ok(())`.
  - `a_flow_of_another_collection_is_untouched`: collection `d` holds a flow using `auth/login.yml`. Moving in `c` leaves it unchanged.
  - `a_move_between_collections_reports_the_dangling_flow`: `move_item("c","auth/login.yml","d","login.yml")` leaves the flow in `c` unchanged and publishes the event with `failed` naming it and `moved out of the collection`.
- [ ] **Step 3: Run** `cargo test -j4 -p rocket-app flow_request_ref_e2e`. Expected: compile error until the module is declared, then the first four tests fail because the harness has no updater wired. Wire it in the harness with `.with_flow_references(flow_service.clone())` only after seeing them fail.
- [ ] **Step 4: Implement** the harness and fix any wiring the tests expose. No production code should be needed beyond Plans 1 and 2. If a test fails for a production reason, fix the production code in the task that owns it and say so in the ledger.
- [ ] **Step 5: Run** `cargo test -j4 -p rocket-app flow_request_ref_e2e` (expected: 5 passed), then `cargo test -j4 -p rocket-app move_item` and `cargo check -j4 -p rocket-app`.

Next plan: docs/superpowers/plans/2026-10-18-flow-request-ref-rewrite/plan-03-flow-request-ref-rewrite.md
