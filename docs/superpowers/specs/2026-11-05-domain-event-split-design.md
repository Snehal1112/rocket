# Splitting DomainEvent into per-context events (D-74, D-71)

Status: design for review. Roadmap: `docs/roadmaps/ddd-roadmap.md` D-74 and D-71. Decision: `docs/engineering/ddd-decisions.md` D-03 (direction (b), per-context enums behind the same wire format). Related: R-55 (events and IPC rework) stays parked; this spec does not need it.

## Goal

No single global event enum in `rocket-shared`. Each context owns its events, in its own crate. The frontend sees exactly the same channels and the same JSON as today. After the last context moves, the acp, grpc and load_test types leave `rocket-shared` too (D-71).

## Non-goals

- Any new event, any renamed channel, any change to a payload field (R-55 territory).
- Frontend changes.
- Changing how events are produced (which service publishes what).

## Current state (checked 2026-11-05)

- `crates/rocket-shared/src/events.rs`: `DomainEvent`, 81 variants, tagged `type` in camelCase, snake_case fields; `EventPublisher::publish(&self, DomainEvent)`; `NullEventPublisher`; contract tests pinning the keys.
- `DomainEvent::` is used at about 546 sites in `rocket-app` (30 files), `rocket-infra` (4), `rocket-acp` (1), `src-tauri` (3).
- `src-tauri/src/tauri_event_bus.rs`: `TauriEventBus::publish` records in-app collection writes (`RecentWrites::record`), maps the variant to a channel in one `match` (`channel_for`), adds `seq` and `tsMs` (`envelope`), emits, and also fires `collection-changed` for `BranchSwitched` and `BranchMerged`.
- `DomainEvent` has fields of the types in `acp.rs`, `grpc.rs` and `load_test.rs`, which is why those cannot leave shared yet (D-71).

## Design

### 1. The port, in rocket-shared

```rust
pub trait PublishedEvent: Send + Sync + 'static {
    /// The frontend channel this event is emitted on.
    fn channel(&self) -> &'static str;
    /// The event as JSON: the `type` tag plus the fields, without `seq` and `tsMs`.
    fn to_json(&self) -> Result<serde_json::Value, serde_json::Error>;
    /// Extra channels the same payload is also emitted on. Default: none.
    fn also_emit_on(&self) -> &'static [&'static str] { &[] }
    /// Collections this event says the app just wrote (for the file watcher echo). Default: none.
    fn written_collections(&self) -> Vec<&str> { Vec::new() }
    /// The event as `Any`, so a listener can downcast it.
    fn as_any(&self) -> &dyn std::any::Any;
}

impl dyn PublishedEvent {
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> { self.as_any().downcast_ref::<T>() }
}

pub trait EventPublisher: Send + Sync {
    fn publish(&self, event: &dyn PublishedEvent);
}
```

- `EventPublisher` stays object-safe, so every `Arc<dyn EventPublisher>` and `Box<dyn EventPublisher>` keeps its type. `NullEventPublisher` stays.
- `as_any` and `downcast_ref` exist because test recorders store `DomainEvent` values and need the concrete event back from the trait object.
- A small `impl_published_event!`-style helper is not planned; each enum writes its own `channel()` match, which is the move of today's arms.
- `DomainEvent` implements `PublishedEvent` during the migration, so old and new events go through one bus.

### 2. Wire format

- Every per-context enum keeps `#[serde(tag = "type", rename_all = "camelCase")]` with the same variant names, field names and field attributes as today, so the JSON of every event is byte-identical.
- The contract tests in `events.rs` move with their variants. Amended in plan 1: instead of comparing old and new enums, each moved context pins frozen JSON literals (one per variant) captured from the old `DomainEvent` serialization before the variants were removed, plus a channel-name test. `DomainEvent` no longer needs to keep the moved variants.
- `seq` and `tsMs` are still added by the bus only.

### 3. Who owns what

An enum lives in the domain crate of its context. Contexts that exist only in `rocket-app` keep their enum in `rocket-app`. The counts add up to 81.

