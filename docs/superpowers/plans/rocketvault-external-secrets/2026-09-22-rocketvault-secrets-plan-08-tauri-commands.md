# RocketVault Secrets Plan 08: Tauri Commands and Service Wiring — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose `SecretManagerService` (Plan 05) to the frontend via five
Tauri commands, and wire the concrete RocketVault stack (`FsSecretManagerRepo`,
`KeyringSecretStore::new_vault_connections()`, `ReqwestVaultSecretFetcher`)
into `src-tauri/src/lib.rs`'s startup sequence, including passing it into
`RequestExecutionService::new_with_audit(...)`'s three new trailing
constructor parameters (Plan 06 — as actually implemented, this is **not** a
separate builder method; the three vault dependencies are required
constructor arguments, appended after `audit`, matching every other
`RequestExecutionService::new`/`new_with_audit` call site across the
workspace).

**Architecture:** `SecretManagerService`'s methods all take `&self` — its
three fields are trait objects/`Arc`s with no interior mutable state of their
own (mutation happens via file I/O in `FsSecretManagerRepo` and the OS
keychain, not in-memory), so unlike `WorkspaceService` (managed as
`Mutex<WorkspaceService>`), this service is managed **directly**, no `Mutex`
wrapper needed — this also sidesteps holding a lock guard across an `.await`
point for the two async commands, which this codebase has no existing
precedent for doing safely with `std::sync::Mutex`.

