# rok parity C 01: cookie helpers, shared jar lock and script cookie service Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build everything cookie-related that scripts need below the script engine: the new `same_site` field and pure helpers, one jar lock shared by every writer, and a `ScriptCookies` service that does the reads and writes.

**Architecture:** Pure matching, building and projection helpers go in `rocket-http` (no I/O). A `CookieJarLock` is created once in `src-tauri` and shared by the reqwest cookie store, `CookieService` and `RequestExecutionService`. `ScriptCookies` in `rocket-app` borrows the repo and the lock and runs each operation under the lock.

**Tech Stack:** Rust, `chrono`, `url`, `tempfile` (tests), `wiremock` (not needed here).

**Spec:** `docs/superpowers/specs/2026-10-07-rok-js-api-parity-c-cookies-design.md`; findings and rulings in `00-plan-index.md`.

## Global Constraints

- 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (spec "Required reading"). Also read `crates/rocket-http/CLAUDE.md`, `crates/rocket-infra/CLAUDE.md`, `crates/rocket-app/CLAUDE.md` and `.claude/rules/rust-ddd-boundaries.md`.
- Entry shape (spec section 3): `{ id, key, value, domain, path, secure, httpOnly, expires, maxAge, sameSite }`; `key` is the cookie name; `id` is `domain|path|name`.
- Identity: name, domain and path. `add` and `upsert` both replace an existing cookie.
- No `domain`: host-only cookie on the URL host. No `path`: the RFC 6265 default path (`default_cookie_path`).
- `maxAge` converts to `expires`, capped at 400 days like the existing parser. `__Host-` cookies: `domain` omitted and `secure` required, otherwise reject.
- `sameSite` is stored and round-tripped, not enforced. Cookie values are never logged and never added to the redaction list.
- `Cookie` gains `same_site: Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`, so existing jar files load unchanged. Update every struct-literal construction site (grep `Cookie {` across `crates/` and `src-tauri/`).
- Domain crates have no I/O. Production paths return `DomainResult` errors and never unwrap. camelCase serde only on DTOs, never on the persistence structs (`Cookie` and `CookieJar` keep their existing attributes).
- Cargo commands always carry `-j4`; never `--workspace` or `--all`. Commit through the `dev-workflow-skills:1-git-commit` skill, with a pathspec commit (no `git add -A`).

## Review Focus

- An old jar file with no `sameSite` key loads, and saving it back does not add `sameSite: null`.
- A cookie name or value with `;`, `=`, spaces or non-ASCII text round-trips through a build, a save and a read.
- `maxAge` of 0, a negative number or a huge number: rejected for 0 and negative, capped for huge.
- Host-only versus domain cookies for IP-literal hosts (`127.0.0.1`, `[::1]`) and for `localhost`.
- Two threads upserting different cookies into the same domain at the same time keep both (no lost update), also when one goes through `RepoCookieStore` and the other through `ScriptCookies`.

---

### Task 1: `same_site` and the pure script-cookie helpers

**Files:**
- Modify: `crates/rocket-http/src/cookie.rs` (`Cookie`, add `same_site`; `parse_set_cookie` stores `SameSite` when present)
- Create: `crates/rocket-http/src/script_cookie.rs`
- Modify: `crates/rocket-http/src/lib.rs` (export the new module and types)
- Modify: every `Cookie { .. }` construction site outside `rocket-http`
- Test: unit tests in `script_cookie.rs` and `cookie.rs`

**Interfaces:**
- Produces, all in `rocket_http::script_cookie` and re-exported at the crate root:
  - `pub struct UrlScope { pub host: String, pub path: String, pub https: bool }` and `impl UrlScope { pub fn parse(url: &str) -> DomainResult<UrlScope> }` (error `DomainError::InvalidInput` for a URL without a host).
  - `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] #[serde(rename_all = "camelCase")] pub struct CookieEntry { pub id: String, pub key: String, pub value: String, pub domain: String, pub path: String, pub secure: bool, pub http_only: bool, pub expires: Option<String>, pub max_age: Option<i64>, pub same_site: Option<String> }`.
  - `#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)] #[serde(rename_all = "camelCase", default)] pub struct CookieInput { pub key: String, pub value: String, pub domain: Option<String>, pub path: Option<String>, pub secure: Option<bool>, pub http_only: Option<bool>, pub expires: Option<String>, pub max_age: Option<i64>, pub same_site: Option<String> }`.
  - `pub fn cookie_id(cookie: &Cookie) -> String` (`domain|path|name`).
  - `pub fn to_entry(cookie: &Cookie, now: DateTime<Utc>) -> CookieEntry` (`max_age` derived, ruling 8).
  - `pub fn build_cookie(scope: &UrlScope, input: &CookieInput, now: DateTime<Utc>) -> DomainResult<Cookie>` (rulings 7 and 8).
  - `pub fn matching_cookies(jars: &[CookieJar], scope: &UrlScope, now: DateTime<Utc>) -> Vec<Cookie>` (wraps `cookies_for_request`).

