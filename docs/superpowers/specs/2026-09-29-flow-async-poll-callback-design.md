# Flow Async — Repeat Until and Wait for Callback — Design Spec

**Date:** 2026-09-29
**Status:** Approved in brainstorming — pending written-spec review
**Builds on:** `docs/superpowers/specs/2026-09-28-flow-phase2-branching-design.md` (Phase 2), whose non-goals list "loops, retries, repeat until". This spec adds waiting without loops: the graph stays acyclic and every node still runs once.
**Branch:** `worktree-flow-async`, based on `worktree-flow-phase2-branching` at `d00008ff`.

**Out of scope:**
- Loop-back wires or any cycle in the graph. A cycle stays a save-time error.
- Public callbacks from the internet. Rocket does not start or manage a tunnel (ngrok, Cloudflare Tunnel). Callers must reach this machine on its local network.
- Message brokers and streams (Kafka, AMQP, MQTT, SSE, WebSocket). There is no runtime for these protocols.
- Parallel execution of branches. Runs stay sequential.
- A timeout exit on a poll or wait. Giving up fails the node (decision 1).
- A separate Delay node. `repeat_until` with a condition covers "wait, then read".

---

## 1. Background

A Flow runs its nodes one by one in topological order, and each node runs exactly once (`crates/rocket-app/src/flow_execution_service.rs:630`). A Request node sends its request once and passes the response on. There is no way to say "call this until the job is done" or "wait until the server calls me back".

Three real cases drive this spec:

1. **Job and status.** `POST /jobs` starts work, then `GET /jobs/{id}` must be called until its status is `done`.
2. **Publish, then read.** A request publishes an event, a worker processes it later, and a read endpoint shows the result once it is processed.
3. **Callback.** A request registers a callback URL, and the server calls that URL later.

Cases 1 and 2 are covered by **Repeat until** on the Request node (part P1). Case 3 is covered by a new **Wait for callback** node (part P2). Both need a shared **foundation** (part P0).

Relevant current code (verified 2026-09-29 at `d00008ff`):

| Area | Location |
|---|---|
| Node kinds | `crates/rocket-flow/src/node.rs:17` (`FlowNodeKind`, `debug` field uses `is_false` skip pattern) |
| Validation | `crates/rocket-flow/src/validate.rs` (`accepts_trigger` :28, `kind_name` :62, `source_handle_exists` :153) |
| Run loop | `crates/rocket-app/src/flow_execution_service.rs` — `run` (:600), cancel set (:531, :569, :580), cancel check (:632), `execute_node` (:760), Request arm (:828-877), `result_to_step` (:982) |
| Fate rules | `crates/rocket-app/src/flow_routing.rs` (`decide_fate`) |
| Condition evaluation | `evaluate_flow_route_expression` (`flow_execution_service.rs:174`), `FlowCoercion::Bool` |
| Request execution | `RequestExecutionService::execute_capturing` (`crates/rocket-app/src/execution_service.rs:1485`), history save in `finish_phases` (:1357, save at :1426) |
| Variable scopes | `crates/rocket-environment/src/context.rs` (`VariableContext.runtime`) |
| Events | `crates/rocket-shared/src/events.rs:222-275` (`FlowStepStarted` :231, `FlowStepCompleted` :236) |
| Event bus | `src-tauri/src/tauri_event_bus.rs:26-31` |
| IPC | `src-tauri/src/commands/flow.rs` (`FlowNodeKindDto` :112, conversions :136-190, `run_flow` :373, `cancel_flow_run` :384) |
| Frontend | `src/lib/tauri-api.ts:1760` (node kind union), :1898-1947 (flow event listeners); `src/components/flow/` (`FlowCanvas`, `FlowToolbar`, `NodePalette`, `nodes/*`, `properties/*`); `patchFlowNodeStatus` in `src/stores/pane-store.ts:792` |
| Prior listener | `src-tauri/src/commands/oauth2.rs:554-629` (one-shot OAuth callback, not reused) |

## 2. Goals

- A user can turn on **Repeat until** on any Request node, set a condition, interval, max attempts and timeout, and see the attempt count while it runs.
- A user can add a **Wait for callback** node, use its URL as `{{callback.<name>}}` in an earlier request, and have the flow continue when the call arrives.
- Stop ends a poll or wait at once, not only between nodes.
- Every existing flow file loads, runs the same, and re-saves byte-identically.

