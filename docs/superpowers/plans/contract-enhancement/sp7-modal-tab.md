# SP7-01 — NewContractModal + Toast + Tests

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **⚠️ Worktree** — all commands run inside `.worktrees/contract-enhancement` on branch `feat/contract-lock-enhancement`.

**Goal:** Build `NewContractModal.tsx` with the complete single-page form, inline validation, toast notifications on success, and full test coverage.

**Architecture:** Single-page form (no wizard). Toast uses the project's existing toast mechanism — find it with the grep in Task 1 before writing any code. Version default is `"0.1.0"`. `recomputeDrift` is called after a successful publish.

**Tech Stack:** React 18, TypeScript, shadcn/ui, Vitest + Testing Library

**Spec:** `Implementation_Plan_v2.md §7.9`

**Depends on:** SP6-02 merged.

---

## Task 1: Discover the project's toast API

**Files:** none created — read-only investigation

- [ ] **Step 1: Find the existing toast mechanism**

```bash
# Find where toasts are called in the existing frontend
grep -r "toast\|sonner\|useToast" frontend/src --include="*.tsx" --include="*.ts" \
  -l | grep -v node_modules | grep -v ".test." | head -10
```

- [ ] **Step 2: Read one call site to get the exact import and usage**

```bash
# Pick the first result and show how toast is called
grep -n "toast" $(grep -rl "toast" frontend/src --include="*.tsx" | head -1) | head -10
```

Note the exact import path and call signature. It will be one of:
- `import { toast } from 'sonner'` → `toast.success('message')`
- `import { useToast } from '@/components/ui/use-toast'` → `const { toast } = useToast()` → `toast({ title: '...' })`
- Some other pattern

Use **exactly** this pattern in `NewContractModal.tsx`. Do not guess.

- [ ] **Step 3: Commit nothing — proceed to Task 2 with the pattern noted**

---

## Task 2: `NewContractModal.tsx`

**Files:**
- Create: `frontend/src/components/contracts/NewContractModal.tsx`

- [ ] **Step 1: Create the modal**

Use the toast import pattern discovered in Task 1. The placeholder `TOAST_IMPORT` and `showToast(...)` below must be replaced with the actual pattern before committing.

