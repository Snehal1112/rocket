# SP9-02 — Telemetry (all 9 events, every action) + Breadcrumb

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **⚠️ Worktree** — all commands run inside `.worktrees/contract-enhancement` on branch `feat/contract-lock-enhancement`.

**Goal:** Wire all 9 `track()` events from spec §14 with complete code for every call site. No stubs, no "apply the same pattern" references — every action gets its full implementation shown. Add breadcrumb `CollectionName / Contracts`.

**Spec:** `Implementation_Plan_v2.md §14, PR 6`

**Depends on:** SP9-01 merged.

---

## Task 1: Discover telemetry API + wire `NewContractModal` (3 events)

**Files:**
- Modify: `frontend/src/components/contracts/NewContractModal.tsx`

- [ ] **Step 1: Find the telemetry API**

```bash
grep -rn "\btrack\b\|analytics\.\|posthog\.\|amplitude\.\|mixpanel\." \
  frontend/src --include="*.ts" --include="*.tsx" \
  | grep -v node_modules | grep -v ".test." | grep -E "import|from" | head -8
```

Read one complete usage:

```bash
TRACKFILE=$(grep -rl "\btrack(" frontend/src/components --include="*.tsx" | grep -v node_modules | grep -v ".test." | head -1)
grep -B2 -A3 "\btrack(" "$TRACKFILE" | head -20
```

Note **exactly**: import path, function name, call signature. Use it in every `track(...)` call in this plan.

- [ ] **Step 2: Wire telemetry into `NewContractModal.tsx`**

Open `NewContractModal.tsx`. Add the telemetry import at the top.

In the `submit` function, `createContract` already returns the saved `Contract`. Verify it is captured:

```typescript
const contract = await createContract(collectionId, { ... }, [])
```

If not captured, add the `const contract =` assignment.

After the `createContract` call, add telemetry based on the path:

```typescript
async function submit(publishImmediately: boolean) {
  // ... existing validation and error handling unchanged ...

  setSaving(true)
  try {
    // ... build scope, policy, provider, consumers unchanged ...

    const contract = await createContract(collectionId, {
      name: form.name.trim(),
      version: form.version.trim(),
      provider,
      consumers,
      scope,
      policy,
      effectiveAt: form.effectiveAt,
      expiresAt: form.expiresAt || null,
      publishImmediately,
    }, [])

    // ── Telemetry ────────────────────────────────────────────
    if (publishImmediately) {
      if (publishImmediately) await recomputeDrift(collectionId)

      // Event 1: contracts.created
      try {
        track('contracts.created', {
          scopeType: form.scopeType,          // 'collection' | 'folder' | 'requests'
          consumerCount: consumers.length,
          publishedImmediately: true,
        })
      } catch {}

      // Event 2: contracts.published
      try {
        track('contracts.published', {
          contractId: contract.id,
          endpointCount: contract.endpointCount,
        })
      } catch {}

      // Toast: "Contract created and published."
      // REPLACE with actual toast call (found in SP9-01 Task 3 Step 1):
      /* toast.success('Contract created and published.') */

    } else {
      // Event 3: contracts.draft_saved
      try {
        track('contracts.draft_saved', {
          collectionId,
          scopeType: form.scopeType,
          consumerCount: consumers.length,
        })
      } catch {}

      // Toast: "Contract saved as draft."
      /* toast('Contract saved as draft.') */
    }
    // ─────────────────────────────────────────────────────────

    resetAndClose()
  } catch (err) {
    setErrors({ _global: String(err) })
  } finally {
    setSaving(false)
  }
}
```

Replace the toast comment stubs with real calls using the API you found in SP9-01 Task 3 Step 1.

- [ ] **Step 3: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | grep "NewContractModal" | head -5
```

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/contracts/NewContractModal.tsx
git commit -m "feat(contracts): telemetry — contracts.created, published, draft_saved in NewContractModal"
```

---

## Task 2: Telemetry in `contractsActions.ts` — 6 actions × `status_changed` + `drift_detected`

**Files:**
- Modify: `frontend/src/store/contracts/contractsActions.ts`

- [ ] **Step 1: Add telemetry import**

At the top of `contractsActions.ts`, add the telemetry import found in Task 1 Step 1.

- [ ] **Step 2: Wire `contracts.status_changed` into every lifecycle action**

Below is the complete implementation for **all 7 actions**. Replace the entire lifecycle actions section in `contractsActions.ts`:

