# SP8-03 — Status Bar + Stable useContractDrift + Barrel + Export OpenAPI Rust Command

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **⚠️ Worktree** — all commands run inside `.worktrees/contract-enhancement` on branch `feat/contract-lock-enhancement`.

**Goal:** Create `ContractsStatusItem` with separators before AND after (spec §9); write the stable `useContractDrift` hook using `useRef` for the timer; create barrel `index.ts`; add `export_contract_openapi` Rust command (Option B: Rust generates YAML, frontend triggers save dialog).

**Spec:** `Implementation_Plan_v2.md §9, §10, §7.10 Export as OpenAPI`

**Depends on:** SP8-02 merged.

---

## Task 1: `ContractsStatusItem.tsx` + wire into StatusBar

**Files:**
- Create: `frontend/src/components/status-bar/ContractsStatusItem.tsx`
- Modify: existing StatusBar component

- [ ] **Step 1: Find the StatusBar component and understand its separator pattern**

```bash
# Find StatusBar
find frontend/src -name "StatusBar*" -o -name "*StatusBar*" | grep -v node_modules | head -5
grep -rn "StatusBar\|status-bar" frontend/src/components --include="*.tsx" -l | head -3
```

```bash
# Read to understand layout and how separators are used
cat <found-status-bar-file> | head -80
```

Note: (a) how existing items are rendered, (b) the exact separator JSX used (e.g. `<span>|</span>`, a CSS `border-l`, a `Separator` component). Use the SAME pattern.

- [ ] **Step 2: Find how active collection is accessed in StatusBar**

```bash
grep -n "activeCollection\|collectionId\|useWorkspace\|useCollection" \
  <found-status-bar-file> | head -10
```

Use the same pattern to get `activeCollectionId` and `activeCollectionName`.

- [ ] **Step 3: Create `ContractsStatusItem.tsx`**

```tsx
import { useMemo } from 'react'
import { Lock } from 'lucide-react'
import { useContractsStore } from '@/store/contracts/contractsSlice'
import { usePaneStore } from '@/stores/pane-store'

interface ContractsStatusItemProps {
  collectionId: string | null
  collectionName: string | null
}

/**
 * Status bar chip: "{n} contracts · {n} drifting · {n} breaching"
 * Shows separator | before and after (per spec §9).
 * Renders nothing when collectionId is null or no contracts exist.
 */
export function ContractsStatusItem({
  collectionId,
  collectionName,
}: ContractsStatusItemProps) {
  const byId         = useContractsStore(s => s.byId)
  const byCollection = useContractsStore(s => s.byCollection)
  const openContractTab = usePaneStore(s => s.openContractTab)

  const meta = useMemo(() => {
    if (!collectionId) return null
    const ids = byCollection[collectionId] ?? []
    const contracts = ids.map(id => byId[id]).filter(Boolean)
    if (contracts.length === 0) return null
    return {
      total:       contracts.length,
      driftCount:  contracts.filter(c => c.status === 'drift').length,
      breachCount: contracts.filter(c => c.status === 'breach').length,
    }
  }, [byId, byCollection, collectionId])

  if (!meta || !collectionId || !collectionName) return null

  return (
    <>
      {/* Separator BEFORE — use same element/classname as other StatusBar separators */}
      <span className="mx-1 text-muted-foreground/30 select-none" aria-hidden="true">|</span>

      <button
        type="button"
        className="flex items-center gap-[5px] text-[11px] text-muted-foreground hover:text-foreground transition-colors"
        onClick={() => openContractTab(collectionId, collectionName)}
        aria-label={`${meta.total} contract${meta.total !== 1 ? 's' : ''}`}
      >
        <Lock className="w-[11px] h-[11px]" aria-hidden="true" />
        <span>{meta.total} contract{meta.total !== 1 ? 's' : ''}</span>
        {meta.driftCount > 0 && (
          <span className="text-[hsl(var(--warning))]">
            · {meta.driftCount} drifting
          </span>
        )}
        {meta.breachCount > 0 && (
          <span className="text-[hsl(var(--destructive))]">
            · {meta.breachCount} breaching
          </span>
        )}
      </button>

      {/* Separator AFTER — spec §9 says separator on both sides */}
      <span className="mx-1 text-muted-foreground/30 select-none" aria-hidden="true">|</span>
    </>
  )
}
```

