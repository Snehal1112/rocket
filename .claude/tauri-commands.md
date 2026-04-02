# Tauri Command Modules

Commands live in `src-tauri/src/commands/`. They are thin IPC bridges — all business logic lives in `rocket-app` services.

| Module | Purpose |
|---|---|
| `collections.rs` | CRUD for collections, folders, requests |
| `environments.rs` | CRUD for environments |
| `execution.rs` | HTTP request dispatch |
| `git.rs` | Git operations (status, commit, push/pull, branch, stash) |
| `history.rs` | Request history search and clear |
| `workspaces.rs` | Workspace CRUD, pin/unpin, config management |
| `cookies.rs` | Cookie jar management |
| `oauth2.rs` | OAuth2 token acquisition |
| `templates.rs` | Saved request templates |
| `ui_state.rs` | UI layout and mode persistence across sessions |

## Wiring

`src-tauri/src/lib.rs` constructs each `rocket-app` service by injecting concrete `rocket-infra` implementations, then registers them as Tauri managed state. Commands call service methods directly; services never call back into Tauri.

The active workspace path is held in `Arc<Mutex<PathBuf>>` shared across all services. Switching workspaces only requires updating this pointer — no service graph rebuild needed.
