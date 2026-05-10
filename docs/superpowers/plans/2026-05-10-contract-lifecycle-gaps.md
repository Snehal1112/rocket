# Contract Lifecycle Gaps Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close four gaps between the contract state machine definition and running code: wire dangling frontend actions, implement Archive status end-to-end, and make expiry auto-transition in Rust.

**Architecture:** Three independent tracks land in sequence — (1) a pure-frontend wiring fix, (2) Archive as a new `ContractStatus` variant propagated through Rust domain → infra record → IPC DTO → TypeScript, (3) expiry check injected into `FsContractRepo::list_contracts` so contracts auto-transition on every load with no background thread.

**Tech Stack:** Rust (rocket-collection, rocket-infra, rocket-app, src-tauri), TypeScript/React (Zustand, Tauri IPC, Vitest)

---

## File Map

| File | Change |
|---|---|
| `src/components/contracts/ContractsTab.tsx` | Wire `accept_drift`, `archive`, `unarchive`; stub `review_diff`, `open_review`, `remind_reviewers`; render archived group |
| `crates/rocket-collection/src/contract/types.rs` | Add `Archived` to `ContractStatus` |
| `crates/rocket-collection/src/contract/state_machine.rs` | Add `Archive`, `Unarchive` events + transitions |
| `crates/rocket-infra/src/contract_records/types.rs` | Add `Archived` to `ContractStatusRecord` + From impls |
| `crates/rocket-app/src/contract_service.rs` | Skip `Archived` in drift recompute |
| `crates/rocket-infra/src/fs_contract_repo.rs` | Auto-expiry check in `list_contracts` |
| `src-tauri/src/commands/contract_dtos/types.rs` | Add `ContractStatusDto::Archived` + From impls |
| `src-tauri/src/commands/contract.rs` | Add `archive_contract`, `unarchive_contract` commands |
| `src-tauri/src/lib.rs` | Register two new commands |
| `src/types/contracts.ts` | Add `'archived'` to `ContractStatus`; add `archived` to `ContractCounts` |
| `src/lib/tauri-api.ts` | Add `'archived'` to status enum; add `archiveContract`, `unarchiveContract` |
| `src/lib/contracts/statusMachine.ts` | Add `archived` label; update helpers |
| `src/stores/contracts/contractsActions.ts` | Add `archiveContract`, `unarchiveContract` actions |
| `src/stores/contracts/contractsSelectors.ts` | Count `archived`; add archived group; update sort order |
| `src/hooks/useContractsFilter.ts` | Exclude archived from `'all'` view |
| `src/components/contracts/ContractCard.tsx` | Add `'unarchive'` to `ContractAction` |
| `src/components/contracts/internal/PrimaryAction.tsx` | Add `archived` case → "Unarchive" button |
| `src/components/contracts/ContractStatusChip.tsx` | Add `archived` chip variant |
| `src/components/contracts/ContractsFilterBar.tsx` | Add `'archived'` to `STATUS_CHIPS` + count map |

---

## Task 1: Wire `accept_drift` and stubs in ContractsTab

**Files:**
- Modify: `src/components/contracts/ContractsTab.tsx`

- [ ] **Step 1: Add `accept_drift` case + stubs to the switch in `handleAction`**

  Find the `switch (action)` block in `handleAction` (it ends with a `default: break`). Add these cases immediately before `default`:

  ```typescript
  case 'accept_drift':
    // Same backend as resign — accepts all detected drift and re-signs at the new shape.
    await publishContract(collectionId, contractId);
    break;
  case 'review_diff':
  case 'open_review':
  case 'remind_reviewers':
    // Stub — full UI for these actions is a future feature.
    break;
  ```

- [ ] **Step 2: Verify TypeScript compiles**

  ```bash
  yarn tsc --noEmit
  ```

  Expected: no errors.

- [ ] **Step 3: Commit**

  ```bash
  git add src/components/contracts/ContractsTab.tsx
  git commit -m "fix(contracts): wire accept_drift to resign; stub review_diff/open_review/remind_reviewers"
  ```

---

## Task 2: Add `Archived` to Rust domain types

