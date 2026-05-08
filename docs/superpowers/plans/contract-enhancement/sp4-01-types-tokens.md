# SP4-01 — Frontend: TypeScript Types + Design Tokens

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **⚠️ Worktree** — all commands run inside `.worktrees/contract-enhancement` on branch `feat/contract-lock-enhancement`.

**Goal:** Create `src/types/contracts.ts` with the exact type shapes from `Implementation_Plan_v2.md §2`, add all missing design tokens to `globals.css`, and expose HTTP method colors from `src/lib/colors.ts` as CSS variables.

**Architecture:** Types follow Option B (Tauri): `Contract.signedSnapshot` is kept in the type as `null`-always (Rust owns the snapshot file), `changelog` is embedded in `Contract` returned from IPC. All type names follow the spec exactly — `Party` not `ContractParty`, `ChangeKind: 'add' | 'remove' | 'modify'` not variants with `-ed` suffix.

**Tech Stack:** TypeScript, CSS

**Spec:** `docs/superpowers/specs/2026-05-07-contract-lock-enhancement-design.md`; `Implementation_Plan_v2.md §2, §4`

**Depends on:** SP3 merged (Tauri commands defined so IPC shapes are known).

---

## Task 1: `src/types/contracts.ts` — exact types from spec

**Files:**
- Create: `frontend/src/types/contracts.ts`

- [ ] **Step 1: Write the file**

