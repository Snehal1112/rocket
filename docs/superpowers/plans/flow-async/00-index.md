# Flow Async — Plan Index

**Spec:** `docs/superpowers/specs/2026-09-29-flow-async-poll-callback-design.md`
**Branch / worktree:** `worktree-flow-async` at `.claude/worktrees/flow-async` (based on `d00008ff`).

Nine plans, at most three tasks each. Execute in order. Each part is shippable when its last plan is done.

| # | File | Part | Delivers |
|---|---|---|---|
| 01 | `01-p0-cancel-signal.md` | P0 | Per-run cancel signal, `NodeRunContext`, Stop during a node's wait |
| 02 | `02-p0-progress-event.md` | P0 | `FlowStepProgress` event from engine to node card |
| 03 | `03-p1-model-dto-types.md` | P1 | `RepeatUntil` model, validation, DTO, TS types |
| 04 | `04-p1-engine-loop.md` | P1 | Poll loop, deferred History, step `attempts` |
| 05 | `05-p1-frontend.md` | P1 | Card row, properties section, "Poll request" palette entry |
| 06 | `06-p2-callback-listener.md` | P2 | `CallbackListener` trait, fake, `hyper` implementation |
| 07 | `07-p2-model-and-run-start.md` | P2 | `WaitForCallback` kind, validation, DTO, endpoints at run start, `{{callback.*}}` |
| 08 | `08-p2-node-execution.md` | P2 | Waiting, `accept_when`, timeout, output |
| 09 | `09-p2-frontend.md` | P2 | TS types, node card, editor, callback host setting, palette entry |

## Rules for every task

- Read `CLAUDE.md` and `.claude/rules/00-shortcuts.md` first.
- Tasks that touch flow `.yml` persistence, `rocket-flow` models, variable resolution or Tauri flow commands start with: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.
- TDD: failing test first, watch it fail, minimal code, watch it pass.
- Cargo: always `-j4`, target one crate (`cargo test -j4 -p rocket-app <filter>`). Never run the full workspace test suite. `cargo check -j4 -p rocket --tests` also compiles `src-tauri` tests.
- Frontend: `yarn test src/components/flow`, `yarn tsc --noEmit`, `yarn check`.
- Commit every task with the `dev-workflow-skills:1-git-commit` skill (conventional commits, `feat(flow):` / `fix(flow):`). Never a freeform `git commit -m`.
- A write hook blocks any file containing the literal text of the panicking-unwrap call, even in prose. Use `?`, `expect` in tests, or rephrase.
- Flow expression behaviour must be tested with the real `DenoScriptEngine` where a script result matters. The scripted fake engine hides real-engine bugs (see commit `46cd63f8`).

## Locked interface contract

Every plan uses exactly these names and types. A plan that needs a change here updates this index in the same commit.

### P0 — `crates/rocket-app/src/flow_cancel.rs` (new)

```rust
/// Owner side of a run's cancel signal. Held in `FlowExecutionService`.
pub(crate) struct CancelHandle { /* tokio::sync::watch::Sender<bool> */ }
/// Node side. Cheap to clone.
#[derive(Clone)]
pub(crate) struct CancelSignal { /* tokio::sync::watch::Receiver<bool> */ }

pub(crate) fn cancel_pair() -> (CancelHandle, CancelSignal);

impl CancelHandle {
    pub(crate) fn cancel(&self);
}
impl CancelSignal {
    pub(crate) fn is_cancelled(&self) -> bool;
    /// Resolves when the run is cancelled. Never resolves otherwise.
    pub(crate) async fn cancelled(&mut self);
    /// Sleeps for `dur`. Returns `Err(Cancelled)` as soon as the run is cancelled.
    pub(crate) async fn sleep(&mut self, dur: std::time::Duration) -> Result<(), Cancelled>;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Cancelled;
```

`FlowExecutionService`:
- New field `cancel_handles: Arc<Mutex<HashMap<String, CancelHandle>>>`. `run` inserts the handle next to `in_flight` and removes it on every exit path (use a drop guard struct so an early `?` cannot leak it).
- `cancel(run_id)` keeps inserting into `cancelled` and also calls the handle's `cancel()`.
- `execute_node` gains a parameter `ctx: &mut NodeRunContext` (last parameter).

```rust
pub(crate) struct NodeRunContext {
    pub(crate) run_id: String,
    pub(crate) node_id: String,
    pub(crate) cancel: CancelSignal,
}
```

- After `execute_node` returns, `run` checks `ctx.cancel.is_cancelled()`. If set, it keeps the node's real step when the node returned `Ok` (a request that finished keeps its result), records an `Err` as `failed_step(node_id, "cancelled".into())`, sets `stopped_reason = "cancelled"`, publishes the step, and breaks. (Plan 01 refinement: marking a finished request as failed would hide its real response.)
- In plan 01 the `execute_node` parameter is named `_ctx` (unused). Plans 04 and 08 rename it to `ctx` when they read it.
- Nodes turn a `Cancelled` into `Err(DomainError::Internal("cancelled".into()))`. `run` never inspects the message; it relies on the signal.

