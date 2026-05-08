# SP4-03 — Frontend: Zustand Store + Selectors

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **⚠️ Worktree** — all commands run inside `.worktrees/contract-enhancement` on branch `feat/contract-lock-enhancement`.

**Goal:** Create `contractsSlice.ts` (Zustand store, no persist, IPC-backed), `contractsActions.ts` (thunk-style actions), and `contractsSelectors.ts` (memoized selectors) with full unit tests for selectors.

**Architecture (Option B):** No `persist` middleware — Rust is the store. All mutations call Tauri IPC first; on success update local cache. `recomputeDrift` calls the Rust command (not `drift.ts`). Changelog is embedded in `Contract` returned from IPC.

**Tech Stack:** TypeScript, Zustand, Vitest

**Spec:** `Implementation_Plan_v2.md §5`

**Depends on:** SP4-02 merged.

---

## Task 1: `contractsSlice.ts` + `contractsActions.ts`

**Files:**
- Create: `frontend/src/store/contracts/contractsSlice.ts`
- Create: `frontend/src/store/contracts/contractsActions.ts`

- [ ] **Step 1: Create `contractsSlice.ts`**

```typescript
import { create } from 'zustand'
import { subscribeWithSelector } from 'zustand/middleware'
import type { Contract, ContractsState, CreateContractFormValues } from '@/types/contracts'
import { contractsActions } from './contractsActions'

export interface ContractsStore extends ContractsState {
  // CRUD
  loadContracts: (collectionId: string) => Promise<void>
  createContract: (collectionId: string, values: CreateContractFormValues, requests: any[]) => Promise<Contract>
  updateContract: (id: string, patch: Partial<Contract>) => void
  deleteContract: (collectionId: string, id: string) => Promise<void>

  // Lifecycle (all call Tauri IPC, return updated Contract)
  publishContract: (collectionId: string, id: string) => Promise<void>
  pauseContract: (collectionId: string, id: string) => Promise<void>
  resumeContract: (collectionId: string, id: string) => Promise<void>
  renewContract: (collectionId: string, id: string, newExpiresAt: string | null) => Promise<void>
  sendForReview: (collectionId: string, id: string) => Promise<void>
  approveContract: (collectionId: string, id: string) => Promise<void>
  rejectContract: (collectionId: string, id: string) => Promise<void>
  duplicateContract: (collectionId: string, id: string) => Promise<void>

  // Drift (calls Rust recompute_drift command)
  recomputeDrift: (collectionId: string) => Promise<void>

  // UI
  setHovered: (id: string | null) => void
  upsert: (contract: Contract) => void
}

export const useContractsStore = create<ContractsStore>()(
  subscribeWithSelector((set, get) => ({
    byId: {},
    byCollection: {},
    hoveredId: null,
    loading: false,
    error: null,

    ...contractsActions(set, get),

    updateContract: (id, patch) =>
      set(state => ({
        byId: {
          ...state.byId,
          [id]: { ...state.byId[id], ...patch, updatedAt: new Date().toISOString() },
        },
      })),

    upsert: (contract) =>
      set(state => ({ byId: { ...state.byId, [contract.id]: contract } })),

    setHovered: (id) => set({ hoveredId: id }),
  }))
)
```

- [ ] **Step 2: Create `contractsActions.ts`**

