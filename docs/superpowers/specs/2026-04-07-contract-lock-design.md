# Contract Lock — Design Spec

> **Type:** Spec (reference only — never executed directly)
> **Date:** 2026-04-07
> **Status:** Approved

---

## Overview

Contract Lock lets a developer attach a signed stability agreement to a collection, folder, or individual request. Any consumer browsing the collection can see who agreed to the contract, for which project and version, and a full audit trail of every change made to covered endpoints after the contract was signed.

The middle-path implementation is intentionally architected so that Model B enforcement (warn / block on breaking changes) can be layered on with a single enum extension and a pre-save hook — no restructuring required.

---

## Goals

| Goal | Priority |
|---|---|
| Attach a contract to a collection, folder, or individual request | Must have |
| Record provider team, consumer team, project name, version, effective/expiry dates | Must have |
| Snapshot covered endpoint signatures at contract creation time | Must have |
| Diff every subsequent save against the snapshot and append to an audit changelog | Must have |
| Display contract badge on sidebar (collection / folder / request) | Must have |
| Display contract panel: parties, status, changelog entries | Must have |
| `enforcement_mode` field defined in domain model (always `Informational` in this sprint) | Must have |
| Optional attached document (OpenAPI spec, PDF, custom file) | Nice to have |
| Model B warn/block enforcement | Out of scope — future sprint |

---

## Out of Scope

- User account integration (parties are free-text team names until SP6)
- OAuth2 / secret var contracts
- Postman / Insomnia contract import
- Model B enforcement UI (warn dialog, block-save)

---

## UI Design

### Feature overview card

The Contract Lock feature is surfaced to users as a first-class panel inside the collection sidebar. The card below is the canonical reference for the feature overview tile shown in the comparison view and in onboarding tooltips.