## 3. Decisions made in brainstorming

1. A poll or wait that gives up **fails the node**. Downstream nodes are skipped as `upstream_failed`. There is no timeout exit.
2. Polling is a **setting on the Request node** (`repeat_until`), not a new node kind. The palette gets a "Poll request" entry that adds a Request with it turned on.
3. Conditions are **script conditions**, the same as the If node.
4. Each attempt **runs the pre-request and post-response scripts**. Only the **final attempt** is saved to History.
5. Callbacks are **local only**: callers on this machine or its local network.
6. The callback URL is a **run-scoped variable**, and listeners open **when the run starts**. Calls that arrive before the node's turn are held.

## 4. Parts and order

| Part | Adds | Covers | Depends on |
|---|---|---|---|
| **P0 Foundation** | Stop during a wait, `FlowStepProgress` event | all | — |
| **P1 Repeat until** | `repeat_until` on Request nodes | cases 1 and 2 | P0 |
| **P2 Wait for callback** | `WaitForCallback` node kind, callback listener | case 3 | P0; P1 only for the shared condition editor |

Each part is usable on its own when merged.

---

## 5. P0 — Foundation

### 5.1 Stop during a wait

Today `cancel(run_id)` adds the id to a set, and `run` checks it only before each node (`flow_execution_service.rs:632`).

- Add a per-run cancel signal that a node can await: a `tokio::sync::watch` channel of `bool`. `tokio` is already a dependency, while `tokio-util` is only transitive, so no new crate is added.
- `run` creates the signal and registers it next to the existing `in_flight` entry. `cancel` triggers it as well as adding to the `cancelled` set, so the existing between-node check keeps working.
- `execute_node` receives a `NodeRunContext { run_id, cancel }` value. It does not receive `&self`'s maps.
- A waiting node uses `tokio::select!` on its sleep or receive and on the cancel signal. On cancel it returns a new internal error kind that `run` maps to the existing `stopped_reason = "cancelled"` and records the node as `Failed` with the error `"cancelled"`.
- A request that is already in flight still finishes. Aborting the HTTP call is out of scope.
- The signal and the `in_flight` entry are removed on every exit path of `run`, including an early `?` return.

### 5.2 Progress event

New variant in `crates/rocket-shared/src/events.rs`:

```rust
/// Emitted while a node is still running, to report progress such as a
/// poll attempt or a callback wait. It never changes the node's status.
FlowStepProgress {
    run_id: String,
    node_id: String,
    /// 1-based attempt number, or `None` when attempts do not apply.
    attempt: Option<u32>,
    max_attempts: Option<u32>,
    /// Short text shown on the node, such as "attempt 3/30" or
    /// "waiting… 42s left · 1 ignored call".
    message: String,
},
```

- `src-tauri/src/tauri_event_bus.rs` maps it to `flow-step-progress`.
- `src/lib/tauri-api.ts` adds `onFlowStepProgress`. Both run paths in `FlowToolbar.tsx` subscribe and patch `nodeDetail[nodeId].progress`.
- `FlowStepCompleted` clears `progress` for that node.
- Nodes show `progress` under their title while their status is `running`, in the place `NodeStatusCaption` uses for "Not taken".

### 5.3 Compatibility

P0 adds no saved fields. Existing flows run with the same statuses and events, plus no progress events.

---

## 6. P1 — Repeat until

> 📖 Before starting any task in this part, read `docs/superpowers/specs/opencollection-spec-reference.md`.

### 6.1 Data model (`rocket-flow`)

```rust
Request {
    label: String,
    source: RequestSource,
    #[serde(default, skip_serializing_if = "is_false")]
    debug: bool,
    /// When set, the request is sent again until `condition` holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    repeat_until: Option<RepeatUntil>,
},

pub struct RepeatUntil {
    /// Script condition, evaluated like an If node's condition against
    /// `response`. The request is done when it is truthy.
    pub condition: String,
    /// Pause between attempts. Default 2000, minimum 100.
    pub interval_ms: u64,
    /// Default 30, range 1..=1000.
    pub max_attempts: u32,
    /// Overall deadline from the first send. Default 60000, maximum 3_600_000.
    pub timeout_ms: u64,
}
```

Saved example:

