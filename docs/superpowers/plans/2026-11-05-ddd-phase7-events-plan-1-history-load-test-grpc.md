# DomainEvent split, Plan 1: HistoryEvent, LoadTestEvent, GrpcEvent (D-74, D-71 part 1)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `HistoryCleared`, the two load test events and the four gRPC session events leave `DomainEvent` and live in `rocket-history`, `rocket-http` and `rocket-grpc`; the load test and gRPC payload types leave `rocket-shared` with them. The frontend sees the same channels and JSON.

**Architecture:** Each context gets one enum implementing `PublishedEvent` (plan 0): `HistoryEvent` (`rocket-history/src/events.rs`), `LoadTestEvent` (`rocket-http/src/load_test_events.rs`, with the payload types moved from `rocket-shared/src/load_test.rs`), `GrpcEvent` (`rocket-grpc/src/events.rs`, with `GrpcMetadataPair` moved from `rocket-shared/src/grpc.rs`). The seven variants and their arms are removed from `DomainEvent`, its channel match, the golden `CHANNELS` table and `one_of_each()` (81 becomes 80, 78, then 74). Publishers (`HistoryService`, the load test `EventProgress` adapter, `GrpcService`) publish the new enums. Test recorders that would silently drop a foreign event now fail loudly (plan 0 review, deferred minor).

**Tech Stack:** Rust: `rocket-shared`, `rocket-history`, `rocket-http`, `rocket-grpc`, `rocket-infra`, `rocket-app`, `src-tauri` (package `rocket`).

**Spec:** `docs/superpowers/specs/2026-11-05-domain-event-split-design.md` (sections 2, 3 and step 1 of section 5). Series index: `docs/superpowers/plans/2026-11-05-ddd-phase7-events-index.md`.

**Workspace:** git worktree `../rocket-events-p1` on branch `refactor/ddd-phase7-events-plan-1`, cut from local `main`. Delete the worktree after the merge (user ruling). Do not touch other worktrees.

## Global Constraints

- The wire format is byte-identical. Tags and channels: `historyCleared` on `history-changed`; `loadTestProgress` on `load_test_progress`; `loadTestComplete` on `load_test_complete`; `grpcSessionStarted`, `grpcSessionHeaders`, `grpcSessionMessage`, `grpcSessionFinished` on `grpc-session-started`, `grpc-session-headers`, `grpc-session-message`, `grpc-session-finished`.
- Each new enum keeps `#[serde(tag = "type", rename_all = "camelCase")]`, the same variant names, field names and field attributes as today; payload structs keep their serde attributes, including `#[serde(default)]` on the `LoadTestResult` lists.
- Ruling: spec section 2 asks for an old-versus-new JSON comparison against `DomainEvent`, but the variants leave `DomainEvent` in this plan. Each enum instead gets a test against a frozen JSON literal captured from the old enum before the variants are removed (step 1 of each task).
- `also_emit_on` and `written_collections` stay at their defaults for these enums.
- Production code has no `unwrap`; test code may use `expect`. `cargo` takes `-j4`; never `--workspace`. `rustfmt --edition 2021` only on leaf files, never on `lib.rs` or `mod.rs`. Commits use a pathspec and only when the whole tree compiles.

## Review Focus

- A tag or channel typo on one of the 7 moved events silences a frontend listener: frozen literal and channel tests (Tasks 1 to 3).
- Load test progress keeps its camelCase payload keys (`elapsedMs`, `requestsPerSecond`, `recentLog` and so on): Task 2.
- A saved load test result without the `#[serde(default)]` lists (`phaseTimeline`, `requestLog`, `timeSeries`) still deserializes: Task 2.
- A test recorder that receives an event type it does not know must fail with the channel in the message, not drop the event: Task 1.
- The bus still emits the moved events on the same channels through `emit_channels`, and `DomainEvent` has exactly 74 variants with 74 golden channel entries: Tasks 1 to 3 bus and shared tests.

---