**Important:** Replace the `className` on the separator `<span>` with whatever existing separators use in the StatusBar file you found in Step 1.

- [ ] **Step 4: Wire into StatusBar**

Add `<ContractsStatusItem>` to the StatusBar JSX in the same row as other items, using the active collection from Step 2:

```tsx
import { ContractsStatusItem } from './ContractsStatusItem'

// Inside StatusBar render, alongside existing items:
<ContractsStatusItem
  collectionId={activeCollectionId ?? null}
  collectionName={activeCollectionName ?? null}
/>
```

- [ ] **Step 5: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | grep "StatusBar\|ContractsStatus" | head -5
```

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/status-bar/ContractsStatusItem.tsx
git add <status-bar-file>
git commit -m "feat(contracts): ContractsStatusItem — separators before+after, count+drift+breach"
```

---

## Task 2: Stable `useContractDrift` + barrel `index.ts`

**Files:**
- Modify: `frontend/src/hooks/useContractDrift.ts`
- Create: `frontend/src/components/contracts/index.ts`

- [ ] **Step 1: Verify Tauri event name for request saves**

```bash
grep -rn "emit\|request_saved\|collection:" \
  crates/rocket-infra/src/ src-tauri/src/commands/ --include="*.rs" \
  | grep -v "//.*" | grep -iv "test" | head -15
```

Find the exact string passed to `app_handle.emit(...)` or `window.emit(...)` when a request is saved. Note it exactly — it will replace `'collection:request_saved'` below.

- [ ] **Step 2: Write the stable `useContractDrift` hook**

The previous version had a `useCallback`-in-`useCallback` chain that could cause the debounce timer to reset on every render. This version uses `useRef` directly for the timer and a `useRef` for the stable function:

```typescript
import { useEffect, useRef } from 'react'
import { useContractsStore } from '@/store/contracts/contractsSlice'
// Adjust if the Tauri event API import differs in this project:
// run: grep -rn "from '@tauri-apps" frontend/src --include="*.ts" | head -5
import { listen } from '@tauri-apps/api/event'

const DRIFT_DEBOUNCE_MS = 250

/**
 * Subscribes to collection:request_saved Tauri events and triggers
 * Rust drift recomputation debounced at 250ms.
 *
 * Uses a stable ref-based debounce so the timer is never reset by
 * re-renders. Also fires on visibilitychange (user returns to the app).
 *
 * Option B: calls recompute_drift Tauri command — never uses frontend drift.ts.
 */
export function useContractDrift(collectionId: string) {
  const recomputeDrift = useContractsStore(s => s.recomputeDrift)

  // Stable ref to current recomputeDrift to avoid capturing stale closures
  const recomputeRef = useRef(recomputeDrift)
  const collectionIdRef = useRef(collectionId)
  useEffect(() => { recomputeRef.current = recomputeDrift }, [recomputeDrift])
  useEffect(() => { collectionIdRef.current = collectionId }, [collectionId])

  // Stable debounced trigger (ref-based, never reset by re-renders)
  const timerRef = useRef<ReturnType<typeof setTimeout>>()
  const trigger = useRef(() => {
    clearTimeout(timerRef.current)
    timerRef.current = setTimeout(
      () => recomputeRef.current(collectionIdRef.current),
      DRIFT_DEBOUNCE_MS,
    )
  })

  // Subscribe to Tauri request-saved events
  useEffect(() => {
    let unlisten: (() => void) | undefined

    // REPLACE 'collection:request_saved' with the actual event name found in Step 1
    listen<{ collectionId: string }>('collection:request_saved', event => {
      if (event.payload.collectionId === collectionIdRef.current) {
        trigger.current()
      }
    })
      .then(fn => { unlisten = fn })
      .catch(err => console.warn('[useContractDrift] failed to subscribe:', err))

    return () => {
      unlisten?.()
      clearTimeout(timerRef.current)
    }
  }, []) // empty deps — subscribed once, uses refs for current values

  // Also fire on tab focus
  useEffect(() => {
    function onVisibility() {
      if (!document.hidden) trigger.current()
    }
    document.addEventListener('visibilitychange', onVisibility)
    return () => document.removeEventListener('visibilitychange', onVisibility)
  }, [])

  // Expose for tests / manual trigger
  return { triggerDrift: trigger.current }
}
```

- [ ] **Step 3: Create barrel `index.ts`**