```yaml
kind: Request
label: Poll job status
source: { type: Saved, request_path: jobs/get-job.yml }
repeat_until:
  condition: response.body.status === "done"
  interval_ms: 2000
  max_attempts: 30
  timeout_ms: 60000
```

A flow without `repeat_until` saves exactly as before.

### 6.2 Validation (`validate.rs`)

- `condition` must not be blank.
- `interval_ms >= 100`, `1 <= max_attempts <= 1000`, `timeout_ms <= 3_600_000`, `timeout_ms >= interval_ms`.
- A failing rule is a save-time error naming the node, like the existing If rules.

### 6.3 Execution (`execute_node`, Request arm)

1. Resolve wired URL, header and body values once, before the first attempt. Every attempt uses the same resolved input. Pre-request scripts may still change it per attempt.
2. For attempt `n` in `1..=max_attempts`:
   1. Publish `FlowStepProgress { attempt: n, max_attempts, message: "attempt n/N" }`.
   2. Send with `execute_capturing`. History is skipped for this attempt (6.4).
   3. If the send itself errors (network error, script error), fail the node at once. Only a received response is retried.
   4. Evaluate `condition` with `evaluate_flow_route_expression(…, FlowCoercion::Bool, …)` against the response. A non-2xx response is still evaluated, so `response.status === 200` can wait for a 404 to turn into a 200.
   5. A script error in `condition` fails the node at once with the script error.
   6. If the condition is true, save this attempt to History and succeed. The node's output is this response, as for a normal Request. The node succeeds even when the final response is non-2xx, because the author's condition decides "done" (for example `response.status === 404` to wait for a delete). `result_to_step` gets this case from a `condition_met` flag rather than the status code.
   7. If the condition is false and the deadline or the attempt limit is reached, save this attempt to History and fail with `"condition not met after {n} attempts ({elapsed}s)"`.
   8. Otherwise sleep `interval_ms`, cut short by the deadline, inside `select!` with the cancel signal (5.1).
3. `FlowStepCompleted` carries the last response's status code and total duration. Its `debug_request` is the last attempt's request. `FlowStepResult` gains `attempts: Option<u32>`.

### 6.4 History

`execute_capturing` gains a `record_history: bool` parameter that is passed to `finish_phases`, which skips `history_repo.save` when it is false. All existing callers pass `true`. The poll loop passes `false` for every attempt and then saves the final attempt's entry itself through a small `RequestExecutionService` method, so History holds one entry per poll.

Console output, test results and `RequestExecuted` events are still published for every attempt, because scripts run on every attempt.

### 6.5 IPC and frontend

- `FlowNodeKindDto::Request` gains `repeat_until: Option<RepeatUntilDto>` (camelCase DTO), mapped both ways. `FlowStepResult` and the completed event gain `attempts`.
- `src/lib/tauri-api.ts` mirrors both.
- **Request card:** a row under Body, `↻ until <condition> · <interval>s · max <N>`, with the condition truncated. It has no handle. When a run finishes it shows `✓ 200 · 7 attempts · 14.2s`.
- **Properties panel:** a "Repeat until" section with a switch, the condition in the same editor the If node uses, and number fields for interval (seconds), max attempts and timeout (seconds). Turning the switch on fills the defaults.
- **Palette:** a "Poll request" entry that adds a Request node with `repeat_until` set to the defaults and the condition `response.status === 200`, then opens the request picker like the normal Request entry.

---

## 7. P2 — Wait for callback

> 📖 Before starting any task in this part, read `docs/superpowers/specs/opencollection-spec-reference.md`.

### 7.1 Data model (`rocket-flow`)

```rust
/// Waits for an inbound HTTP call on a run-scoped local URL. Requests use
/// the URL as `{{callback.<name>}}`.
WaitForCallback {
    label: String,
    /// Letters, digits and `_`. Unique within the flow.
    name: String,
    /// Default 60000, maximum 3_600_000.
    timeout_ms: u64,
    /// Optional script condition over `request`. Calls that do not match
    /// are answered and ignored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    accept_when: Option<String>,
},
```

Per-flow setting, on `Flow`:

```rust
/// Host used in callback URLs. `None` means this machine's LAN IP.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub callback_host: Option<String>,
```

### 7.2 Validation

