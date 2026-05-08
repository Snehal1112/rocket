# SP4-02 — Frontend: snapshot.ts + drift.ts (preview) + avatarColor.ts

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **⚠️ Worktree** — all commands run inside `.worktrees/contract-enhancement` on branch `feat/contract-lock-enhancement`.

**Goal:** Create three pure utility libraries — `snapshot.ts` (builds a `RequestShapeMap` from collection requests), `drift.ts` (frontend-only preview drift computation used in `NewContractModal`), and `avatarColor.ts` — each with full unit tests.

**Architecture (Option B):** `drift.ts` is a **preview-only** engine — it runs client-side for the `NewContractModal` "what will be snapshotted" preview and for optimistic UI only. The authoritative drift computation runs in Rust via `recompute_drift` Tauri command. `drift.ts` never touches the Zustand store directly.

**Tech Stack:** TypeScript, Vitest

**Spec:** `Implementation_Plan_v2.md §6, §11`

**Depends on:** SP4-01 merged.

---

## Task 1: `snapshot.ts` + unit tests

**Files:**
- Create: `frontend/src/lib/contracts/snapshot.ts`
- Create: `frontend/src/lib/contracts/snapshot.test.ts`

- [ ] **Step 1: Write failing tests**

Create `frontend/src/lib/contracts/snapshot.test.ts`:

```typescript
import { describe, it, expect } from 'vitest'
import { computeSnapshot } from './snapshot'
import type { ContractScope } from '@/types/contracts'

const mockRequests = [
  {
    id: 'r1', method: 'GET', path: '/payments',
    params: [{ key: 'currency', required: true }],
    headers: [{ key: 'Authorization', required: true }],
    body: null, folderId: 'folder-a',
  },
  {
    id: 'r2', method: 'POST', path: '/payments',
    params: [],
    headers: [{ key: 'Content-Type', required: true }],
    body: { schema: '{"type":"object"}' }, folderId: 'folder-a',
  },
  {
    id: 'r3', method: 'GET', path: '/users',
    params: [{ key: 'page', required: false }],
    headers: [],
    body: null, folderId: 'folder-b',
  },
]

describe('computeSnapshot', () => {
  it('collection scope returns all requests', () => {
    const scope: ContractScope = { type: 'collection' }
    const snap = computeSnapshot(mockRequests, scope)
    expect(Object.keys(snap)).toHaveLength(3)
    expect(snap['r1'].method).toBe('GET')
    expect(snap['r1'].path).toBe('/payments')
  })

  it('collection scope captures params and headers correctly', () => {
    const snap = computeSnapshot(mockRequests, { type: 'collection' })
    expect(snap['r1'].params).toEqual([{ key: 'currency', required: true }])
    expect(snap['r1'].headers).toEqual([{ key: 'Authorization', required: true }])
  })

  it('folder scope returns only matching folder requests', () => {
    const scope: ContractScope = { type: 'folder', folderId: 'folder-a', path: 'payments/' }
    const snap = computeSnapshot(mockRequests, scope)
    expect(Object.keys(snap)).toHaveLength(2)
    expect(snap['r1']).toBeDefined()
    expect(snap['r2']).toBeDefined()
    expect(snap['r3']).toBeUndefined()
  })

  it('requests scope returns only specified requestIds', () => {
    const scope: ContractScope = { type: 'requests', requestIds: ['r1', 'r3'] }
    const snap = computeSnapshot(mockRequests, scope)
    expect(Object.keys(snap)).toHaveLength(2)
    expect(snap['r1']).toBeDefined()
    expect(snap['r3']).toBeDefined()
    expect(snap['r2']).toBeUndefined()
  })

  it('captures bodySchema when body has schema', () => {
    const snap = computeSnapshot(mockRequests, { type: 'collection' })
    expect(snap['r2'].bodySchema).toBe('{"type":"object"}')
    expect(snap['r1'].bodySchema).toBeUndefined()
  })

  it('empty requests returns empty map', () => {
    const snap = computeSnapshot([], { type: 'collection' })
    expect(Object.keys(snap)).toHaveLength(0)
  })
})
```

