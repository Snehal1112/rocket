# Rocket API Client - Comprehensive Code Analysis Report

**Date**: 2026-03-25
**Branch**: feat/ux-workflows
**Version**: 0.1.0

## Project Overview

| Metric | Value |
|--------|-------|
| Total source lines | ~14,300 |
| Rust crate files | 46 |
| Tauri command files | 12 |
| TypeScript/React files | 65 |
| Rust crates | 7 (shared, collection, environment, history, http, app, infra) |
| Rust test functions | 139 across 37 files |
| Frontend test files | 3 |
| Frontend framework | React 19 + Zustand + Radix UI + Tailwind v4 |
| Backend framework | Tauri 2 + reqwest 0.12 |

---

## 1. Security Assessment

### HIGH

**S1. TLS verification fully disableable**
- `reqwest_executor.rs:106` -- `danger_accept_invalid_certs(true)` on per-request basis
- `src-tauri/src/commands/oauth2.rs:87-96` -- WebKit webview TLS errors set to `Ignore` on Linux
- Risk: MITM attacks during OAuth2 flows where tokens and secrets traverse the wire.
- Fix: Allow importing specific CA certs rather than global TLS disable. Add user-visible warning.

**S2. Credentials stored in plaintext on disk**
- `types.rs:151-188` -- `Auth` enum contains `client_secret`, `secret_key`, `password`, `token`
- All auth data serialized directly into JSON files in `~/.rocket-api/`
- No file permission enforcement (relies on default umask)
- Fix: Use OS keyring (`keyring` crate) for secrets. Set 0600 permissions on credential files.

### MEDIUM

**S3. Binary body file read without path validation**
- `reqwest_executor.rs:240-259` -- `std::fs::read(path)` with no sandboxing
- Frontend-controlled `file_path` can read any file, then exfiltrate via HTTP body.
- Fix: Require Tauri file dialog selection. Maintain allow-list of user-selected paths.

**S4. CSP includes `unsafe-eval`**
- `tauri.conf.json:21` -- `script-src 'self' 'unsafe-eval'`
- Required for Monaco editor workers but weakens XSS protection.
- Combined with S7 (OAuth tokens in JS), creates token theft vector.
- Fix: Strip `unsafe-eval` in production or isolate Monaco in a separate webview context.

**S5. Path traversal in environment/history/template/cookie repos**
- `fs_environment_repo.rs:17`, `fs_history_repo.rs:17`, `fs_template_repo.rs:17`
- Direct `dir.join(format!("{}.json", name))` with no sanitization.
- Collection repo has `validate_path` but other repos do not.
- Fix: Extract `validate_path` into a shared utility and apply to all repos.

### LOW

**S6. Cookie values stored unencrypted** -- `fs_cookie_repo.rs:53`
**S7. OAuth2 tokens exposed to frontend JS** -- `oauth2.rs:31`
**S8. AWS SigV4 always signs empty body** -- `reqwest_executor.rs:195` (weakens signature integrity for POST/PUT)

### Positive Security Findings
- Path traversal prevention in collection repo with tests (lines 481-579)
- PKCE implementation follows RFC 7636 correctly
- OAuth2 CSRF state validation present
- Tauri capabilities are minimal (no filesystem/shell permissions)
- No hardcoded secrets in source code
- SSL verification enabled by default
- `.unwrap()` usage in production code limited to safe patterns (HMAC key, Mutex locks, `about:blank` parse)

---

## 2. Performance Assessment

### CRITICAL

**P1. New `reqwest::Client` built per request -- no connection pooling**
- `reqwest_executor.rs:27` -- `Client::builder()...build()` on every invocation
- Discards DNS cache, TCP connections, and TLS sessions.
- `ReqwestExecutor` is a zero-sized struct with no cached client.
- Fix: Store a default `Client` as a field. Rebuild only when SSL/redirect options differ.
- Impact: 50-200ms savings per request to the same origin.

**P2. Blocking filesystem I/O on async runtime threads**
- All repository traits use synchronous `fn` (not `async fn`).
- `std::fs` operations block Tokio worker threads.
- Zero uses of `spawn_blocking` or `tokio::fs` in the codebase.
- Under concurrent requests, this can starve the async runtime.
- Fix: Wrap sync repo calls in `tokio::task::spawn_blocking` or migrate to async.

