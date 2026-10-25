# rok parity C 02: engine, host and the JS API Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give scripts `rok.cookies.*` (request-scoped) and `rok.cookies.jar()` in pre-request, post-response and test scripts, on top of plan 01.

**Architecture:** `ScriptContext` carries the request URL and a cookie snapshot, so reads are synchronous. Writes and every `jar()` method go through one async op to a new `ScriptHost::cookies` call, served by `ExecutionScriptHost` with `ScriptCookies`. The JS wrapper in `bootstrap.js` keeps the local snapshot in step with each write's reply.

**Tech Stack:** Rust (`deno_core` ops), JavaScript (`bootstrap.js`), TypeScript (Monaco typings), `wiremock`.

**Spec:** `docs/superpowers/specs/2026-10-07-rok-js-api-parity-c-cookies-design.md`; findings and rulings in `00-plan-index.md`.

## Global Constraints

- 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. Also read `docs/features/script-files.md`, `docs/superpowers/specs/2026-10-07-js-script-security-design.md`, `crates/rocket-infra/CLAUDE.md`, `crates/rocket-app/CLAUDE.md` and `.claude/rules/rust-ddd-boundaries.md`.
- Request-scoped reads (sync, from the snapshot): `get` (value or `undefined`), `has(name[, value])`, `one(id)`, `all`, `count`, `idx(n)`, `indexOf`, `toObject`, `toString` (Cookie-header format), `each`, `find`, `filter`, `map`, `reduce`.
- Request-scoped writes (async): `add`, `upsert`, `remove`, `delete`, `clear`. Each goes to the host at once and updates the local snapshot. `clear()` clears only cookies matching the URL.
- `jar()` methods (async host calls): `getCookie` (entry or `null`), `getCookies`, `hasCookie` (optional callback `(error, exists)`), `setCookie` (`(url, name, value)` or `(url, object)`), `setCookies`, `deleteCookie`, `deleteCookies`, `clear` (whole jar).
- Available in Safe mode, same exposure as Part B (spec Risks; accepted). Cookie values are not added to the redaction list.
- A script with no host (Flow transform and condition nodes, `apply_actions`) gets `rok.cookies is not available here` from every async call; request-scoped sync reads see an empty snapshot.
- Host-call messages shown to scripts never include cookie values (error texts name the cookie, not its value).
- Typings in `src/components/editor/rok-types.ts` and the runtime in `crates/rocket-infra/src/scripting/bootstrap.js` stay in sync (a test in `src/components/editor/__tests__/rok-types.test.ts` pins it).
- Cargo commands always carry `-j4`; never `--workspace` or `--all`. Frontend commands use Bun: `bunx tsc --noEmit`, `bun run check`, `bun run test <pattern>`. Commit through the `dev-workflow-skills:1-git-commit` skill, with a pathspec commit.

## Review Focus

- A cookie whose value holds `;`, `=`, quotes, spaces or Unicode comes back from `get`, `toObject` and `toString` unchanged.
- `rok.cookies.get('missing')` is `undefined`, `jar().getCookie(url, 'missing')` is `null`, and neither throws.
- A write followed at once by a read in the same script sees the write (read-your-writes), also after a failed write (the snapshot is unchanged).
- `add('a','1')` called twice leaves one cookie; `delete` of a cookie that is not there is a no-op, not an error.
- A pre-request script that adds a cookie: the real request carries it. A `Set-Cookie` on the response is visible to the post-response script.
- 50 concurrent `jar().setCookie` calls (a `Promise.all`) keep all 50 cookies and respect the host-call cap.

---

### Task 1: engine side, `ScriptContext`, `ScriptHost::cookies` and the JS API

