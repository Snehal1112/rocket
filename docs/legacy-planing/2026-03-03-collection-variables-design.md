# Collection Variables System Design

## Goal

Add Bruno-compatible collection-level variables to Rocket, giving users a
variable resolution hierarchy (environment overrides collection) and secret
variable masking, with no changes required to existing `.bru` files.

## Architecture

### Variable resolution hierarchy

```
active environment vars  →  collection vars
        (wins)                  (fallback)
```

Scripts (phase 2) will sit above both. For now, environment always wins on
key collision.

### File format (`collection.bru` at collection root)

Bruno-compatible format:

```
vars {
  baseUrl: https://api.example.com
  timeout: 30
}
vars:secret [
  apiKey
]
```

- `vars {}` block holds all variable values (including secrets).
- `vars:secret []` lists which keys are secret.
- If `collection.bru` does not exist yet, treat as empty and create on first
  save.

### Variable resolution at request dispatch

The backend merges collection vars and active environment vars before
forwarding a request. Environment vars win on key collision. Resolution
applies to: URL, headers, body, query params, and auth fields.

Unresolved `{{var}}` references resolve to an empty string (Bruno behaviour).

---

## Backend changes

**`collection_repository.go`**
- `ReadCollectionVars(name string) ([]CollectionVar, error)` — parse
  `collection.bru`, return key/value/secret rows.
- `WriteCollectionVars(name string, vars []CollectionVar) error` — write
  `vars {}` and `vars:secret []` blocks back to `collection.bru`.

**`collection_handler.go`**
- `GET  /api/v1/collections/:name/variables` — return vars; secret values
  masked as `""` in the response.
- `POST /api/v1/collections/:name/variables` — replace all vars atomically.

**`request_handler.go`**
- Before dispatching, load collection vars and active environment vars, merge
  (env wins), then substitute `{{key}}` in all string fields of the request.

---

## Frontend changes

**`src/lib/api.ts`**
- `getCollectionVariables(name: string): Promise<CollectionVar[]>`
- `saveCollectionVariables(name: string, vars: CollectionVar[]): Promise<void>`

**`src/store/collections.ts`**
- Add `collectionVariables: CollectionVar[]`
- Add `fetchCollectionVariables(name: string): Promise<void>`
- Add `saveCollectionVariables(name: string, vars: CollectionVar[]): Promise<void>`
- Call `fetchCollectionVariables` inside `setActiveCollection` after the
  collection is set.

**`src/components/collections/CollectionVariablesEditor.tsx`** *(new)*
- Variable table: key | value | secret toggle | enabled toggle | delete row.
- Secret values display as `●●●●`; user must re-enter to change (same as
  Bruno).
- Add-row button at the bottom.

**`src/components/collections/CollectionOverview.tsx`**
- Add a "Variables" tab alongside existing content.
- Renders `CollectionVariablesEditor`.
- Save triggers `saveCollectionVariables`.

---

## Data model

```ts
interface CollectionVar {
  key: string
  value: string   // empty string returned from API when secret
  enabled: boolean
  secret: boolean
}
```

---

## Out of scope (this phase)

- Folder-level variables (comes with scripting phase).
- Runtime variables set by scripts (`bru.setVar()`) — phase 2.
- Variable autocomplete / highlighting in the URL bar.
- Postman-style global variables.
