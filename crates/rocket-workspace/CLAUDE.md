# rocket-workspace

Defines the workspace domain model and the repository trait for persisting workspace state. This crate is intentionally I/O-free — no filesystem or network access lives here.

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

### `CollectionReference`
Represents a collection belonging to a workspace. `ref_type: CollectionRefType` is either `Embedded` (collection lives inside `workspace/collections/`) or `External` (referenced by absolute `path`). External collections can be shared across workspaces.

### `WorkspaceConfigRepository` (trait)
```rust
pub trait WorkspaceConfigRepository: Send + Sync {
    fn load(&self, workspace_path: &Path) -> DomainResult<WorkspaceConfig>;
    fn save(&self, workspace_path: &Path, config: &WorkspaceConfig) -> DomainResult<()>;
}
```
Implemented by `FsWorkspaceConfigRepo` in `rocket-infra`.

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