### MODERATE

**P3. History `list()` reads ALL files, sorts, then truncates**
- `fs_history_repo.rs:22-43` -- full directory scan + N deserializations per call
- `search()` calls `list(None)` to load everything before filtering.
- Degrades linearly as history grows.
- Fix: Maintain an index file or migrate to SQLite. Filter during iteration.

**P4. Response body fully buffered as String, doubled during IPC**
- `reqwest_executor.rs:78-84` -- `bytes().await` then `String::from_utf8_lossy().to_string()`
- No response size limit; 10MB response peaks at ~40MB memory.
- Fix: Add max body size option. Write large bodies to temp files.

**P5. `formatBody` not memoized in ResponseBodyViewer**
- `JSON.parse` + `JSON.stringify` runs on every re-render.
- Fix: Wrap in `useMemo` keyed on `response.body`.

**P6. No Vite manual chunk splitting**
- `vite.config.ts` -- default chunking bundles all vendor libs together.
- Cache invalidation affects entire vendor chunk on any dependency update.
- Fix: Add `manualChunks` for Monaco, Radix, and other large dependencies.

### MINOR

**P7. No environment/collection settings caching** -- read from disk on every request
**P8. `serde_json::to_string_pretty` for persistence** -- 30-40% larger than compact
**P9. Blocking `std::fs::read` for binary bodies in async context** -- `reqwest_executor.rs:242`
**P10. No history size cap** -- unbounded disk growth over time

---

## 3. Architecture Assessment

### Crate Dependency Graph (Clean)

```
rocket-shared          (leaf -- no internal deps)
    ^
    |
rocket-collection, rocket-environment, rocket-history, rocket-http  (domain layer)
    ^
    |
rocket-app  (application services)      rocket-infra  (concrete impls)
    ^                                       ^
    |                                       |
    +----------- src-tauri -----------------+  (composition root)
```

- Dependencies flow inward. Domain crates depend only on `rocket-shared`.
- `rocket-app` and `rocket-infra` are at the same level, not depending on each other. This is correct for Clean Architecture -- the composition root in `src-tauri` handles the wiring.
- All six domain traits have object-safety compile-time tests.
- Services accept `Box<dyn Trait>` enabling proper dependency injection.

### Bugs Found

**A1. `watch_collections` uses `NullEventPublisher` -- events go nowhere (BUG)**
- `src-tauri/src/commands/app.rs:20` -- when triggered from frontend, the file watcher's events are discarded
- The `setup` block in `lib.rs` uses `TauriEventBus`, but `watch_collections` creates its own watcher with `NullEventPublisher`
- Fix: Pass the same `TauriEventBus` to `watch_collections`, or share the watcher instance.

**A2. `onFileChange` listens to `"file-change"` but backend never emits it (BUG)**
- `tauri-api.ts:315` -- dead listener, the backend emits `"collection-changed"` instead
- Fix: Align event names between frontend and backend.

**A3. `queryParams` absent from frontend `ExecuteRequestInput`**
- `tauri-api.ts:161-171` -- the Rust struct has `query_params: Vec<QueryParam>` but the TS type omits it
- Query params are handled via URL string manipulation on the frontend, bypassing the backend's structured support.
- Fix: Add `queryParams` to the TS interface and pass structured params to backend.

**A4. AWS SigV4 frontend sends `{ authType: "none" }` despite full backend support**
- `execute-request.ts:42-43` -- AWS auth is treated as unsupported in the frontend-to-backend conversion
- The full signing implementation in Rust is unreachable from the UI.
- Fix: Wire the `AwsSigV4` auth type through `toApiAuth()`.

### Design Concerns

**A5. Synchronous trait signatures block async evolution**
- All repository traits use synchronous `fn` signatures.
- Forces callers to bridge sync/async gap unsafely.
- Fix: Migrate traits to `async fn` using `async-trait` (already a workspace dependency).