**Task count:** This plan is 2 tasks, not the usual 3-task budget for this
series (see `00-plan-index.md`'s breakdown table). The work only splits into
two genuine slices: the command/DTO layer (Task 1) and the startup-wiring
layer (Task 2) — both are already atomic, testable deliverables on their own,
and there's no third natural cut that isn't an arbitrary half of one of them
(matches Plan 07's identical reasoning for its own 2-task count).

**Tech Stack:** Rust, `tauri` (commands, managed state).

**Spec:** `docs/superpowers/specs/2026-09-22-rocketvault-external-secrets-spec.md`
(§4.3, §5). Plan index: `docs/superpowers/plans/rocketvault-external-secrets/00-plan-index.md`.
Previous plans: [Plan 03](2026-09-22-rocketvault-secrets-plan-03-wire-client.md),
[Plan 04](2026-09-22-rocketvault-secrets-plan-04-persistence.md),
[Plan 05](2026-09-22-rocketvault-secrets-plan-05-secret-manager-service.md),
[Plan 06](2026-09-22-rocketvault-secrets-plan-06-execution-service.md).

## Global Constraints

- Per this repo's hard rule ("Serde: `#[serde(rename_all = "camelCase")]` on
  IPC DTOs only — never on persistence structs"), define IPC-facing DTOs
  distinct from the domain `SecretManagerConnection`/`ExternalSecretRef`
  (Plan 01, which deliberately have no such rename on `SecretManagerConnection`
  since it's a persistence struct — `ExternalSecretRef` already has the
  rename because it nests inside `Environment`, an existing IPC-facing type;
  this plan's DTO for it is a thin re-export-with-rename regardless, kept
  separate on principle so the Tauri command layer never directly leaks a
  domain type's derive attributes).
- `SecretManagerService` is managed directly (`app.manage(secret_manager_svc)`),
  not `Mutex`-wrapped — see Architecture above. Commands access it via
  `State<'_, SecretManagerService>`.
- `RequestExecutionService::new_with_audit(...)` (Plan 06) gained three new
  **required, trailing** constructor parameters, in this fixed order:
  `secret_manager_repo: Box<dyn SecretManagerRepository>`, then
  `vault_connection_secret_store: Arc<dyn SecretStore>`, then
  `vault_fetcher: Arc<dyn VaultSecretFetcher>` — appended after `audit:
  Arc<dyn SecurityAuditPublisher>`, the existing last parameter. This is not a
  separate builder call; pass them directly into the existing
  `new_with_audit(...)` call.
- `SecretManagerService::new(repo: Box<dyn SecretManagerRepository>, secret_store: Arc<dyn SecretStore>, fetcher: Arc<dyn VaultSecretFetcher>) -> Self`,
  `list`/`save`/`delete` (sync), `test_connection`/`fetch_secret_names` (async)
  — Plan 05.
- `FsSecretManagerRepo::new(path: PathBuf) -> Self` (Plan 04) — belongs under
  the same app data directory as other app-level (non-workspace) persistence
  (`dirs::home_dir().join(".rocket-api")`, confirmed as the existing base
  directory pattern by reading `src-tauri/src/lib.rs`), sibling to
  `history`/`templates`/`cookies`: `data_dir.join("secret_managers.yml")`.
- `KeyringSecretStore::new_vault_connections()` / `ReqwestVaultSecretFetcher::new()`
  — Plans 03/04.
- Test code uses `.expect("message")` for fallible calls, not the bare
  panicking shorthand.

---

## Task 1: Command module + DTOs

**Files:**
- Create: `src-tauri/src/commands/secret_managers.rs`
- Modify: `src-tauri/src/lib.rs` (module declaration + `invoke_handler!` registration)

**Interfaces:**
- Consumes: `SecretManagerService` (Plan 05), `SecretManagerConnection`,
  `ExternalSecretRef` (Plan 01).
- Produces: `SecretManagerConnectionDto`, `ExternalSecretRefDto`, and five
  `#[tauri::command]` functions — consumed by Plan 09 (frontend wrapper
  functions call these by name).

- [ ] **Step 1: Write the failing tests**

This module's commands are thin — the existing convention in this codebase
(confirmed by reading `src-tauri/src/commands/environments.rs` in full: no
`#[cfg(test)]` module in that file at all) is that Tauri command *modules*
are not unit-tested directly; the services underneath them (already tested in
Plans 04/05) carry the test coverage. This task's "test" is therefore a
compile-and-manual-invoke check, matching that established convention — write
the DTOs and commands in Step 3 below, then verify with `cargo check -p
src-tauri` in Step 2, rather than writing a new test file this codebase has
no precedent for at this layer.

- [ ] **Step 2: Run the compile check to confirm the module doesn't exist yet**

Run: `cargo check -p src-tauri`
Expected: FAIL — `secret_managers` module and its types don't exist yet (this
step exists only to confirm the starting state; there's no red/green test
cycle for this task per the note in Step 1).

- [ ] **Step 3: Implement the DTOs and commands**

```rust
// src-tauri/src/commands/secret_managers.rs
use rocket_app::SecretManagerService;
use rocket_environment::secret_manager::SecretManagerConnection;
use rocket_environment::external_secret::ExternalSecretRef;
use rocket_shared::error::DomainError;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretManagerConnectionDto {
    pub id: String,
    pub label: String,
    pub base_url: String,
    pub client_id: String,
    pub verify_ssl: bool,
    pub allow_insecure_http: bool,
}

impl From<SecretManagerConnection> for SecretManagerConnectionDto {
    fn from(c: SecretManagerConnection) -> Self {
        Self {
            id: c.id,
            label: c.label,
            base_url: c.base_url,
            client_id: c.client_id,
            verify_ssl: c.verify_ssl,
            allow_insecure_http: c.allow_insecure_http,
        }
    }
}

impl From<SecretManagerConnectionDto> for SecretManagerConnection {
    fn from(dto: SecretManagerConnectionDto) -> Self {
        Self {
            id: dto.id,
            label: dto.label,
            base_url: dto.base_url,
            client_id: dto.client_id,
            verify_ssl: dto.verify_ssl,
            allow_insecure_http: dto.allow_insecure_http,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalSecretRefDto {
    pub name: String,
    pub secret_id: String,
}

impl From<ExternalSecretRef> for ExternalSecretRefDto {
    fn from(r: ExternalSecretRef) -> Self {
        Self {
            name: r.name,
            secret_id: r.secret_id,
        }
    }
}

#[tauri::command]
pub fn list_secret_manager_connections(
    svc: State<'_, SecretManagerService>,
) -> Result<Vec<SecretManagerConnectionDto>, DomainError> {
    Ok(svc.list()?.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub fn save_secret_manager_connection(
    connection: SecretManagerConnectionDto,
    client_secret: Option<String>,
    svc: State<'_, SecretManagerService>,
) -> Result<(), DomainError> {
    svc.save(connection.into(), client_secret)
}

#[tauri::command]
pub fn delete_secret_manager_connection(
    id: String,
    svc: State<'_, SecretManagerService>,
) -> Result<(), DomainError> {
    svc.delete(&id)
}

#[tauri::command]
pub async fn test_secret_manager_connection(
    id: String,
    vault_name: String,
    svc: State<'_, SecretManagerService>,
) -> Result<(), DomainError> {
    svc.test_connection(&id, &vault_name).await
}

#[tauri::command]
pub async fn fetch_external_secret_names(
    id: String,
    vault_name: String,
    svc: State<'_, SecretManagerService>,
) -> Result<Vec<ExternalSecretRefDto>, DomainError> {
    Ok(svc
        .fetch_secret_names(&id, &vault_name)
        .await?
        .into_iter()
        .map(Into::into)
        .collect())
}
```

(`State<'_, SecretManagerService>` — not `State<'_, Mutex<SecretManagerService>>`
— works directly here because `SecretManagerService` requires no interior
mutability; every async command above only holds an immutable `&SecretManagerService`
across its `.await`, which is always safe regardless of what's being awaited,
unlike holding a `std::sync::Mutex` guard across an await point.)

- [ ] **Step 4: Register the module**

Find how `src-tauri/src/commands/environments.rs` and its sibling command
modules are declared and registered (check `src-tauri/src/commands/mod.rs` if
it exists, or the module declarations directly in `src-tauri/src/lib.rs`, and
the `tauri::generate_handler![...]` / `invoke_handler` macro list) — add
`pub mod secret_managers;` alongside the existing command module
declarations, and add all five new command function paths
(`commands::secret_managers::list_secret_manager_connections`, etc.) to the
handler list, following the exact existing entries' naming convention (e.g.
`commands::environments::list_environments` already there).

- [ ] **Step 5: Run the compile check**

Run: `cargo check -p src-tauri`
Expected: PASS — a clean compile. `State<'_, SecretManagerService>` in a
command's signature type-checks on its own; Tauri only resolves *at runtime*
whether that type was actually handed to `.manage(...)`, so the fact that
`SecretManagerService` isn't managed yet (Task 2's job) has no effect on
compilation. Do not attempt to run the app yet — invoking one of these
commands before Task 2 lands would panic at runtime with a missing-managed-
state error, which is expected and out of scope for this step.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/secret_managers.rs src-tauri/src/lib.rs
git commit -m "feat(tauri): add secret manager connection commands"
```

---

## Task 2: `lib.rs` startup wiring

**Files:**
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `FsSecretManagerRepo` (Plan 04), `KeyringSecretStore::new_vault_connections()`
  (Plan 04), `ReqwestVaultSecretFetcher::new()` (Plan 03),
  `SecretManagerService::new(...)` (Plan 05),
  `RequestExecutionService::new_with_audit(...)`'s three new trailing
  parameters (Plan 06).
- Produces: the fully wired app — no new Rust symbol, this task only changes
  `src-tauri/src/lib.rs`'s setup closure.

- [ ] **Step 1: Construct the RocketVault stack once, near the other app-level services**

In `src-tauri/src/lib.rs`, find where `data_dir` (the `~/.rocket-api`
directory, confirmed as the existing pattern by reading this file earlier in
this conversation) and the other app-level repos (`FsHistoryRepo`,
`FsTemplateRepo`, `FsCookieRepo`) are constructed, and add alongside them:

```rust
let vault_connection_secret_store: Arc<dyn rocket_environment::secret_store::SecretStore> =
    Arc::new(rocket_infra::KeyringSecretStore::new_vault_connections());