```
┌─────────────────────────────────────────────────────────────────┐
│  [📄]  Contract Lock — proposed feature                         │
│        API stability guarantees, attached to collections        │
│                                                                 │
│  [ Rocket exclusive · no Bruno equivalent ]                     │
│                                                                 │
│  A contract is a signed stability agreement attached to a       │
│  collection or folder. It tells any consumer — developer,       │
│  team, or external partner — exactly what was promised,         │
│  by whom, and for which project version.                        │
│                                                                 │
│  ┌─────────────────────────┐  ┌─────────────────────────────┐  │
│  │ What it stores          │  │ Visual signal               │  │
│  │ Document (OpenAPI /     │  │ Lock badge on collection    │  │
│  │ custom), version,       │  │ sidebar + status chip on    │  │
│  │ effective date, expiry  │  │ each covered endpoint       │  │
│  └─────────────────────────┘  └─────────────────────────────┘  │
│  ┌─────────────────────────┐  ┌─────────────────────────────┐  │
│  │ Parties                 │  │ Consumer view               │  │
│  │ Provider team + one or  │  │ Who agreed, for which       │  │
│  │ more consumer teams,    │  │ project, whether the        │  │
│  │ linked to a project     │  │ contract is still active    │  │
│  └─────────────────────────┘  └─────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

### Sample contract cards — in-app appearance

Each contract is rendered as a card in the Contract Panel. Two representative states are shown:

**State 1 — Active contract**

```
┌──────────────────────────────────────────────────────────────┐
│ Payments API v2.3 — Billing Team → Platform Team             │
│ Project: Checkout Revamp · Effective: 2026-01-15             │
│ Expires: 2026-12-31                                          │
│                                                              │
│  ● Provider: Billing Team   →   ● Consumer: Platform Team   │
│                                                              │
│ ─────────────────────────────────────────────────────────── │
│ Breaking change policy: 30-day notice required  [ Active ]  │
└──────────────────────────────────────────────────────────────┘
```

**State 2 — Expiring soon**

```
┌──────────────────────────────────────────────────────────────┐
│ Auth Service v1.0 — Identity Team → Mobile Team              │
│ Project: App Relaunch · Effective: 2025-09-01                │
│ Expires: 2026-06-30                                          │
│                                                              │
│  ● Provider: Identity Team  →   ● Consumer: Mobile Team     │
│                                                              │
│ ─────────────────────────────────────────────────────────── │
│ Breaking change policy: Version bump required  [Expiring]   │
└──────────────────────────────────────────────────────────────┘
```

### Status chip colour rules

| Status | Condition | Chip colour |
|---|---|---|
| Active | No expiry date, or expiry > 30 days away | Green |
| Expiring soon | Expiry within 30 days | Amber |
| Expired | Past expiry date | Red |

### Sidebar badge rules

| Scope level | Badge position | Click target |
|---|---|---|
| Collection | Right of collection name in sidebar | Opens Contract Panel sheet |
| Folder | Right of folder name in sidebar tree | Opens Contract Panel sheet |
| Request | Right of request name in sidebar tree | Opens Contract Panel sheet |

The lock icon inherits the status colour: green lock = active, amber = expiring, red = expired.

### Contract Panel layout (right-side sheet, 480 px wide)

```
┌─ Contract Panel ─────────────────────────────────────────┐
│ Payments API v2.3                          [ Active ]     │
├──────────────────────────────────────────────────────────┤
│ Provider        Billing Team                              │
│ Consumer        Platform Team                             │
│ ─────────────────────────────────────────────────────    │
│ Project         Checkout Revamp                           │
│ Version         v2.3                                      │
│ Effective       2026-01-15                                │
│ Expires         2026-12-31                                │
├──────────────────────────────────────────────────────────┤
│ CHANGE LOG                                                │
│ Date       Field              Type     Old       New      │
│ 2026-04-07 query_param.amount removed  amount    —        │
│ 2026-04-07 method             changed  GET       POST     │
├──────────────────────────────────────────────────────────┤
│ [ Remove contract ]                                       │
└──────────────────────────────────────────────────────────┘
```

### Attach Contract Dialog layout (modal, 448 px wide)

```
┌─ Attach contract ──────────────────────────────────────┐
│ Title                                                   │
│ [ Payments API v2.3                                ]    │
│                                                         │
│ Provider team              Consumer team                │
│ [ Billing Team          ]  [ Platform Team         ]    │
│                                                         │
│ Project                    Version                      │
│ [ Checkout Revamp       ]  [ v1.0                  ]    │
│                                                         │
│ Effective date             Expiry date (optional)       │
│ [ 2026-04-07            ]  [                       ]    │
│                                                         │
│ Scope: Entire collection                                │
│                                                         │
│               [ Cancel ]  [ Attach contract ]           │
└─────────────────────────────────────────────────────────┘
```

### Design screenshot reference

The following is the approved visual design captured during the brainstorming session (2026-04-07). It shows the feature overview tile and two sample contract cards in their rendered in-app state.

> **File:** `docs/superpowers/specs/assets/contract-lock-design-snapshot.png`
> **Status:** Approved — use as the reference for all frontend implementation decisions in Plan 04.

Key visual decisions captured in the screenshot:

- The feature overview tile uses a document icon (filled, purple tint) in a rounded square container — Lucide `FileText` at 18 px, container 36 × 36 px, `background: #EEEDFE`.
- The "Rocket exclusive · no Bruno equivalent" badge uses purple fill (`#EEEDFE`) with dark purple text (`#534AB7`).
- The four info quadrants (What it stores / Visual signal / Parties / Consumer view) use a 2 × 2 grid with `background: var(--color-background-secondary)` cards, `border-radius: var(--border-radius-md)`, label in 11 px muted text, value in 13 px medium weight.
- Contract cards show provider and consumer as pill badges with a coloured dot: purple dot for provider, green dot for consumer (first contract); amber dot for provider, red/coral dot for consumer (second contract). Dot size 8 × 8 px, `border-radius: 50%`.
- The status chip is right-aligned in the footer row of each card. Active = `background: #E1F5EE; color: #0F6E56`. Expiring soon = `background: #FAC775; color: #633806`.
- The breaking change policy text is 12 px, `color: var(--color-text-secondary)`.
- All card borders are `0.5px solid var(--color-border-tertiary)`, `border-radius: var(--border-radius-lg)`.

---

## Architecture

### Crate placement

