# Contract Lock Enhancement — Design Spec

> **Type:** Spec (reference only — never executed directly)
> **Date:** 2026-05-07
> **Status:** Approved

---

## Overview

Enhances the shipped Contract Lock feature to match the hi-fi design (`API Contracts - Option 1 Hi-Fi.html`) and extended lifecycle model. All persistence remains in Rust (`.rocket/contracts/*.yml`) following the existing DDD architecture. The frontend adopts the full type system and UI from the implementation plan but wires to Tauri IPC instead of localStorage.

**Visual reference:** `API Contracts - Option 1 Hi-Fi.html` — open at 1440px. All pixel values, colours, and interaction states are authoritative.

**Design system reference:** `design-system.md` — all tokens, component specs, spacing.

---

## Regression Prevention Contract

The following must not break under any circumstance:

1. Old YAML files with `provider: "String"` and `consumer: "String"` must deserialise cleanly — zero migration scripts, zero data loss.
2. All 5 existing Tauri commands (`attach_contract`, `list_contracts`, `get_contract`, `delete_contract`, `get_contract_changelog`) keep their call signatures. New commands are additive.
3. The existing save hook (`on_request_saved`) is unchanged.
4. `ContractStatus::Active` and `ContractStatus::Expired` retain their exact YAML serialisation values (`active`, `expired`).
5. `cargo test --workspace` must pass after every plan.
6. `yarn tsc --noEmit` must pass after every frontend plan.

---

## Sub-Project Map

| Sub-project | Layer | Plans | Depends on |
|---|---|---|---|
| SP1 — Rust domain types | `rocket-collection` | SP1-01, SP1-02 | — |
| SP2 — Policy-aware diff + state machine | `rocket-collection` | SP2-01, SP2-02 | SP1 |
| SP3 — Service + Tauri commands | `rocket-app`, `rocket-infra`, `src-tauri` | SP3-01, SP3-02, SP3-03 | SP2 |
| SP4 — Frontend types + store + tokens | Frontend | SP4-01, SP4-02 | SP3 |
| SP5 — Leaf components | Frontend | SP5-01, SP5-02, SP5-03 | SP4 |
| SP6 — ContractCard + ContextMenu | Frontend | SP6-01, SP6-02 | SP5 |
| SP7 — ContractsTab + NewContractModal | Frontend | SP7-01, SP7-02, SP7-03 | SP6 |
| SP8 — Tab wiring + sidebar + drift engine | Frontend | SP8-01, SP8-02 | SP7 |
| SP9 — Keyboard shortcuts + a11y | Frontend | SP9-01 | SP8 |

---

## SP1 — Rust Domain Types

### New types

