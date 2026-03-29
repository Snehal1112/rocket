# Workspace Feature — Design Spec

**Date:** 2026-03-29
**Status:** Approved
**Scope:** Workspace lifecycle, default tabs, sidebar modes, per-workspace config, collection binding

---

## 1. Overview

RocketAPI workspaces are containers that group collections, global environments, and Git context together. This spec defines the full workspace feature: domain model extensions, per-workspace configuration files, workspace default tabs, sidebar modes, pin/description support, and collection binding (embedded + external).

The feature is modeled after Bruno 3.0's workspace system with RocketAPI-specific adaptations (mutually exclusive tab groups, configurable multi-workspace mode, external collection references).

### What already exists

- `rocket-workspace` crate: `Workspace` entity (id, name, path), `WorkspaceRegistry`, `WorkspaceRepository` trait
- `rocket-infra`: `FsWorkspaceRepo` (persists registry to `~/.config/rocket-api/workspaces.yml`), `SharedPathCollectionRepo` (dynamic collection path switching)
- `rocket-app`: `WorkspaceService` with list, create, switch, rename, close, delete + domain events (`WorkspaceCreated`, `WorkspaceSwitched`, `WorkspaceRenamed`, `WorkspaceClosed`, `WorkspaceDeleted`)
- `src-tauri`: all workspace Tauri commands wired, `open_folder_picker` command
- Frontend: `workspace-store.ts` (full Zustand store with Tauri event listeners), `WorkspaceSwitcher` dropdown with create/rename/close/delete dialogs, `CreateWorkspaceDialog` with folder picker

### What this spec adds

- Domain model: `description`, `pinned` fields on `Workspace`; `WorkspaceConfig` struct for per-workspace `workspace.yml`; `CollectionReference` type (embedded vs external)
- Per-workspace `workspace.yml` file inside each workspace directory (portable, Git-friendly)
- App-level registry: `multi_workspace_mode` setting
- Workspace default tabs: Overview, Global Environments, Git (mutually exclusive with collection/request tabs)
- Sidebar: single-workspace mode (default, unchanged) and multi-workspace mode (accordion sections)
- Pin workspace functionality
- Tab state persistence across sessions
- First-launch auto-creation of Default Workspace at `~/Documents/RocketAPI/`

---

## 2. Domain Model Changes

### 2.1 Workspace entity (`rocket-workspace`)

Current fields: `id: String`, `name: String`, `path: PathBuf`

Add:

```rust
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub description: Option<String>,  // NEW — editable text shown on workspace home tab
    pub pinned: bool,                  // NEW — pinned workspaces float to top of switcher
}
```

### 2.2 WorkspaceRegistry

Add a global setting:

```rust
pub struct WorkspaceRegistry {
    pub workspaces: Vec<Workspace>,
    pub active_workspace_id: String,
    pub multi_workspace_mode: bool,  // NEW — default false
}
```

When `multi_workspace_mode` is `false` (default): switching workspaces unloads the previous one, sidebar shows only active workspace's collections. When `true`: multiple workspaces stay loaded, sidebar uses accordion layout.

### 2.3 WorkspaceConfig (NEW)

A new struct representing the per-workspace `workspace.yml` that lives inside each workspace directory:

```rust
pub struct WorkspaceConfig {
    pub name: String,
    pub description: Option<String>,
    pub collections: Vec<CollectionReference>,
    pub environments: WorkspaceEnvironmentsConfig,
}

pub struct CollectionReference {
    pub name: String,
    pub ref_type: CollectionRefType,
    pub path: Option<PathBuf>,  // only for external
}

pub enum CollectionRefType {
    Embedded,   // lives in workspace/collections/
    External,   // absolute path on disk
}

pub struct WorkspaceEnvironmentsConfig {
    pub active_environment: Option<String>,
}
```

### 2.4 New repository trait

```rust
pub trait WorkspaceConfigRepository: Send + Sync {
    fn load(&self, workspace_path: &Path) -> DomainResult<WorkspaceConfig>;
    fn save(&self, workspace_path: &Path, config: &WorkspaceConfig) -> DomainResult<()>;
}
```

Implemented by `FsWorkspaceConfigRepo` in `rocket-infra`.

---

## 3. File Layout

### 3.1 App-level registry

Location: `~/.config/rocket-api/workspaces.yml` (resolved via `dirs::config_dir()`)

```yaml
activeWorkspaceId: "default"
multiWorkspaceMode: false
workspaces:
  - id: "default"
    name: "Default Workspace"
    path: "/Users/snehal/Documents/RocketAPI"
    pinned: true
  - id: "ws-abc123"
    name: "My Project"
    path: "/Users/snehal/projects/my-project"
    description: "Backend APIs for the main product"
    pinned: false
```

