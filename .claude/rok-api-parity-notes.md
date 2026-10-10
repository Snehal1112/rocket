# rok JS API parity notes

Goal: every API on Bruno's JavaScript API Reference works in Rocket as `rok.*` (plus `req`, `res`, `test`, `expect`). No `bru` alias, no importer rewrite.

Runtime: `crates/rocket-infra/src/scripting/bootstrap.js` and `ops/{req,res,rok}.rs`. Typings: `src/components/editor/rok-types.ts`. Keep them in sync.

## Series status (2026-10-07)

| Part | Scope | Status |
|---|---|---|
| A | Sync, state-only gaps | Spec and plans committed: `docs/superpowers/plans/rok-parity-a/` (index plus 2 plans). Implemented on branch worktree-rok-parity-a (plans 01 and 02, final review fixed). Follow-ups: OAuth2 credential APIs, per-folder `__dirname`, collection read-all op for the overlay, getSize after setBody, spec wording (null writes, empty-string misses). |
| B | Async host calls: `sendRequest`, `runRequest`, `sleep` (`req.onFail` stays a no-op) | Implemented from `docs/superpowers/plans/rok-parity-b/` (index plus 5 plans). Follow-ups: History badge for script requests, scanner flags for `sendRequest`/`runRequest`, nested runs seeing the caller's unsaved env writes, refreshing `PhaseState.var_ctx.env` after env writes. |
| C | Cookies: `rok.cookies.*`, `jar()` | Spec committed: `docs/superpowers/specs/2026-10-07-rok-js-api-parity-c-cookies-design.md`. Depends on B's `ScriptHost`. No plan yet. |
| D | `runner.iterationData`, `iterationIndex`, `totalIterations` | Deferred. Needs a runner CSV/JSON data-file feature first. |
| E | `rok.grpc.*` | Deferred. Needs the gRPC protocol-parity plans merged. |

## Decisions

- Persistence keeps Rocket semantics, which means what the code does today: every script write except `setVar` already persists (env, global and collection), and `EnvVarWrite.persist` is inert. New set and delete APIs persist the same way. (Earlier notes said the opposite, which was wrong.)
- Bruno-only Developer-mode APIs (`cwd`, `__dirname`, `__filename`, `require` of node built-ins) follow the trust model in `docs/superpowers/specs/2026-10-07-js-script-security-design.md`.

## Decisions for B

- Allowed in Safe mode too, like Bruno (user ruling). This widens the trust spec's boundary, follow-up note needed there.
- Split time budget: 5 s CPU, per-request timeout default 30 s, sleep cap 60 s, 5 min ceiling.

## Notes for B

- Engine model: scripts run as `(async function () { ... }).call(globalThis)` on a per-script current-thread Tokio runtime inside `spawn_blocking`. Host calls go over a channel to `run_script_bounded`, which serves them with the borrowed `&dyn ScriptHost`.
- Budget: `ScriptLimits` (5 s busy time, 5 min ceiling, 60 s sleep cap) in `crates/rocket-infra/src/scripting/budget.rs`. `DenoScriptEngine::with_limits` injects short limits for tests.
- `ScriptHost` (`crates/rocket-scripting/src/host.rs`) has defaulted methods, so part C adds cookie methods the same way `run_request` was added.
- `ExecutionScriptHost` (`crates/rocket-app/src/execution_service/script_host.rs`) borrows the service. `runRequest` lookup and the recursion guard are in `execution_service/run_request.rs`.

## Notes for C

- Needs a cookie jar bridge into `rocket-http`. The jar has no clear or disable UI yet.
- Request-scoped helpers use the active request URL. `jar()` takes explicit URLs. `__Host-` cookies must omit `domain`.