```tsx
import { useState } from 'react'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group'
// REPLACE with actual toast import from Task 1 discovery
// e.g. import { toast } from 'sonner'
// e.g. import { useToast } from '@/components/ui/use-toast'
import { useContractsStore } from '@/store/contracts/contractsSlice'
import type { ContractScope, ContractPolicy, Party } from '@/types/contracts'

interface NewContractModalProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  collectionId: string
  collectionName: string
}

interface FormState {
  name: string
  version: string
  providerName: string
  /** Comma-separated for MVP — SSO combobox in future sprint */
  consumerNames: string
  scopeType: 'collection' | 'folder' | 'requests'
  scopePath: string
  effectiveAt: string
  expiresAt: string
  breakingChangePolicy: 'strict' | 'lenient' | 'additive_ok'
  noticeDays: string
  uptimeSla: string
}

type FormErrors = Partial<Record<keyof FormState | '_global', string>>

function todayISO(): string {
  return new Date().toISOString().split('T')[0]
}

function slugify(s: string): string {
  return s
    .toLowerCase()
    .trim()
    .replace(/\s+/g, '-')
    .replace(/[^a-z0-9-]/g, '')
}

function buildParties(csv: string): Party[] {
  return csv
    .split(',')
    .map(n => n.trim())
    .filter(Boolean)
    .map(name => ({ id: slugify(name), name, kind: 'team' as const }))
}

function validate(f: FormState): FormErrors {
  const e: FormErrors = {}
  if (f.name.trim().length < 2)
    e.name = 'At least 2 characters'
  if (!/^\d+\.\d+\.\d+/.test(f.version.trim()))
    e.version = 'Must be semver, e.g. 1.0.0'
  if (!f.providerName.trim())
    e.providerName = 'Required'
  if (!f.consumerNames.trim())
    e.consumerNames = 'At least one consumer required'
  if (!f.effectiveAt)
    e.effectiveAt = 'Required'
  if (f.expiresAt && f.expiresAt <= f.effectiveAt)
    e.expiresAt = 'Must be after effective date'
  if (f.uptimeSla !== '') {
    const n = Number(f.uptimeSla)
    if (isNaN(n) || n < 0 || n > 100)
      e.uptimeSla = 'Must be 0–100'
  }
  return e
}

const INITIAL_STATE: FormState = {
  name: '',
  version: '0.1.0',
  providerName: '',
  consumerNames: '',
  scopeType: 'collection',
  scopePath: '',
  effectiveAt: todayISO(),
  expiresAt: '',
  breakingChangePolicy: 'lenient',
  noticeDays: '30',
  uptimeSla: '',
}

export function NewContractModal({
  open,
  onOpenChange,
  collectionId,
  collectionName,
}: NewContractModalProps) {
  // REPLACE with actual toast hook call if needed (e.g. const { toast } = useToast())
  const createContract = useContractsStore(s => s.createContract)
  const recomputeDrift = useContractsStore(s => s.recomputeDrift)

  const [form, setForm] = useState<FormState>(INITIAL_STATE)
  const [errors, setErrors] = useState<FormErrors>({})
  const [saving, setSaving] = useState(false)

  function setField<K extends keyof FormState>(field: K) {
    return (e: React.ChangeEvent<HTMLInputElement>) =>
      setForm(prev => ({ ...prev, [field]: e.target.value }))
  }

  function resetAndClose() {
    setForm(INITIAL_STATE)
    setErrors({})
    setSaving(false)
    onOpenChange(false)
  }

  async function submit(publishImmediately: boolean) {
    const errs = validate(form)
    if (Object.keys(errs).length > 0) {
      setErrors(errs)
      return
    }
    setSaving(true)
    try {
      const scope: ContractScope =
        form.scopeType === 'collection'
          ? { type: 'collection' }
          : form.scopeType === 'folder'
          ? { type: 'folder', folderId: form.scopePath, path: form.scopePath }
          : { type: 'requests', requestIds: [] }

      const policy: ContractPolicy = {
        breakingChangePolicy: form.breakingChangePolicy,
        noticeDays: Math.max(0, parseInt(form.noticeDays) || 30),
        uptimeSla: form.uptimeSla !== '' ? Number(form.uptimeSla) : null,
      }

      const provider: Party = {
        id: slugify(form.providerName),
        name: form.providerName.trim(),
        kind: 'team',
      }
      const consumers = buildParties(form.consumerNames)

      await createContract(
        collectionId,
        {
          name: form.name.trim(),
          version: form.version.trim(),
          provider,
          consumers,
          scope,
          policy,
          effectiveAt: form.effectiveAt,
          expiresAt: form.expiresAt || null,
          publishImmediately,
        },
        [],
      )

      if (publishImmediately) {
        await recomputeDrift(collectionId)
        // REPLACE with actual toast call, e.g.:
        // toast.success('Contract created and published.')
        // toast({ title: 'Contract created and published.' })
        console.info('[toast] Contract created and published.')
      } else {
        // REPLACE with actual toast call, e.g.:
        // toast('Contract saved as draft.')
        console.info('[toast] Contract saved as draft.')
      }

      resetAndClose()
    } catch (err) {
      setErrors({ _global: String(err) })
    } finally {
      setSaving(false)
    }
  }

  function inputProps(field: keyof FormState) {
    return {
      id: `nc-${field}`,
      value: form[field] as string,
      onChange: setField(field),
      'aria-invalid': !!errors[field],
      'aria-describedby': errors[field] ? `nc-${field}-err` : undefined,
    }
  }

  return (
    <Dialog open={open} onOpenChange={v => { if (!saving) { if (!v) resetAndClose(); else onOpenChange(v) } }}>
      <DialogContent className="max-w-lg max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>New contract</DialogTitle>
        </DialogHeader>

        <div className="space-y-4 py-2">

          {/* ── Name ─────────────────────────── */}
          <div className="space-y-1.5">
            <Label htmlFor="nc-name" className="text-sm">
              Title <span className="text-destructive" aria-hidden="true">*</span>
            </Label>
            <Input {...inputProps('name')} autoFocus placeholder="Payments API v2" />
            {errors.name && (
              <p id="nc-name-err" className="text-xs text-destructive">{errors.name}</p>
            )}
          </div>

          {/* ── Version ──────────────────────── */}
          <div className="space-y-1.5">
            <Label htmlFor="nc-version" className="text-sm">Version</Label>
            <Input {...inputProps('version')} className="font-mono w-36" placeholder="0.1.0" />
            {errors.version && (
              <p id="nc-version-err" className="text-xs text-destructive">{errors.version}</p>
            )}
          </div>

          {/* ── Provider + Consumer(s) ─────── */}
          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-1.5">
              <Label htmlFor="nc-providerName" className="text-sm">
                Provider team <span className="text-destructive" aria-hidden="true">*</span>
              </Label>
              <Input {...inputProps('providerName')} placeholder="Billing Team" />
              {errors.providerName && (
                <p id="nc-providerName-err" className="text-xs text-destructive">
                  {errors.providerName}
                </p>
              )}
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="nc-consumerNames" className="text-sm">
                Consumer team(s) <span className="text-destructive" aria-hidden="true">*</span>
              </Label>
              <Input
                {...inputProps('consumerNames')}
                placeholder="Platform, Mobile"
              />
              <p className="text-[11px] text-muted-foreground leading-none">
                Comma-separated
              </p>
              {errors.consumerNames && (
                <p id="nc-consumerNames-err" className="text-xs text-destructive">
                  {errors.consumerNames}
                </p>
              )}
            </div>
          </div>

          {/* ── Scope ────────────────────────── */}
          <div className="space-y-1.5">
            <Label className="text-sm">Scope</Label>
            <RadioGroup
              value={form.scopeType}
              onValueChange={v =>
                setForm(p => ({ ...p, scopeType: v as FormState['scopeType'] }))
              }
              className="flex gap-4"
            >
              {(['collection', 'folder', 'requests'] as const).map(s => (
                <div key={s} className="flex items-center gap-1.5">
                  <RadioGroupItem value={s} id={`scope-${s}`} />
                  <Label
                    htmlFor={`scope-${s}`}
                    className="text-sm font-normal cursor-pointer capitalize"
                  >
                    {s}
                  </Label>
                </div>
              ))}
            </RadioGroup>
            {form.scopeType !== 'collection' && (
              <Input
                id="nc-scopePath"
                value={form.scopePath}
                onChange={setField('scopePath')}
                className="mt-1.5 font-mono text-sm"
                placeholder={
                  form.scopeType === 'folder' ? 'auth/' : 'No request picker yet'
                }
              />
            )}
          </div>

          {/* ── Dates ────────────────────────── */}
          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-1.5">
              <Label htmlFor="nc-effectiveAt" className="text-sm">
                Effective date <span className="text-destructive" aria-hidden="true">*</span>
              </Label>
              <Input type="date" {...inputProps('effectiveAt')} />
              {errors.effectiveAt && (
                <p id="nc-effectiveAt-err" className="text-xs text-destructive">
                  {errors.effectiveAt}
                </p>
              )}
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="nc-expiresAt" className="text-sm">
                Expiry date{' '}
                <span className="text-[11px] font-normal text-muted-foreground">
                  (optional)
                </span>
              </Label>
              <Input type="date" {...inputProps('expiresAt')} />
              {errors.expiresAt && (
                <p id="nc-expiresAt-err" className="text-xs text-destructive">
                  {errors.expiresAt}
                </p>
              )}
            </div>
          </div>

          {/* ── Breaking change policy ────────── */}
          <div className="space-y-1.5">
            <Label className="text-sm">Breaking change policy</Label>
            <RadioGroup
              value={form.breakingChangePolicy}
              onValueChange={v =>
                setForm(p => ({ ...p, breakingChangePolicy: v as FormState['breakingChangePolicy'] }))
              }
              className="flex gap-4"
            >
              {(
                [
                  ['strict', 'Strict'],
                  ['lenient', 'Lenient'],
                  ['additive_ok', 'Additive OK'],
                ] as const
              ).map(([val, label]) => (
                <div key={val} className="flex items-center gap-1.5">
                  <RadioGroupItem value={val} id={`policy-${val}`} />
                  <Label
                    htmlFor={`policy-${val}`}
                    className="text-sm font-normal cursor-pointer"
                  >
                    {label}
                  </Label>
                </div>
              ))}
            </RadioGroup>
          </div>

          {/* ── Notice period + Uptime SLA ────── */}
          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-1.5">
              <Label htmlFor="nc-noticeDays" className="text-sm">Notice period</Label>
              <div className="flex items-center gap-2">
                <Input
                  id="nc-noticeDays"
                  type="number"
                  min={0}
                  max={365}
                  value={form.noticeDays}
                  onChange={setField('noticeDays')}
                  className="w-20"
                />
                <span className="text-sm text-muted-foreground">days</span>
              </div>
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="nc-uptimeSla" className="text-sm">
                Uptime SLA{' '}
                <span className="text-[11px] font-normal text-muted-foreground">
                  (optional)
                </span>
              </Label>
              <div className="flex items-center gap-2">
                <Input
                  id="nc-uptimeSla"
                  type="number"
                  min={0}
                  max={100}
                  step={0.1}
                  value={form.uptimeSla}
                  onChange={setField('uptimeSla')}
                  className="w-24"
                  placeholder="99.9"
                  aria-invalid={!!errors.uptimeSla}
                />
                <span className="text-sm text-muted-foreground">%</span>
              </div>
              {errors.uptimeSla && (
                <p className="text-xs text-destructive">{errors.uptimeSla}</p>
              )}
            </div>
          </div>

          {/* ── Global error ─────────────────── */}
          {errors._global && (
            <p className="text-xs text-destructive bg-destructive/10 rounded-md px-3 py-2">
              {errors._global}
            </p>
          )}
        </div>

        <DialogFooter className="gap-2 pt-2 flex-wrap">
          <Button
            variant="ghost"
            onClick={resetAndClose}
            disabled={saving}
          >
            Cancel
          </Button>
          <Button
            variant="outline"
            onClick={() => submit(false)}
            disabled={saving}
          >
            {saving ? 'Saving…' : 'Save as Draft'}
          </Button>
          <Button onClick={() => submit(true)} disabled={saving}>
            {saving ? 'Creating…' : 'Create & Publish →'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
```