- `name` matches `^[A-Za-z0-9_]+$` and is unique among WaitForCallback nodes.
- `timeout_ms` is in `1000..=3_600_000`.
- The node accepts a `trigger` input (`accepts_trigger`) and no data inputs. It has one exit, `result`.
- `kind_name`, `source_handle_exists` and the frontend `flow-wiring.ts` rules include the new kind.

### 7.3 Listener (`rocket-app` trait, `rocket-infra` implementation)

```rust
#[async_trait]
pub trait CallbackListener: Send + Sync {
    /// Opens one endpoint and returns its URL and a receiver of calls.
    async fn open(&self, host: &str) -> DomainResult<CallbackEndpoint>;
}

pub struct CallbackEndpoint {
    pub url: String,
    pub calls: tokio::sync::mpsc::Receiver<ReceivedCall>,
    /// Dropping this closes the endpoint.
    pub guard: Box<dyn Send>,
}

pub struct ReceivedCall {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
    pub body: String,
}
```

The `rocket-infra` implementation:
- Binds `0.0.0.0:0` (a free port) per endpoint using `hyper` 1.x, which is already in `Cargo.lock`. The plan confirms the server features needed.
- Serves only `/cb/<token>`, where `<token>` is 32 random URL-safe characters per run. Any other path gets `404`.
- Answers each call at once with `200` and body `{"received":true}`, then sends the call to a bounded channel that holds up to 100 calls. When the channel is full, the call gets `503` and is not delivered.
- Caps the request body at 1 MB. Larger calls get `413` and are not delivered.
- Shuts down when `guard` is dropped. Later calls cannot connect.

The URL is `http://<host>:<port>/cb/<token>`. `<host>` is `Flow.callback_host`, or this machine's first non-loopback IPv4 address, or `127.0.0.1` if there is none.

`rocket-app` already dev-depends on `rocket-infra`, so `rocket-infra` cannot implement a `rocket-app` trait without a dependency cycle. The server in `rocket-infra/src/callback_server.rs` therefore exposes its own `ServerCall` and `ServerEndpoint` types, and a thin adapter, `src-tauri/src/callback_adapter.rs` (`HyperCallbackAdapter`), implements `CallbackListener` on top of it. `src-tauri/src/lib.rs` wires the adapter into `FlowExecutionService` with `with_callback_listener`. Tests use an in-memory fake.

### 7.4 Execution

At run start, before `FlowRunStarted` and before the run is marked in flight (`crates/rocket-app/src/flow_callbacks.rs`, `RunCallbacks`):
1. For each WaitForCallback node, call `open(host)`. A failure fails the whole run before any node runs, with a clear error.
2. Put `callback.<name> = <url>` into a run-scoped variable map. `ExecuteRequestInput` gains `flow_vars: HashMap<String, String>` (serde default, empty for every non-flow caller). `build_execute_request_input` fills it for every Request node, and `begin_phases` merges it into `VariableContext.runtime` before scripts run, so `{{callback.<name>}}` resolves in any field and in scripts. Values set by scripts still win, as runtime merging already works.
3. Keep each endpoint in the run's state. Endpoints are dropped when `run` returns, on every path.

When a WaitForCallback node runs (`execute_node`):
1. Start a deadline of `timeout_ms` from now. Time spent before the node's turn does not count.
2. Loop: `select!` on the next call, the deadline and the cancel signal.
   - For a call, if `accept_when` is set, evaluate it with the call exposed as `request` (`method`, `path`, `query`, `headers`, `body` parsed as JSON when possible). This uses a sibling of `evaluate_flow_route_expression` that binds `request` instead of `response`, with the same sandbox, timeout and `FlowCoercion::Bool`. A script error fails the node. A false result increments an ignored count and continues.
   - Every second while waiting, publish `FlowStepProgress { message: "waiting… {left}s left · {k} ignored call(s)" }`.
3. An accepted call succeeds the node. Its output is a `CapturedOutput::Request` whose response has status `200`, the call's method as `status_text`, the call's headers and body, and `duration_ms` measured from the node's start. Downstream wires use `response.body` and `response.headers` as for a Request node. The step's `value` is the received method, so the card can show "✓ received POST".
4. The deadline fails the node with `"no matching callback within {timeout}s ({k} ignored)"`.

A skipped or failed WaitForCallback node keeps its endpoint open only until the run ends. Its calls are answered and discarded.

### 7.5 IPC and frontend