### P0 — progress event

`crates/rocket-shared/src/events.rs`:

```rust
FlowStepProgress {
    run_id: String,
    node_id: String,
    attempt: Option<u32>,
    max_attempts: Option<u32>,
    message: String,
},
```

- Channel name `flow-step-progress` in `src-tauri/src/tauri_event_bus.rs`.
- `FlowExecutionService::publish_progress(&self, ctx: &NodeRunContext, attempt: Option<u32>, max_attempts: Option<u32>, message: String)`.

Frontend:

```ts
// src/lib/tauri-api.ts
export interface FlowStepProgressEvent {
  type: 'flowStepProgress';
  run_id: string;
  node_id: string;
  attempt: number | null;
  max_attempts: number | null;
  message: string;
}
export const onFlowStepProgress: (handler: (e: FlowStepProgressEvent) => void) => Promise<UnlistenFn>;

// src/types/pane-types.ts — FlowNodeDetail
progress?: string;

// src/stores/pane-store.ts
patchFlowNodeProgress: (tabId: string, nodeId: string, message: string) => void;
```

- `patchFlowNodeProgress` merges `{ progress: message }` into the node's existing `nodeDetail`. `patchFlowNodeStatus` with a detail replaces the detail, so a completed step clears `progress`.
- `FlowToolbar` gets an optional `onPatchProgress?(nodeId: string, message: string)` prop, subscribed in both the `handleRun` path and the resumed-run effect. `FlowPane` wires it to `patchFlowNodeProgress`.
- `NodeStatusCaption` gets a `progress?: string` prop. It renders `data-testid='node-progress'` with the text when `status === 'running'` and `progress` is set. Every node data type gains `progress?: string` and passes `data.progress` (plan 09's `WaitForCallbackNode` too).
- `publish_progress` carries `#[cfg_attr(not(test), allow(dead_code))]` until plan 04 calls it; plan 04 removes the attribute. The same applies to `CancelSignal::cancelled` / `sleep` in `flow_cancel.rs`.

### P1 — model (`crates/rocket-flow/src/node.rs`)

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepeatUntil {
    pub condition: String,
    pub interval_ms: u64,
    pub max_attempts: u32,
    pub timeout_ms: u64,
}
impl RepeatUntil {
    pub const DEFAULT_CONDITION: &'static str = "response.status === 200";
    pub const DEFAULT_INTERVAL_MS: u64 = 2000;
    pub const MIN_INTERVAL_MS: u64 = 100;
    pub const DEFAULT_MAX_ATTEMPTS: u32 = 30;
    pub const MAX_MAX_ATTEMPTS: u32 = 1000;
    pub const DEFAULT_TIMEOUT_MS: u64 = 60_000;
    pub const MAX_TIMEOUT_MS: u64 = 3_600_000;
}
impl Default for RepeatUntil { /* the DEFAULT_* values */ }

// FlowNodeKind::Request gains, after `debug`:
#[serde(default, skip_serializing_if = "Option::is_none")]
repeat_until: Option<RepeatUntil>,
```

`RepeatUntil` is re-exported from `rocket_flow`. Existing `Request { label, source, debug }` patterns change to use `..`.

### P1 — request execution (`crates/rocket-app/src/execution_service.rs`)

- `ExecuteRequestInput` gains `#[serde(default)] pub skip_history: bool`.
- `ExecuteRequestOutput` (derives only `Debug, Clone`, no serde) gains `pub deferred_history: Option<rocket_history::HistoryEntry>`. When `skip_history` is true, `finish_phases` builds the entry as today but stores it here instead of saving it.
- `RequestExecutionService::save_deferred_history(&self, entry: &rocket_history::HistoryEntry)` saves it (non-fatal, like today).
- This replaces the spec's `record_history` parameter (spec §6.4 is updated in plan 04).

### P1 — engine (`crates/rocket-app/src/flow_poll.rs`, new)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PollStats { pub(crate) attempts: u32, pub(crate) elapsed_ms: u64 }

impl FlowExecutionService {
    // Second impl block, in flow_poll.rs. Called from the Request arm of
    // `execute_node` after wires are resolved and applied.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn run_repeat_until(
        &self,
        exec: &RequestExecutionService,
        input: &RunFlowInput,
        request_input: ExecuteRequestInput,
        repeat: &rocket_flow::RepeatUntil,
        external_secrets: &HashMap<String, String>,
        secret_values: &HashSet<String>,
        debug_on: bool,
        logs: &mut Vec<FlowLogEntry>,
        debug: &mut Option<FlowDebugRequest>,
        ctx: &mut NodeRunContext,
    ) -> DomainResult<ExecutedNode>;
}
```

- Visibility needed by `flow_poll.rs`: `FlowExecutionService::publish_progress` and `NodeRunContext` are `pub(crate)` (P0); `flow_execution_service::to_flow_logs` becomes `pub(crate)` (plan 04).

- `ExecutedNode` gains `pub(crate) poll: Option<PollStats>`. `ExecutedNode::plain` sets `None`.
- `result_to_step`: `Ok(executed)` with `poll: Some(stats)` and a `Request` capture is `Success` whatever the status code, with `status_code`, `duration_ms: Some(stats.elapsed_ms)` and `attempts: Some(stats.attempts)`.
- `FlowStepResult` and `DomainEvent::FlowStepCompleted` gain `#[serde(default, skip_serializing_if = "Option::is_none")] attempts: Option<u32>`.
- Give-up error text: `condition not met after {n} attempts ({secs:.1}s)`.

