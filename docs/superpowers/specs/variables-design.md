# Variables System — Design Spec

**Version:** Final  
**Feature:** Full Bruno-compatible variable resolution for RocketAPI

---

## Directory Structure (Target)

```
~/.rocket-api/<workspace>/
  workspace.yml                    ← globalEnvironment: "shared-team" stored here
  environments/                    ← GLOBAL environments (workspace-wide)
    shared-team.yml
    company-defaults.yml
  collections/
    my-api/
      opencollection.yml           ← collection variables (request.variables[])
      environments/                ← REGULAR environments (per-collection)
        local.yml
        staging.yml
        production.yml
      auth/
        folder.yml                 ← folder variables (request.variables[])
        oauth/
          folder.yml               ← nested folder vars (overrides parent on collision)
          refresh.yml
      get-users.yml                ← request variables (runtime.variables[])
```

Two separate `environments/` directories:
- `<workspace>/environments/` — global environments, active across all collections
- `<collection>/environments/` — regular environments, scoped to one collection

---

## Resolution Hierarchy

```
┌────────────────────────────────────────────────────────────┐  ← highest
│  7. Runtime Variables     bru.setVar() in scripts          │
├────────────────────────────────────────────────────────────┤
│  6. Request Variables     runtime.variables[] in .yml      │
├────────────────────────────────────────────────────────────┤
│  5. Folder Variables      request.variables[] in           │
│                           folder.yml (full chain, inner    │
│                           folder wins on collision)        │
├────────────────────────────────────────────────────────────┤
│  4. Environment Variables active env in collection/envs/   │
├────────────────────────────────────────────────────────────┤
│  3. Collection Variables  request.variables[] in           │
│                           opencollection.yml               │
├────────────────────────────────────────────────────────────┤
│  2. Global Environment    selected env in workspace/envs/  │
├────────────────────────────────────────────────────────────┤
│  1. Process Env           {{process.env.FOO}} syntax only  │
└────────────────────────────────────────────────────────────┘  ← lowest
```

**Priority rules:**
- Environment (4) beats Collection (3) — Bruno's documented order
- Folder variables walk full parent chain — innermost folder wins
- `{{process.env.KEY}}` resolves only with that full prefix syntax

---

## Scope Definitions

### 1. Process Environment Variables

| | |
|---|---|
| Source | `std::env::vars()` at Tauri app launch |
| Syntax | `{{process.env.MY_VAR}}` only — bare `{{MY_VAR}}` does NOT resolve |
| Writable | No — read-only |
| Persistence | In-memory only, loaded once at startup |
| Secret | Never logged or synced |

### 2. Global Environment Variables

| | |
|---|---|
| File | `<workspace>/environments/<n>.yml` |
| Selection | "Global" tab in Environment Switcher |
| Selection persistence | `workspace.yml` → `globalEnvironment: "<n>"` |
| Scope | All collections in workspace simultaneously |
| Priority | Below active collection environment (4) |
| Version control | Yes — committed to Git |
| Secrets | NOT written to `.yml`; masked in UI |

The Environment Switcher has two independent sections — any env file can appear in either slot simultaneously:

```
┌──────────────────────────────────────┐
│  Global                              │
│  ● shared-team  ✓                    │  ← workspace/environments/shared-team.yml
│  ○ company-defaults                  │
│  ────────────────────────────────── │
│  Environment                         │
│  ● staging  ✓                        │  ← collection/environments/staging.yml
│  ○ local                             │
│  ○ production                        │
└──────────────────────────────────────┘
```

**File format** (identical to regular environments):
```yaml
name: shared-team
variables:
  - name: sharedToken
    value: ""           # secret — value in OS keychain only
    enabled: true
    secret: true
  - name: baseApiUrl
    value: https://api.acme.com
    enabled: true
    secret: false
```

### 3. Collection Variables

| | |
|---|---|
| File | `<collection>/opencollection.yml` → `request.variables[]` |
| Rust type | `CollectionSettings.variables: Vec<CollectionVariable>` |
| Status | ✅ Stored and resolved |
| Priority | Below environment |
| UI | Collection Settings → Variables tab |

**`value` vs `initialValue` resolution rule:**  
Each variable has two value fields:
- `value` — local override, NOT committed to Git
- `initialValue` — shared default, committed to Git (shown as placeholder in editor)

At resolution time: use `value` if non-empty; fall back to `initialValue`.

```ts
const resolved = v.value || v.initialValue || '';
if (v.enabled && v.key && resolved) ctx[v.key] = resolved;
```

This rule applies to **folder variables** too.

### 4. Environment Variables

