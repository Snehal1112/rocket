# Backend sends resolve `{{process.env}}`

Ticket: https://github.com/Snehal1112/rocket/issues/112

## Goal

A request that resolves `{{process.env.NAME}}` (an OS variable or a `.env` value) when the user clicks Send resolves it the same way, with the same values, when the backend runs it: a Flow request node, a script's `rok.runRequest`, and the assistant's `run_request`. The collection trust gate decides on every path, exactly as it does for a normal send today.

## Why

Users keep API keys and tokens in `.env` files next to the collection. Today those requests work in the request tab and break as soon as they run inside a Flow, behind `rok.runRequest`, or from the assistant: the placeholder is sent as literal text. `docs/features/collection-trust.md` lists this under Known limits.

## Current state (checked in source on main@08dfd726)

| Area | Today | Where |
|---|---|---|
| Single send | The frontend resolves `process.env.*` in its first pass, so the backend gets the request already filled in | `src/lib/execute-request.ts` (`buildVariableContext`, `fetchProcessEnv`) |
| Backend resolution of an HTTP send | `resolve_request_parts` builds its map with `build_variable_context`, which has no process env | `crates/rocket-app/src/execution_service.rs:1307-1321` |
| Backend protocols that do resolve it | WebSocket, GraphQL subscriptions and gRPC use `build_variable_context_with_process_env` | `execution_service.rs:1221-1240`, `execution_service/websocket_resolution.rs:150`, `:216`, `src-tauri/src/commands/grpc.rs:57` |
| The values and the gate | `process_env_for(collection, environment, global)` returns the layered map (default `.env` files, environment `.env` files, OS on top) and `dot_env_keys`; it returns nothing and reads no file when `process_env_allowed(collection)` is false | `execution_service.rs:1082-1174` |
| `.env` values as secrets | The frontend masks every `.env` value as a secret in the Console | `src/lib/execute-request.ts:141-144` (`dotEnvValues`) |
| Backend secret masking | `var_ctx.secret_values` masks history, console and outputs; `run_secret_values` carries it out of a run | `execution_service.rs:187`, `:2839` |
| Assistant output | `run_request` masks the body and test results with `run_secret_values` plus the request's literal credentials | `crates/rocket-app/src/mcp_tool_service.rs:765-870` |
| Flow | Request nodes go through the same phases; Input nodes, Auth-node fetch context and `send_time_credential_value` call `build_variable_context` directly | `crates/rocket-app/src/flow_execution_service.rs:1165`, `:1646`, `:1999` |

Every backend HTTP send reaches `resolve_request_parts`: Flow request nodes, the collection runner, `rok.runRequest` (`execute_nested`), the load test (`load_test_service.rs:34`), GraphQL introspection, and the assistant's `run_request`.

## Requirements

- **R1** A Flow request node resolves `{{process.env.NAME}}` from the OS environment and from the collection's `.env` files (default and environment `dotEnvFilePath` chain), with the precedence a normal send uses: process env is the lowest layer, and a real OS variable wins over a `.env` value with the same name.
- **R2** A request run by `rok.runRequest` resolves `{{process.env.NAME}}` the same way, using the calling send's collection and environment.
- **R3** The assistant's `run_request` resolves `{{process.env.NAME}}` the same way, and the request is sent with the real values.
- **R4** The trust gate holds on every backend path: when `process_env_allowed(collection)` is false, no `.env` file is read, no OS value is used, and the placeholder is sent as written, as a normal send does.
- **R5** Every value that came from a `.env` file and was resolved by a backend send is treated as a secret value of that send, so history, the Console, Flow step traces and run outputs mask it, as they do for secret variables.
- **R6** In the assistant's `run_request` output (body, test results, anything returned to the agent), every process env value the request actually used is masked, real OS values included, not only `.env` values.
- **R7** When a backend send uses a `{{process.env.*}}` placeholder and the gate withholds access, the run's Console gets one warning line, worded like the frontend's `warnIfProcessEnvWithheld`. The send is not blocked and the step or script is not failed.
- **R8** Nothing changes for a normal send from the request tab, for WebSocket, GraphQL subscriptions and gRPC, or for the precedence of any other scope.

## Decided