- [ ] **Step 2: Replace the `console.info` toast stubs**

Using the toast pattern you discovered in Task 1, replace both `console.info` lines with real toast calls. Example for `sonner`:

```tsx
// At top: import { toast } from 'sonner'
// Publish success:
toast.success('Contract created and published.')
// Draft success:
toast('Contract saved as draft.')
```

Example for shadcn `useToast`:

```tsx
// At top: import { useToast } from '@/components/ui/use-toast'
// Inside component: const { toast } = useToast()
// Publish success:
toast({ title: 'Contract created and published.' })
// Draft success:
toast({ title: 'Contract saved as draft.' })
```

- [ ] **Step 3: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | grep "NewContractModal" | head -5
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/contracts/NewContractModal.tsx
git commit -m "feat(contracts): NewContractModal — full form, validation, toast on publish/draft, recomputeDrift"
```

---

## Task 3: `NewContractModal.test.tsx` — 6 test cases

**Files:**
- Create: `frontend/src/components/contracts/NewContractModal.test.tsx`

- [ ] **Step 1: Create the test file**

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { NewContractModal } from './NewContractModal'

const mockCreate = vi.fn().mockResolvedValue({ id: 'new', status: 'active' })
const mockRecompute = vi.fn().mockResolvedValue([])

vi.mock('@/store/contracts/contractsSlice', () => ({
  useContractsStore: (selector: (s: any) => any) =>
    selector({ createContract: mockCreate, recomputeDrift: mockRecompute }),
}))

function renderModal(props = {}) {
  return render(
    <NewContractModal
      open={true}
      onOpenChange={vi.fn()}
      collectionId="col1"
      collectionName="Payments"
      {...props}
    />,
  )
}

beforeEach(() => {
  mockCreate.mockClear()
  mockRecompute.mockClear()
})

describe('NewContractModal', () => {
  it('renders dialog title', () => {
    renderModal()
    expect(screen.getByText('New contract')).toBeInTheDocument()
  })

  it('default version is 0.1.0', () => {
    renderModal()
    expect(screen.getByDisplayValue('0.1.0')).toBeInTheDocument()
  })

  it('shows error when name is too short on submit', async () => {
    renderModal()
    fireEvent.click(screen.getByText('Create & Publish →'))
    await waitFor(() =>
      expect(screen.getByText('At least 2 characters')).toBeInTheDocument(),
    )
  })

  it('shows error when version is not semver', async () => {
    renderModal()
    const versionInput = screen.getByDisplayValue('0.1.0')
    await userEvent.clear(versionInput)
    await userEvent.type(versionInput, 'not-semver')
    fireEvent.click(screen.getByText('Create & Publish →'))
    await waitFor(() =>
      expect(screen.getByText('Must be semver, e.g. 1.0.0')).toBeInTheDocument(),
    )
  })

  it('shows error when consumers field is empty', async () => {
    renderModal()
    await userEvent.type(screen.getByPlaceholderText('Payments API v2'), 'My API')
    await userEvent.type(screen.getByPlaceholderText('Billing Team'), 'Billing')
    fireEvent.click(screen.getByText('Create & Publish →'))
    await waitFor(() =>
      expect(screen.getByText('At least one consumer required')).toBeInTheDocument(),
    )
  })

  it('calls createContract + recomputeDrift on valid publish', async () => {
    renderModal()
    await userEvent.type(screen.getByPlaceholderText('Payments API v2'), 'Payments v2')
    await userEvent.type(screen.getByPlaceholderText('Billing Team'), 'Billing')
    await userEvent.type(screen.getByPlaceholderText('Platform, Mobile'), 'Platform')
    fireEvent.click(screen.getByText('Create & Publish →'))
    await waitFor(() => expect(mockCreate).toHaveBeenCalledTimes(1))
    expect(mockCreate.mock.calls[0][1]).toMatchObject({
      name: 'Payments v2',
      publishImmediately: true,
    })
    expect(mockRecompute).toHaveBeenCalledWith('col1')
  })
})
```