| | |
|---|---|
| File | `<collection>/environments/<n>.yml` |
| Selection | One active per collection; persisted to `localStorage["rocket-api:active-env:<collection>"]` |
| Priority | Above collection — env wins on collision |
| Version control | Yes — travels with the collection in Git |
| Secrets | NOT written to `.yml`; masked in UI |

**Current status:** `FsEnvironmentRepo` is pointed at `<workspace>/environments/` (wrong). Tauri commands have no `collection` parameter. Both must change.

**No file migration needed:** existing workspace-level env files are correctly placed as global environments. Collection environments are created fresh.

**File format:**
```yaml
name: staging
variables:
  - name: baseUrl
    value: https://staging.example.com
    enabled: true
    secret: false
  - name: apiKey
    value: ""            # secret — real value in OS keychain only
    enabled: true
    secret: true
```

### 5. Folder Variables

| | |
|---|---|
| File | `<folder>/folder.yml` → `request.variables[]` |
| Rust type | `OcFolderInfo.request.variables: Option<Vec<OcVariable>>` |
| Status | ✅ Schema exists; ❌ not read at resolve time |
| Scope | All requests in this folder and all sub-folders |

**Folder chain inheritance:**  
For a request at `auth/oauth/refresh.yml`, the resolver walks the full parent chain server-side:

```
auth/folder.yml       ← loaded first (lower priority)
auth/oauth/folder.yml ← loaded second (inner folder wins on collision)
```

The Tauri command `get_folder_chain_variables(collection, request_path)` takes the full request path and returns one merged, priority-applied list.

**File format:**
```yaml
name: auth
type: folder
request:
  variables:
    - name: folderBase
      value: https://auth.example.com
    - name: timeout
      value: "5000"
      disabled: true
```

### 6. Request Variables

| | |
|---|---|
| File | `<request>.yml` → `runtime.variables[]` |
| Rust type | `OcHttpRequestRuntime.variables: Vec<OcVariable>` |
| Status | ✅ Schema exists; ❌ not read at resolve time |
| Scope | This request only |

**File format:**
```yaml
runtime:
  variables:
    - name: userId
      value: "42"
```

### 7. Runtime Variables (SP3 — deferred)

Set by pre/post-request scripts via `bru.setVar("key", "value")`. Cleared after each send. Highest priority of all scopes.

### 8. Prompt Variables (Phase 2 — deferred)

`{{prompt.VAR}}` pauses the send, shows a dialog per unique prompt variable, stores user input in runtime scope, clears after response. User cancel aborts the send.

---

## Current State vs Target

| Scope | File path correct | Resolved at send | UI editor | Work needed |
|---|---|---|---|---|
| Process env | N/A | ❌ | N/A | Load at startup via Tauri command |
| Global env | ✅ `<workspace>/environments/` | ❌ | ❌ | Global switcher tab; `workspace.yml` pointer; store wiring |
| Environment | ❌ wrong path | ✅ | ✅ | Move to `<collection>/environments/`; add `collection` param to Tauri commands |
| Collection | ✅ | ✅ | ✅ | Add `value\|\|initialValue` fallback |
| Folder | ✅ (schema) | ❌ | ❌ | Chain-walk Tauri command; resolution wiring; `FolderVariablesPopover` |
| Request | ✅ (schema) | ❌ | ❌ | Tauri commands; resolution wiring; Variables tab in editor |
| Runtime | — | — | — | SP3 scripting — deferred |
| Prompt | — | — | — | Phase 2 — deferred |

---

## Architecture Changes

### Rust: VariableContext (new — rocket-environment crate)

```rust
pub struct VariableContext {
    pub runtime:     HashMap<String, String>,  // highest priority
    pub request:     HashMap<String, String>,
    pub folder:      HashMap<String, String>,
    pub env:         HashMap<String, String>,  // env beats collection
    pub collection:  HashMap<String, String>,
    pub global_env:  HashMap<String, String>,
    pub process_env: HashMap<String, String>,  // lowest; keys stored as "process.env.KEY"
}

impl VariableContext {
    pub fn flatten(&self) -> HashMap<String, String>
    // Insertion order: global_env → collection → env → folder → request → runtime

    pub fn flatten_with_process_env(&self) -> HashMap<String, String>
    // Same as flatten() but prepends "process.env.KEY" entries first (lowest priority)
}
```

### Rust: CollectionVariable (updated)

```rust
pub struct CollectionVariable {
    pub key:           String,
    pub value:         String,
    pub initial_value: String,  // fallback when value is empty
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub enabled:       bool,
    pub secret:        bool,
}
```

### New Tauri Commands