**A6. Duplicate repository instances across services**
- `RequestExecutionService` creates its own `FsEnvironmentRepo`, `FsHistoryRepo`, etc. -- separate from the ones in `EnvironmentService`, `HistoryService`.
- Both instances point to the same filesystem directory.
- Risk: Race conditions under concurrent requests.
- Fix: Share repos via `Arc<dyn Repository>`.

**A7. `CollectionRepository` trait has 14 methods (ISP pressure)**
- Handles collection CRUD, request CRUD, folder CRUD, item moves, and settings.
- Consider splitting into `CollectionRepository` + `RequestRepository` as scope grows.

**A8. No auto-generated TypeScript bindings**
- All TS types in `tauri-api.ts` are hand-written mirrors of Rust structs.
- Risk: Silent desync when Rust types change.
- Fix: Consider `specta` or `ts-rs` for auto-generated bindings.

**A9. Data directory `~/.rocket-api` hardcoded in multiple locations**
- `src-tauri/src/lib.rs:33` and `src-tauri/src/commands/app.rs:9,17`
- Fix: Extract to a single `fn app_data_dir()`.

**A10. Event system fire-and-forget with no subscriber mechanism**
- `EventPublisher::publish` is synchronous and void-returning.
- `NullEventPublisher` is the only non-test implementation visible beyond `TauriEventBus`.
- Some services hold an unused `events` field with `#[allow(dead_code)]`.

---

## 4. Quality Assessment

### Test Coverage
- **Rust**: 139 test functions across 37 files -- strong for v0.1.0. All domain crates tested. Integration tests with mocks in `execution_service.rs`. Security path-traversal tests in `fs_collection_repo.rs`.
- **TypeScript**: Only 3 test files (`pane-utils.test.ts`, `url-params.test.ts`, `pane-store.test.ts`). Major gap: `AuthEditor`, `CollectionsSidebar`, `execute-request.ts`, `auto-save.ts`, `env-store.ts` have zero coverage.

### `.unwrap()` Usage (Clean)
- 212 total occurrences across 22 files.
- ~190+ are in `#[cfg(test)]` blocks -- acceptable.
- Production uses limited to safe patterns: HMAC key creation, Mutex locks, URL literal parsing, `unwrap_or_default()` fallbacks.

### Error Handling
- **Rust**: Excellent. `DomainError` with 7 variants, consistent `DomainResult<T>`, proper `From` impls.
- **TypeScript**: Inconsistent. Multiple `catch (err) { console.error(...) }` blocks in `CollectionsSidebar.tsx` (lines 137, 286, 480, 728, 843, 901) that silently swallow errors with no user feedback.
- `DomainError` serializes to plain string, losing the error variant. Frontend cannot programmatically distinguish "not found" from "internal error" without string parsing.

### Code Smells

**Q1. `CollectionsSidebar.tsx` -- god component (1105 lines)**
- Contains 4 components + all CRUD callbacks in one file.
- `CollectionsSidebar` function alone (lines 699-1105) mixes data fetching, state management, keyboard navigation, and rendering.
- Fix: Extract `RequestNode`, `FolderNode`, `CollectionNode` into separate files. Extract CRUD logic into a custom hook.

**Q2. `AuthEditor.tsx` -- DRY violation in token handling (594 lines)**
- `handleGetToken` (lines 160-189) and `handleRefreshToken` (lines 223-230) duplicate token response parsing logic.
- Fix: Extract shared `parseTokenResponse()` function.

**Q3. `build_folder_tree` writes to disk during a read operation**
- `fs_collection_repo.rs:319-324` -- silently writes uid migration as a side effect of reading.
- Fix: Separate migration into an explicit step or log when it happens.

**Q4. Duplicate `generate_uid` function**
- Defined identically in both `rocket-collection/src/request.rs:4-6` and `rocket-collection/src/folder.rs:4-6`.
- Fix: Consolidate into a single shared location.

**Q5. Repeated `.json` extension normalization**
- Pattern `if path.ends_with(".json") { ... } else { format!("{}.json", path) }` appears 5 times in `fs_collection_repo.rs`.
- Fix: Extract to `fn normalize_json_path(path: &str) -> String`.

**Q6. `auth?: any` type hole**
- `tauri-api.ts:231` -- `settings: { auth?: any; ... }` loses type safety on auth.
- Fix: Use the proper `Auth` type.