- Scope is the three paths in the ticket, plus every other backend HTTP send that shares `resolve_request_parts` (the collection runner, the load test, GraphQL introspection). They get the behavior because the fix goes in the one shared place; special-casing them out would make a request behave differently by path. (Claude's call, accepted by the user.)
- The assistant's runs do send host values (user's decision). Masking in its output is stricter than elsewhere (R6) because the agent reads the responses. (User's decision.)
- `.env` values are secrets everywhere (R5), matching the frontend. OS values that do not come from a `.env` file are masked only in assistant output (R6), matching how a normal send treats them today. (Claude's call, accepted by the user.)
- A withheld value warns and never blocks (R7), matching the frontend. (Claude's call, accepted by the user.)

## Settled after the builder check (Claude's calls)

- **The single send.** It also passes through `resolve_request_parts`, but the frontend has already filled every placeholder it could, using the same gate and the same map. So the backend finds nothing new to resolve there (R8). It must not warn on a single send either, because the frontend already does. R7's line comes only from sends the backend drives: Flow, the runner, `rok.runRequest`, the assistant, the load test and introspection.
- **Which `.env` values are secrets (R5).** All values of `dot_env_keys` for the send, not only the referenced ones, which is exactly what the frontend's `dotEnvValues` masks today. A `.env` key that an OS variable shadows is an OS value. The existing `MIN_REDACTION_LEN` rule still applies.
- **"Used" in R6.** The `process.env.*` names that the first pass recorded in the request's field templates (URL, path, query, headers, body, form, auth, plus inherited folder and collection headers and auth), at every nested `rok.runRequest` level.
- **Script-written text.** `process.env` is not one of the scopes that fill text a script wrote (`req.setUrl`, `setHeader`, `setBody`). That matches today's send, and it keeps a Safe-mode script from building a placeholder that pulls a host value. Placeholders in the saved request still resolve.
- **The warning (R7).** One line per backend send whose templates reference `process.env.*` while the gate is denied, in that send's own Console. A Flow request node gets it in its step; a nested run gets it in the caller's Console, like its other lines.
- **Nested runs and the runner.** A nested level uses the caller's collection, environment and global environment, as `execute_nested` already does. `.env` files are read once per send level or runner step, the same as a normal send reads them once per send.
- **Flow Auth-node static tokens are in scope.** For `Bearer {{process.env.TOKEN}}`, `send_time_credential_value` uses the same map as the send, so the mask matches what is sent.

## Non-goals

- Flow Input nodes and Flow Auth-node token fetches (`FetchContext.vars`) resolving `{{process.env}}`. They build their own maps and are not request sends. Follow-up ticket.
- OAuth2 token requests from `OAuth2Service` resolving `{{process.env}}`. Follow-up ticket.
- Redacting non-`.env` OS values that History stores in clear text. Already a documented known limit and a separate follow-up.
- `rok.interpolate` and `rok.getProcessEnv` (unchanged; the latter stays Developer-mode only).
- Any change to the trust store, capabilities, or the frontend send.

## Constraints

- `rocket-app` only for the logic; no new I/O. `.env` reading stays behind the injected `dot_env_loader` (`rocket-infra`).
- Read `docs/superpowers/specs/opencollection-spec-reference.md` before any task: variable resolution and `.env` are in its trigger list. Its rule (line 543) already says `.env` values are exposed as `{{process.env.NAME}}` only when the gate allows it.
- Every send stays inside `with_send_pins`; collection and environment lookups go through `send_collection_repo()`, `global_env_repo()`, `regular_env_repo()`.
- No panicking unwraps in production paths (project hard rule).
- Phase logic stays shared: do not add process env handling to only one caller of the phase methods (see `crates/rocket-app/CLAUDE.md`, Phase-callable execution).

## Left to the builder

- Whether `resolve_request_parts` calls `build_variable_context_with_process_env` directly or reuses one `process_env_for` result per send (the send reads `.env` files once, not once per field or nested level).
- How `dot_env_keys` values reach `var_ctx.secret_values`, and how the process env values a request used reach `run_request` for R6.
- How a nested `rok.runRequest` run's secret values flow back into the caller's masking.
- The exact warning text for R7, as long as it matches the frontend's meaning.

## Verification

- Rust tests in `rocket-app` for R1 to R8, each failing on main@08dfd726 before the change: Flow request node, `rok.runRequest` target, `run_request` output masking, the denied gate (no file read, placeholder kept), `.env` value masked in history and run outputs, the warning line, and an unchanged result for a frontend-resolved single send.
- `cargo check -j4`, `cargo test -j4 -p rocket-app <names>`, and `yarn tsc --noEmit` and `yarn check` if any frontend file changes.
- Manual check in the app: a Flow and an assistant run against an echo endpoint with a `.env` value; the value is sent and appears masked.

## Docs to update

- `docs/features/collection-trust.md`: remove the Known limits line about backend HTTP, and describe the backend paths under "Host environment".
- `docs/roadmaps/roadmap.md`: mark the item shipped if it is listed.
