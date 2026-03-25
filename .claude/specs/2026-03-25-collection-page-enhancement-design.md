# Collection Page Enhancement — Design Spec

**Date**: 2026-03-25
**Status**: Draft

## Goal

Enhance the collection overview page to match Postman/Bruno feature parity with three dedicated tabs: Overview, Authorization, and Variables. Store data in a format compatible with future Postman/Bruno export.

## Current State

### What Rocket has now
- `CollectionSettings` (Rust): `description`, `auth`, `headers`
- Stored in `_settings.json` per collection
- UI: Single page with description textarea, then Auth/Headers/Requests tabs in a card
- No collection-level variables (only environment variables exist)

### What Postman has
- **Overview tab**: Name, description (Markdown), schema URL
- **Authorization tab**: Full auth config (inheritable by requests)
- **Variables tab**: Collection-scoped key-value variables (key, initial value, current value, type)
- **Pre-request Script / Tests tabs**: JS scripting (out of scope)
- Variables stored in the collection JSON alongside auth and info

### What Bruno has
- **Overview**: Name, description in `collection.bru`
- **Auth**: Collection auth in `collection.bru`
- **Headers**: Default headers
- **Script / Tests**: JS scripting (out of scope)
- **Vars**: Collection-level variables (Secret and non-secret)
- Variables stored in `environments/` as `.bru` files, but also supports collection-level vars

## Design

### 1. Data Model Changes

**Rust — `CollectionSettings` in `settings.rs`**:

Add `variables` field:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSettings {
    pub description: Option<String>,
    pub auth: Option<Auth>,
    #[serde(default)]
    pub headers: Vec<Header>,
    #[serde(default)]
    pub variables: Vec<CollectionVariable>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CollectionVariable {
    pub key: String,
    pub value: String,
    /// Initial/default value (for Postman compatibility on export).
    #[serde(default)]
    pub initial_value: String,
    pub enabled: bool,
    /// Mark as secret to hide in the UI (like Bruno).
    #[serde(default)]
    pub secret: bool,
}
```

This maps cleanly to:
- **Postman export**: `variable[].key`, `variable[].value`, `variable[].type` (string/secret)
- **Bruno export**: `vars:secret` and `vars:pre-request` blocks in `.bru`

### 2. TypeScript Type Changes

**`tauri-api.ts`** — add `CollectionVariable`:

```typescript
export interface CollectionVariable {
  key: string;
  value: string;
  initialValue: string;
  enabled: boolean;
  secret: boolean;
}

export interface CollectionSettings {
  description?: string;
  auth?: Auth;
  headers: Header[];
  variables: CollectionVariable[];
}
```

**`pane-types.ts`** — update `CollectionSection`:

```typescript
export type CollectionSection = 'overview' | 'auth' | 'headers' | 'variables';
```

### 3. Variable Resolution

Collection variables should be resolved alongside environment variables. Priority order (matching Postman):
1. Environment variables (highest priority, override collection vars)
2. Collection variables
3. Unresolved placeholder stays as `{{name}}`

Update `execution_service.rs` `build_variable_map()` to load collection variables first, then layer environment variables on top.

### 4. UI — Collection Page Tabs

Replace the current combined card layout with dedicated top-level tabs:

**Overview tab**:
- Collection name (read-only, prominent heading)
- Description (Markdown textarea with save-on-blur)
- Stats: request count, folder count, created/modified date
- Default headers editor (moved from separate tab)

**Authorization tab**:
- Full `AuthEditor` component (same as request-level)
- Banner: "This authorization method will be used for every request in this collection. You can override this by specifying one in the request."
- Save button

**Variables tab**:
- Table with columns: Key, Initial Value, Current Value, Secret toggle, Enabled toggle
- Add/remove rows
- Secret values shown as dots until clicked
- Save button
- Info text: "Collection variables are available to all requests in this collection. Environment variables take precedence."

### 5. `_settings.json` Format (on disk)

```json
{
  "description": "My API collection",
  "auth": {
    "authType": "bearer",
    "token": "{{API_TOKEN}}"
  },
  "headers": [
    { "key": "Content-Type", "value": "application/json", "enabled": true }
  ],
  "variables": [
    { "key": "BASE_URL", "value": "https://api.example.com", "initialValue": "https://api.example.com", "enabled": true, "secret": false },
    { "key": "API_TOKEN", "value": "tok_live_xxx", "initialValue": "", "enabled": true, "secret": true }
  ]
}
```

### 6. Export Compatibility

For future Postman export, the mapping is:
- `description` -> `info.description`
- `auth` -> `auth` (top-level in postman_collection.json)
- `variables` -> `variable[]` with `{ key, value, type: "string"|"secret" }`

For future Bruno export:
- `description` -> description in `collection.bru`
- `auth` -> `auth:bearer { token {{API_TOKEN}} }` block in `collection.bru`
- `variables` -> `vars:pre-request { key: value }` and `vars:secret [key]` in `collection.bru`

No data is lost because all fields have direct mappings.

### 7. New Tauri Command

Add a lightweight `get_collection_settings` command to avoid loading the full collection tree just to read settings. Currently settings are only accessible via `get_collection` which walks the entire filesystem.

```rust
#[tauri::command]
pub fn get_collection_settings(
    name: String,
    svc: State<'_, CollectionService>,
) -> Result<CollectionSettings, DomainError> {
    svc.get_settings(&name)
}
```

### 8. Files to Change

**Backend (Rust)**:
- `crates/rocket-collection/src/settings.rs` — add `CollectionVariable` struct and `variables` field
- `crates/rocket-app/src/execution_service.rs` — load collection variables into the variable map
- `src-tauri/src/commands/collections.rs` — add `get_collection_settings` command

**Frontend (TypeScript)**:
- `src/lib/tauri-api.ts` — add `CollectionVariable` type, update `CollectionSettings`, add `getCollectionSettings()` API
- `src/types/pane-types.ts` — update `CollectionSection` type
- `src/components/collections/CollectionOverviewTab.tsx` — rewrite with new tab layout (Overview, Authorization, Variables)
- New: `src/components/collections/CollectionVariablesEditor.tsx` — variable table component

### 9. Out of Scope
- Pre-request scripts and test scripts (Postman/Bruno feature, complex JS runtime)
- Postman/Bruno import/export commands (future feature, this spec just ensures data compatibility)
- Collection-level events/webhooks
- Folder-level auth/headers cascade (Postman feature, requires Folder struct changes)