**Q7. Verbose mock boilerplate in tests**
- `execution_service.rs:164-320` -- 150+ lines of mock implementations.
- `MockHistoryRepo` appears nearly identically in both `history_service.rs` and `execution_service.rs`.
- Fix: Create a `test-utils` module or use `mockall` crate.

**Q8. No Clippy configuration**
- No `.clippy.toml` or workspace-level `[lints]` section.
- Consider: `#![warn(clippy::pedantic)]` at workspace level.

### Naming Conventions (Consistent)
- Rust: `snake_case` throughout. Repos follow `Fs{Domain}Repo` convention.
- TypeScript: `camelCase`/`PascalCase` consistent. One exception: `OAuth2TokenResponse` uses `snake_case` fields (mirrors raw OAuth2 protocol).
- Serde: consistent `rename_all = "camelCase"` for frontend interop.

---

## Summary: Priority Remediation Roadmap

### Phase 1: Quick Wins (Low effort, high impact)
| # | Finding | Effort |
|---|---------|--------|
| P1 | Reuse `reqwest::Client` (connection pooling) | 1 hour |
| A2 | Fix dead `onFileChange` listener event name | 15 min |
| S5 | Extract shared path validation utility | 2 hours |
| P5 | Add `useMemo` for response body formatting | 30 min |
| P6 | Add Vite manual chunk splitting | 30 min |
| Q4 | Consolidate duplicate `generate_uid` | 15 min |
| Q5 | Extract `normalize_json_path` helper | 30 min |
| Q6 | Fix `auth?: any` type hole | 15 min |

### Phase 2: Bug Fixes and Important Improvements (Medium effort)
| # | Finding | Effort |
|---|---------|--------|
| A1 | Fix `watch_collections` NullEventPublisher bug | 1 hour |
| A3 | Add `queryParams` to frontend ExecuteRequestInput | 2 hours |
| A4 | Wire AWS SigV4 auth through frontend | 2 hours |
| P2 | Wrap blocking FS calls in `spawn_blocking` | 4 hours |
| S3 | Validate binary body file paths | 2 hours |
| S8 | Fix AWS SigV4 body signing | 3 hours |
| Q1 | Split `CollectionsSidebar.tsx` god component | 4 hours |
| Q2 | Extract shared token response parsing | 1 hour |

### Phase 3: Strategic Improvements (Higher effort)
| # | Finding | Effort |
|---|---------|--------|
| A5 | Migrate repository traits to async | 8 hours |
| A6 | Share repo instances via `Arc<dyn Repo>` | 4 hours |
| A8 | Auto-generate TS bindings with `specta`/`ts-rs` | 4 hours |
| S2 | Integrate OS keyring for credentials | 8 hours |
| S1 | Per-domain TLS certificate trust | 6 hours |
| S4 | Isolate Monaco from main CSP context | 4 hours |
| P3 | History index or SQLite migration | 4 hours |
| P4 | Large response body streaming/temp file | 6 hours |
| -- | Add frontend test coverage for critical components | 8 hours |

---

## Health Score: 7.0 / 10

| Domain | Score | Notes |
|--------|-------|-------|
| Security | 6.5 | Good awareness (PKCE, CSRF, path traversal) but plaintext credentials and TLS bypass are significant |
| Performance | 6.0 | No connection pooling and blocking I/O are critical; acceptable for single-user desktop app today |
| Architecture | 8.0 | Clean DDD layering, proper trait abstractions, good dependency direction |
| Quality (Rust) | 8.5 | Strong test coverage, clean error handling, safe unwrap usage |
| Quality (TS) | 6.0 | God component, minimal tests, silently swallowed errors, `any` type leak |
| Overall | **7.0** | Solid foundation for v0.1.0 with clear hardening priorities before wider release |

**Strengths**: Clean DDD architecture with proper dependency inversion, 139 Rust tests across all crates, consistent naming conventions, proper trait abstractions enabling testability and future flexibility.

**Weaknesses**: Blocking I/O on async runtime, no connection pooling, plaintext credentials, inconsistent path validation, 2 bugs (dead event listener, NullEventPublisher in watch_collections), god component on frontend, minimal frontend tests.
