# UI Design Pattern Consistency Fix — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Standardise every form/settings panel in the app to match the Authorization tab's structural pattern: `ScrollArea > div.p-6.max-w-3xl.mx-auto > Card > CardContent.p-4` with an explicit save button driven by `useSaveButton`.

**Architecture:** Four targeted markup changes across four files. No new components, no new abstractions, no backend changes. Tasks are independent and can be done in any order.

**Tech Stack:** React 18, TypeScript, Tailwind CSS, shadcn/ui (`Card`, `CardContent`, `ScrollArea`, `Button`), `useSaveButton` hook at `src/hooks/use-save-button.ts`.

---

## File Map

| File | Change type |
|---|---|
| `src/components/collections/CollectionOverviewTab.tsx` | Add `Card`/`CardContent` wrapper around Variables tab content |
| `src/components/request/RequestPanel.tsx` | Replace bare `div` groups with `Card`/`CardContent` + `ScrollArea` wrapper |
| `src/components/workspace/WorkspaceEnvironmentsTab.tsx` | Remove debounce/SavedPill; add `Card` wrapper + explicit save button |
| `src/components/environments/EnvironmentDialog.tsx` | Remove debounce/SavedPill; add `Card` wrapper + explicit save button |

---

## Task 1: CollectionOverviewTab — Wrap Variables tab in Card

**Files:**
- Modify: `src/components/collections/CollectionOverviewTab.tsx:619-651`

The Variables tab section currently wraps `CollectionVariablesEditor` and the save button in a bare `div.space-y-4`. Wrap it with `Card > CardContent.space-y-4.p-4` to match the Auth tab pattern. No logic changes.

- [ ] **Step 1: Open the file and locate the Variables tab section**

The section starts at line 619. It looks like this:

```tsx
{activeSection === 'variables' && (
  <ScrollArea className='h-full'>
    <div className='p-6 max-w-3xl mx-auto space-y-6'>
      <div className='space-y-4'>
        <CollectionVariablesEditor ... />
        <div className='flex justify-end'>
          <Button ...>...</Button>
        </div>
      </div>
    </div>
  </ScrollArea>
)}
```

- [ ] **Step 2: Check that `Card` and `CardContent` are already imported**

Search the imports at the top of the file for `from '@/components/ui/card'`. If the import is missing, add it. (As of the current file, `Card` and `CardContent` are imported at line 8.)

- [ ] **Step 3: Replace the bare `div.space-y-4` with `Card > CardContent`**

Replace lines 622–648 so the Variables tab section reads:

```tsx
{activeSection === 'variables' && (
  <ScrollArea className='h-full'>
    <div className='p-6 max-w-3xl mx-auto space-y-6'>
      <Card>
        <CardContent className='space-y-4 p-4'>
          <CollectionVariablesEditor
            variables={variables}
            onChange={(v) => {
              setVariables(v);
              setIsDirty(true);
            }}
          />

          <div className='flex justify-end'>
            <Button
              size='sm'
              onClick={() => void triggerSave()}
              disabled={!isDirty || saveState !== 'idle'}
              className={cn('gap-1.5', saveState === 'success' && 'text-green-600')}
            >
              {saveState === 'saving' ? (
                <Loader2 className='h-3.5 w-3.5 animate-spin' />
              ) : saveState === 'success' ? (
                <Check className='h-3.5 w-3.5' />
              ) : (
                <Save className='h-3.5 w-3.5' />
              )}
              {saveState === 'success' ? 'Saved' : 'Save'}
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  </ScrollArea>
)}
```

- [ ] **Step 4: Type-check**

```bash
yarn tsc --noEmit
```

Expected: no new errors.

- [ ] **Step 5: Lint**

```bash
yarn check
```

Expected: no new errors or warnings.

- [ ] **Step 6: Commit**

```bash
git add src/components/collections/CollectionOverviewTab.tsx
git commit -m "fix(ui): wrap collection variables tab in Card/CardContent"
```

---

## Task 2: RequestPanel — Wrap Settings section in Card/CardContent + ScrollArea

**Files:**
- Modify: `src/components/request/RequestPanel.tsx:938-1023`

The Settings section currently renders two bare `div.rounded-md.border.bg-muted/20.p-3` groups with no outer `ScrollArea`. Replace each group with `Card > CardContent.p-4` and wrap both cards in `ScrollArea > div.p-6.max-w-3xl.mx-auto.space-y-4`. No logic changes — `handleSettingsChange` is unchanged.