- [ ] **Step 2: Run tests — verify they fail**

```bash
cd frontend && yarn vitest run src/lib/contracts/snapshot.test.ts 2>&1 | tail -5
```

Expected: `Cannot find module './snapshot'`.

- [ ] **Step 3: Implement `snapshot.ts`**

Create `frontend/src/lib/contracts/snapshot.ts`:

```typescript
import type { ContractScope, RequestShapeMap, RequestShape, ParamShape } from '@/types/contracts'

/**
 * Builds a RequestShapeMap from a collection's live requests, filtered to scope.
 *
 * Option B note: this function is used ONLY in NewContractModal for the live
 * "preview" panel showing which endpoints will be snapshotted. The authoritative
 * snapshot is taken by Rust when `publish_contract` is called.
 */
export function computeSnapshot(
  requests: CollectionRequest[],
  scope: ContractScope,
): RequestShapeMap {
  const inScope = filterRequestsByScope(requests, scope)
  return Object.fromEntries(
    inScope.map(req => [
      req.id,
      buildShape(req),
    ])
  )
}

function buildShape(req: CollectionRequest): RequestShape {
  return {
    method: req.method,
    path: req.path,
    params: (req.params ?? []).map(p => ({ key: p.key, required: p.required ?? false, type: p.type })),
    headers: (req.headers ?? []).map(h => ({ key: h.key, required: h.required ?? false })),
    bodySchema: req.body?.schema ?? undefined,
  }
}

function filterRequestsByScope(
  requests: CollectionRequest[],
  scope: ContractScope,
): CollectionRequest[] {
  if (scope.type === 'collection') return requests
  if (scope.type === 'folder') return requests.filter(r => r.folderId === scope.folderId)
  return requests.filter(r => scope.requestIds.includes(r.id))
}

/** Loose type for any request object from the collection store.
 *  The actual type comes from rocket-collection's IPC shape — adjust
 *  field names if the real type differs (e.g. `query` vs `params`). */
export interface CollectionRequest {
  id: string
  method: string
  path: string
  folderId?: string
  params?: Array<{ key: string; required?: boolean; type?: string }>
  headers?: Array<{ key: string; required?: boolean }>
  body?: { schema?: string } | null
}
```

- [ ] **Step 4: Run tests — verify they pass**

```bash
cd frontend && yarn vitest run src/lib/contracts/snapshot.test.ts 2>&1 | tail -5
```

Expected: `6 tests passed`.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/contracts/snapshot.ts frontend/src/lib/contracts/snapshot.test.ts
git commit -m "feat(contracts): snapshot.ts — computeSnapshot with 6 unit tests (collection/folder/requests scope)"
```

---

## Task 2: `drift.ts` (preview-only) + 12 unit tests

**Files:**
- Create: `frontend/src/lib/contracts/drift.ts`
- Create: `frontend/src/lib/contracts/drift.test.ts`

- [ ] **Step 1: Write failing tests first**

Create `frontend/src/lib/contracts/drift.test.ts`:

```typescript
import { describe, it, expect } from 'vitest'
import { computeDrift } from './drift'
import type { Contract } from '@/types/contracts'

function makeContract(overrides: Partial<Contract> = {}): Contract {
  return {
    id: 'c1',
    collectionId: 'col1',
    name: 'Payments API',
    version: '1.0.0',
    status: 'active',
    provider: { id: 'p1', name: 'Billing', kind: 'team' },
    consumers: [{ id: 'c1', name: 'Platform', kind: 'team' }],
    scope: { type: 'collection' },
    policy: { breakingChangePolicy: 'lenient', noticeDays: 30, uptimeSla: null },
    effectiveAt: '2026-01-01',
    expiresAt: null,
    signedSnapshot: {
      'r1': {
        method: 'GET', path: '/payments',
        params: [{ key: 'currency', required: true }, { key: 'page', required: false }],
        headers: [{ key: 'Authorization', required: true }],
      },
    },
    driftCount: 0, breachCount: 0, endpointCount: 1,
    changelog: [], createdBy: 'user1',
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    ...overrides,
  }
}

