# SP2 Plan 2: Query Parameters + Path Parameters

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a dedicated query parameters editor panel with bidirectional URL sync, and path parameter support (`:id` style extraction from URL).

**Architecture:** Query params are a `KeyValueEntry[]` in the tab's `RequestState`. URL bar and params table are two views of the same data. The Rust backend gains `QueryParam` in `rocket-shared` and the executor merges them into the URL.

**Tech Stack:** React, TypeScript, Zustand, shadcn/ui, Rust (rocket-shared, rocket-http, rocket-infra)

---

## Tasks Overview

### Task 1: Rust — add QueryParam to shared types
- Add `QueryParam { key, value, enabled }` to `rocket-shared/src/types.rs`
- Add `query_params: Vec<QueryParam>` to `HttpRequest` in `rocket-http`
- Update `ReqwestExecutor` to merge query params into URL before execution
- Tests: serialization roundtrip, param merging into URL

### Task 2: Frontend — URL ↔ params sync utilities
- Create `frontend/src/lib/url-params.ts`
- `parseQueryParams(url) → KeyValueEntry[]`
- `buildUrl(baseUrl, params) → string`
- `extractPathParams(url) → string[]` (finds `:id`, `{id}` patterns)
- Tests: parse, build, roundtrip, edge cases (empty values, encoded chars, duplicate keys)

### Task 3: Frontend — Query params editor panel
- Create `frontend/src/components/request/QueryParamsEditor.tsx`
- Key-value table with: key input, value input, enabled toggle, delete button, add row
- Wired to `usePaneStore.updateRequest(tabId, { queryParams })`
- URL bar updates trigger param reparse (debounced 300ms)
- Param edits immediately rebuild URL

### Task 4: Frontend — Path params panel
- Extract `:param` and `{param}` patterns from URL
- Show read-only param name + editable value field
- Editing value replaces the placeholder in the URL

### Task 5: Wire into request panel tabs
- Add "Params" tab to request panel (alongside Headers, Body, Auth)
- Show query param count badge on tab

---

# SP2 Plan 3: OAuth 2.0 + AWS Signature v4 Auth

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add OAuth 2.0 (authorization code, client credentials, password grant) and AWS Signature v4 authentication to the HTTP execution engine.

**Architecture:** New `Auth` enum variants in `rocket-shared`. OAuth 2.0 token flow logic in `rocket-http/src/oauth2.rs`. AWS signing in `rocket-http/src/aws_sig.rs`. Infra layer applies auth before sending request.

**Tech Stack:** Rust (reqwest, sha2, hmac, chrono), React, TypeScript, shadcn/ui

---

## Tasks Overview

### Task 1: Rust — extend Auth enum
- Add `OAuth2 { grant_type, client_id, client_secret, token_url, scope, access_token, refresh_token, expires_at }` variant
- Add `AwsSigV4 { access_key, secret_key, region, service, session_token }` variant
- Update serde tagged serialization
- Tests: serialization roundtrip for new variants

### Task 2: Rust — OAuth 2.0 token acquisition
- Create `crates/rocket-http/src/oauth2.rs`
- `OAuthConfig` struct with grant type, URLs, credentials
- `async fn acquire_token(config, executor) → OAuthToken` — calls token endpoint
- `fn is_expired(token) → bool`
- Support: `client_credentials`, `password`, `authorization_code` (with PKCE)
- Tests: token parsing, expiry check (unit tests with mock responses)

### Task 3: Rust — AWS Signature v4 signing
- Create `crates/rocket-http/src/aws_sig.rs`
- `fn sign_request(request, credentials, region, service) → signed_headers`
- Implements: canonical request → string to sign → signing key → signature → Authorization header
- Add `hmac` and `hex` dependencies to `rocket-http`
- Tests: test against AWS's published test vectors

### Task 4: Rust — update executor for new auth types
- `ReqwestExecutor` handles `Auth::OAuth2` → acquire/refresh token, attach as Bearer
- `ReqwestExecutor` handles `Auth::AwsSigV4` → sign request, attach headers
- Integration test (ignored): real OAuth flow against a test provider

### Task 5: Frontend — OAuth 2.0 config UI
- New auth tab panel: grant type selector, client ID/secret fields, token URL, scope
- "Get Token" button triggers token acquisition via Tauri invoke
- Displays current token, expiry, refresh button
- shadcn form components

### Task 6: Frontend — AWS Sig v4 config UI
- New auth tab panel: access key, secret key, region, service fields
- Session token (optional) for temporary credentials

---

# SP2 Plan 4: Binary Body + Collection-Level Inheritance

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Support binary file upload in request body, and add collection-level auth/headers that requests inherit.

**Architecture:** Binary body uses Tauri's file dialog to pick a file, then sends the file path to Rust which streams it. Collection settings are stored as `collection.json` in the collection root directory.

**Tech Stack:** Rust (reqwest, tauri-plugin-dialog), React, TypeScript, shadcn/ui

---

## Tasks Overview

