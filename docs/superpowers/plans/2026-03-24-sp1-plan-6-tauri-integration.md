# SP1 Plan 6: Tauri Integration + Frontend Migration

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the Tauri 2.0 binary — create the Tauri app shell, register all commands, connect app services via managed state, and migrate the React frontend from HTTP fetch to Tauri invoke.

**Architecture:** src-tauri is a thin binary that creates infrastructure implementations, injects them into app services, registers services as Tauri managed state, and maps each `#[tauri::command]` to an app service method call (1-3 lines each).

**Tech Stack:** Tauri 2.0, tauri-plugin-fs, tauri-plugin-dialog, tauri-plugin-notification, React, TypeScript, @tauri-apps/api

---

## File Structure

```
src-tauri/
  Cargo.toml
  tauri.conf.json
  build.rs
  src/
    main.rs
    lib.rs                        # Tauri app setup + command registration
    commands/
      mod.rs
      collections.rs              # Collection commands (delegates to CollectionService)
      environments.rs             # Environment commands
      execution.rs                # Execute request command
      history.rs                  # History commands
      templates.rs                # Template commands
      cookies.rs                  # Cookie commands
      app.rs                      # App utility commands (data dir, file watcher)
    tauri_event_bus.rs            # EventPublisher impl → Tauri emit

frontend/
  src/
    lib/
      tauri-api.ts                # TypeScript bridge — invoke() wrappers
      api.ts                      # Re-exports from tauri-api.ts (preserves imports)
    features/
      realtime/
        hooks/useRealtimeSync.ts  # Replaced: WebSocket → Tauri listen()
```

---

## Chunk 1: Tauri binary scaffold

### Task 1: src-tauri Cargo.toml + config + entry points

**Files:**
- Create: `src-tauri/Cargo.toml`
- Create: `src-tauri/tauri.conf.json`
- Create: `src-tauri/build.rs`
- Create: `src-tauri/src/main.rs`

- [ ] **Step 1: Create src-tauri/Cargo.toml**

```toml
[package]
name = "rocket-api-tauri"
version.workspace = true
edition.workspace = true

[lib]
name = "rocket_api_lib"
crate-type = ["lib", "cdylib", "staticlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
rocket-shared.workspace = true
rocket-collection.workspace = true
rocket-environment.workspace = true
rocket-http.workspace = true
rocket-history.workspace = true
rocket-app.workspace = true
rocket-infra.workspace = true

tauri = { version = "2", features = [] }
tauri-plugin-shell = "2"
tauri-plugin-fs = "2"
tauri-plugin-dialog = "2"
tauri-plugin-notification = "2"
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
log.workspace = true
env_logger.workspace = true
dirs.workspace = true
```

- [ ] **Step 2: Create build.rs, main.rs, tauri.conf.json**

(Same as Plan 1 Task 1 Steps 2, 4, 5 — refer to those for exact content.)

- [ ] **Step 3: Verify compilation**

```bash
cd src-tauri && cargo check
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/build.rs src-tauri/src/main.rs src-tauri/tauri.conf.json
git commit -m "feat(tauri): binary scaffold"
```

---

### Task 2: TauriEventBus — EventPublisher → Tauri emit

**Files:**
- Create: `src-tauri/src/tauri_event_bus.rs`

- [ ] **Step 1: Implement TauriEventBus**

```rust
use rocket_shared::events::{DomainEvent, EventPublisher};
use tauri::{AppHandle, Emitter};

pub struct TauriEventBus {
    app: AppHandle,
}

impl TauriEventBus {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl EventPublisher for TauriEventBus {
    fn publish(&self, event: DomainEvent) {
        let event_name = match &event {
            DomainEvent::FileChanged { .. } => "file-change",
            DomainEvent::RequestExecuted { .. } => "request-executed",
            DomainEvent::CollectionCreated { .. } | DomainEvent::CollectionDeleted { .. } | DomainEvent::CollectionRenamed { .. } => "collection-changed",
            _ => "domain-event",
        };
        let _ = self.app.emit(event_name, &event);
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/tauri_event_bus.rs
git commit -m "feat(tauri): TauriEventBus — domain events → frontend"
```

---

### Task 3: Tauri commands — thin delegates to app services

**Files:**
- Create: `src-tauri/src/commands/mod.rs`
- Create: `src-tauri/src/commands/collections.rs`
- Create: `src-tauri/src/commands/environments.rs`
- Create: `src-tauri/src/commands/execution.rs`
- Create: `src-tauri/src/commands/history.rs`
- Create: `src-tauri/src/commands/templates.rs`
- Create: `src-tauri/src/commands/cookies.rs`
- Create: `src-tauri/src/commands/app.rs`