```typescript
import type { Contract, ContractsStore } from './contractsSlice'
import type { CreateContractFormValues } from '@/types/contracts'
import * as api from '@/lib/tauri-api'
import { computeSnapshot } from '@/lib/contracts/snapshot'

type Set = (partial: Partial<ContractsStore> | ((s: ContractsStore) => Partial<ContractsStore>)) => void
type Get = () => ContractsStore

export function contractsActions(set: Set, get: Get) {
  function upsertInCollection(collectionId: string, contract: Contract) {
    set(state => {
      const existing = state.byCollection[collectionId] ?? []
      const ids = existing.includes(contract.id) ? existing : [contract.id, ...existing]
      return {
        byId: { ...state.byId, [contract.id]: contract },
        byCollection: { ...state.byCollection, [collectionId]: ids },
      }
    })
  }

  return {
    loadContracts: async (collectionId: string) => {
      set({ loading: true, error: null })
      try {
        // collectionId is the collection root path in Tauri
        const contracts: Contract[] = await api.listContracts(collectionId)
        const byId: Record<string, Contract> = {}
        const ids: string[] = []
        for (const c of contracts) { byId[c.id] = c; ids.push(c.id) }
        set(state => ({
          byId: { ...state.byId, ...byId },
          byCollection: { ...state.byCollection, [collectionId]: ids },
          loading: false,
        }))
      } catch (err) {
        set({ loading: false, error: String(err) })
      }
    },

    createContract: async (collectionId: string, values: CreateContractFormValues, _requests: any[]) => {
      const contract: Contract = await api.attachContract(collectionId, {
        title: values.name,
        provider: values.provider,
        consumers: values.consumers,
        version: values.version,
        effectiveDate: values.effectiveAt,
        expiresAt: values.expiresAt,
        documentPaths: [],
        scope: values.scope,
        policy: values.policy,
        initialSnapshots: [],
        publishImmediately: values.publishImmediately,
      })
      upsertInCollection(collectionId, contract)
      return contract
    },

    deleteContract: async (collectionId: string, id: string) => {
      await api.deleteContract(collectionId, id)
      set(state => {
        const { [id]: _, ...rest } = state.byId
        return {
          byId: rest,
          byCollection: {
            ...state.byCollection,
            [collectionId]: (state.byCollection[collectionId] ?? []).filter(cid => cid !== id),
          },
        }
      })
    },

    publishContract: async (collectionId: string, id: string) => {
      const contract = await api.publishContract(collectionId, id, [])
      upsertInCollection(collectionId, contract)
    },

    pauseContract: async (collectionId: string, id: string) => {
      const contract = await api.pauseContract(collectionId, id)
      upsertInCollection(collectionId, contract)
    },

    resumeContract: async (collectionId: string, id: string) => {
      const contract = await api.resumeContract(collectionId, id)
      upsertInCollection(collectionId, contract)
    },

    renewContract: async (collectionId: string, id: string, newExpiresAt: string | null) => {
      const contract = await api.renewContract(collectionId, id, newExpiresAt)
      upsertInCollection(collectionId, contract)
    },

    sendForReview: async (collectionId: string, id: string) => {
      const contract = await api.sendForReview(collectionId, id)
      upsertInCollection(collectionId, contract)
    },

    approveContract: async (collectionId: string, id: string) => {
      const contract = await api.approveContract(collectionId, id)
      upsertInCollection(collectionId, contract)
    },

    rejectContract: async (collectionId: string, id: string) => {
      const contract = await api.rejectContract(collectionId, id)
      upsertInCollection(collectionId, contract)
    },

    duplicateContract: async (collectionId: string, id: string) => {
      const contract = await api.duplicateContract(collectionId, id)
      upsertInCollection(collectionId, contract)
    },

    recomputeDrift: async (collectionId: string) => {
      // Calls Rust — returns updated summaries, then re-fetches contracts
      await api.recomputeDrift(collectionId, [])
      // Reload the full contract list to get updated drift counts + changelog
      await get().loadContracts(collectionId)
    },
  }
}
```

- [ ] **Step 3: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | grep "contracts" | head -15
```

- [ ] **Step 4: Commit**

```bash
git add frontend/src/store/contracts/contractsSlice.ts
git add frontend/src/store/contracts/contractsActions.ts
git commit -m "feat(contracts): Zustand store + actions — no persist, all mutations via Tauri IPC"
```

---

## Task 2: `contractsSelectors.ts` + unit tests

**Files:**
- Create: `frontend/src/store/contracts/contractsSelectors.ts`
- Create: `frontend/src/store/contracts/contractsSelectors.test.ts`

- [ ] **Step 1: Write failing tests**

Create `frontend/src/store/contracts/contractsSelectors.test.ts`:

```typescript
import { describe, it, expect } from 'vitest'
import {
  selectContractsForCollection,
  selectContractCounts,
  groupContracts,
  sortContractsAttentionFirst,
} from './contractsSelectors'
import type { Contract } from '@/types/contracts'

function c(id: string, status: Contract['status'], driftCount = 0, breachCount = 0): Contract {
  return {
    id, collectionId: 'col1', name: `Contract ${id}`, version: '1.0.0',
    status, provider: { id: 'p', name: 'Provider', kind: 'team' },
    consumers: [{ id: 'c', name: 'Consumer', kind: 'team' }],
    scope: { type: 'collection' },
    policy: { breakingChangePolicy: 'lenient', noticeDays: 30, uptimeSla: null },
    effectiveAt: '2026-01-01', expiresAt: null,
    signedSnapshot: null, driftCount, breachCount, endpointCount: 1,
    changelog: [], createdBy: 'u1',
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
  }
}