- `FlowNodeKindDto::WaitForCallback` and `FlowDto.callbackHost`, mapped both ways. `tauri-api.ts` mirrors them.
- **Node card** (`nodes/WaitForCallbackNode.tsx`): a Run when handle, a result handle, the title, `⏳ Wait for callback · <name> · <timeout>s`, and the variable `{{callback.<name>}}` with a copy button. After a run it shows `✓ received POST · 3.1s` or the failure.
- **Properties panel** (`properties/WaitForCallbackEditor.tsx`): label, name, timeout in seconds, and `accept_when` in the same condition editor as the If node, with `request` described in the help text. A note says the URL is reachable from the local network while a run is active.
- **Flow settings:** a "Callback host" field in the flow toolbar menu, with placeholder "auto (LAN IP)" and the hint `host.docker.internal` for Docker.
- **Palette:** a "Wait for callback" entry. New nodes get the name `callback`, `callback_2` and so on.
- The node is added to `FlowCanvas` `nodeTypes`, `flow-wiring.ts`, `flowExits.ts`, `NodePropertiesPanel.tsx` and `NodePalette.tsx`.

---

## 8. Error handling

| Situation | Result |
|---|---|
| Poll condition never true | Node fails: "condition not met after N attempts (Ts)" |
| Poll condition script error | Node fails at once with the script error |
| Poll send error (network, pre-request script) | Node fails at once, as for a normal Request |
| Callback listener cannot bind | Run fails before any node, "could not open callback listener: …" |
| No matching callback in time | Node fails: "no matching callback within Ts (k ignored)" |
| `accept_when` script error | Node fails at once with the script error |
| Stop during a poll or wait | Node fails with "cancelled", run stops with `stopped_reason = "cancelled"` |
| Invalid `repeat_until` or node settings | Save-time validation error naming the node |

## 9. Testing

**P0 (Rust):** cancel during a simulated wait returns promptly; the run reports `cancelled`; the signal and `in_flight` entry are cleaned up on every exit path; `FlowStepProgress` is published in order. **(TS):** progress text shows while running and clears on completion.

**P1 (Rust):** succeeds on attempt N; gives up at max attempts; gives up at the deadline; a 404 is retried until a 200; a condition script error fails at once; a send error fails at once; cancel between attempts; History has exactly one entry per poll; a flow without `repeat_until` round-trips byte-identically; validation rejects each bad value. **(TS):** the card row renders; the properties section edits and saves; the palette entry adds a configured node.

**P2 (Rust):** a call before the node's turn is accepted; a call during the wait is accepted; `accept_when` ignores non-matching calls; timeout fails with the ignored count; cancel during the wait; endpoints close on success, failure, cancel and skip; a wrong token gets 404; a body over 1 MB gets 413; `{{callback.name}}` resolves in URL, header, body and scripts; validation rejects bad names and duplicates. The listener implementation gets an integration test in `rocket-infra` against a real local socket. **(TS):** the node renders; the editor edits and saves; wiring rules accept Run when in and result out; the callback host setting saves.

**Manual (per part):** P1 with a local endpoint that returns `pending` twice, then `done`. P2 with a local script that posts to the callback URL after a delay.

## 10. Implementation plan split

Plans use at most 3 tasks per plan file plus an index, under `docs/superpowers/plans/flow-async/`.

- **P0:** 01 cancel signal and `NodeRunContext`; 02 `FlowStepProgress` event through to the canvas.
- **P1:** 03 model, validation, DTO and TS types; 04 engine loop and History flag; 05 card row, properties section, palette entry.
- **P2:** 06 `CallbackListener` trait, fake and `hyper` implementation; 07 node kind, validation, run-start endpoints and `callback.*` variables; 08 node execution; 09 DTO, TS types, node card, editor, callback host setting, palette entry.

## 11. Acceptance criteria

- The job-and-status flow (case 1) runs to completion with a Repeat until Request, showing the attempt count while it runs.
- The publish-then-read flow (case 2) waits for the read endpoint to show the result, then passes it downstream.
- The callback flow (case 3) sends `{{callback.<name>}}` in a request, receives the call from a local service, and passes `response.body` downstream.
- Stop ends a running poll or wait within one second.
- Every existing flow file loads, runs with the same statuses, and re-saves byte-identically.
- `cargo check`, targeted `cargo test -j4 -p <crate>`, `yarn tsc --noEmit`, `yarn check` and `yarn test src/components/flow` pass.
