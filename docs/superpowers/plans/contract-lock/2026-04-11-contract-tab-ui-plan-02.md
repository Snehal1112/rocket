# Contract Tab UI — Plan 02: Form + Live Preview + Root Tab

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the create/edit form, the live preview column, the top bar, and the root `ContractTab` component that owns all view routing and data operations.

**Architecture:** `ContractForm` is a controlled component — it holds no state, receives `values` and calls `onChange`. `ContractLivePreview` synthesises a temporary `Contract` object from the current form values and passes it to `ContractCard` with `preview=true`. `ContractTab` is the single stateful root: it owns `view`, `form`, `saving`, and `error` state, loads contracts on mount, and routes between the three views (list / create+edit / changelog).

**Tech Stack:** React 18, TypeScript, Zustand, shadcn/ui (`Input`, `Label`, `RadioGroup`, `Select`, `Button`, `ScrollArea`, `Separator`), Lucide icons

**Spec:** `docs/superpowers/specs/2026-04-11-contract-tab-ui-design.md`

**Depends on:** Plan 01 complete and merged. The following components must exist:
- `src/components/contract/ContractCard.tsx`
- `src/components/contract/ContractEmptyState.tsx`
- `src/components/contract/ChangelogSummaryBar.tsx`
- `src/components/contract/ChangelogTable.tsx`
- `src/types/pane-types.ts` — `ContractTab` type + `isContractTab` guard
- `src/stores/pane-store.ts` — `openContractTab` action
- `src/stores/contract-store.ts` — `useContractStore` with `loadContracts`, `attachContract`, `removeContract`, `loadChangelog`, `contractStatus`

---

## File Map

| File | Action |
|---|---|
| `src/components/contract/ContractForm.tsx` | Create — controlled form for create / edit |
| `src/components/contract/ContractLivePreview.tsx` | Create — right-column live preview |
| `src/components/contract/ContractTabTopBar.tsx` | Create — top bar with back button + action button |
| `src/components/contract/ContractTab.tsx` | Create — root view router + all data ops |

---

## Task 1: ContractForm

**Files:**
- Create: `src/components/contract/ContractForm.tsx`

- [ ] **Step 1: Create `src/components/contract/ContractForm.tsx`**

A fully controlled form component. It calls `onChange` on every field change so the parent (`ContractTab`) can pass current values to the live preview. `folders` and `requests` are lists of relative paths used to populate the scope selectors.