const currentRequests = [
  { id: 'r1', method: 'GET', path: '/payments',
    params: [{ key: 'currency', required: true }, { key: 'page', required: false }],
    headers: [{ key: 'Authorization', required: true }],
    body: null, folderId: 'root',
  },
]

describe('computeDrift — no changes', () => {
  it('returns zero counts when snapshot matches current', () => {
    const report = computeDrift(makeContract(), currentRequests)
    expect(report.driftCount).toBe(0)
    expect(report.breachCount).toBe(0)
    expect(report.diffs).toHaveLength(0)
  })
})

describe('computeDrift — method change (always breaking)', () => {
  it('detects method change as breaking for all policies', () => {
    const requests = [{ ...currentRequests[0], method: 'POST' }]
    for (const policy of ['strict', 'lenient', 'additive_ok'] as const) {
      const contract = makeContract({ policy: { breakingChangePolicy: policy, noticeDays: 30, uptimeSla: null } })
      const report = computeDrift(contract, requests)
      expect(report.breachCount).toBeGreaterThan(0)
      const change = report.diffs[0]?.changes.find(c => c.field === 'method')
      expect(change?.isBreaking).toBe(true)
    }
  })
})

describe('computeDrift — path change (always breaking)', () => {
  it('detects path change as breaking', () => {
    const requests = [{ ...currentRequests[0], path: '/v2/payments' }]
    const report = computeDrift(makeContract(), requests)
    const change = report.diffs[0]?.changes.find(c => c.field === 'path')
    expect(change?.isBreaking).toBe(true)
  })
})

describe('computeDrift — required param removed', () => {
  it('is always breaking regardless of policy', () => {
    // Remove 'currency' (required: true)
    const requests = [{ ...currentRequests[0], params: [{ key: 'page', required: false }] }]
    for (const policy of ['strict', 'lenient', 'additive_ok'] as const) {
      const contract = makeContract({ policy: { breakingChangePolicy: policy, noticeDays: 30, uptimeSla: null } })
      const report = computeDrift(contract, requests)
      const change = report.diffs[0]?.changes.find(c => c.field === 'params.currency')
      expect(change?.isBreaking).toBe(true)
    }
  })
})

describe('computeDrift — optional param removed', () => {
  it('is breaking for strict and lenient, not for additive_ok', () => {
    // Remove 'page' (required: false)
    const requests = [{ ...currentRequests[0], params: [{ key: 'currency', required: true }] }]

    const strict = computeDrift(makeContract({ policy: { breakingChangePolicy: 'strict', noticeDays: 30, uptimeSla: null } }), requests)
    expect(strict.diffs[0]?.changes.find(c => c.field === 'params.page')?.isBreaking).toBe(true)

    const lenient = computeDrift(makeContract({ policy: { breakingChangePolicy: 'lenient', noticeDays: 30, uptimeSla: null } }), requests)
    expect(lenient.diffs[0]?.changes.find(c => c.field === 'params.page')?.isBreaking).toBe(true)

    const additive = computeDrift(makeContract({ policy: { breakingChangePolicy: 'additive_ok', noticeDays: 30, uptimeSla: null } }), requests)
    expect(additive.diffs[0]?.changes.find(c => c.field === 'params.page')?.isBreaking).toBe(false)
  })
})

describe('computeDrift — new param added', () => {
  it('is breaking only for strict policy', () => {
    const requests = [{ ...currentRequests[0], params: [...currentRequests[0].params, { key: 'format', required: false }] }]

    const strict = computeDrift(makeContract({ policy: { breakingChangePolicy: 'strict', noticeDays: 30, uptimeSla: null } }), requests)
    expect(strict.diffs[0]?.changes.find(c => c.field === 'params.format')?.isBreaking).toBe(true)

    const lenient = computeDrift(makeContract({ policy: { breakingChangePolicy: 'lenient', noticeDays: 30, uptimeSla: null } }), requests)
    expect(lenient.diffs[0]?.changes.find(c => c.field === 'params.format')?.isBreaking).toBe(false)

    const additive = computeDrift(makeContract({ policy: { breakingChangePolicy: 'additive_ok', noticeDays: 30, uptimeSla: null } }), requests)
    expect(additive.diffs[0]?.changes.find(c => c.field === 'params.format')?.isBreaking).toBe(false)
  })
})

