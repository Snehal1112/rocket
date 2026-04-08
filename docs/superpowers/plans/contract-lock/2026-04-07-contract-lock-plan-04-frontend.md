# Contract Lock — Plan 04: Frontend — Store + Badge + Panel + Dialog

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the Zustand contract store, the sidebar lock badge, the contract panel (parties + changelog), and the attach-contract dialog.

**Architecture:** `contract-store.ts` mirrors the five IPC commands. `ContractBadge` is a small icon shown on sidebar items that have an active contract. `ContractPanel` is a right-side sheet showing parties, status, and the changelog table. `AttachContractDialog` is a form dialog for creating a contract. All components use shadcn/ui primitives only.

**Tech Stack:** React 18, TypeScript, Zustand, shadcn/ui, Lucide icons, Tauri v2 IPC

**Depends on:** Plan 03 merged.

---

## File Map

| File | Action |
|---|---|
| `frontend/src/lib/tauri-api.ts` | Modify — add 5 contract API wrappers |
| `frontend/src/stores/contract-store.ts` | Create — Zustand store |
| `frontend/src/components/contract/ContractBadge.tsx` | Create — lock icon badge for sidebar |
| `frontend/src/components/contract/ContractPanel.tsx` | Create — right-side sheet |
| `frontend/src/components/contract/AttachContractDialog.tsx` | Create — create/attach dialog |
| `frontend/src/components/layout/CollectionsSidebar.tsx` | Modify — render `ContractBadge` |

---

## Task 1: API wrappers + Zustand store

**Files:**
- Modify: `frontend/src/lib/tauri-api.ts`
- Create: `frontend/src/stores/contract-store.ts`

- [ ] **Step 1: Add contract API wrappers to `tauri-api.ts`**

Add these alongside existing wrappers:

```typescript
// ── Contract Lock ──────────────────────────────────────────

export interface ContractScope =
  | { type: 'collection' }
  | { type: 'folder'; rel_path: string }
  | { type: 'request'; rel_path: string }

export interface Contract {
  id: string
  title: string
  provider: string
  consumer: string
  project: string
  version: string
  effectiveDate: string         // "YYYY-MM-DD"
  expiryDate: string | null
  documentPath: string | null
  enforcementMode: 'informational' | 'warn' | 'block'
  scope: ContractScope
}

export interface ChangelogEntry {
  timestamp: string
  requestPath: string
  field: string
  changeType: 'changed' | 'added' | 'removed'
  oldValue: string | null
  newValue: string | null
}

export interface ContractChangelog {
  contractId: string
  entries: ChangelogEntry[]
}

export interface AttachContractInput {
  title: string
  provider: string
  consumer: string
  project: string
  version: string
  effectiveDate: string
  expiryDate: string | null
  documentPath: string | null
  scope: ContractScope
  initialSnapshots: []         // frontend sends empty — backend builds from saved state
}

export const attachContract = (collectionRoot: string, input: AttachContractInput) =>
  invoke<Contract>('attach_contract', { collectionRoot, input })

export const listContracts = (collectionRoot: string) =>
  invoke<Contract[]>('list_contracts', { collectionRoot })

export const getContract = (collectionRoot: string, contractId: string) =>
  invoke<Contract>('get_contract', { collectionRoot, contractId })

export const deleteContract = (collectionRoot: string, contractId: string) =>
  invoke<void>('delete_contract', { collectionRoot, contractId })

export const getContractChangelog = (collectionRoot: string, contractId: string) =>
  invoke<ContractChangelog>('get_contract_changelog', { collectionRoot, contractId })
```

- [ ] **Step 2: Create `contract-store.ts`**

