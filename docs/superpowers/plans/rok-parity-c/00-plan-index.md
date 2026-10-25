# rok API parity, sub-project C (cookies): plan index

**Spec:** `docs/superpowers/specs/2026-10-07-rok-js-api-parity-c-cookies-design.md`
**Series notes:** `docs/features/rok-api-parity-notes.md`. Roadmap row: R-03 in `docs/roadmaps/roadmap.md`.
**Built on:** Part B (`docs/superpowers/plans/rok-parity-b/`) and its review fixes (`docs/superpowers/plans/rok-parity-b-fixes/`). Read both indexes for the `ScriptHost` model, the host-call cap (`ScriptLimits.max_host_calls`) and the vault and guard rulings.
**Ledger:** one per plan, `.superpowers/sdd/rok-parity-c-<plan file name without .md>/progress.md` (create it on the plan's first task; log `Ruling:` lines there).

Run the plans in order. Each plan has at most 3 tasks and produces working, tested software on its own. When a plan is complete, move on to the next without asking.

| Plan | File | Tasks |
|---|---|---|
| 01 | `01-jar-lock-and-service.md` | `Cookie.same_site` plus pure script-cookie helpers in `rocket-http`; one shared jar lock for the reqwest store, `CookieService` and scripts; the `ScriptCookies` service in `rocket-app` |
| 02 | `02-engine-host-and-js.md` | `ScriptHost` cookie call, `ScriptContext` snapshot, engine ops and `rok.cookies` / `jar()` in `bootstrap.js`; the host side and end-to-end tests; typings, snippets, docs and the manual check |

## Findings that shaped the plans (read before starting)

The spec's open risk was "confirm `CookieService` and `RepoCookieStore` share one lock or one repo". They do not.

- `RepoCookieStore` (`crates/rocket-infra/src/cookie_store.rs`) owns a private `write_lock: Mutex<()>`, held for one batch of `Set-Cookie` headers.
- `CookieService` (`crates/rocket-app/src/cookie_service.rs`) holds its own `Box<dyn CookieRepository>` and takes no lock for `save` and `clear`.
- `RequestExecutionService` already holds an unused `cookie_repo: Box<dyn CookieRepository>` field ("Reserved for automatic cookie persistence").
- `src-tauri/src/lib.rs` builds three separate `SharedPathCookieRepo` handles over the same directory: for `CookieService` (about line 465), for `ReqwestExecutor::with_cookie_repo` (about line 484) and for the execution service (about line 586).
- Each handle builds a short-lived `FsCookieRepo` per call and writes with `atomic_write`, so a file is never torn. But two read-modify-write cycles from different handles can lose an update.

## Rulings (read before starting)

1. **One shared lock.** A `CookieJarLock` (a cloneable `Arc<Mutex<()>>` wrapper, poison-tolerant) lives in `rocket-http`. `src-tauri` creates one and passes it to the executor's store, to `CookieService` and to `RequestExecutionService`. Constructors that exist today keep their signatures: each gets a `with_cookie_lock(lock)` builder and defaults to a fresh private lock, so the many test call sites do not change.
2. **Script cookie logic lives in a new `rocket-app` module, `script_cookies.rs`,** not in `CookieService`. The spec says `CookieService` gets the script methods, but the execution service does not hold a `CookieService`; it holds the repo and (now) the lock. `ScriptCookies` borrows `&dyn CookieRepository` and `&CookieJarLock`, so scripts and the UI service use the same repo contract and the same lock. `CookieService.save` and `.clear` also take the lock (they are writers).
3. **JS-facing types live in `rocket-http`.** `CookieEntry` (what a script sees: `id, key, value, domain, path, secure, httpOnly, expires, maxAge, sameSite`) and `CookieInput` (what a script passes to `add`, `upsert`, `setCookie`) are plain serde DTOs with `rename_all = "camelCase"`, because that is the JS contract. They are not persistence structs. `rocket-scripting` already depends on `rocket-http`, so `ScriptContext` and `ScriptHost` use them directly.
4. **One host method.** `ScriptHost::cookies(&self, op: HostCookieOp) -> Result<HostCookieResult, HostError>` with a default of `Unavailable`, instead of eight methods. One `HostCall::Cookies` variant, one async op `op_rok_cookies`.
5. **Write operations return the fresh list.** Every request-scoped write returns the cookies now matching the URL, and the JS wrapper replaces its local snapshot with them, so a following `rok.cookies.get` is consistent (spec section 2).
6. **Snapshot URL.** The request-scoped snapshot and URL are taken when each script phase starts, from `ctx.request.url` (the resolved request after earlier scripts' mutations, before the running script's own `req.setUrl`). The spec says "final request URL after pre-request mutation". A `req.setUrl` made later in the same script is not seen by `rok.cookies`. Documented as a known limit.
7. **Domain rules.** A `domain` that does not match the URL host (`domain_matches`) rejects. A leading dot is stripped. No `domain` gives a host-only cookie on the URL host. `__Host-` cookies: `domain` must be omitted, `secure` must be true, and `path` must be `/` (the RFC 6265bis rule); otherwise reject.
8. **`maxAge` in entries is derived:** whole seconds until `expires`, or `null` for a session cookie or an expired one. It is not stored. `maxAge` in input wins over `expires`, capped at 400 days (`MAX_COOKIE_AGE_SECS` in `cookie.rs`); `maxAge <= 0` on `add` and `upsert` rejects (use `remove`).
9. **Jar on or off.** Scripts read and write the jar whatever the request's `use_cookie_jar` option says. A request with the jar off does not send the cookies. Documented.
10. **Out of scope:** R-68 (redirect hop guard), the cookie management UI, per-workspace or per-collection jars, History badges and scanner flags for `rok.cookies.jar` (spec Risks: later). The Safe-mode exposure (a script can read any domain's cookies and send them with `rok.sendRequest`) is accepted by the spec and stays.
11. **Frontend commands use Bun** (the user's standing rule overrides the `yarn` commands in the project `CLAUDE.md`): `bunx tsc --noEmit`, `bun run check`, `bun run test <pattern>`.

## Next plan to execute

Plan 01 first. After plan 02, sub-project C is complete. The next roadmap items are R-04 (runner `iterationData`, needs a data-file feature first) and R-05 (`rok.grpc.*`, needs a spec); both need a brainstorm before a plan.
