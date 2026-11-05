# DomainEvent split, Plan 7: move the leftover helper types and close D-74

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `rocket-shared/src/events.rs` keeps only the publisher port (`PublishedEvent` re-export, `EventPublisher`, `NullEventPublisher`). The Flow payload types move to `rocket-flow`, the WebSocket and GraphQL payload enums to `rocket-app`, and `FileChangeKind` to `rocket-collection`, each next to the event enum that carries it. The frontend sees the same JSON. D-74 closes.

**Architecture:** A pure move. Each type keeps its derives, serde attributes, field order and doc comments, and its tests move with it. Callers change only their `use` paths, guided by `cargo check`. No re-export is left in `rocket-shared`, so a missed caller fails to compile instead of silently using an old path.

**Tech Stack:** Rust: `rocket-shared`, `rocket-flow`, `rocket-collection`, `rocket-app`, `rocket-infra`, `src-tauri` (package `rocket`).

**Spec:** `docs/superpowers/specs/2026-11-05-domain-event-split-design.md` (section 3, the payload-type rule, and step 7 of section 5). Series index: `docs/superpowers/plans/2026-11-05-ddd-phase7-events-index.md`.

**Workspace:** git worktree `../rocket-events-p7` on branch `refactor/ddd-phase7-events-plan-7`, cut from local `main`. Delete the worktree after the merge.

## Global Constraints

- Where each type goes (checked on `main` at `e0cf9d3f` with `rg -l "\b<Type>\b" crates src-tauri`):

| Types | Users today | New home |
|---|---|---|
| `FlowNodeStatus`, `FlowSkipReason`, `FlowLogLevel`, `FlowLogEntry`, `FlowPartialMode`, `FlowPartialRunInfo`, `FlowDebugHeader`, `FlowDebugResponse`, `FlowDebugRequest`, `FlowStepTrace`, `FlowWireValue`, `FlowRouteEval`, `FlowRejectedCall`, `FlowWaitDetail`, `FlowCallbackInfo`, `FlowLiveProgress`, `FlowPollDetail`, `FlowRewriteFailure` (18) | `rocket-flow`, `rocket-app`, `src-tauri` | new `crates/rocket-flow/src/run_types.rs`, `pub mod run_types;` |
| `WebSocketDirection`, `WebSocketPayloadKind`, `GraphQlSubscriptionEventKind`, `WebSocketSessionState` | `rocket-app`, `src-tauri` tests | `crates/rocket-app/src/execution_events.rs` (public, next to `ExecutionEvent`) |
| `FileChangeKind` | `rocket-collection`, `rocket-infra` | `crates/rocket-collection/src/events.rs`, re-exported as `rocket_collection::FileChangeKind` |

- Ruling (corrects plan 6): `rocket-import` and `rocket-app/src/openapi_connect.rs` use a different `FileChangeKind` (`Put`/`Delete`, defined in `rocket-import`). The shared one is used only by collection code, so under the spec's single-context rule it moves to `rocket-collection`. Do not touch the `rocket-import` type.
- Ruling: the module stays `rocket_shared::events`. It holds the publisher port that about 100 `use` lines import; renaming it is churn with no design gain.
- Every moved type keeps its exact derives (including `Deserialize`, `Default`, `Copy` where present), serde attributes, field order and doc comments. JSON must not change: `FlowEvent`, `ExecutionEvent` and `CollectionEvent` frozen-literal tests, which embed these types, must pass unchanged.
- The tests in `rocket-shared/src/events.rs` move with their types: the Flow ones (`flow_node_status_wire_shapes`, `flow_partial_run_info_wire_shape`, `a_flow_poll_detail_rides_in_the_step_trace`, `flow_node_status_round_trips`, `flow_skip_reason_wire_shapes`, `a_response_that_was_not_cut_serializes_without_truncated`, `a_request_body_flag_is_omitted_when_false_and_defaults_when_missing`, `an_empty_flow_step_trace_serializes_to_an_empty_object`, `a_flow_wire_value_is_camel_case_and_omits_false_flags`, `a_flow_wire_value_carries_its_expression_hash_in_camel_case`, `a_flow_wire_value_saved_before_the_hash_existed_still_parses`, `a_flow_wait_detail_carries_the_last_rejected_call`) go to `run_types.rs`; the publisher tests stay.
- No re-export shim in `rocket-shared`. No new crate dependency edge: `rocket-flow` has `serde` and `serde_json`; `rocket-app`, `rocket-infra` and `src-tauri` already depend on `rocket-flow` and `rocket-collection`. Check with `cargo tree -p rocket-flow -e normal --depth 1` and `cargo tree -p rocket-collection -e normal --depth 1`.
- Never run rustfmt on `lib.rs` or `mod.rs`; add module lines by hand. `cargo` takes `-j4`, never `--workspace`. The fmt check `cargo fmt --check -p rocket-shared -p rocket-flow -p rocket-collection -p rocket-app -p rocket-infra -p rocket` counts as clean when its only diffs are the pre-existing ones in `rocket-infra/src/oc/mod.rs` and `rocket-infra/src/rocketvault/mod.rs`. Bulk path edits use `perl -pi -e` per file, then fix grouped `use` lines by hand.

