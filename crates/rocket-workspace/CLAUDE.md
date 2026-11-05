# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# rocket-workspace

Defines the workspace domain model and the repository traits for persisting workspace state. This crate is intentionally I/O-free — no filesystem or network access lives here.

## Running Tests

```bash
cargo test -p rocket-workspace
cargo test -p rocket-workspace <test_name>   # single test
```

## Key Public Types

### `Workspace`
A single named workspace with a UUID `id`, a display `name`, a `path` pointing to its data directory under `~/.rocket-api/`, an optional `description: Option<String>`, and a `pinned: bool` flag. Constructed with `Workspace::new(name, path)` which generates a UUID automatically. The special `"default"` workspace uses a hard-coded id and is always `pinned = true`.

### `WorkspaceRegistry`
The single persisted document for all workspaces. Holds a `Vec<Workspace>`, an `active_workspace_id`, and a `multi_workspace_mode: bool` flag (default `false`). When `multi_workspace_mode` is `true`, the sidebar shows the workspace section. Created once via `WorkspaceRegistry::new_with_default(path)` on first run. Key methods:

| Method | Purpose |
|---|---|
| `active()` | Returns a reference to the currently active workspace. |
| `find_by_id(id)` | Immutable lookup by id. |
| `find_by_id_mut(id)` | Mutable lookup by id. |
| `name_exists(name, exclude_id)` | Case-insensitive uniqueness check, with optional self-exclusion for renames. |
| `ensure_can_add(name, path)` | Refuses a path that is already registered (`AlreadyExists("This workspace is already open")`, checked first) and a taken name. Callers run it before writing any file. |
| `add(workspace)` | `ensure_can_add`, then push. A refusal leaves the registry unchanged. |
| `rename(id, new_name)` | Validates the name, refuses a name used by another workspace, returns the old name. `NotFound` for a missing id (checked after the duplicate rule). |

A service adds and renames through these methods and does not push to `workspaces` or set `name` itself (D-60).

### `WorkspaceRepository` (trait)
```rust
pub trait WorkspaceRepository: Send + Sync {
    fn load(&self) -> DomainResult<WorkspaceRegistry>;
    fn save(&self, registry: &WorkspaceRegistry) -> DomainResult<()>;
}
```
Defines the persistence contract. The concrete filesystem implementation lives in `rocket-infra`. Services in `rocket-app` hold a `Box<dyn WorkspaceRepository>` and never depend on the concrete type.

### `WorkspaceConfig`
Per-workspace portable configuration file stored as `workspace.yml` inside each workspace directory. Contains:
- `name: String` — workspace display name.
- `description: Option<String>` — optional workspace description.
- `collections: Vec<CollectionReference>` — ordered list of collection references.
- `environments: WorkspaceEnvironmentsConfig` — workspace-level environment settings (active environment name).

Mutation methods: `add_embedded_collection`, `add_external_collection`, `remove_collection`, `has_collection`.

### `WorkspaceEnvironmentsConfig`
Holds `active_environment: Option<String>` — the name of the currently active environment for the workspace. Defaults to `None`. Serialized with `camelCase`.

### `CollectionReference`
Represents a collection belonging to a workspace. `ref_type: CollectionRefType` is either `Embedded` (collection lives inside `workspace/collections/`) or `External` (referenced by absolute `path`). External collections can be shared across workspaces. Note: `ref_type` serializes as `"type"` (serde rename).

### `WorkspaceConfigRepository` (trait)
```rust
pub trait WorkspaceConfigRepository: Send + Sync {
    fn load(&self, workspace_path: &Path) -> DomainResult<WorkspaceConfig>;
    fn save(&self, workspace_path: &Path, config: &WorkspaceConfig) -> DomainResult<()>;
}
```
`load` returns a default config derived from the directory name if `workspace.yml` does not exist — callers must not assume the file is always present. Implemented by `FsWorkspaceConfigRepo` in `rocket-infra`.

## Events

`events.rs` holds `WorkspaceEvent` (created, switched, renamed, closed, deleted, pinned, unpinned, description updated; channels `workspace-created`, `workspace-switched` and so on; implements `PublishedEvent`). Frozen JSON literals in the tests pin the wire format. `WorkspaceService` in `rocket-app` publishes it. The event fields stay snake_case on purpose, as before the split; the tag is camelCase.

## UI state and file watcher ports

- `UiStateRepository` (`ui_state.rs`) loads and saves the window layout (`UiState`, no serde derives). `FsUiStateRepo` in `rocket-infra` keeps it in `ui-state.yml` of the app config folder. The persisted keys stay camelCase because files from earlier versions use them.
- `FileWatcher` (`file_watcher.rs`) starts and stops the watch on a workspace's collections folder. `NotifyFileWatcher` in `rocket-infra` implements it and creates a missing folder on start; `WorkspaceWatcher` in `rocket-app` restarts it and publishes `FileWatcherFailed`.

## Patterns and Conventions

- All fallible methods return `DomainResult<T>` (`Result<T, DomainError>`) from `rocket-shared`.
- Serialization uses `serde` with `rename_all = "camelCase"` so field names match the TypeScript frontend.
- `Workspace::validate_name` is the single validation entry point; call it before persisting any name.
- The `"default"` workspace id is a stable string constant, not a UUID, to allow bootstrapping without an existing registry file.
- All new optional fields use `#[serde(default, skip_serializing_if = ...)]` for backward compatibility with existing registry/config files.

## Relationships

- **Depends on**: `rocket-shared` (error types only).
- **Implemented by**: `rocket-infra` (`FsWorkspaceRepository` reads/writes `workspaces.yml`; `FsWorkspaceConfigRepo` reads/writes per-workspace `workspace.yml`).
- **Consumed by**: `rocket-app` (`WorkspaceService`) and `src-tauri` commands that switch or list workspaces.
- The active workspace path resolved from the registry is stored in `Arc<Mutex<PathBuf>>` in `src-tauri` and threaded into every other service at runtime.