```typescript
import { create } from 'zustand'
import {
  attachContract,
  listContracts,
  deleteContract,
  getContractChangelog,
  Contract,
  ContractChangelog,
  AttachContractInput,
} from '@/lib/tauri-api'

interface ContractStore {
  // State
  contracts: Contract[]
  changelogs: Record<string, ContractChangelog>  // keyed by contract id
  loading: boolean
  error: string | null

  // Actions
  loadContracts: (collectionRoot: string) => Promise<void>
  attachContract: (collectionRoot: string, input: AttachContractInput) => Promise<Contract>
  removeContract: (collectionRoot: string, contractId: string) => Promise<void>
  loadChangelog: (collectionRoot: string, contractId: string) => Promise<void>

  // Selectors
  contractsForScope: (scopeType: 'collection' | 'folder' | 'request', relPath?: string) => Contract[]
  contractStatus: (contract: Contract) => 'active' | 'expiring' | 'expired'
}

export const useContractStore = create<ContractStore>((set, get) => ({
  contracts: [],
  changelogs: {},
  loading: false,
  error: null,

  loadContracts: async (collectionRoot) => {
    set({ loading: true, error: null })
    try {
      const contracts = await listContracts(collectionRoot)
      set({ contracts, loading: false })
    } catch (e) {
      set({ error: String(e), loading: false })
    }
  },

  attachContract: async (collectionRoot, input) => {
    const contract = await attachContract(collectionRoot, input)
    set((s) => ({ contracts: [...s.contracts, contract] }))
    return contract
  },

  removeContract: async (collectionRoot, contractId) => {
    await deleteContract(collectionRoot, contractId)
    set((s) => ({
      contracts: s.contracts.filter((c) => c.id !== contractId),
    }))
  },

  loadChangelog: async (collectionRoot, contractId) => {
    const log = await getContractChangelog(collectionRoot, contractId)
    set((s) => ({
      changelogs: { ...s.changelogs, [contractId]: log },
    }))
  },

  contractsForScope: (scopeType, relPath) => {
    return get().contracts.filter((c) => {
      if (scopeType === 'collection') return c.scope.type === 'collection'
      if (scopeType === 'folder') return c.scope.type === 'folder' && c.scope.rel_path === relPath
      if (scopeType === 'request') return c.scope.type === 'request' && c.scope.rel_path === relPath
      return false
    })
  },

  contractStatus: (contract) => {
    const today = new Date()
    today.setHours(0, 0, 0, 0)
    if (!contract.expiryDate) return 'active'
    const exp = new Date(contract.expiryDate)
    if (exp < today) return 'expired'
    const diff = (exp.getTime() - today.getTime()) / (1000 * 60 * 60 * 24)
    if (diff <= 30) return 'expiring'
    return 'active'
  },
}))
```

- [ ] **Step 3: Verify TypeScript**

```bash
cd frontend && yarn tsc --noEmit
```

Expected: no errors.

---

## Task 2: ContractBadge + ContractPanel

**Files:**
- Create: `frontend/src/components/contract/ContractBadge.tsx`
- Create: `frontend/src/components/contract/ContractPanel.tsx`

- [ ] **Step 1: Create `ContractBadge.tsx`**

Small lock icon shown inline on sidebar items. Clicking it opens the `ContractPanel` sheet.

```tsx
import { Lock } from 'lucide-react'
import { useState } from 'react'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { Contract } from '@/lib/tauri-api'
import { useContractStore } from '@/stores/contract-store'
import { ContractPanel } from './ContractPanel'

interface ContractBadgeProps {
  contracts: Contract[]
  collectionRoot: string
}

export function ContractBadge({ contracts, collectionRoot }: ContractBadgeProps) {
  const [open, setOpen] = useState(false)
  const [selected, setSelected] = useState<Contract | null>(null)
  const contractStatus = useContractStore((s) => s.contractStatus)

  if (contracts.length === 0) return null

  const firstContract = contracts[0]
  const status = contractStatus(firstContract)

  const iconColor =
    status === 'expired' ? 'text-destructive' :
    status === 'expiring' ? 'text-warning' :
    'text-muted-foreground'

  const handleClick = (e: React.MouseEvent) => {
    e.stopPropagation()
    setSelected(firstContract)
    setOpen(true)
  }

  return (
    <>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            onClick={handleClick}
            className={`inline-flex items-center justify-center h-4 w-4 rounded-sm hover:bg-accent ${iconColor}`}
            aria-label="View contract"
          >
            <Lock className="h-3 w-3" />
          </button>
        </TooltipTrigger>
        <TooltipContent side="right">
          <p className="text-xs">{firstContract.title}</p>
          <p className="text-xs text-muted-foreground">
            {firstContract.provider} → {firstContract.consumer}
          </p>
        </TooltipContent>
      </Tooltip>

      {selected && (
        <ContractPanel
          open={open}
          onOpenChange={setOpen}
          contract={selected}
          collectionRoot={collectionRoot}
        />
      )}
    </>
  )
}
```

