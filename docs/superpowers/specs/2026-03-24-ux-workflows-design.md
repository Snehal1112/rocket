# UX Workflows: Environments, Collection CRUD, Stub Wiring — Design Spec

**Date:** 2026-03-24
**Status:** Approved
**Depends on:** SP2 (Core Feature Completion)

## Goal

Complete the essential UX workflows that make Rocket a daily-driver API client. Three feature groups delivered sequentially: (1) wire existing UI stubs, (2) environment variable system, (3) collection CRUD with auto-save.

## Decisions

- **Environment switcher:** Header bar dropdown (Postman-style), always visible
- **Environment editing:** Dialog/modal with key-value editor
- **Request saving:** Auto-save for collection-owned requests (Bruno-style)
- **Collection management:** Right-click context menus + hover action icons (both)
- **Delivery order:** Stubs first, then environments, then collection CRUD

---

## Feature 1: Wire Up Stubs

Four targeted fixes to connect existing UI elements to backend functionality.

### 1.1 — "New Collection" Button

- Currently: `Button` in `CollectionsSidebar.tsx:309` with no click handler
- Fix: Click replaces the button with an inline `Input` field
- Enter confirms: calls `createCollection(name)` from `tauri-api.ts`
- `onCollectionChanged` listener already refreshes the sidebar on success
- Escape or blur cancels, restores the button
- **Validation:** Empty name is ignored (same as Escape). Duplicate name shows an inline error toast. Name validation rejects characters invalid for filesystem paths (`/`, `\`, `:`, `*`, `?`, `"`, `<`, `>`, `|`)

### 1.2 — Collection Settings Save

- Currently: `handleSave()` in `CollectionSettingsDialog.tsx` logs to console
- Fix: Wire to Tauri command `save_collection_settings(collection_name, settings)`
- The `settings` payload matches the `CollectionSettings` VO defined in the SP2 spec: `{ auth: Option<Auth>, headers: Vec<Header> }`. Serialize as JSON to the Tauri command.
- Route: `src-tauri` command -> `CollectionService` -> `CollectionRepository`
- If the Tauri command doesn't exist yet, register it and connect to `CollectionService`
- Close dialog on success

### 1.3 — OAuth 2.0 "Get Token" Button

- Currently: `onClick` handler is empty in `AuthEditor.tsx`
- Fix: Frontend-orchestrated flow using existing `executeRequest()`
- The token fetch request is constructed as a standalone HTTP request with `auth: { authType: "none" }` — the OAuth token endpoint itself does not use the app's configured auth
- Build a token request from the OAuth fields (token URL, client ID, client secret, grant type)
- For `client_credentials`: POST with `grant_type=client_credentials` + client credentials as form-encoded body (or Basic auth header per RFC 6749)
- For `password`: POST with `grant_type=password` + username + password in form-encoded body
- **`authorization_code` is out of scope for this spec.** It requires browser-based redirect handling (authorization URL, PKCE, redirect capture) which is a separate feature. The UI should disable the "Get Token" button when `authorization_code` grant is selected, with a tooltip: "Authorization code flow coming soon."
- Parse JSON response for `access_token`, `refresh_token`, `expires_in`
- Store token in the tab's auth state (`request.auth.accessToken`)
- Show error in a toast/inline message if token fetch fails
- **Note:** The `Auth` type in `tauri-api.ts` currently only has `none`, `basic`, `bearer`, `api-key`. The `AuthState` in `pane-types.ts` already includes `oauth2` and `aws-sig-v4`. The bridge type in `tauri-api.ts` must be extended to match, or the execution service must translate `oauth2` auth into a bearer token header before sending.

### 1.4 — Cmd+Enter to Send Request

- Currently: `useKeyboardShortcuts.ts` captures the combo but logs a TODO
- Fix: Read active tab from pane store, extract its request state
- Call the same execution path as `RequestPanel.tsx`'s Send button
- Requires extracting the send logic into a shared function (or a new `useExecuteRequest` hook) that both the button and the shortcut can call

---

## Feature 2: Environment System

### 2.1 — New Zustand Store: `env-store.ts`

```typescript
interface EnvState {
  environments: Environment[];
  activeEnvId: string | null;

  loadEnvironments: () => Promise<void>;
  setActiveEnv: (id: string | null) => void;
  createEnvironment: (name: string) => Promise<void>;
  updateEnvironment: (env: Environment) => Promise<void>;
  deleteEnvironment: (name: string) => Promise<void>;
  getActiveVariables: () => Record<string, string>;
  resolveVariables: (text: string) => string;
}
```

- `loadEnvironments()` calls `listEnvironments()` from tauri-api on app mount
- Both `createEnvironment` and `updateEnvironment` delegate to `saveEnvironment()` from tauri-api. `createEnvironment(name)` constructs a default `Environment` object with `{ name, variables: [] }` and calls `saveEnvironment(env)`.
- `resolveVariables(text)` replaces `{{key}}` patterns using active environment's variable map
- Variables with `enabled: false` are excluded from resolution

### 2.2 — Header Bar Environment Switcher

- New component: `src/components/layout/EnvironmentSwitcher.tsx`
- Placed in `Header.tsx` between the logo and the theme toggle
- Visual: colored dot (green=active, gray=none) + environment name as a pill + dropdown caret
- Uses shadcn `DropdownMenu`
- Menu items:
  - "No Environment" (deselects active)
  - Separator
  - List of all environments (radio-style, checkmark on active)
  - Separator
  - "Manage Environments..." (opens dialog)

### 2.3 — Environment Management Dialog

- New component: `src/components/environments/EnvironmentDialog.tsx`
- shadcn `Dialog`, two-panel layout:
  - **Left panel (200px):** Environment list with add (Plus icon) and delete (Trash2 icon) buttons. Click to select.
  - **Right panel:** Key-value editor for selected environment's variables
- Key-value rows follow the same pattern as `QueryParamsEditor`:
  - Checkbox (enabled/disable), Key input, Value input, Secret toggle (eye icon), Delete button
- Secret toggle masks the value display (shows dots) and sets `secret: true` on the `Variable` (matching the existing `Variable` type in tauri-api.ts)
- Auto-save with 500ms debounce: each edit calls `updateEnvironment()` via tauri-api
- New environment: prompts for name, creates with empty variables array

### 2.4 — Variable Substitution

- Applied in the frontend before calling `executeRequest()`
- Resolution targets: URL, header values, query param values, body text, auth field values (token URL, username, password, API key value, etc.)
- `resolveVariables(text)` does regex replacement: `/\{\{([\w.-]+)\}\}/g` (supports alphanumeric, underscore, hyphen, and dot in variable names)
- Unresolved variables (no matching key in active env) are left as `{{key}}` literal
- No visual highlighting in URL bar for v1 (plain text input). Future enhancement can add token coloring.

### 2.5 — Rust Backend Verification

- `tauri-api.ts` already has typed functions: `listEnvironments`, `getEnvironment`, `saveEnvironment`, `deleteEnvironment`
- Verify these Tauri commands are registered in `src-tauri/src/main.rs` (or lib.rs)
- Verify `EnvironmentService` in `rocket-app` is wired up with a working `EnvironmentRepository` implementation in `rocket-infra`
- If any command is missing, register it following the pattern of existing collection commands

---

## Feature 3: Collection CRUD + Auto-Save

### 3.1 — Sidebar Hover Action Icons

Added to existing sidebar node components (`CollectionNode`, `FolderNode`, `RequestNode`):

| Node Type | Hover Icons |
|-----------|-------------|
| Collection | New Request (Plus), New Folder (FolderPlus), Settings (Settings) |
| Folder | New Request (Plus), New Folder (FolderPlus) |
| Request | Duplicate (Copy), Delete (Trash2) |

- Icons: 14px, `text-muted-foreground`, appear on the right side of the row on hover
- Use Tailwind `group/hover` pattern: parent has `group`, icons have `opacity-0 group-hover:opacity-100`
- Icons are `lucide-react` (already installed)

### 3.2 — Right-Click Context Menus

Uses shadcn `ContextMenu` (already installed).

**Collection:**
- New Request
- New Folder
- Rename
- Delete (with `AlertDialog` confirmation)
- Settings (opens `CollectionSettingsDialog`)

**Folder:**
- New Request
- New Folder
- Rename
- Delete (with confirmation)

**Request:**
- Duplicate
- Rename
- Move to... (sub-menu listing collections, each expandable to show top-level folders only; disabled if no collections exist)
- Delete (with confirmation)

### 3.3 — Inline Rename

- Rename (from context menu or F2 shortcut) replaces the label with an `Input`
- Enter confirms: calls appropriate tauri-api function (`renameCollection` or file-level rename via `moveItem`)
- Escape cancels
- Input is auto-focused and pre-filled with current name, text selected

### 3.4 — New Request Flow

- "New Request" from hover icon or context menu:
  1. Creates a new request file in the collection/folder via `saveRequest(collection, path, defaultRequest)`
  2. Opens it as a tab with `source: { collection, path }`
  3. Tab title shows the request name
  4. Inline rename activates immediately so user can name it

### 3.5 — Auto-Save for Collection Requests

- Trigger: any change to request state for tabs where `tab.source` is defined
- Implementation: side effect in pane-store's `updateRequest` action
- Debounced at 500ms using a module-level debounce map keyed by `tabId`
- Calls `saveRequest(source.collection, source.path, request)` from tauri-api
- **Cleanup:** When a tab is closed, cancel any pending debounce timer and remove the entry from the debounce map
- No dirty indicator shown for collection requests (they're always saved)
- Draft tabs (no `source`) keep the existing `isDirty` flag behavior

### 3.6 — Save Draft to Collection Dialog

- New component: `src/components/collections/SaveToCollectionDialog.tsx`
- Triggered by Cmd+S on a draft tab
- shadcn `Dialog` showing:
  - Collection tree as a picker (reuse sidebar tree rendering)
  - Folder selection within the chosen collection
  - Request name input (pre-filled from tab title or URL path)
- On confirm:
  - Calls `saveRequest(collection, folder/name, request)`
  - Updates tab: `tabType: 'request'`, `source: { collection, path }`, `isDirty: false`
  - Future edits auto-save

### 3.7 — Delete Confirmation

- Uses shadcn `AlertDialog` (already installed)
- Collection delete: "Delete collection '{name}'? This removes all requests inside it."
- Folder delete: "Delete folder '{name}' and all requests inside it?" Close all tabs whose `source.path` starts with the deleted folder's path.
- Request delete: "Delete request '{name}'?"
- On confirm: calls `deleteCollection()`, `deleteFolder()`, or `deleteRequest()` via tauri-api
- If the deleted item (or any contained item) is open in a tab, close that tab automatically
- Sidebar refreshes via existing `onCollectionChanged` listener

---

## Files to Create/Modify

### New Files
- `src/stores/env-store.ts` — environment Zustand store
- `src/components/layout/EnvironmentSwitcher.tsx` — header bar dropdown
- `src/components/environments/EnvironmentDialog.tsx` — environment CRUD dialog
- `src/components/collections/SaveToCollectionDialog.tsx` — save draft to collection
- `src/hooks/useExecuteRequest.ts` — shared request execution logic (for Send button + Cmd+Enter)

### Modified Files
- `src/components/layout/Header.tsx` — add EnvironmentSwitcher
- `src/components/layout/CollectionsSidebar.tsx` — hover icons, context menus, inline rename, new collection handler
- `src/components/collections/CollectionSettingsDialog.tsx` — wire handleSave
- `src/components/request/AuthEditor.tsx` — wire OAuth "Get Token"
- `src/components/request/RequestPanel.tsx` — extract send logic to shared hook
- `src/hooks/useKeyboardShortcuts.ts` — wire Cmd+Enter, add Cmd+S for save
- `src/stores/pane-store.ts` — auto-save side effect in updateRequest

### Potentially Modified (Rust)
- `src-tauri/src/lib.rs` or `src-tauri/src/main.rs` — register missing Tauri commands
- `crates/rocket-app/src/collection_service.rs` — add `save_collection_settings` if missing
- `crates/rocket-infra/` — verify environment repository implementation

## Non-Goals

- Drag-and-drop reordering in sidebar (future enhancement)
- Import/export (Postman, OpenAPI, cURL) — separate feature
- URL bar variable highlighting — future enhancement
- Environment variable autocomplete in editors — future enhancement
