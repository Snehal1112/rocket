# Save Feedback UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add consistent save feedback across the app — a green "Saved" button state for explicit saves, and an "All changes saved" pill for auto-save sections.

**Architecture:** A shared `useSaveButton` hook encapsulates the `idle → saving → success → idle` state machine for all explicit save buttons. A shared `SavedPill` component handles the auto-save indicator. No toast on success; `toast.error` on failure only.

**Tech Stack:** React 18, TypeScript, Tailwind CSS, Sonner (toasts), Lucide React icons, Vitest

**Spec:** `docs/superpowers/specs/2026-04-08-save-feedback-ux-design.md`

---

## File Map

| Action | Path |
|---|---|
| Create | `src/hooks/use-save-button.ts` |
| Create | `src/hooks/use-save-button.test.ts` |
| Create | `src/components/ui/saved-pill.tsx` |
| Modify | `src/components/collections/CollectionOverviewTab.tsx` |
| Modify | `src/components/collections/CollectionSettingsDialog.tsx` |
| Modify | `src/components/workspace/WorkspaceOverviewTab.tsx` |
| Modify | `src/components/workspace/WorkspaceEnvironmentsTab.tsx` |
| Modify | `src/components/environments/EnvironmentDialog.tsx` |

---

## Task 1: Create `useSaveButton` hook

**Files:**
- Create: `src/hooks/use-save-button.ts`
- Create: `src/hooks/use-save-button.test.ts`

- [ ] **Step 1: Write the failing test**

```ts
// src/hooks/use-save-button.test.ts
import { act, renderHook } from '@testing-library/react';
import { toast } from 'sonner';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useSaveButton } from './use-save-button';

vi.mock('sonner', () => ({ toast: { error: vi.fn() } }));

describe('useSaveButton', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('starts in idle state', () => {
    const { result } = renderHook(() => useSaveButton(async () => {}));
    expect(result.current.state).toBe('idle');
  });

  it('transitions idle → saving → success → idle on success', async () => {
    const fn = vi.fn().mockResolvedValue(undefined);
    const { result } = renderHook(() => useSaveButton(fn));

    await act(async () => {
      void result.current.trigger();
    });

    expect(result.current.state).toBe('success');

    act(() => vi.advanceTimersByTime(2000));
    expect(result.current.state).toBe('idle');
  });

  it('transitions idle → saving → idle on error and calls toast.error', async () => {
    const fn = vi.fn().mockRejectedValue(new Error('boom'));
    const { result } = renderHook(() => useSaveButton(fn, 'Custom error'));

    await act(async () => {
      void result.current.trigger();
    });

    expect(result.current.state).toBe('idle');
    expect(toast.error).toHaveBeenCalledWith('Custom error');
  });

  it('ignores trigger calls when not idle', async () => {
    let resolve!: () => void;
    const fn = vi.fn().mockImplementation(() => new Promise<void>((r) => { resolve = r; }));
    const { result } = renderHook(() => useSaveButton(fn));

    act(() => { void result.current.trigger(); });
    expect(result.current.state).toBe('saving');

    act(() => { void result.current.trigger(); });
    expect(fn).toHaveBeenCalledTimes(1);

    await act(async () => { resolve(); });
  });
});
```

- [ ] **Step 2: Run test to confirm it fails**

```bash
cd /home/numericlabs/data/rocket/rocket
yarn test src/hooks/use-save-button.test.ts
```

Expected: error about missing module `./use-save-button`.

- [ ] **Step 3: Create the hook**

```ts
// src/hooks/use-save-button.ts
import { useCallback, useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';

export type SaveButtonState = 'idle' | 'saving' | 'success';

/**
 * Manages the idle → saving → success → idle state machine for a save button.
 * Shows toast.error on failure; caller renders success state visually (no success toast).
 * Pass any async fn — it does not need to be memoized by the caller.
 */
export function useSaveButton(fn: () => Promise<void>, errorMessage = 'Failed to save') {
  const [state, setState] = useState<SaveButtonState>('idle');
  // Keep a ref to the latest fn so callers don't need useCallback.
  const fnRef = useRef(fn);
  fnRef.current = fn;
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const trigger = useCallback(async () => {
    if (state !== 'idle') return;
    if (timerRef.current) clearTimeout(timerRef.current);
    setState('saving');
    try {
      await fnRef.current();
      setState('success');
      timerRef.current = setTimeout(() => setState('idle'), 2000);
    } catch (err) {
      console.error(err);
      toast.error(errorMessage);
      setState('idle');
    }
  }, [state, errorMessage]);

  // Clear the success timer if the component unmounts.
  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  return { state, trigger };
}
```