- [ ] **Step 2: Create `ContractPanel.tsx`**

Right-side sheet showing parties, status chip, dates, and changelog table.

```tsx
import { useEffect } from 'react'
import { Sheet, SheetContent, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { Badge } from '@/components/ui/badge'
import { Separator } from '@/components/ui/separator'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Button } from '@/components/ui/button'
import { Trash2 } from 'lucide-react'
import { Contract } from '@/lib/tauri-api'
import { useContractStore } from '@/stores/contract-store'

interface ContractPanelProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  contract: Contract
  collectionRoot: string
}

export function ContractPanel({ open, onOpenChange, contract, collectionRoot }: ContractPanelProps) {
  const { loadChangelog, changelogs, removeContract, contractStatus } = useContractStore()
  const changelog = changelogs[contract.id]
  const status = contractStatus(contract)

  useEffect(() => {
    if (open) loadChangelog(collectionRoot, contract.id)
  }, [open, contract.id])

  const handleDelete = async () => {
    await removeContract(collectionRoot, contract.id)
    onOpenChange(false)
  }

  const statusBadgeVariant = status === 'expired' ? 'destructive' : status === 'expiring' ? 'warning' : 'default'
  const statusLabel = status === 'expired' ? 'Expired' : status === 'expiring' ? 'Expiring soon' : 'Active'

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="w-[480px] sm:w-[540px] overflow-y-auto">
        <SheetHeader className="mb-4">
          <div className="flex items-start justify-between gap-2">
            <SheetTitle className="text-base leading-tight">{contract.title}</SheetTitle>
            <Badge variant={statusBadgeVariant}>{statusLabel}</Badge>
          </div>
        </SheetHeader>

        {/* Parties — pill badges with coloured dots per design spec */}
        <div className="rounded-lg border p-4 mb-4 space-y-3 text-sm">
          <div className="flex items-center gap-2 flex-wrap">
            <div className="flex items-center gap-2 bg-secondary rounded-full px-3 py-1 text-xs">
              <span className="w-2 h-2 rounded-full bg-[#534AB7] shrink-0" />
              <span>Provider: {contract.provider}</span>
            </div>
            <span className="text-muted-foreground text-sm">→</span>
            <div className="flex items-center gap-2 bg-secondary rounded-full px-3 py-1 text-xs">
              <span className="w-2 h-2 rounded-full bg-[#1D9E75] shrink-0" />
              <span>Consumer: {contract.consumer}</span>
            </div>
          </div>
          <Separator />
          <div className="flex justify-between">
            <span className="text-muted-foreground">Project</span>
            <span>{contract.project}</span>
          </div>
          <div className="flex justify-between">
            <span className="text-muted-foreground">Version</span>
            <span>{contract.version}</span>
          </div>
          <div className="flex justify-between">
            <span className="text-muted-foreground">Effective</span>
            <span>{contract.effectiveDate}</span>
          </div>
          {contract.expiryDate && (
            <div className="flex justify-between">
              <span className="text-muted-foreground">Expires</span>
              <span>{contract.expiryDate}</span>
            </div>
          )}
        </div>

        {/* Changelog */}
        <p className="text-xs font-medium text-muted-foreground uppercase tracking-wide mb-2">
          Change log
        </p>
        {!changelog || changelog.entries.length === 0 ? (
          <p className="text-sm text-muted-foreground py-4 text-center border rounded-lg">
            No changes recorded since contract was signed.
          </p>
        ) : (
          <div className="border rounded-lg overflow-hidden mb-4">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="text-xs">Date</TableHead>
                  <TableHead className="text-xs">Field</TableHead>
                  <TableHead className="text-xs">Type</TableHead>
                  <TableHead className="text-xs">Old</TableHead>
                  <TableHead className="text-xs">New</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {changelog.entries.map((entry, i) => (
                  <TableRow key={i}>
                    <TableCell className="text-xs text-muted-foreground whitespace-nowrap">
                      {new Date(entry.timestamp).toLocaleDateString()}
                    </TableCell>
                    <TableCell className="text-xs font-mono">{entry.field}</TableCell>
                    <TableCell>
                      <Badge
                        variant={
                          entry.changeType === 'removed' ? 'destructive' :
                          entry.changeType === 'added' ? 'default' : 'secondary'
                        }
                        className="text-xs"
                      >
                        {entry.changeType}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-xs text-muted-foreground font-mono">
                      {entry.oldValue ?? '—'}
                    </TableCell>
                    <TableCell className="text-xs font-mono">
                      {entry.newValue ?? '—'}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}

        {/* Danger zone */}
        <Separator className="my-4" />
        <Button
          variant="destructive"
          size="sm"
          onClick={handleDelete}
          className="w-full"
        >
          <Trash2 className="h-4 w-4 mr-2" />
          Remove contract
        </Button>
      </SheetContent>
    </Sheet>
  )
}
```

