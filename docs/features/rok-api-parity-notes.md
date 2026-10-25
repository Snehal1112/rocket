# rok JS API parity notes

Goal: every API on Bruno's JavaScript API Reference works in Rocket as `rok.*` (plus `req`, `res`, `test`, `expect`). No `bru` alias, no importer rewrite.

Runtime: `crates/rocket-infra/src/scripting/bootstrap.js` and `ops/{req,res,rok}.rs`. Typings: `src/components/editor/rok-types.ts`. Keep them in sync.

## Series status (2026-10-25)

| Part | Scope | Status |
|---|---|---|
| A | Sync, state-only gaps | Spec and plans committed: `docs/superpowers/plans/rok-parity-a/` (index plus 2 plans). Merged to main (plans 01 and 02, final review fixed). Follow-ups: OAuth2 credential APIs, per-folder `__dirname`, collection read-all op for the overlay, getSize after setBody, spec wording (null writes, empty-string misses). |
| B | Async host calls: `sendRequest`, `runRequest`, `sleep` (`req.onFail` stays a no-op) | Implemented and on main, from `docs/superpowers/plans/rok-parity-b/` (index plus 5 plans) with review fixes in `docs/superpowers/plans/rok-parity-b-fixes/`. Follow-ups are roadmap R-66 (re-resolve the outer request after the pre-request script) and R-67 (review minors). Still true: History badge for script requests, scanner flags for `sendRequest`/`runRequest`. |
| C | Cookies: `rok.cookies.*`, `jar()` | Implemented and on main, from `docs/superpowers/plans/rok-parity-c/` (index plus 2 plans). Spec: `docs/superpowers/specs/2026-10-07-rok-js-api-parity-c-cookies-design.md`. Manual check: `docs/manual-tests/rok-cookies.md`. |
| D | `runner.iterationData`, `iterationIndex`, `totalIterations` | Deferred. Needs a runner CSV/JSON data-file feature first. |
| E | `rok.grpc.*` | Deferred. The gRPC dependency is now on main; it needs a spec. |

## Decisions

- Persistence keeps Rocket semantics, which means what the code does today: every script write except `setVar` already persists (env, global and collection), and `EnvVarWrite.persist` is inert. New set and delete APIs persist the same way. (Earlier notes said the opposite, which was wrong.)
- Bruno-only Developer-mode APIs (`cwd`, `__dirname`, `__filename`, `require` of node built-ins) follow the trust model in `docs/superpowers/specs/2026-10-07-js-script-security-design.md`.

## Decisions for B

- Allowed in Safe mode too, like Bruno (user ruling). This widens the trust spec's boundary, follow-up note needed there.
- Split time budget: 5 s CPU, per-request timeout default 30 s, sleep cap 60 s, 5 min ceiling.

## Decisions for C

Rulings are in `docs/superpowers/plans/rok-parity-c/00-plan-index.md`.

- One shared `CookieJarLock` for the reqwest store, `CookieService` and scripts (ruling 1).
- Script cookie logic is `ScriptCookies` in `rocket-app`, not in `CookieService` (ruling 2).
- One host method, `ScriptHost::cookies`, with one `HostCall::Cookies` and one async op `op_rok_cookies` (ruling 4).
- The request-scoped snapshot URL is read when each script phase starts (ruling 6).
- A `domain` that does not match the URL host rejects; `__Host-` cookies need no domain, `secure: true` and path `/` (ruling 7).
- Scripts read and write the jar whatever the request's jar option says (ruling 9).

Known limits:

- A `req.setUrl` made later in the same script is not seen by `rok.cookies`.
- A `Set-Cookie` stored by `rok.sendRequest` or a nested `rok.runRequest` does not refresh the caller's snapshot (the snapshot is replaced only when a request-scoped write or a `jar()` call uses exactly the request URL string).
- A request with the jar option off does not send script cookies.
- Cookies of any domain are visible to Safe-mode scripts through `jar()`, and `rok.sendRequest` can send them (accepted by the spec's Risks).
- `maxAge` is derived on read (seconds until expiry, or null). On input, zero or negative rejects.
- A cookie key must be an RFC 7230 token and a value cannot hold a control character (tab, CR, LF, NUL, DEL) or `;`. Key plus value is at most 4096 bytes. `RepoCookieStore` also skips a stored cookie that cannot be a header value, so one bad cookie never hides the rest.
- A script-supplied `expires` in the past is stored as an already-expired cookie and never matched. Use `remove` to delete.
- With no host (Flow transform and condition nodes) every async call rejects with `rok.cookies is not available here`. With no request URL they reject with `rok.cookies.<method>: no request URL`.

## Notes for B

- Engine model: scripts run as `(async function () { ... }).call(globalThis)` on a per-script current-thread Tokio runtime inside `spawn_blocking`. Host calls go over a channel to `run_script_bounded`, which serves them with the borrowed `&dyn ScriptHost`.
- Budget: `ScriptLimits` (5 s busy time, 5 min ceiling, 60 s sleep cap) in `crates/rocket-infra/src/scripting/budget.rs`. `DenoScriptEngine::with_limits` injects short limits for tests.
- `ScriptHost` (`crates/rocket-scripting/src/host.rs`) has defaulted methods, so part C adds cookie methods the same way `run_request` was added.
- `ExecutionScriptHost` (`crates/rocket-app/src/execution_service/script_host.rs`) borrows the service. `runRequest` lookup and the recursion guard are in `execution_service/run_request.rs`.

## Notes for C

- Done (see Decisions for C). The jar has no clear or disable UI yet.
- Request-scoped helpers use the active request URL. `jar()` takes explicit URLs. `__Host-` cookies must omit `domain`.