```typescript
pauseContract: async (collectionId, id) => {
  const prev = get().byId[id]?.status
  const contract = await api.pauseContract(collectionId, id)
  upsertInCollection(collectionId, contract)
  try { track('contracts.status_changed', { contractId: id, from: prev, to: contract.status }) } catch {}
},

resumeContract: async (collectionId, id) => {
  const prev = get().byId[id]?.status
  const contract = await api.resumeContract(collectionId, id)
  upsertInCollection(collectionId, contract)
  try { track('contracts.status_changed', { contractId: id, from: prev, to: contract.status }) } catch {}
},

publishContract: async (collectionId, id) => {
  const prev = get().byId[id]?.status
  const contract = await api.publishContract(collectionId, id, [])
  upsertInCollection(collectionId, contract)
  try { track('contracts.status_changed', { contractId: id, from: prev, to: contract.status }) } catch {}
},

approveContract: async (collectionId, id) => {
  const prev = get().byId[id]?.status
  const contract = await api.approveContract(collectionId, id)
  upsertInCollection(collectionId, contract)
  try { track('contracts.status_changed', { contractId: id, from: prev, to: contract.status }) } catch {}
},

rejectContract: async (collectionId, id) => {
  const prev = get().byId[id]?.status
  const contract = await api.rejectContract(collectionId, id)
  upsertInCollection(collectionId, contract)
  try { track('contracts.status_changed', { contractId: id, from: prev, to: contract.status }) } catch {}
},

renewContract: async (collectionId, id, newExpiresAt) => {
  const prev = get().byId[id]?.status
  const contract = await api.renewContract(collectionId, id, newExpiresAt)
  upsertInCollection(collectionId, contract)
  try { track('contracts.status_changed', { contractId: id, from: prev, to: contract.status }) } catch {}
},

sendForReview: async (collectionId, id) => {
  const prev = get().byId[id]?.status
  const contract = await api.sendForReview(collectionId, id)
  upsertInCollection(collectionId, contract)
  try { track('contracts.status_changed', { contractId: id, from: prev, to: contract.status }) } catch {}
},
```

- [ ] **Step 3: Wire `contracts.drift_detected` in `recomputeDrift`**

Replace the `recomputeDrift` action entirely:

```typescript
recomputeDrift: async (collectionId) => {
  // Snapshot statuses before recompute to detect transitions
  const beforeStatuses: Record<string, string> = {}
  for (const id of (get().byCollection[collectionId] ?? [])) {
    const status = get().byId[id]?.status
    if (status) beforeStatuses[id] = status
  }

  await api.recomputeDrift(collectionId, [])
  await get().loadContracts(collectionId)

  // Emit drift_detected for every contract that newly entered drift or breach
  for (const id of (get().byCollection[collectionId] ?? [])) {
    const prev = beforeStatuses[id]
    const curr = get().byId[id]
    if (!curr) continue

    const nowDrifting = curr.status === 'drift' || curr.status === 'breach'
    const wasDrifting = prev === 'drift' || prev === 'breach'
    if (nowDrifting && !wasDrifting) {
      try {
        track('contracts.drift_detected', {
          contractId: id,
          driftCount:  curr.driftCount,
          breachCount: curr.breachCount,
          // Milliseconds since the contract was first signed (createdAt)
          elapsedMsSinceSigned: curr.createdAt
            ? Date.now() - new Date(curr.createdAt).getTime()
            : undefined,
        })
      } catch {}
    }
  }
},
```

- [ ] **Step 4: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | grep "contractsActions" | head -5
```

- [ ] **Step 5: Commit**

```bash
git add frontend/src/store/contracts/contractsActions.ts
git commit -m "feat(contracts): telemetry — status_changed (7 actions), drift_detected (elapsedMsSinceSigned)"
```

---

## Task 3: `contracts.card_action` + `contracts.tab_opened/filter_used/empty_state_cta` + breadcrumb

**Files:**
- Modify: `frontend/src/components/contracts/ContractsTab.tsx`

- [ ] **Step 1: Verify and fill all `track()` stubs in `ContractsTab`**

Find all stubs:

```bash
grep -n "REPLACE.*track\|\/\/ track\|contracts\." \
  frontend/src/components/contracts/ContractsTab.tsx
```

The stubs were added in SP7-02 as comments. Replace each one with a real call.

**`contracts.tab_opened`** — in the `loadContracts().then()` callback:

```typescript
.then(() => {
  setLastSync(new Date())
  try {
    track('contracts.tab_opened', {
      collectionId,
      contractCount: useContractsStore.getState().byCollection[collectionId]?.length ?? 0,
    })
  } catch {}
})
```

**`contracts.filter_used`** — in the three filter bar callbacks:

```typescript
onSearch={q => {
  setSearch(q)
  if (q.length > 0) {
    try { track('contracts.filter_used', { filterType: 'search' }) } catch {}
  }
}}
onToggleStatus={s => {
  toggleStatus(s)
  try { track('contracts.filter_used', { filterType: 'status', value: s }) } catch {}
}}
onSetSort={s => {
  setSort(s)
  try { track('contracts.filter_used', { filterType: 'sort', value: s }) } catch {}
}}
```

**`contracts.empty_state_cta`** — in the empty state CTA callback:

```typescript
<ContractsEmptyState
  onStartFromCurrent={() => {
    try { track('contracts.empty_state_cta', { action: 'start_from_current' }) } catch {}
    setModalOpen(true)
  }}