### 3.2 Per-workspace directory

```
my-project/
├── workspace.yml          # workspace config (portable)
├── collections/           # embedded collections
│   ├── Users API/
│   │   ├── opencollection.yml
│   │   └── ...
│   └── Auth Service/
│       ├── opencollection.yml
│       └── ...
└── environments/          # global environments
    ├── dev.yml
    └── staging.yml
```

### 3.3 Per-workspace `workspace.yml`

```yaml
name: "My Project"
description: "Backend APIs for the main product"
collections:
  - name: "Users API"
    type: embedded
  - name: "Auth Service"
    type: embedded
  - name: "Shared Auth Library"
    type: external
    path: "/Users/snehal/projects/shared-auth-collection"
environments:
  activeEnvironment: "staging"
```

### 3.4 Default Workspace

- Auto-created on first launch at `~/Documents/RocketAPI/`
- Uses hardcoded id `"default"`
- Cannot be closed or deleted
- Does not show Git tab (lives in app data, not a Git repo)
- Location changeable later in Preferences → General (future scope)

---

## 4. Workspace Default Tabs

### 4.1 Tab group model

The editor area operates in one of two mutually exclusive modes:

- **Workspace mode**: shows workspace default tabs (not closable)
- **Collection mode**: shows collection/request/diff/conflict tabs (closable as usual)

Opening one mode dismisses the other. The pane store tracks which mode is active.

### 4.2 Default tabs per workspace type

| Workspace type | Default tabs |
|---|---|
| Default Workspace | Overview, Global Environments |
| Custom workspace | Overview, Global Environments, Git |

### 4.3 Tab behavior

- **Not closable**: workspace default tabs have no × button, no "Close" in context menu
- **Not reorderable**: fixed left-to-right order (Overview → Environments → Git)
- **Clicking workspace** in sidebar or switcher: closes all collection/request tabs, opens workspace default tabs, activates Overview
- **Clicking collection/request** in sidebar: closes all workspace default tabs, opens the request tab
- **On app launch**: restore last active mode. If workspace mode was active, reopen workspace default tabs. If collection mode was active, reopen the previously open collection/request tabs
- **On workspace switch**: close all tabs (both workspace and collection), open the new workspace's default tabs

### 4.4 New tab type

Add to `pane-types.ts`:

```typescript
export type WorkspaceSection = 'overview' | 'environments' | 'git';

export interface WorkspaceTab extends BaseTab {
  tabType: 'workspace';
  workspaceId: string;
  activeSection: WorkspaceSection;
}

export type Tab = RequestTab | CollectionTab | DiffTab | ConflictTab | WorkspaceTab;
```

### 4.5 Pane store changes

New actions on the pane store:

- `openWorkspaceTabs(workspaceId: string, isDefault: boolean)`: closes all existing tabs, opens 2 or 3 workspace default tabs
- `closeWorkspaceTabs()`: removes all workspace tabs from the pane
- `isWorkspaceMode(): boolean`: returns true if workspace tabs are currently showing

When `openTab()` is called for a request/collection tab and workspace tabs are showing, it calls `closeWorkspaceTabs()` first. When `openWorkspaceTabs()` is called and collection tabs are showing, it calls `closeAll()` first.

---

## 5. Workspace Home Tab Content

### 5.1 Overview section

- Workspace name (h2, with ⋯ menu → rename)
- Description (inline editable text, saves to both registry and workspace.yml)
- Info bar showing disk path (monospace)
- Quick action buttons: Rename, Show in Folder

### 5.2 Collections list

- Each collection shown as a card row: icon, name, request/folder count, embedded/external badge
- Clicking a collection opens its `CollectionOverviewTab` (switches to collection mode)
- "Add collection" dashed button at bottom with options: "Create new collection", "Link external collection"

### 5.3 Global Environments tab

- Same UI as existing `EnvironmentDialog` but rendered inline as tab content instead of a dialog
- Scoped to the workspace's `environments/` directory
- Left panel: list of environments with add/delete
- Right panel: variable editor for selected environment
- Active environment selector

### 5.4 Git tab

- Only shown for custom workspaces (not Default Workspace)
- Reuses existing Git UI components (`GitPanel`, status, branches, commit, etc.)
- Scoped to the workspace directory as the Git root

---

## 6. Sidebar Design

### 6.1 Single-workspace mode (default)

Identical to current sidebar. Shows only the active workspace's collections. No workspace grouping headers. Workspace switching happens in the title bar dropdown. When workspace switches, sidebar reloads with new workspace's collections.

### 6.2 Multi-workspace mode (opt-in)