describe('computeDrift — entire request removed', () => {
  it('is always breaking', () => {
    const report = computeDrift(makeContract(), []) // no current requests
    expect(report.breachCount).toBeGreaterThan(0)
    const change = report.diffs[0]?.changes.find(c => c.field === 'request')
    expect(change?.kind).toBe('remove')
    expect(change?.isBreaking).toBe(true)
  })
})

describe('computeDrift — new request added', () => {
  it('is breaking only for strict policy', () => {
    const requests = [
      ...currentRequests,
      { id: 'r2', method: 'DELETE', path: '/payments/:id', params: [], headers: [], body: null, folderId: 'root' },
    ]

    const strict = computeDrift(makeContract({ policy: { breakingChangePolicy: 'strict', noticeDays: 30, uptimeSla: null } }), requests)
    expect(strict.diffs.find(d => d.requestId === 'r2')?.changes[0].isBreaking).toBe(true)

    const lenient = computeDrift(makeContract({ policy: { breakingChangePolicy: 'lenient', noticeDays: 30, uptimeSla: null } }), requests)
    expect(lenient.diffs.find(d => d.requestId === 'r2')?.changes[0].isBreaking).toBe(false)
  })
})

describe('computeDrift — draft contract', () => {
  it('returns empty report when signedSnapshot is null (draft)', () => {
    const draft = makeContract({ status: 'draft', signedSnapshot: null })
    const report = computeDrift(draft, currentRequests)
    expect(report.driftCount).toBe(0)
    expect(report.diffs).toHaveLength(0)
  })
})

describe('computeDrift — paused contract', () => {
  it('returns empty report (paused contracts are skipped)', () => {
    const paused = makeContract({ status: 'paused' })
    const report = computeDrift(paused, currentRequests)
    expect(report.driftCount).toBe(0)
  })
})
```

- [ ] **Step 2: Run tests — verify they fail**

```bash
cd frontend && yarn vitest run src/lib/contracts/drift.test.ts 2>&1 | tail -5
```

Expected: `Cannot find module './drift'`.

- [ ] **Step 3: Implement `drift.ts`**

Create `frontend/src/lib/contracts/drift.ts`:

```typescript
import type { Contract, DriftReport, RequestDiff, FieldChange, BreakingChangePolicy, RequestShapeMap } from '@/types/contracts'
import { computeSnapshot, type CollectionRequest } from './snapshot'

/**
 * PREVIEW-ONLY frontend drift engine.
 *
 * Option B: This function is used exclusively in NewContractModal to show a
 * live "what would change" preview. It is NOT used to update the Zustand store.
 * The authoritative drift computation happens in Rust via recompute_drift.
 *
 * Returns empty report for draft/paused contracts or when signedSnapshot is null.
 */