**Files:**
- Modify: `crates/rocket-scripting/src/host.rs` (`HostCookieOp`, `HostCookieResult`, `ScriptHost::cookies`)
- Modify: `crates/rocket-scripting/src/context.rs` (`cookie_url`, `cookies`, `with_cookies`)
- Modify: `crates/rocket-infra/src/scripting/host_bridge.rs` (`HostCall::Cookies`, serve it)
- Modify: `crates/rocket-infra/src/scripting/ops/host.rs` (`op_rok_cookies`)
- Modify: `crates/rocket-infra/src/scripting/ops/rok.rs` (`op_rok_cookie_snapshot`) and `state.rs` (carry the snapshot in `ScriptInputState`)
- Modify: `crates/rocket-infra/src/scripting/engine.rs` (register both ops, seed the state)
- Modify: `crates/rocket-infra/src/scripting/bootstrap.js` (`rok.cookies`)
- Test: engine tests in `crates/rocket-infra/src/scripting/engine.rs` with a fake host

**Interfaces:**
- Consumes: plan 01 `CookieEntry`, `CookieInput`.
- Produces:
  - `pub enum HostCookieOp { Find { url: String }, Upsert { url: String, cookie: CookieInput }, Remove { url: String, name: String }, ClearUrl { url: String }, ClearAll }` with `Serialize, Deserialize` and `#[serde(tag = "op", rename_all = "camelCase")]`, in `rocket_scripting`.
  - `pub enum HostCookieResult { Cookies(Vec<CookieEntry>), Count(usize), Done }`.
  - `ScriptHost::cookies(&self, _op: HostCookieOp) -> Result<HostCookieResult, HostError>` defaulting to `Err(HostError::Unavailable)`.
  - `ScriptContext.cookie_url: Option<String>` and `ScriptContext.cookies: Vec<CookieEntry>`, both empty by default on every constructor; `ScriptContext::with_cookies(self, url: String, cookies: Vec<CookieEntry>) -> Self`.
  - JS: `rok.cookies` with the surface in Global Constraints; `rok.cookies.jar()` returns the jar object. Errors are `rok.cookies.<method>: <message>`.

