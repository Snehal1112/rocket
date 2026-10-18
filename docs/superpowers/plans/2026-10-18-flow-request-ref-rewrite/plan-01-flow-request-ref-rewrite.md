# Flow Request Ref Rewrite, Plan 1 (domain core) Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** The domain can rewrite and look up the Saved request paths of a collection's flows, and the event that reports a rewrite exists.

**Architecture:** Pure path helpers in `rocket-flow`. A new `DomainEvent` variant in `rocket-shared`. A `FlowReferenceUpdater` trait in `rocket-app` implemented by `FlowService`. Nothing is called yet, so this plan changes no runtime behaviour.

**Tech Stack:** Rust, serde.

**Spec:** `docs/superpowers/specs/2026-10-18-flow-request-ref-rewrite-design.md`

## Global Constraints

- `rocket-flow` stays free of I/O. `rocket-app` holds trait objects only.
- No unwrap calls in production paths. Tests use `.expect("...")` and `expect_err`.
- Matching is exact or `target` + `/`, on whole segments, case sensitive. A trailing `/` is stripped from `target`. An empty target never matches.
- The rewrite saves through `FlowRepository::save` and never calls `validate`. A flow that cannot be read is never written. One bad flow never stops the walk.
- The event adds no serde rename attribute. Its fields are single words.
- Cargo: `cargo check -j4 -p <crate>` and `cargo test -j4 -p <crate> <filter>`, never a workspace command.

## Review Focus

1. A target of `""` or `"/"` must match nothing, never every flow (test `path_is_within_never_matches_an_empty_target`).
2. A folder named `auth` must not touch `author/x.yml` (test `path_is_within_does_not_match_a_longer_sibling_name`).
3. A flow with two Saved nodes on the item is rewritten and listed once (test `flows_using_lists_each_flow_once_sorted`).
4. A flow with a dangling edge (it would fail `validate`) is still rewritten (test `rewrite_request_paths_saves_a_flow_that_would_fail_validation`).
5. A failing flow list never panics or aborts, and a lookup with an empty, absolute or `..` path is refused (tests `rewrite_request_paths_reports_a_list_error`, `flows_using_rejects_bad_arguments`).

## File Structure

- Create `crates/rocket-flow/src/request_refs.rs`: the three pure helpers and their tests.
- Modify `crates/rocket-flow/src/lib.rs`: `pub mod request_refs;` and a `pub use` line.
- Modify `crates/rocket-shared/src/events.rs`: `FlowRewriteFailure` and the new `DomainEvent` variant.
- Modify `src-tauri/src/tauri_event_bus.rs`: one arm in `channel_for` and a test.
- Create `crates/rocket-app/src/flow_references.rs`: trait, report type, `FlowService` impl, `test_support` fakes, tests.
- Modify `crates/rocket-app/src/lib.rs`: `mod flow_references;` and re-exports.

### Task 1: Path helpers in rocket-flow

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-flow/src/request_refs.rs`
- Modify: `crates/rocket-flow/src/lib.rs`

**Interfaces:**
- Produces, in `rocket_flow::request_refs`, re-exported at the crate root:
  - `pub fn path_is_within(request_path: &str, target: &str) -> bool`
  - `pub fn rewrite_saved_paths(flow: &mut Flow, from: &str, to: &str) -> usize`
  - `pub fn uses_saved_path(flow: &Flow, target: &str) -> bool`

- [ ] **Step 1: Read** `crates/rocket-flow/src/node.rs:18-29,149-159` (the `Request` variant and `RequestSource`) and `flow.rs:7-54,89` (`FlowNode`, `Flow`, `sample_flow`).
- [ ] **Step 2: Write the failing tests** in an inline `#[cfg(test)] mod tests` of `request_refs.rs`. Add `flow_with(paths: &[&str]) -> Flow`, which builds one `FlowNodeKind::Request { source: RequestSource::Saved { request_path }, debug: false, repeat_until: None, .. }` node per path with ids `n0, n1...`, built like `sample_flow` does, and `saved_paths(flow: &Flow) -> Vec<String>`. Tests:
  - `path_is_within_matches_exact_and_folder_prefix`: `("auth/login.yml","auth/login.yml")`, `("auth/login.yml","auth")`, `("auth/login.yml","auth/")` and `("auth","auth")` are true.
  - `path_is_within_does_not_match_a_longer_sibling_name`: `("author/x.yml","auth")` is false.
  - `path_is_within_never_matches_an_empty_target`: `("a.yml","")`, `("a.yml","/")` and `("","")` are false.
  - `rewrite_saved_paths_replaces_a_folder_prefix`: flow `["auth/login.yml","auth/sub/me.yml","other/x.yml"]`, `rewrite_saved_paths(&mut f,"auth","identity")` returns `2` and `saved_paths(&f) == ["identity/login.yml","identity/sub/me.yml","other/x.yml"]`.
  - `rewrite_saved_paths_replaces_an_exact_request_path`: flow `["a.yml"]`, `("a.yml","b/a.yml")` returns `1` and the path is `b/a.yml`.
  - `rewrite_saved_paths_ignores_inline_requests`: a flow with one `RequestSource::Inline` node (build `InlineRequestData` as the tests in `node.rs` do) and one Output node returns `0` and equals its clone.
  - `uses_saved_path_is_true_for_a_nested_request`: `["auth/login.yml"]` uses `"auth"` and `"auth/login.yml"`, not `"other"`.
