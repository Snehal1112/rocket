# rok JS API parity, sub-project C: cookies

Date: 2026-10-07

Series: see `.claude/rok-api-parity-notes.md`. Depends on part B (`2026-10-07-rok-js-api-parity-b-async-design.md`): C uses B's `ScriptHost` trait, so it is implemented after B.

## Goal

`rok.cookies.*` (request-scoped) and `rok.cookies.jar()` work as on Bruno's JavaScript API Reference, with `bru` replaced by `rok`, in pre-request, post-response and test scripts.

## Current state

- `CookieRepository` (`crates/rocket-http/src/cookie_repository.rs`) with `FsCookieRepo` in `rocket-infra` (`~/.rocket-api/cookies/`, one jar per domain).
- `RepoCookieStore` (`crates/rocket-infra/src/cookie_store.rs`) stores and sends cookies during requests.
- `rocket_http::cookie` already has `cookies_for_request`, `parse_set_cookie`, `domain_matches`, `path_matches`, `is_expired`, `cookie_header`, `default_cookie_path`.
- `CookieService` in `rocket-app` exposes `get_all`, `get_by_domain`, `save`, `clear`.
- `Cookie` has no `sameSite` or `maxAge`. Scripts cannot reach the jar.

## Decisions

- One global jar, as today. Per-workspace or per-collection jars are out of scope.
- Available in Safe mode, consistent with B. Same exposure: `jar()` can read session cookies for any domain. See Risks.
- `sameSite` is stored and round-tripped, not enforced.
- The cookie-jar management UI is out of scope.

## Design

### 1. Surfaces

- Request-scoped `rok.cookies.*`: bound to the final request URL after pre-request mutation.
- `rok.cookies.jar()`: explicit URL on each call.

### 2. Reads and writes

- Request-scoped reads are synchronous, served from a snapshot in `ScriptContext`: the cookies matching the request URL, taken when the script starts. Methods: `get`, `has(name[, value])`, `one(id)`, `all`, `count`, `idx(n)`, `indexOf`, `toObject`, `toString` (Cookie-header format), `each`, `find`, `filter`, `map`, `reduce`.
- Request-scoped writes are async: `add`, `upsert`, `remove`, `delete`, `clear`. Each goes to the host straight away, so a request sent later in the same script, or the real request after a pre-request script, sees it. Each also updates the local snapshot, so a following `rok.cookies.get` is consistent.
- `jar()` methods are async host ops: `getCookie`, `getCookies`, `hasCookie` (optional callback `(error, exists)`), `setCookie` (`(url, name, value)` or `(url, object)`), `setCookies`, `deleteCookie`, `deleteCookies`, `clear`.

### 3. Semantics

- Entry shape: `{ id, key, value, domain, path, secure, httpOnly, expires, maxAge, sameSite }`. `key` is the cookie name (Bruno naming). `id` is `domain|path|name`.
- Identity: name, domain and path. `add` and `upsert` both replace an existing cookie.
- Defaults: no `domain` gives a host-only cookie on the URL host. No `path` uses the RFC 6265 default path.
- `maxAge` converts to `expires`, capped at 400 days like the existing parser.
- `__Host-` cookies: `domain` must be omitted and `secure` is required, otherwise the call rejects.
- `remove` and `delete`: remove cookies of that name that would be sent to the URL.
- `clear()`: request-scoped clears only cookies matching the URL. `jar.clear()` clears the whole jar.
- `getCookie` returns the entry or `null`. Request-scoped `get` returns the value or `undefined`.
- Cookie values are not added to the redaction list.

### 4. Backend changes

- `Cookie` gains `same_site: Option<String>` with a serde default and skip-if-none, so existing jar files load unchanged. All struct-literal construction sites are updated.
- `ScriptContext` gains the scoped URL and the matching cookie snapshot.
- `ScriptHost` (from B) gains cookie operations: find-for-url, upsert, remove, clear. `CookieService` gets matching script-facing methods that reuse the `rocket-http` matching functions and run the repo read-modify-write under one lock, as `RepoCookieStore` does.
- Typings in `rok-types.ts`, JS wrappers in `bootstrap.js`, IntelliSense and snippet entries.
- Domain logic stays in `rocket-http` and `rocket-app`. No I/O in domain crates. Production paths return `DomainResult` errors instead of panicking.

## Risks

- In Safe mode a script can read cookies for any domain and send them out with `sendRequest`. This follows from the B ruling. Mitigation to consider later: surface script-originated requests in History and flag `rok.cookies.jar` in the scanner.
- `jar.clear()` is destructive. Kept for Bruno parity.
- Concurrent writes from a script and a running request. Mitigated by the shared write lock. The plan must confirm `CookieService` and `RepoCookieStore` use the same lock or the same repo instance.

## Testing

- Unit: URL scoping and matching helpers, `maxAge` conversion, `__Host-` rules, entry shape, with `tempfile` repos.
- Engine tests with a fake host: sync reads, write-then-read, callback form of `hasCookie`, error rejection.
- `wiremock` integration: a pre-request script adds a cookie and the real request carries it. A `Set-Cookie` from a response is visible to the post-response script.
- Backward compatibility: an old jar file without `same_site` still loads and saves.
- Verification: `cargo check -j4`, targeted `-j4` tests for `rocket-http`, `rocket-app`, `rocket-infra`, `rocket-scripting`, then `yarn tsc --noEmit` and `yarn check`.

## Required reading for implementation

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. Also `.claude/script-files.md` and `docs/superpowers/specs/2026-10-07-js-script-security-design.md`.