const byId = {
  r1: c('r1', 'active'),
  r2: c('r2', 'drift', 3, 0),
  r3: c('r3', 'breach', 2, 1),
  r4: c('r4', 'draft'),
  r5: c('r5', 'paused'),
}
const byCollection = { col1: ['r1', 'r2', 'r3', 'r4', 'r5'] }

describe('selectContractsForCollection', () => {
  it('returns contracts for the given collection', () => {
    const result = selectContractsForCollection(byId, byCollection, 'col1')
    expect(result).toHaveLength(5)
  })
  it('returns empty array for unknown collection', () => {
    const result = selectContractsForCollection(byId, byCollection, 'unknown')
    expect(result).toHaveLength(0)
  })
})

describe('selectContractCounts', () => {
  it('counts all statuses correctly', () => {
    const contracts = Object.values(byId)
    const counts = selectContractCounts(contracts)
    expect(counts.total).toBe(5)
    expect(counts.active).toBe(1)
    expect(counts.drift).toBe(1)
    expect(counts.breach).toBe(1)
    expect(counts.draft).toBe(1)
    expect(counts.paused).toBe(1)
  })
  it('sums drift counts for totalChanges', () => {
    const contracts = Object.values(byId)
    const counts = selectContractCounts(contracts)
    // r2 has 3 drift, r3 has 2 drift
    expect(counts.totalChanges).toBe(5)
  })
})

describe('groupContracts', () => {
  it('puts breach and drift in attention group', () => {
    const contracts = Object.values(byId)
    const { attention, active, inactive } = groupContracts(contracts)
    expect(attention.map(c => c.id).sort()).toEqual(['r2', 'r3'].sort())
    expect(active.map(c => c.id)).toEqual(['r1'])
    expect(inactive.map(c => c.id).sort()).toEqual(['r4', 'r5'].sort())
  })
})

describe('sortContractsAttentionFirst', () => {
  it('breach appears before drift before active', () => {
    const contracts = Object.values(byId)
    const sorted = sortContractsAttentionFirst(contracts)
    const statuses = sorted.map(c => c.status)
    const breachIdx = statuses.indexOf('breach')
    const driftIdx = statuses.indexOf('drift')
    const activeIdx = statuses.indexOf('active')
    expect(breachIdx).toBeLessThan(driftIdx)
    expect(driftIdx).toBeLessThan(activeIdx)
  })
})
```

- [ ] **Step 2: Run — verify they fail**

```bash
cd frontend && yarn vitest run src/store/contracts/contractsSelectors.test.ts 2>&1 | tail -5
```

- [ ] **Step 3: Implement `contractsSelectors.ts`**

```typescript
import type { Contract, ContractCounts, ContractStatus } from '@/types/contracts'
import { needsAttention, isActive, isInactive } from '@/lib/contracts/statusMachine'

export function selectContractsForCollection(
  byId: Record<string, Contract>,
  byCollection: Record<string, string[]>,
  collectionId: string,
): Contract[] {
  const ids = byCollection[collectionId] ?? []
  return ids.map(id => byId[id]).filter(Boolean)
}

export function selectContractCounts(contracts: Contract[]): ContractCounts {
  let active = 0, drift = 0, breach = 0, inReview = 0, draft = 0, paused = 0, expired = 0
  let totalChanges = 0, changesAdded = 0, changesRemoved = 0, changesModified = 0

  for (const c of contracts) {
    if (isActive(c.status)) active++
    else if (c.status === 'drift') drift++
    else if (c.status === 'breach') breach++
    else if (c.status === 'in_review') inReview++
    else if (c.status === 'draft') draft++
    else if (c.status === 'paused') paused++
    else if (c.status === 'expired') expired++

    totalChanges += c.driftCount
    // Approximate breakdown from changelog entries (last 100)
    for (const entry of c.changelog) {
      if (entry.kind === 'add') changesAdded++
      else if (entry.kind === 'remove') changesRemoved++
      else changesModified++
    }
  }

  return { total: contracts.length, active, drift, breach, inReview, draft, paused, expired,
           totalChanges, changesAdded, changesRemoved, changesModified }
}

