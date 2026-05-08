# SP3-01 — Updated attach_contract + lifecycle commands

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Update `attach_contract` Tauri command to accept the new `ContractParty`/`Vec<ContractParty>` shape. Add `publish_contract`, `pause_contract`, `resume_contract`, `renew_contract` commands.

**Architecture:** All commands are thin delegates to `ContractService`. Input DTOs are IPC-only (`#[serde(rename_all = "camelCase")]`) and never written to disk. `transition_status` from the state machine is called inside the service method.

**Tech Stack:** Rust, Tauri v2

**Spec:** `docs/superpowers/specs/2026-05-07-contract-lock-enhancement-design.md` §SP3

**Depends on:** SP2-01 merged.

---

> **⚠️ Worktree** — all commands run inside `.worktrees/contract-enhancement` on branch `feat/contract-lock-enhancement`.

## Task 1: Update `AttachContractInput` + `attach_contract` command

**Files:**
- Modify: `src-tauri/src/commands/contract.rs`

- [ ] **Step 1: Update `AttachContractInput` DTO**

Find and replace the existing `AttachContractInput` struct:

```rust
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachContractInput {
    pub title: String,
    pub provider: rocket_collection::contract::types::ContractParty,
    pub consumers: Vec<rocket_collection::contract::types::ContractParty>,
    pub version: String,
    pub effective_date: String,
    pub expiry_date: Option<String>,
    pub document_paths: Vec<std::path::PathBuf>,
    pub scope: rocket_collection::contract::types::ContractScope,
    pub policy: rocket_collection::contract::types::ContractPolicy,
    pub initial_snapshots: Vec<rocket_collection::contract::snapshot::RequestSignatureSnapshot>,
    /// If true, status is set to Active and snapshot taken on creation.
    /// If false, status is Draft and no snapshot is taken.
    pub publish_immediately: bool,
}
```

- [ ] **Step 2: Update `attach_contract` command body**

The command builds a `Contract` and calls `svc.attach_contract(...)`. Update the `Contract` construction to use the new fields:

```rust
#[tauri::command]
pub fn attach_contract(
    collection_root: String,
    input: AttachContractInput,
    svc: tauri::State<'_, rocket_app::ContractService>,
) -> Result<rocket_collection::contract::types::Contract, String> {
    use rocket_collection::contract::types::*;
    use chrono::NaiveDate;
    use ulid::Ulid;

    let root = std::path::PathBuf::from(&collection_root);

    let effective_date = NaiveDate::parse_from_str(&input.effective_date, "%Y-%m-%d")
        .map_err(|e| format!("invalid effectiveDate: {e}"))?;

    let expiry_date = input.expiry_date.as_deref()
        .map(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d"))
        .transpose()
        .map_err(|e| format!("invalid expiryDate: {e}"))?;

    let now = chrono::Utc::now();
    let status = if input.publish_immediately {
        ContractStatus::Active
    } else {
        ContractStatus::Draft
    };

    let contract = Contract {
        id: Ulid::new(),
        title: input.title,
        provider: input.provider,
        consumers: input.consumers,
        project: String::new(), // project field removed in new model; keep empty for compat
        version: input.version,
        status,
        effective_date,
        expiry_date,
        document_paths: vec![],
        enforcement_mode: ContractEnforcementMode::Informational,
        scope: input.scope,
        policy: input.policy,
        drift_count: 0,
        breach_count: 0,
        endpoint_count: if input.publish_immediately {
            input.initial_snapshots.len() as u32
        } else {
            0
        },
        created_by: None,
        created_at: Some(now),
        updated_at: Some(now),
    };

    let snapshots = if input.publish_immediately { input.initial_snapshots } else { vec![] };

    svc.attach_contract(&root, contract, snapshots, input.document_paths)
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 3: Cargo check**

```bash
cargo check -p rocket-tauri 2>&1 | grep "^error" | head -20
```

Fix any mismatches. `ContractService::attach_contract` signature may need to be updated to accept `Vec<ContractParty>` consumers. Check `rocket-app/src/contract_service.rs` and update accordingly.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/contract.rs
git commit -m "feat(contract): update attach_contract command — ContractParty, consumers Vec, publish_immediately"
```

---

## Task 2: Add `publish_contract`, `pause_contract`, `resume_contract`, `renew_contract`