- [ ] **Step 4: Run test to confirm it passes**

```bash
yarn test src/hooks/use-save-button.test.ts
```

Expected: all 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/hooks/use-save-button.ts src/hooks/use-save-button.test.ts
git commit -m "feat(ux): add useSaveButton hook with idle/saving/success state machine"
```

---

## Task 2: Create `SavedPill` component

**Files:**
- Create: `src/components/ui/saved-pill.tsx`

- [ ] **Step 1: Create the component**

```tsx
// src/components/ui/saved-pill.tsx
import { Check } from 'lucide-react';
import { useEffect, useState } from 'react';

/**
 * Appears for 3 s then hides itself. Re-mount with a new key to restart the timer.
 * Usage: {savedAt && <SavedPill key={savedAt} />}
 */
export function SavedPill() {
  const [visible, setVisible] = useState(true);

  useEffect(() => {
    const id = setTimeout(() => setVisible(false), 3000);
    return () => clearTimeout(id);
  }, []);

  if (!visible) return null;

  return (
    <span className='flex items-center gap-1 text-xs text-muted-foreground'>
      <Check className='h-3 w-3' />
      All changes saved
    </span>
  );
}
```

- [ ] **Step 2: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/components/ui/saved-pill.tsx
git commit -m "feat(ux): add SavedPill component for auto-save sections"
```

---

## Task 3: Update `CollectionOverviewTab`

**Files:**
- Modify: `src/components/collections/CollectionOverviewTab.tsx`

This file has one `saveSettings` function used by all four tab sections (overview, auth, variables, readme). A single `useSaveButton` instance is shared — only one section is visible at a time so one state machine is correct.

Current state: `saving` boolean + `setSaving`, manual spinner on overview/readme buttons, no spinner on auth/variables buttons, `toast.success` on success.

- [ ] **Step 1: Update imports**

Replace line 1:
```tsx
import { Folder as FolderIcon, Loader2, Save } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { toast } from 'sonner';
```
With:
```tsx
import { Check, Folder as FolderIcon, Loader2, Save } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { toast } from 'sonner';
import { useSaveButton } from '@/hooks/use-save-button';
import { cn } from '@/lib/utils';
```

- [ ] **Step 2: Remove `saving` state and refactor `saveSettings`**

Remove line 220:
```tsx
const [saving, setSaving] = useState(false);
```

Replace the entire `saveSettings` block (lines 257-280) with a plain async function (no `setSaving`) plus the hook call:

```tsx
// Persist all settings to disk (no auto-save).
const saveSettings = useCallback(async () => {
  await saveCollectionSettings(collectionName, {
    auth: authStateToApi(auth),
    headers: headers
      .filter((h) => h.key)
      .map((h) => ({
        key: h.key,
        value: h.value,
        enabled: h.enabled,
      })),
    description: description || undefined,
    readme: readme || undefined,
    variables,
  });
}, [collectionName, auth, headers, description, readme, variables]);

const { state: saveState, trigger: triggerSave } = useSaveButton(
  saveSettings,
  'Failed to save settings',
);
```

- [ ] **Step 3: Update the overview tab's onBlur and Save button**

Change `onBlur={saveSettings}` on the description textarea (in the overview section) to:
```tsx
onBlur={() => void triggerSave()}
```

Replace the overview Save button (currently `disabled={saving}`, uses `saving` for spinner):
```tsx
<Button
  size='sm'
  onClick={() => void triggerSave()}
  disabled={saveState !== 'idle'}
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
```

- [ ] **Step 4: Update the auth tab Save button**