- [ ] **Step 1: Create commands module**

Each command is 1-3 lines. Example pattern:

```rust
// commands/collections.rs
use rocket_app::CollectionService;
use rocket_collection::{Collection, CollectionSummary, Request};
use rocket_shared::error::DomainError;
use tauri::State;

#[tauri::command]
pub fn list_collections(
    svc: State<'_, CollectionService>,
) -> Result<Vec<CollectionSummary>, DomainError> {
    svc.list()
}

#[tauri::command]
pub fn create_collection(
    name: String,
    svc: State<'_, CollectionService>,
) -> Result<Collection, DomainError> {
    svc.create(&name)
}
// ... etc for each method
```

For the async execute command:
```rust
// commands/execution.rs
#[tauri::command]
pub async fn execute_request(
    input: ExecuteRequestInput,
    svc: State<'_, RequestExecutionService>,
) -> Result<HttpResponse, DomainError> {
    svc.execute(input).await
}
```

- [ ] **Step 2: Implement all command modules following the pattern**

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/
git commit -m "feat(tauri): all command handlers — thin delegates to app services"
```

---

### Task 4: lib.rs — Tauri setup + DI wiring

**Files:**
- Create: `src-tauri/src/lib.rs`

- [ ] **Step 1: Wire everything together**

```rust
mod commands;
mod tauri_event_bus;

use rocket_app::*;
use rocket_infra::*;
use tauri::Manager;
use std::sync::Arc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let app_handle = app.handle().clone();

            // Determine data directory
            let home = dirs::home_dir().expect("Home directory not found");
            let data_dir = home.join(".rocket-api");
            let collections_dir = data_dir.join("collections");
            let environments_dir = data_dir.join("environments");
            let history_dir = data_dir.join("history");
            let templates_dir = data_dir.join("templates");
            let cookies_dir = data_dir.join("cookies");

            // Ensure directories exist
            for dir in [&data_dir, &collections_dir, &environments_dir, &history_dir, &templates_dir, &cookies_dir] {
                std::fs::create_dir_all(dir).ok();
            }

            // Create event bus
            let event_bus = Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone()));

            // Create infrastructure implementations
            let collection_repo = Box::new(FsCollectionRepo::new(collections_dir.clone()));
            let env_repo = Box::new(FsEnvironmentRepo::new(environments_dir));
            let history_repo = Box::new(FsHistoryRepo::new(history_dir));
            let template_repo = Box::new(FsTemplateRepo::new(templates_dir));
            let cookie_repo = Box::new(FsCookieRepo::new(cookies_dir));
            let executor = Box::new(ReqwestExecutor::new());

            // For execution service, we need separate repo instances
            let env_repo_2 = Box::new(FsEnvironmentRepo::new(data_dir.join("environments")));
            let history_repo_2 = Box::new(FsHistoryRepo::new(data_dir.join("history")));
            let cookie_repo_2 = Box::new(FsCookieRepo::new(data_dir.join("cookies")));
            let event_bus_2 = Box::new(rocket_shared::events::NullEventPublisher); // or clone pattern

            // Create application services
            let collection_svc = CollectionService::new(collection_repo, event_bus);
            let env_svc = EnvironmentService::new(env_repo, Box::new(rocket_shared::events::NullEventPublisher));
            let history_svc = HistoryService::new(history_repo, Box::new(rocket_shared::events::NullEventPublisher));
            let template_svc = TemplateService::new(template_repo, Box::new(rocket_shared::events::NullEventPublisher));
            let cookie_svc = CookieService::new(cookie_repo, Box::new(rocket_shared::events::NullEventPublisher));
            let exec_svc = RequestExecutionService::new(env_repo_2, executor, history_repo_2, cookie_repo_2, event_bus_2);

            // Register as Tauri managed state
            app.manage(collection_svc);
            app.manage(env_svc);
            app.manage(history_svc);
            app.manage(template_svc);
            app.manage(cookie_svc);
            app.manage(exec_svc);

            // Start file watcher
            let watcher = NotifyFileWatcher::new();
            let watcher_publisher = Arc::new(rocket_shared::events::NullEventPublisher); // TODO: use TauriEventBus
            let _ = watcher.start(collections_dir, watcher_publisher);
            app.manage(watcher);

            log::info!("RocketAPI initialized at {:?}", data_dir);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::collections::list_collections,
            commands::collections::get_collection,
            commands::collections::create_collection,
            commands::collections::delete_collection,
            commands::collections::rename_collection,
            commands::collections::save_request,
            commands::collections::delete_request,
            commands::collections::create_folder,
            commands::collections::delete_folder,
            commands::collections::move_item,
            commands::environments::list_environments,
            commands::environments::get_environment,
            commands::environments::save_environment,
            commands::environments::delete_environment,
            commands::execution::execute_request,
            commands::history::list_history,
            commands::history::get_history_entry,
            commands::history::clear_history,
            commands::templates::list_templates,
            commands::templates::save_template,
            commands::templates::delete_template,
            commands::cookies::get_cookies,
            commands::cookies::set_cookies,
            commands::cookies::clear_cookies,
            commands::app::get_app_data_dir,
            commands::app::watch_collections,
            commands::app::stop_watching,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 2: Verify Tauri compilation**