export function computeDrift(
  contract: Contract,
  currentRequests: CollectionRequest[],
): DriftReport {
  const empty: DriftReport = {
    contractId: contract.id,
    computedAt: new Date().toISOString(),
    diffs: [],
    driftCount: 0,
    breachCount: 0,
  }

  // Paused contracts are not monitored
  if (contract.status === 'paused') return empty

  // Draft / unpublished contracts have no snapshot yet
  if (!contract.signedSnapshot) return empty

  const currentSnapshot = computeSnapshot(currentRequests, contract.scope)
  const diffs: RequestDiff[] = []
  const policy = contract.policy.breakingChangePolicy

  // Diff each snapshotted request against current
  for (const [reqId, signed] of Object.entries(contract.signedSnapshot)) {
    const current = currentSnapshot[reqId]
    const changes: FieldChange[] = []

    if (!current) {
      // Entire request removed — always breaking
      changes.push({
        field: 'request',
        kind: 'remove',
        before: `${signed.method} ${signed.path}`,
        isBreaking: true,
      })
    } else {
      if (current.method !== signed.method) {
        changes.push({ field: 'method', kind: 'modify', before: signed.method, after: current.method, isBreaking: true })
      }
      if (current.path !== signed.path) {
        changes.push({ field: 'path', kind: 'modify', before: signed.path, after: current.path, isBreaking: true })
      }
      changes.push(...diffParams('params', signed.params, current.params, policy))
      changes.push(...diffParams('headers', signed.headers, current.headers, policy))
    }

    if (changes.length > 0) {
      diffs.push({ requestId: reqId, method: signed.method, path: signed.path, changes })
    }
  }

  // New endpoints added to collection
  for (const [reqId, current] of Object.entries(currentSnapshot)) {
    if (!contract.signedSnapshot[reqId]) {
      diffs.push({
        requestId: reqId,
        method: current.method,
        path: current.path,
        changes: [{
          field: 'request',
          kind: 'add',
          after: `${current.method} ${current.path}`,
          isBreaking: policy === 'strict',
        }],
      })
    }
  }

  const driftCount = diffs.reduce((n, d) => n + d.changes.length, 0)
  const breachCount = diffs.reduce((n, d) => n + d.changes.filter(c => c.isBreaking).length, 0)

  return { contractId: contract.id, computedAt: new Date().toISOString(), diffs, driftCount, breachCount }
}

function diffParams(
  prefix: string,
  signed: Array<{ key: string; required: boolean }>,
  current: Array<{ key: string; required: boolean }>,
  policy: BreakingChangePolicy,
): FieldChange[] {
  const changes: FieldChange[] = []
  const signedMap = Object.fromEntries(signed.map(p => [p.key, p]))
  const currentMap = Object.fromEntries(current.map(p => [p.key, p]))

  // Removed params
  for (const [key, sp] of Object.entries(signedMap)) {
    if (!currentMap[key]) {
      // Required param removed → always breaking; optional → breaking except additive_ok
      const isBreaking = sp.required || policy !== 'additive_ok'
      changes.push({ field: `${prefix}.${key}`, kind: 'remove', before: key, isBreaking })
    } else if (currentMap[key].required !== sp.required) {
      // optional → required is breaking
      const isBreaking = !sp.required && currentMap[key].required
      changes.push({
        field: `${prefix}.${key}.required`,
        kind: 'modify',
        before: String(sp.required),
        after: String(currentMap[key].required),
        isBreaking,
      })
    }
  }

  // Added params
  for (const key of Object.keys(currentMap)) {
    if (!signedMap[key]) {
      changes.push({ field: `${prefix}.${key}`, kind: 'add', after: key, isBreaking: policy === 'strict' })
    }
  }

  return changes
}
```

- [ ] **Step 4: Run tests — all 12 must pass**

```bash
cd frontend && yarn vitest run src/lib/contracts/drift.test.ts 2>&1 | tail -10
```

Expected: `12 tests passed`.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/contracts/drift.ts frontend/src/lib/contracts/drift.test.ts
git commit -m "feat(contracts): drift.ts — preview-only engine + 12 unit tests (all policy variants)"
```

---

## Task 3: `avatarColor.ts` + `statusMachine.ts`

**Files:**
- Create: `frontend/src/lib/contracts/avatarColor.ts`
- Create: `frontend/src/lib/contracts/statusMachine.ts`

- [ ] **Step 1: Create `avatarColor.ts`**

```typescript
/** Deterministic avatar color from party name/seed. Cycles through 7 brand colors. */
const AVATAR_PALETTE = [
  '#3B82F6', // blue
  '#A855F7', // purple
  '#22C55E', // green
  '#F59E0B', // amber
  '#EC4899', // pink
  '#14B8A6', // teal
  '#EF4444', // red
]

function hashString(s: string): number {
  let h = 0
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) | 0
  return Math.abs(h)
}

export function avatarColorForName(name: string, override?: string): string {
  if (override) return override
  return AVATAR_PALETTE[hashString(name) % AVATAR_PALETTE.length]
}

export function initialsForName(name: string): string {
  const words = name.trim().split(/\s+/)
  if (words.length >= 2) return (words[0][0] + words[1][0]).toUpperCase()
  return name.slice(0, 2).toUpperCase()
}
```