- [ ] **Step 1: Add missing imports**

At the top of `src/components/request/RequestPanel.tsx`, `Card` and `CardContent` are not currently imported. Add them alongside any existing shadcn/ui imports. The existing import block has `Button`, `Checkbox`, `Input`, `Select*`, `Tabs*` from `@/components/ui/*`. Add:

```tsx
import { Card, CardContent } from '@/components/ui/card';
import { ScrollArea } from '@/components/ui/scroll-area';
```

- [ ] **Step 2: Replace the Settings section markup (lines 938–1023)**

Replace the entire `{activeSection === 'settings' && ( ... )}` block with:

```tsx
{activeSection === 'settings' && (
  <ScrollArea className='h-full'>
    <div className='p-6 max-w-3xl mx-auto space-y-4'>
      {/* Security group. */}
      <Card>
        <CardContent className='p-4 space-y-3'>
          <div className='flex items-center gap-2 mb-1'>
            <ShieldCheck className='h-3.5 w-3.5 text-muted-foreground' />
            <span className='text-[11px] font-medium uppercase tracking-wider text-muted-foreground'>
              Security
            </span>
          </div>
          <label
            htmlFor='verify-ssl'
            className='flex items-center gap-2.5 rounded-md px-2 py-1.5 -mx-1 cursor-pointer transition-colors hover:bg-muted/60'
          >
            <Checkbox
              id='verify-ssl'
              checked={settings.verifySsl}
              onCheckedChange={(checked) => handleSettingsChange({ verifySsl: !!checked })}
            />
            <div>
              <span className='text-sm'>Verify SSL certificate</span>
              <p className='text-[11px] text-muted-foreground leading-tight mt-0.5'>
                Validate the server's TLS certificate chain.
              </p>
            </div>
          </label>
        </CardContent>
      </Card>

      {/* Connection group. */}
      <Card>
        <CardContent className='p-4 space-y-3'>
          <div className='flex items-center gap-2 mb-1'>
            <RotateCw className='h-3.5 w-3.5 text-muted-foreground' />
            <span className='text-[11px] font-medium uppercase tracking-wider text-muted-foreground'>
              Connection
            </span>
          </div>
          <label
            htmlFor='follow-redirects'
            className='flex items-center gap-2.5 rounded-md px-2 py-1.5 -mx-1 cursor-pointer transition-colors hover:bg-muted/60'
          >
            <Checkbox
              id='follow-redirects'
              checked={settings.followRedirects}
              onCheckedChange={(checked) =>
                handleSettingsChange({ followRedirects: !!checked })
              }
            />
            <div>
              <span className='text-sm'>Follow redirects</span>
              <p className='text-[11px] text-muted-foreground leading-tight mt-0.5'>
                Automatically follow HTTP 3xx redirects.
              </p>
            </div>
          </label>
          <div className='flex items-center gap-2.5 rounded-md px-2 py-1.5 -mx-1'>
            <Clock className='h-3.5 w-3.5 text-muted-foreground shrink-0' />
            <div className='flex items-center gap-2.5 flex-1'>
              <div className='flex-1'>
                <label htmlFor='timeout-ms' className='text-sm'>
                  Timeout
                </label>
                <p className='text-[11px] text-muted-foreground leading-tight mt-0.5'>
                  Max wait time before aborting the request.
                </p>
              </div>
              <div className='flex items-center gap-1.5'>
                <Input
                  id='timeout-ms'
                  type='number'
                  min={0}
                  className='h-7 w-24 text-xs text-right tabular-nums'
                  value={settings.timeoutMs}
                  onChange={(e) => {
                    const val = Number(e.target.value);
                    if (!Number.isNaN(val) && val >= 0) {
                      handleSettingsChange({ timeoutMs: val });
                    }
                  }}
                />
                <span className='text-[11px] text-muted-foreground'>ms</span>
              </div>
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  </ScrollArea>
)}
```

- [ ] **Step 3: Type-check**

```bash
yarn tsc --noEmit
```

Expected: no new errors.

- [ ] **Step 4: Lint**

```bash
yarn check
```

Expected: no new errors or warnings.

- [ ] **Step 5: Commit**

```bash
git add src/components/request/RequestPanel.tsx
git commit -m "fix(ui): wrap request settings section in Card/CardContent with ScrollArea"
```

---

## Task 3: WorkspaceEnvironmentsTab — Replace auto-save with explicit save button + Card wrapper

