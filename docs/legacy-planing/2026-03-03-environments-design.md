# Environments Feature Design

**Date:** 2026-03-03
**Status:** Approved
**Author:** Claude (brainstormed with user)

---

## Goal

Implement a full environments feature — create, edit, delete, and activate environments per collection — matching the Bruno/Postman workflow. Environments let users define `{{variableName}}` substitutions so the same request can be sent against dev, staging, or prod without editing URLs manually.

---

## Approach

Replace the existing stubbed `EnvironmentsPanel` right-sidebar with:
1. A **Bruno-style modal** (`EnvironmentsDialog`) for managing environments.
2. An **inline env selector** in the `RequestBuilder` URL bar area.

---

## Backend Changes

### Secret variable storage

Two files per environment inside `collections/{name}/environments/`:

| File | Contents | Git |
|------|----------|-----|
| `name.env` | Non-secret `KEY=VALUE` pairs | Committed |
| `name.env.secret` | Secret `KEY=VALUE` pairs | Gitignored |

`ListEnvironments` discovers environments by scanning for `.env` files (unchanged). The `.env.secret` companion is read/written alongside.

### Updated API response shape

All environment endpoints return/accept variables as an array of objects:

```json
{
  "name": "dev",
  "variables": [
    { "key": "baseUrl", "value": "http://localhost:3000", "enabled": true, "secret": false },
    { "key": "token",   "value": "abc123",                "enabled": true, "secret": true }
  ]
}
```

On write, the backend splits the array: non-secret enabled vars → `.env`, secret enabled vars → `.env.secret`. Disabled vars are omitted from both files (Bruno convention).

### New endpoint

| Method | Path | Description |
|--------|------|-------------|
| `DELETE` | `/api/v1/collections/{name}/environments/{envName}` | Deletes `.env` + `.env.secret` |

Existing endpoints (`GET` list, `GET` single, `POST` save) are updated to handle the new shape.

---

## Frontend Type Changes

```typescript
// types/index.ts
interface EnvironmentVariable {
  key: string
  value: string
  enabled: boolean
  secret: boolean   // added
}
```

---

## Frontend State (`store/collections.ts`)

Three new actions added to the collections store:

- **`createEnvironment(collectionName, name)`** — POST empty env to API, refresh list, set as active.
- **`deleteEnvironment(collectionName, name)`** — DELETE via API, clear `activeEnvironment` if it was the deleted env, refresh list.
- **`saveEnvironment(collectionName, env)`** — POST full env object (replaces the current `console.log` stub).

### Active environment persistence

`setActiveEnvironment` writes to `localStorage`:
```
key:   rocket-api:active-env:{collectionName}
value: "{envName}"  (or removed if null)
```

When `setActiveCollection` fires, the store reads localStorage and auto-restores the last-used environment for that collection.

---

## UI Components

### 1. `EnvironmentsDialog` (new)

**File:** `frontend/src/components/collections/EnvironmentsDialog.tsx`

A shadcn `Dialog` (not a sidebar). Two-column layout:

```
┌─ Manage Environments ──────────────────────────────┐
│ ┌────────────┐ ┌──────────────────────────────────┐ │
│ │ dev     [×]│ │ KEY         VALUE          S  EN │ │
│ │ staging [×]│ │ baseUrl     http://api     □  [✓]│ │
│ │ prod    [×]│ │ token       ••••••         [✓][✓]│ │
│ │            │ │                                   │ │
│ │ [+ New]    │ │ [+ Add Variable]                  │ │
│ └────────────┘ └──────────────────────────────────┘ │
│                                          [Save]      │
└────────────────────────────────────────────────────┘
```

- Left column: scrollable env list, each row has name + delete-on-hover button. Clicking a row selects it for editing.
- New environment: inline text input at the bottom of the left column (no `prompt()`).
- Right column: variable table. Columns: enabled checkbox, KEY input, VALUE input (masked/`type="password"` when `secret=true`), secret toggle icon, delete button.
- Footer: Save button (disabled when no env selected or no changes).
- Dialog opened via "Manage" icon button in the RequestBuilder.

### 2. `RequestBuilder.tsx` — env selector in URL bar

Added to the right side of the URL bar wrapper div (alongside the name input area):

```
┌────────────────────────────────────────────────────┐
│ request name...                [🌐 dev ▼] [Manage] │
│ GET ▼ │ https://{{baseUrl}}/users           [Send]  │
└────────────────────────────────────────────────────┘
```

- `<Select>` with "No Environment" + all environments for the active collection.
- Changing selection calls `setActiveEnvironment` + persists to localStorage.
- "Manage" icon button (`Settings2` icon) opens `EnvironmentsDialog`.

### 3. Remove `EnvironmentsPanel` and `App.tsx` panel toggle

- `frontend/src/components/collections/EnvironmentsPanel.tsx` — **deleted**.
- `App.tsx` — remove `showEnvironments` state, the toggle button, and the `<EnvironmentsPanel>` render. No right panel.

---

## Variable Substitution

`substituteVariables()` in `frontend/src/lib/environment.ts` already handles `{{varName}}` syntax correctly. No changes needed. It is called in `RequestBuilder` before sending — this remains unchanged.

---

## Files Changed

| File | Change |
|------|--------|
| `backend/internal/infrastructure/repository/collection_repository.go` | Read/write `.env.secret`, updated variable shape |
| `backend/internal/interfaces/handlers/collection_handler.go` | Updated request/response shape, add DELETE handler |
| `backend/internal/interfaces/handlers/routes.go` | Register DELETE route |
| `frontend/src/types/index.ts` | Add `secret` flag to `EnvironmentVariable` |
| `frontend/src/store/collections.ts` | Add `createEnvironment`, `deleteEnvironment`, wire `saveEnvironment`, add localStorage persistence |
| `frontend/src/lib/api.ts` | Add `deleteEnvironment` API method |
| `frontend/src/components/collections/EnvironmentsDialog.tsx` | **New** — full management dialog |
| `frontend/src/components/request-builder/RequestBuilder.tsx` | Add env selector + Manage button to URL bar |
| `frontend/src/components/collections/EnvironmentsPanel.tsx` | **Deleted** |
| `frontend/src/App.tsx` | Remove panel toggle + EnvironmentsPanel |

---

## What Is Not In Scope

- Environment import/export (Bruno `.bru` format)
- Environment variable inheritance / overrides
- Variable highlighting in URL input (autocomplete for `{{`)
- Environment duplication (clone)