export function groupContracts(contracts: Contract[]): {
  attention: Contract[]
  active: Contract[]
  inactive: Contract[]
} {
  return {
    attention: contracts.filter(c => needsAttention(c.status)),
    active:    contracts.filter(c => isActive(c.status)),
    inactive:  contracts.filter(c => isInactive(c.status)),
  }
}

const STATUS_ORDER: Record<ContractStatus, number> = {
  breach: 0, drift: 1, in_review: 2,
  active: 3, expiring_in_30_days: 4,
  draft: 5, paused: 6, expired: 7,
} as any

export function sortContractsAttentionFirst(contracts: Contract[]): Contract[] {
  return [...contracts].sort((a, b) => {
    const orderDiff = (STATUS_ORDER[a.status] ?? 9) - (STATUS_ORDER[b.status] ?? 9)
    if (orderDiff !== 0) return orderDiff
    return b.updatedAt.localeCompare(a.updatedAt)
  })
}
```

- [ ] **Step 4: Run tests — all 7 pass**

```bash
cd frontend && yarn vitest run src/store/contracts/contractsSelectors.test.ts 2>&1 | tail -5
```

Expected: `7 tests passed`.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/store/contracts/contractsSelectors.ts
git add frontend/src/store/contracts/contractsSelectors.test.ts
git commit -m "feat(contracts): contractsSelectors — counts, grouping, attention-first sort + 7 tests"
```

---

## Task 3: `useContracts.ts` + `useContractDrift.ts` hooks

**Files:**
- Create: `frontend/src/hooks/useContracts.ts`
- Create: `frontend/src/hooks/useContractDrift.ts`

- [ ] **Step 1: Create `useContracts.ts`**

```typescript
import { useMemo } from 'react'
import { useContractsStore } from '@/store/contracts/contractsSlice'
import {
  selectContractsForCollection,
  selectContractCounts,
  sortContractsAttentionFirst,
} from '@/store/contracts/contractsSelectors'

export function useContracts(collectionId: string) {
  const byId = useContractsStore(s => s.byId)
  const byCollection = useContractsStore(s => s.byCollection)
  const loading = useContractsStore(s => s.loading)

  const contracts = useMemo(
    () => sortContractsAttentionFirst(
      selectContractsForCollection(byId, byCollection, collectionId)
    ),
    [byId, byCollection, collectionId],
  )

  const counts = useMemo(() => selectContractCounts(contracts), [contracts])

  return { contracts, counts, isLoading: loading }
}
```

- [ ] **Step 2: Create `useContractDrift.ts`**

```typescript
import { useEffect, useRef, useCallback } from 'react'
import { useContractsStore } from '@/store/contracts/contractsSlice'

function useDebouncedCallback<T extends (...args: any[]) => any>(fn: T, ms: number): T {
  const timer = useRef<ReturnType<typeof setTimeout>>()
  return useCallback(
    ((...args: any[]) => {
      clearTimeout(timer.current)
      timer.current = setTimeout(() => fn(...args), ms)
    }) as T,
    [fn, ms],
  )
}

/**
 * Subscribes to collection changes and triggers Rust drift recomputation.
 * Debounced at 250ms. Fires on tab focus (visibilitychange).
 *
 * Option B: calls recomputeDrift Tauri command (not frontend drift.ts engine).
 * Wire this hook into ContractsTab.
 */
export function useContractDrift(collectionId: string) {
  const recomputeDrift = useContractsStore(s => s.recomputeDrift)

  const debounced = useDebouncedCallback(
    () => { recomputeDrift(collectionId) },
    250,
  )

  // Fire on tab focus
  useEffect(() => {
    const onVisibility = () => { if (!document.hidden) debounced() }
    document.addEventListener('visibilitychange', onVisibility)
    return () => document.removeEventListener('visibilitychange', onVisibility)
  }, [debounced])

  // Expose trigger for callers that want to fire manually
  return { triggerDrift: debounced }
}
```

- [ ] **Step 3: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 4: Commit**

```bash
git add frontend/src/hooks/useContracts.ts frontend/src/hooks/useContractDrift.ts
git commit -m "feat(contracts): useContracts + useContractDrift hooks"
```
