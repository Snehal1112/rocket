# Create Request — Design Spec

> **Type:** Spec (reference only — never executed directly)
> **Plans:** See `docs/superpowers/plans/2026-03-31-create-request-sp-a-foundation.md`, `sp-b-dialogs.md`, `sp-c-ui-entry-points.md`
> **Source:** Bruno docs — https://docs.usebruno.com/get-started/bruno-basics/create-a-request

---

## Overview

RocketAPI needs parity with Bruno's three request-creation entry points, plus Bruno's custom filename handling. Currently RocketAPI only supports creating requests via an inline text input in the collection tree — no dialog, no unsaved/ephemeral requests, no type selection (HTTP / GraphQL / gRPC / WebSocket), and no filename sanitization.

---

## Feature Goals

| Goal | Priority |
|---|---|
| Create a named request within a collection (with type + method + URL) | Must have |
| Create an unsaved/ephemeral request without touching any collection | Must have |
| Quick inline `+` button on the tab strip (left-click HTTP, right-click picker) | Must have |
| Save an ephemeral request to a collection later | Must have |
| Filesystem-safe filenames with special-char sanitization | Must have |
| Display the filesystem name when it differs from the display name | Must have |
| Warn before closing an unsaved ephemeral tab | Must have |

---

## Three Creation Entry Points

### 1. Within a Collection — `CreateRequestDialog`

**Trigger:** `···` menu on a collection node → "New Request"

**Flow:**
1. Dialog opens with four fields:
   - **Request Type** — `HTTP` / `GraphQL` / `gRPC` / `WebSocket` / `From cURL`
   - **Request Name** — free text; filesystem name shown below when special chars detected
   - **HTTP Method** — shown only for HTTP and cURL types (`GET` default)
   - **URL** — pre-fillable, optional at creation time
2. User clicks **Create**
3. File is saved to disk immediately at `<collection>/<sanitizedName>.yml`
4. Tab opens with full `source` binding (collection + path)
5. Dialog closes and resets

**Edge cases:**
- Empty name → validation error, no save attempted
- Name with special chars (`/`, `[`, `]`, `*`, `:`, etc.) → sanitized filename shown as hint; display name preserved in tab title
- `Enter` key submits the form
- `Escape` / clicking outside → dialog closes, nothing saved

---

### 2. Without a Collection — Ephemeral Request

**Trigger:** `FilePlus` icon button in the sidebar toolbar (collections panel header)

**Flow:**
1. New "Untitled" tab opens immediately — no dialog, no file created on disk
2. Tab has no `source` (not bound to any collection or path)
3. User configures and runs the request freely
4. **Save (optional):** "Save to Collection" button in the request toolbar → opens `SaveToCollectionDialog`
5. **Discard:** Close the tab → close-guard dialog warns data will be lost

**Key behaviour:**
- Ephemeral tabs are in-memory only — closing the app discards them
- Multiple ephemeral tabs can be open simultaneously
- `Cmd+S` on an ephemeral tab opens `SaveToCollectionDialog` instead of auto-saving

---

### 3. Inline `+` Tab Button

**Trigger:** `+` icon button at the end of the open tab strip

**Left-click:** Opens HTTP ephemeral request immediately (same as entry point 2 above)

**Right-click:** Context menu with four options:
- HTTP
- GraphQL
- gRPC
- WebSocket

Each option opens an ephemeral tab with the corresponding `requestType`.

**Behaviour:** No dialog, no file created. Identical to entry point 2 in all other respects.

---

## Save to Collection Flow — `SaveToCollectionDialog`

**Trigger:** "Save to Collection" button in `RequestPanel` toolbar (visible only when `!tab.source`), or `Cmd+S` on an ephemeral tab.

**Dialog fields:**
- **Request Name** — pre-filled from tab title (empty if "Untitled"); filesystem name hint shown when special chars present
- **Save to Collection** — dropdown listing all open collections; includes "+ New Collection" option at the bottom
- **New Collection Name** — text input; shown only when "+ New Collection" is selected or no collections exist