**Files:**
- Modify: `src/components/workspace/WorkspaceEnvironmentsTab.tsx`

Remove the debounce + SavedPill auto-save pattern. Add `useSaveButton` + explicit save button in the footer. Wrap the right-panel variable editor in a `Card`.

- [ ] **Step 1: Update imports**

Replace the current import block at the top of `src/components/workspace/WorkspaceEnvironmentsTab.tsx`:

```tsx
import { Check, Eye, EyeOff, Loader2, Plus, Save, Trash2, X } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { toast } from 'sonner';
import { InlineEnvName } from '@/components/environments/InlineEnvName';
import { RocketIdle } from '@/components/illustrations';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import type { Environment, Variable } from '@/lib/tauri-api';
import { deleteGlobalEnvironment, saveGlobalEnvironment } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { useSaveButton } from '@/hooks/use-save-button';
import { useEnvStore } from '@/stores/env-store';
```

Key changes from current:
- Remove `useRef` (no longer needed for debounce)
- Remove `SavedPill` import
- Add `Loader2`, `Save` to lucide imports
- Add `Card`, `CardContent` from `@/components/ui/card`
- Add `useSaveButton` from `@/hooks/use-save-button`

- [ ] **Step 2: Replace state declarations and debounce logic**

Find the current state declarations and `persistEnv` callback (lines 24–71). Replace them with:

```tsx
const [selectedName, setSelectedName] = useState<string | null>(null);
const [editingVars, setEditingVars] = useState<Variable[]>([]);
const [isAddingEnv, setIsAddingEnv] = useState(false);
const [newEnvName, setNewEnvName] = useState('');
const [isDirty, setIsDirty] = useState(false);
```

Then add the save function and hook immediately after:

```tsx
const saveSettings = useCallback(async () => {
  if (!selectedName) return;
  const env = environments.find((e) => e.name === selectedName);
  if (!env) return;
  await updateEnvironment({ ...env, variables: editingVars });
  setIsDirty(false);
}, [selectedName, environments, editingVars, updateEnvironment]);

const { state: saveState, trigger: triggerSave } = useSaveButton(
  saveSettings,
  'Failed to save changes',
);
```

- [ ] **Step 3: Reset dirty + editingVars when selected env changes**

The existing `useEffect` that syncs `editingVars` when `selectedName` or `environments` changes (currently lines ~46–49) should also reset `isDirty`. Update it to:

```tsx
useEffect(() => {
  const env = environments.find((e) => e.name === selectedName);
  setEditingVars(env ? env.variables.slice() : []);
  setIsDirty(false);
}, [selectedName, environments]);
```

Remove the old `useEffect` that reset `savedAt` on env switch (previously lines ~51–55) — it's no longer needed.

- [ ] **Step 4: Update `updateVar`, `addVar`, `removeVar` to set dirty instead of persisting**

Replace the three callbacks. They currently call `persistEnv(...)`. Change them to only update local state and mark dirty:

```tsx
const updateVar = useCallback(
  (idx: number, patch: Partial<Variable>) => {
    if (!selectedName) return;
    const updated = editingVars.slice();
    updated[idx] = { ...updated[idx], ...patch };
    setEditingVars(updated);
    setIsDirty(true);
  },
  [selectedName, editingVars],
);

const addVar = useCallback(() => {
  if (!selectedName) return;
  const newVar: Variable = { key: '', value: '', enabled: true, secret: false };
  setEditingVars((prev) => [...prev, newVar]);
  setIsDirty(true);
}, [selectedName]);

const removeVar = useCallback(
  (idx: number) => {
    if (!selectedName) return;
    setEditingVars((prev) => prev.filter((_, i) => i !== idx));
    setIsDirty(true);
  },
  [selectedName],
);
```

- [ ] **Step 5: Update the JSX — right panel**

In the JSX, the right panel currently has:
1. A column header row with a `SavedPill` slot (lines ~243–255)
2. A `ScrollArea` with variable rows (lines ~256–330)
3. A footer row with "Add Variable" button (lines ~332–342)

Replace the entire right panel content (inside `{selectedName ? ( ... ) : ( empty state )}`) with:

```tsx
<Card className='flex-1 flex flex-col min-w-0 overflow-hidden'>
  <CardContent className='p-0 flex flex-col h-full'>
    {/* Column headers */}
    <div className='flex items-center gap-1.5 px-3 pt-3 pb-1.5 border-b border-border/40 shrink-0'>
      {/* checkbox placeholder */}
      <div className='w-4 shrink-0' />
      <p className='flex-1 text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
        Key
      </p>
      <p className='flex-1 text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
        Value
      </p>
      <div className='w-[52px] shrink-0' />
    </div>
    <ScrollArea className='flex-1'>
      <div className='px-3 pt-2 pb-1 space-y-1'>
        {editingVars.map((variable, idx) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: env variables may share keys; index is the correct identity
          <div
            key={idx}
            className={cn(
              'flex gap-1.5 items-center py-0.5 group',
              !variable.enabled && 'opacity-50',
            )}
          >
            {/* Enabled toggle. */}
            <Button
              variant='ghost'
              size='icon'
              onClick={() => updateVar(idx, { enabled: !variable.enabled })}
              className={cn(
                'w-4 h-4 rounded border p-0 shrink-0',
                variable.enabled
                  ? 'bg-primary border-primary text-primary-foreground hover:bg-primary/90'
                  : 'border-border hover:bg-muted',
              )}
              title={variable.enabled ? 'Disable variable' : 'Enable variable'}
            >
              {variable.enabled && <Check className='h-3 w-3' />}
            </Button>

            {/* Key input. */}
            <Input
              placeholder='Key'
              value={variable.key}
              onChange={(e) => updateVar(idx, { key: e.target.value })}
              className='flex-1 text-xs h-7 font-mono'
            />

            {/* Value input, masked when secret. */}
            <Input
              placeholder='Value'
              type={variable.secret ? 'password' : 'text'}
              value={variable.value}
              onChange={(e) => updateVar(idx, { value: e.target.value })}
              className='flex-1 text-xs h-7 font-mono'
            />

            {/* Secret toggle. */}
            <Button
              variant='ghost'
              size='icon'
              className='h-6 w-6 shrink-0 opacity-0 group-hover:opacity-100 transition-opacity'
              onClick={() => updateVar(idx, { secret: !variable.secret })}
              title={variable.secret ? 'Show value' : 'Hide value'}
            >
              {variable.secret ? (
                <EyeOff className='h-3.5 w-3.5 text-muted-foreground' />
              ) : (
                <Eye className='h-3.5 w-3.5 text-muted-foreground' />
              )}
            </Button>

            {/* Delete row. */}
            <Button
              variant='ghost'
              size='icon'
              className='h-6 w-6 shrink-0 opacity-0 group-hover:opacity-100 transition-opacity'
              onClick={() => removeVar(idx)}
              title='Delete variable'
            >
              <X className='h-3.5 w-3.5 text-muted-foreground hover:text-destructive' />
            </Button>
          </div>
        ))}
      </div>
    </ScrollArea>

    <div className='px-3 py-2 border-t border-border/40 shrink-0 flex items-center justify-between'>
      <Button
        variant='ghost'
        size='sm'
        onClick={addVar}
        className='h-7 text-xs text-muted-foreground hover:text-foreground gap-1.5'
      >
        <Plus className='h-3.5 w-3.5' />
        Add Variable
      </Button>
      <Button
        size='sm'
        onClick={() => void triggerSave()}
        disabled={!isDirty || saveState !== 'idle'}
        className={cn('gap-1.5', saveState === 'success' && 'text-green-600')}
      >
        {saveState === 'saving' ? (
          <Loader2 className='h-3.5 w-3.5 animate-spin' />
        ) : saveState === 'success' ? (
          <Check className='h-3.5 w-3.5' />
        ) : (
          <Save className='h-3.5 w-3.5' />
        )}
        {saveState === 'success' ? 'Saved' : 'Save'}
      </Button>
    </div>
  </CardContent>
</Card>
```

The empty state (no env selected) `div` inside the ternary is unchanged.

The outer right-panel `div` (currently `<div className='flex-1 flex flex-col min-w-0'>`) should be kept as-is — the `Card` sits inside it. The `Card` has `flex-1` so it fills the available space within that wrapper.

- [ ] **Step 6: Type-check**

```bash
yarn tsc --noEmit
```

Expected: no new errors.

- [ ] **Step 7: Lint**

```bash
yarn check
```

Expected: no new errors or warnings.

- [ ] **Step 8: Commit**

```bash
git add src/components/workspace/WorkspaceEnvironmentsTab.tsx
git commit -m "fix(ui): replace auto-save with explicit save button in WorkspaceEnvironmentsTab"
```

---

