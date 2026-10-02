# rocket-app

Application-layer orchestration crate. It wires together the domain crates
(`rocket-collection`, `rocket-environment`, `rocket-git`, `rocket-history`,
`rocket-http`, `rocket-workspace`) into concrete use-case services, but never
touches the filesystem or any I/O directly — those concerns live in
`rocket-infra`.

## Public Types

| Type | Purpose |
|---|---|
| `CollectionService` | CRUD for collections, folders, and requests; enforces name validation; publishes a `DomainEvent` for every mutation. |
| `CookieService` | Thin wrapper over `CookieRepository` for per-domain cookie jars. |
| `EnvironmentService` | CRUD for environments; publishes `EnvironmentSaved/Deleted` events. |
| `RequestExecutionService` | Core HTTP dispatch: resolves variables, merges collection settings, runs the request, saves history, publishes `RequestExecuted`. |
| `ExecuteRequestInput` | Serialisable input DTO for `RequestExecutionService::execute`. |
| `CollectionRunnerService` | Runs a folder's/collection's requests in sequence; honours `rok.runner.setNextRequest`/`skipRequest`; publishes `RunnerStarted/StepCompleted/Finished`. |
| `RunCollectionInput` / `RunSummary` | IPC DTOs for `CollectionRunnerService::run`. |
| `GitAppService` | Full git workflow (status, stage, commit, push/pull/fetch, branch, stash, conflicts) with event publishing. |
| `HistoryService` | List, search, and clear request history. |
| `TemplateService` | CRUD for saved request templates (stored via `rocket-history`). |
| `AgentConfigService` | CRUD for ACP agent configs; `resolve_credential` fetches the agent's API key from RocketVault. |
| `AcpSessionService` | Starts/prompts/ends ACP agent sessions via `Box<dyn AcpSessionClient>`; publishes `AcpSessionStarted/Chunk/Finished/Failed` (all chunks before the terminal event); fixed 120s prompt timeout force-kills the session. |
| `WorkspaceService` | Create, switch, rename, close, delete, pin/unpin workspaces; link external collections; toggle multi-workspace mode; mutates the shared `Arc<Mutex<PathBuf>>` active path on switch. |

## Service Method Details

### CollectionService
- Every mutating method publishes a `DomainEvent` (e.g. `CollectionCreated`, `RequestSaved`, `FolderDeleted`, `ItemMoved`, `CollectionSettingsSaved`) after the underlying repository call succeeds.
- `rename_request` — mutates only the `name` field inside the JSON; the filename stays the same, producing a single `Modify` filesystem event.
- `move_item` — moves a request or folder between collections or paths.
- `reorder_items` — reorders items within a folder by supplying the new name order.
- `get_settings` / `save_settings` — read and write per-collection settings (auth, headers, variables).

### WorkspaceService
Constructor takes **two** repos: `WorkspaceRepository` (registry) and `WorkspaceConfigRepository` (`workspace.yml` files).

| Method | Notes |
|---|---|
| `create` | Creates the directory, `collections/`, `environments/` subdirs, and writes `workspace.yml`. |
| `switch` | Updates `active_workspace_id` in registry and mutates `Arc<Mutex<PathBuf>>`. |
| `delete` | Removes the workspace directory from disk via `fs::remove_dir_all`. Cannot delete `"default"` or the last workspace. |
| `close` | Removes from registry only (no disk deletion). Cannot close the last workspace. |
| `pin` / `unpin` | Toggles `workspace.pinned` in the registry. |
| `update_description` | Sets `workspace.description`; pass `None` to clear. |
| `open_workspace` | Registers an existing on-disk workspace; directory must contain `workspace.yml`. |
| `get_workspace_config` | Loads `WorkspaceConfig` from the workspace's `workspace.yml` via `WorkspaceConfigRepository`. |
| `link_external_collection` | Validates `opencollection.yml` presence, reads its `name` field, appends a `CollectionReference` to `WorkspaceConfig`. |
| `get_multi_workspace_mode` / `set_multi_workspace_mode` | Reads/writes `registry.multi_workspace_mode`. |