- [ ] **Step 1: Write the failing tests** (engine tests with a `FakeCookieHost` holding a `Mutex<Vec<CookieEntry>>` that implements `cookies`, built like the existing `CountingHost` tests):
  - `cookies_reads_come_from_the_snapshot`: a context with two cookies; the script checks `get`, `has` (name and name+value), `one(id)`, `all().length`, `count()`, `idx(0)`, `indexOf`, `toObject()`, `toString()` (`a=1; b=2`), `each`, `find`, `filter`, `map`, `reduce`; assert on the logged output.
  - `cookies_get_missing_is_undefined`.
  - `cookies_write_updates_the_snapshot_from_the_reply`: `await rok.cookies.upsert('c','3'); console.log(rok.cookies.get('c'))` prints `3`; the fake host saw one `Upsert` with the context URL.
  - `cookies_failed_write_leaves_the_snapshot_alone`: the fake host answers `HostError::Failed("boom")`; the script catches, logs the error text `rok.cookies.add: boom` and the snapshot is unchanged.
  - `cookies_delete_and_clear_use_the_url_scope`: `delete('a')` sends `Remove`, `clear()` sends `ClearUrl`.
  - `jar_methods_send_the_right_ops`: one assertion per method (`getCookie` returns `null` for a miss, `hasCookie` callback form gets `(null, true)`, `setCookie(url, name, value)` and `setCookie(url, {..})` both send `Upsert`, `setCookies` sends one per object, `deleteCookie`, `deleteCookies`, `clear`).
  - `cookie_values_with_special_characters_survive`: `ev;il="x y"=é` round-trips through `get`.
  - `cookies_without_a_host_reject_with_not_available`: run with no host; sync reads return empty, `await rok.cookies.add('a','1')` rejects with `rok.cookies is not available here`.
  - `cookie_calls_count_against_max_host_calls`: 20 concurrent `jar().setCookie` with `max_host_calls = 3` never run more than 3 at once (reuse the concurrency counter pattern from `host_calls_are_served_at_most_max_host_calls_at_a_time`).
  - In `host.rs` tests: `host_cookie_op_reads_the_engine_json_shape` and `script_host_cookies_defaults_to_unavailable`.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -j4 -p rocket-infra cookies`
Expected: FAIL (types and ops missing).

- [ ] **Step 3: Implement.** One async op `op_rok_cookies(#[string] op_json: String) -> Result<String, ScriptHostError>` that parses a `HostCookieOp`, sends `HostCall::Cookies { op, reply }`, awaits the answer and returns the result as JSON (`{"cookies":[..]}`, `{"count":n}` or `{"done":true}`); errors go through `host_error(&state, "rok.cookies", error)`. One sync op `op_rok_cookie_snapshot() -> String` (`{"url":..,"cookies":[..]}`) reading the state. In `bootstrap.js` keep the snapshot in a closure array `_ck` and replace it from every write reply that carries `cookies`. `toString()` joins `key=value` pairs with `; ` in snapshot order. `hasCookie` accepts the optional callback like `sendRequest` does. Keep method bodies small: one private helper `_cookieCall(op)` that builds the JSON, awaits the op and throws `rok.cookies.<method>: ...` on failure.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -j4 -p rocket-infra scripting` and `cargo test -j4 -p rocket-scripting`
Expected: PASS (235 scripting tests before this task, plus the new ones).

- [ ] **Step 5: Run `cargo clippy -j4 -p rocket-infra -p rocket-scripting`, then commit** (`feat(scripting): add rok.cookies and the cookie host call`).

---

### Task 2: the host side, snapshots and end-to-end tests

**Files:**
- Modify: `crates/rocket-app/src/execution_service/script_host.rs` (`ExecutionScriptHost::cookies`)
- Modify: `crates/rocket-app/src/execution_service.rs` (`run_script_phase` attaches the snapshot)
- Test: `crates/rocket-app/src/execution_service/script_host_tests.rs`

**Interfaces:**
- Consumes: Task 1 `ScriptHost::cookies`, `ScriptContext::with_cookies`; plan 01 `ScriptCookies`, `UrlScope`, `to_entry`, `RequestExecutionService::cookie_lock()` and the `cookie_repo` field.
- Produces: `impl ScriptHost for ExecutionScriptHost` serves `cookies`: `Find` returns `Cookies(find(..) mapped with to_entry)`, `Upsert` and `Remove` and `ClearUrl` return `Cookies(fresh list for the URL)` (ruling 5), `ClearAll` returns `Done`. A bad URL or a build error becomes `HostError::Failed("<message>")`, without the value.

- [ ] **Step 1: Write the failing tests** (end to end through the service, a real `ReqwestExecutor` with an `FsCookieRepo` tempdir sharing the service's repo and lock, and a `wiremock` server):
  - `pre_request_script_cookie_is_sent_with_the_request`: pre-request `await rok.cookies.upsert('sid','abc')`; the mock requires header `cookie: sid=abc`; assert 200 and no script error. (The request has `use_cookie_jar` on.)
  - `set_cookie_from_the_response_is_visible_to_the_post_response_script`: the mock answers with `Set-Cookie: tok=xyz; Path=/`; the post-response script logs `rok.cookies.get('tok')`; assert `xyz`. (The snapshot for the post-response phase is taken after the response was stored.)
  - `jar_reads_and_writes_cross_domains`: `jar().setCookie('https://other.test/', 'k', 'v')` then `getCookie('https://other.test/', 'k')`.
  - `jar_clear_empties_the_whole_jar`.
  - `host_cookie_error_does_not_leak_the_value`: a `__Host-` cookie with a domain and a recognizable value; the script error text names the cookie and does not contain the value.
  - `cookies_are_unavailable_in_a_flow_transform_node` (a context run through `execute` with no host): rejects with the not-available text.
  - `concurrent_script_and_request_writes_keep_all_cookies`: a script does 20 `upsert`s in a `Promise.all` while the mock answers with `Set-Cookie`; afterwards the jar holds all 21.
  - `nested_run_request_sees_cookies_set_by_the_caller`: the caller upserts a cookie, then `await rok.runRequest('child')`; the child request (mock requires the cookie header) succeeds.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -j4 -p rocket-app pre_request_script_cookie_is_sent`
Expected: FAIL (the host returns `Unavailable`).

- [ ] **Step 3: Implement** `ExecutionScriptHost::cookies`: build `ScriptCookies::new(self.svc.cookie_repo.as_ref(), self.svc.cookie_lock())` and map the op. In `run_script_phase` make `ctx` mutable and, before `engine.execute_with_host`, set `ctx = ctx.with_cookies(url, snapshot)` where `url = ctx.request.url.clone()` and `snapshot` is `ScriptCookies::find(..)` mapped with `to_entry` (an unparsable URL, such as a template left unresolved, gives no snapshot and leaves the URL unset; do not fail the phase). Check the three call paths that build contexts (`before_request`, `after_response`, `tests`) all go through `run_script_phase`; any that do not need the same call.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -j4 -p rocket-app execution_service` and `cargo clippy -j4 -p rocket-app`
Expected: PASS (567 tests before this task, plus the new ones) and no warnings.

- [ ] **Step 5: Commit** (`feat(scripting): serve rok.cookies from the cookie jar`).

---

### Task 3: typings, snippets, docs and the manual check

**Files:**
- Modify: `src/components/editor/rok-types.ts` (types for `rok.cookies` and `jar()`)
- Modify: `src/components/editor/__tests__/rok-types.test.ts` (extend the sync check)
- Modify: `src/components/request/ScriptSnippetSidebar.tsx` (snippets, in its existing format)
- Modify: `docs/features/rok-api-parity-notes.md` (Part C row and a "Decisions for C" section)
- Modify: `docs/roadmaps/roadmap.md` (R-03 done, `What remains` lists the known limits)
- Modify: `crates/rocket-app/CLAUDE.md`, `crates/rocket-infra/CLAUDE.md` (one or two short lines each)
- Create: `docs/manual-tests/rok-cookies.md`

**Interfaces:** none.

- [ ] **Step 1: Extend the typings test first.** In `rok-types.test.ts` add a test `rok_types_declare_every_cookie_method` that reads the methods from `bootstrap.js` the way the existing sync test does and fails when `rok.cookies.*` or `jar()` methods are missing from `rok-types.ts`. Run `bun run test rok-types`; expected: FAIL.

- [ ] **Step 2: Add the typings** (`CookieEntry`, `CookieInput`, `RokCookies`, `RokCookieJar`, callback signatures for `hasCookie`) so the Monaco script editors offer IntelliSense. Run `bun run test rok-types`; expected: PASS.

- [ ] **Step 3: Add snippets** to `ScriptSnippetSidebar.tsx` following its current structure: set a cookie before a request, read a response cookie in post-response, delete a cookie, and `jar().getCookie` for another domain.

- [ ] **Step 4: Update the docs.** In the notes file: Part C status line, and a "Decisions for C" section that copies rulings 1, 2, 4, 6, 7 and 9 of the plan index in one line each, plus the known limits (the snapshot URL is read when the script starts; a request with the jar off does not send script cookies; cookies are visible to Safe-mode scripts for any domain; `maxAge` is derived). In the roadmap: R-03 `done (on main)`, with the limits above in `What remains`; keep R-68 as is. Crate guidance lines: the shared `CookieJarLock` rule ("every writer of the cookie jars takes `CookieJarLock`") in both CLAUDE.md files. In `docs/manual-tests/rok-cookies.md`: eight numbered checks to run in the real app (pre-request add, response cookie in post-response, jar cross-domain read, `delete`, `jar().clear()` confirmation text, cookie with special characters, a Flow transform node rejecting, jar-off request not sending the cookie).

- [ ] **Step 5: Verify and commit.** Run `bunx tsc --noEmit`, `bun run check` and `bun run test rok-types`; expected: all clean. Do not write the literal text for unwrapping a Rust result in any file (a write hook blocks it). Commit (`docs(scripting): document rok.cookies and add typings`).

---

## Next plan to execute

This is the last plan of sub-project C. When Task 3 is complete, the ledger shows "Task 3: complete" and the manual check is done or handed to the user, run a final whole-branch review of plans 01 and 02 together (see `.claude/rules/harness.md`), then finish the branch. R-04 and R-05 each need a brainstorm before a plan.