**Files:**
- Modify: `crates/rocket-collection/src/contract/types.rs`
- Modify: `crates/rocket-collection/src/contract/state_machine.rs`
- Test: `crates/rocket-collection/src/contract/state_machine.rs` (inline tests)

- [ ] **Step 1: Read the OpenCollection spec**

  📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Add `Archived` variant to `ContractStatus`**

  In `crates/rocket-collection/src/contract/types.rs`, add `Archived` after `Expired`:

  ```rust
  pub enum ContractStatus {
      /// Not yet published — snapshot not taken.
      Draft,
      /// Healthy, in compliance.
      #[default]
      Active,
      /// Non-breaking changes detected since signing.
      Drift,
      /// Breaking changes detected — consumer build at risk.
      Breach,
      /// Sent for consumer sign-off (not yet approved).
      InReview,
      /// Monitoring suspended by the provider.
      Paused,
      /// Expiry date is within 30 days.
      ExpiringIn30Days,
      /// Past expiry date.
      Expired,
      /// Manually archived — hidden from main list, recoverable.
      Archived,
  }
  ```

- [ ] **Step 3: Add `Archive` and `Unarchive` events to `StatusEvent`**

  In `crates/rocket-collection/src/contract/state_machine.rs`, add to the `StatusEvent` enum after `Renew`:

  ```rust
  pub enum StatusEvent {
      Publish,
      DriftDetected,
      BreachDetected,
      Resign,
      MarkBreaking,
      Pause,
      Resume,
      SendForReview,
      Approve,
      Reject,
      Renew,
      ExpiryLapsed,
      ExpiringSoon,
      Archive,
      Unarchive,
  }
  ```

- [ ] **Step 4: Add transitions for `Archive` and `Unarchive`**

  In the `match (current, event)` block inside `transition`, add before the catch-all `(_, ExpiryLapsed) => Expired`:

  ```rust
  // Archive: from Paused or Expired → Archived
  (Paused | Expired, Archive) => Archived,

  // Unarchive: returns to Draft for a fresh start
  (Archived, Unarchive) => Draft,
  ```

  Also ensure the `ExpiryLapsed` catch-all does NOT apply to `Archived`:

  ```rust
  // Any non-archived status can lapse into Expired
  (status, ExpiryLapsed) if *status != Archived => Expired,
  ```

  Replace the existing catch-all `(_, ExpiryLapsed) => Expired` with the guarded form above.

- [ ] **Step 5: Write failing tests**

  Add to the `#[cfg(test)]` block at the bottom of `state_machine.rs`:

  ```rust
  #[test]
  fn paused_archive_to_archived() {
      let result = transition(&ContractStatus::Paused, &StatusEvent::Archive).unwrap();
      assert_eq!(result, ContractStatus::Archived);
  }

  #[test]
  fn expired_archive_to_archived() {
      let result = transition(&ContractStatus::Expired, &StatusEvent::Archive).unwrap();
      assert_eq!(result, ContractStatus::Archived);
  }

  #[test]
  fn archived_unarchive_to_draft() {
      let result = transition(&ContractStatus::Archived, &StatusEvent::Unarchive).unwrap();
      assert_eq!(result, ContractStatus::Draft);
  }

  #[test]
  fn active_cannot_archive() {
      let result = transition(&ContractStatus::Active, &StatusEvent::Archive);
      assert!(result.is_err());
  }

  #[test]
  fn archived_does_not_expire() {
      let result = transition(&ContractStatus::Archived, &StatusEvent::ExpiryLapsed);
      assert!(result.is_err());
  }
  ```

- [ ] **Step 6: Run tests — expect failures (Archived variant missing from match exhaustiveness)**

  ```bash
  cargo test -p rocket-collection 2>&1 | tail -20
  ```

  Expected: compile errors about non-exhaustive match arms (new `Archived` variant unhandled in callers).

- [ ] **Step 7: Fix non-exhaustive matches in `contract_service.rs`**

  In `crates/rocket-app/src/contract_service.rs`, find the `match contract.status` in `recompute_drift_for_collection` (the early-continue skip list) and add `Archived`:

  ```rust
  match contract.status {
      ContractStatus::Draft
      | ContractStatus::Paused
      | ContractStatus::Expired
      | ContractStatus::InReview
      | ContractStatus::Archived => continue,
      _ => {}
  }
  ```

  Also fix the `publish_contract` method's match if it has one:

  ```rust
  let event = match &contract.status {
      ContractStatus::Drift | ContractStatus::Breach => StatusEvent::Resign,
      _ => StatusEvent::Publish,
  };
  ```

  (This one is already exhaustive via `_`; no change needed.)