```tsx
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Button } from '@/components/ui/button'
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Paperclip } from 'lucide-react'

export interface ContractFormValues {
  title: string
  provider: string
  consumer: string
  project: string
  version: string
  effectiveDate: string
  expiryDate: string
  scopeType: 'collection' | 'folder' | 'request'
  scopePath: string
  documentPath: string | null
}

interface ContractFormProps {
  values: ContractFormValues
  onChange: (values: ContractFormValues) => void
  folders: string[]    // available folder rel paths for scope select
  requests: string[]   // available request rel paths for scope select
  error: string | null
}

export function ContractForm({
  values,
  onChange,
  folders,
  requests,
  error,
}: ContractFormProps) {
  const set =
    (field: keyof ContractFormValues) =>
    (e: React.ChangeEvent<HTMLInputElement>) =>
      onChange({ ...values, [field]: e.target.value })

  const setScopeType = (v: ContractFormValues['scopeType']) =>
    onChange({ ...values, scopeType: v, scopePath: '' })

  return (
    <div className="flex flex-col gap-4 overflow-y-auto pr-2">
      {/* Validation error */}
      {error && (
        <p className="text-xs text-destructive bg-destructive/8 rounded-md px-3 py-2">
          {error}
        </p>
      )}

      {/* Title */}
      <div className="space-y-1.5">
        <Label htmlFor="cl-title" className="text-xs">Contract title</Label>
        <Input
          id="cl-title"
          placeholder="Payments API v2.3"
          value={values.title}
          onChange={set('title')}
          className="h-8 text-sm"
        />
      </div>

      {/* Provider + Consumer */}
      <div className="grid grid-cols-2 gap-3">
        <div className="space-y-1.5">
          <Label htmlFor="cl-provider" className="text-xs">Provider team</Label>
          <Input
            id="cl-provider"
            placeholder="Billing Team"
            value={values.provider}
            onChange={set('provider')}
            className="h-8 text-sm"
          />
        </div>
        <div className="space-y-1.5">
          <Label htmlFor="cl-consumer" className="text-xs">Consumer team</Label>
          <Input
            id="cl-consumer"
            placeholder="Platform Team"
            value={values.consumer}
            onChange={set('consumer')}
            className="h-8 text-sm"
          />
        </div>
      </div>

      {/* Project + Version */}
      <div className="grid grid-cols-2 gap-3">
        <div className="space-y-1.5">
          <Label htmlFor="cl-project" className="text-xs">Project</Label>
          <Input
            id="cl-project"
            placeholder="Checkout Revamp"
            value={values.project}
            onChange={set('project')}
            className="h-8 text-sm"
          />
        </div>
        <div className="space-y-1.5">
          <Label htmlFor="cl-version" className="text-xs">Version</Label>
          <Input
            id="cl-version"
            placeholder="v1.0"
            value={values.version}
            onChange={set('version')}
            className="h-8 text-sm"
          />
        </div>
      </div>

      {/* Effective + Expiry */}
      <div className="grid grid-cols-2 gap-3">
        <div className="space-y-1.5">
          <Label htmlFor="cl-effective" className="text-xs">Effective date</Label>
          <Input
            id="cl-effective"
            type="date"
            value={values.effectiveDate}
            onChange={set('effectiveDate')}
            className="h-8 text-sm"
          />
        </div>
        <div className="space-y-1.5">
          <Label htmlFor="cl-expiry" className="text-xs">Expiry (optional)</Label>
          <Input
            id="cl-expiry"
            type="date"
            value={values.expiryDate}
            onChange={set('expiryDate')}
            className="h-8 text-sm"
          />
        </div>
      </div>

      {/* Scope */}
      <div className="space-y-2">
        <Label className="text-xs">Scope</Label>
        <RadioGroup
          value={values.scopeType}
          onValueChange={(v) => setScopeType(v as ContractFormValues['scopeType'])}
          className="space-y-1.5"
        >
          {/* Collection */}
          <div className="flex items-center gap-2">
            <RadioGroupItem value="collection" id="scope-col" />
            <Label htmlFor="scope-col" className="text-xs font-normal cursor-pointer">
              Entire collection
            </Label>
          </div>

          {/* Folder */}
          <div className="flex items-center gap-2 flex-wrap">
            <RadioGroupItem value="folder" id="scope-folder" />
            <Label htmlFor="scope-folder" className="text-xs font-normal cursor-pointer">
              Folder
            </Label>
            {values.scopeType === 'folder' && (
              <Select
                value={values.scopePath}
                onValueChange={(v) => onChange({ ...values, scopePath: v })}
              >
                <SelectTrigger className="h-7 text-xs w-44">
                  <SelectValue placeholder="Select folder…" />
                </SelectTrigger>
                <SelectContent>
                  {folders.length === 0 && (
                    <SelectItem value="__none__" disabled className="text-xs text-muted-foreground">
                      No folders found
                    </SelectItem>
                  )}
                  {folders.map((f) => (
                    <SelectItem key={f} value={f} className="text-xs">{f}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}
          </div>

          {/* Request */}
          <div className="flex items-center gap-2 flex-wrap">
            <RadioGroupItem value="request" id="scope-req" />
            <Label htmlFor="scope-req" className="text-xs font-normal cursor-pointer">
              Single request
            </Label>
            {values.scopeType === 'request' && (
              <Select
                value={values.scopePath}
                onValueChange={(v) => onChange({ ...values, scopePath: v })}
              >
                <SelectTrigger className="h-7 text-xs w-52">
                  <SelectValue placeholder="Select request…" />
                </SelectTrigger>
                <SelectContent>
                  {requests.length === 0 && (
                    <SelectItem value="__none__" disabled className="text-xs text-muted-foreground">
                      No requests found
                    </SelectItem>
                  )}
                  {requests.map((r) => (
                    <SelectItem key={r} value={r} className="text-xs">{r}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}
          </div>
        </RadioGroup>
      </div>

      {/* Document attach */}
      <div className="space-y-1.5">
        <Label className="text-xs">Attach document (optional)</Label>
        <Button
          variant="outline"
          size="sm"
          className="h-8 w-full justify-start text-xs text-muted-foreground font-normal"
        >
          <Paperclip className="h-3.5 w-3.5 mr-2 shrink-0" />
          {values.documentPath ?? 'Browse file…'}
        </Button>
      </div>
    </div>
  )
}
```