**Files:**
- Modify: `src-tauri/src/commands/contract.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add the four lifecycle commands**

Append to `src-tauri/src/commands/contract.rs`:

```rust
#[tauri::command]
pub fn publish_contract(
    collection_root: String,
    contract_id: String,
    snapshots: Vec<rocket_collection::contract::snapshot::RequestSignatureSnapshot>,
    svc: tauri::State<'_, rocket_app::ContractService>,
) -> Result<rocket_collection::contract::types::Contract, String> {
    let id = ulid::Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.publish_contract(&std::path::PathBuf::from(&collection_root), id, snapshots)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pause_contract(
    collection_root: String,
    contract_id: String,
    svc: tauri::State<'_, rocket_app::ContractService>,
) -> Result<rocket_collection::contract::types::Contract, String> {
    let id = ulid::Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.transition_contract_status(
        &std::path::PathBuf::from(&collection_root),
        id,
        rocket_collection::contract::state_machine::StatusEvent::Pause,
    ).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn resume_contract(
    collection_root: String,
    contract_id: String,
    svc: tauri::State<'_, rocket_app::ContractService>,
) -> Result<rocket_collection::contract::types::Contract, String> {
    let id = ulid::Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.transition_contract_status(
        &std::path::PathBuf::from(&collection_root),
        id,
        rocket_collection::contract::state_machine::StatusEvent::Resume,
    ).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn renew_contract(
    collection_root: String,
    contract_id: String,
    new_expires_at: Option<String>,
    svc: tauri::State<'_, rocket_app::ContractService>,
) -> Result<rocket_collection::contract::types::Contract, String> {
    use chrono::NaiveDate;
    let id = ulid::Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    let expiry = new_expires_at.as_deref()
        .map(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d"))
        .transpose()
        .map_err(|e| format!("invalid expiresAt: {e}"))?;
    svc.renew_contract(&std::path::PathBuf::from(&collection_root), id, expiry)
        .map_err(|e| e.to_string())
}
```

Also add the corresponding service methods to `rocket-app/src/contract_service.rs`:

```rust
pub fn transition_contract_status(
    &self,
    collection_root: &std::path::Path,
    id: ulid::Ulid,
    event: rocket_collection::contract::state_machine::StatusEvent,
) -> DomainResult<rocket_collection::contract::types::Contract> {
    let mut contract = self.repo.get(collection_root, id)?;
    let new_status = rocket_collection::contract::state_machine::transition_status(&contract.status, &event)
        .map_err(|e| rocket_shared::error::DomainError::InvalidOperation(format!("{:?}", e)))?;
    contract.status = new_status;
    contract.updated_at = Some(chrono::Utc::now());
    self.repo.save(collection_root, &contract)?;
    Ok(contract)
}

pub fn publish_contract(
    &self,
    collection_root: &std::path::Path,
    id: ulid::Ulid,
    snapshots: Vec<rocket_collection::contract::snapshot::RequestSignatureSnapshot>,
) -> DomainResult<rocket_collection::contract::types::Contract> {
    let mut contract = self.repo.get(collection_root, id)?;
    let new_status = rocket_collection::contract::state_machine::transition_status(
        &contract.status,
        &rocket_collection::contract::state_machine::StatusEvent::Publish,
    ).map_err(|e| rocket_shared::error::DomainError::InvalidOperation(format!("{:?}", e)))?;
    contract.status = new_status;
    contract.endpoint_count = snapshots.len() as u32;
    contract.updated_at = Some(chrono::Utc::now());
    if !snapshots.is_empty() {
        let snapshot = rocket_collection::contract::snapshot::ContractSnapshot {
            contract_id: id,
            entries: snapshots,
        };
        self.repo.save_snapshot(collection_root, &snapshot)?;
    }
    self.repo.save(collection_root, &contract)?;
    Ok(contract)
}

pub fn renew_contract(
    &self,
    collection_root: &std::path::Path,
    id: ulid::Ulid,
    new_expiry: Option<chrono::NaiveDate>,
) -> DomainResult<rocket_collection::contract::types::Contract> {
    let mut contract = self.repo.get(collection_root, id)?;
    let new_status = rocket_collection::contract::state_machine::transition_status(
        &contract.status,
        &rocket_collection::contract::state_machine::StatusEvent::Renew,
    ).map_err(|e| rocket_shared::error::DomainError::InvalidOperation(format!("{:?}", e)))?;
    contract.status = new_status;
    contract.expiry_date = new_expiry;
    contract.drift_count = 0;
    contract.breach_count = 0;
    contract.updated_at = Some(chrono::Utc::now());
    self.repo.save(collection_root, &contract)?;
    Ok(contract)
}
```

- [ ] **Step 2: Register commands in `lib.rs`**

Open `src-tauri/src/lib.rs`. Find the `tauri::generate_handler![...]` macro. Add:

```rust
commands::contract::publish_contract,
commands::contract::pause_contract,
commands::contract::resume_contract,
commands::contract::renew_contract,
```

- [ ] **Step 3: Full compile**

```bash
cargo check -p rocket-tauri 2>&1 | grep "^error" | head -20
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/contract.rs src-tauri/src/lib.rs
git add crates/rocket-app/src/contract_service.rs
git commit -m "feat(contract): publish/pause/resume/renew Tauri commands"
```

---

## Task 3: Add review/duplicate/drift commands + tauri-api.ts wrappers

**Files:**
- Modify: `src-tauri/src/commands/contract.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `frontend/src/lib/tauri-api.ts`

- [ ] **Step 1: Add remaining commands**

Append to `src-tauri/src/commands/contract.rs`:

```rust
#[tauri::command]
pub fn send_for_review(
    collection_root: String,
    contract_id: String,
    svc: tauri::State<'_, rocket_app::ContractService>,
) -> Result<rocket_collection::contract::types::Contract, String> {
    let id = ulid::Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.transition_contract_status(
        &std::path::PathBuf::from(&collection_root),
        id,
        rocket_collection::contract::state_machine::StatusEvent::SendForReview,
    ).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn approve_contract(
    collection_root: String,
    contract_id: String,
    svc: tauri::State<'_, rocket_app::ContractService>,
) -> Result<rocket_collection::contract::types::Contract, String> {
    let id = ulid::Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.transition_contract_status(
        &std::path::PathBuf::from(&collection_root),
        id,
        rocket_collection::contract::state_machine::StatusEvent::Approve,
    ).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reject_contract(
    collection_root: String,
    contract_id: String,
    svc: tauri::State<'_, rocket_app::ContractService>,
) -> Result<rocket_collection::contract::types::Contract, String> {
    let id = ulid::Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.transition_contract_status(
        &std::path::PathBuf::from(&collection_root),
        id,
        rocket_collection::contract::state_machine::StatusEvent::Reject,
    ).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn duplicate_contract(
    collection_root: String,
    contract_id: String,
    svc: tauri::State<'_, rocket_app::ContractService>,
) -> Result<rocket_collection::contract::types::Contract, String> {
    let id = ulid::Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.duplicate_contract(&std::path::PathBuf::from(&collection_root), id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn recompute_drift(
    collection_root: String,
    current_snapshots: Vec<rocket_collection::contract::snapshot::RequestSignatureSnapshot>,
    svc: tauri::State<'_, rocket_app::ContractService>,
) -> Result<Vec<rocket_app::contract_service::ContractDriftSummary>, String> {
    svc.recompute_drift_for_collection(&std::path::PathBuf::from(&collection_root), &current_snapshots)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_contract_summary(
    collection_root: String,
    svc: tauri::State<'_, rocket_app::ContractService>,
) -> Result<Vec<rocket_app::contract_service::ContractSummary>, String> {
    svc.list_summaries(&std::path::PathBuf::from(&collection_root))
        .map_err(|e| e.to_string())
}
```

Add `duplicate_contract` and `list_summaries` to `ContractService` in `rocket-app`:

```rust
pub fn duplicate_contract(
    &self,
    collection_root: &std::path::Path,
    id: ulid::Ulid,
) -> DomainResult<rocket_collection::contract::types::Contract> {
    use rocket_collection::contract::types::ContractStatus;
    let source = self.repo.get(collection_root, id)?;
    // Bump patch version
    let new_version = bump_patch_version(&source.version);
    let now = chrono::Utc::now();
    let duplicate = rocket_collection::contract::types::Contract {
        id: ulid::Ulid::new(),
        title: format!("{} (copy)", source.title),
        version: new_version,
        status: ContractStatus::Draft,
        drift_count: 0,
        breach_count: 0,
        endpoint_count: 0,
        created_at: Some(now),
        updated_at: Some(now),
        ..source
    };
    self.repo.save(collection_root, &duplicate)?;
    Ok(duplicate)
}

fn bump_patch_version(v: &str) -> String {
    let parts: Vec<&str> = v.split('.').collect();
    if parts.len() == 3 {
        if let Ok(patch) = parts[2].parse::<u32>() {
            return format!("{}.{}.{}", parts[0], parts[1], patch + 1);
        }
    }
    format!("{v}-copy")
}

pub fn list_summaries(
    &self,
    collection_root: &std::path::Path,
) -> DomainResult<Vec<ContractSummary>> {
    let contracts = self.repo.list(collection_root)?;
    Ok(contracts.into_iter().map(|c| ContractSummary {
        id: c.id.to_string(),
        title: c.title,
        status: c.status,
        drift_count: c.drift_count,
        breach_count: c.breach_count,
        endpoint_count: c.endpoint_count,
    }).collect())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractSummary {
    pub id: String,
    pub title: String,
    pub status: rocket_collection::contract::types::ContractStatus,
    pub drift_count: u32,
    pub breach_count: u32,
    pub endpoint_count: u32,
}
```

- [ ] **Step 2: Register all new commands in `lib.rs`**

Add to `tauri::generate_handler![...]`:

```rust
commands::contract::send_for_review,
commands::contract::approve_contract,
commands::contract::reject_contract,
commands::contract::duplicate_contract,
commands::contract::recompute_drift,
commands::contract::get_contract_summary,
```

- [ ] **Step 3: Add wrappers to `tauri-api.ts`**

Open `frontend/src/lib/tauri-api.ts`. Add after the existing contract functions:

```typescript
// ─── Contract lifecycle commands ──────────────────────────────

export async function publishContract(collectionRoot: string, contractId: string, snapshots: RequestSignatureSnapshot[]): Promise<Contract> {
  return invoke('publish_contract', { collectionRoot, contractId, snapshots })
}

export async function pauseContract(collectionRoot: string, contractId: string): Promise<Contract> {
  return invoke('pause_contract', { collectionRoot, contractId })
}

export async function resumeContract(collectionRoot: string, contractId: string): Promise<Contract> {
  return invoke('resume_contract', { collectionRoot, contractId })
}

export async function renewContract(collectionRoot: string, contractId: string, newExpiresAt: string | null): Promise<Contract> {
  return invoke('renew_contract', { collectionRoot, contractId, newExpiresAt })
}

export async function sendForReview(collectionRoot: string, contractId: string): Promise<Contract> {
  return invoke('send_for_review', { collectionRoot, contractId })
}

export async function approveContract(collectionRoot: string, contractId: string): Promise<Contract> {
  return invoke('approve_contract', { collectionRoot, contractId })
}

export async function rejectContract(collectionRoot: string, contractId: string): Promise<Contract> {
  return invoke('reject_contract', { collectionRoot, contractId })
}

export async function duplicateContract(collectionRoot: string, contractId: string): Promise<Contract> {
  return invoke('duplicate_contract', { collectionRoot, contractId })
}

export async function recomputeDrift(collectionRoot: string, currentSnapshots: RequestSignatureSnapshot[]): Promise<ContractDriftSummary[]> {
  return invoke('recompute_drift', { collectionRoot, currentSnapshots })
}

export async function getContractSummary(collectionRoot: string): Promise<ContractSummary[]> {
  return invoke('get_contract_summary', { collectionRoot })
}
```

Also add the TypeScript types for IPC returns to `tauri-api.ts` (or import from `types/contracts.ts` once SP4-01 is done):

```typescript
export interface ContractDriftSummary {
  contractId: string
  status: ContractStatus
  driftCount: number
  breachCount: number
}

export interface ContractSummary {
  id: string
  title: string
  status: ContractStatus
  driftCount: number
  breachCount: number
  endpointCount: number
}

export interface RequestSignatureSnapshot {
  requestPath: string
  method: string
  urlPattern: string
  headers: Array<{ key: string; value: string }>
  queryParams: Array<{ key: string; value: string }>
  bodyContent: string | null
  formFields: Array<{ key: string; value: string }>
  authType: string
  authDetail: string
  capturedAt: string
}
```

- [ ] **Step 4: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | head -30
```

Expected: no errors.

- [ ] **Step 5: Full compile**

```bash
cargo check -p rocket-tauri 2>&1 | grep "^error" | head -20
```

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/contract.rs
git add src-tauri/src/lib.rs
git add crates/rocket-app/src/contract_service.rs
git add frontend/src/lib/tauri-api.ts
git commit -m "feat(contract): all lifecycle Tauri commands + tauri-api.ts wrappers"
```