### Task 1: Rust — binary body support
- Add `Binary { file_path }` variant to `BodyMode`
- `ReqwestExecutor`: read file from path, stream as request body
- Set Content-Type from file extension or `application/octet-stream`
- Tests: binary body construction (unit), file streaming (integration with tempfile)

### Task 2: Frontend — binary body picker
- When body mode = "binary", show file picker button (uses `@tauri-apps/plugin-dialog`)
- Display selected file name + size
- "Clear" button to remove

### Task 3: Rust — CollectionSettings model
- Create `CollectionSettings { auth: Option<Auth>, headers: Vec<Header> }` in `rocket-collection`
- Add to `Collection` aggregate
- `CollectionRepository` trait: `get_settings(name)`, `save_settings(name, settings)`
- `FsCollectionRepo`: reads/writes `collection.json` in collection root
- Tests: save/load settings roundtrip

### Task 4: Rust — merge collection settings in execution
- `RequestExecutionService.execute()` loads collection settings if `source.collection` is set
- Merge logic: request auth overrides collection auth (if not `None`); headers merge (request headers take precedence by key)
- Tests: merge logic with mock repos

### Task 5: Frontend — collection settings UI
- Collection context menu → "Settings" opens a dialog
- Auth tab: configure collection-level auth (same components as request auth)
- Headers tab: add default headers for all requests in collection
- Visual indicator on requests that inherit auth: "(inherited)" label

---

# SP2 Plan 5: Keyboard Shortcuts + Response Views + History Search

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add keyboard shortcuts, pretty-print response body with syntax highlighting, HTML preview, and history search/filter.

**Architecture:** Keyboard shortcuts are a React hook registering global key listeners. Response views are tab-switchable panels in the response area. History search adds a filter VO to the domain and a search UI.

**Tech Stack:** React, TypeScript, Zustand, shadcn/ui, Rust (rocket-history)

---

## Tasks Overview

### Task 1: Frontend — keyboard shortcuts hook
- Create `frontend/src/hooks/useKeyboardShortcuts.ts`
- Shortcuts:
  - `Cmd/Ctrl+Enter` → send request (active tab)
  - `Cmd/Ctrl+N` → new draft tab
  - `Cmd/Ctrl+W` → close active tab
  - `Cmd/Ctrl+S` → save request to collection
  - `Cmd/Ctrl+E` → toggle environment selector
  - `Cmd/Ctrl+,` → open settings
  - `Cmd/Ctrl+1-9` → switch to tab by index
  - `Cmd/Ctrl+Tab` → next tab in group
  - `Cmd/Ctrl+Shift+Tab` → previous tab in group
- Platform detection: `navigator.platform` for Mac vs Windows/Linux
- Register in `MainLayout`, unregister on unmount

### Task 2: Frontend — response pretty-print
- Create `frontend/src/components/response/ResponseBodyViewer.tsx`
- Views: Pretty (JSON), Raw, Preview (HTML), Headers
- Pretty JSON: parse → `JSON.stringify(null, 2)` + lightweight syntax highlighting
  - Use simple regex-based tokenizer (no heavy code editor dependency)
  - Color tokens: strings=green, numbers=amber, booleans=blue, keys=purple, null=gray
- XML: format with indentation
- HTML preview: render in sandboxed `<iframe srcDoc={body} sandbox="" />`
- Tab switcher in response panel header

### Task 3: Frontend — response headers table
- Key-value table showing response headers
- Searchable/filterable
- Copy header value on click

### Task 4: Rust — history search support
- Add `HistoryFilter` VO to `rocket-history`:
  ```rust
  pub struct HistoryFilter {
      pub method: Option<String>,
      pub url_contains: Option<String>,
      pub status_min: Option<u16>,
      pub status_max: Option<u16>,
      pub after: Option<DateTime<Utc>>,
      pub before: Option<DateTime<Utc>>,
  }
  ```
- Add `fn search(&self, filter: &HistoryFilter) → DomainResult<Vec<HistoryEntry>>` to `HistoryRepository` trait
- Implement in `FsHistoryRepo`: load all → filter in memory
- Add `search_history` Tauri command
- Tests: filter by method, status range, URL pattern

### Task 5: Frontend — history search UI
- Search bar at top of history panel
- Filter dropdowns: method (GET/POST/...), status (2xx/3xx/4xx/5xx), date range
- Results update as you type (debounced)
- Click history entry → opens in new tab (read-only)

---

## Milestone Checklist — SP2 Complete

- [ ] **Plan 0:** All 6 SP1 bugs fixed
- [ ] **Plan 1:** VS Code-style tabs with bidirectional split panes
- [ ] **Plan 2:** Query params editor with URL sync
- [ ] **Plan 3:** OAuth 2.0 + AWS Sig v4 auth working end-to-end
- [ ] **Plan 4:** Binary body upload + collection-level inherited settings
- [ ] **Plan 5:** Keyboard shortcuts, response pretty-print, history search
- [ ] All tests pass: `cargo test --workspace` + `npx vitest run`
- [ ] No clippy warnings
- [ ] All features smoke-tested in `cargo tauri dev`