- [ ] **Step 1: Write the failing tests** in `script_cookie.rs`, with a fixed `now` and an `https://api.example.com/v1/items` scope:
  - `url_scope_parses_host_path_and_scheme` and `url_scope_rejects_a_url_without_a_host` (`"not a url"`, `"file:///tmp/x"`).
  - `build_cookie_without_domain_is_host_only_with_the_default_path` (domain `api.example.com`, path `/v1`).
  - `build_cookie_strips_a_leading_dot_and_rejects_a_domain_that_does_not_match` (`.example.com` ok; `other.com` rejects with `InvalidInput`).
  - `build_cookie_max_age_wins_over_expires_and_is_capped_at_400_days`, and `build_cookie_rejects_max_age_zero_and_negative`.
  - `host_prefixed_cookie_needs_secure_and_no_domain_and_root_path`: `__Host-sid` with a domain rejects; without `secure: true` rejects; with `path: "/x"` rejects; with `secure: true` and no domain succeeds with path `/`.
  - `to_entry_derives_max_age_and_id`: a cookie expiring in 3600 s gives `max_age == Some(3600)`; a session cookie gives `None`; `id == "api.example.com|/v1|sid"`.
  - `cookie_entry_serializes_camel_case_keys` (`httpOnly`, `maxAge`, `sameSite`) and `cookie_input_reads_partial_json` (only `{"key":"a","value":"1"}`).
  - `matching_cookies_applies_domain_path_secure_and_expiry` (an http scope does not get a `secure` cookie; an expired cookie is skipped).
  - IP and localhost: `build_cookie_for_an_ip_host_is_host_only` (`127.0.0.1`, and a `domain` equal to the IP is allowed, a parent-looking domain rejects).
  In `cookie.rs`: `old_jar_yaml_without_same_site_loads_and_saves_without_the_key` (serde_yaml round trip: no `sameSite` text in the output) and `parse_set_cookie_keeps_same_site`.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -j4 -p rocket-http script_cookie`
Expected: FAIL (module and field missing).

- [ ] **Step 3: Implement.** Add the field and fix all construction sites (compile errors list them). Write the module: `build_cookie` reuses `default_cookie_path`, `domain_matches` and `MAX_COOKIE_AGE_SECS` (make that constant `pub(crate)`); `expires` input is RFC 3339 text and an unparsable value rejects with `InvalidInput`; the cookie `expires` field keeps the format the existing parser writes (look at `parse_set_cookie` and copy its format). Add `url` to `rocket-http`'s dependencies if it is not there.

- [ ] **Step 4: Run to verify they pass, plus the crate**

Run: `cargo test -j4 -p rocket-http` then `cargo check -j4 -p rocket-app -p rocket-infra -p rocket-scripting -p rocket-import -p rocket-export`
Expected: PASS and a clean check (construction sites fixed in every crate; if another crate fails to compile, fix it).

- [ ] **Step 5: Commit** (`feat(cookies): add same_site and script cookie helpers`).

---

### Task 2: one jar lock shared by every writer

**Files:**
- Modify: `crates/rocket-http/src/cookie_repository.rs` (add `CookieJarLock`)
- Modify: `crates/rocket-infra/src/cookie_store.rs` (`RepoCookieStore` uses the shared lock)
- Modify: `crates/rocket-infra/src/reqwest_executor.rs` (`with_cookie_repo_and_lock`)
- Modify: `crates/rocket-app/src/cookie_service.rs` (lock on `save` and `clear`)
- Modify: `crates/rocket-app/src/execution_service.rs` (`cookie_lock` field, `with_cookie_lock` builder)
- Modify: `src-tauri/src/lib.rs` (create one lock and pass it to all three)
- Test: `crates/rocket-infra/src/cookie_store.rs`, `crates/rocket-app/src/cookie_service.rs`

**Interfaces:**
- Produces:
  - `#[derive(Clone, Default)] pub struct CookieJarLock(Arc<Mutex<()>>)` in `rocket_http` with `pub fn lock(&self) -> std::sync::MutexGuard<'_, ()>` that recovers a poisoned lock (the lock guards no data).
  - `RepoCookieStore::with_lock(repo: Arc<dyn CookieRepository>, lock: CookieJarLock) -> Self`; `RepoCookieStore::new(repo)` stays and uses `CookieJarLock::default()`.
  - `ReqwestExecutor::with_cookie_repo_and_lock(self, repo: Arc<dyn CookieRepository>, lock: CookieJarLock) -> Self`; `with_cookie_repo(repo)` stays.
  - `CookieService::with_lock(self, lock: CookieJarLock) -> Self`.
  - `RequestExecutionService::with_cookie_lock(self, lock: CookieJarLock) -> Self` and a `pub(crate) fn cookie_lock(&self) -> &CookieJarLock`; the default is a private fresh lock.