| Enum | Home crate | Variants |
|---|---|---|
| `CollectionEvent` | `rocket-collection` | 17: collection, request, folder and item events, trust, settings and variable saves, `CollectionVariableWritten`, `FileChanged`, `FileWatcherFailed` |
| `EnvironmentEvent` | `rocket-environment` | 2 |
| `WorkspaceEvent` | `rocket-workspace` | 8 |
| `FlowEvent` | `rocket-flow` | 6: five run events and `FlowRequestPathsRewritten` |
| `AcpEvent` | `rocket-acp` | 16: 13 session events and 3 assistant conversation events |
| `GrpcEvent` | `rocket-grpc` | 4 |
| `LoadTestEvent` | `rocket-http` | 2 |
| `GitEvent` | `rocket-git` | 11 |
| `HistoryEvent` | `rocket-history` | 1 |
| `ExecutionEvent` | `rocket-app` | 12: `RequestExecuted`, three runner events, console and script events, tests completed, WebSocket and GraphQL events |
| `OpenApiSyncEvent` | `rocket-app` | 2 |

Rulings made in this design (change them in review if wrong):
- `FileChanged` and `FileWatcherFailed` go with `CollectionEvent`, because `rocket-infra` (the watcher) may depend on `rocket-collection`, and the channel they use is `collection-changed`.
- WebSocket and GraphQL events go with `ExecutionEvent` because both are published by `rocket-app` and no domain crate owns them.
- Assistant conversation events go with `AcpEvent` (D-05: the assistant stays in `rocket-acp`).
- Payload types used by exactly one context move into that context's crate in the same plan (this is D-71: `acp.rs` into `rocket-acp`, `grpc.rs` into `rocket-grpc`, `load_test.rs` into `rocket-http`). Types used by several contexts stay in `rocket-shared`.

### 4. The bus (src-tauri)

`TauriEventBus::publish(&dyn PublishedEvent)`:
1. `written_collections()` goes to `RecentWrites` (replaces `RecentWrites::record(&DomainEvent)`).
2. `channel()` and `to_json()` plus `seq` and `tsMs` make the payload.
3. Emit on the channel, then on every `also_emit_on()` channel (replaces the `BranchSwitched`/`BranchMerged` special case; `GitEvent` returns `["collection-changed"]` for those two).
   `to_json` costs nothing extra, because the bus already built a `serde_json::Value` before.
4. `channel_for` and its giant `match` disappear; its arms move into each enum's `channel()`.

### 5. Migration

One context per plan, in this order. After every plan the app builds, the tests pass and the wire format is unchanged, because `DomainEvent` and the new trait coexist until the last plan.

0. Foundation: `PublishedEvent`, the new `EventPublisher` signature, `DomainEvent: PublishedEvent`, the bus on the new port, all call sites passing `&DomainEvent` (mechanical).
1. `HistoryEvent`, `LoadTestEvent` (and `load_test.rs` types move), `GrpcEvent` (and `grpc.rs` moves).
2. `EnvironmentEvent`, `WorkspaceEvent`.
3. `FlowEvent`, `GitEvent`.
4. `AcpEvent` (and `acp.rs` moves).
5. `OpenApiSyncEvent`, `ExecutionEvent`.
6. `CollectionEvent` (with `RecentWrites` and the file watcher).
7. Delete `DomainEvent` and `events.rs` leftovers; shrink `rocket-shared` docs.

Each plan follows the project rule of at most three tasks, so a context with many call sites may take two plans.

### 6. Testing and guards

- Wire: per-context contract tests (moved), plus the old-versus-new comparison test per moved variant until step 7.
- Channels: a test per enum that every variant has a channel and that the channel names equal today's (the table from `channel_for`).
- Bus: the existing `tauri_event_bus` tests (`envelope`, channel mapping, echo) keep passing; a new test covers `also_emit_on` for branch switch and merge.
- Echo: the `RecentWrites` tests keep passing through `written_collections()`.
- Per plan: `cargo check -j4 --all-targets`, tests of the touched crates, clippy of the touched crates, a fresh `opus` review before merge.

## Risks

- A typo in a channel name breaks one frontend listener silently. Mitigation: the per-enum channel test copies the current table and the old-versus-new test.
- Mechanical churn in about 35 files. Mitigation: scripted replacement per context, compile errors as the guide.
- A payload type that two contexts use. Mitigation: it stays in shared until a ruling says otherwise.

## Out of scope

R-55 sub-projects (missing-event audit, event-to-cache layer, composite commands, internal subscribers, activity surface), D-72 (OAuth2 model), any rename of event payload keys (snake_case fields stay).