Replace (currently plain Save button with no loading state):
```tsx
<Button size='sm' onClick={saveSettings} className='gap-1.5'>
  <Save className='h-3.5 w-3.5' />
  Save
</Button>
```
With:
```tsx
<Button
  size='sm'
  onClick={() => void triggerSave()}
  disabled={saveState !== 'idle'}
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
```

- [ ] **Step 5: Update the variables tab Save button**

Replace (currently plain Save button with no loading state):
```tsx
<Button size='sm' onClick={saveSettings} className='gap-1.5'>
  <Save className='h-3.5 w-3.5' />
  Save
</Button>
```
With the same button as Step 4.

- [ ] **Step 6: Update the readme tab onBlur and Save button**

Change `onBlur={saveSettings}` on the MarkdownEditor to:
```tsx
onBlur={() => void triggerSave()}
```

Replace the readme Save button (currently uses `saving` state):
```tsx
<Button size='sm' onClick={saveSettings} disabled={saving} className='gap-1.5'>
  {saving ? (
    <Loader2 className='h-3.5 w-3.5 animate-spin' />
  ) : (
    <Save className='h-3.5 w-3.5' />
  )}
  Save
</Button>
```
With the same button as Step 4.

- [ ] **Step 7: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors. If `toast` is now unused (its only call was `toast.success`), remove the import.

- [ ] **Step 8: Commit**

```bash
git add src/components/collections/CollectionOverviewTab.tsx
git commit -m "feat(ux): wire useSaveButton to all CollectionOverviewTab save buttons"
```

---

## Task 4: Update `CollectionSettingsDialog`

**Files:**
- Modify: `src/components/collections/CollectionSettingsDialog.tsx`

The dialog closes after a successful save. To show the success state briefly, close is delayed 1.2 s inside the save function.

- [ ] **Step 1: Update imports**

Replace line 1:
```tsx
import { useState } from 'react';
```
With:
```tsx
import { Check, Loader2, Save } from 'lucide-react';
import { useCallback, useState } from 'react';
import { useSaveButton } from '@/hooks/use-save-button';
```

- [ ] **Step 2: Replace `handleSave` with hook**

Remove the `handleSave` function (lines 32–67) and replace with:

```tsx
const saveFn = useCallback(async () => {
  let apiAuth: Auth | undefined;
  if (auth.authType === 'basic')
    apiAuth = {
      authType: 'basic',
      username: auth.basic?.username ?? '',
      password: auth.basic?.password ?? '',
    };
  else if (auth.authType === 'bearer')
    apiAuth = { authType: 'bearer', token: auth.bearer?.token ?? '' };
  else if (auth.authType === 'api-key')
    apiAuth = {
      authType: 'api-key',
      key: auth.apiKey?.key ?? '',
      value: auth.apiKey?.value ?? '',
      addTo: auth.apiKey?.addTo ?? 'header',
    };
  else apiAuth = undefined;

  await saveCollectionSettings(collectionName, {
    auth: apiAuth,
    headers: headers
      .filter((h) => h.key)
      .map((h) => ({ key: h.key, value: h.value, enabled: h.enabled })),
    variables: [],
  });
  // Let the success state show briefly before closing.
  setTimeout(() => onClose(), 1200);
}, [auth, headers, collectionName, onClose]);

const { state: saveState, trigger: triggerSave } = useSaveButton(saveFn, 'Failed to save settings');
```

- [ ] **Step 3: Replace Save button**

Replace (lines 107–109):
```tsx
<Button size='sm' onClick={handleSave}>
  Save
</Button>
```
With:
```tsx
<Button
  size='sm'
  onClick={() => void triggerSave()}
  disabled={saveState !== 'idle'}
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
```

