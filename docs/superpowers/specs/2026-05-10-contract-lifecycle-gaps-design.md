# Contract Lifecycle Gaps — Design Spec

**Date:** 2026-05-10  
**Status:** Approved  
**Scope:** Fix four categories of gap between the contract state machine definition and the running implementation.

---

## Background

A full audit of `state_machine.rs` against the frontend action handlers, store actions, and Tauri commands revealed four categories of gap:

1. **Dangling action handlers** — four `ContractAction` values fire but are never handled in `ContractsTab`
2. **Archive not implemented** — Archive buttons exist in the UI but have no backend status, command, or store action
3. **Auto-expiry never fires** — `ExpiryLapsed` / `ExpiringSoon` are defined in the state machine but nothing triggers them; expiry is computed client-side only
4. **`MarkBreaking` unreachable** — left as a future hook; no change in this spec

---

## Section 1 — Action Wiring Fixes

### Problem

`ContractsTab.onAction` has no handler for `review_diff`, `accept_drift`, `open_review`, or `remind_reviewers`. All four fall through to a silent no-op.

`accept_drift` is the most critical: it is the primary CTA on drift/breach cards ("Accept as v{N}") and does nothing.

### Design

**`accept_drift`** → wired to `resignContract(collectionId, contractId)`.  
This is identical to the existing `resign` action at the store/backend level. The distinction is only in the button label ("Accept as v2.2" vs "Re-sign"). No new store action or Tauri command needed.

**`review_diff`** → no-op (stub). The diff view UI is a future feature.

**`open_review`** → no-op (stub). The review panel UI is a future feature.

**`remind_reviewers`** → no-op (stub). Reviewer notifications are a future feature.

### Changes

| File | Change |
|---|---|
| `src/components/contracts/ContractsTab.tsx` | Add `case 'accept_drift'` → `resignContract`; add empty `case` for `review_diff`, `open_review`, `remind_reviewers` |

---

## Section 2 — Archive Status

### Problem

Archive buttons appear on Paused and Expired contract cards but have no backend status, no Tauri command, and no store action. Clicking Archive silently does nothing.

### Design

**Archive** is a new terminal-ish status: contracts hide from the main list, appear under an "Archived" filter, and can be unarchived (returns to Draft for a fresh start).

#### State Machine Additions (`rocket-collection/src/contract/state_machine.rs`)

New events:
- `StatusEvent::Archive`
- `StatusEvent::Unarchive`

New transitions:
```
(Paused | Expired, Archive)  → Archived
(Archived, Unarchive)        → Draft
```

`Archived` is skipped by `recompute_drift_for_collection` (alongside Draft, Paused, Expired, InReview).

#### Rust Type Changes

- `ContractStatus::Archived` variant added to `rocket-collection/src/contract/types.rs`
- `ContractStatusDto::Archived` added to `src-tauri/src/commands/contract_dtos/types.rs`
- `From` impls updated for both directions

#### New Tauri Commands (`src-tauri/src/commands/contract.rs`)

- `archive_contract(collection_root, contract_id)` → `transition_contract_status(..., StatusEvent::Archive)`
- `unarchive_contract(collection_root, contract_id)` → `transition_contract_status(..., StatusEvent::Unarchive)`

#### TypeScript Changes

- `'archived'` added to `ContractStatus` in `src/types/contracts.ts` and `ContractStatusDto` in `src/lib/tauri-api.ts`
- `archiveContract` / `unarchiveContract` added to `ContractsActions` interface and `contractsActions.ts`
- `ContractsTab.onAction` wires `'archive'` → `archiveContract`
- `ContractStatusChip` gets `archived` variant: muted background, no pulsing dot, label "Archived"
- `ContractsFilterBar` adds "Archived" filter option (maps to `status === 'archived'`)
- `PrimaryAction` for `archived` → "Unarchive" button (action `'unarchive'`)
- Add `'unarchive'` to `ContractAction` type

#### Drift Skip

In `recompute_drift_for_collection`, add `ContractStatus::Archived` to the early-continue match arm alongside Draft, Paused, Expired, InReview.

---

## Section 3 — Auto-Expiry

### Problem

`ExpiryLapsed` and `ExpiringSoon` are valid state machine events but are never fired. Expiry is computed in the browser via `contract-store.ts:computeStatus` (date math). The new `contractsSlice` uses the Rust `status` field, which never transitions to `Expired` or `ExpiringIn30Days` automatically.

### Design

**Check happens inside `FsContractRepo::list_contracts`** (in `rocket-infra`).

After deserializing each contract from disk, before returning the list, apply expiry logic:

```
today = current local date (NaiveDate::from_ymd)

for each contract in list:
  if contract.status ∈ {Expired, Archived, Draft, InReview}:
    skip  // already terminal or non-monitored

  if expiryDate is Some(date) AND date < today:
    apply ExpiryLapsed → status = Expired
    save to disk

  else if expiryDate is Some(date) AND (date - today) <= 30 days:
    if contract.status == Active:
      apply ExpiringSoon → status = ExpiringIn30Days
      save to disk
```

This runs transparently on every `list_contracts` call. No scheduler, no background thread. The Rust `status` field becomes the authoritative source of truth for expiry, replacing the client-side date math in `contract-store.ts`.

#### Changes

| File | Change |
|---|---|
| `crates/rocket-infra/src/fs_contract_repo.rs` | Add expiry check loop in `list_contracts` after deserialization |
| `crates/rocket-app/src/contract_service.rs` | No change (already delegates to repo) |
| Frontend | No change — `contractsSlice` already uses Rust `status` field |

#### Backward Compatibility

Contracts currently stored as `Active` with a past `expiryDate` will silently transition to `Expired` on the next `list_contracts` call. This is correct — those contracts have genuinely expired. The old frontend `computeStatus` in `contract-store.ts` already showed them as expired visually; this just makes the Rust field match.

---

## Section 4 — `MarkBreaking` (deferred)

`StatusEvent::MarkBreaking` (`Drift → Breach`) remains in the state machine as a valid future hook. No UI path, no command, no change in this spec.

---

## Summary of All Changes

### Rust (`rocket-collection`)
- `ContractStatus::Archived` variant
- `StatusEvent::Archive`, `StatusEvent::Unarchive`
- State machine transitions for archive/unarchive
- Skip `Archived` in `recompute_drift_for_collection`

### Rust (`rocket-infra`)
- Expiry check in `FsContractRepo::list_contracts`

### Rust (`src-tauri`)
- `archive_contract` Tauri command
- `unarchive_contract` Tauri command
- `ContractStatusDto::Archived` + From impls

### TypeScript (types + API)
- `'archived'` in `ContractStatus` and `ContractStatusDto`
- `archiveContract`, `unarchiveContract` in `tauri-api.ts`

### TypeScript (store)
- `archiveContract`, `unarchiveContract` in `contractsActions.ts`

### TypeScript (UI)
- `ContractsTab.onAction`: wire `accept_drift` → resign; wire `archive` → archiveContract; add `'unarchive'` case → unarchiveContract; stub `review_diff`, `open_review`, `remind_reviewers`
- `ContractStatusChip`: `archived` variant
- `ContractCard`: `'unarchive'` added to `ContractAction`; PrimaryAction for `archived` → "Unarchive"
- `ContractsFilterBar`: "Archived" filter option
- `contractsSlice`: add `'archived'` to TypeScript `ContractStatus`

---

## What This Does NOT Change

- `MarkBreaking` — deferred
- `review_diff`, `open_review`, `remind_reviewers` — stubs only, no UI built
- `contract-store.ts` (old store) — left untouched; its client-side `computeStatus` is now redundant but harmless