### Task 1: Loud recorders and HistoryEvent

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-history/src/events.rs`
- Modify: `crates/rocket-history/src/lib.rs` (module and re-export `HistoryEvent`), `crates/rocket-shared/src/events.rs` (remove the `HistoryCleared` variant and its contract-test uses), `crates/rocket-shared/src/domain_event_port.rs` (arm, `CHANNELS` entry, `one_of_each()` entry; count 81 to 80), `crates/rocket-app/src/history_service.rs`, `crates/rocket-app/src/test_doubles.rs`, `src-tauri/src/tauri_event_bus.rs` (tests that name `HistoryCleared`)
- Modify (recorders): every `else { return; }` after `downcast_ref::<DomainEvent>()` (find with `grep -rn "downcast_ref::<DomainEvent>" crates src-tauri --include='*.rs'`; about 19 sites in `rocket-infra`, `rocket-app`)

**Interfaces:**
- Produces: `pub enum HistoryEvent { HistoryCleared }` implementing `PublishedEvent` (`channel()` is `"history-changed"`), re-exported as `rocket_history::HistoryEvent`.
- Produces: `RecordingPublisher::others(&self) -> Vec<(String, serde_json::Value)>` in `rocket-app/src/test_doubles.rs`: channel and `to_json()` of every published event that is not a `DomainEvent`; `events()` still returns only `DomainEvent`s.
- Consumes: `PublishedEvent` and `dyn PublishedEvent::downcast_ref` from `rocket-shared`.

- [ ] **Step 1: Write the failing tests.**
  - `rocket-history/src/events.rs`: `history_cleared_keeps_its_channel_and_json` (`channel() == "history-changed"`, `to_json() == json!({"type": "historyCleared"})`) and `history_cleared_has_no_extra_channels_or_written_collections`.
  - `rocket-app/src/test_doubles.rs`: `recording_publisher_keeps_events_of_other_types` (publish `&HistoryEvent::HistoryCleared`: `events()` is empty and `others()` is `[("history-changed", json!({"type":"historyCleared"}))]`).
  - One representative recorder, `env_audit.rs` `RecordingPublisher`: `foreign_event_panics_with_its_channel` (`#[should_panic(expected = "history-changed")]`, publishes `&HistoryEvent::HistoryCleared`).
  - Update `history_service.rs` `clear_publishes_history_cleared_once` to assert through `others()`.
  - The frozen literal is `{"type":"historyCleared"}`; confirm it against the current contract test in `rocket-shared/src/events.rs` before removing the variant.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-history events` — Expected: compile error (red).
- [ ] **Step 3: Implement.** Add `HistoryEvent`; move the `HistoryCleared` arm out of `domain_event_port.rs` and drop the variant from `DomainEvent`; `HistoryService::clear` publishes `&HistoryEvent::HistoryCleared`; `RecordingPublisher` stores foreign events for `others()`; every other recorder replaces the silent return with `panic!("recorder got a non-DomainEvent on channel {}", event.channel())`. Update the bus tests that built `DomainEvent::HistoryCleared`.
- [ ] **Step 4: Run** `cargo check -j4 --all-targets`, then `cargo test -j4 -p rocket-history`, `cargo test -j4 -p rocket-shared`, `cargo test -j4 -p rocket-app`, `cargo test -j4 -p rocket-infra file_watcher` and `cargo test -j4 -p rocket tauri_event_bus` — Expected: pass; `one_of_each_covers_every_variant` now counts 80.
- [ ] **Step 5: Commit** in groups: `feat(history): add the HistoryEvent enum`, `refactor(shared): move HistoryCleared out of DomainEvent`, `test: fail loudly on events a recorder does not know`.

### Task 2: LoadTestEvent and the load test payload types

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Move: `crates/rocket-shared/src/load_test.rs` to `crates/rocket-http/src/load_test_events.rs` (`git mv`; contents unchanged apart from imports), then add `LoadTestEvent` to it
- Modify: `crates/rocket-shared/src/lib.rs` (remove `pub mod load_test;`), `crates/rocket-shared/src/events.rs` (remove the two variants, their import, their contract tests which move to the new file), `crates/rocket-shared/src/domain_event_port.rs` (arms, `CHANNELS` entries, `one_of_each()` entries; 80 to 78), `crates/rocket-http/src/lib.rs` (module and re-exports of `LoadTestEvent`, `LoadTestProgressEvent`, `LoadTestResult`, `TimeSeriesPoint`, `RequestLogEntry`, `PhaseMarker`), `crates/rocket-http/src/load_test.rs`, `crates/rocket-app/src/load_test_service.rs` (adapter publishes `LoadTestEvent`, tests use `others()`), `src-tauri/src/tauri_event_bus.rs`, and every file that names `rocket_shared::load_test` (grep)

**Interfaces:**
- Produces: `pub enum LoadTestEvent { LoadTestProgress(LoadTestProgressEvent), LoadTestComplete(LoadTestResult) }` implementing `PublishedEvent` (`channel()` is `"load_test_progress"` and `"load_test_complete"`). The payload types keep their names and derives and are re-exported from `rocket_http`.
- Consumes: `RecordingPublisher::others()` from Task 1.

- [ ] **Step 1: Write the failing tests and capture the literals.** Before removing the variants, serialize the existing contract-test fixtures (`events.rs`, around `LoadTestProgress` and `LoadTestComplete`) with the old `DomainEvent` once and paste the output as `serde_json::json!` literals. In `load_test_events.rs`: `progress_event_json_is_unchanged` and `complete_event_json_is_unchanged` (`to_json()` equals the literals), `load_test_events_keep_their_channels` (`load_test_progress`, `load_test_complete`), `progress_payload_keys_stay_camel_case` (keys `elapsedMs`, `requestsPerSecond`, `recentLog` present), `result_without_the_default_lists_still_deserializes` (JSON without `phaseTimeline`, `requestLog`, `timeSeries` gives empty vectors). Update `event_progress_publishes_the_matching_domain_events` in `load_test_service.rs` (rename to `..._load_test_events`) to compare `others()` channels and payload values (`elapsedMs == 250`, `totalRequests == 1`). Add a bus test that `emit_channels(&LoadTestEvent::...)` is the single expected channel.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-http load_test_events` — Expected: compile error (red).
- [ ] **Step 3: Implement.** Move the file, add the enum, fix imports, drop the variants and arms from `rocket-shared`, switch the adapter and the bus tests.
- [ ] **Step 4: Run** `cargo check -j4 --all-targets`, `cargo test -j4 -p rocket-http`, `cargo test -j4 -p rocket-shared`, `cargo test -j4 -p rocket-app load_test` and `cargo test -j4 -p rocket tauri_event_bus` — Expected: pass; the shared count is 78. `grep -rn "rocket_shared::load_test\|shared::load_test" crates src-tauri --include='*.rs'` prints nothing.
- [ ] **Step 5: Commit** in groups: `refactor(http): move the load test payload types from shared`, `feat(http): add the LoadTestEvent enum`, `refactor: publish load test progress as LoadTestEvent`.

