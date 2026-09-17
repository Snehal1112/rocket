# Collection Runner — Design Spec

**Date:** 2026-09-16
**Status:** Resolved — ready for implementation planning
**Scope:** A Collection Runner feature that executes a folder or collection's requests in sequence, consuming the `rok.runner.setNextRequest()` / `rok.runner.skipRequest()` scripting API that already exists but has no consumer.
**Out of scope:** `rok.runRequest(path)` (ad-hoc request chaining from inside a script — a different feature, still deferred per the SP3 spec).
**Reference:** [Bruno](https://www.usebruno.com/) — the OpenCollection-adjacent tool this app already tracks for scripting-API parity (`rok.*` mirrors Bruno's `bru.*`) — ships a Collection Runner with the closest prior art to this feature. Its actual behavior (verified via its public docs/GitHub, not assumed) directly resolves §8's open questions below; see the "Resolution" line under each.

---

## 1. Background

The SP3 JS scripting spec (`2026-05-20-sp3-js-scripting-design.md`) shipped a `rok.runner` API — `setNextRequest(name | null)` and `skipRequest()` — and the supporting data model (`ScriptResult.next_request: Option<NextRequest>`, `ScriptResult.skip_request: bool`, `req.getExecutionMode(): "runner" | "standalone"`) up front, anticipating a Collection Runner. That spec explicitly deferred the runner itself: *"Collection runner scope isolation — SP3 collection runner (separate plan)"*.

The runner was never built. As of 2026-09-16, a scripting-feature review confirmed:
- `rok.runner.setNextRequest()` / `skipRequest()` are fully wired at the Rust op layer (`crates/rocket-infra/src/scripting/ops/rok.rs`) and set the corresponding `ScriptResult` fields.
- **Nothing downstream reads `ScriptResult.next_request` or `.skip_request`** — not `RequestExecutionService::execute()` in `rocket-app`, not any Tauri command, not the frontend. Calling these ops from a script has zero effect.
- `req.getExecutionMode()` always returns the hardcoded string `"standalone"` (`ScriptContext` constructors in `rocket-scripting/src/context.rs`) — there is no code path that ever sets it to `"runner"`.

This spec proposes closing that gap: an actual Collection Runner feature that executes multiple requests in sequence and honors the runner scripting API.

## 2. Goals

- Run every request in a folder or collection, in a defined order, without manual per-request clicking.
- Let `rok.runner.setNextRequest(name)` / `setNextRequest(null)` / `skipRequest()` actually control the run sequence from inside a script (the primary motivation — these ops already exist and are documented to users).
- Surface per-request pass/fail (test results, status code, error) in a single run summary view.
- Reuse the existing single-request execution path (`RequestExecutionService::execute`) unchanged per request — the runner is an orchestration layer on top, not a new execution engine.

## 3. Non-goals (this spec)

- `rok.runRequest(path)` — synchronous ad-hoc request chaining from inside a script body. Different mechanism (an op that itself triggers execution mid-script), separate spec.
- Data-driven / CSV-parameterized runs (Bruno/Postman "runner with a data file"). Can layer on top later; not required to consume `rok.runner.*`.
- Scheduled/CI runs (`newman`-style headless execution). Out of scope for the desktop app UI.
- Parallel execution across requests in a run — v1 is strictly sequential, matching how `setNextRequest`/`skipRequest` reason about "the next request."

## 4. Sequencing model

A run target is a folder or a whole collection. The default sequence is the same explicit item order already persisted in `_order.yml` (see `rocket-infra` `CLAUDE.md`, "On-disk format") and used by `CollectionService::reorder_items` — no new ordering concept needed. Sub-folders are traversed depth-first in that same order; `OpaqueProtocolItem` entries (GraphQL/gRPC/WebSocket — not round-tripped as executable requests today) are skipped.

Each step runs its phases individually — before-request, then (unless skipped) send, after-response, tests — and the runner inspects the `ScriptResult` **after every phase that actually ran**, not just once at the end:

| Phase result | Runner behavior |
|---|---|
| before-request `skip_request == true` | Do not send this request at all. Skip send/after-response/tests entirely for this step (there is no response to run them against) and record it in the run summary as "skipped by script." |
| any executed phase's `next_request == Some(NextRequest::Name(name))` | After this step finishes (all its phases that were going to run have run), jump to the item named `name` in the current run set instead of the next item in sequence. Unknown name → run summary records an error for this step, run stops. |
| any executed phase's `next_request == Some(NextRequest::Stop)` | End the run after this step, regardless of remaining items. |
| no phase set `next_request` | Continue to the next item in sequence (default). |

Checking `next_request` after **every** phase that ran (not only the last one) is a deliberate improvement over checking it once at the end: it means a before-request script can call both `rok.runner.skipRequest()` and `rok.runner.setNextRequest(name)` together and have the jump actually honored — Bruno's equivalent (`bru.runner.skipRequest()` + `bru.runner.setNextRequest()` combined in a pre-request script) is a known, currently-unresolved limitation in Bruno itself ([usebruno/bruno#5831](https://github.com/usebruno/bruno/issues/5831)) precisely because Bruno only reads `setNextRequest` from post-response/test scripts, which a skipped request never reaches. Rocket's phase-by-phase `ScriptResult` model (three separate `engine.execute()` calls per send, unlike a monolithic pipeline) makes checking after each phase nearly free — this spec takes the opportunity to not inherit that limitation. If two different phases both set `next_request` in the same step (e.g. before-request sets one name, tests sets another), the **last phase that ran wins** — same "later phase overrides earlier" precedent already established for `runtime_vars` merging in `apply_script_side_effects`.

`req.getExecutionMode()` returns `"runner"` for every request dispatched by the runner, `"standalone"` otherwise (single-send from the Request tab) — this already has a field for it, it's just never set to anything but `"standalone"` today.

## 5. Where this plugs into existing code

- **`rocket-app`**: new `CollectionRunnerService`, sibling to `RequestExecutionService` (same DI pattern — trait-object repos, no direct I/O). It does **not** call `RequestExecutionService::execute()` as one atomic black box — per §4, it needs to observe each phase's `ScriptResult` before deciding whether to proceed. Resolution (see §8.1): `RequestExecutionService` gains a lower-level, phase-stepped entry point that both the existing single-send `execute()` and the new runner call into, so the single-send path's behavior/tests are provably unchanged (it becomes a thin wrapper that runs every phase unconditionally, same as today) while the runner gets the per-phase visibility it needs. Exact method shape is an implementation-plan-level decision (verify current `execute()`'s internals first), not finalized further here — but the constraint is fixed: no duplicated phase-orchestration logic between the two callers.
- **`rocket-scripting`**: no new types needed — `NextRequest`, `ScriptResult.next_request`/`.skip_request` already exist and match this design.
- **`src-tauri`**: new IPC commands, e.g. `run_collection(collection, folder_path?, environment_name?)` returning a run ID, plus a way to stream per-step results as they complete: `DomainEvent::RunnerStepCompleted { run_id, item_name, status, test_pass_count, test_fail_count, script_error }` and `DomainEvent::RunnerFinished { run_id, stopped_reason }`, mirroring the existing `RequestExecuted`/`TestsCompleted` event pattern — the frontend already has a Tauri-event-driven update pattern to build on for other live views.
- **Frontend**: a new "Runner" entry point (folder/collection context menu, à la Bruno's "Run Folder"), a run view showing per-step status as steps complete (reuse `TestsPanel`-style pass/fail rendering per step), and a way to stop an in-progress run.

## 6. Out-of-scope details deliberately left to implementation

- Exact IPC command shapes and event payloads (the sketch in §5 is a starting point, not final).
- Whether folder-level and collection-level runs share one command or two.
- Tag-based filtering of which requests run (Bruno supports include/exclude tags at run time; `ScriptContext.request_tags` already exists in this codebase, so this is a cheap, natural follow-up — not required for v1, see §7).

## 7. Acceptance criteria (v1)

- A user can right-click a folder and "Run Folder," and every request in it executes in order.
- A script calling `rok.runner.skipRequest()` (before-request phase) causes that request to be skipped and shown as such in the run summary, with no HTTP call made and no after-response/tests phase run for that step.
- A script calling `rok.runner.setNextRequest("Some Request")`, from **any** phase that runs for that step (before-request, after-response, or tests), jumps the run to that request next instead of the next item in sequence — including when combined with `skipRequest()` in the same before-request script (§4).
- A script calling `rok.runner.setNextRequest(null)` stops the run after the current step.
- `req.getExecutionMode()` returns `"runner"` inside any script running as part of a Runner execution.
- Runtime-scope variables (`rok.setVar`/`rok.getVar`) set in one step are readable via `rok.getVar` in every later step of the same run (§8.2).
- By default, a failed step (non-2xx, or a failed test) does not stop the run — the run continues through every remaining item and the summary reports all failures. A "stop on first failure" toggle is available per run (§8.3).
- The run summary shows, per step: request name, status code (or "skipped"), pass/fail test count, and any script error.
- Each step's individual request still lands in `rocket-history` exactly as a standalone send does today (§8.4) — no new persistence mechanism needed for that part.

## 8. Resolved decisions

Resolved by direct reference to Bruno's actual (not assumed) runner behavior — see the citations inline. Bruno is the closest prior art: this codebase's `rok.*` scripting API already deliberately mirrors Bruno's `bru.*` API (see `rok.runner.setNextRequest`/`skipRequest`, both borrowed 1:1 from Bruno's naming).

### 8.1 Does `execute()` need a phase-level entry point?

**Yes.** Confirmed necessary, not optional — Bruno's own runner architecture works exactly this way: `skipRequest()` is only ever read from the **pre-request** script (before the HTTP call), and `setNextRequest()` is only meaningful from **post-response/test** scripts (after it) — Bruno's docs state each explicitly, and its GitHub issue tracker confirms the two currently don't compose correctly in Bruno itself because of this phase split ([usebruno/bruno#5831](https://github.com/usebruno/bruno/issues/5831)). Rocket already executes each phase as a separate `engine.execute()` call (`rocket-scripting/CLAUDE.md`: "`HttpService` calls `engine.execute(ctx)` three times per send"), so `RequestExecutionService` must expose that phase boundary to the runner rather than hiding it behind one atomic `execute()` call. See §5's resolution: refactor so both the runner and the existing single-send path share one phase-stepped implementation, with `execute()` remaining the unconditional-all-phases convenience wrapper for the single-send case (zero behavior change there).

### 8.2 Do runtime variables carry forward between steps?

**Yes, for the duration of one run.** Bruno's runner is explicitly built around request chaining — carrying a value (e.g. an auth token) from one request's response into the next request's script is the headline use case for having a runner at all (see Bruno's "Request Chaining" docs). Rocket already has the mechanism for this within a single request's phases: `apply_script_side_effects` merges `result.runtime_vars` into `var_ctx.runtime` for the *next phase* of the *same* request (`execution_service.rs:297-302`). The runner extends this one level up: it holds one `VariableContext` for the whole run and carries its `runtime` map forward from step to step (not just phase to phase), resetting only when a new run starts. A step's `env`/`collection`/`folder`/`request` scopes are rebuilt fresh per step as today (they depend on that step's own request/folder), only `runtime` persists across steps.

### 8.3 Does a failed step stop the run by default?

**No — continue by default, matching both Bruno and Postman.** Bruno's CLI runner only stops early when the opt-in `--bail` flag is passed ("stop execution after a failure of a request, test, or assertion... omit it in PR runs where you want the complete list of failures in one report"); the default is to run every item and report all results. `CollectionRunnerService`'s run-request API takes an optional `stop_on_failure: bool` (default `false`), mirroring `--bail`.

### 8.4 Is a run's per-step result persisted?

**Individual requests: yes, for free. The run grouping itself: no, v1 is in-session only.** Each step still goes through the normal single-request execution path per §8.1's resolution (a phase-stepped call into the same `RequestExecutionService` machinery `execute()` uses), which already writes a `HistoryEntry` per request exactly as a standalone send does — no new code needed for that. What does *not* exist yet is a "this history entry belongs to run #N, which also included these other N-1 entries" grouping — v1 does not add that; the run summary is an ephemeral, in-memory view for the duration the Runner panel is open (rebuildable from the stream of `RunnerStepCompleted` events, not from a persisted record). A "run history" grouping concept is a reasonable follow-up, not required to hit this spec's acceptance criteria.