let vault_fetcher: Arc<dyn rocket_environment::vault_secret_fetcher::VaultSecretFetcher> =
    Arc::new(rocket_infra::ReqwestVaultSecretFetcher::new());
```

`SecretManagerRepository` isn't `Clone`, and both `SecretManagerService` and
`RequestExecutionService::new_with_audit(...)` need their own owned
`Box<dyn SecretManagerRepository>` (not a shared `Arc`, per Plan 05's and
Plan 06's exact signatures — confirmed by reading both plan files' locked
signatures above). Don't declare one shared `FsSecretManagerRepo` and try to
hand it to both — construct it inline, twice, once per consumer (each
instance is a cheap, stateless-except-for-the-path struct pointed at the same
`secret_managers.yml`, so this is not wasteful):

```rust
let secret_manager_svc = rocket_app::SecretManagerService::new(
    Box::new(rocket_infra::FsSecretManagerRepo::new(data_dir.join("secret_managers.yml"))),
    Arc::clone(&vault_connection_secret_store),
    Arc::clone(&vault_fetcher),
);
```

(The second inline `Box::new(FsSecretManagerRepo::new(data_dir.join("secret_managers.yml")))`
call goes into the `exec_svc` construction in Step 2 below — two small,
independent instances, not one shared one.)

- [ ] **Step 2: Pass the vault stack into `RequestExecutionService::new_with_audit(...)`**

Find the existing `exec_svc` construction (`RequestExecutionService::new_with_audit(...)`,
already followed by `.with_script_engine(...)` and
`.with_collection_env_repo_factory(...)` per this file's existing chain) and
append the three new arguments to the constructor call itself — Plan 06 added
them as required trailing constructor parameters, not a separate builder
method, so they go inside the `new_with_audit(...)` parens, after
`audit_publisher.clone()`, in the exact order Plan 06 fixed
(`secret_manager_repo`, then `vault_connection_secret_store`, then
`vault_fetcher`). The two existing `.with_script_engine(...)`/
`.with_collection_env_repo_factory(...)` builder calls stay exactly as they
are, chained after the now-longer constructor call:

```rust
let exec_svc = RequestExecutionService::new_with_audit(
    Box::new(FsEnvironmentRepo::with_secret_store(
        environments_dir.clone(),
        env_secret_store(),
    )),
    Arc::clone(&executor),
    Box::new(FsHistoryRepo::new(history_dir)),
    Box::new(FsCollectionRepo::new_standalone(collections_dir.clone())),
    Box::new(FsCookieRepo::new(cookies_dir)),
    Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
    audit_publisher.clone(),
    Box::new(rocket_infra::FsSecretManagerRepo::new(data_dir.join("secret_managers.yml"))),
    Arc::clone(&vault_connection_secret_store),
    Arc::clone(&vault_fetcher),
)
.with_script_engine(Box::new(DenoScriptEngine::new()))
.with_collection_env_repo_factory(Box::new(
    SharedCollectionEnvironmentRepo::new(Arc::clone(&active_workspace_path)),
));
```

`CollectionRunnerService`'s own construction is unchanged — Plan 07 confirmed
it takes no new constructor parameters (it receives `exec: &RequestExecutionService`
as a call parameter at `run()` time, already wired through `exec_svc` above).

- [ ] **Step 3: Register `secret_manager_svc` as managed state**

Alongside the existing `app.manage(...)` calls:

```rust
app.manage(secret_manager_svc);
```

(Directly, not `Mutex`-wrapped — see this plan's Architecture section.)

- [ ] **Step 4: Compile and manual verification**

Run: `cargo check --workspace`
Expected: clean compile across every crate, including `src-tauri`.

Then, per this repo's UI/feature verification convention for Tauri-layer
changes (`yarn tauri dev`), manually confirm: the app starts without a panic
from the new managed state, and a call to `list_secret_manager_connections`
from the frontend dev console (or once Plan 09's UI exists) returns an empty
list on a fresh `~/.rocket-api/` with no `secret_managers.yml` yet (proving
`FsSecretManagerRepo`'s "missing file → empty list" behavior from Plan 04
holds through the full stack, not just its own unit tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(tauri): wire RocketVault secret manager stack into app startup"
```

---

## Milestone Checklist — Plan 08

- [ ] Five Tauri commands: list/save/delete/test_connection/fetch_external_secret_names — camelCase DTOs, domain types never cross the IPC boundary directly
- [ ] `SecretManagerService` managed directly (no `Mutex`) — both async commands hold only an immutable reference across their `.await`
- [ ] `RequestExecutionService::new_with_audit(...)` call in `lib.rs` gains the three new trailing arguments (`secret_manager_repo`, `vault_connection_secret_store`, `vault_fetcher`), with `.with_script_engine(...)`/`.with_collection_env_repo_factory(...)` still chained after it unchanged
- [ ] `cargo check --workspace` — clean compile
- [ ] Manual `yarn tauri dev` smoke check: app starts, `list_secret_manager_connections` returns an empty list on a fresh install

## Next Plan

[Plan 09: Frontend types + Secret Manager Connections UI](2026-09-22-rocketvault-secrets-plan-09-frontend-connections.md) —
consumes the five Tauri commands from this plan.