- [ ] **Step 2: Run tests**

```bash
cd frontend && yarn vitest run src/components/contracts/NewContractModal.test.tsx 2>&1 | tail -8
```

Expected: `6 tests passed`.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/contracts/NewContractModal.test.tsx
git commit -m "test(contracts): NewContractModal — 6 tests (validation + publish flow + recomputeDrift)"
```

---

# SP7-02 — ContractsTab (last-sync, error state, hotkeys)

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **⚠️ Worktree** — all commands run inside `.worktrees/contract-enhancement` on branch `feat/contract-lock-enhancement`.

**Goal:** Build `ContractsTab.tsx` with last-sync timestamp in the pane header subtitle, error state UI, telemetry on all 9 events, and `j/k/e/p/n/del` keyboard shortcuts via `useHotkeys`.

**Spec:** `Implementation_Plan_v2.md §7.1, §12, §14`

**Depends on:** SP7-01 merged.

---

## Task 1: Find `useHotkeys` import + telemetry API

**Files:** none — read-only investigation

- [ ] **Step 1: Find how `useHotkeys` is already used in the project**

```bash
grep -rn "useHotkeys\|hotkeys" frontend/src --include="*.tsx" --include="*.ts" \
  | grep -v node_modules | grep -v ".test." | head -10
```

Note the exact import path and call signature.

- [ ] **Step 2: Find the telemetry `track` API**

```bash
grep -rn "track\b" frontend/src --include="*.tsx" --include="*.ts" \
  | grep -v node_modules | grep -v ".test." | grep -v "// " | head -10