Sidebar header changes from "Collections" to "Workspaces". Collections are grouped under collapsible workspace section headers.

Each workspace section header shows:
- Collapse/expand chevron
- Workspace icon
- Workspace name (font-weight: 500)
- Collection count (muted, right-aligned)
- On hover: + button (add collection), ⋯ button (workspace actions menu)

Workspace actions context menu:
- Add collection
- Open workspace home
- Rename workspace
- Show in folder
- ─── (separator)
- Close workspace (destructive)

Each section is independently collapsible. Collections nest under their workspace section with the existing collection tree UI (folders, requests, method badges).

### 6.3 Clicking workspace section header

Clicking the workspace name in a section header (not the chevron) opens the workspace default tabs for that workspace. Clicking the chevron only toggles expand/collapse.

---

## 7. Pin Workspace

### 7.1 Behavior

- Pinned workspaces appear at the top of the workspace switcher dropdown, separated from unpinned workspaces by a divider
- Within each group (pinned, unpinned), workspaces are sorted alphabetically
- Pin icon shown on hover in the switcher dropdown — click to toggle
- Pin state persisted in the app-level registry (`pinned: bool` on `Workspace`)

### 7.2 Backend

Add to `WorkspaceService`:

```rust
pub fn pin(&self, id: &str) -> DomainResult<()>;
pub fn unpin(&self, id: &str) -> DomainResult<()>;
```

Add Tauri commands: `pin_workspace`, `unpin_workspace`

Add domain events: `WorkspacePinned { id }`, `WorkspaceUnpinned { id }`

---

## 8. Collection Binding

### 8.1 Embedded collections

Default behavior. When a collection is created inside a workspace, it goes into `<workspace>/collections/<collection-name>/`. The `workspace.yml` is updated with an embedded reference.

### 8.2 External collections

A collection directory anywhere on disk can be "linked" to a workspace. The `workspace.yml` stores the absolute path. The collection's `opencollection.yml` is read from that external location.

### 8.3 Collection resolution

When loading collections for a workspace, the system:
1. Reads `workspace.yml` to get the collection references
2. For embedded refs: scans `<workspace>/collections/` directory
3. For external refs: resolves the absolute path from the reference
4. Returns a unified list of `CollectionSummary` with a `ref_type` indicator

### 8.4 FsCollectionRepo changes