```typescript
// ─── Enums ────────────────────────────────────────────────

export type ContractStatus =
  | 'active'
  | 'drift'
  | 'breach'
  | 'in_review'
  | 'draft'
  | 'paused'
  | 'expired'

export type PartyKind = 'team' | 'company' | 'service'
export type PartyRole = 'provider' | 'consumer'

/** 'add' | 'remove' | 'modify' — matches Rust ChangeType snake_case IPC values */
export type ChangeKind = 'add' | 'remove' | 'modify'

export type ContractScopeType = 'collection' | 'folder' | 'requests'

export type ContractScope =
  | { type: 'collection' }
  | { type: 'folder'; folderId: string; path: string }
  | { type: 'requests'; requestIds: string[] }

export type BreakingChangePolicy = 'strict' | 'lenient' | 'additive_ok'

// ─── Entities ─────────────────────────────────────────────

/** A party (provider or consumer) in a contract. */
export interface Party {
  id: string
  name: string
  kind: PartyKind
  /** Seed for avatar color — hash of name if not set */
  avatarSeed?: string
  /** Hex color override for avatar bg */
  avatarColor?: string
}

export interface ContractPolicy {
  breakingChangePolicy: BreakingChangePolicy
  /** Days of notice required before breaking changes land */
  noticeDays: number
  /** 0–100 percentage. null = no SLA defined */
  uptimeSla: number | null
}

export interface ChangelogEntry {
  id: string
  contractId: string
  /** ISO datetime */
  at: string
  kind: ChangeKind
  /** Short human-readable label e.g. "query.limit removed" */
  summary: string
  /** Full diff detail, optional */
  detail?: string
  requestId?: string
  requestMethod?: string
  requestPath?: string
  /** True if this change breaks the contract per its policy */
  isBreaking: boolean
  authorId?: string
  authorName?: string
}

/** Map of requestId → shape at time of signing */
export type RequestShapeMap = Record<string, RequestShape>

export interface RequestShape {
  method: string
  path: string
  params: ParamShape[]
  headers: ParamShape[]
  bodySchema?: string   // JSON schema string
}

export interface ParamShape {
  key: string
  required: boolean
  type?: string
}

export interface Contract {
  id: string
  collectionId: string
  name: string
  /** SemVer string e.g. "1.0.2" */
  version: string
  status: ContractStatus
  provider: Party
  consumers: Party[]
  scope: ContractScope
  policy: ContractPolicy
  /** ISO date "YYYY-MM-DD" */
  effectiveAt: string
  /** ISO date or null */
  expiresAt: string | null
  /**
   * Option B (Tauri): always null on the frontend — snapshot lives in Rust as
   * {id}-snapshot.yml. Kept in the type for forward compat and preview usage.
   */
  signedSnapshot: RequestShapeMap | null
  /** Cached: derived from signedSnapshot diff in Rust */
  driftCount: number
  breachCount: number
  endpointCount: number
  /** Last 100 changelog entries, returned by Rust IPC */
  changelog: ChangelogEntry[]
  createdBy: string
  createdAt: string
  updatedAt: string
}

// ─── Drift ────────────────────────────────────────────────

export interface DriftReport {
  contractId: string
  computedAt: string
  diffs: RequestDiff[]
  driftCount: number
  breachCount: number
}

export interface RequestDiff {
  requestId: string
  method: string
  path: string
  changes: FieldChange[]
}

export interface FieldChange {
  /** e.g. "params.limit", "method", "body.schema" */
  field: string
  kind: ChangeKind
  before?: string
  after?: string
  isBreaking: boolean
}

// ─── Store shape ──────────────────────────────────────────

export interface ContractsState {
  byId: Record<string, Contract>
  /** collectionId → sorted contractIds (attention-first) */
  byCollection: Record<string, string[]>
  hoveredId: string | null
  loading: boolean
  error: string | null
}

// ─── Filter / view state ──────────────────────────────────

export type ContractFilterStatus = ContractStatus | 'all'
export type ContractSortKey = 'updated' | 'name' | 'effective' | 'drift'
export type ContractViewMode = 'cards' | 'table'

export interface ContractsFilterState {
  search: string
  statuses: ContractFilterStatus[]
  sort: ContractSortKey
  sortDir: 'asc' | 'desc'
  view: ContractViewMode
}

// ─── Creation form ────────────────────────────────────────

export interface CreateContractFormValues {
  name: string
  version: string
  provider: Party
  consumers: Party[]   // min 1
  scope: ContractScope
  policy: ContractPolicy
  effectiveAt: string
  expiresAt: string | null
  publishImmediately: boolean
}

// ─── IPC summaries (returned by Rust commands) ────────────

export interface ContractDriftSummary {
  contractId: string
  status: ContractStatus
  driftCount: number
  breachCount: number
}

export interface ContractSummary {
  id: string
  name: string
  status: ContractStatus
  driftCount: number
  breachCount: number
  endpointCount: number
}

// ─── Computed helpers ─────────────────────────────────────

export interface ContractCounts {
  total: number
  active: number
  drift: number
  breach: number
  inReview: number
  draft: number
  paused: number
  expired: number
  /** Sum of all driftCount across all contracts (used in "Changes · 30d" card) */
  totalChanges: number
  /** Breakdown for summary row trend line */
  changesAdded: number
  changesRemoved: number
  changesModified: number
}
```

- [ ] **Step 2: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | grep "contracts.ts" | head -10
```

Expected: no errors from `contracts.ts`.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/types/contracts.ts
git commit -m "feat(contracts): TypeScript type definitions — Party, ChangeKind, full Contract shape"
```

---

## Task 2: Design tokens in `globals.css`

**Files:**
- Modify: `frontend/src/globals.css`

- [ ] **Step 1: Check existing tokens to avoid duplication**

```bash
grep -E "\-\-warning|\-\-success|\-\-destructive-soft|\-\-color-method" frontend/src/globals.css | head -20
```

Note which tokens already exist. Only add the missing ones.

- [ ] **Step 2: Add missing tokens to `:root` (light mode)**

In the `:root` block, add only the tokens missing from Step 1:

```css
/* Contract feature tokens — only add if not already present */

/* Warning (amber) — for drift state */
--warning: 38 92% 50%;
--warning-foreground: 0 0% 98%;
--warning-soft: 38 92% 93%;

/* Success (green) — for active/compliant state */
--success: 160 63% 45%;
--success-foreground: 0 0% 98%;
--success-soft: 160 50% 92%;

/* Destructive soft — for breach state background tint */
--destructive-soft: 0 74% 94%;

/* HTTP method colors */
--color-method-get: 160 63% 35%;
--color-method-post: 38 92% 48%;
--color-method-put: 213 88% 42%;
--color-method-delete: 0 74% 52%;
--color-method-patch: 283 52% 49%;
--color-method-options: 190 80% 42%;
--color-method-head: 330 65% 55%;
```