```

Note the exact import path and call signature (e.g. `import { track } from '@/lib/analytics'`).

- [ ] **Step 3: Note both for Task 2 — proceed**

---

## Task 2: `ContractsTab.tsx`

**Files:**
- Create: `frontend/src/components/contracts/ContractsTab.tsx`

- [ ] **Step 1: Create the full component**

Replace `HOTKEYS_IMPORT`, `TRACK_IMPORT`, and `track(...)` calls with the actual APIs found in Task 1.

```tsx
import { useState, useEffect, useCallback, useRef } from 'react'
import { Lock, RefreshCw, Download, Plus } from 'lucide-react'
import { formatDistanceToNow } from 'date-fns'
import { Button } from '@/components/ui/button'
import { ScrollArea } from '@/components/ui/scroll-area'
// REPLACE: import { useHotkeys } from '<actual-hotkeys-path>'
// REPLACE: import { track } from '<actual-analytics-path>'
import { useContractsStore } from '@/store/contracts/contractsSlice'
import { useContracts } from '@/hooks/useContracts'
import { useContractDrift } from '@/hooks/useContractDrift'
import { useContractsFilter } from '@/hooks/useContractsFilter'
import { groupContracts } from '@/store/contracts/contractsSelectors'
import { ContractsSummaryRow } from './ContractsSummaryRow'
import { ContractsFilterBar } from './ContractsFilterBar'
import { ContractsGroupHeader } from './ContractsGroupHeader'
import { ContractCard } from './ContractCard'
import { ContractCardSkeleton } from './ContractCardSkeleton'
import { ContractsEmptyState } from './ContractsEmptyState'
import { NewContractModal } from './NewContractModal'
import type { ContractAction } from './ContractCard'
import type { Contract } from '@/types/contracts'

interface ContractsTabProps {
  collectionId: string
  collectionName: string
}

