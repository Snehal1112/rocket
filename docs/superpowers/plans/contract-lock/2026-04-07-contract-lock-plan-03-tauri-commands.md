# Contract Lock — Plan 03: Tauri IPC Commands

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose `ContractService` to the frontend via five Tauri commands and wire the save hook into the existing request save command.

**Architecture:** New `src-tauri/src/commands/contract.rs` for all contract commands. `ContractService` is managed as Tauri state. The existing `save_request` command gains one additional call to `contract_service.on_request_saved()` — no other changes to save_request.

**Tech Stack:** Rust, Tauri v2, `rocket-app`, `rocket-collection`

**Depends on:** Plan 02 merged.

---

## File Map

| File | Action |
|---|---|
| `src-tauri/src/commands/contract.rs` | Create — 5 IPC commands |
| `src-tauri/src/commands/mod.rs` | Modify — export `contract` |
| `src-tauri/src/lib.rs` | Modify — manage `ContractService` state + register commands + wire save hook |

---

## Task 1: Contract commands

**Files:**
- Create: `src-tauri/src/commands/contract.rs`

- [ ] **Step 1: Create `contract.rs`**

```rust
use rocket_app::ContractService;
use rocket_collection::contract::{
    snapshot::RequestSignatureSnapshot,
    changelog::ContractChangelog,
    types::Contract,
};
use std::path::PathBuf;
use tauri::State;
use ulid::Ulid;

/// Input struct for attaching a contract.
/// Matches the frontend form fields exactly.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachContractInput {
    pub title: String,
    pub provider: String,
    pub consumer: String,
    pub project: String,
    pub version: String,
    pub effective_date: String,          // "YYYY-MM-DD"
    pub expiry_date: Option<String>,     // "YYYY-MM-DD" or null
    pub document_path: Option<PathBuf>,
    pub scope: rocket_collection::contract::types::ContractScope,
    pub initial_snapshots: Vec<RequestSignatureSnapshot>,
}

#[tauri::command]
pub fn attach_contract(
    collection_root: String,
    input: AttachContractInput,
    svc: State<ContractService>,
) -> Result<Contract, String> {
    let root = PathBuf::from(&collection_root);

    let effective_date = chrono::NaiveDate::parse_from_str(&input.effective_date, "%Y-%m-%d")
        .map_err(|e| e.to_string())?;

    let expiry_date = input.expiry_date
        .as_deref()
        .map(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d"))
        .transpose()
        .map_err(|e| e.to_string())?;

    let contract = rocket_collection::contract::types::Contract {
        id: ulid::Ulid::new(),   // overwritten by service
        title: input.title,
        provider: input.provider,
        consumer: input.consumer,
        project: input.project,
        version: input.version,
        effective_date,
        expiry_date,
        document_path: input.document_path,
        enforcement_mode: rocket_collection::contract::types::ContractEnforcementMode::Informational,
        scope: input.scope,
    };

    svc.attach_contract(&root, contract, input.initial_snapshots)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_contracts(
    collection_root: String,
    svc: State<ContractService>,
) -> Result<Vec<Contract>, String> {
    svc.list_contracts(&PathBuf::from(&collection_root))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_contract(
    collection_root: String,
    contract_id: String,
    svc: State<ContractService>,
) -> Result<Contract, String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.get_contract(&PathBuf::from(&collection_root), id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_contract(
    collection_root: String,
    contract_id: String,
    svc: State<ContractService>,
) -> Result<(), String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.delete_contract(&PathBuf::from(&collection_root), id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_contract_changelog(
    collection_root: String,
    contract_id: String,
    svc: State<ContractService>,
) -> Result<ContractChangelog, String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.get_changelog(&PathBuf::from(&collection_root), id)
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 2: Verify compile**

```bash
cargo check -p rocket-tauri
```

Expected: clean.

---

## Task 2: Register state + commands + save hook

**Files:**
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Export `contract` from `commands/mod.rs`**

Add alongside existing `pub mod` lines:

```rust
pub mod contract;
```

- [ ] **Step 2: Manage `ContractService` in `src-tauri/src/lib.rs`**

In the imports section, add:

```rust
use rocket_app::ContractService;
use rocket_infra::FsContractRepo;
use std::sync::Arc;
```

In the app setup block (alongside where other services are managed):

```rust
let contract_svc = ContractService::new(Arc::new(FsContractRepo));
app.manage(contract_svc);
```

- [ ] **Step 3: Register commands in `src-tauri/src/lib.rs`**

In the `.invoke_handler(tauri::generate_handler![...])` block, add:

```rust
commands::contract::attach_contract,
commands::contract::list_contracts,
commands::contract::get_contract,
commands::contract::delete_contract,
commands::contract::get_contract_changelog,
```

- [ ] **Step 4: Wire save hook into existing save_request command**

Locate `src-tauri/src/commands/collections.rs` (or wherever `save_request` lives). After the successful save, add:

```rust
// Contract audit hook — silently diffs and logs any changes
if let Ok(new_snap) = build_snapshot_from_request(&request) {
    let _ = contract_svc.on_request_saved(
        &PathBuf::from(&collection_root),
        new_snap,
    );
}
```

Add this helper function in the same file:

```rust
fn build_snapshot_from_request(
    request: &rocket_collection::HttpRequest,
) -> Result<rocket_collection::contract::snapshot::RequestSignatureSnapshot, ()> {
    use rocket_collection::contract::snapshot::RequestSignatureSnapshot;
    use std::path::PathBuf;

    Ok(RequestSignatureSnapshot {
        request_path: PathBuf::from(&request.path),
        method: request.method.to_string(),
        url_pattern: request.url.clone(),
        query_param_keys: request.query_params.iter().map(|p| p.key.clone()).collect(),
        header_keys: request.headers.iter().map(|h| h.key.clone()).collect(),
        body_field_keys: extract_body_keys(&request.body),
        auth_type: request.auth.as_ref().map(|a| a.type_name()).unwrap_or("none").to_string(),
        captured_at: chrono::Utc::now(),
    })
}

fn extract_body_keys(body: &Option<rocket_collection::Body>) -> Vec<String> {
    match body {
        Some(rocket_collection::Body::FormUrlEncoded(fields)) => {
            fields.iter().map(|f| f.key.clone()).collect()
        }
        Some(rocket_collection::Body::Multipart(fields)) => {
            fields.iter().map(|f| f.key.clone()).collect()
        }
        Some(rocket_collection::Body::Json(raw)) => {
            // Extract top-level keys from JSON object if parseable
            serde_json::from_str::<serde_json::Value>(raw)
                .ok()
                .and_then(|v| v.as_object().cloned())
                .map(|m| m.keys().cloned().collect())
                .unwrap_or_default()
        }
        _ => vec![],
    }
}
```

Note: adjust field names (`request.path`, `request.method`, etc.) to match the actual `HttpRequest` struct in `rocket-collection`. The pattern is exact — only names may differ.

- [ ] **Step 5: Full compile check**

```bash
cargo build -p rocket-tauri
```

Expected: clean build. Fix any field name mismatches in `build_snapshot_from_request`.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/contract.rs
git add src-tauri/src/commands/mod.rs
git add src-tauri/src/lib.rs
git commit -m "feat(contract): Tauri IPC commands + save hook wired"
```