## Task 4: EnvironmentDialog — Replace auto-save with explicit save button + Card wrapper

**Files:**
- Modify: `src/components/environments/EnvironmentDialog.tsx`

Same pattern as Task 3. Remove debounce + SavedPill, add `useSaveButton` + explicit save button in footer, wrap the right panel variable editor in a `Card`.

- [ ] **Step 1: Update imports**

Replace the current import block:

```tsx
import { Eye, EyeOff, Loader2, Plus, Save, Trash2, X } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { toast } from 'sonner';
import { RocketIdle } from '@/components/illustrations';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { Checkbox } from '@/components/ui/checkbox';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import type { Environment, Variable } from '@/lib/tauri-api';
import { deleteEnvironment as deleteEnvironmentApi, saveEnvironment } from '@/lib/tauri-api';
import { useEnvStore } from '@/stores/env-store';
import { cn } from '@/lib/utils';
import { useSaveButton } from '@/hooks/use-save-button';
import { InlineEnvName } from './InlineEnvName';
```

Key changes from current:
- Remove `useRef`
- Remove `SavedPill` import
- Add `Loader2`, `Save` to lucide imports
- Add `Card`, `CardContent` from `@/components/ui/card`
- Add `useSaveButton` from `@/hooks/use-save-button`

- [ ] **Step 2: Replace state declarations and debounce logic**

Find the current `debounceRef`, `savedAt`, and `saveEnv` callback. Replace with:

```tsx
const [selectedName, setSelectedName] = useState<string | null>(environments[0]?.name ?? null);
const [isAddingEnv, setIsAddingEnv] = useState(false);
const [newEnvName, setNewEnvName] = useState('');
const [isDirty, setIsDirty] = useState(false);
```

Add the save function and hook:

```tsx
const saveSettings = useCallback(async () => {
  if (!selectedEnv || !activeCollection) return;
  await saveEnvironment(activeCollection, selectedEnv);
  setIsDirty(false);
}, [selectedEnv, activeCollection]);

const { state: saveState, trigger: triggerSave } = useSaveButton(
  saveSettings,
  'Failed to save changes',
);
```

- [ ] **Step 3: Remove the `useEffect` that resets `savedAt` on env switch**

Delete the `useEffect` block that was:

```tsx
useEffect(() => {
  setSavedAt(null);
}, [selectedName]);
```

It's no longer needed.

- [ ] **Step 4: Update `updateVariable`, `addVariable`, `removeVariable` to set dirty instead of persisting**

These currently call `saveEnv(updated)`. Remove those calls and add `setIsDirty(true)`. The store state updates (via `useEnvStore.setState`) remain:

```tsx
const updateVariable = useCallback(
  (idx: number, patch: Partial<Variable>) => {
    if (!selectedEnv) return;
    const variables = selectedEnv.variables.slice();
    variables[idx] = { ...variables[idx], ...patch };
    const updated = { ...selectedEnv, variables };
    useEnvStore.setState((s) => ({
      environments: s.environments.map((e) => (e.name === updated.name ? updated : e)),
    }));
    setIsDirty(true);
  },
  [selectedEnv],
);

const addVariable = useCallback(() => {
  if (!selectedEnv) return;
  const variable: Variable = { key: '', value: '', enabled: true, secret: false };
  const updated = { ...selectedEnv, variables: [...selectedEnv.variables, variable] };
  useEnvStore.setState((s) => ({
    environments: s.environments.map((e) => (e.name === updated.name ? updated : e)),
  }));
  setIsDirty(true);
}, [selectedEnv]);

const removeVariable = useCallback(
  (idx: number) => {
    if (!selectedEnv) return;
    const variables = selectedEnv.variables.filter((_, i) => i !== idx);
    const updated = { ...selectedEnv, variables };
    useEnvStore.setState((s) => ({
      environments: s.environments.map((e) => (e.name === updated.name ? updated : e)),
    }));
    setIsDirty(true);
  },
  [selectedEnv],
);
```

- [ ] **Step 5: Reset `isDirty` when selected env changes**

Add a `useEffect` to reset dirty state on env switch:

```tsx
// biome-ignore lint/correctness/useExhaustiveDependencies: selectedName is the intentional trigger
useEffect(() => {
  setIsDirty(false);
}, [selectedName]);
```

- [ ] **Step 6: Update JSX — right panel**

The right panel currently has column headers, a `ScrollArea`, a footer with `SavedPill`. Replace the entire `{selectedEnv ? ( ... ) : ( empty state )}` selected-env content with:

```tsx
<Card className='flex-1 flex flex-col min-w-0 overflow-hidden'>
  <CardContent className='p-0 flex flex-col h-full'>
    {/* Column headers */}
    <div className='flex items-center gap-1.5 px-3 pt-3 pb-1.5 border-b border-border/40 shrink-0'>
      {/* checkbox placeholder */}
      <div className='w-4 shrink-0' />
      <p className='flex-1 text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
        Key
      </p>
      <p className='flex-1 text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
        Value
      </p>
      {/* action buttons placeholder */}
      <div className='w-[52px] shrink-0' />
    </div>
    <ScrollArea className='flex-1'>
      <div className='px-3 pt-2 pb-1 space-y-1'>
        {selectedEnv.variables.map((variable, idx) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: index is stable here — rows are not reordered
          <div
            key={idx}
            className={cn(
              'flex gap-1.5 items-center rounded-sm px-0 py-0.5 group',
              !variable.enabled && 'opacity-50',
            )}
          >
            <Checkbox
              checked={variable.enabled}
              onCheckedChange={(checked) => updateVariable(idx, { enabled: !!checked })}
              aria-label={`${variable.enabled ? 'Disable' : 'Enable'} variable`}
              className='shrink-0'
            />
            <Input
              placeholder='Key'
              value={variable.key}
              onChange={(e) => updateVariable(idx, { key: e.target.value })}
              className='flex-1 h-7 text-xs font-mono'
              aria-label={`Variable key ${idx + 1}`}
            />
            <Input
              placeholder='Value'
              type={variable.secret ? 'password' : 'text'}
              value={variable.value}
              onChange={(e) => updateVariable(idx, { value: e.target.value })}
              className='flex-1 h-7 text-xs font-mono'
              aria-label={`Variable value ${idx + 1}`}
            />
            <Button
              variant='ghost'
              size='icon'
              className='h-6 w-6 shrink-0 opacity-0 group-hover:opacity-100 transition-opacity'
              onClick={() => updateVariable(idx, { secret: !variable.secret })}
              title={variable.secret ? 'Show value' : 'Hide value'}
            >
              {variable.secret ? (
                <EyeOff className='h-3.5 w-3.5 text-muted-foreground' />
              ) : (
                <Eye className='h-3.5 w-3.5 text-muted-foreground' />
              )}
            </Button>
            <Button
              variant='ghost'
              size='icon'
              className='h-6 w-6 shrink-0 opacity-0 group-hover:opacity-100 transition-opacity'
              onClick={() => removeVariable(idx)}
              aria-label={`Delete variable ${idx + 1}`}
            >
              <X className='h-3.5 w-3.5 text-muted-foreground hover:text-destructive' />
            </Button>
          </div>
        ))}
      </div>
    </ScrollArea>
    <div className='px-3 py-2 border-t border-border/40 flex items-center justify-between shrink-0'>
      <Button
        variant='ghost'
        size='sm'
        onClick={addVariable}
        className='h-7 text-xs text-muted-foreground hover:text-foreground gap-1.5'
      >
        <Plus className='h-3.5 w-3.5' />
        Add Variable
      </Button>
      <Button
        size='sm'
        onClick={() => void triggerSave()}
        disabled={!isDirty || saveState !== 'idle'}
        className={cn('gap-1.5', saveState === 'success' && 'text-green-600')}
      >
        {saveState === 'saving' ? (
          <Loader2 className='h-3.5 w-3.5 animate-spin' />
        ) : saveState === 'success' ? (
          <Check className='h-3.5 w-3.5' />
        ) : (
          <Save className='h-3.5 w-3.5' />
        )}
        {saveState === 'success' ? 'Saved' : 'Save'}
      </Button>
    </div>
  </CardContent>
</Card>
```

The empty state `div` inside the ternary is unchanged. The outer right-panel `div` (`<div className='flex-1 flex flex-col min-w-0'>`) is kept as-is — the `Card` with `flex-1` sits inside it.

- [ ] **Step 7: Type-check**

```bash
yarn tsc --noEmit
```

Expected: no new errors.

- [ ] **Step 8: Lint**

```bash
yarn check
```

Expected: no new errors or warnings.

- [ ] **Step 9: Commit**

```bash
git add src/components/environments/EnvironmentDialog.tsx
git commit -m "fix(ui): replace auto-save with explicit save button in EnvironmentDialog"
```
