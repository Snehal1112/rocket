# rocket-infra

Provides all filesystem and network implementations for the repository and service traits defined in the domain crates. This is the only crate that does I/O; everything else operates on trait objects.

## Workspace role

`src-tauri` wires these concrete types into the service graph at startup. Domain crates (`rocket-collection`, `rocket-environment`, etc.) define the traits; `rocket-infra` provides the structs that implement them. `rocket-app` services hold `Box<dyn Trait>` and never depend on this crate directly.

## Public types

| Type | Implements | Notes |
|---|---|---|
| `FsCollectionRepo` | `CollectionRepository` | Reads/writes OpenCollection YAML under a given base directory. |
| `SharedPathCollectionRepo` | `CollectionRepository` | Wraps `FsCollectionRepo` behind an `Arc<Mutex<PathBuf>>` so the active workspace can change at runtime without rebuilding the service graph. |
| `FsEnvironmentRepo` | `EnvironmentRepository` | One `.yml` file per environment under `environments/`. |
| `FsHistoryRepo` | `HistoryRepository` | One `.yml` file per history entry under `history/`, sorted newest-first. |
| `FsTemplateRepo` | `TemplateRepository` | Template storage under `templates/`. |
| `FsCookieRepo` | `CookieRepository` | Cookie jar storage under `cookies/`. |
| `FsWorkspaceRepo` | `WorkspaceRepository` | Persists the workspace registry to `workspaces.yml`. Creates a "Default Workspace" on first load. |
| `FsWorkspaceConfigRepo` | `WorkspaceConfigRepository` | Reads/writes per-workspace `workspace.yml` (collections list, description, environment settings). |
| `ReqwestExecutor` | `HttpExecutor` | Executes HTTP requests via `reqwest`. Handles all auth schemes, body types, and AWS SigV4 signing. |
| `NotifyFileWatcher` | — | Wraps the `notify` crate; publishes `DomainEvent::FileChanged` via `EventPublisher` when collection files change. |

## Internal modules (not public API)

- `opencollection` — serde structs mirroring the OpenCollection YAML schema (`OcCollection`, `OcHttpRequest`, `OcFolderInfo`, etc.). Used only for serialization; domain types are used everywhere else.
- `oc_conversions` — bidirectional conversion between domain `Request` and `OcHttpRequest`.
- `migration` — detects and converts legacy JSON collections to OpenCollection YAML on first access; idempotent.

## Key patterns

**Path validation.** Every file path accepted by `FsCollectionRepo` is validated with `validate_path()`, which canonicalizes the nearest existing ancestor and checks that the resolved path stays inside the collection base directory. Path traversal attempts return `DomainError::InvalidInput`.

**On-disk format.** Collections are directories. Each directory contains `opencollection.yml` (metadata + settings), `folder.yml` (subfolder metadata), request files as `.yml`, and `_order.yml` (explicit item ordering). Legacy `.json` request files and the old `.uid` sidecar are auto-migrated on first access.

**UID storage.** UIDs are stored inside `opencollection.yml` and `folder.yml`. The legacy `.uid` file is read as a fallback during migration and then deleted.

**`SharedPathCollectionRepo` pattern.** The active workspace path is held in `Arc<Mutex<PathBuf>>`. Each repository call creates a short-lived `FsCollectionRepo` pointing at `<workspace>/collections`, so switching workspaces only requires updating the shared path.

**OAuth2 client credentials.** `ReqwestExecutor` fetches tokens synchronously as part of `execute()`. Other OAuth2 flows (authorization code, implicit) are not implemented and are silently skipped.

## Testing

All repository types have unit tests using `tempfile::TempDir` for ephemeral filesystem fixtures. `ReqwestExecutor` OAuth2 tests use `wiremock` to mock the token endpoint. Network tests that require live HTTP are marked `#[ignore]`.