Contract domain types and logic live in `rocket-collection`. Contracts are intrinsically scoped to a collection and share the same filesystem root. A separate crate is not warranted at this stage — when Model B enforcement is added, the pre-save hook will live in `rocket-app` (the service layer), calling `diff_signature` from `rocket-collection`.

### Storage layout

All contract artefacts live under a `.rocket/contracts/` directory inside each collection root. This directory is committed to Git alongside `.bru` files, giving consumers full history via `git log`.

```
{collection_root}/
  .rocket/
    contracts/
      {id}.yml              ← contract definition
      {id}-snapshot.yml     ← signature snapshot taken at contract creation
      {id}-changelog.yml    ← append-only audit log of every change post-signing
```

### Model B extension seam

`ContractEnforcementMode` is defined as a three-variant enum from day one:

```rust
pub enum ContractEnforcementMode {
    Informational,   // ← only reachable variant in this sprint
    Warn,            // ← Model B: show dialog, allow override
    Block,           // ← Model B: reject save, require contract update
}
```

The save handler in the collection service already calls `diff_signature` and appends to the changelog. When Model B is built, it adds one match arm in `rocket-app/src/collection_service.rs`:

```rust
match contract.enforcement_mode {
    Informational => { /* already logs */ }
    Warn  => { emit_event(ContractBreakingChangeWarning { diff }) }
    Block => { return Err(ContractViolation { diff }) }
}
```

No domain restructuring required.

---

## Domain Types

### `Contract`

```rust
// crates/rocket-collection/src/contract/types.rs

use ulid::Ulid;
use chrono::NaiveDate;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub id:               Ulid,
    pub title:            String,
    pub provider:         String,           // free-text team name
    pub consumer:         String,           // free-text team name
    pub project:          String,
    pub version:          String,
    pub effective_date:   NaiveDate,
    pub expiry_date:      Option<NaiveDate>,
    pub status:           ContractStatus,   // derived, not stored
    pub document_path:    Option<PathBuf>,  // optional attached file
    pub enforcement_mode: ContractEnforcementMode,
    pub scope:            ContractScope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContractStatus {
    Active,
    ExpiringIn30Days,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContractEnforcementMode {
    Informational,
    Warn,
    Block,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContractScope {
    Collection,
    Folder   { rel_path: PathBuf },
    Request  { rel_path: PathBuf },
}
```

`ContractStatus` is always derived at read-time from `effective_date` and `expiry_date` — it is never persisted.

---

### `RequestSignatureSnapshot`

Captures the observable shape of a request at the moment a contract is signed.

```rust
// crates/rocket-collection/src/contract/snapshot.rs

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RequestSignatureSnapshot {
    pub request_path:    PathBuf,
    pub method:          String,
    pub url_pattern:     String,
    pub query_param_keys: Vec<String>,
    pub header_keys:     Vec<String>,
    pub body_field_keys: Vec<String>,
    pub auth_type:       String,
    pub captured_at:     DateTime<Utc>,
}

/// All snapshots for one contract, keyed by request path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractSnapshot {
    pub contract_id: Ulid,
    pub entries:     Vec<RequestSignatureSnapshot>,
}
```

---

### `ChangelogEntry`

Every save to a covered request appends one or more entries.

```rust
// crates/rocket-collection/src/contract/changelog.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangelogEntry {
    pub timestamp:    DateTime<Utc>,
    pub request_path: PathBuf,
    pub field:        String,          // e.g. "query_param.amount"
    pub change_type:  ChangeType,
    pub old_value:    Option<String>,
    pub new_value:    Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    Changed,
    Added,
    Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractChangelog {
    pub contract_id: Ulid,
    pub entries:     Vec<ChangelogEntry>,  // append-only
}
```

---

### `SignatureChange` (diff output — used by Model B seam)

```rust
// crates/rocket-collection/src/contract/diff.rs

#[derive(Debug, Clone)]
pub struct SignatureChange {
    pub field:       String,
    pub change_type: ChangeType,
    pub old_value:   Option<String>,
    pub new_value:   Option<String>,
}

/// Pure function — no I/O. Called by save handler.
pub fn diff_signature(
    old: &RequestSignatureSnapshot,
    new: &RequestSignatureSnapshot,
) -> Vec<SignatureChange>
```

