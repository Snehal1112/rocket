# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# rocket-infra

Provides all filesystem and network implementations for the repository and service traits defined in the domain crates. This is the only crate that does I/O; everything else operates on trait objects.

## Commands

```bash
# Run all tests for this crate
cargo test -p rocket-infra

# Run a single test by name (substring match)
cargo test -p rocket-infra settings_roundtrip

# Fast compile check
cargo check -p rocket-infra
```

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
| `FsWorkspaceRepo` | `WorkspaceRepository` | Persists the workspace registry to `workspaces.yml`. Creates a "My Workspace" on first load. |
| `FsWorkspaceConfigRepo` | `WorkspaceConfigRepository` | Reads/writes per-workspace `workspace.yml` (collections list, description, environment settings). |
| `ReqwestExecutor` | `HttpExecutor` | Executes HTTP requests via `reqwest`. Handles all auth schemes, body types, and AWS SigV4 signing. |
| `NotifyFileWatcher` | — | Wraps the `notify` crate; publishes `DomainEvent::FileChanged` via `EventPublisher` when collection files change. |

## Internal modules

These are `pub` in `lib.rs` but are serialization-layer details — callers outside this crate should not depend on them directly.

- `opencollection` — serde structs mirroring the OpenCollection YAML schema (`OcCollection`, `OcHttpRequest`, `OcFolderInfo`, etc.). Used only for serialization; domain types are used everywhere else. Also contains GraphQL, gRPC, and WebSocket structs for schema completeness, but the repo only round-trips `OcHttpRequest` for individual request files — other protocol types land as `OpaqueProtocolItem` in the domain layer.
- `oc_conversions` — bidirectional `From` impls between domain types and `Oc*` serde structs. The boundary layer between persistence and domain.
- `migration` — detects and converts legacy JSON collections to OpenCollection YAML on first access; idempotent.

## Key patterns

**Path validation.** Every file path accepted by `FsCollectionRepo` is validated with `validate_path()`, which canonicalizes the nearest existing ancestor and checks that the resolved path stays inside the collection base directory. Path traversal attempts return `DomainError::InvalidInput`.

**On-disk format.** Collections are directories. Each directory contains `opencollection.yml` (metadata + settings), `folder.yml` (subfolder metadata), request files as `.yml`, and `_order.yml` (explicit item ordering). Legacy `.json` request files and the old `.uid` sidecar are auto-migrated on first access.

**UID storage.** UIDs are stored inside `opencollection.yml` and `folder.yml`. The legacy `.uid` file is read as a fallback during migration and then deleted.

**`SharedPathCollectionRepo` pattern.** The active workspace path is held in `Arc<Mutex<PathBuf>>`. Each repository call creates a short-lived `FsCollectionRepo` pointing at `<workspace>/collections`, so switching workspaces only requires updating the shared path.

**OAuth2 client credentials.** `ReqwestExecutor` fetches tokens synchronously as part of `execute()`. Other OAuth2 flows (authorization code, implicit) are not implemented and are silently skipped.

**`OcAuth` serde design.** `OcAuth` is `#[serde(untagged)]`: the string `"inherit"` deserializes to `OcAuth::Inherit`; an object with a `type` field deserializes to `OcAuth::Typed`. New auth variants must go inside `OcAuthTyped` (tagged by `type`), not as new `OcAuth` variants.

**`OcItem` variant ordering.** The `OcItem` enum uses `#[serde(untagged)]`, so serde tries variants top-to-bottom. More specific types (those with a unique required field) must come before less specific ones — `Http` before `Folder`, etc. Changing variant order breaks deserialization of existing YAML files.

## Testing

All repository types have unit tests using `tempfile::TempDir` for ephemeral filesystem fixtures. `ReqwestExecutor` OAuth2 tests use `wiremock` to mock the token endpoint. Network tests that require live HTTP are marked `#[ignore]`.
