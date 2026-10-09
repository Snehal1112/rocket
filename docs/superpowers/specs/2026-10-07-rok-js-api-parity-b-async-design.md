# rok JS API parity, sub-project B: async host calls

Date: 2026-10-07

Series: see `.claude/rok-api-parity-notes.md`. Part A (sync gaps) is `2026-10-07-rok-js-api-parity-a-sync-design.md`.

## Goal

`rok.sendRequest`, `rok.runRequest` and `rok.sleep` work as on Bruno's JavaScript API Reference, with `bru` replaced by `rok`.

## Decisions

- Gating: these APIs work in Safe mode too, like Bruno. No trust check. This widens the exposure assumed by `2026-10-07-js-script-security-design.md`: an untrusted collection can now send env values to any host. See Risks.
- Timeout: split budget (below).
- `req.onFail` stays a no-op. A real implementation needs the V8 runtime to stay alive across the real HTTP send, which the one-run-per-script model cannot do. Known gap.
- Out of scope: `httpsAgent`, `require('https')`, History and Timeline badges for script-originated requests, flow-node host wiring.

## Current state

`run_script` in `crates/rocket-infra/src/scripting/engine.rs` runs the script with a synchronous `execute_script` and never runs the event loop. `axios` is a stub that throws. The 5 s `SCRIPT_TIMEOUT` assumes nothing waits on I/O.

## Design

### 1. Engine model

- User code is wrapped as an async function, so top-level `await` and `return` work. The event loop then runs until the returned promise settles, so unawaited callback-style work also finishes. A rejection is captured as the script error, as thrown errors are today.
- The V8 thread receives a Tokio `Handle`. Async ops forward each host call to the runtime through a oneshot channel, so no `!Send` value crosses threads.
- New trait `ScriptHost` in `rocket-scripting` (async): `send_request` and `run_request`. `ScriptEngine` gains `execute_with_host(ctx, host)`. The existing `execute` delegates with no host. With no host, `sendRequest` and `runRequest` reject with "not available here". `sleep` needs no host.
- Wrapping changes `this` and the return value of the top-level code. Existing sync scripts and `require` behaviour must be unchanged, and tests cover this.

### 2. Budget

- 5 s of script CPU time. The clock pauses while awaiting a host call or sleep.
- Each `sendRequest` uses its own `timeout` option, default 30 s.
- `sleep` is capped at 60 s per call.
- A 5-minute ceiling per script stops endless polling loops.
- The existing watchdog already terminates the isolate through its handle. It reads shared state recording when the current await began and the accumulated await time.

### 3. `rok.sendRequest(options, callback?)`

- Options: `method`, `url`, `headers`, `data` (objects serialized as JSON), `timeout`. Works with `await` or a callback `(err, res)`.
- Resolves to `{ status, statusText, headers, data, responseTime }`, with `data` JSON-parsed when possible.
- The host runs it through `HttpExecutor` with the workspace TLS, proxy and client-certificate settings.
- No variable interpolation, as in Bruno. Scripts use `rok.interpolate`.
- `httpsAgent` throws "not supported". Each call adds a Console entry. Secret values are redacted in everything the script can print.
- Network errors reject with an `Error` carrying a redacted message.

### 4. `rok.runRequest("Folder/Name")`

- Path relative to the collection root, no extension, forward slashes.
- The host resolves it and runs the full pipeline (scripts, auth, variables). It returns the same shape as `sendRequest`.
- Non-HTTP items resolve to `{ status: "skipped" }`. An unknown path rejects with `rok.runRequest: invalid request path - <path>`.
- Recursion guard: a call chain that revisits a request, or is deeper than 5, rejects. This covers a collection-level pre-request script calling `runRequest`.
- Variable writes from the nested run are merged into the outer script's input snapshot and output state, so a later `rok.getVar` or `getEnvVar` sees them.
- The host calls back into `ExecutionService`, which owns the engine. The plan must resolve that cycle with an `Arc`/weak handle, not a second engine.

### 5. `rok.sleep(ms)`

Async op. Values are clamped to 0 to 60000. Non-numbers reject.

## Risks

- Safe-mode network access breaks the trust spec's boundary. Add a note to the security spec. Mitigation to consider later: surface script-originated requests in History, and have the scanner flag `rok.sendRequest`.
- Event-loop wrapping could change timing of existing scripts. Cover with the existing engine test suite before adding features.
- `runRequest` reentrancy into `ExecutionService`.

## Testing

- Engine tests with a fake host: await, callback, rejection, no host.
- `wiremock` for the real host: status, headers, JSON body, timeout, TLS settings passthrough.
- Budget tests with short injected limits: a CPU loop dies at the budget, `sleep` does not count against it, the ceiling kills a polling loop.
- Recursion guard (self call, cycle, depth) and nested-write merge tests.
- Redaction test: a secret in an error message and in a response echo.
- Verification: `cargo check -j4`, targeted `-j4` tests for `rocket-scripting`, `rocket-infra`, `rocket-app`, then `yarn tsc --noEmit` and `yarn check` for typings.

## Required reading for implementation

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. Also `.claude/script-files.md` and `docs/superpowers/specs/2026-10-07-js-script-security-design.md`.

## Implementation notes (2026-10-08)

The plans in `docs/superpowers/plans/rok-parity-b/` corrected these points after reading the code. The full rulings are in `00-plan-index.md`.

- The script thread was a `spawn_blocking` thread on the caller's runtime with no event loop. It now builds its own current-thread Tokio runtime, because deno_core needs one for async ops.
- No Tokio `Handle` crosses to the script thread. Host calls travel over a channel to the task that called the engine, which serves them with a borrowed `&dyn ScriptHost`.
- No `Arc`/`Weak` handle was needed: `ExecutionScriptHost<'a>` borrows `RequestExecutionService`, and nested runs reuse the same stateless engine.
- The budget counts busy time (time running code) rather than subtracting await time, so an unawaited request cannot hide a busy loop. The watchdog stops a waiting run with an abort flag and running code with `terminate_execution`.
- Wrapping scripts as async functions makes top-level `var` and function declarations local, runs promise callbacks that used to be dropped, and turns an unhandled rejection into the script error. Error text keeps its `Uncaught` prefix.
- `test()` awaits async bodies. 4xx and 5xx responses resolve. `runRequest` paths are request file paths relative to the collection root, without extension. GraphQL items run; WebSocket and gRPC items are skipped.
- Known limits: env, collection and global writes a script makes before `runRequest` are not visible to the nested run, and later phases of the outer request do not see env values the nested run wrote (as with `setEnvVar` today).