This function is the core of Model B. It already exists and is already called by the save handler. Model B just acts on its return value instead of silently logging it.

---

## File Map

| File | Action | Purpose |
|---|---|---|
| `crates/rocket-collection/src/contract/mod.rs` | Create | Module root |
| `crates/rocket-collection/src/contract/types.rs` | Create | `Contract`, `ContractScope`, `ContractEnforcementMode`, `ContractStatus` |
| `crates/rocket-collection/src/contract/snapshot.rs` | Create | `RequestSignatureSnapshot`, `ContractSnapshot` |
| `crates/rocket-collection/src/contract/changelog.rs` | Create | `ChangelogEntry`, `ContractChangelog`, `ChangeType` |
| `crates/rocket-collection/src/contract/diff.rs` | Create | `diff_signature()` pure function |
| `crates/rocket-collection/src/contract/repository.rs` | Create | `ContractRepository` trait |
| `crates/rocket-infra/src/fs_contract_repo.rs` | Create | Filesystem implementation of `ContractRepository` |
| `crates/rocket-app/src/contract_service.rs` | Create | `ContractService` — create, read, list, delete + save-hook |
| `src-tauri/src/commands/contract.rs` | Create | Tauri IPC commands |
| `frontend/src/stores/contract-store.ts` | Create | Zustand store |
| `frontend/src/components/contract/ContractBadge.tsx` | Create | Lock badge shown on sidebar items |
| `frontend/src/components/contract/ContractPanel.tsx` | Create | Full panel: parties, status, changelog |
| `frontend/src/components/contract/AttachContractDialog.tsx` | Create | Dialog to create/attach a contract |
| `crates/rocket-collection/src/lib.rs` | Modify | Export `contract` module |
| `crates/rocket-app/src/lib.rs` | Modify | Wire `ContractService` |
| `src-tauri/src/lib.rs` | Modify | Register contract commands |

---

## IPC Commands

All use camelCase serde annotations per RocketAPI convention.

```
attach_contract(collection_id, scope, contract_input) → Contract
list_contracts(collection_id)                          → Vec<Contract>
get_contract(contract_id)                              → Contract
delete_contract(contract_id)                           → ()
get_contract_changelog(contract_id)                    → ContractChangelog
```

---

## Save Hook Integration

When a request is saved, `CollectionService::save_request` checks whether the request is covered by any active contract. If it is, it calls `diff_signature` and appends any changes to the changelog. This is the only modification to the existing save path.

```
save_request(request)
  → for each contract covering this request:
      old_snapshot = load snapshot for this request
      new_snapshot = build_snapshot(request)
      changes = diff_signature(old_snapshot, new_snapshot)
      if changes is not empty:
          append changes to {id}-changelog.yml
          update {id}-snapshot.yml   ← snapshot always tracks latest
```

---

## UI Behaviour

### Sidebar badge

A small lock icon appears next to any collection, folder, or request that has an active contract. Clicking it opens the Contract Panel as a right-side sheet.

### Contract Panel

Shows: title, provider → consumer, project, version, status chip (Active / Expiring / Expired), effective and expiry dates, optional document link, and a changelog table with columns: timestamp, field, change type, old value, new value.

### Attach Contract Dialog

Form fields: title, provider, consumer, project, version, effective date, expiry date (optional), scope selector, document file picker (optional). On submit: creates contract, takes snapshot of all covered requests, initialises empty changelog.

---

## Future: Model B Addition Checklist

When Model B enforcement is built, the only changes needed are:

1. Add `enforcement_mode` selector to `AttachContractDialog` (currently hidden, field already in domain)
2. In `contract_service.rs` save hook, add match arm on `enforcement_mode`:
   - `Warn` → emit `ContractBreakingChangeWarning` event to frontend
   - `Block` → return `Err(ContractViolation)`
3. Frontend: handle `ContractBreakingChangeWarning` event → show override dialog
4. Frontend: handle `ContractViolation` error → show blocking modal

No domain types, no repository changes, no snapshot/diff logic changes required.