export function ContractsTab({ collectionId, collectionName }: ContractsTabProps) {
  const [modalOpen, setModalOpen] = useState(false)
  const [focusedIdx, setFocusedIdx] = useState(-1)
  const [lastSync, setLastSync] = useState<Date | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)

  const loadContracts = useContractsStore(s => s.loadContracts)
  const recomputeDrift = useContractsStore(s => s.recomputeDrift)
  const store = useContractsStore()

  const { contracts, counts, isLoading } = useContracts(collectionId)
  useContractDrift(collectionId)
  const { filtered, filterState, setSearch, toggleStatus, setSort, setView } =
    useContractsFilter(contracts)

  const { attention, active, inactive } = groupContracts(filtered)
  const allCards: Contract[] = [...attention, ...active, ...inactive]

  // ── Load on mount ─────────────────────────────────────
  useEffect(() => {
    setLoadError(null)
    loadContracts(collectionId)
      .then(() => {
        setLastSync(new Date())
        try {
          // REPLACE with actual track call
          // track('contracts.tab_opened', { collectionId, contractCount: contracts.length })
        } catch {}
      })
      .catch(err => setLoadError(String(err)))
  }, [collectionId])

  // ── Action handler ────────────────────────────────────
  const handleAction = useCallback(
    async (action: ContractAction, contractId: string) => {
      try {
        // REPLACE with actual track call
        // track('contracts.card_action', { contractId, action })
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
          // 'open', 'edit', 'view_changelog', 'export' → handled by routing/navigation
        }
      } catch (err) {
        console.error('[ContractsTab] action error:', err)
      }
    },
    [collectionId, store],
  )

  // ── j/k/e/p/n/del keyboard shortcuts ─────────────────
  // REPLACE useHotkeys calls below with actual import once found in Task 1.
  // Pattern for react-hotkeys-hook:
  //   useHotkeys('j', handler, { scopes: ['contracts'] })
  // Uncomment and fix once the import path is known:
  /*
  useHotkeys('j', () => setFocusedIdx(i => Math.min(i + 1, allCards.length - 1)), { scopes: ['contracts'] })
  useHotkeys('k', () => setFocusedIdx(i => Math.max(0, i - 1)), { scopes: ['contracts'] })
  useHotkeys('n', () => setModalOpen(true), { scopes: ['contracts'] })
  useHotkeys('e', () => { const c = allCards[focusedIdx]; if (c) handleAction('edit', c.id) }, { scopes: ['contracts'] })
  useHotkeys('p', () => {
    const c = allCards[focusedIdx]
    if (!c) return
    handleAction(c.status === 'paused' ? 'resume' : 'pause', c.id)
  }, { scopes: ['contracts'] })
  useHotkeys(['delete', 'backspace'], () => {
    const c = allCards[focusedIdx]
    if (c) handleAction('delete', c.id)
  }, { scopes: ['contracts'] })
  */

  // ── Helpers ───────────────────────────────────────────
  const isEmpty = !isLoading && !loadError && contracts.length === 0
  const noResults = !isLoading && contracts.length > 0 && filtered.length === 0

  function lastSyncLabel(): string {
    if (!lastSync) return ''
    return `Last sync ${formatDistanceToNow(lastSync, { addSuffix: true })}`
  }

  function handleSync() {
    setLoadError(null)
    recomputeDrift(collectionId)
      .then(() => setLastSync(new Date()))
      .catch(err => setLoadError(String(err)))
  }

  // ── Render ────────────────────────────────────────────
  return (
    <div className="flex flex-col h-full bg-background">

      {/* ── Pane header ─────────────────────────────────── */}
      <div className="flex items-end justify-between gap-4 px-6 pt-[18px] pb-[14px] border-b border-border flex-shrink-0">
        <div className="flex items-center gap-3">
          {/* Icon */}
          <div
            className="w-9 h-9 rounded-[calc(var(--radius)-2px)] bg-[hsl(var(--primary)/0.1)] text-primary flex items-center justify-center shrink-0"
            aria-hidden="true"
          >
            <Lock className="h-[18px] w-[18px]" />
          </div>
          {/* Title + subtitle */}
          <div>
            <h1 className="text-xl font-semibold text-foreground leading-tight tracking-[-0.01em]">
              Contracts
            </h1>
            <div className="text-xs text-muted-foreground flex items-center gap-2 mt-0.5 flex-wrap">
              <span>{collectionName}</span>
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
        </div>

        {/* Action buttons */}
        <div className="flex items-center gap-2">
          <Button
            variant="ghost"
            size="sm"
            onClick={handleSync}
            aria-label="Sync contracts"
          >
            <RefreshCw className="h-3.5 w-3.5 mr-1.5" aria-hidden="true" />
            Sync
          </Button>
          <Button
            variant="outline"
            size="sm"
            aria-label="Export contracts"
          >
            <Download className="h-3.5 w-3.5 mr-1.5" aria-hidden="true" />
            Export
          </Button>
          <Button size="sm" onClick={() => setModalOpen(true)}>
            <Plus className="h-3.5 w-3.5 mr-1" aria-hidden="true" />
            New contract
          </Button>
        </div>
      </div>

      {/* ── Error banner ────────────────────────────────── */}
      {loadError && (
        <div
          role="alert"
          className="mx-6 mt-3 px-4 py-3 rounded-md bg-destructive/10 border border-destructive/30 text-sm text-destructive flex items-center justify-between flex-shrink-0"
        >
          <span>Failed to load contracts: {loadError}</span>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 text-xs text-destructive hover:text-destructive"
            onClick={() => { setLoadError(null); loadContracts(collectionId).then(() => setLastSync(new Date())).catch(e => setLoadError(String(e))) }}
          >
            Retry
          </Button>
        </div>
      )}

      {/* ── Summary row ─────────────────────────────────── */}
      {!isEmpty && !loadError && <ContractsSummaryRow counts={counts} />}

      {/* ── Filter bar ──────────────────────────────────── */}
      {!isEmpty && !loadError && (
        <ContractsFilterBar
          filterState={filterState}
          counts={counts}
          onSearch={q => {
            setSearch(q)
            try {
              // REPLACE: track('contracts.filter_used', { filterType: 'search' })
            } catch {}
          }}
          onToggleStatus={s => {
            toggleStatus(s)
            try {
              // REPLACE: track('contracts.filter_used', { filterType: 'status' })
            } catch {}
          }}
          onSetSort={s => {
            setSort(s)
            try {
              // REPLACE: track('contracts.filter_used', { filterType: 'sort' })
            } catch {}
          }}
          onSetView={setView}
        />
      )}

      {/* ── Content area ────────────────────────────────── */}
      {isEmpty && !loadError ? (
        <ContractsEmptyState
          onStartFromCurrent={() => {
            try {
              // REPLACE: track('contracts.empty_state_cta', { action: 'start_from_current' })
            } catch {}
            setModalOpen(true)
          }}
        />
      ) : (
        <ScrollArea className="flex-1">
          <div className="px-6 py-4">
            {isLoading ? (
              <>
                {[1, 2, 3].map(i => (
                  <ContractCardSkeleton key={i} />
                ))}
              </>
            ) : noResults ? (
              <div className="text-center py-12 text-sm text-muted-foreground">
                No contracts match your filters.
              </div>
            ) : (
              <>
                {attention.length > 0 && (
                  <>
                    <ContractsGroupHeader label="Needs attention" count={attention.length} />
                    {attention.map((c, i) => (
                      <ContractCard
                        key={c.id}
                        contract={c}
                        collectionName={collectionName}
                        onAction={handleAction}
                        focused={focusedIdx === i}
                      />
                    ))}
                  </>
                )}
                {active.length > 0 && (
                  <>
                    <ContractsGroupHeader label="Active" count={active.length} />
                    {active.map((c, i) => (
                      <ContractCard
                        key={c.id}
                        contract={c}
                        collectionName={collectionName}
                        onAction={handleAction}
                        focused={focusedIdx === attention.length + i}
                      />
                    ))}
                  </>
                )}
                {inactive.length > 0 && (
                  <>
                    <ContractsGroupHeader label="Inactive" count={inactive.length} />
                    {inactive.map((c, i) => (
                      <ContractCard
                        key={c.id}
                        contract={c}
                        collectionName={collectionName}
                        onAction={handleAction}
                        focused={focusedIdx === attention.length + active.length + i}
                      />
                    ))}
                  </>
                )}
              </>
            )}
          </div>
        </ScrollArea>
      )}

      {/* ── New contract modal ──────────────────────────── */}
      <NewContractModal
        open={modalOpen}
        onOpenChange={setModalOpen}
        collectionId={collectionId}
        collectionName={collectionName}
      />
    </div>
  )
}
```

- [ ] **Step 2: Uncomment hotkeys block**

After identifying the `useHotkeys` import in Task 1, uncomment and fix the hotkeys block:

```tsx
// Example with react-hotkeys-hook:
import { useHotkeys } from 'react-hotkeys-hook'