`SharedPathCollectionRepo` currently points to a single base directory. It needs to be extended to support multiple collection roots (one per workspace's collections dir + any external paths). The simplest approach: when the active workspace changes, rebuild the collection path list from the workspace config.

### 8.5 New Tauri command

```
link_external_collection(workspace_id: string, collection_path: string) -> CollectionSummary
```

Validates the path contains an `opencollection.yml`, adds an external reference to `workspace.yml`, and returns the collection summary.

---

## 9. Tab State Persistence

### 9.1 Storage

File: `<app-config-dir>/ui-state.yml` (resolved via Tauri's `app_config_dir()`)

```yaml
activeMode: "workspace"
workspaceTabs:
  workspaceId: "ws-abc123"
```

For v1, only the active mode and workspace id are persisted. Collection tab restoration (re-reading source files from disk) is a future enhancement.

### 9.2 Persistence triggers

- Save on tab open/close/switch
- Save on workspace switch
- Save on sidebar collapse/expand
- Debounced (500ms) to avoid excessive writes

### 9.3 Restore on launch

On app startup:
1. Load `ui-state.yml`
2. If `activeMode` is `"workspace"`: open workspace default tabs for the stored workspace
3. If `activeMode` is `"collection"`: show empty state (collection tab restoration from source files is future scope)
4. If no prior state exists (first launch): open workspace default tabs for the active workspace

---

## 10. Sub-Project Breakdown

Six sub-projects, executed sequentially. Each has its own plan file in `docs/superpowers/plans/workspace/`.

| Plan file | SP | Focus |
|---|---|---|
| `2026-03-29-plan-01-workspace-feature-domain-model.md` | SP-W1 | Domain model + per-workspace config |
| `2026-03-29-plan-02-workspace-feature-backend-crud.md` | SP-W2 | Backend CRUD extensions |
| `2026-03-29-plan-03-workspace-feature-collection-binding.md` | SP-W3 | Collection binding |
| `2026-03-29-plan-04-workspace-feature-default-tabs.md` | SP-W4 | Workspace default tabs |
| `2026-03-29-plan-05-workspace-feature-sidebar-modes.md` | SP-W5 | Sidebar modes |
| `2026-03-29-plan-06-workspace-feature-polish-settings.md` | SP-W6 | Polish + settings |

### SP-W1: Domain model + per-workspace config
**Layer:** Backend (rocket-workspace, rocket-shared)
**Scope:**
- Add `description: Option<String>`, `pinned: bool` to `Workspace` entity
- Add `multi_workspace_mode: bool` to `WorkspaceRegistry`
- Create `WorkspaceConfig`, `CollectionReference`, `CollectionRefType` structs
- Create `WorkspaceConfigRepository` trait
- Add `WorkspacePinned`, `WorkspaceUnpinned` domain events to `rocket-shared`
- Unit tests for all new types and validation

### SP-W2: Backend CRUD extensions
**Layer:** Backend (rocket-infra, rocket-app, src-tauri)
**Scope:**
- Implement `FsWorkspaceConfigRepo` in `rocket-infra` (read/write per-workspace `workspace.yml`)
- Create workspace directory structure on `create` (`workspace.yml`, `collections/`, `environments/`)
- Add `pin`, `unpin`, `update_description` to `WorkspaceService`
- Add `open_workspace` to `WorkspaceService` (reads existing `workspace.yml` from disk, registers in registry)
- Wire new Tauri commands: `pin_workspace`, `unpin_workspace`, `update_workspace_description`, `open_workspace`
- Update `create_workspace` to write `workspace.yml` inside the workspace dir
- Update frontend `tauri-api.ts` and `workspace-store.ts` with new commands and events
- Tests for all new service methods

### SP-W3: Collection binding
**Layer:** Full-stack (rocket-infra, rocket-app, src-tauri, frontend)
**Scope:**
- Extend collection loading to read from `workspace.yml` collection references
- Support embedded collections (scan `collections/` dir) and external references (resolve absolute path)
- Implement `link_external_collection` Tauri command
- Update `create_collection` to write into workspace's `collections/` dir and update `workspace.yml`
- Update `CollectionSummary` with `ref_type` field
- Update sidebar to show embedded/external badge on collections
- Tests for collection resolution with mixed embedded/external refs

### SP-W4: Workspace default tabs
**Layer:** Frontend (React/TypeScript)
**Scope:**
- Add `WorkspaceTab` type to `pane-types.ts`
- Implement mutual exclusion logic in pane store (`openWorkspaceTabs`, `closeWorkspaceTabs`)
- Create `WorkspaceOverviewTab` component (name, description, path, collections list, quick actions)
- Create `WorkspaceEnvironmentsTab` component (inline environment editor, reusing existing `EnvironmentDialog` internals)
- Create `WorkspaceGitTab` component (reusing existing Git UI components)
- Non-closable tab behavior in `TabBar`
- Default Workspace shows 2 tabs (no Git), custom workspaces show 3
- Wire sidebar workspace click → open workspace tabs
- Wire sidebar collection/request click → close workspace tabs

### SP-W5: Sidebar modes
**Layer:** Frontend (React/TypeScript)
**Scope:**
- Create `WorkspaceSection` component (collapsible header with workspace actions menu)
- Update `CollectionsSidebar` to read `multiWorkspaceMode` and render either flat or grouped layout
- Workspace section header: chevron, icon, name, collection count, hover actions (+, ⋯)
- Workspace context menu: add collection, open workspace home, rename, show in folder, close
- Independent expand/collapse state per workspace section
- Clicking workspace name → open workspace tabs; clicking chevron → toggle collapse

### SP-W6: Polish + settings
**Layer:** Full-stack
**Scope:**
- Multi-workspace mode toggle in settings/preferences UI
- Tab state persistence (`ui-state.yml`): save/restore active mode and open tabs
- First-launch auto-creation of Default Workspace at `~/Documents/RocketAPI/`
- Pin sorting in workspace switcher dropdown (pinned first, then alphabetical)
- Edge cases: closing last workspace prevention, switching when tabs are dirty, workspace deleted while its tabs are open
- Update `WorkspaceSwitcher` dropdown layout (pinned section, divider, unpinned section, actions)

---

## 11. Dependencies Between Sub-Projects

```
SP-W1 (domain model)
  └── SP-W2 (backend CRUD)
        └── SP-W3 (collection binding)
              ├── SP-W4 (workspace default tabs)  [frontend]
              └── SP-W5 (sidebar modes)            [frontend]
                    └── SP-W6 (polish + settings)  [full-stack]
```

SP-W4 and SP-W5 can be built in parallel after SP-W3 completes. SP-W6 depends on both SP-W4 and SP-W5.

---

## 12. Out of Scope (Future)

- Import/export workspace (zip, Git clone)
- Workspace templates
- Workspace-level scripting context
- Shared workspaces across team members (cloud sync)
- Changing Default Workspace location in Preferences
- API specifications attached to workspaces