- [ ] **Step 1: Write the failing tests.**
  - In `cookie_store.rs` tests (real `FsCookieRepo` in a tempdir): `shared_lock_serializes_a_store_write_and_an_outside_writer`. Two `RepoCookieStore`s over the same dir and the same `CookieJarLock` each call `set_cookies` from 8 threads for different cookie names on one domain. Assert all 16 names are in the jar afterwards. Add `without_a_shared_lock_the_test_setup_can_lose_updates` only if it can be made deterministic; otherwise skip it and say so in the report.
  - In `cookie_service.rs` tests: `save_waits_for_the_shared_lock`: hold `lock.lock()` on the test thread, call `save` from a spawned thread, assert it has not finished after 100 ms, drop the guard, assert it finishes.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -j4 -p rocket-infra shared_lock_serializes` and `cargo test -j4 -p rocket-app save_waits_for_the_shared_lock`
Expected: FAIL (types missing).

- [ ] **Step 3: Implement** the type and the builders. Replace `RepoCookieStore.write_lock: Mutex<()>` with the `CookieJarLock` and use `lock.lock()` where the private lock was taken. In `CookieService`, take the lock for `save` and `clear` (reads stay lock-free). In `src-tauri/src/lib.rs` create `let cookie_lock = rocket_http::CookieJarLock::default();` before the three consumers and pass clones: `CookieService::new(..).with_lock(cookie_lock.clone())`, `ReqwestExecutor::..with_cookie_repo_and_lock(.., cookie_lock.clone())`, and `.with_cookie_lock(cookie_lock.clone())` on the `RequestExecutionService` construction (find it by the `SharedPathCookieRepo` at about line 586).

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -j4 -p rocket-infra cookie` and `cargo test -j4 -p rocket-app cookie` and `cargo check -j4` (the Tauri crate compiles the wiring).
Expected: PASS and a clean check.

- [ ] **Step 5: Commit** (`fix(cookies): share one jar lock between store, service and scripts`).

---

### Task 3: `ScriptCookies`, the script-facing cookie service

**Files:**
- Create: `crates/rocket-app/src/execution_service/script_cookies.rs` (declare it in `execution_service.rs` next to `script_host`)
- Test: same file (use the in-memory repo from `crates/rocket-app/src/test_doubles.rs` or a small local mock, and one `FsCookieRepo` tempdir test for the pinned-repo path)

**Interfaces:**
- Consumes: Task 1 helpers; Task 2 `CookieJarLock` and `RequestExecutionService::cookie_lock()`; the service's existing `cookie_repo: Box<dyn CookieRepository>` field.
- Produces: `pub(crate) struct ScriptCookies<'a> { repo: &'a dyn CookieRepository, lock: &'a CookieJarLock }` with `pub(crate) fn new(repo, lock) -> Self` and, each returning `DomainResult`:
  - `find(&self, scope: &UrlScope, now) -> Vec<Cookie>`
  - `upsert(&self, scope: &UrlScope, input: &CookieInput, now) -> Cookie`
  - `remove(&self, scope: &UrlScope, name: &str, now) -> usize` (removes cookies of that name that would be sent to the URL, ruling per spec section 3)
  - `clear_for_url(&self, scope: &UrlScope, now) -> usize`
  - `clear_all(&self) -> ()`
  Every method takes the lock for its whole read-modify-write, and uses `repo.pinned()` once per call (see `RepoCookieStore::set_cookies`) so one call never spans two workspaces.

- [ ] **Step 1: Write the failing tests:**
  - `upsert_adds_then_replaces_by_name_domain_and_path` (second upsert changes the value, the jar still holds one cookie).
  - `find_returns_only_cookies_that_match_the_url` (other domain, other path, secure on http, expired).
  - `remove_deletes_only_matching_cookies_of_that_name` (same name on another domain survives).
  - `clear_for_url_clears_only_matching_cookies` and `clear_all_empties_every_jar`.
  - `upsert_surfaces_a_build_error_and_writes_nothing` (a `__Host-` cookie with a domain).
  - `operations_hold_the_lock`: hold `lock.lock()` in the test thread, run `upsert` on another thread, assert it waits, then finishes after the guard drops.
  - `upsert_uses_a_pinned_repo`: a mock repo whose `pinned()` returns a second repo; assert the write lands in the pinned one.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -j4 -p rocket-app script_cookies`
Expected: FAIL (module missing).

- [ ] **Step 3: Implement** the struct. Group cookies by their jar domain when saving (a `Cookie.domain` is the key of `CookieJar`; `CookieJar::add` already replaces by name and path). Delete jar entries that become empty only if the existing code does so elsewhere; otherwise leave an empty jar.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -j4 -p rocket-app script_cookies` then `cargo test -j4 -p rocket-app execution_service` and `cargo clippy -j4 -p rocket-app`
Expected: PASS and no warnings.

- [ ] **Step 5: Commit** (`feat(cookies): add the script cookie service`).

---

## Next plan to execute

When all three tasks are complete and the ledger shows "Task 3: complete", move on to `02-engine-host-and-js.md` without asking.