### GitAppService
Wraps `Box<dyn GitService>`. Every mutating operation publishes a `DomainEvent`. Notable methods:
- `diff_staged` — diff for already-staged files.
- `checkout_remote_branch` — creates a local tracking branch. Rejects an existing local branch of the same name unless `force` is set, which instead resets that branch (and its upstream) to the remote's tip, discarding local commits absent from that remote. A forced reset of the *checked-out* branch still refuses to run over uncommitted tracked changes.
- `fetch` — no event published (read-only remote op).
- `conflicts` — publishes `GitConflictDetected` only when the list is non-empty.
- `abort_merge` — reverts in-progress merge; publishes `GitStatusChanged`.

## Key Patterns

- **Trait-object injection.** Every service takes `Box<dyn SomeRepository>` and `Box<dyn EventPublisher>` via its constructor. No concrete types appear in this crate, making all services fully testable with in-memory mocks.
- **DomainResult everywhere.** All fallible methods return `DomainResult<T>` from `rocket-shared`.
- **Variable resolution in `RequestExecutionService::execute`.** Collection variables are loaded first; environment variables override them. The merged map is passed to `rocket_environment::resolve()` before the HTTP call.
- **Client certificates and RocketVault.** `client_certificates::environment_client_certificates(repo, environment_name, collection_dir, vars, external_secrets)` turns the environment's persisted `ClientCertificate` entries into `ResolvedClientCertificate`s. `RequestExecutionService::resolve_request` and `OAuth2Service` (`resolve_get_token_request_with_secrets`, `refresh_token_with_secrets`) both call it, so they resolve identically. Per entry: `{{placeholders}}` in the domain, file paths and passphrase, then relative paths joined onto the collection folder, then each `certificateSecret`, `privateKeySecret` and `pkcs12Secret` reference looked up in the RocketVault map (`alias.secretName`). A found reference becomes `CertificateSource::Inline` (PKCS12 secrets are base64 and ignore whitespace). A missing, empty, undecodable or over-1-MiB (`MAX_INLINE_SECRET_BYTES`) reference makes the entry `CertificateMaterial::Unavailable { reason }`, which the executor turns into an error only when that entry is selected for the URL. Load tests call `resolve_request` with an empty secrets map, so vault-backed certificates are `Unavailable` there. Multi-line vault values are masked whole and line by line (`redaction::redaction_forms`).
- **Phase-callable execution.** `RequestExecutionService::execute` is a thin composition over `begin_phases` → `run_before_request_phase` → `send_request` → `run_after_response_phase` → `run_tests_phase` → `finish_phases`, all sharing one `PhaseState`. `CollectionRunnerService` drives the same methods one phase at a time so it can act on `skip_request` before the send and on `next_request` after every phase. Do not add phase logic to only one caller.
- **Header and auth merging.** Collection-level headers and auth are applied as defaults; request-level values take precedence. A *disabled* request header does not suppress the collection header with the same key.
- **Non-fatal history write.** `let _ = self.history_repo.save(...)` is intentional — a history persistence failure must not abort the response.
- **Event publishing is fire-and-forget.** Services publish `DomainEvent` variants after successful operations; errors from downstream listeners are not propagated back.
- **Tests use inline mocks.** Each service module contains its own mock implementations in `#[cfg(test)]`. `tempfile` is used in `WorkspaceService` tests that require real directories.

## Workspace Position

```
src-tauri (Tauri commands)
    └── rocket-app  ← this crate
            ├── rocket-collection
            ├── rocket-environment
            ├── rocket-git
            ├── rocket-history
            ├── rocket-http
            ├── rocket-workspace
            └── rocket-shared
```

`src-tauri/src/lib.rs` constructs each service by injecting concrete `rocket-infra` implementations, then stores them in Tauri managed state. Tauri commands call service methods directly; services never call back into Tauri.