- [ ] **Step 3: Run** `cargo test -j4 -p rocket-flow request_refs`. Expected: compile error, `cannot find function 'path_is_within' in this scope`.
- [ ] **Step 4: Implement** the three functions and declare the module. `path_is_within` strips one trailing `/` from `target`, returns false when the result is empty, then checks equality or `starts_with(target + "/")`. `rewrite_saved_paths` replaces the matching prefix of `from` with `to` on each `FlowNodeKind::Request` Saved node and counts them. `uses_saved_path` reuses `path_is_within`.
- [ ] **Step 5: Run** `cargo test -j4 -p rocket-flow request_refs`. Expected: 7 passed. Then `cargo check -j4 -p rocket-flow`, clean.

### Task 2: Event variant and bus arm

**Files:**
- Modify: `crates/rocket-shared/src/events.rs` (after `ItemMoved`, ~line 304)
- Modify: `src-tauri/src/tauri_event_bus.rs` (`channel_for` ~line 84, tests ~line 128)

**Interfaces:**
- Produces:
  - `rocket_shared::events::FlowRewriteFailure { pub flow: String, pub message: String }`, derive `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`.
  - `DomainEvent::FlowRequestPathsRewritten { collection: String, from: String, to: String, flows: Vec<String>, failed: Vec<FlowRewriteFailure> }`.
  - Event channel name `flow-request-paths-rewritten`.

- [ ] **Step 1: Read** `events.rs:278-310` and `tauri_event_bus.rs:14-34,80-90,128-160`.
- [ ] **Step 2: Write the failing test** in the `tauri_event_bus.rs` test module: `flow_request_paths_rewritten_has_its_own_channel` builds the variant with empty vectors and asserts `channel_for(&event) == "flow-request-paths-rewritten"`. Also add to the `rocket-shared` events tests, if the file has a serde test module, `flow_request_paths_rewritten_serializes_with_a_type_tag`: `serde_json::to_value(&event)` has `"type": "flowRequestPathsRewritten"` and a `failed[0].flow` string.
- [ ] **Step 3: Run** `cargo test -j4 -p rocket-shared flow_request_paths`. Expected: compile error, `no variant named 'FlowRequestPathsRewritten'`.
- [ ] **Step 4: Implement** the struct and the variant in `events.rs`, and the `channel_for` arm. `channel_for` has no wildcard, so the arm keeps the app compiling.
- [ ] **Step 5: Run** `cargo test -j4 -p rocket-shared flow_request_paths` (expected: 1 passed), `cargo check -j4 -p rocket` (expected: clean, this builds the whole app and takes a while), `cargo test -j4 -p rocket flow_request_paths_rewritten_has_its_own_channel` (expected: 1 passed).

### Task 3: FlowReferenceUpdater and its FlowService impl

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-app/src/flow_references.rs`
- Modify: `crates/rocket-app/src/lib.rs` (modules, `pub use flow_service::FlowService` at ~line 106)

**Interfaces:**
- Consumes: `rocket_flow::{rewrite_saved_paths, uses_saved_path}`, `rocket_shared::events::FlowRewriteFailure`.
- Produces:
  - `pub trait FlowReferenceUpdater: Send + Sync { fn rewrite_request_paths(&self, collection: &str, from: &str, to: &str) -> FlowRewriteReport; fn flows_using(&self, collection: &str, path: &str) -> DomainResult<Vec<String>>; }`
  - `#[derive(Debug, Default, Clone, PartialEq)] pub struct FlowRewriteReport { pub rewritten: Vec<String>, pub failed: Vec<FlowRewriteFailure> }`
  - `impl FlowReferenceUpdater for FlowService`, both re-exported from `rocket_app`.
  - `#[cfg(test)] pub(crate) mod test_support` with `ScriptedFlowRepo` (an in-memory `FlowRepository`) and `flow_with(name, paths)`, reused by Plan 2.