- [ ] **Step 2: Create `statusMachine.ts`**

The frontend `transitionStatus` accepts a `DriftReport` (unlike the Rust machine which takes `StatusEvent`). This is used for **optimistic UI only** — the source of truth is Rust.

```typescript
import type { ContractStatus, DriftReport } from '@/types/contracts'

/**
 * Determines the next contract status based on a computed DriftReport.
 * Used for optimistic UI updates only — Rust is the authoritative source.
 *
 * Manual transitions (pause/resume/publish/etc.) are handled by the
 * Tauri commands directly and update the store via IPC response.
 */
export function transitionStatus(
  current: ContractStatus,
  report: DriftReport,
): ContractStatus {
  // Paused and draft contracts are not affected by drift
  if (current === 'paused' || current === 'draft') return current

  if (report.breachCount > 0) return 'breach'
  if (report.driftCount > 0) return 'drift'

  // No changes — if currently drift/breach, revert to active (changes reverted)
  if (current === 'drift' || current === 'breach') return 'active'

  return current
}

/** Human-readable label for each status. */
export function statusLabel(status: ContractStatus): string {
  const labels: Record<ContractStatus, string> = {
    draft:               'Draft',
    active:              'Active',
    drift:               'Drift',
    breach:              'Breaching',
    in_review:           'In review',
    paused:              'Paused',
    expired:             'Expired',
  }
  return labels[status] ?? status
}

/** Returns the display label for use inside ContractStatusChip (includes count for drift/breach). */
export function statusChipLabel(status: ContractStatus, count?: number): string {
  if (status === 'drift' && count && count > 0) return `⚠ Drift · ${count}`
  if (status === 'breach') return 'Breaching'
  return statusLabel(status)
}

export function needsAttention(status: ContractStatus): boolean {
  return ['breach', 'drift', 'in_review'].includes(status)
}

export function isActive(status: ContractStatus): boolean {
  return status === 'active' || status === 'expiring_in_30_days' as ContractStatus
}

export function isInactive(status: ContractStatus): boolean {
  return ['draft', 'paused', 'expired'].includes(status)
}
```

- [ ] **Step 3: Write statusMachine tests**

Create `frontend/src/lib/contracts/statusMachine.test.ts`:

```typescript
import { describe, it, expect } from 'vitest'
import { transitionStatus } from './statusMachine'
import type { DriftReport } from '@/types/contracts'

function report(driftCount: number, breachCount: number): DriftReport {
  return { contractId: 'c1', computedAt: '', diffs: [], driftCount, breachCount }
}

describe('transitionStatus', () => {
  it('active + no drift → stays active', () => {
    expect(transitionStatus('active', report(0, 0))).toBe('active')
  })
  it('active + drift → drift', () => {
    expect(transitionStatus('active', report(3, 0))).toBe('drift')
  })
  it('active + breach → breach', () => {
    expect(transitionStatus('active', report(2, 1))).toBe('breach')
  })
  it('drift + no changes → reverts to active (changes reverted)', () => {
    expect(transitionStatus('drift', report(0, 0))).toBe('active')
  })
  it('breach + no changes → reverts to active', () => {
    expect(transitionStatus('breach', report(0, 0))).toBe('active')
  })
  it('paused + breach → stays paused (not monitored)', () => {
    expect(transitionStatus('paused', report(5, 2))).toBe('paused')
  })
  it('draft + drift → stays draft (not published)', () => {
    expect(transitionStatus('draft', report(3, 1))).toBe('draft')
  })
})
```

```bash
cd frontend && yarn vitest run src/lib/contracts/statusMachine.test.ts 2>&1 | tail -5
```

Expected: `7 tests passed`.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/lib/contracts/avatarColor.ts
git add frontend/src/lib/contracts/statusMachine.ts
git add frontend/src/lib/contracts/statusMachine.test.ts
git commit -m "feat(contracts): avatarColor, statusMachine (DriftReport→status) + 7 unit tests"
```
