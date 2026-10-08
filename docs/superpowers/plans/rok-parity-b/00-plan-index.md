# rok API parity, sub-project B: plan index

**Spec:** `docs/superpowers/specs/2026-10-07-rok-js-api-parity-b-async-design.md`
**Series notes:** `.claude/rok-api-parity-notes.md`
**Ledger:** one per plan, `.superpowers/sdd/rok-parity-b-<plan file name without .md>/progress.md` (create it on the plan's first task; log `Ruling:` lines there).

Run the plans in order. Each plan has at most 3 tasks and produces working, tested software on its own. Each plan file ends with a "Next plan to execute" section: the executing Claude moves on to the next plan on its own when the current one is complete (no consent needed between plans). After plan 05 the next work is sub-project C (cookies), which still needs its own plans.

| Plan | File | Tasks |
|---|---|---|
| 01 | `01-async-engine-model.md` | async wrapper and event loop on the script thread, `ScriptHost` with `execute_with_host` and a basic `rok.sendRequest`, `rok.sleep` and async `test()` |
| 02 | `02-split-budget.md` | `ScriptLimits` with injectable limits and the sleep cap, busy-time watchdog with the 5-minute ceiling |
| 03 | `03-send-request.md` | engine-side `sendRequest` (callback form, `httpsAgent`, Console entries, redaction), the `rocket-app` host and its wiring, `wiremock` end-to-end tests |
| 04 | `04-run-request.md` | engine-side `runRequest` with the nested-write merge, path lookup and recursion guard, the host side with `execute_nested` and end-to-end tests |
| 05 | `05-snippets-docs-and-manual-check.md` | snippets and the Monaco top-level-await fix, docs (security spec note, series notes, crate guidance) and the final manual check |

## Rulings and corrections to the spec (read before starting)

These came from reading the real code while planning. Plan 05 Task 2 updates the spec text to match.

1. **The engine does not run on a dedicated thread with its own Tokio runtime.** `run_script_with_timeout` in `crates/rocket-infra/src/scripting/engine.rs` runs `run_script` through `tokio::task::spawn_blocking` on the caller's runtime, calls the synchronous `execute_script`, and never runs the event loop. deno_core 0.400 polls async ops through `deno_unsync::spawn`, which requires a current-thread runtime (it masks `!Send` futures as `Send` and `debug_assert`s the flavor). **Ruling:** the blocking thread builds its own current-thread runtime (`enable_all`) and `block_on`s the script. `spawn_blocking` stays, so the queued-script regression test keeps its meaning.
2. **No Tokio `Handle` crosses to the V8 thread.** The spec says the V8 thread receives a `Handle` and forwards host calls through oneshot channels. **Ruling:** ops send a `HostCall` (with a oneshot reply) over an unbounded mpsc channel to the async task that called the engine (`run_script_bounded`). That task serves the calls with the borrowed `&dyn ScriptHost` in a `FuturesUnordered`. This is what lets the host borrow `&RequestExecutionService` instead of needing `'static` data.
3. **No `Arc`/`Weak` handle is needed for `runRequest`.** `RequestExecutionService` is not in an `Arc`: Tauri manages it by value (`app.manage(exec_svc)` in `src-tauri/src/lib.rs`, `State<'_, RequestExecutionService>` in about 15 commands), and it owns the engine as `script_engine: Option<Box<dyn ScriptEngine>>`. **Ruling:** each script run gets an `ExecutionScriptHost<'a>` that borrows `&'a RequestExecutionService`. A nested run calls back into the same service and the same engine (`DenoScriptEngine` is stateless and builds a new `JsRuntime` per call), so there is no second engine and no cycle. The recursive future is boxed at the host (`Box::pin`). `src-tauri` does not change.
4. **The watchdog is a wall-clock `tokio::time::timeout(5 s)`, not a monitor of await state.** On timeout a plain `std::thread` waits for the `IsolateHandle` and calls `terminate_execution`. Termination does nothing while V8 is idle waiting for an op. **Ruling:** the budget counts *busy time* (time inside `execute_script` and inside each event-loop poll) instead of subtracting await time, so an unawaited slow request running next to a busy loop cannot hide the loop. The watchdog loop in `run_script_bounded` checks busy time against the 5 s budget and wall time against the 5-minute ceiling. On a trip it sets an abort flag with a `tokio::sync::Notify` (which ends a run that is waiting) and keeps the existing "terminate whenever the handle arrives" thread (which ends JS that is running). "CPU time" in the spec therefore means busy wall time on the V8 thread, not OS CPU time.
5. **Wrapping user code changes four observable things.** The spec asks for unchanged sync behaviour. Wrapping as `(async function () { <code>\n}).call(globalThis)` keeps `this`, `require`, globals and error line numbers, but: top-level `var` and function declarations are no longer global properties (Bruno wraps scripts the same way); promise callbacks that were silently dropped now run; an unhandled rejection now becomes the script error; column numbers on line 1 shift by 21. The `Uncaught (in promise)` prefix is rewritten to `Uncaught`, so error text keeps its old shape. Plan 01 Task 1 pins all of this with tests.
6. **Addition: `test()` awaits async bodies.** With `await` available, `test('x', async () => { ... })` would otherwise always pass. Plan 01 Task 3 records the result when the returned promise settles.
7. **`sendRequest` details.** 4xx and 5xx responses resolve (the spec only rejects network errors; axios would reject them). A missing or non-positive `timeout` means 30 s, and values are capped at 300 s (the ceiling). The host enforces the timeout with `tokio::time::timeout`; the executor gets the timeout plus one second as a backstop, so the message is always `rok.sendRequest: timed out after N ms`. An object or array `data` is sent as JSON and gets `Content-Type: application/json` when the script set none. The request inherits the calling request's resolved options: `verify_ssl`, `follow_redirects`, `max_redirects`, `encode_url`, `use_cookie_jar` and the active environment's client certificates (vault certificates are fetched with `with_vault_certificates`). The proxy is applied by `ReqwestExecutor` itself.
8. **Console entries and redaction live in the engine op**, because the op has the secret list (`ScriptInputState.secret_values`). The host returns raw messages. Thrown script errors in general are still not redacted, as today. Only host-call messages and the Console lines are.
9. **`runRequest` path lookup** uses `collection_repo.get` and the runner's `flatten_run_set`, so the path is the request's file path relative to the collection root (folder directory names plus the file name), with the extension stripped. GraphQL items run, as in the Collection Runner. WebSocket and gRPC items resolve to `{ status: "skipped" }`. Opaque items have no file name, so their path is invalid. A request that is not in a collection rejects with the invalid-path message.
10. **Recursion guard.** The chain holds normalized request paths, outermost first, and includes the outer request. A call whose target is already in the chain rejects with `rok.runRequest: recursive call to <path>`. A chain already 6 long rejects with `rok.runRequest: nesting deeper than 5 requests`, so 5 nested levels run. When a collection-level pre-request script calls `runRequest`, the nested run's own copy of that script gets the rejection: it is that nested run's script error, and the nested request still sends (script errors never block a send). An unsaved request has no path, so its chain starts empty.
11. **Nested-write merge.** The nested run is seeded with the caller's runtime variables, including the caller's own writes so far. Afterwards: runtime changes flow into the caller's input snapshot and output (so later phases of the outer request see them); env, global and collection scopes are re-read from the repositories (the nested run already persisted its writes); the caller's earlier pending writes to keys the nested run changed are dropped (the later write wins); the read-your-writes overlay forgets those keys. **Known limits:** env, collection and global writes the caller made *before* the call are not saved yet, so the nested run does not see them; and later phases of the outer request do not see refreshed env values, the same as today's `setEnvVar` (nothing updates `PhaseState.var_ctx.env` after a script write).
12. **Who gets a host.** The three request phases (single send, Collection Runner, Flow request nodes, nested runs) pass an `ExecutionScriptHost`. `apply_actions` (jsonq) and Flow transform and condition nodes call `execute` with no host, so `sendRequest` and `runRequest` reject there with "is not available here".
13. **Nested runs are normal sends otherwise:** they save a History entry and publish their own console, tests and `RequestExecuted` events. There is no badge (out of scope in the spec).
14. **Addition: Monaco must accept top-level `await` and `return`.** Plan 05 Task 1 adds the TypeScript diagnostic codes 1108, 1308, 1375 and 1378 to `diagnosticCodesToIgnore` for script editors, so the new syntax shows no red squiggle.

## Open risks the code could not settle

- deno_core 0.400 `#[op2]` async ops: the vendored test cases (`deno_ops-0.276.0/op2/test_cases/async/`) show `async fn` with `Rc<RefCell<OpState>>`, numeric arguments and `Result` returns. A `#[string]` return on an async op is common in Deno itself but is not in those test cases. If the macro rejects `#[op2] #[string] pub async fn`, try `#[op2(async)]` with the same signature and log a `Ruling:`.
- Building a Tokio runtime inside a `spawn_blocking` thread is allowed (blocking threads are not inside an entered runtime). Plan 01 Task 1's tests prove it on the first run. If it panics with "Cannot start a runtime from within a runtime", run the script on a `std::thread` with a oneshot result instead, adjust the queued-script test, and log a `Ruling:`.
- deno_core reports unhandled promise rejections as event-loop errors. Plan 01 Task 1 pins this. If it does not, drop that one test and log a `Ruling:`.
- The recursive `execute_nested` future must be `Send` through two `async_trait` boxes. It should be, because both trait boundaries erase the type. Plan 04 Task 3 is where it compiles first.
- `runRequest` loads the whole collection tree on every call. Fine for the 5-level limit; revisit if a script calls it in a tight loop.

## Global constraints (apply to every task)

- Hook rules (`.claude/rules/harness.md`): `cargo test|check|clippy|build` must pass `-j4`. Never `cargo test --workspace` or `--all`. No `git add -A`, `--all` or `.`. No bare `git stash`.
- Commits: use the `dev-workflow-skills:1-git-commit` skill with explicit paths and a pathspec commit (`git commit --only ... -- <paths>`). Conventional commit prefixes (`feat:`, `fix:`, `test:`, `docs:`, `chore:`).
- Before a commit that stages `.rs`, `cargo check -j4` must pass. For `.ts` or `.tsx`, `yarn tsc --noEmit` and `yarn check`.
- Rust: no panicking `unwrap` calls in production paths (use `DomainResult`, `ScriptOpError`, `ScriptHostError`, `HostError`). No `rename_all = "camelCase"` on persistence structs. The new `HostRequest`, `HostResponse` and friends are internal engine types with snake_case JSON, not IPC DTOs.
- Keep `crates/rocket-infra/src/scripting/bootstrap.js` and `src/components/editor/rok-types.ts` in sync. The sync test in `src/components/editor/__tests__/rok-types.test.ts` fails if a top-level `rok` member lacks a typing.
- Code comments are short full sentences ending in a period. No emojis in code, comments or commit messages.
- gRPC files and `rok.grpc` are off-limits (another session owns them). Reading `CollectionItem::Grpc(..).file_name` in the new `run_request.rs` is the only gRPC contact and changes no gRPC file.
- Tasks that touch `rocket-infra`, `rocket-app`, collections or variable resolution begin with the line "📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`." (CLAUDE.md injection rule).
- Chain to the next plan automatically when the current one is complete, without asking. One plan at a time; the visible task list shows only the executing plan's tasks.
- `cargo test` takes one positional filter. Several test names go after `--`: `cargo test -j4 -p rocket-infra -- name_a name_b`.

## Verification after each plan

```bash
cargo check -j4
cargo test -j4 -p rocket-scripting
cargo test -j4 -p rocket-infra scripting
cargo test -j4 -p rocket-app execution_service
cargo test -j4 -p rocket-app collection_runner_service
cargo test -j4 -p rocket-app flow_execution_service
yarn tsc --noEmit
yarn check
yarn test rok-types
```

## After the last plan

Sub-project C (cookies) comes next. Its spec is committed: `docs/superpowers/specs/2026-10-07-rok-js-api-parity-c-cookies-design.md`. It has no plans yet, so the executing Claude invokes the `superpowers:writing-plans` skill and writes them under `docs/superpowers/plans/rok-parity-c/` (index plus plan files of at most 3 tasks each). C extends the `ScriptHost` trait from plan 01 with cookie methods that have default bodies, exactly as plan 04 adds `run_request`.
