# Plan 05 — Tauri AppState wiring and commands

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire `WorkspaceService` into `AppState`, update all existing services to read `active_workspace_path`, and register 8 new Tauri commands.

**Architecture:** `AppState` gains `active_workspace_path: Arc<Mutex<PathBuf>>`. On app startup the active workspace path is loaded from `FsWorkspaceRepo`. All commands that construct `FsCollectionRepo`, `FsEnvironmentRepo`, `FsHistoryRepo`, `FsTemplateRepo` read from `active_workspace_path` instead of a hardcoded path.

**Tech Stack:** Rust, Tauri v2, tauri-plugin-dialog

**Spec:** `docs/superpowers/specs/2026-03-28-workspace-feature-design.md`

**Previous plan:** `plan-04-workspace-service.md`
**Next plan:** `plan-06-tauri-api-frontend.md`

---

### Task 1: Add active_workspace_path to AppState

**Files:**
- Modify: `src-tauri/src/` — wherever `AppState` is defined

First, find the file:
```bash
find src-tauri/src -name "*.rs" | xargs grep -l "AppState" | head -5
```

- [ ] **Step 1: Add `active_workspace_path` field to `AppState`**

```rust
use std::sync::{Arc, Mutex};
use std::path::PathBuf;

pub struct AppState {
    // ... existing fields ...
    pub active_workspace_path: Arc<Mutex<PathBuf>>,
    pub workspace_service: Mutex<WorkspaceService>,
}
```

- [ ] **Step 2: In the Tauri `.setup()` closure, initialize `FsWorkspaceRepo` and `WorkspaceService`**

```rust
use rocket_infra::FsWorkspaceRepo;
use rocket_app::WorkspaceService;
use rocket_shared::events::NullEventPublisher; // replace with real publisher if already wired

let app_data_dir = app.path().app_data_dir()
    .expect("failed to resolve app data dir");

let workspace_repo = Box::new(FsWorkspaceRepo::new(app_data_dir.clone()));

// Shared active path — must be Arc so WorkspaceService and AppState both hold it.
let active_workspace_path: Arc<Mutex<PathBuf>> = Arc::new(Mutex::new(PathBuf::new()));

let workspace_service = WorkspaceService::new(
    workspace_repo,
    Box::new(NullEventPublisher),
    Arc::clone(&active_workspace_path),
);

// Bootstrap: load the active workspace path at startup.
let active_ws = workspace_service.get_active()
    .expect("failed to load active workspace on startup");
*active_workspace_path.lock().unwrap() = active_ws.path.clone();
```

- [ ] **Step 3: Pass `active_workspace_path` and `workspace_service` into the `AppState` constructor**

Adapt to your existing constructor pattern.

- [ ] **Step 4: Verify app compiles**

```bash
cargo build -p rocket (or the src-tauri package name)
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/
git commit -m "feat(workspace): add active_workspace_path and WorkspaceService to AppState"
```

---

### Task 2: Re-point existing services to active_workspace_path

**Files:**
- Modify: `src-tauri/src/` — wherever `FsCollectionRepo`, `FsEnvironmentRepo`, `FsHistoryRepo`, `FsTemplateRepo` are constructed inside commands

Find all construction sites:
```bash
grep -rn "FsCollectionRepo::new\|FsEnvironmentRepo::new\|FsHistoryRepo::new\|FsTemplateRepo::new" src-tauri/src/
```

- [ ] **Step 1: Replace every hardcoded path with `active_workspace_path`**

Each occurrence that looks like:
```rust
let repo = FsCollectionRepo::new(some_fixed_path);
```

Should become:
```rust
let base_dir = state.active_workspace_path.lock().unwrap().clone();
let repo = FsCollectionRepo::new(base_dir);
```

Apply the same pattern for `FsEnvironmentRepo`, `FsHistoryRepo`, `FsTemplateRepo`.

- [ ] **Step 2: Verify app compiles and launches**

```bash
yarn tauri dev
```

Expected: app launches without panic. The collections sidebar should still show collections from the default workspace.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/
git commit -m "feat(workspace): re-point all service constructors to active_workspace_path"
```

---

### Task 3: Register 8 workspace Tauri commands

**Files:**
- Modify: `src-tauri/src/` — wherever commands are defined and registered

- [ ] **Step 1: Add the 8 command functions**

```rust
#[tauri::command]
fn list_workspaces(state: tauri::State<AppState>) -> Result<Vec<Workspace>, String> {
    state.workspace_service.lock().unwrap()
        .list().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_active_workspace(state: tauri::State<AppState>) -> Result<Workspace, String> {
    state.workspace_service.lock().unwrap()
        .get_active().map_err(|e| e.to_string())
}

#[tauri::command]
fn create_workspace(
    name: String,
    path: String,
    state: tauri::State<AppState>,
) -> Result<Workspace, String> {
    state.workspace_service.lock().unwrap()
        .create(&name, PathBuf::from(path))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn switch_workspace(id: String, state: tauri::State<AppState>) -> Result<Workspace, String> {
    state.workspace_service.lock().unwrap()
        .switch(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn rename_workspace(
    id: String,
    new_name: String,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    state.workspace_service.lock().unwrap()
        .rename(&id, &new_name).map_err(|e| e.to_string())
}

#[tauri::command]
fn close_workspace(id: String, state: tauri::State<AppState>) -> Result<(), String> {
    state.workspace_service.lock().unwrap()
        .close(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_workspace(id: String, state: tauri::State<AppState>) -> Result<(), String> {
    state.workspace_service.lock().unwrap()
        .delete(&id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn open_folder_picker(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let folder = app.dialog().file().pick_folder().await;
    Ok(folder.map(|p| p.to_string_lossy().to_string()))
}
```

- [ ] **Step 2: Register all 8 commands in `.invoke_handler(tauri::generate_handler![...])`**

Add to the existing list:
```rust
list_workspaces,
get_active_workspace,
create_workspace,
switch_workspace,
rename_workspace,
close_workspace,
delete_workspace,
open_folder_picker,
```

- [ ] **Step 3: Verify app launches and commands are callable**

```bash
yarn tauri dev
```

Open browser devtools → Console and test a command:
```js
await window.__TAURI__.core.invoke('list_workspaces')
```

Expected: returns an array with at least one workspace (`Default Workspace`).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/
git commit -m "feat(workspace): register 8 workspace Tauri commands including open_folder_picker"
```