## Review Focus

- A moved type that lost a serde attribute or derive changes nested JSON (for example `FlowWireValue`'s omitted false flags, or `FlowDebugResponse.truncated`): the moved wire tests plus the unchanged frozen literals of `FlowEvent`, `ExecutionEvent` and `CollectionEvent`, Tasks 1 and 2.
- `FlowWireValue` and other types with `Deserialize` must still read old saved data (`a_flow_wire_value_saved_before_the_hash_existed_still_parses`): Task 1.
- The wrong `FileChangeKind` swapped in (the `rocket-import` one): Task 2 leaves `rocket-import` untouched; `git diff --stat` must not list it.
- A leftover type in `rocket-shared/src/events.rs`: Task 2 ends with only the port there (checked by reading the file).
- Docs that still say these types live in `rocket-shared`: Task 3 `rg` sweep.

---

### Task 1: Flow payload types to rocket-flow

**Files:**
- Create: `crates/rocket-flow/src/run_types.rs` (the 18 Flow types and their 12 tests)
- Modify: `crates/rocket-flow/src/lib.rs` (`pub mod run_types;`, by hand, in order), `crates/rocket-flow/src/events.rs` and `routing.rs` (import from `crate::run_types`), `crates/rocket-shared/src/events.rs` (remove the 18 types and their tests), and every caller `cargo check` flags (today: `rocket-app` `flow_execution_service.rs`, `flow_trace.rs`, `flow_poll.rs`, `flow_partial.rs`, `flow_partial_run_tests.rs`, `flow_callbacks.rs`, `flow_references.rs`, `collection_service.rs`, and `src-tauri/src/commands/flow.rs`).

**Interfaces:**
- Produces: `rocket_flow::run_types::{FlowNodeStatus, ..., FlowRewriteFailure}` with unchanged definitions.

- [ ] **Step 1: Move the 12 Flow tests** into `run_types.rs` (with `use super::*;`), unchanged, and add `pub mod run_types;` with an empty-for-now file body except the tests.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-flow run_types` — Expected: compile error (red), the types are not there yet.
- [ ] **Step 3: Move** the 18 type definitions from `rocket-shared/src/events.rs` to `run_types.rs` (cut and paste, nothing edited but the module doc), remove the moved tests from `rocket-shared`, and fix every `use` path the compiler flags.
- [ ] **Step 4: Run** `cargo check -j4 --all-targets`, `cargo test -j4 -p rocket-flow`, `cargo test -j4 -p rocket-shared`, `cargo test -j4 -p rocket-app flow`, `cargo test -j4 -p rocket --lib flow` — Expected: pass, the `FlowEvent` frozen-literal tests unchanged. `rg -n "rocket_shared::events::.*Flow" crates src-tauri -g '*.rs'` prints nothing (also check grouped `use` blocks with `rg -n "Flow(NodeStatus|StepTrace|WireValue|DebugRequest|LiveProgress)" crates/rocket-shared`). fmt check clean.
- [ ] **Step 5: Commit** `refactor(flow): move the Flow payload types into rocket-flow` with a pathspec.

### Task 2: WebSocket, GraphQL and file-change types; shrink rocket-shared

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/execution_events.rs` (add the four enums above `ExecutionEvent`, drop their `rocket_shared` import), `crates/rocket-collection/src/events.rs` (add `FileChangeKind`), `crates/rocket-collection/src/lib.rs` (`pub use events::{CollectionEvent, FileChangeKind};`), `crates/rocket-shared/src/events.rs` (remove the five types and any import that becomes unused; module doc says it holds the publisher port), and every caller `cargo check` flags (today: `rocket-app` `websocket_service.rs`, `graphql_subscription.rs`; `rocket-infra` `file_watcher.rs`, `file_watcher/batch.rs`, `file_watcher/echo.rs`; `src-tauri/src/tauri_event_bus.rs` tests).

**Interfaces:**
- Produces: `rocket_app::execution_events::{WebSocketDirection, WebSocketPayloadKind, GraphQlSubscriptionEventKind, WebSocketSessionState}`, `rocket_collection::FileChangeKind`, unchanged definitions.

- [ ] **Step 1: Add a test** in `rocket-collection/src/events.rs`, `file_change_kind_is_lowercase`: `serde_json::to_string(&FileChangeKind::Create)` is `"\"create\""`, `Modify` is `"\"modify\""`, `Remove` is `"\"remove\""`.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-collection file_change_kind` — Expected: passes today through the `rocket_shared` import; it must still pass after the move, which is the point.
- [ ] **Step 3: Move** the five types, update `lib.rs`, and fix every `use` the compiler flags. `rocket-import` must not change.
- [ ] **Step 4: Run** `cargo check -j4 --all-targets`, `cargo test -j4 -p rocket-shared`, `cargo test -j4 -p rocket-collection`, `cargo test -j4 -p rocket-app`, `cargo test -j4 -p rocket-infra file_watcher`, `cargo test -j4 -p rocket --lib tauri_event_bus` — Expected: pass, frozen literals unchanged. Read `rocket-shared/src/events.rs`: only the `PublishedEvent` re-export, `EventPublisher`, `NullEventPublisher` and their tests remain. `git diff --stat` lists nothing under `crates/rocket-import`. `cargo tree` checks show no new edge. fmt check clean.
- [ ] **Step 5: Commit** `refactor: move realtime and file-change types out of rocket-shared` with a pathspec.

### Task 3: Docs and close D-74

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-shared/CLAUDE.md` (the Events section: `events.rs` holds only the publisher port; keep one short history line for the split instead of the seven "Moved out of `DomainEvent`" bullets, pointing at the index), `crates/rocket-flow/CLAUDE.md` (the `run_types` module), `crates/rocket-collection/CLAUDE.md` (`FileChangeKind` lives here now; drop "stays in `rocket-shared`"), `crates/rocket-app/CLAUDE.md` (the realtime enums live in `execution_events.rs`), `docs/roadmaps/ddd-roadmap.md` (D-74 status done with the date 2026-11-05 and a one-line summary; D-71 stays done; the progress paragraph says the series is finished), `docs/superpowers/plans/2026-11-05-ddd-phase7-events-index.md` (plan 6 "done, merged"; plan 7 file and "done, unmerged"), `docs/superpowers/specs/2026-11-05-domain-event-split-design.md` (status line: implemented by plans 0 to 7; correct the `FileChangeKind` note if the spec names its home).
- Sweep: `rg -n "rocket_shared::events::(Flow|WebSocket|GraphQl|FileChangeKind)|events\.rs.*(Flow|WebSocket|FileChangeKind)" crates src-tauri src .claude CLAUDE.md docs/features docs/engineering -g '!node_modules'` and fix every claim about current code. History in plans, reports and specs stays.

- [ ] **Step 1: Edit the docs.** Verify every claim with `rg` first.
- [ ] **Step 2: Run** `cargo clippy -j4 -p rocket-shared -p rocket-flow -p rocket-collection -p rocket-app -p rocket-infra --all-targets` and `cargo clippy -j4 -p rocket --all-targets` — Expected: no new warnings (the existing "very complex type" warning is allowed). fmt check — Expected: clean as defined above.
- [ ] **Step 3: Commit** `docs: record DomainEvent split plan 7 and close D-74`.