- [ ] **Step 4: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add src/components/collections/CollectionSettingsDialog.tsx
git commit -m "feat(ux): add save feedback to CollectionSettingsDialog"
```

---

## Task 5: Update `WorkspaceOverviewTab`

**Files:**
- Modify: `src/components/workspace/WorkspaceOverviewTab.tsx`

The doc Save button and its `onBlur` on the textarea both call `handleSaveDoc`. Wire both through the hook.

- [ ] **Step 1: Update imports**

Add to the lucide import on line 2–10 (add `Check, Loader2, Save`):
```tsx
import {
  Box,
  Check,
  ExternalLink,
  FileText,
  FolderOpen,
  Loader2,
  MoreHorizontal,
  Plus,
  Save,
  Trash2,
  Upload,
} from 'lucide-react';
```

Add to the react import (line 11) — add `useCallback` if not present:
```tsx
import { useCallback, useEffect, useState } from 'react';
```

Add after the existing imports:
```tsx
import { useSaveButton } from '@/hooks/use-save-button';
import { cn } from '@/lib/utils';
```

- [ ] **Step 2: Refactor `handleSaveDoc` and wire the hook**

Replace `handleSaveDoc` (lines 107–113):
```tsx
async function handleSaveDoc() {
  try {
    await updateDescription(workspaceId, docContent.trim() || null);
  } catch (err) {
    console.error('[WorkspaceOverview] save doc failed:', err);
  }
}
```
With:
```tsx
const saveDocFn = useCallback(async () => {
  await updateDescription(workspaceId, docContent.trim() || null);
}, [workspaceId, docContent]);

const { state: saveDocState, trigger: triggerSaveDoc } = useSaveButton(
  saveDocFn,
  'Failed to save documentation',
);
```

- [ ] **Step 3: Update the textarea `onBlur` and Save button**

In the edit pane (around line 339), change:
```tsx
onBlur={() => void handleSaveDoc()}
```
To:
```tsx
onBlur={() => void triggerSaveDoc()}
```

Replace the Save button (around line 345–350):
```tsx
<Button
  size='sm'
  className='h-6 text-[10px] px-3'
  onClick={() => void handleSaveDoc()}
>
  Save
</Button>
```
With:
```tsx
<Button
  size='sm'
  className={cn('h-6 text-[10px] px-3 gap-1', saveDocState === 'success' && 'text-green-600')}
  onClick={() => void triggerSaveDoc()}
  disabled={saveDocState !== 'idle'}
>
  {saveDocState === 'saving' ? (
    <Loader2 className='h-3 w-3 animate-spin' />
  ) : saveDocState === 'success' ? (
    <Check className='h-3 w-3' />
  ) : (
    <Save className='h-3 w-3' />
  )}
  {saveDocState === 'success' ? 'Saved' : 'Save'}
</Button>
```

- [ ] **Step 4: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add src/components/workspace/WorkspaceOverviewTab.tsx
git commit -m "feat(ux): add save feedback to WorkspaceOverviewTab doc save"
```

---

## Task 6: Update `WorkspaceEnvironmentsTab`

**Files:**
- Modify: `src/components/workspace/WorkspaceEnvironmentsTab.tsx`

This uses debounced auto-save. Add `savedAt` state + `SavedPill` in the right panel header. Wire `setSavedAt` to the resolved debounce callback and `toast.error` on failure.

- [ ] **Step 1: Update imports**

Add `toast` import:
```tsx
import { toast } from 'sonner';
```

Add `SavedPill` import:
```tsx
import { SavedPill } from '@/components/ui/saved-pill';
```

(`Check` is already imported in this file for the enabled toggle — no change needed.)

- [ ] **Step 2: Add `savedAt` state**

After the existing `useState` declarations (around line 19–22), add:
```tsx
const [savedAt, setSavedAt] = useState<number | null>(null);
```

- [ ] **Step 3: Update `persistEnv` to set `savedAt` on success and `toast.error` on failure**

Replace the `persistEnv` callback (lines 46–56):
```tsx
const persistEnv = useCallback(
  (env: Environment) => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => {
      void updateEnvironment(env).catch((err) => {
        console.error('[WorkspaceEnvironmentsTab] failed to save environment', err);
      });
    }, 400);
  },
  [updateEnvironment],
);
```
With:
```tsx
const persistEnv = useCallback(
  (env: Environment) => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => {
      updateEnvironment(env)
        .then(() => setSavedAt(Date.now()))
        .catch((err) => {
          console.error('[WorkspaceEnvironmentsTab] failed to save environment', err);
          toast.error('Failed to save changes');
        });
    }, 400);
  },
  [updateEnvironment],
);
```

- [ ] **Step 4: Add header row with environment name and `SavedPill` to the right panel**