- [ ] **Step 2: Verify TypeScript**

```bash
cd frontend && yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/components/contract/ContractForm.tsx
git commit -m "feat(contract-tab): ContractForm controlled component"
```

---

## Task 2: ContractLivePreview + ContractTabTopBar

**Files:**
- Create: `src/components/contract/ContractLivePreview.tsx`
- Create: `src/components/contract/ContractTabTopBar.tsx`

- [ ] **Step 1: Create `src/components/contract/ContractLivePreview.tsx`**

Synthesises a temporary `Contract` from current form values and renders `ContractCard` with `preview=true`. Shows a placeholder when the form is blank.

```tsx
import { Lock } from 'lucide-react'
import { Contract } from '@/lib/tauri-api'
import { ContractCard } from './ContractCard'
import { ContractFormValues } from './ContractForm'

interface ContractLivePreviewProps {
  values: ContractFormValues
  collectionRoot: string
}

export function ContractLivePreview({ values, collectionRoot }: ContractLivePreviewProps) {
  const isEmpty = !values.title && !values.provider && !values.consumer

  const scope =
    values.scopeType === 'folder'
      ? { type: 'folder' as const, rel_path: values.scopePath || 'select a folder' }
      : values.scopeType === 'request'
      ? { type: 'request' as const, rel_path: values.scopePath || 'select a request' }
      : { type: 'collection' as const }

  const previewContract: Contract = {
    id: 'preview',
    title:          values.title          || 'Contract title',
    provider:       values.provider       || 'Provider team',
    consumer:       values.consumer       || 'Consumer team',
    project:        values.project        || 'Project name',
    version:        values.version        || 'v1.0',
    effectiveDate:  values.effectiveDate  || new Date().toISOString().split('T')[0],
    expiryDate:     values.expiryDate     || null,
    documentPath:   values.documentPath,
    enforcementMode: 'informational',
    scope,
  }

  return (
    <div className="flex flex-col h-full">
      <p className="text-xs font-medium uppercase tracking-wide text-muted-foreground mb-3">
        Live preview
      </p>

      {isEmpty ? (
        <div className="flex flex-col items-center justify-center h-44 border border-dashed rounded-lg gap-2 text-muted-foreground">
          <Lock className="h-5 w-5 opacity-30" />
          <p className="text-xs">Fill in the form to preview</p>
        </div>
      ) : (
        <ContractCard
          contract={previewContract}
          collectionRoot={collectionRoot}
          preview
        />
      )}

      {/* Informational hint */}
      <div className="mt-4 border-l-2 border-primary/30 bg-primary/5 rounded-r-md px-3 py-2.5">
        <p className="text-xs text-muted-foreground leading-relaxed">
          Once created, RocketAPI snapshots all covered endpoint signatures.
          Every subsequent save is diffed automatically and logged here.
        </p>
      </div>
    </div>
  )
}
```

- [ ] **Step 2: Create `src/components/contract/ContractTabTopBar.tsx`**

Shows different content depending on which view is active:
- `list` view → lock icon + collection name + "New contract" button
- Other views → back button + view title