#### `ContractParty`
Replaces bare `String` for provider/consumer. Backward-compat serde handles old plain-string YAML.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractParty {
    pub id: String,
    pub name: String,
    pub kind: PartyKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_seed: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PartyKind { #[default] Team, Company, Service }
```

Backward-compat deserialization: if YAML value at `provider` is a plain string `s`, deserialise as `ContractParty { id: slugify(s), name: s, kind: Team, .. }`. `slugify` = lowercase + spaces to hyphens.

#### `ContractPolicy` + `BreakingChangePolicy`

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractPolicy {
    pub breaking_change_policy: BreakingChangePolicy,
    pub notice_days: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime_sla: Option<f32>,
}

impl Default for ContractPolicy {
    fn default() -> Self {
        Self { breaking_change_policy: BreakingChangePolicy::Lenient, notice_days: 30, uptime_sla: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BreakingChangePolicy { Strict, #[default] Lenient, AdditiveOk }
```

#### Updated `ContractStatus` — 8 variants

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContractStatus {
    Draft,
    #[default]
    Active,
    Drift,
    Breach,
    InReview,
    Paused,
    ExpiringIn30Days,
    Expired,
}
```

Status is now **stored** in YAML, not computed from expiry at runtime. The old computed approach is replaced: on load, if stored status is `Active` and expiry date is ≤ 30 days away, the service upgrades to `ExpiringIn30Days` in memory before returning to the frontend (without writing to disk, to avoid noise commits).

#### Updated `Contract` struct

```rust
pub struct Contract {
    // Existing — unchanged
    pub id: Ulid,
    pub title: String,
    pub effective_date: NaiveDate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiry_date: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub document_paths: Vec<PathBuf>,
    pub enforcement_mode: ContractEnforcementMode,
    pub scope: ContractScope,

    // Upgraded — backward-compat serde
    pub provider: ContractParty,        // was: String
    pub consumers: Vec<ContractParty>,  // was: consumer: String

    // Stored status — was computed
    #[serde(default)]
    pub status: ContractStatus,

    // New fields — all #[serde(default)]
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub policy: ContractPolicy,
    #[serde(default)]
    pub drift_count: u32,
    #[serde(default)]
    pub breach_count: u32,
    #[serde(default)]
    pub endpoint_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

fn default_version() -> String { "1.0.0".to_string() }
```

#### Updated `ChangelogEntry`

```rust
pub struct ChangelogEntry {
    // Existing
    pub timestamp: DateTime<Utc>,
    pub request_path: PathBuf,
    pub field: String,
    pub change_type: ChangeType,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    // New
    #[serde(default)]
    pub is_breaking: bool,
}
```

---

## SP2 — Policy-Aware Diff + State Machine

### State machine (`state_machine.rs`)

Pure function — no I/O.

```rust
pub fn transition(
    current: &ContractStatus,
    event: &StatusEvent,
) -> Result<ContractStatus, DomainError>
```

Valid transitions:

| From | Event | To |
|---|---|---|
| `Draft` | `Publish` | `Active` |
| `Active` | `DriftDetected` | `Drift` |
| `Active` | `BreachDetected` | `Breach` |
| `Active` / `Drift` / `Breach` | `Pause` | `Paused` |
| `Active` / `Drift` / `Breach` / `Paused` | `SendForReview` | `InReview` |
| `Drift` / `Breach` | `Resign` | `Active` |
| `Drift` | `MarkBreaking` | `Breach` |
| `Paused` | `Resume` | `Active` |
| `Expired` | `Renew` | `Active` |
| `InReview` | `Approve` | `Active` |
| `InReview` | `Reject` | `Draft` |
| Any | `ExpiryLapsed` | `Expired` |
| `Active` | `ExpiringSoon` | `ExpiringIn30Days` |

All other combinations return `Err(DomainError::InvalidStatusTransition { from, event })`.

### Policy-aware `diff_signature`

New signature — backward compatible with default policy:

```rust
pub fn diff_signature(
    old: &RequestSignatureSnapshot,
    new: &RequestSignatureSnapshot,
    policy: &BreakingChangePolicy,
) -> Vec<ChangelogEntry>
```

Breaking rules by policy:

| Change | Strict | Lenient | AdditiveOk |
|---|---|---|---|
| Method changed | ✅ | ✅ | ✅ |
| Path changed | ✅ | ✅ | ✅ |
| Required param removed | ✅ | ✅ | ✅ |
| Optional param removed | ✅ | ❌ | ❌ |
| New param added | ✅ | ❌ | ❌ |
| Header removed | ✅ | ✅ | ❌ |
| Auth type changed | ✅ | ✅ | ✅ |
| Entire request removed | ✅ | ✅ | ✅ |
| New endpoint added | ✅ | ❌ | ❌ |

Existing call site in the save hook passes `&BreakingChangePolicy::Lenient` as the default — no behaviour change for existing contracts.

### `recompute_drift` service method

```rust
pub fn recompute_drift_for_collection(
    &self,
    collection_root: &Path,
) -> DomainResult<Vec<ContractDriftSummary>>
```

Algorithm:
1. Load all contracts for the collection
2. For each contract with status `Active | Drift | Breach | ExpiringIn30Days`:
   a. Load `{id}-snapshot.yml`
   b. Load current requests from collection
   c. Build `RequestSignatureSnapshot` for each request in scope
   d. Run `diff_signature` with contract's `policy.breaking_change_policy`
   e. Count `drift_count` (total changes) and `breach_count` (breaking changes)
   f. Determine new status via `transition()` state machine
   g. Append new `ChangelogEntry` items to `{id}-changelog.yml`
   h. Save updated `{id}.yml` with new counts + status
3. Return `Vec<ContractDriftSummary>` (id, status, drift_count, breach_count)

Skip contracts with status `Draft | Paused | Expired | InReview`.

---

## SP3 — Service + Tauri Commands

### Updated `AttachContractInput` DTO

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachContractInput {
    pub title: String,
    pub provider: ContractParty,          // was: String
    pub consumers: Vec<ContractParty>,    // was: consumer: String
    pub version: String,
    pub effective_date: String,
    pub expiry_date: Option<String>,
    pub document_paths: Vec<PathBuf>,
    pub scope: ContractScope,
    pub policy: ContractPolicy,
    pub initial_snapshots: Vec<RequestSignatureSnapshot>,
    pub publish_immediately: bool,
}
```

When `publish_immediately = true`, status is set to `Active` and snapshot is taken. When false, status is `Draft` and `signedSnapshot` is null.

### New commands (all in `src-tauri/src/commands/contract.rs`)

```
publish_contract(collection_root, contract_id) → Contract
pause_contract(collection_root, contract_id) → Contract
resume_contract(collection_root, contract_id) → Contract
renew_contract(collection_root, contract_id, new_expires_at: Option<String>) → Contract
send_for_review(collection_root, contract_id) → Contract
approve_contract(collection_root, contract_id) → Contract
reject_contract(collection_root, contract_id) → Contract
duplicate_contract(collection_root, contract_id) → Contract
recompute_drift(collection_root) → Vec<ContractDriftSummary>
get_contract_summary(collection_root) → Vec<ContractSummary>
```

`duplicate_contract` creates a `Draft` copy with version bumped (patch +1), id regenerated, status `Draft`, snapshot null, changelog empty.

All lifecycle commands call `ContractService::transition_status(root, id, event)` which uses the state machine and saves.

### `tauri-api.ts` additions

All new commands get typed wrapper functions in `frontend/src/lib/tauri-api.ts`. IPC field names use `camelCase` throughout.

---

## SP4 — Frontend Types + Store + Design Tokens

### `src/types/contracts.ts`

Full type definitions from the implementation plan — adopted exactly. Key addition: `ContractParty` replaces `string` for provider/consumer.

### Design tokens to add to `globals.css`

```css
/* Light */
:root {
  --warning-soft: 38 92% 93%;
  --success: 160 63% 45%;
  --success-foreground: 0 0% 98%;
  --success-soft: 160 50% 92%;
  --destructive-soft: 0 74% 94%;
}
/* Dark */
.dark {
  --warning-soft: 38 60% 16%;
  --success: 160 63% 49%;
  --success-foreground: 0 0% 10%;
  --success-soft: 160 50% 14%;
  --destructive-soft: 0 50% 18%;
}
```

Check `design-system.md` before adding — skip tokens that already exist under any name.

### `contractsSlice.ts` — IPC-backed, no `persist`

```typescript
// No persist middleware — data lives in Rust
export const useContractsStore = create<ContractsStore>()(
  subscribeWithSelector((set, get) => ({
    byId: {},
    byCollection: {},
    hoveredId: null,
    loading: false,
    error: null,
    // All mutations: call tauri-api.ts → on success update local cache
    loadContracts: async (collectionRoot) => { ... },
    createContract: async (collectionRoot, values) => { ... },
    publishContract: async (collectionRoot, contractId) => { ... },
    // ... all other actions via IPC
  }))
)
```

Optimistic updates: apply immediately to local state, roll back on IPC error.

---

## SP5 — Leaf Components

All components use shadcn/ui primitives exclusively. No raw HTML elements. Lucide icons only.

### `ContractStatusChip`

| Status | bg | text | dot animation |
|---|---|---|---|
| `active` | `success-soft` | `success` | glow pulse |
| `drift` | `warning-soft` | `warning` | none |
| `breach` | `destructive-soft` | `destructive` | `pulseRed` |
| `in_review` | `primary/14%` | `primary` | none |
| `draft` | `muted` | `muted-foreground` | none |
| `paused` | `muted` | `foreground` | none |
| `expiring_in_30_days` | `warning-soft` | `warning` | none |
| `expired` | `muted-fg/18%` | `muted-foreground` | none |

`pulseRed` animation:
```css
@keyframes pulseRed {
  0%, 100% { box-shadow: 0 0 0 0 hsl(var(--destructive) / 0.4); }
  50%       { box-shadow: 0 0 0 4px hsl(var(--destructive) / 0); }
}
@media (prefers-reduced-motion: reduce) {
  .pulse-red { animation: none; }
}
```

### `PartyPill`

```
[avatar 20px] [name] [· Provider] — rounded-full, bg-card border
```

`PartyAvatar`: 20px circle. Color from `avatarColor` or generated from name hash cycling: `[#3B82F6, #A855F7, #22C55E, #F59E0B, #EC4899, #14B8A6, #EF4444]`. Initials: first two chars of name uppercased.

### `ScopeTag` + `ChangeChip` + `MiniChangelog`

Per implementation plan §7.4, §7.5 — adopted exactly.

### `ContractsSummaryRow`

5 stat cards: Total · Active & Healthy · Drifting · Breaching · Changes 30d. Per implementation plan §7.6.

### `ContractsFilterBar`

Search (debounced 200ms) + status chips + sort dropdown + view toggle. Table view: "coming soon" toast on click. Per implementation plan §7.7.

### `ContractsEmptyState`

Per implementation plan §7.8 — lock icon, headline, two CTAs.

---

## SP6 — ContractCard + ContractContextMenu

### `ContractCard`

Two-column grid (`1fr 220px`, gap 24px). Per implementation plan §7.2 exactly.

Status modifiers via `data-status` attribute:
- `drift` → amber left border
- `breach` → red left border + `destructive-soft` bg tint
- `paused` → muted bg
- `expired` → 75% opacity

Hover: border darkens, box-shadow appears. Action buttons (`opacity-0 group-hover:opacity-100`).

Accessibility: `<article role="article" aria-labelledby="cc-name-{id}">`. Status chip has `role="status"` + `<span className="sr-only">`.

### `ContractContextMenu`

Uses shadcn `ContextMenu`. Per implementation plan §7.10. Delete action shows a confirmation `AlertDialog` before calling `deleteContract`.

---

## SP7 — ContractsTab + NewContractModal

### `ContractsTab`

Full-height flex column. PaneHeader + SummaryRow + FilterBar + ScrollArea with grouped cards. Groups: "Needs attention" (breach/drift/in_review), "Active", "Inactive" (draft/paused/expired). Empty groups omitted.

PaneHeader right side: `Sync` (ghost) · `Export` (outline) · `+ New contract` (primary).

`Sync` calls `recomputeDrift(collectionRoot)`.

### `NewContractModal`

Single-page form (no wizard). Per implementation plan §7.9. Footer: `Cancel` · `Save as Draft` · `Create & Publish →`.

`snapshot.ts` (frontend, pure TypeScript) used only for live preview in the modal right column — shows which endpoints will be snapshotted. Never used for drift comparison.

`Create & Publish` → calls `createContract` with `publishImmediately: true` → Rust takes snapshot → returns `Active` contract. Toast: "Contract created and published."

`Save as Draft` → calls `createContract` with `publishImmediately: false` → status `Draft`, no snapshot. Toast: "Contract saved as draft."

---

## SP8 — Tab Wiring + Sidebar + Drift Engine

### Tab type

`ContractTab` pane type in `pane-types.ts` is renamed to `ContractsTab` (plural) to match the new component name. All existing usages updated.

### Sidebar augmentation

`CollectionsSidebar.tsx`: lock pin on collection row (per §8.1 of implementation plan). Shows contract count + drift/breach icons. Tooltip on hover.

`RequestItem.tsx`: coloured dot per contract status (per §8.2).

### Status bar

`ContractsStatusItem.tsx` (per §9 of implementation plan). Shows only when `totalContracts > 0`.

### `useContractDrift`

Subscribes to collection mutations via `subscribeWithSelector` on the collection store. Calls `recomputeDrift(collectionRoot)` debounced 250ms after any request mutation. Also fires on `visibilitychange` (tab focus).

---

## SP9 — Keyboard Shortcuts + Accessibility

### Keyboard shortcuts (scoped to contracts tab active)

| Key | Action |
|---|---|
| `j` / `k` | Next / previous card |
| `Enter` | Open focused card |
| `e` | Edit focused contract |
| `p` | Pause / Resume |
| `n` | Open New contract modal |
| `⌘L` | Open Contracts tab (global) |
| `Escape` | Close modal / dismiss menu |
| `Delete` / `Backspace` | Delete with confirm |

Implemented with `useHotkeys` (already in the project).

### ARIA

- All cards: `<article role="article" aria-labelledby="cc-name-{id}">`
- Status chip: `role="status"` + `<span className="sr-only">Status: {label}</span>`
- All icon-only buttons: `aria-label` required
- Context menu: reachable via `Shift+F10`
- Drift/breach colour never the only signal — icon + text always present
- `prefers-reduced-motion`: disable `pulseRed`, fallback to static red border
- Focus ring: `focus-visible:ring-[3px] focus-visible:ring-ring/50`

---

## File Map Summary

### Rust (modified/created)

| File | Action |
|---|---|
| `crates/rocket-collection/src/contract/types.rs` | Modify — add `ContractParty`, `ContractPolicy`, `BreakingChangePolicy`, `PartyKind`; update `Contract`, `ContractStatus` |
| `crates/rocket-collection/src/contract/changelog.rs` | Modify — add `is_breaking` to `ChangelogEntry` |
| `crates/rocket-collection/src/contract/state_machine.rs` | Create — `StatusEvent` enum + `transition()` pure function |
| `crates/rocket-collection/src/contract/diff.rs` | Modify — add `policy` param + `is_breaking` computation |
| `crates/rocket-collection/src/contract/mod.rs` | Modify — re-export new types |
| `crates/rocket-app/src/contract_service.rs` | Modify — `recompute_drift_for_collection`, `transition_status`, updated `attach_contract` |
| `src-tauri/src/commands/contract.rs` | Modify — 10 new commands |
| `src-tauri/src/lib.rs` | Modify — register new commands |

### Frontend (new files)

```
src/types/contracts.ts
src/lib/contracts/avatarColor.ts
src/lib/contracts/statusMachine.ts
src/lib/contracts/snapshot.ts
src/store/contracts/contractsSlice.ts
src/store/contracts/contractsSelectors.ts
src/components/contracts/ContractsTab.tsx
src/components/contracts/ContractCard.tsx
src/components/contracts/ContractCardSkeleton.tsx
src/components/contracts/ContractStatusChip.tsx
src/components/contracts/PartyPill.tsx
src/components/contracts/PartyAvatar.tsx
src/components/contracts/ScopeTag.tsx
src/components/contracts/MiniChangelog.tsx
src/components/contracts/ChangeChip.tsx
src/components/contracts/ContractsSummaryRow.tsx
src/components/contracts/ContractsFilterBar.tsx
src/components/contracts/ContractsEmptyState.tsx
src/components/contracts/ContractsGroupHeader.tsx
src/components/contracts/NewContractModal.tsx
src/components/contracts/ContractContextMenu.tsx
src/components/contracts/index.ts
src/components/status-bar/ContractsStatusItem.tsx
src/hooks/useContracts.ts
src/hooks/useContractDrift.ts
src/hooks/useContractsFilter.ts
```

### Frontend (modified)

```
frontend/src/lib/tauri-api.ts
frontend/src/types/pane-types.ts
frontend/src/stores/pane-store.ts
frontend/src/components/layout/CollectionsSidebar.tsx
frontend/src/components/sidebar/RequestItem.tsx
frontend/src/components/panes/EditorGroup.tsx
frontend/src/globals.css
```