```bash
cd src-tauri && cargo check
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(tauri): full DI wiring — infra → app services → managed state"
```

---

## Chunk 2: Frontend migration

### Task 5: TypeScript Tauri API bridge

**Files:**
- Create: `frontend/src/lib/tauri-api.ts`

- [ ] **Step 1: Install Tauri frontend deps**

```bash
cd frontend && npm install @tauri-apps/api @tauri-apps/plugin-fs @tauri-apps/plugin-dialog @tauri-apps/plugin-notification @tauri-apps/plugin-shell
npm install -D @tauri-apps/cli
```

- [ ] **Step 2: Create tauri-api.ts**

Full TypeScript bridge with types matching Rust structs. Every exported function wraps `invoke('command_name', { args })`. Include `onFileChange()` using `listen('file-change')`.

(Refer to the previous plan's Task 7 for the complete file content.)

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/tauri-api.ts frontend/package.json
git commit -m "feat(frontend): Tauri API bridge — TypeScript invoke wrappers"
```

---

### Task 6: Replace api.ts + useRealtimeSync

**Files:**
- Modify: `frontend/src/lib/api.ts`
- Modify: `frontend/src/features/realtime/hooks/useRealtimeSync.ts`
- Modify: `frontend/vite.config.ts`

- [ ] **Step 1: Replace api.ts with re-exports from tauri-api.ts**

- [ ] **Step 2: Replace WebSocket listener with Tauri event listener**

- [ ] **Step 3: Update vite.config.ts for Tauri dev server**

- [ ] **Step 4: Verify no remaining fetch() calls to Go backend**

```bash
grep -rn "localhost:8080\|/api/v1" frontend/src/ --include="*.ts" --include="*.tsx"
```
Expected: no results.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/
git commit -m "feat(frontend): replace HTTP fetch + WebSocket with Tauri IPC"
```

---

## Chunk 3: End-to-end verification

### Task 7: Build and smoke test

- [ ] **Step 1: Run Tauri dev mode**

```bash
cargo tauri dev
```

- [ ] **Step 2: Verify each feature**

- [ ] App window opens with React frontend
- [ ] Collections sidebar loads from `~/.rocket-api/collections/`
- [ ] Create new collection → appears in sidebar
- [ ] Create request inside collection → saves to disk as JSON
- [ ] Execute GET https://httpbin.org/get → response panel shows status, body, timing
- [ ] Headers/body/auth editing → saves correctly
- [ ] History records the request
- [ ] Environment CRUD works
- [ ] Template CRUD works
- [ ] External file change → UI refreshes
- [ ] Rename/delete collection works
- [ ] Move request between folders works

- [ ] **Step 3: Fix any serialization mismatches**

Common issues: camelCase field names, Option<T> as null vs undefined, array wrapping.

- [ ] **Step 4: Build production binary**

```bash
cargo tauri build
```
Expected: produces installer in `src-tauri/target/release/bundle/`.

- [ ] **Step 5: Commit fixes**

```bash
git add -A
git commit -m "fix: end-to-end smoke test fixes"
```

---

### Task 8: Archive Go backend + update README

- [ ] **Step 1: Archive Go backend**

```bash
mv backend backend-legacy
git add -A
git commit -m "chore: archive Go backend (replaced by Tauri Rust)"
```

- [ ] **Step 2: Update README with Tauri dev instructions**

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: update README for Tauri 2.0"
```

---

## Milestone Checklist — Plan 6 (Final SP1 milestone)

- [ ] Tauri app compiles and launches
- [ ] All commands registered and delegating to app services
- [ ] DI wiring: infra repos → app services → Tauri state
- [ ] Frontend uses invoke() instead of fetch()
- [ ] File watcher emits events to frontend
- [ ] All 12 smoke test scenarios pass
- [ ] Production build produces installer
- [ ] Go backend archived
- [ ] README updated
- [ ] Full workspace: `cargo test --workspace` — all pass
- [ ] Full workspace: `cargo clippy --workspace` — no warnings