```tsx
import { ChevronLeft, Lock, Plus } from 'lucide-react'
import { Button } from '@/components/ui/button'

type ViewKind = 'list' | 'create' | 'edit' | 'changelog'

interface ContractTabTopBarProps {
  collectionName: string
  view: ViewKind
  viewTitle?: string   // used in create / edit / changelog views
  onBack?: () => void
  onNew?: () => void
}

export function ContractTabTopBar({
  collectionName,
  view,
  viewTitle,
  onBack,
  onNew,
}: ContractTabTopBarProps) {
  return (
    <div className="flex items-center justify-between px-5 py-3 border-b border-border shrink-0 bg-background">
      <div className="flex items-center gap-2.5 min-w-0">
        {/* Back button — all non-list views */}
        {view !== 'list' && onBack && (
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-2 -ml-1 text-muted-foreground shrink-0"
            onClick={onBack}
          >
            <ChevronLeft className="h-4 w-4" />
            Back
          </Button>
        )}

        {/* List view title */}
        {view === 'list' && (
          <>
            <div className="w-6 h-6 rounded-md bg-primary/10 flex items-center justify-center shrink-0">
              <Lock className="h-3.5 w-3.5 text-primary" />
            </div>
            <div className="min-w-0">
              <p className="text-sm font-medium text-foreground leading-tight">Contracts</p>
              <p className="text-xs text-muted-foreground leading-tight truncate">
                {collectionName}
              </p>
            </div>
          </>
        )}

        {/* Create / edit / changelog title */}
        {view !== 'list' && viewTitle && (
          <p className="text-sm font-medium text-foreground truncate">{viewTitle}</p>
        )}
      </div>

      {/* New contract button — list view only */}
      {view === 'list' && onNew && (
        <Button size="sm" className="h-7 text-xs shrink-0" onClick={onNew}>
          <Plus className="h-3.5 w-3.5 mr-1" />
          New contract
        </Button>
      )}
    </div>
  )
}
```

- [ ] **Step 3: Verify TypeScript**

```bash
cd frontend && yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/contract/ContractLivePreview.tsx
git add src/components/contract/ContractTabTopBar.tsx
git commit -m "feat(contract-tab): ContractLivePreview and ContractTabTopBar"
```

---

## Task 3: ContractTab root component

**Files:**
- Create: `src/components/contract/ContractTab.tsx`

- [ ] **Step 1: Create `src/components/contract/ContractTab.tsx`**

This is the only stateful component in the contract tab system. It owns: which view is active, the create/edit form values, saving/error state. All IPC calls go through `useContractStore`.