**On Save:**
1. If creating new collection: `createCollection(name)` called first
2. `saveRequest(collectionName, fsName, payload)` called with current tab state
3. Tab's `source`, `title`, and `isDirty` updated in `usePaneStore`
4. Dialog closes

**Edge cases:**
- No collections open → defaults to creating new collection
- Empty request name → validation error
- Empty new-collection name (when creating) → validation error
- Save failure → error message shown inline, dialog stays open

---

## Custom Filename Sanitization

Bruno allows any characters in request display names but maps unsafe filesystem characters to `-` in the stored filename.

**Unsafe characters replaced with `-`:** `/` `\` `:` `*` `?` `"` `<` `>` `|` `[` `]`

**Additional rules:**
- Leading/trailing whitespace stripped before sanitizing
- Consecutive dashes collapsed to single `-`
- Empty result after sanitization → fallback to `"request"`
- Always appended with `.yml` (RocketAPI's file format)

**Example mappings:**

| Display Name | Filename |
|---|---|
| `Get Users` | `Get Users.yml` |
| `GET /users/:id` | `GET -users--id.yml` |
| `items[0]` | `items-0-.yml` |
| `GET /users/:id [v2]*` | `GET -users--id -v2--.yml` |
| `///` | `request.yml` |

**UI hint:** When the filesystem name differs from `<displayName>.yml`, a small hint line appears below the name input: `Saved as: <fsName>`.

---

## Close-Guard for Ephemeral Tabs

Existing behaviour: guard fires only when `tab.isDirty === true`.

New behaviour: guard fires when **either**:
- `tab.isDirty === true` (unsaved edits to a sourced tab), **or**
- `!tab.source` (ephemeral tab — data loss regardless of dirty state)

**Dialog copy:**

| Condition | Title | Description |
|---|---|---|
| `!tab.source` | Unsaved Changes | "This request has never been saved to a collection. Closing it will discard all changes. Close anyway?" |
| `tab.isDirty` | Unsaved Changes | "This request has unsaved changes. Close anyway?" |

---

## Data Model Changes

### `RequestState` — add `requestType`

```ts
// src/types/pane-types.ts
export interface RequestState {
  requestType: 'http' | 'graphql' | 'grpc' | 'websocket'; // NEW — default: 'http'
  method: HttpMethod;
  url: string;
  // ... rest unchanged
}
```

Default in `createDefaultRequest()`:
```ts
requestType: 'http',
```

### `PaneState` — add `openEphemeralTab`

```ts
// src/stores/pane-store.ts
openEphemeralTab: (requestType?: 'http' | 'graphql' | 'grpc' | 'websocket') => void;
```

Opens a `RequestTab` with no `source`, title `"Untitled"`, `isDirty: false`.

---

## Component Map

| Component | Location | Role |
|---|---|---|
| `CreateRequestDialog` | `src/components/request/` | Within-collection creation dialog |
| `SaveToCollectionDialog` | `src/components/request/` | Save ephemeral tab to collection |
| `sanitizeFilename()` | `src/lib/filename-utils.ts` | Pure utility — display name → `.yml` path |
| `openEphemeralTab()` | `src/stores/pane-store.ts` | Store action — open sourceless tab |
| Inline `+` button | `src/components/panes/TabStrip.tsx` | Left-click HTTP, right-click type picker |
| `FilePlus` sidebar button | `src/components/layout/CollectionsSidebar.tsx` | Workspace-level ephemeral request shortcut |
| Close-guard update | `src/components/panes/EditorGroup.tsx` | Guard ephemeral tabs from silent close |

---

## What Is NOT in Scope

- **cURL import parsing** — "From cURL" request type opens the dialog but cURL parsing is a separate feature
- **GraphQL / gRPC / WebSocket editors** — `requestType` is stored but the specialized editors are future work; they open as HTTP tabs for now
- **Folder-scoped creation** — `CreateRequestDialog` accepts an optional `folderPath` prop for future use but folder picker UI is not included
- **Tab state persistence** — ephemeral tabs are in-memory only; persistence across app restarts is deferred to the `.rocket-ui.yml` decision