// Inside component body (add after allCards derivation):
useHotkeys('j', () => setFocusedIdx(i => Math.min(i + 1, allCards.length - 1)), { scopes: ['contracts'] })
useHotkeys('k', () => setFocusedIdx(i => Math.max(0, i - 1)),                   { scopes: ['contracts'] })
useHotkeys('n', () => setModalOpen(true),                                         { scopes: ['contracts'] })
useHotkeys('e', () => { const c = allCards[focusedIdx]; if (c) handleAction('edit', c.id) },  { scopes: ['contracts'] })
useHotkeys('p', () => {
  const c = allCards[focusedIdx]
  if (!c) return
  handleAction(c.status === 'paused' ? 'resume' : 'pause', c.id)
}, { scopes: ['contracts'] })
useHotkeys(['delete', 'backspace'], () => {
  const c = allCards[focusedIdx]
  if (c) handleAction('delete', c.id)
}, { scopes: ['contracts'] })
```

- [ ] **Step 3: Replace all track() stubs**

Replace each `// REPLACE: track(...)` comment with the actual call using the API found in Task 1.

- [ ] **Step 4: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | grep "ContractsTab" | head -5
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/contracts/ContractsTab.tsx
git commit -m "feat(contracts): ContractsTab — last-sync subtitle, error banner+retry, hotkeys, telemetry"
```

---

## Task 3: `ContractsFilterBar.test.tsx`

**Files:**
- Create: `frontend/src/components/contracts/ContractsFilterBar.test.tsx`

Per spec §15 — search filters cards, status chips toggle, sort reorders.

- [ ] **Step 1: Write tests**

```tsx
import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent, act, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ContractsFilterBar } from './ContractsFilterBar'
import type { ContractsFilterState, ContractCounts } from '@/types/contracts'