- [ ] **Step 3: Verify TypeScript**

```bash
cd frontend && yarn tsc --noEmit
```

Expected: no errors.

---

## Task 3: AttachContractDialog + sidebar wiring

**Files:**
- Create: `frontend/src/components/contract/AttachContractDialog.tsx`
- Modify: `frontend/src/components/layout/CollectionsSidebar.tsx`

- [ ] **Step 1: Create `AttachContractDialog.tsx`**

```tsx
import { useState } from 'react'
import {
  Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { useContractStore } from '@/stores/contract-store'
import { AttachContractInput, ContractScope } from '@/lib/tauri-api'

interface AttachContractDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  collectionRoot: string
  /** Pre-fill scope based on where the user right-clicked. */
  defaultScope: ContractScope
}

const EMPTY_FORM = {
  title: '',
  provider: '',
  consumer: '',
  project: '',
  version: '',
  effectiveDate: new Date().toISOString().split('T')[0],
  expiryDate: '',
}

export function AttachContractDialog({
  open,
  onOpenChange,
  collectionRoot,
  defaultScope,
}: AttachContractDialogProps) {
  const { attachContract } = useContractStore()
  const [form, setForm] = useState(EMPTY_FORM)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const set = (field: keyof typeof EMPTY_FORM) =>
    (e: React.ChangeEvent<HTMLInputElement>) =>
      setForm((f) => ({ ...f, [field]: e.target.value }))

  const handleSubmit = async () => {
    if (!form.title || !form.provider || !form.consumer || !form.project || !form.version || !form.effectiveDate) {
      setError('All fields except expiry date are required.')
      return
    }
    setSaving(true)
    setError(null)
    try {
      const input: AttachContractInput = {
        ...form,
        expiryDate: form.expiryDate || null,
        documentPath: null,
        scope: defaultScope,
        initialSnapshots: [],
      }
      await attachContract(collectionRoot, input)
      setForm(EMPTY_FORM)
      onOpenChange(false)
    } catch (e) {
      setError(String(e))
    } finally {
      setSaving(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Attach contract</DialogTitle>
        </DialogHeader>

        <div className="space-y-3 py-2">
          {error && (
            <p className="text-sm text-destructive">{error}</p>
          )}

          <div className="space-y-1">
            <Label htmlFor="title">Title</Label>
            <Input id="title" placeholder="Payments API v2.3" value={form.title} onChange={set('title')} />
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-1">
              <Label htmlFor="provider">Provider team</Label>
              <Input id="provider" placeholder="Billing Team" value={form.provider} onChange={set('provider')} />
            </div>
            <div className="space-y-1">
              <Label htmlFor="consumer">Consumer team</Label>
              <Input id="consumer" placeholder="Platform Team" value={form.consumer} onChange={set('consumer')} />
            </div>
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-1">
              <Label htmlFor="project">Project</Label>
              <Input id="project" placeholder="Checkout Revamp" value={form.project} onChange={set('project')} />
            </div>
            <div className="space-y-1">
              <Label htmlFor="version">Version</Label>
              <Input id="version" placeholder="v1.0" value={form.version} onChange={set('version')} />
            </div>
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-1">
              <Label htmlFor="effectiveDate">Effective date</Label>
              <Input id="effectiveDate" type="date" value={form.effectiveDate} onChange={set('effectiveDate')} />
            </div>
            <div className="space-y-1">
              <Label htmlFor="expiryDate">Expiry date (optional)</Label>
              <Input id="expiryDate" type="date" value={form.expiryDate} onChange={set('expiryDate')} />
            </div>
          </div>

          <div className="text-xs text-muted-foreground pt-1">
            Scope: <span className="font-medium">{formatScope(defaultScope)}</span>
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>Cancel</Button>
          <Button onClick={handleSubmit} disabled={saving}>
            {saving ? 'Attaching…' : 'Attach contract'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function formatScope(scope: ContractScope): string {
  if (scope.type === 'collection') return 'Entire collection'
  if (scope.type === 'folder') return `Folder: ${scope.rel_path}`
  return `Request: ${scope.rel_path}`
}
```