### P1 — IPC and TS

```rust
// src-tauri/src/commands/flow.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepeatUntilDto { pub condition: String, pub interval_ms: u64, pub max_attempts: u32, pub timeout_ms: u64 }
// FlowNodeKindDto::Request gains: #[serde(default)] repeat_until: Option<RepeatUntilDto>  (→ `repeatUntil`)
```

```ts
// src/lib/tauri-api.ts
export interface RepeatUntil { condition: string; intervalMs: number; maxAttempts: number; timeoutMs: number }
// Request kind gains: repeatUntil?: RepeatUntil | null
// FlowStepResult gains: attempts?: number;  FlowStepCompletedEvent gains: attempts?: number
// src/lib/flow-repeat.ts
export const DEFAULT_REPEAT_UNTIL: RepeatUntil; // 'response.status === 200', 2000, 30, 60000
// src/types/pane-types.ts — FlowNodeDetail gains: attempts?: number
```

### P2 — listener (`crates/rocket-app/src/callback_listener.rs`, new)

```rust
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReceivedCall {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

pub struct CallbackEndpoint {
    pub url: String,
    pub calls: tokio::sync::mpsc::Receiver<ReceivedCall>,
    /// Dropping this closes the endpoint.
    pub guard: Box<dyn Send + Sync>,
}

#[async_trait::async_trait]
pub trait CallbackListener: Send + Sync {
    /// `host` is `Flow.callback_host`; `None` means auto-detect the LAN IP.
    async fn open(&self, host: Option<&str>) -> DomainResult<CallbackEndpoint>;
}

/// Default when nothing is wired. `open` returns
/// `DomainError::Internal("callback listener is not configured")`.
pub struct NoCallbackListener;
```

- Test double: `crates/rocket-app/src/test_doubles.rs` gains `FakeCallbackListener` with `new() -> Arc<Self>`, `failing(message) -> Arc<Self>`, `queue_on_open(call)` (delivered into the next endpoint the moment it opens), `sender(index) -> tokio::sync::mpsc::Sender<ReceivedCall>`, `is_closed(index) -> bool`, `opened_count() -> usize`, `hosts() -> Vec<Option<String>>` and `async wait_opened(count)`. URLs are `http://fake:1/cb/<index>`. `CallbackListener` is implemented for `Arc<FakeCallbackListener>`, so tests pass `Box::new(Arc::clone(&fake))`.
- `FlowExecutionService::with_callback_listener(self, listener: Box<dyn CallbackListener>) -> Self`. `new` keeps its signature and defaults to `NoCallbackListener`.
- Implementation (plan 06 extension): `crates/rocket-infra/src/callback_server.rs`, `pub struct HyperCallbackListener;` with `HyperCallbackListener::new()` and an inherent `async fn open(&self, host: Option<&str>) -> DomainResult<ServerEndpoint>`. `rocket-infra` does not depend on `rocket-app` (`rocket-app` dev-depends on `rocket-infra`, so the other direction would be a cycle), so it uses mirror types `ServerCall` / `ServerEndpoint`, and `src-tauri/src/callback_adapter.rs` (`HyperCallbackAdapter`) implements `rocket_app::CallbackListener` on top of it. Constants: `MAX_BODY_BYTES = 1_048_576`, `CHANNEL_CAPACITY = 100`, `TOKEN_LEN = 32`; `pub fn detect_lan_ip() -> String`. New `rocket-infra` deps: `hyper` (`server`, `http1`), `hyper-util` (`tokio`), `http-body-util`, `bytes` (all already in `Cargo.lock`). `src-tauri` gains `async-trait`. Wired in `src-tauri/src/lib.rs` with `.with_callback_listener(Box::new(callback_adapter::HyperCallbackAdapter(rocket_infra::HyperCallbackListener::new())))`.