- [ ] **Step 8: Run tests — expect pass**

  ```bash
  cargo test -p rocket-collection
  ```

  Expected: all tests pass including the 5 new ones.

- [ ] **Step 9: Commit**

  ```bash
  git add crates/rocket-collection/src/contract/types.rs \
          crates/rocket-collection/src/contract/state_machine.rs \
          crates/rocket-app/src/contract_service.rs
  git commit -m "feat(contracts): add Archived status + Archive/Unarchive state machine events"
  ```

---

## Task 3: Add `Archived` to the infra persistence layer

**Files:**
- Modify: `crates/rocket-infra/src/contract_records/types.rs`

- [ ] **Step 1: Read the OpenCollection spec**

  📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Add `Archived` to `ContractStatusRecord`**

  In `crates/rocket-infra/src/contract_records/types.rs`, find `ContractStatusRecord` and add `Archived` after `Expired`:

  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
  #[serde(rename_all = "snake_case")]
  pub enum ContractStatusRecord {
      Draft,
      #[default]
      Active,
      Drift,
      Breach,
      InReview,
      Paused,
      ExpiringIn30Days,
      Expired,
      Archived,
  }
  ```

- [ ] **Step 3: Update `From<&ContractStatus> for ContractStatusRecord`**

  Add the `Archived` arm:

  ```rust
  impl From<&ContractStatus> for ContractStatusRecord {
      fn from(s: &ContractStatus) -> Self {
          match s {
              ContractStatus::Draft => Self::Draft,
              ContractStatus::Active => Self::Active,
              ContractStatus::Drift => Self::Drift,
              ContractStatus::Breach => Self::Breach,
              ContractStatus::InReview => Self::InReview,
              ContractStatus::Paused => Self::Paused,
              ContractStatus::ExpiringIn30Days => Self::ExpiringIn30Days,
              ContractStatus::Expired => Self::Expired,
              ContractStatus::Archived => Self::Archived,
          }
      }
  }
  ```

- [ ] **Step 4: Update `From<ContractStatusRecord> for ContractStatus`**

  Add the `Archived` arm:

  ```rust
  impl From<ContractStatusRecord> for ContractStatus {
      fn from(r: ContractStatusRecord) -> Self {
          match r {
              ContractStatusRecord::Draft => Self::Draft,
              ContractStatusRecord::Active => Self::Active,
              ContractStatusRecord::Drift => Self::Drift,
              ContractStatusRecord::Breach => Self::Breach,
              ContractStatusRecord::InReview => Self::InReview,
              ContractStatusRecord::Paused => Self::Paused,
              ContractStatusRecord::ExpiringIn30Days => Self::ExpiringIn30Days,
              ContractStatusRecord::Expired => Self::Expired,
              ContractStatusRecord::Archived => Self::Archived,
          }
      }
  }
  ```

- [ ] **Step 5: Check that infra compiles**

  ```bash
  cargo check -p rocket-infra
  ```

  Expected: no errors.

- [ ] **Step 6: Commit**

  ```bash
  git add crates/rocket-infra/src/contract_records/types.rs
  git commit -m "feat(contracts): add Archived to ContractStatusRecord persistence layer"
  ```

---

## Task 4: Add `Archived` to the IPC DTO layer + Tauri commands

**Files:**
- Modify: `src-tauri/src/commands/contract_dtos/types.rs`
- Modify: `src-tauri/src/commands/contract.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Read the OpenCollection spec**

  📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Add `Archived` to `ContractStatusDto`**

  In `src-tauri/src/commands/contract_dtos/types.rs`, add `Archived` after `Expired`:

  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
  #[serde(rename_all = "snake_case")]
  pub enum ContractStatusDto {
      Draft,
      #[default]
      Active,
      Drift,
      Breach,
      InReview,
      Paused,
      ExpiringIn30Days,
      Expired,
      Archived,
  }
  ```

- [ ] **Step 3: Update both `From` impls in `contract_dtos/types.rs`**

  ```rust
  impl From<&ContractStatus> for ContractStatusDto {
      fn from(s: &ContractStatus) -> Self {
          match s {
              ContractStatus::Draft => Self::Draft,
              ContractStatus::Active => Self::Active,
              ContractStatus::Drift => Self::Drift,
              ContractStatus::Breach => Self::Breach,
              ContractStatus::InReview => Self::InReview,
              ContractStatus::Paused => Self::Paused,
              ContractStatus::ExpiringIn30Days => Self::ExpiringIn30Days,
              ContractStatus::Expired => Self::Expired,
              ContractStatus::Archived => Self::Archived,
          }
      }
  }

  impl From<ContractStatusDto> for ContractStatus {
      fn from(d: ContractStatusDto) -> Self {
          match d {
              ContractStatusDto::Draft => Self::Draft,
              ContractStatusDto::Active => Self::Active,
              ContractStatusDto::Drift => Self::Drift,
              ContractStatusDto::Breach => Self::Breach,
              ContractStatusDto::InReview => Self::InReview,
              ContractStatusDto::Paused => Self::Paused,
              ContractStatusDto::ExpiringIn30Days => Self::ExpiringIn30Days,
              ContractStatusDto::Expired => Self::Expired,
              ContractStatusDto::Archived => Self::Archived,
          }
      }
  }
  ```

- [ ] **Step 4: Add `archive_contract` and `unarchive_contract` commands**

  In `src-tauri/src/commands/contract.rs`, add these two functions after `reject_contract`:

  ```rust
  #[tauri::command]
  pub fn archive_contract(
      collection_root: String,
      contract_id: String,
      svc: tauri::State<'_, ContractService>,
  ) -> Result<ContractDto, String> {
      let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
      svc.transition_contract_status(
          &PathBuf::from(&collection_root),
          id,
          rocket_collection::contract::StatusEvent::Archive,
      )
      .map(|c| (&c).into())
      .map_err(|e| e.to_string())
  }

  #[tauri::command]
  pub fn unarchive_contract(
      collection_root: String,
      contract_id: String,
      svc: tauri::State<'_, ContractService>,
  ) -> Result<ContractDto, String> {
      let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
      svc.transition_contract_status(
          &PathBuf::from(&collection_root),
          id,
          rocket_collection::contract::StatusEvent::Unarchive,
      )
      .map(|c| (&c).into())
      .map_err(|e| e.to_string())
  }
  ```

- [ ] **Step 5: Register commands in `lib.rs`**

  In `src-tauri/src/lib.rs`, inside `generate_handler![...]`, add after `export_contract_openapi`:

  ```rust
  commands::contract::archive_contract,
  commands::contract::unarchive_contract,
  ```

- [ ] **Step 6: Cargo check**

  ```bash
  cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | grep "^error" | head -10
  ```

  Expected: no errors.

- [ ] **Step 7: Commit**

  ```bash
  git add src-tauri/src/commands/contract_dtos/types.rs \
          src-tauri/src/commands/contract.rs \
          src-tauri/src/lib.rs
  git commit -m "feat(contracts): add archive/unarchive Tauri commands"
  ```

---

## Task 5: Auto-expiry in `FsContractRepo::list_contracts`

**Files:**
- Modify: `crates/rocket-infra/src/fs_contract_repo.rs`
- Test: `crates/rocket-infra/src/fs_contract_repo.rs` (inline)

- [ ] **Step 1: Read the OpenCollection spec**

  📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Add required imports**

  Add to the existing `use` block at the top of `fs_contract_repo.rs`:

  ```rust
  use chrono::Local;
  use rocket_collection::contract::{StatusEvent, transition_status};
  use rocket_collection::contract::types::ContractStatus;
  ```

- [ ] **Step 3: Replace `list_contracts` with the expiry-aware version**

  Replace the existing `list_contracts` implementation:

  ```rust
  fn list_contracts(&self, collection_root: &Path) -> ContractResult<Vec<Contract>> {
      let dir = Self::contracts_dir(collection_root);
      if !dir.exists() {
          return Ok(vec![]);
      }
      let today = Local::now().date_naive();
      let thirty_days = chrono::Duration::days(30);

      let mut contracts = Vec::new();
      for entry in std::fs::read_dir(dir)? {
          let entry = entry?;
          let path = entry.path();
          let name = path.file_name().unwrap_or_default().to_string_lossy();
          if name.ends_with(".yml")
              && !name.contains("-snapshot")
              && !name.contains("-changelog")
          {
              let yaml = std::fs::read_to_string(&path)?;
              match serde_yaml::from_str::<ContractRecord>(&yaml) {
                  Ok(r) => {
                      let mut contract: Contract = r.into();
                      // Auto-transition expiry — skip already-terminal statuses.
                      if !matches!(
                          contract.status,
                          ContractStatus::Expired
                              | ContractStatus::Archived
                              | ContractStatus::Draft
                              | ContractStatus::InReview
                      ) {
                          if let Some(expiry) = contract.expiry_date {
                              let event = if expiry < today {
                                  Some(StatusEvent::ExpiryLapsed)
                              } else if expiry - today <= thirty_days {
                                  // Only transition Active → ExpiringIn30Days, not already-drifting.
                                  if contract.status == ContractStatus::Active {
                                      Some(StatusEvent::ExpiringSoon)
                                  } else {
                                      None
                                  }
                              } else {
                                  None
                              };
                              if let Some(ev) = event {
                                  if let Ok(new_status) =
                                      transition_status(&contract.status, &ev)
                                  {
                                      contract.status = new_status;
                                      contract.updated_at = Some(chrono::Utc::now());
                                      if let Err(e) =
                                          self.save_contract(collection_root, &contract)
                                      {
                                          tracing::warn!(
                                              contract_id = %contract.id,
                                              error = %e,
                                              "expiry auto-transition save failed"
                                          );
                                      }
                                  }
                              }
                          }
                      }
                      contracts.push(contract);
                  }
                  Err(e) => tracing::warn!(
                      path = %path.display(), error = %e,
                      "skipping malformed contract YAML"
                  ),
              }
          }
      }
      Ok(contracts)
  }
  ```

- [ ] **Step 4: Check compile**

  ```bash
  cargo check -p rocket-infra
  ```

  Expected: no errors.

- [ ] **Step 5: Run rocket-infra tests**

  ```bash
  cargo test -p rocket-infra 2>&1 | tail -10
  ```

  Expected: all existing tests pass.

- [ ] **Step 6: Commit**

  ```bash
  git add crates/rocket-infra/src/fs_contract_repo.rs
  git commit -m "feat(contracts): auto-transition expiry in list_contracts (ExpiryLapsed / ExpiringSoon)"
  ```

---

## Task 6: TypeScript types, API, and status helpers

**Files:**
- Modify: `src/types/contracts.ts`
- Modify: `src/lib/tauri-api.ts`
- Modify: `src/lib/contracts/statusMachine.ts`

- [ ] **Step 1: Add `'archived'` to `ContractStatus` in `types/contracts.ts`**

  Find:
  ```typescript
  export type ContractStatus =
    | 'active'
    | 'drift'
    | 'breach'
    | 'in_review'
    | 'draft'
    | 'paused'
    | 'expired'
    | 'expiring_in_30_days';
  ```

  Replace with:
  ```typescript
  export type ContractStatus =
    | 'active'
    | 'drift'
    | 'breach'
    | 'in_review'
    | 'draft'
    | 'paused'
    | 'expired'
    | 'expiring_in_30_days'
    | 'archived';
  ```

- [ ] **Step 2: Add `archived` to `ContractCounts`**

  Find the `ContractCounts` interface and add `archived: number`:

  ```typescript
  export interface ContractCounts {
    total: number;
    active: number;
    drift: number;
    breach: number;
    inReview: number;
    draft: number;
    paused: number;
    expired: number;
    archived: number;
    totalChanges: number;
    changesAdded: number;
    changesRemoved: number;
    changesModified: number;
  }
  ```

- [ ] **Step 3: Add `'archived'` to `ContractStatusDto` in `tauri-api.ts`**

  Find the `PartyKind` type and search for the contract status DTO enum. Add `| 'archived'` to the status union (the IPC layer uses string literals, not an enum class). Search for where `ContractStatus` is typed in `tauri-api.ts`:

  ```bash
  grep -n "ContractStatus\|status.*string\|archived" src/lib/tauri-api.ts | head -20
  ```

  Find the status type definition and add `'archived'` to it.

- [ ] **Step 4: Add `archiveContract` and `unarchiveContract` to `tauri-api.ts`**

  Add these two functions after `rejectContract`:

  ```typescript
  export async function archiveContract(
    collectionRoot: string,
    contractId: string,
  ): Promise<Contract> {
    return invoke('archive_contract', { collectionRoot, contractId });
  }

  export async function unarchiveContract(
    collectionRoot: string,
    contractId: string,
  ): Promise<Contract> {
    return invoke('unarchive_contract', { collectionRoot, contractId });
  }
  ```

- [ ] **Step 5: Update `statusMachine.ts` labels and helpers**

  In `src/lib/contracts/statusMachine.ts`, update `statusLabel` to include `archived`:

  ```typescript
  export function statusLabel(status: ContractStatus): string {
    const labels: Record<ContractStatus, string> = {
      draft: 'Draft',
      active: 'Active',
      drift: 'Drift',
      breach: 'Breaching',
      in_review: 'In review',
      paused: 'Paused',
      expired: 'Expired',
      expiring_in_30_days: 'Expiring Soon',
      archived: 'Archived',
    };
    return labels[status] ?? status;
  }
  ```

  Update `isInactive` to NOT include `archived` (archived hides from main list):

  ```typescript
  export function isInactive(status: ContractStatus): boolean {
    return ['draft', 'paused', 'expired'].includes(status);
  }
  ```

  (No change needed — `archived` was not in this list. Verify it's not already there.)

- [ ] **Step 6: TypeScript check**

  ```bash
  yarn tsc --noEmit
  ```

  Expected: no errors.

- [ ] **Step 7: Commit**

  ```bash
  git add src/types/contracts.ts src/lib/tauri-api.ts src/lib/contracts/statusMachine.ts
  git commit -m "feat(contracts): add archived status to TypeScript types and tauri-api"
  ```

---

## Task 7: Store actions and selectors

**Files:**
- Modify: `src/stores/contracts/contractsActions.ts`
- Modify: `src/stores/contracts/contractsSelectors.ts`
- Modify: `src/hooks/useContractsFilter.ts`

- [ ] **Step 1: Add `archiveContract` and `unarchiveContract` to `ContractsActions` interface**

  In `contractsActions.ts`, find the `ContractsActions` interface and add:

  ```typescript
  archiveContract: (collectionId: string, contractId: string) => Promise<void>;
  unarchiveContract: (collectionId: string, contractId: string) => Promise<void>;
  ```

- [ ] **Step 2: Implement both actions**

  In the `contractsActions` function return object, add after `rejectContract`:

  ```typescript
  archiveContract: async (collectionId, contractId) => {
    const raw = await api.archiveContract(collectionId, contractId);
    upsertInCollection(collectionId, adaptIpcContract(raw));
  },

  unarchiveContract: async (collectionId, contractId) => {
    const raw = await api.unarchiveContract(collectionId, contractId);
    upsertInCollection(collectionId, adaptIpcContract(raw));
  },
  ```

- [ ] **Step 3: Update `selectContractCounts` to count `archived`**

  In `contractsSelectors.ts`, add `archived = 0` to the counter declarations and count it:

  ```typescript
  export function selectContractCounts(contracts: Contract[]): ContractCounts {
    let active = 0, drift = 0, breach = 0, inReview = 0,
        draft = 0, paused = 0, expired = 0, archived = 0;
    let totalChanges = 0, changesAdded = 0, changesRemoved = 0, changesModified = 0;

    for (const c of contracts) {
      if (isActive(c.status)) active++;
      else if (c.status === 'drift') drift++;
      else if (c.status === 'breach') breach++;
      else if (c.status === 'in_review') inReview++;
      else if (c.status === 'draft') draft++;
      else if (c.status === 'paused') paused++;
      else if (c.status === 'expired') expired++;
      else if (c.status === 'archived') archived++;

      totalChanges += c.driftCount;
      for (const entry of c.changelog) {
        if (entry.kind === 'add') changesAdded++;
        else if (entry.kind === 'remove') changesRemoved++;
        else changesModified++;
      }
    }

    return {
      total: contracts.length,
      active, drift, breach, inReview,
      draft, paused, expired, archived,
      totalChanges, changesAdded, changesRemoved, changesModified,
    };
  }
  ```

- [ ] **Step 4: Update `groupContracts` to return an `archived` group**

  ```typescript
  export function groupContracts(contracts: Contract[]): {
    attention: Contract[];
    active: Contract[];
    inactive: Contract[];
    archived: Contract[];
  } {
    return {
      attention: contracts.filter((c) => needsAttention(c.status)),
      active: contracts.filter((c) => isActive(c.status)),
      inactive: contracts.filter((c) => isInactive(c.status)),
      archived: contracts.filter((c) => c.status === 'archived'),
    };
  }
  ```

- [ ] **Step 5: Update `STATUS_ORDER` to include `archived`**

  In `contractsSelectors.ts`, add `archived: 8` to `STATUS_ORDER`:

  ```typescript
  const STATUS_ORDER: Partial<Record<ContractStatus, number>> = {
    breach: 0,
    drift: 1,
    in_review: 2,
    active: 3,
    expiring_in_30_days: 4,
    draft: 5,
    paused: 6,
    expired: 7,
    archived: 8,
  };
  ```

- [ ] **Step 6: Exclude `archived` from the `'all'` filter in `useContractsFilter.ts`**

  In `applyFilter`, update the status filter block:

  ```typescript
  if (!state.statuses.includes('all')) {
    result = result.filter((c) => state.statuses.includes(c.status as ContractFilterStatus));
  } else {
    // 'all' excludes archived — archived only shows when explicitly filtered.
    result = result.filter((c) => c.status !== 'archived');
  }
  ```

- [ ] **Step 7: TypeScript check**

  ```bash
  yarn tsc --noEmit
  ```

  Expected: no errors.

- [ ] **Step 8: Commit**

  ```bash
  git add src/stores/contracts/contractsActions.ts \
          src/stores/contracts/contractsSelectors.ts \
          src/hooks/useContractsFilter.ts
  git commit -m "feat(contracts): archive/unarchive store actions; exclude archived from 'all' filter"
  ```

---

## Task 8: UI components for Archived status

**Files:**
- Modify: `src/components/contracts/ContractCard.tsx`
- Modify: `src/components/contracts/internal/PrimaryAction.tsx`
- Modify: `src/components/contracts/ContractStatusChip.tsx`
- Modify: `src/components/contracts/ContractsFilterBar.tsx`
- Modify: `src/components/contracts/ContractsTab.tsx`

- [ ] **Step 1: Add `'unarchive'` to `ContractAction` in `ContractCard.tsx`**

  Find the `ContractAction` type and add `'unarchive'`:

  ```typescript
  export type ContractAction =
    | 'open' | 'edit' | 'resign' | 'review_diff' | 'accept_drift'
    | 'open_review' | 'remind_reviewers' | 'publish' | 'pause' | 'resume'
    | 'renew' | 'send_for_review' | 'approve' | 'reject' | 'duplicate'
    | 'export' | 'delete' | 'archive' | 'unarchive' | 'view_changelog';
  ```

- [ ] **Step 2: Add `archived` case to `PrimaryAction.tsx`**

  In `PrimaryAction.tsx`, add before `default: return null`:

  ```typescript
  case 'archived':
    return (
      <Button
        variant='outline'
        size='sm'
        className='h-7 text-xs'
        onClick={(e) => {
          stop(e);
          onAction('unarchive', contract.id);
        }}
      >
        Unarchive
      </Button>
    );
  ```

- [ ] **Step 3: Add `archived` chip variant to `ContractStatusChip.tsx`**

  In `chipVariants`, add:
  ```typescript
  archived: 'bg-muted text-muted-foreground border-border',
  ```

  In `dotVariants`, add:
  ```typescript
  archived: null,
  ```

- [ ] **Step 4: Add `'archived'` to `STATUS_CHIPS` in `ContractsFilterBar.tsx`**

  Find the `STATUS_CHIPS` array and add `'archived'`:

  ```typescript
  const STATUS_CHIPS: ContractFilterStatus[] = [
    'all', 'active', 'drift', 'breach', 'draft', 'paused', 'expired', 'archived',
  ];
  ```

  Add to the `map` in `getChipCount`:

  ```typescript
  function getChipCount(status: ContractFilterStatus, counts: ContractCounts): number {
    if (status === 'all') return counts.total;
    const map: Partial<Record<ContractFilterStatus, number>> = {
      active: counts.active,
      drift: counts.drift,
      breach: counts.breach,
      in_review: counts.inReview,
      draft: counts.draft,
      paused: counts.paused,
      expired: counts.expired,
      archived: counts.archived,
    };
    return map[status] ?? 0;
  }
  ```

- [ ] **Step 5: Wire archive/unarchive in `ContractsTab` and render archived group**

  **5a** — Add store selectors near the top of the component alongside other store reads:

  ```typescript
  const archiveContract = useContractsStore((s) => s.archiveContract);
  const unarchiveContract = useContractsStore((s) => s.unarchiveContract);
  ```

  **5b** — In `handleAction`, add these cases before `default`:

  ```typescript
  case 'archive':
    await archiveContract(collectionId, contractId);
    break;
  case 'unarchive':
    await unarchiveContract(collectionId, contractId);
    break;
  ```

  **5c** — Add `archiveContract`, `unarchiveContract` to the `useCallback` dependency array.

  **5d** — Update the destructure of `groupContracts`:

  ```typescript
  const { attention, active, inactive, archived } = groupContracts(filtered);
  const allCards = [...attention, ...active, ...inactive, ...archived];
  ```

  **5e** — Add the archived rendering block after the `inactive` block in the JSX (inside the `<>` fragment):

  ```tsx
  {archived.length > 0 && (
    <>
      <ContractsGroupHeader label='Archived' count={archived.length} />
      {archived.map((c, i) => (
        <ContractCard
          key={c.id}
          ref={(el) => {
            cardRefs.current[attention.length + active.length + inactive.length + i] = el;
          }}
          contract={c}
          collectionName={collectionName}
          collectionRoot={collectionId}
          onAction={handleAction}
          focused={focusedIdx === attention.length + active.length + inactive.length + i}
        />
      ))}
    </>
  )}
  ```

- [ ] **Step 6: TypeScript check**

  ```bash
  yarn tsc --noEmit
  ```

  Expected: no errors.

- [ ] **Step 7: Run Vitest**

  ```bash
  yarn test contracts 2>&1 | tail -20
  ```

  Expected: all existing tests pass.

- [ ] **Step 8: Format**

  ```bash
  yarn format
  ```

- [ ] **Step 9: Commit**

  ```bash
  git add src/components/contracts/ContractCard.tsx \
          src/components/contracts/internal/PrimaryAction.tsx \
          src/components/contracts/ContractStatusChip.tsx \
          src/components/contracts/ContractsFilterBar.tsx \
          src/components/contracts/ContractsTab.tsx
  git commit -m "feat(contracts): archive UI — status chip, filter chip, group render, unarchive CTA"
  ```

---

## Self-Review Checklist

- [x] **Spec coverage:** Section 1 (accept_drift wiring) → Task 1. Section 2 (Archive) → Tasks 2–4 + Task 8. Section 3 (auto-expiry) → Task 5. Section 4 (MarkBreaking deferred) → no task, correct.
- [x] **Placeholder scan:** All code blocks are complete. No TBDs.
- [x] **Type consistency:** `archiveContract` / `unarchiveContract` defined in tauri-api.ts (Task 6), imported in contractsActions.ts (Task 7), called in ContractsTab (Task 8). `'archived'` added to `ContractStatus` (Task 6) before it is used in selectors (Task 7) and UI (Task 8). `ContractCounts.archived` added in Task 6 and counted in Task 7.
- [x] **`ContractStatusRecord.Archived`** added in Task 3 before the Tauri DTO (Task 4) needs it — correct order.
- [x] **State machine guard:** `Archived` excluded from `ExpiryLapsed` catch-all in Task 2. Same status skipped in drift recompute in Task 2.