```rust
// ── Regular (collection-scoped) environments ──
list_environments(collection: String) -> Vec<Environment>
get_environment(collection: String, name: String) -> Environment
save_environment(collection: String, env: Environment)
delete_environment(collection: String, name: String)

// ── Global environment (selection pointer in workspace.yml) ──
get_global_environment_name() -> Option<String>
set_global_environment(name: Option<String>)   // None = deselect

// ── Process environment (read-only, OS vars) ──
get_process_env_vars() -> HashMap<String, String>

// ── Folder variables (chain walk, not single folder) ──
get_folder_chain_variables(collection: String, request_path: String) -> Vec<CollectionVariable>
save_folder_variables(collection: String, folder_path: String, vars: Vec<CollectionVariable>)

// ── Request variables ──
get_request_variables(collection: String, request_path: String) -> Vec<CollectionVariable>
save_request_variables(collection: String, request_path: String, vars: Vec<CollectionVariable>)
```

### TypeScript: Key type changes

```ts
// Updated CollectionVariable
export interface CollectionVariable {
  key:           string
  value:         string
  initialValue?: string   // fallback when value empty
  enabled:       boolean
  secret:        boolean
}

// New buildVariableContext (src/lib/variable-context.ts)
function buildVariableContext(params: {
  runtimeVars?:    Record<string, string>
  requestVars?:    CollectionVariable[]
  folderVars?:     CollectionVariable[]   // already chain-merged by backend
  collectionVars?: CollectionVariable[]
  envVars?:        Record<string, string>
  globalVars?:     Record<string, string>
  processEnvVars?: Record<string, string>
}): Record<string, string>
// Insertion: process.env.* → global → collection → env → folder → request → runtime

// New VariableScopeEntry (src/lib/url-variables.ts)
export interface VariableScopeEntry {
  value:  string
  source: 'runtime'|'request'|'folder'|'environment'|'collection'|'global'|'process'
  label:  string    // "Staging", "Collection", "Folder", etc.
  secret: boolean   // true → show ●●●● in overlay tooltip
}
```

### Updated useEnvStore fields

```ts
// New state
activeCollection: string | null       // which collection's envs are loaded
globalEnvName:    string | null       // name from workspace.yml
globalEnv:        Environment | null  // loaded global env object
processEnvVars:   Record<string, string>

// Updated actions
loadEnvironments(collection: string): Promise<void>
  // scopes environments to collection; restores activeEnvId from localStorage

// New actions
setGlobalEnv(name: string | null): Promise<void>
fetchGlobalEnv(): Promise<void>
loadProcessEnvVars(): Promise<void>
getGlobalVariables(): Record<string, string>
```

---

## Fields Resolved at Send Time

| Field | Now | After | Notes |
|---|---|---|---|
| URL | ✅ | ✅ all 7 scopes | |
| Header keys | ⚠️ | ✅ | Both key AND value resolved |
| Header values | ✅ | ✅ | |
| JSON / XML / text body | ✅ | ✅ | Resolved as single string |
| Form-urlencoded fields | ❌ | ✅ | Each field VALUE individually |
| Multipart fields | ❌ | ✅ | Each field VALUE individually |
| Auth fields (all types) | ⚠️ | ✅ | All string fields per auth type |
| Query param values | ❌ | ✅ | |
| Path param values | ❌ | ✅ | |
| Collection default headers | ❌ | ✅ | Merged before resolution |
| Collection default auth | ❌ | ✅ | Merged before resolution |
| GraphQL query + variables | ❌ | ✅ | |
| gRPC metadata values | ❌ | ✅ | |
| WebSocket URL + messages | ❌ | ✅ | |
| Request name / description | N/A | N/A | Metadata — not interpolated |

---

## URL Overlay Scope Badges

| Scope | Label | Colour |
|---|---|---|
| Runtime | Runtime | Orange |
| Request | Request | Purple |
| Folder | Folder | Amber |
| Environment | env name | Green |
| Collection | Collection | Blue |
| Global | Global | Teal |
| Process env | Process Env | Grey |
| Unresolved | — | Red |

Secret variables from **any scope** show `●●●●` in the overlay tooltip. Badge colour still shows (variable IS resolved — only the preview is hidden).

---

## Out of Scope

| Item | Milestone |
|---|---|
| Runtime vars from scripts (`bru.setVar`) | SP3 scripting |
| Script env mutation (`bru.setEnvVar` / `bru.getEnvVar`) | SP3 scripting |
| Prompt variables `{{prompt.VAR}}` | Phase 2 |
| Environment `extends` inheritance | Future |
| `.env` file loading (`dotEnvFilePath`) | Future |
| OS keychain for secret storage | Future |
| Variable value variants (multi-value / typed) | Future |
| Collection runner scope isolation between requests | SP3 |