- [ ] **Step 2: Wire `ContractBadge` into `CollectionsSidebar.tsx`**

In `CollectionsSidebar.tsx`, import the badge and store:

```tsx
import { ContractBadge } from '@/components/contract/ContractBadge'
import { useContractStore } from '@/stores/contract-store'
```

Inside the component, load contracts when collection root changes:

```tsx
const { loadContracts, contractsForScope } = useContractStore()

useEffect(() => {
  if (collectionRoot) loadContracts(collectionRoot)
}, [collectionRoot])
```

On the collection row element, add the badge after the collection name:

```tsx
<ContractBadge
  contracts={contractsForScope('collection')}
  collectionRoot={collectionRoot}
/>
```

On each folder row, add:

```tsx
<ContractBadge
  contracts={contractsForScope('folder', folder.relPath)}
  collectionRoot={collectionRoot}
/>
```

On each request row, add:

```tsx
<ContractBadge
  contracts={contractsForScope('request', request.relPath)}
  collectionRoot={collectionRoot}
/>
```

Note: `collectionRoot`, `folder.relPath`, and `request.relPath` should be replaced with the actual prop/variable names present in your sidebar implementation.

- [ ] **Step 3: Final TypeScript check**

```bash
cd frontend && yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Smoke test manually**

1. Open RocketAPI, open a collection.
2. Right-click a collection in the sidebar → "Attach contract" (you can add this context menu item as a follow-up — for now open `AttachContractDialog` via a temporary button).
3. Fill in the form and submit.
4. Verify a lock icon appears on the collection sidebar item.
5. Click the lock icon — verify the Contract Panel sheet opens with correct parties.
6. Edit a request param and save.
7. Re-open the Contract Panel — verify the changelog shows the field change with timestamp.

- [ ] **Step 5: Commit design asset**

Copy the design snapshot screenshot into the spec assets directory:

```bash
mkdir -p docs/superpowers/specs/assets
cp <path-to-screenshot> docs/superpowers/specs/assets/contract-lock-design-snapshot.png
git add docs/superpowers/specs/assets/contract-lock-design-snapshot.png
git commit -m "docs(contract): add design snapshot asset"
```

- [ ] **Step 6: Commit frontend**

```bash
git add frontend/src/lib/tauri-api.ts
git add frontend/src/stores/contract-store.ts
git add frontend/src/components/contract/
git add frontend/src/components/layout/CollectionsSidebar.tsx
git commit -m "feat(contract): store, ContractBadge, ContractPanel, AttachContractDialog"
```