### P2 — model

```rust
// crates/rocket-flow/src/node.rs
WaitForCallback {
    label: String,
    name: String,
    timeout_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    accept_when: Option<String>,
},
// constants on a helper: pub const CALLBACK_DEFAULT_TIMEOUT_MS: u64 = 60_000; CALLBACK_MIN_TIMEOUT_MS = 1000; CALLBACK_MAX_TIMEOUT_MS = 3_600_000
// pub const CALLBACK_VAR_PREFIX: &str = "callback.";

// crates/rocket-flow/src/flow.rs — Flow gains
#[serde(default, skip_serializing_if = "Option::is_none")]
pub callback_host: Option<String>,
```

- Validation (V10, plan 07; V9 is plan 03's repeat_until rule): name `^[A-Za-z0-9_]+$` and unique among Wait nodes, `timeout_ms` in `1000..=3_600_000`, `accept_when` not blank, only a `trigger` input (`Wait for callback nodes only have a 'trigger' input`), only a `result` exit. `kind_name` is `"Wait for callback"`.
- `ExecuteRequestInput` gains `#[serde(default)] pub flow_vars: HashMap<String, String>`. `resolve_request` extends its flattened variable map with it (last, so it wins over stored scopes) and `begin_phases` extends `VariableContext.runtime` with it before the pre-request script.
- Run start (plan 07 extension): `crates/rocket-app/src/flow_callbacks.rs` holds `pub(crate) struct RunCallbacks` with `async open_all(listener: &dyn CallbackListener, flow: &Flow) -> DomainResult<Self>`, `vars() -> &HashMap<String, String>` and `endpoint_mut(node_id) -> Option<&mut CallbackEndpoint>`. `run` calls `open_all` before registering `in_flight` and before `FlowRunStarted`; an error is `could not open callback listener: …` and ends the call with no events. `run` owns the value, so every endpoint closes on every exit path. `execute_node` gains `callbacks: &mut RunCallbacks` after the plan 01 context parameter; the Request arm sets `request_input.flow_vars = callbacks.vars().clone()` right after building the input.
- `RequestExecutionService::evaluate_flow_callback_condition(&self, collection: &str, call: &ReceivedCall, source: &str, secret_values: &HashSet<String>) -> FlowScriptOutcome`. The script sees `request` with `method`, `path`, `query` (object), `headers` (object), `body` (parsed JSON, else text). Result coerced with `!!(...)`, so `"true"`/`"false"`. The generated wrapper contains `const request = res.getBody();`, which scripted-engine tests use as their rule needle.
- Output of an accepted call: `CapturedOutput::Request` built by `pub(crate) fn callback_output(call: &ReceivedCall, duration_ms: u64) -> ExecuteRequestOutput` in `crates/rocket-app/src/flow_wait.rs` (status 200, call headers and body; `status_text` carries the received method). The waiting loop is `FlowExecutionService::wait_for_callback(...)` in the same file; `ExecutedNode::plain` becomes `pub(crate)`.
- `FlowStepResult.value` (and the completed event's `value`) of a succeeded Wait node is the received method, e.g. `"POST"`.
- Progress message: `waiting… {left}s left · {k} ignored call(s)`, once a second.
- Timeout error text: `no matching callback within {secs}s ({k} ignored)`.

### P2 — IPC and TS

```rust
// FlowNodeKindDto::WaitForCallback { label, name, timeout_ms, #[serde(default)] accept_when: Option<String> }  (camelCase fields)
// FlowDto gains #[serde(default)] callback_host: Option<String>  (→ `callbackHost`)
```

```ts
// Flow node kind union gains:
| { kind: 'WaitForCallback'; label: string; name: string; timeoutMs: number; acceptWhen?: string | null }
// Flow gains: callbackHost?: string | null

// src/lib/flow-callback.ts (plan 09)
export const DEFAULT_CALLBACK_TIMEOUT_MS = 60_000; MIN_CALLBACK_TIMEOUT_MS = 1_000; MAX_CALLBACK_TIMEOUT_MS = 3_600_000;
export function isValidCallbackName(name: string): boolean;
export function callbackVariable(name: string): string;   // `{{callback.<name>}}`
export function nextCallbackName(nodes: FlowNode[]): string; // callback, callback_2, …
// NodePalette gains an optional `nodes?: FlowNode[]` prop (FlowPane passes tab.nodes).
// FlowTab gains callbackHost?: string | null; store action setFlowCallbackHost(tabId, host: string | null).
// FlowPane.handleSave adds `callbackHost` to the payload only when it is set.
// Components: nodes/WaitForCallbackNode.tsx, properties/WaitForCallbackEditor.tsx, CallbackHostSetting.tsx.
```
