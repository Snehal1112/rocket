# SP2: Core Feature Completion — Design Spec

**Date:** 2026-03-24
**Status:** Approved
**Depends on:** SP1 (Tauri migration + DDD architecture)

## Goal

Complete the core feature set to make RocketAPI a daily-driver API testing tool. Fix SP1 frontend wiring bugs, add VS Code-style tabs with split panes, query parameter editor, OAuth 2.0 + AWS Sig v4 auth, binary body uploads, collection-level inherited settings, keyboard shortcuts, response pretty-printing, and history search/filter.

## Architecture Decisions

### Tab/Pane System
- **Data model:** Recursive binary tree (`PaneNode`) stored in Zustand
- Each node is either a `Split` (direction + two children + size ratio) or a `Leaf` (editor group with tabs array)
- Each `Tab` owns a full `RequestState` + `ResponseState` pair (Postman model)
- Splitting replaces a leaf with a split node; closing last tab collapses the group and simplifies the tree
- Tab state includes `isDirty` flag for unsaved change tracking
- Tabs can be `request` (from collection), `draft` (unsaved), or `history` (read-only replay)

### URL ↔ Query Params Sync
- Source of truth: `queryParams: KeyValueEntry[]` array in tab state
- URL bar is a derived view: editing URL parses query string → updates params; editing params rebuilds URL
- Debounce URL parsing at 300ms; params-to-URL is immediate
- Toggling `enabled` on a param adds/removes it from URL without deleting the row

### Auth Extensions
- `Auth` enum in `rocket-shared` gains two new variants: `OAuth2` and `AwsSigV4`
- OAuth 2.0: domain logic in `rocket-http/src/oauth2.rs`, supports authorization_code, client_credentials, password grants. Token caching in memory.
- AWS Sig v4: pure signing algorithm in `rocket-http/src/aws_sig.rs`, no AWS SDK dependency. Computes signed headers from request + credentials.

### Collection-Level Settings
- New `CollectionSettings` VO: `{ auth: Option<Auth>, headers: Vec<Header> }`
- Stored as `collection.json` in collection root directory
- Execution service merges collection settings → request settings (request overrides collection)

### Keyboard Shortcuts
- Frontend-only (React hook), no Rust changes
- Cmd/Ctrl+Enter: send request. Cmd/Ctrl+N: new tab. Cmd/Ctrl+W: close tab. Cmd/Ctrl+S: save request. Cmd/Ctrl+Shift+P: command palette (future).

### Response Improvements
- Pretty JSON: `JSON.stringify(parsed, null, 2)` with syntax highlighting (via lightweight tokenizer, no heavy editor dependency)
- HTML preview: sandboxed iframe render
- Raw: plain text view
- Headers: key-value table

### History Search
- New `HistoryFilter` VO: method, status range, URL pattern, date range
- `HistoryRepository` trait gains `search(query, filters)` method
- FsHistoryRepo implements via scan-and-filter (acceptable for local file count)

## Chunk Breakdown

| Chunk | What | Est. |
|---|---|---|
| 0 | SP1 bug fixes (sidebar, history, env, watcher, build, archive) | 3-4 days |
| 1 | Tab system + split panes | 5-7 days |
| 2 | Query params + path params | 3-4 days |
| 3 | OAuth 2.0 + AWS Sig v4 auth | 5-6 days |
| 4 | Binary body + collection inheritance | 4-5 days |
| 5 | Keyboard shortcuts + response views + history search | 4-5 days |

## Frontend Stack
- shadcn/ui components (init: `yarn dlx shadcn@latest init --preset b2CkJ2CsV --template vite --monorepo`)
- Zustand for state management
- React 18 + TypeScript
- Tauri 2.0 IPC via `invoke()` / `listen()`