/>
```

- [ ] **Step 2: Wire `contracts.card_action` in `handleAction`**

Find `handleAction` in `ContractsTab.tsx`. The SP7-02 plan added a `// REPLACE: track(...)` stub at the top. Replace the entire `handleAction` function with this complete version:

```typescript
const handleAction = useCallback(
  async (action: ContractAction, contractId: string) => {
    // Event: contracts.card_action (spec §14)
    try {
      track('contracts.card_action', { contractId, action })
    } catch {}

    try {
      switch (action) {
        case 'pause':           await store.pauseContract(collectionId, contractId); break
        case 'resume':          await store.resumeContract(collectionId, contractId); break
        case 'delete':          await store.deleteContract(collectionId, contractId); break
        case 'duplicate':       await store.duplicateContract(collectionId, contractId); break
        case 'publish':         await store.publishContract(collectionId, contractId); break
        case 'resign':          await store.publishContract(collectionId, contractId); break
        case 'send_for_review': await store.sendForReview(collectionId, contractId); break
        case 'approve':         await store.approveContract(collectionId, contractId); break
        case 'reject':          await store.rejectContract(collectionId, contractId); break
        case 'renew':           await store.renewContract(collectionId, contractId, null); break
        // 'open', 'edit', 'view_changelog', 'export' → routing/navigation
      }
    } catch (err) {
      console.error('[ContractsTab] action error:', action, err)
    }
  },
  [collectionId, store],
)
```

- [ ] **Step 3: Verify or add breadcrumb**

First check if the project has a `Breadcrumb` component:

```bash
grep -rn "Breadcrumb\|breadcrumb" frontend/src/components/ui/ --include="*.tsx" | head -5
```

**If `Breadcrumb` component exists**, replace the pane header `<h1>Contracts</h1>` block in `ContractsTab.tsx` with:

```tsx
<div>
  <nav aria-label="Breadcrumb" className="text-xs text-muted-foreground mb-0.5">
    <ol className="flex items-center gap-1">
      <li>{collectionName}</li>
      <li aria-hidden="true">/</li>
      <li className="text-foreground font-medium">Contracts</li>
    </ol>
  </nav>
  <div className="text-xs text-muted-foreground flex items-center gap-2 mt-0.5">
    <span>{counts.total} contract{counts.total !== 1 ? 's' : ''}</span>
    {lastSync && (
      <>
        <span className="w-[3px] h-[3px] rounded-full bg-muted-foreground/40" aria-hidden="true" />
        <span>{lastSyncLabel()}</span>
      </>
    )}
  </div>
</div>
```

**If no `Breadcrumb` component**, the existing subtitle `{collectionName} · {n} contracts` already shows the breadcrumb information. Enhance it slightly:

```tsx
<div>
  <h1 className="text-xl font-semibold text-foreground leading-tight tracking-[-0.01em]">
    Contracts
  </h1>
  <div
    className="text-xs text-muted-foreground flex items-center gap-1.5 mt-0.5 flex-wrap"
    aria-label={`${collectionName} / Contracts`}
  >
    <span>{collectionName}</span>
    <span aria-hidden="true">/</span>
    <span className="text-foreground/70">Contracts</span>
    <span className="w-[3px] h-[3px] rounded-full bg-muted-foreground/40" aria-hidden="true" />
    <span>{counts.total} contract{counts.total !== 1 ? 's' : ''}</span>
    {lastSync && (
      <>
        <span className="w-[3px] h-[3px] rounded-full bg-muted-foreground/40" aria-hidden="true" />
        <span>{lastSyncLabel()}</span>
      </>
    )}
  </div>
</div>
```

- [ ] **Step 4: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 5: Final unit test run**

```bash
cd frontend && yarn vitest run \
  src/components/contracts/ \
  src/lib/contracts/ \
  src/store/contracts/ \
  src/hooks/ \
  2>&1 | tail -15
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/contracts/ContractsTab.tsx
git commit -m "feat(contracts): telemetry — card_action, tab_opened, filter_used, empty_state_cta; CollectionName / Contracts breadcrumb"
```