- [ ] **Step 3: Add to `.dark` block**

```css
/* Warning */
--warning: 38 92% 50%;
--warning-foreground: 0 0% 10%;
--warning-soft: 38 60% 16%;

/* Success */
--success: 160 63% 49%;
--success-foreground: 0 0% 10%;
--success-soft: 160 50% 14%;

/* Destructive soft */
--destructive-soft: 0 50% 18%;

/* HTTP method colors */
--color-method-get: 160 63% 49%;
--color-method-post: 38 92% 55%;
--color-method-put: 207 100% 55%;
--color-method-delete: 0 72% 55%;
--color-method-patch: 283 60% 62%;
--color-method-options: 190 80% 55%;
--color-method-head: 330 65% 65%;
```

- [ ] **Step 4: Add `pulseRed` animation**

At the end of `globals.css`:

```css
@keyframes pulseRed {
  0%, 100% { box-shadow: 0 0 0 0 hsl(var(--destructive) / 0.4); }
  50%       { box-shadow: 0 0 0 4px hsl(var(--destructive) / 0); }
}
.animate-pulse-red {
  animation: pulseRed 1.5s ease-in-out infinite;
}
@media (prefers-reduced-motion: reduce) {
  .animate-pulse-red {
    animation: none;
    box-shadow: 0 0 0 2px hsl(var(--destructive) / 0.5);
  }
}
```

- [ ] **Step 5: Commit**

```bash
git add frontend/src/globals.css
git commit -m "feat(contracts): design tokens — warning, success, destructive-soft, method colors, pulseRed"
```

---

## Task 3: Expose method colors from `src/lib/colors.ts`

**Files:**
- Modify: `frontend/src/lib/colors.ts`

- [ ] **Step 1: Find existing method color definitions**

```bash
grep -n "method\|GET\|POST\|DELETE" frontend/src/lib/colors.ts | head -20
```

- [ ] **Step 2: Add CSS variable mapping alongside existing JS values**

In `colors.ts`, add a new exported object that maps HTTP methods to their CSS variable references. Insert after the existing method color definitions (do NOT replace them — add alongside):

```typescript
/** CSS variable references for HTTP method colors.
 *  Use in Tailwind-incompatible contexts (inline styles, canvas, etc.)
 *  For Tailwind: use text-[hsl(var(--color-method-get))] etc. directly.
 */
export const METHOD_COLOR_VARS: Record<string, string> = {
  GET:     'hsl(var(--color-method-get))',
  POST:    'hsl(var(--color-method-post))',
  PUT:     'hsl(var(--color-method-put))',
  DELETE:  'hsl(var(--color-method-delete))',
  PATCH:   'hsl(var(--color-method-patch))',
  OPTIONS: 'hsl(var(--color-method-options))',
  HEAD:    'hsl(var(--color-method-head))',
}

/** Returns the Tailwind class for a given HTTP method. */
export function methodColorClass(method: string): string {
  const map: Record<string, string> = {
    GET:     'text-[hsl(var(--color-method-get))]',
    POST:    'text-[hsl(var(--color-method-post))]',
    PUT:     'text-[hsl(var(--color-method-put))]',
    DELETE:  'text-[hsl(var(--color-method-delete))]',
    PATCH:   'text-[hsl(var(--color-method-patch))]',
    OPTIONS: 'text-[hsl(var(--color-method-options))]',
    HEAD:    'text-[hsl(var(--color-method-head))]',
  }
  return map[method.toUpperCase()] ?? 'text-muted-foreground'
}
```

- [ ] **Step 3: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 4: Commit**

```bash
git add frontend/src/lib/colors.ts
git commit -m "feat(contracts): expose HTTP method CSS variable references from colors.ts"
```