```typescript
// frontend/src/components/contracts/index.ts
export { ContractsTab }         from './ContractsTab'
export { ContractCard }         from './ContractCard'
export { ContractCardSkeleton } from './ContractCardSkeleton'
export { ContractStatusChip }   from './ContractStatusChip'
export { ContractContextMenu }  from './ContractContextMenu'
export { PartyPill }            from './PartyPill'
export { PartyAvatar }          from './PartyAvatar'
export { ScopeTag }             from './ScopeTag'
export { MiniChangelog }        from './MiniChangelog'
export { ChangeChip }           from './ChangeChip'
export { ContractsSummaryRow }  from './ContractsSummaryRow'
export { ContractsFilterBar }   from './ContractsFilterBar'
export { ContractsEmptyState }  from './ContractsEmptyState'
export { ContractsGroupHeader } from './ContractsGroupHeader'
export { NewContractModal }     from './NewContractModal'
export type { ContractAction }  from './ContractCard'
```

- [ ] **Step 4: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 5: Commit**

```bash
git add frontend/src/hooks/useContractDrift.ts
git add frontend/src/components/contracts/index.ts
git commit -m "feat(contracts): stable useContractDrift (ref-based debounce, no render resets) + barrel index"
```

---

## Task 3: Export as OpenAPI — Rust command + Tauri registration

**Files:**
- Modify: `crates/rocket-app/src/contract_service.rs`
- Modify: `src-tauri/src/commands/contract.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add `export_as_openapi_yaml` to `ContractService`**

In `crates/rocket-app/src/contract_service.rs`, add:

```rust
/// Generates a minimal OpenAPI 3.0 YAML stub for the given contract.
/// Returns the YAML as a String — the frontend triggers the save dialog.
pub fn export_as_openapi_yaml(
    &self,
    collection_root: &std::path::Path,
    id: ulid::Ulid,
) -> rocket_shared::error::DomainResult<String> {
    let contract = self.repo.get(collection_root, id)?;
    let snapshot = self.repo.load_snapshot(collection_root, id).ok();

    let title = &contract.name;
    let version = &contract.version;

    let mut paths_yaml = String::new();
    if let Some(snap) = &snapshot {
        for entry in &snap.entries {
            let path = entry.url_pattern.trim_start_matches('/');
            let method = entry.method.to_lowercase();
            paths_yaml.push_str(&format!(
                "  /{}:\n    {}:\n      summary: '{}'\n      responses:\n        '200':\n          description: OK\n",
                path, method,
                format!("{} {}", entry.method, entry.url_pattern)
            ));
        }
    }
    if paths_yaml.is_empty() {
        paths_yaml = "  /example:\n    get:\n      summary: Example endpoint\n      responses:\n        '200':\n          description: OK\n".to_string();
    }

    let yaml = format!(
        "openapi: '3.0.3'\ninfo:\n  title: '{}'\n  version: '{}'\npaths:\n{}",
        title, version, paths_yaml
    );

    Ok(yaml)
}
```

Note: adjust field names (`entry.url_pattern`, `entry.method`) to match the actual `RequestSignatureSnapshot` struct fields in the project.

- [ ] **Step 2: Add Tauri command**

In `src-tauri/src/commands/contract.rs`, add:

```rust
/// Returns an OpenAPI 3.0 YAML stub for a contract as a String.
/// The frontend is responsible for triggering the save dialog.
#[tauri::command]
pub fn export_contract_openapi(
    collection_root: String,
    contract_id: String,
    svc: tauri::State<'_, rocket_app::ContractService>,
) -> Result<String, String> {
    let id = ulid::Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.export_as_openapi_yaml(&std::path::PathBuf::from(&collection_root), id)
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 3: Register command in `lib.rs`**

In `src-tauri/src/lib.rs`, in the `tauri::generate_handler![...]` macro, add:

```rust
commands::contract::export_contract_openapi,
```

- [ ] **Step 4: Compile check**

```bash
cargo check -p rocket-app
cargo check -p rocket-tauri 2>&1 | grep "^error" | head -10
```

Fix any field name mismatches in `export_as_openapi_yaml`.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/contract_service.rs
git add src-tauri/src/commands/contract.rs
git add src-tauri/src/lib.rs
git commit -m "feat(contracts): export_contract_openapi Rust command — generates OpenAPI 3.0 YAML stub"
```