- [ ] **Step 1: Read** `flow_service.rs:1-60,90-135`, `flow.rs:59-80` (`FlowRepository`) and `crates/rocket-collection` `Collection::validate_name`.
- [ ] **Step 2: Write the failing tests.** In `test_support`: `ScriptedFlowRepo` holds `flows: Mutex<Vec<(String, Flow)>>` in insertion order plus `fail_get`, `fail_save: Mutex<Vec<String>>` (flow names), `fail_list: Mutex<bool>` and `saved: Mutex<Vec<String>>` (names in save order). Constructor `ScriptedFlowRepo::seeded(collection: &str, flows: Vec<Flow>) -> Arc<Self>`. A failure returns `DomainError::Internal("boom".into())`. `flow_with(name, paths)` builds a flow like Task 1's helper. Tests, all on collection `"c"` through `FlowService::new(Box::new(...))`:
  - `rewrite_request_paths_rewrites_and_saves_matching_flows`: flows `A` (`auth/login.yml`) and `B` (`x.yml`), `rewrite_request_paths("c","auth","identity")` gives `rewritten == ["A"]`, empty `failed`, and `A` now holds `identity/login.yml`.
  - `rewrite_request_paths_skips_a_flow_that_does_not_match`: `saved == ["A"]`.
  - `rewrite_request_paths_reports_a_failed_save_and_continues`: `fail_save = ["A"]`, flows `A` and `C` both match, `rewritten == ["C"]` and `failed == [FlowRewriteFailure { flow: "A".into(), message }]` where `message.contains("boom")`.
  - `rewrite_request_paths_does_not_write_a_flow_it_could_not_read`: `fail_get = ["A"]`, `A` is in `failed`, `saved` does not contain `A`.
  - `rewrite_request_paths_reports_a_list_error`: `fail_list` set, `rewritten` is empty and `failed` has one entry with `flow == "(all flows)"`.
  - `rewrite_request_paths_saves_a_flow_that_would_fail_validation`: a flow with a Saved node plus an edge to node `ghost` (build the edge as `edge()` in `graph.rs:232` does) is rewritten and saved.
  - `flows_using_lists_each_flow_once_sorted`: `B` with one match and `A` with two Saved nodes on the folder gives `["A","B"]`. A list error returns `Err`.
  - `flows_using_rejects_bad_arguments`: for `("c","")`, `("c","/abs.yml")`, `("c","../x.yml")` and `("../c","a.yml")` the result is `Err(DomainError::InvalidInput(_))` and `ScriptedFlowRepo` was never listed (count the `list` calls).
- [ ] **Step 3: Run** `cargo test -j4 -p rocket-app flow_references`. Expected: compile error, `cannot find trait 'FlowReferenceUpdater'`.
- [ ] **Step 4: Implement.** `rewrite_request_paths` lists, gets each flow, calls `rewrite_saved_paths`, and calls the repo `save` directly when the count is above zero. An error goes to `failed` with `err.to_string()` and the loop continues. A list error returns one `failed` entry named `(all flows)`. `flows_using` first validates (`Collection::validate_name`, non-empty path, no leading `/`, no `..` component) and returns `InvalidInput` otherwise, then filters with `uses_saved_path`, sorts and dedupes. `FlowService` gets a private in-crate accessor for its repo. Add `mod flow_references;` and `pub use flow_references::{FlowReferenceUpdater, FlowRewriteReport};` to `lib.rs`.
- [ ] **Step 5: Run** `cargo test -j4 -p rocket-app flow_references`. Expected: 8 passed. Then `cargo check -j4 -p rocket-app`, clean, and `cargo test -j4 -p rocket-app flow_service`, all existing tests still pass.

Next plan: docs/superpowers/plans/2026-10-18-flow-request-ref-rewrite/plan-02-flow-request-ref-rewrite.md