```tsx
import { useEffect, useState } from 'react'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Button } from '@/components/ui/button'
import { Separator } from '@/components/ui/separator'
import { useContractStore } from '@/stores/contract-store'
import { AttachContractInput } from '@/lib/tauri-api'
import type { ContractTab as ContractTabType } from '@/types/pane-types'

import { ContractTabTopBar } from './ContractTabTopBar'
import { ContractCard } from './ContractCard'
import { ContractEmptyState } from './ContractEmptyState'
import { ContractForm, ContractFormValues } from './ContractForm'
import { ContractLivePreview } from './ContractLivePreview'
import { ChangelogSummaryBar } from './ChangelogSummaryBar'
import { ChangelogTable } from './ChangelogTable'

// ── View discriminant ──────────────────────────────────────────
type View =
  | { type: 'list' }
  | { type: 'create' }
  | { type: 'edit'; contractId: string }
  | { type: 'changelog'; contractId: string }

// ── Default form values ────────────────────────────────────────
const EMPTY_FORM: ContractFormValues = {
  title:         '',
  provider:      '',
  consumer:      '',
  project:       '',
  version:       '',
  effectiveDate: new Date().toISOString().split('T')[0],
  expiryDate:    '',
  scopeType:     'collection',
  scopePath:     '',
  documentPath:  null,
}

// ── Props ──────────────────────────────────────────────────────
interface ContractTabProps {
  tab: ContractTabType
}

// ── Component ─────────────────────────────────────────────────
export function ContractTab({ tab }: ContractTabProps) {
  const {
    contracts,
    changelogs,
    loadContracts,
    attachContract,
    removeContract,
    loadChangelog,
  } = useContractStore()

  const [view,   setView]   = useState<View>({ type: 'list' })
  const [form,   setForm]   = useState<ContractFormValues>(EMPTY_FORM)
  const [saving, setSaving] = useState(false)
  const [error,  setError]  = useState<string | null>(null)

  // Load contracts whenever the collection changes
  useEffect(() => {
    loadContracts(tab.collectionRoot)
  }, [tab.collectionRoot])

  // ── Navigation helpers ────────────────────────────────────
  const goList = () => {
    setView({ type: 'list' })
    setForm(EMPTY_FORM)
    setError(null)
  }

  const goCreate = () => {
    setView({ type: 'create' })
    setForm(EMPTY_FORM)
    setError(null)
  }

  const goEdit = (contractId: string) => {
    const c = contracts.find((x) => x.id === contractId)
    if (!c) return
    const scopeType =
      c.scope.type === 'folder'  ? 'folder'  :
      c.scope.type === 'request' ? 'request' : 'collection'
    setForm({
      title:         c.title,
      provider:      c.provider,
      consumer:      c.consumer,
      project:       c.project,
      version:       c.version,
      effectiveDate: c.effectiveDate,
      expiryDate:    c.expiryDate ?? '',
      scopeType,
      scopePath:     (c.scope as any).rel_path ?? '',
      documentPath:  c.documentPath,
    })
    setView({ type: 'edit', contractId })
    setError(null)
  }

  const goChangelog = async (contractId: string) => {
    await loadChangelog(tab.collectionRoot, contractId)
    setView({ type: 'changelog', contractId })
  }

  // ── Submit ────────────────────────────────────────────────
  const handleSubmit = async () => {
    if (
      !form.title ||
      !form.provider ||
      !form.consumer ||
      !form.project ||
      !form.version ||
      !form.effectiveDate
    ) {
      setError('Title, both teams, project, version, and effective date are required.')
      return
    }
    if (
      (form.scopeType === 'folder' || form.scopeType === 'request') &&
      !form.scopePath
    ) {
      setError('Please select a folder or request for the scope.')
      return
    }

    setSaving(true)
    setError(null)

    try {
      const scope =
        form.scopeType === 'folder'
          ? { type: 'folder'  as const, rel_path: form.scopePath }
          : form.scopeType === 'request'
          ? { type: 'request' as const, rel_path: form.scopePath }
          : { type: 'collection' as const }

      const input: AttachContractInput = {
        title:            form.title,
        provider:         form.provider,
        consumer:         form.consumer,
        project:          form.project,
        version:          form.version,
        effectiveDate:    form.effectiveDate,
        expiryDate:       form.expiryDate || null,
        documentPath:     form.documentPath,
        scope,
        initialSnapshots: [],
      }

      await attachContract(tab.collectionRoot, input)
      goList()
    } catch (e) {
      setError(String(e))
    } finally {
      setSaving(false)
    }
  }

  // ── Delete ────────────────────────────────────────────────
  const handleDelete = async (contractId: string) => {
    await removeContract(tab.collectionRoot, contractId)
  }

  // ── Render: List view ─────────────────────────────────────
  if (view.type === 'list') {
    return (
      <div className="flex flex-col h-full bg-background">
        <ContractTabTopBar
          collectionName={tab.collectionName}
          view="list"
          onNew={goCreate}
        />
        <ScrollArea className="flex-1">
          <div className="max-w-2xl mx-auto px-6 py-5">
            {contracts.length === 0 ? (
              <ContractEmptyState onNew={goCreate} />
            ) : (
              <div className="space-y-3">
                {contracts.map((c) => (
                  <ContractCard
                    key={c.id}
                    contract={c}
                    collectionRoot={tab.collectionRoot}
                    onViewChangelog={() => goChangelog(c.id)}
                    onEdit={() => goEdit(c.id)}
                    onDelete={() => handleDelete(c.id)}
                  />
                ))}
              </div>
            )}
          </div>
        </ScrollArea>
      </div>
    )
  }

  // ── Render: Create / Edit view ────────────────────────────
  if (view.type === 'create' || view.type === 'edit') {
    const isEdit    = view.type === 'edit'
    const viewTitle = isEdit ? 'Edit contract' : 'New contract'

    return (
      <div className="flex flex-col h-full bg-background">
        <ContractTabTopBar
          collectionName={tab.collectionName}
          view={isEdit ? 'edit' : 'create'}
          viewTitle={viewTitle}
          onBack={goList}
        />

        <div className="flex flex-1 overflow-hidden">
          {/* Left: form */}
          <div className="w-[380px] shrink-0 border-r border-border px-6 py-5 overflow-y-auto">
            <ContractForm
              values={form}
              onChange={setForm}
              folders={[]}    // TODO: wire real folder list from collection tree store
              requests={[]}   // TODO: wire real request list from collection tree store
              error={error}
            />
          </div>

          {/* Right: live preview */}
          <div className="flex-1 px-6 py-5 bg-muted/30 overflow-y-auto">
            <ContractLivePreview
              values={form}
              collectionRoot={tab.collectionRoot}
            />
          </div>
        </div>

        {/* Action bar */}
        <div className="flex items-center justify-end gap-2 px-6 py-3 border-t border-border bg-background shrink-0">
          <Button variant="outline" size="sm" onClick={goList}>
            Cancel
          </Button>
          <Button size="sm" onClick={handleSubmit} disabled={saving}>
            {saving
              ? (isEdit ? 'Saving…' : 'Creating…')
              : (isEdit ? 'Save changes' : 'Create contract')}
          </Button>
        </div>
      </div>
    )
  }

  // ── Render: Changelog view ────────────────────────────────
  if (view.type === 'changelog') {
    const contract = contracts.find((c) => c.id === view.contractId)
    const changelog = changelogs[view.contractId]

    return (
      <div className="flex flex-col h-full bg-background">
        <ContractTabTopBar
          collectionName={tab.collectionName}
          view="changelog"
          viewTitle={contract ? `${contract.title} — Changelog` : 'Changelog'}
          onBack={goList}
        />
        <ScrollArea className="flex-1">
          <div className="max-w-3xl mx-auto px-6 py-5">
            {/* Parties summary */}
            {contract && (
              <div className="flex items-center gap-2 flex-wrap mb-5">
                <span className="inline-flex items-center gap-1.5 bg-secondary rounded-full px-2.5 py-1 text-xs">
                  <span className="w-2 h-2 rounded-full bg-violet-500 shrink-0" />
                  {contract.provider}
                </span>
                <span className="text-muted-foreground text-xs">→</span>
                <span className="inline-flex items-center gap-1.5 bg-secondary rounded-full px-2.5 py-1 text-xs">
                  <span className="w-2 h-2 rounded-full bg-emerald-500 shrink-0" />
                  {contract.consumer}
                </span>
              </div>
            )}

            {/* Metric cards + table */}
            {changelog ? (
              <>
                <ChangelogSummaryBar changelog={changelog} />
                <ChangelogTable entries={changelog.entries} />
              </>
            ) : (
              <p className="text-sm text-muted-foreground">Loading changelog…</p>
            )}
          </div>
        </ScrollArea>
      </div>
    )
  }

  return null
}
```

- [ ] **Step 2: Verify TypeScript**

```bash
cd frontend && yarn tsc --noEmit
```

Expected: no errors. If you see missing module errors, check that all Plan 01 files are present.

- [ ] **Step 3: Commit**

```bash
git add src/components/contract/ContractTab.tsx
git commit -m "feat(contract-tab): ContractTab root component with list/create/edit/changelog views"
```

---

## Plan 02 complete

Final check before handing off to Plan 03:

```bash
cd frontend && yarn tsc --noEmit
```

Expected: zero errors. Plan 03 (EditorGroup wiring, sidebar context menu, badge update, old file deletion) depends on `ContractTab` being importable and type-correct.