const defaultFilter: ContractsFilterState = {
  search: '', statuses: ['all'], sort: 'updated', sortDir: 'desc', view: 'cards',
}
const counts: ContractCounts = {
  total: 5, active: 2, drift: 1, breach: 1, inReview: 0,
  draft: 1, paused: 0, expired: 0,
  totalChanges: 4, changesAdded: 1, changesRemoved: 2, changesModified: 1,
}

function setup(overrides: Partial<{
  onSearch: (q: string) => void
  onToggleStatus: (s: any) => void
  onSetSort: (s: any) => void
}> = {}) {
  const onSearch = overrides.onSearch ?? vi.fn()
  const onToggleStatus = overrides.onToggleStatus ?? vi.fn()
  const onSetSort = overrides.onSetSort ?? vi.fn()
  const onSetView = vi.fn()

  render(
    <ContractsFilterBar
      filterState={defaultFilter}
      counts={counts}
      onSearch={onSearch}
      onToggleStatus={onToggleStatus}
      onSetSort={onSetSort}
      onSetView={onSetView}
    />,
  )
  return { onSearch, onToggleStatus, onSetSort, onSetView }
}

describe('ContractsFilterBar', () => {
  it('renders search input', () => {
    setup()
    expect(screen.getByPlaceholderText('Search contracts…')).toBeInTheDocument()
  })

  it('shows "All" chip with total count', () => {
    setup()
    expect(screen.getByText('All')).toBeInTheDocument()
    // Count badge inside the chip
    expect(screen.getByText('5')).toBeInTheDocument()
  })

  it('hides status chip when count is zero', () => {
    setup()
    // 'paused' count is 0, should not appear
    expect(screen.queryByText('Paused')).not.toBeInTheDocument()
  })

  it('calls onToggleStatus when a chip is clicked', async () => {
    const { onToggleStatus } = setup()
    fireEvent.click(screen.getByText('Active'))
    expect(onToggleStatus).toHaveBeenCalledWith('active')
  })

  it('calls onSearch after debounce (200ms)', async () => {
    const onSearch = vi.fn()
    setup({ onSearch })
    const input = screen.getByPlaceholderText('Search contracts…')
    await userEvent.type(input, 'Billing')
    await waitFor(() => expect(onSearch).toHaveBeenCalledWith('Billing'), { timeout: 500 })
  })
})
```

- [ ] **Step 2: Run tests**

```bash
cd frontend && yarn vitest run src/components/contracts/ContractsFilterBar.test.tsx 2>&1 | tail -8
```

Expected: `5 tests passed`.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/contracts/ContractsFilterBar.test.tsx
git commit -m "test(contracts): ContractsFilterBar — 5 tests (search, chips, debounce)"
```
