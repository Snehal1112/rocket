# DomainEvent split, Plan 5: OpenApiSyncEvent and ExecutionEvent (D-74)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The 2 OpenAPI sync events and the 12 execution events leave `DomainEvent` and live in `OpenApiSyncEvent` and `ExecutionEvent`, both in `rocket-app`. The frontend sees the same channels and JSON. After this plan `DomainEvent` holds exactly the 17 collection events that plan 6 moves.

**Architecture:** Same pattern as plans 1 to 4: one enum per context implementing `PublishedEvent`, in a new file of the context crate (here `rocket-app`, because no domain crate owns these contexts, per the spec's ownership table). Variants, channel arms, golden `CHANNELS` entries and `one_of_each()` entries leave `DomainEvent` (31 becomes 29, then 17), publishers switch, and recorders that now see a new type store it or use `others()`. The WebSocket and GraphQL payload enums (`WebSocketDirection`, `WebSocketPayloadKind`, `GraphQlSubscriptionEventKind`, `WebSocketSessionState`) stay in `rocket-shared/src/events.rs` for now; plan 7 relocates the helper DTOs that remain when `DomainEvent` is deleted.

**Tech Stack:** Rust: `rocket-shared`, `rocket-app`, `src-tauri` (package `rocket`).

**Spec:** `docs/superpowers/specs/2026-11-05-domain-event-split-design.md` (sections 2, 3, 4 and step 5 of section 5). Series index: `docs/superpowers/plans/2026-11-05-ddd-phase7-events-index.md`. Model: `crates/rocket-acp/src/events.rs`, `crates/rocket-flow/src/events.rs`, `crates/rocket-git/src/events.rs`.

**Workspace:** git worktree `../rocket-events-p5` on branch `refactor/ddd-phase7-events-plan-5`, cut from local `main`. Delete the worktree after the merge. Do not touch other worktrees or the main checkout.

## Global Constraints

- The wire format is byte-identical. Tags and channels:

| Enum | Tag | Channel |
|---|---|---|
| `OpenApiSyncEvent` | `openApiSyncChecked` | `openapi-sync-checked` |
| `OpenApiSyncEvent` | `openApiSyncApplied` | `openapi-sync-applied` |
| `ExecutionEvent` | `requestExecuted` | `request-executed` |
| `ExecutionEvent` | `runnerStarted` | `runner-started` |
| `ExecutionEvent` | `runnerStepCompleted` | `runner-step-completed` |
| `ExecutionEvent` | `runnerFinished` | `runner-finished` |
| `ExecutionEvent` | `webSocketMessage` | `ws:message` |
| `ExecutionEvent` | `webSocketStatus` | `ws:status` |
| `ExecutionEvent` | `graphQlSubscriptionMessage` | `graphql:subscription-message` |
| `ExecutionEvent` | `graphQlSubscriptionStatus` | `graphql:subscription-status` |
| `ExecutionEvent` | `consoleOutput` | `script-console` |
| `ExecutionEvent` | `testsCompleted` | `script-tests` |
| `ExecutionEvent` | `scriptError` | `script-error` |
| `ExecutionEvent` | `scriptVariableWritten` | `script-variable-written` |

  Confirm every tag against the old `DomainEvent` serialization before removing anything (the camelCase of `WebSocketMessage` and `GraphQlSubscriptionMessage` is the easy place to slip).
- Each enum keeps `#[serde(tag = "type", rename_all = "camelCase")]`, the same variant names, snake_case field names, types, field attributes **and field declaration order** as today. Order matters: existing tests (for example `runner_started_wire_shape`) compare `serde_json::to_string` output as an exact string, and the frontend sees the same key order.
- `also_emit_on` and `written_collections` stay at their defaults for both enums. In particular `ScriptVariableWritten` does not feed `RecentWrites` today and must not start to.
- Ruling carried from plan 1: frozen JSON literals captured from the old `DomainEvent` before the variants are removed, one per variant. Variants with optional fields get a populated and a `None` form (`RunnerStarted.folder_path`, `RunnerStepCompleted.status_code`, `script_error`, `error`, `ScriptVariableWritten.environment` and `collection`, and any `Option` in the WebSocket, GraphQL, console and test variants).
- The new enums derive `Debug, Clone, Serialize, PartialEq` (add `Eq` only if every payload type allows it). `Deserialize` only under `cfg_attr(test, ...)` and only if a moved back-compat test reads events back.
- No new crate dependency edge: `rocket-app` already depends on `rocket-shared` and `serde_json`; `src-tauri` already depends on `rocket-app`. `rocket-infra` must not depend on `rocket-app`, and the 12 execution variants are not published from `rocket-infra` (confirm with `rg "DomainEvent::" crates/rocket-infra -g '*.rs'`, which should list only the file watcher).
- `rocket-app/src/lib.rs` gets `pub mod execution_events;` and `pub mod openapi_sync_events;` in alphabetical position, by hand. Never run rustfmt on `lib.rs` or `mod.rs`.
- Production code has no `unwrap`; test code may use `expect`. `cargo` takes `-j4`; never `--workspace`; long runs go to a log file or the background. Run `cargo fmt --check -p rocket-shared -p rocket-app -p rocket` before every commit in this plan (a plan 4 slip: fmt was only checked on one crate). Commits use a pathspec and only when the whole tree compiles. Use `rg`, `sed` or `perl` for bulk edits, for example `perl -pi -e 's/DomainEvent::(RunnerStarted|RunnerStepCompleted|RunnerFinished)/ExecutionEvent::$1/g'` per file, then fix imports by hand.
- Test recorders (every `downcast_ref::<DomainEvent>` site, including fully qualified paths and the one in `src-tauri/src/commands/openapi_sync.rs`) panic on events they do not know. A recorder that must now also see one of the new types downcasts it too (or uses `RecordingPublisher::others()`, or a typed accessor next to `flow_events()`, `git_events()` and `acp_events()` in `crates/rocket-app/src/test_doubles.rs`) and still panics on anything else. Runner and WebSocket/GraphQL events publish from spawned tasks, where a panic is swallowed, so tests assert on stored content.
- `RecordingPublisher` does not panic: an event it has no typed arm for goes silently to `others()`. So a test that reads `publisher.events()` for a moved variant stops seeing it, and an absence check such as `events().is_empty()` passes for the wrong reason. Every test that asserts on execution events reads `execution_events()`. Check that no `events().is_empty()` or `events().len()` assertion sits in a test that runs a request, runner, WebSocket or GraphQL path (today they are in `collection_trust.rs`, `collection_service_examples_tests.rs`, `workspace_watcher.rs`, `history_service.rs`, `proposal_service.rs` and `test_doubles.rs`, none of which publish execution events; re-check with `rg`).
- The `websocket_service.rs` doc comments (lines 3 and 264) say events leave "as `DomainEvent`s"; update them to `ExecutionEvent`.

## Review Focus

- A typo in one of the 14 channels silences a frontend listener, and the colon names (`ws:message`, `ws:status`, `graphql:subscription-*`) are not derived from the tags: channel test plus frozen literals, Tasks 1 and 2.
- Field declaration order and the exact-string wire tests (`runner_started_wire_shape` and its siblings) must keep passing unchanged in meaning: Task 2.
- Runner, WebSocket and GraphQL events publish from spawned tasks: the recorders in `collection_runner_service.rs`, `websocket_service.rs` and `graphql_subscription.rs` must store the new type and tests must assert on stored content, not only absence of a panic: Task 2.
- `ScriptVariableWritten` and the other script events must not enter `written_collections` (they do not today): Task 2 test.
- After this plan `DomainEvent` has exactly the 17 collection variants (`FileChanged`, `FileWatcherFailed`, `CollectionCreated`, `CollectionDeleted`, `CollectionRenamed`, `RequestSaved`, `RequestDeleted`, `ItemMoved`, `FolderCreated`, `FolderDeleted`, `ItemsReordered`, `CollectionSettingsSaved`, `CollectionTrustChanged`, `FolderVariablesSaved`, `RequestVariablesSaved`, `FolderSettingsSaved`, `CollectionVariableWritten`); anything else left over is a missed variant: Task 3 check.

---

### Task 1: OpenApiSyncEvent

**Files:**
- Create: `crates/rocket-app/src/openapi_sync_events.rs`
- Modify: `crates/rocket-app/src/lib.rs` (add `pub mod openapi_sync_events;`), `crates/rocket-shared/src/events.rs` (remove the two variants and their contract-test uses), `crates/rocket-shared/src/domain_event_port.rs` (two arms, two `CHANNELS` entries, two `one_of_each()` entries; 31 to 29), `crates/rocket-app/src/openapi_sync_scheduler.rs`, `crates/rocket-app/src/openapi_sync_scheduler_tests.rs`, `crates/rocket-app/src/openapi_sync_lock_tests.rs` (a recorder), `src-tauri/src/commands/openapi_sync.rs` (publishes `OpenApiSyncApplied`, has a recorder), `src-tauri/src/tauri_event_bus.rs` (tests). Find every user with `rg "OpenApiSync(Checked|Applied)" crates src-tauri -g '*.rs'`.

**Interfaces:**
- Produces: `pub enum OpenApiSyncEvent { OpenApiSyncChecked { collection: String, status: String }, OpenApiSyncApplied { collection: String, applied: usize, stale: usize } }` implementing `PublishedEvent` (channels in the table), at `rocket_app::openapi_sync_events::OpenApiSyncEvent`.
- Consumes: `PublishedEvent` (path as in `crates/rocket-flow/src/events.rs`), `RecordingPublisher::others()`.

- [ ] **Step 1: Write the failing tests and capture the literals.** Before removing anything, serialize both variants with the old `DomainEvent` and paste `json!` literals. In `openapi_sync_events.rs`: `openapi_sync_events_json_are_unchanged`, `openapi_sync_events_keep_their_channels`, `openapi_sync_events_have_no_extra_channels_or_written_collections`. Bus test in `tauri_event_bus.rs`: `emit_channels(&OpenApiSyncEvent::...)` is the single expected channel for each variant. Update the scheduler and lock recorders and the `src-tauri` recorder so they read `OpenApiSyncEvent` values with unchanged expected content.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-app openapi_sync_events` — Expected: compile error (red).
- [ ] **Step 3: Implement.** Add the enum; remove the variants, arms and golden entries from `rocket-shared`; switch the publishers; fix every recorder the compiler or a test run flags, following the recorder rule in Global Constraints.
- [ ] **Step 4: Run** `cargo check -j4 --all-targets`, `cargo test -j4 -p rocket-shared`, `cargo test -j4 -p rocket-app openapi_sync`, `cargo test -j4 -p rocket --lib openapi_sync` and `cargo test -j4 -p rocket --lib tauri_event_bus` — Expected: pass; `one_of_each_covers_every_variant` counts 29. `rg "DomainEvent::OpenApiSync" crates src-tauri -g '*.rs'` prints nothing. `cargo fmt --check -p rocket-shared -p rocket-app -p rocket` is clean.
- [ ] **Step 5: Commit** with a pathspec (one commit if the tree cannot compile in groups).

### Task 2: ExecutionEvent

**Files:**
- Create: `crates/rocket-app/src/execution_events.rs`
- Modify: `crates/rocket-app/src/lib.rs` (add `pub mod execution_events;`), `crates/rocket-shared/src/events.rs` (remove the 12 variants and their contract tests, which move; the four WebSocket/GraphQL payload enums stay), `crates/rocket-shared/src/domain_event_port.rs` (12 arms, `CHANNELS` entries, `one_of_each()` entries; 29 to 17), `crates/rocket-app/src/execution_service.rs` (about 21 uses, two recorders), `crates/rocket-app/src/execution_service/script_chain.rs`, `crates/rocket-app/src/execution_service/script_host_tests.rs`, `crates/rocket-app/src/collection_runner_service.rs` (about 16 uses, two recorders), `crates/rocket-app/src/websocket_service.rs`, `crates/rocket-app/src/graphql_subscription.rs` (one recorder), `crates/rocket-app/src/flow_execution_service.rs` (one use), `crates/rocket-app/src/test_doubles.rs` (required: an `execution_events` field and `execution_events()` accessor next to `acp_events()`, a downcast arm in `RecordingPublisher::publish`, and the `others()` doc comment updated; `websocket_service.rs`, `graphql_subscription.rs`, `collection_runner_service.rs`, `script_chain.rs`, `script_host_tests.rs` and `flow_execution_service.rs` read events back through `publisher.events()` today and switch to `execution_events()`), `src-tauri/src/tauri_event_bus.rs` (tests). Find every user with `rg "DomainEvent::(RequestExecuted|Runner|WebSocket|GraphQl|ConsoleOutput|TestsCompleted|ScriptError|ScriptVariableWritten)" crates src-tauri -g '*.rs'`.

**Interfaces:**
- Produces: `pub enum ExecutionEvent` with exactly the 12 variants and field lists (types, attributes and declaration order) of the `DomainEvent` variants today, implementing `PublishedEvent` (channels in the table), at `rocket_app::execution_events::ExecutionEvent`. It imports `WebSocketDirection`, `WebSocketPayloadKind`, `GraphQlSubscriptionEventKind` and `WebSocketSessionState` from `rocket_shared::events`.
- Consumes: `PublishedEvent`, `OpenApiSyncEvent` is unrelated; `RecordingPublisher::others()`.

- [ ] **Step 1: Write the failing tests and capture the literals.** Before removal, serialize all 12 variants with the old `DomainEvent` (populated and `None` forms) and paste `json!` literals. In `execution_events.rs`: `execution_events_json_are_unchanged` (all variants), `execution_events_keep_their_channels` (the 12 names from the table), `execution_events_have_no_extra_channels_or_written_collections` (includes `ScriptVariableWritten`). Move the existing contract tests from `rocket-shared/src/events.rs` (the `runner_*_wire_shape`, `script_variable_written_serializes`, WebSocket, GraphQL, console and tests ones) into the new file with `ExecutionEvent`, keeping every assertion and exact string. Bus test: `emit_channels(&ExecutionEvent::...)` returns the single table channel for a runner event, a WebSocket event and a script event. Update the recorders in `execution_service.rs`, `collection_runner_service.rs` and `graphql_subscription.rs`, and the WebSocket and runner tests, to read `ExecutionEvent` values with unchanged expected strings and counts.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-app execution_events` — Expected: compile error (red).
- [ ] **Step 3: Implement.** Add the enum; remove the variants, arms and golden entries from `rocket-shared`; switch the publishers to `&ExecutionEvent::...`; fix every recorder the compiler or a test run flags, following the recorder rule in Global Constraints.
- [ ] **Step 4: Run** `cargo check -j4 --all-targets`, `cargo test -j4 -p rocket-shared`, `cargo test -j4 -p rocket-app`, `cargo test -j4 -p rocket --lib tauri_event_bus` and `cargo test -j4 -p rocket-infra file_watcher` — Expected: pass; `one_of_each_covers_every_variant` counts 17. `rg "DomainEvent::(RequestExecuted|Runner|WebSocket|GraphQl|ConsoleOutput|TestsCompleted|ScriptError|ScriptVariableWritten)" crates src-tauri -g '*.rs'` prints nothing. `cargo fmt --check -p rocket-shared -p rocket-app -p rocket` is clean.
- [ ] **Step 5: Commit** with a pathspec (one commit if the tree cannot compile in groups).

### Task 3: Docs and checks

**Files:**
- Modify: `crates/rocket-shared/CLAUDE.md` (the `DomainEvent` description and moved-out list: 17 variants left, all collection events), `crates/rocket-app/CLAUDE.md` (the two new modules; any claim that execution, runner, WebSocket, GraphQL or OpenAPI sync services publish `DomainEvent` becomes the new type; verify each with `rg` first), `docs/roadmaps/ddd-roadmap.md` (D-74 progress: plan 4 is merged, plan 5 done and unmerged, 17 variants left, plans 6 and 7 follow; D-74 stays open), `docs/superpowers/plans/2026-11-05-ddd-phase7-events-index.md` (plan 4 "done, merged", plan 5 file and "done, unmerged")

- [ ] **Step 1: Edit the docs.** Run `rg "^    [A-Z]" crates/rocket-shared/src/events.rs` inside the `DomainEvent` body (or read the enum) and confirm exactly the 17 collection variants listed in Review Focus remain.
- [ ] **Step 2: Run** `cargo clippy -j4 -p rocket-shared -p rocket-app --all-targets` and `cargo clippy -j4 -p rocket --all-targets` — Expected: no new warnings (the existing "very complex type" warning is allowed). Run `cargo fmt --check -p rocket-shared -p rocket-app -p rocket` — Expected: no diff.
- [ ] **Step 3: Commit** `docs: record DomainEvent split plan 5 (D-74)`.
