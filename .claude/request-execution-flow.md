# Request Execution Flow

How an API request flows from the frontend to the HTTP response.

## Architecture Overview

```
Frontend (React/TS) → Tauri IPC → Application Service → HTTP Executor → reqwest
```

Four distinct layers, each with a single responsibility.

## Flow

### 1. Frontend — `src/lib/execute-request.ts`

User clicks "Send". The `sendRequest()` function:
- Loads active environment variables and collection variables.
- Resolves `{{placeholders}}` in URL, headers, query params, body, and auth.
- Builds an `ExecuteRequestInput` object.
- Calls `invoke("execute_request", { input })` via Tauri IPC.

### 2. Tauri Command — `src-tauri/src/commands/execution.rs`

```rust
#[tauri::command]
pub async fn execute_request(
    input: ExecuteRequestInput,
    svc: State<'_, RequestExecutionService>,
) -> Result<HttpResponse, DomainError> {
    svc.execute(input).await
}
```

Receives the JSON-deserialized input and delegates to the service layer.
The `RequestExecutionService` is injected as Tauri managed state.

### 3. Application Service — `crates/rocket-app/src/execution_service.rs`

`RequestExecutionService.execute()` orchestrates the full lifecycle:

1. **Load variables** — collection variables (lower priority), then environment variables (higher priority, overrides collection).
2. **Merge auth/headers** — collection-level defaults are applied when request carries no auth; request headers override collection headers by key.
3. **Resolve placeholders** — `{{VAR}}` in URL and header values replaced from the variable map.
4. **Build `HttpRequest`** — fully resolved struct ready for the HTTP executor.
5. **Execute** — delegates to `HttpExecutor` trait (injected dependency).
6. **Save history** — persists a `HistoryEntry` (non-fatal on failure).
7. **Publish event** — emits `DomainEvent::RequestExecuted`.

### 4. HTTP Executor — `crates/rocket-infra/src/reqwest_executor.rs`

`ReqwestExecutor` implements the `HttpExecutor` trait. Receives a fully-resolved `HttpRequest` and:

1. Builds a `reqwest::Client` with SSL and redirect settings.
2. Merges enabled query params into the URL.
3. Adds enabled headers.
4. Applies auth (Basic, Bearer, API Key, AWS SigV4). OAuth2/Digest/NTLM/WSSE are not yet implemented.
5. Applies body (JSON, XML, Text, FormData, Binary file).
6. Sends the request via reqwest.
7. Extracts status, headers, body, timing.
8. Returns `HttpResponse`.

### 5. Response Flow Back

```
ReqwestExecutor → RequestExecutionService → Tauri command → IPC → Frontend
```

The frontend receives the `HttpResponse` and updates:
- Response pane (status, headers, body, timing).
- Console log.

## Service Initialization — `src-tauri/src/lib.rs`

```rust
let exec_svc = RequestExecutionService::new(
    Box::new(FsEnvironmentRepo::new(environments_dir)),
    Box::new(ReqwestExecutor::new()),
    Box::new(FsHistoryRepo::new(history_dir)),
    Box::new(FsCollectionRepo::new(collections_dir)),
    Box::new(FsCookieRepo::new(cookies_dir)),
    Box::new(NullEventPublisher),
);
app.manage(exec_svc);
```

All dependencies are wired via trait objects for testability.

## Variable Resolution

Variables are resolved in two places:
- **Frontend** resolves what it already has (active environment + collection variables from the store).
- **Backend** resolves again from its own sources (filesystem-backed repos).

This means the backend is the authoritative source. The frontend resolution is for UI preview; the backend resolution produces the final values sent over the wire.

## Key Files

| Layer | File | Purpose |
|-------|------|---------|
| Frontend entry | `src/lib/execute-request.ts` | Build input, invoke Tauri |
| Tauri API types | `src/lib/tauri-api.ts` | TypeScript invoke wrappers |
| Tauri command | `src-tauri/src/commands/execution.rs` | IPC entry point |
| Service | `crates/rocket-app/src/execution_service.rs` | Orchestration, merging, history |
| Executor trait | `crates/rocket-http/src/executor.rs` | `HttpExecutor` interface |
| Executor impl | `crates/rocket-infra/src/reqwest_executor.rs` | reqwest HTTP client |
| Request type | `crates/rocket-http/src/request.rs` | `HttpRequest` struct |
| Response type | `crates/rocket-http/src/response.rs` | `HttpResponse` struct |
| Service setup | `src-tauri/src/lib.rs` | Dependency injection |