The right panel starts at line 197. Inside `{selectedName ? ( <> ...` add a header row before the `<ScrollArea>`:

```tsx
{/* Environment name + auto-save indicator. */}
<div className='flex items-center justify-between px-3 py-2 border-b border-border shrink-0'>
  <span className='text-sm font-medium truncate'>{selectedName}</span>
  {savedAt !== null && <SavedPill key={savedAt} />}
</div>
```

- [ ] **Step 5: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add src/components/workspace/WorkspaceEnvironmentsTab.tsx
git commit -m "feat(ux): add auto-save pill to WorkspaceEnvironmentsTab"
```

---

## Task 7: Update `EnvironmentDialog`

**Files:**
- Modify: `src/components/environments/EnvironmentDialog.tsx`

Same pattern as Task 6. The pill goes in the footer row of the right panel, next to "Add Variable".

- [ ] **Step 1: Update imports**

Add `Check` to the lucide import (currently: `Eye, EyeOff, Plus, Trash2, X`):
```tsx
import { Check, Eye, EyeOff, Plus, Trash2, X } from 'lucide-react';
```

Add `toast` import:
```tsx
import { toast } from 'sonner';
```

Add `SavedPill` import:
```tsx
import { SavedPill } from '@/components/ui/saved-pill';
```

Note: `Check` is imported but only used inside `SavedPill` — you may not need it directly in this file. Only import what you actually use.

- [ ] **Step 2: Add `savedAt` state**

After the `debounceRef` declaration (line 29), add:
```tsx
const [savedAt, setSavedAt] = useState<number | null>(null);
```

- [ ] **Step 3: Update `saveEnv` to set `savedAt` on success and `toast.error` on failure**

Replace the `saveEnv` callback (lines 55–67):
```tsx
const saveEnv = useCallback(
  (env: Environment) => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => {
      if (activeCollection) {
        saveEnvironment(activeCollection, env).catch((err) =>
          console.error('[EnvironmentDialog] save failed:', err),
        );
      }
    }, 500);
  },
  [activeCollection],
);
```
With:
```tsx
const saveEnv = useCallback(
  (env: Environment) => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => {
      if (activeCollection) {
        saveEnvironment(activeCollection, env)
          .then(() => setSavedAt(Date.now()))
          .catch((err) => {
            console.error('[EnvironmentDialog] save failed:', err);
            toast.error('Failed to save changes');
          });
      }
    }, 500);
  },
  [activeCollection],
);
```

- [ ] **Step 4: Add `SavedPill` to the right panel footer**

The footer of the right panel (around line 237–242) currently reads:
```tsx
<div className='p-3 pt-0'>
  <Button variant='ghost' size='sm' onClick={addVariable} className='text-sm'>
    <Plus className='h-3.5 w-3.5 mr-1' />
    Add Variable
  </Button>
</div>
```

Replace with:
```tsx
<div className='p-3 pt-0 flex items-center justify-between'>
  <Button variant='ghost' size='sm' onClick={addVariable} className='text-sm'>
    <Plus className='h-3.5 w-3.5 mr-1' />
    Add Variable
  </Button>
  {savedAt !== null && <SavedPill key={savedAt} />}
</div>
```

- [ ] **Step 5: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add src/components/environments/EnvironmentDialog.tsx
git commit -m "feat(ux): add auto-save pill to EnvironmentDialog"
```

---

## Task 8: Final Validation

- [ ] **Step 1: Run all frontend tests**

```bash
yarn test
```

Expected: all tests pass including the new `use-save-button.test.ts`.

- [ ] **Step 2: Full TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Lint check**

```bash
yarn check
```

Expected: no lint or format errors. If there are format issues, run `yarn format` and re-check.

- [ ] **Step 4: Frontend build check**

```bash
yarn build
```

Expected: build succeeds with no errors.

- [ ] **Step 5: Rust check**

```bash
cargo check
```

Expected: no errors (Rust was not modified, but confirm nothing broke).

- [ ] **Step 6: Final commit if any formatting was auto-fixed**

Only commit if Step 3 required `yarn format` to fix issues:
```bash
git add -p
git commit -m "style: auto-format save feedback files"
```