### Task 3: GrpcEvent, the gRPC metadata type, docs and checks

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Move: `crates/rocket-shared/src/grpc.rs` to `crates/rocket-grpc/src/events.rs` (`git mv`), then add `GrpcEvent` with the four session variants and the same fields as today (`session_id`, `method_type`; `headers: Vec<GrpcMetadataPair>`; `index`, `json`; `code`, `code_name`, `message`, `trailers`, `duration_ms`)
- Modify: `crates/rocket-shared/src/lib.rs`, `events.rs`, `domain_event_port.rs` (78 to 74), `crates/rocket-grpc/src/lib.rs` and `call.rs`, `crates/rocket-infra/src/grpc/channel.rs` and `executor.rs`, `crates/rocket-app/src/grpc_service.rs` (publishes `GrpcEvent`; its `Recorder` stores `GrpcEvent`), `src-tauri/src/tauri_event_bus.rs`
- Modify (docs): `crates/rocket-shared/CLAUDE.md`, `crates/rocket-grpc/CLAUDE.md`, `crates/rocket-http/CLAUDE.md`, `crates/rocket-history/CLAUDE.md` if it exists, the spec section 2 (frozen literals instead of old-versus-new), `docs/roadmaps/ddd-roadmap.md` (D-71 progress: `grpc.rs` and `load_test.rs` moved, `acp.rs` remains; D-74 progress), the index (plan 1 done, unmerged)

**Interfaces:**
- Produces: `pub enum GrpcEvent { GrpcSessionStarted{..}, GrpcSessionHeaders{..}, GrpcSessionMessage{..}, GrpcSessionFinished{..} }` implementing `PublishedEvent`; `GrpcMetadataPair` re-exported from `rocket_grpc`.

- [ ] **Step 1: Write the failing tests and capture the literals** as in Task 2, from the current gRPC contract tests (one value per variant, plus `GrpcMetadataPair`). In `events.rs` of `rocket-grpc`: `grpc_events_json_are_unchanged` (four literals), `grpc_events_keep_their_channels` (the four channel names from Global Constraints), `metadata_pair_keeps_its_two_fields`. Update the `grpc_service.rs` recorder tests so `tags()` matches `GrpcEvent`. Add a bus test for `emit_channels(&GrpcEvent::...)`.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-grpc events` — Expected: compile error (red).
- [ ] **Step 3: Implement.** Move the file, add the enum, remove the variants and arms from `rocket-shared`, switch users to `rocket_grpc::GrpcMetadataPair`.
- [ ] **Step 4: Run** `cargo check -j4 --all-targets`, `cargo test -j4 -p rocket-grpc`, `cargo test -j4 -p rocket-shared`, `cargo test -j4 -p rocket-infra grpc`, `cargo test -j4 -p rocket-app grpc`, `cargo test -j4 -p rocket tauri_event_bus` — Expected: pass; shared counts 74. Then `cargo clippy -j4 -p rocket-shared -p rocket-history -p rocket-http -p rocket-grpc -p rocket-infra -p rocket-app --all-targets` and `cargo clippy -j4 -p rocket --all-targets` — Expected: no new warnings (the existing "very complex type" warning is allowed). `grep -rn "shared::grpc\|rocket_shared::load_test" crates src-tauri docs --include='*.rs'` prints nothing.
- [ ] **Step 5: Commit** in groups: `refactor(grpc): move GrpcMetadataPair from shared`, `feat(grpc): add the GrpcEvent enum`, `refactor: publish gRPC session events as GrpcEvent`, `docs: record DomainEvent split plan 1 (D-74, D-71 part)`.
