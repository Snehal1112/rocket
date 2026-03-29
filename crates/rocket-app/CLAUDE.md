# rocket-app

Application-layer orchestration crate. It wires together the domain crates
(`rocket-collection`, `rocket-environment`, `rocket-git`, `rocket-history`,
`rocket-http`, `rocket-workspace`) into concrete use-case services, but never
touches the filesystem or any I/O directly — those concerns live in
`rocket-infra`.

## Public Types

| Type | Purpose |
|---|---|
| `CollectionService` | CRUD for collections, requests, and folders; enforces name validation. |
| `CookieService` | Thin wrapper over `CookieRepository` for per-domain cookie jars. |
| `EnvironmentService` | CRUD for environments; publishes `EnvironmentSaved/Deleted` events. |
| `RequestExecutionService` | Core HTTP dispatch: resolves variables, merges collection settings, runs the request, saves history, publishes `RequestExecuted`. |
| `ExecuteRequestInput` | Serialisable input DTO for `RequestExecutionService::execute`. |
| `GitAppService` | Full git workflow (status, stage, commit, push/pull, branch, stash, conflicts) with event publishing. |
| `HistoryService` | List, search, and clear request history. |
| `TemplateService` | CRUD for saved request templates (stored via `rocket-history`). |
| `WorkspaceService` | Create, switch, rename, close, and delete workspaces; mutates the shared `Arc<Mutex<PathBuf>>` active path on switch. |

## Key Patterns

- **Trait-object injection.** Every service takes `Box<dyn SomeRepository>` and `Box<dyn EventPublisher>` via its constructor. No concrete types appear in this crate, making all services fully testable with in-memory mocks.
- **DomainResult everywhere.** All fallible methods return `DomainResult<T>` from `rocket-shared`.
- **Variable resolution in `RequestExecutionService::execute`.** Collection variables are loaded first; environment variables override them. The merged map is passed to `rocket_environment::resolve()` before the HTTP call.
- **Header and auth merging.** Collection-level headers and auth are applied as defaults; request-level values take precedence. Disabled request headers do not suppress collection headers.
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
