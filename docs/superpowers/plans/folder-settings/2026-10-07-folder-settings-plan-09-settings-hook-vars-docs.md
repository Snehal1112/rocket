# Folder settings, Plan 09: Settings hook, Vars and Docs sub-tabs

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Folder Settings tab loads and saves a folder's whole `FolderSettings` through one hook, shows one Save button with a dirty indicator in its header (Cmd/Ctrl+S works), and its Vars and Docs sub-tabs are real editors. The old `FolderVariablesPopover` dialog is deleted.

**Architecture:** `useFolderSettings(collection, folderPath)` owns load, edit, dirty and save state for the tab, using refs so a late response or a late save never touches the wrong folder. `FolderSettingsTab` (plan 08 shell) calls the hook once and hands `settings` plus an `onChange(patch)` merge callback to the section components. `VarsSection` wraps the existing `CollectionVariablesEditor`. `DocsSection` wraps the existing `MarkdownEditor`. No new Rust and no new Tauri command: plan 04 already provides `getFolderSettings` and `saveFolderSettings`.

**Tech Stack:** React + TypeScript, Vitest + Testing Library, shadcn/ui, lucide-react, Zustand (`usePaneStore`). Package manager is Yarn.

**Spec:** `docs/superpowers/specs/2026-10-07-folder-settings-design.md` (sections "Frontend", "Runtime rules" Vars row, "Out of scope"). Locked contract: `docs/superpowers/plans/folder-settings/00-plan-index.md`.

**Depends on:** Plan 04 (TS wrappers `getFolderSettings`, `saveFolderSettings` and the `FolderSettings` type in `src/lib/tauri-api.ts`) and Plan 08 (`src/components/collections/FolderSettingsTab.tsx`, the `FolderTab` type in `src/types/pane-types.ts`, and the six placeholder files `HeadersSection.tsx`, `AuthSection.tsx`, `ScriptSection.tsx`, `TestSection.tsx`, `VarsSection.tsx`, `DocsSection.tsx` in `src/components/collections/folder-settings/`).

## Assumptions this plan makes about plans 04 and 08

Verify each with a quick read before Task 1. If one is wrong, adjust the snippet, not the design.

- `FolderSettings` (TS, plan 04) has required `headers` and `variables` arrays and optional `auth`, `preRequestScript`, `postResponseScript`, `testsScript` and `docs` (`string | null | undefined`). The tests build fixtures with `{ headers: [], variables: [] }` plus overrides. If other fields are required, extend the `EMPTY_FOLDER_SETTINGS` constant and the fixtures.
- `FolderSettingsTab` takes `{ tab: FolderTab }`, `FolderTab` extends the existing `BaseTab` (`id`, `title`, `isDirty`) and renders the active section from `tab.activeSection`.
- Each section file exports a named component (`VarsSection`, `DocsSection`, ...) with props `{ collectionName: string; folderPath: string; settings: FolderSettings; onChange: (patch: Partial<FolderSettings>) => void }`.
- `FolderNode.tsx` after plan 08 opens the tab from its "Variables" menu items with `openFolderTab(collectionName, basePath, 'vars')`. Task 3 removes whatever `FolderVariablesPopover` wiring is left over.

## Global Constraints

- shadcn/ui primitives only. No raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>` in app code (test stubs excepted). Icons from `lucide-react` only.
- Zustand: narrow selectors only, never fully destructure store state at the top of a component. Destructuring a hook's return value (`useFolderSettings`) is fine.
- Single-line variable-aware fields use `SingleLineEditor`. `CollectionVariablesEditor` uses plain shadcn `Input` fields and has no variable-aware overlay, so this plan does not pass a `scopedContext` to it (see Task 2 note).
- Never `unwrap()` in Rust. This plan touches no Rust.
- Serde: nothing to do here. The TS type mirrors the IPC DTO, which is already camelCase.
- Tests mock `@/lib/tauri-api` with `vi.mock`, in the same style as `src/hooks/__tests__/useVariableCommit.test.ts` and `src/components/collections/__tests__/CollectionNode.test.tsx`. Use `createDeferred` from `src/test/deferred.ts` for async ordering.
- Comments are short full sentences ending with a period.
- Frontend verification at the end of every task: `yarn tsc --noEmit` and `yarn check`.
- Commits: invoke the `dev-workflow-skills:1-git-commit` skill, stage explicit paths only (never `git add -A` or `git add .`), pathspec commit, conventional commit message.

## Review Focus

1. A slow load response for the previous folder never overwrites the settings of the folder now shown (Task 1 test `ignores a stale load response after the folder changes`).
2. Edits made before the load finishes, or after a failed load, are ignored, so an empty object can never be saved over a real `folder.yml` (Task 1 tests `ignores edits until the settings are loaded` and `keeps isLoaded false and reports an error when the load fails`).
3. `save` writes the whole settings object and clears dirty (Task 1 test `save writes the whole object and clears dirty`), and an edit made while a save is in flight keeps the tab dirty (Task 1 test `an edit made during a save keeps the tab dirty`).
4. A save that finishes after the tab changed folder does not mark the new folder clean (Task 1 test `a save finishing after the folder changed does not clean the new folder`).
5. A save that resolves after the tab unmounted does not throw or log (Task 1 test `a save that resolves after unmount is harmless`).
6. A failed save keeps the tab dirty and shows a toast (Task 1 test `a failed save keeps the tab dirty and shows a toast`).
7. The tab header's one Save button, dirty indicator and Cmd/Ctrl+S act only on their own tab (Task 1 tests in `FolderSettingsTab.save.test.tsx`).
8. Editing a variable keeps its `secret` flag and touches no other settings field (Task 2 tests `keeps the secret flag when another field is edited` and `patches only variables`).
9. The Docs editor edits `settings.docs` only, and an empty editor stores `undefined`, not an empty string (Task 3 tests `typing patches only docs` and `clearing the editor stores undefined`).
10. `FolderVariablesPopover` is gone and nothing imports it (Task 3 grep step).

---

## Task 1: `useFolderSettings` hook, save button and tab wiring

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `src/hooks/useFolderSettings.ts`
- Create: `src/components/collections/folder-settings/FolderSaveButton.tsx`
- Modify: `src/components/collections/FolderSettingsTab.tsx`
- Test: `src/hooks/__tests__/useFolderSettings.test.ts`
- Test: `src/components/collections/folder-settings/__tests__/FolderSaveButton.test.tsx`
- Test: `src/components/collections/__tests__/FolderSettingsTab.save.test.tsx`

**Interfaces:**
- Consumes:
  - `getFolderSettings(collection: string, folderPath: string): Promise<FolderSettings>` and `saveFolderSettings(collection: string, folderPath: string, settings: FolderSettings): Promise<void>` from `@/lib/tauri-api` (plan 04).
  - `SaveButtonState` (`'idle' | 'saving' | 'success'`) from `@/hooks/use-save-button`.
  - `usePaneStore` actions `markDirty(tabId: string)` and `markClean(tabId: string)` (`src/stores/pane-store.ts`).
  - The `rocket:save-draft` window event with `detail: { tabId: string }`. `src/hooks/useKeyboardShortcuts.ts` already dispatches it for Cmd/Ctrl+S on every tab type, so no shortcut change is needed.
- Produces:
  - `useFolderSettings(collection: string, folderPath: string): UseFolderSettingsResult`:
    ```ts
    interface UseFolderSettingsResult {
      settings: FolderSettings;
      setSettings: (next: FolderSettings | ((prev: FolderSettings) => FolderSettings)) => void;
      isDirty: boolean;
      isLoaded: boolean;
      error: string | null;   // additive to the locked contract: set when the load fails
      save: () => Promise<void>;
      saveState: SaveButtonState;
    }
    ```
  - `FolderSaveButton` props: `{ isDirty: boolean; isLoaded: boolean; saveState: SaveButtonState; onSave: () => void }`.

**Design note.** The hook does not use `useSaveButton`. That hook blocks a new save for two seconds after a success, which would silently swallow a Cmd/Ctrl+S pressed right after an edit. The hook keeps the same `SaveButtonState` values so the button looks the same.

- [ ] **Step 1: Re-read the plan 04 and 08 outputs this task builds on**

Run:

```bash
grep -n "FolderSettings\|getFolderSettings\|saveFolderSettings" src/lib/tauri-api.ts
sed -n 1,200p src/components/collections/FolderSettingsTab.tsx
grep -n "FolderTab\|FolderSection" src/types/pane-types.ts
```

Expected: the wrappers and the `FolderSettings` type exist, the shell renders a header and the active section, and `FolderTab` is exported. Reconcile any difference from the "Assumptions" section above before continuing.

- [ ] **Step 2: Write the failing hook tests**

Create `src/hooks/__tests__/useFolderSettings.test.ts`:

```ts
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { FolderSettings } from '@/lib/tauri-api';
import { createDeferred } from '@/test/deferred';
import { useFolderSettings } from '../useFolderSettings';

const { mockGet, mockSave, mockToastError } = vi.hoisted(() => ({
  mockGet: vi.fn(),
  mockSave: vi.fn(),
  mockToastError: vi.fn(),
}));

vi.mock('@/lib/tauri-api', () => ({
  getFolderSettings: mockGet,
  saveFolderSettings: mockSave,
}));

vi.mock('sonner', () => ({ toast: { error: mockToastError } }));

const base: FolderSettings = { headers: [], variables: [], docs: 'base docs' };

function load(collection = 'col', folderPath = 'a/b') {
  return renderHook(({ c, p }) => useFolderSettings(c, p), {
    initialProps: { c: collection, p: folderPath },
  });
}

beforeEach(() => {
  mockGet.mockReset();
  mockSave.mockReset();
  mockToastError.mockReset();
  mockGet.mockResolvedValue(base);
  mockSave.mockResolvedValue(undefined);
});

describe('useFolderSettings', () => {
  it('loads the settings on mount', async () => {
    const { result } = load();
    expect(result.current.isLoaded).toBe(false);
    await waitFor(() => expect(result.current.isLoaded).toBe(true));
    expect(mockGet).toHaveBeenCalledWith('col', 'a/b');
    expect(result.current.settings).toEqual(base);
    expect(result.current.isDirty).toBe(false);
    expect(result.current.error).toBeNull();
  });

  it('ignores a stale load response after the folder changes', async () => {
    const a = createDeferred<FolderSettings>();
    const b = createDeferred<FolderSettings>();
    mockGet.mockImplementation((_c: string, path: string) => (path === 'a' ? a.promise : b.promise));
    const { result, rerender } = load('col', 'a');
    rerender({ c: 'col', p: 'b' });
    await act(async () => {
      b.resolve({ ...base, docs: 'docs of b' });
    });
    await act(async () => {
      a.resolve({ ...base, docs: 'docs of a' });
    });
    expect(result.current.settings.docs).toBe('docs of b');
    expect(result.current.isLoaded).toBe(true);
  });

  it('ignores edits until the settings are loaded', () => {
    const pending = createDeferred<FolderSettings>();
    mockGet.mockReturnValue(pending.promise);
    const { result } = load();
    act(() => result.current.setSettings({ ...base, docs: 'early' }));
    expect(result.current.isDirty).toBe(false);
    expect(result.current.settings.docs).not.toBe('early');
  });

  it('keeps isLoaded false and reports an error when the load fails', async () => {
    const errSpy = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    mockGet.mockRejectedValue(new Error('boom'));
    const { result } = load();
    await waitFor(() => expect(result.current.error).not.toBeNull());
    expect(result.current.isLoaded).toBe(false);
    act(() => result.current.setSettings({ ...base, docs: 'x' }));
    await act(async () => {
      await result.current.save();
    });
    expect(mockSave).not.toHaveBeenCalled();
    errSpy.mockRestore();
  });

  it('save writes the whole object and clears dirty', async () => {
    const { result } = load();
    await waitFor(() => expect(result.current.isLoaded).toBe(true));
    act(() => result.current.setSettings((prev) => ({ ...prev, docs: 'new' })));
    expect(result.current.isDirty).toBe(true);
    await act(async () => {
      await result.current.save();
    });
    expect(mockSave).toHaveBeenCalledTimes(1);
    expect(mockSave).toHaveBeenCalledWith('col', 'a/b', { ...base, docs: 'new' });
    expect(result.current.isDirty).toBe(false);
    expect(result.current.saveState).toBe('success');
  });

  it('does not call the backend when nothing changed', async () => {
    const { result } = load();
    await waitFor(() => expect(result.current.isLoaded).toBe(true));
    await act(async () => {
      await result.current.save();
    });
    expect(mockSave).not.toHaveBeenCalled();
  });

  it('an edit made during a save keeps the tab dirty', async () => {
    const inflight = createDeferred<void>();
    mockSave.mockReturnValue(inflight.promise);
    const { result } = load();
    await waitFor(() => expect(result.current.isLoaded).toBe(true));
    act(() => result.current.setSettings((prev) => ({ ...prev, docs: 'one' })));
    let pending!: Promise<void>;
    act(() => {
      pending = result.current.save();
    });
    expect(result.current.saveState).toBe('saving');
    act(() => result.current.setSettings((prev) => ({ ...prev, docs: 'two' })));
    await act(async () => {
      inflight.resolve();
      await pending;
    });
    expect(mockSave).toHaveBeenCalledWith('col', 'a/b', { ...base, docs: 'one' });
    expect(result.current.isDirty).toBe(true);
    expect(result.current.settings.docs).toBe('two');
  });

  it('a save finishing after the folder changed does not clean the new folder', async () => {
    const inflight = createDeferred<void>();
    mockSave.mockReturnValue(inflight.promise);
    const { result, rerender } = load('col', 'a');
    await waitFor(() => expect(result.current.isLoaded).toBe(true));
    act(() => result.current.setSettings((prev) => ({ ...prev, docs: 'edit in a' })));
    let pending!: Promise<void>;
    act(() => {
      pending = result.current.save();
    });
    rerender({ c: 'col', p: 'b' });
    await waitFor(() => expect(result.current.isLoaded).toBe(true));
    act(() => result.current.setSettings((prev) => ({ ...prev, docs: 'edit in b' })));
    await act(async () => {
      inflight.resolve();
      await pending;
    });
    expect(result.current.isDirty).toBe(true);
    expect(result.current.saveState).toBe('idle');
  });

  it('a save that resolves after unmount is harmless', async () => {
    const errSpy = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const inflight = createDeferred<void>();
    mockSave.mockReturnValue(inflight.promise);
    const { result, unmount } = load();
    await waitFor(() => expect(result.current.isLoaded).toBe(true));
    act(() => result.current.setSettings((prev) => ({ ...prev, docs: 'late' })));
    let pending!: Promise<void>;
    act(() => {
      pending = result.current.save();
    });
    unmount();
    await act(async () => {
      inflight.resolve();
      await pending;
    });
    expect(mockSave).toHaveBeenCalledWith('col', 'a/b', { ...base, docs: 'late' });
    expect(errSpy).not.toHaveBeenCalled();
    errSpy.mockRestore();
  });

  it('a failed save keeps the tab dirty and shows a toast', async () => {
    const errSpy = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    mockSave.mockRejectedValue(new Error('disk full'));
    const { result } = load();
    await waitFor(() => expect(result.current.isLoaded).toBe(true));
    act(() => result.current.setSettings((prev) => ({ ...prev, docs: 'x' })));
    await act(async () => {
      await result.current.save();
    });
    expect(result.current.isDirty).toBe(true);
    expect(result.current.saveState).toBe('idle');
    expect(mockToastError).toHaveBeenCalledWith('Failed to save folder settings');
    errSpy.mockRestore();
  });
});
```

- [ ] **Step 3: Run the hook tests and confirm they fail**

Run: `yarn vitest run src/hooks/__tests__/useFolderSettings.test.ts`

Expected: FAIL with `Failed to resolve import "../useFolderSettings"`.

- [ ] **Step 4: Implement the hook**

Create `src/hooks/useFolderSettings.ts`:

```ts
import { useCallback, useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import type { SaveButtonState } from '@/hooks/use-save-button';
import { type FolderSettings, getFolderSettings, saveFolderSettings } from '@/lib/tauri-api';

const EMPTY_FOLDER_SETTINGS: FolderSettings = { headers: [], variables: [] };

export interface UseFolderSettingsResult {
  settings: FolderSettings;
  setSettings: (next: FolderSettings | ((prev: FolderSettings) => FolderSettings)) => void;
  isDirty: boolean;
  isLoaded: boolean;
  /** Set when loading the folder settings failed. */
  error: string | null;
  /** Writes the whole settings object. Does nothing when there is nothing to save. */
  save: () => Promise<void>;
  saveState: SaveButtonState;
}

/**
 * Loads one folder's settings and tracks edits, dirty state and saving.
 * Refs hold the latest values so a late response or a late save never
 * touches the folder the tab shows now.
 */
export function useFolderSettings(collection: string, folderPath: string): UseFolderSettingsResult {
  const [settings, setSettingsState] = useState<FolderSettings>(EMPTY_FOLDER_SETTINGS);
  const [isDirty, setIsDirty] = useState(false);
  const [isLoaded, setIsLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saveState, setSaveState] = useState<SaveButtonState>('idle');

  const settingsRef = useRef<FolderSettings>(EMPTY_FOLDER_SETTINGS);
  const loadedRef = useRef(false);
  const dirtyRef = useRef(false);
  const savingRef = useRef(false);
  const mountedRef = useRef(true);
  const editVersionRef = useRef(0);
  const successTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // The key names the folder this hook currently shows.
  const key = JSON.stringify([collection, folderPath]);
  const keyRef = useRef(key);
  keyRef.current = key;
  const collectionRef = useRef(collection);
  collectionRef.current = collection;
  const folderPathRef = useRef(folderPath);
  folderPathRef.current = folderPath;

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      if (successTimerRef.current) clearTimeout(successTimerRef.current);
    };
  }, []);

  // Load on mount and whenever the folder changes. The flag drops stale responses.
  useEffect(() => {
    let active = true;
    settingsRef.current = EMPTY_FOLDER_SETTINGS;
    loadedRef.current = false;
    dirtyRef.current = false;
    editVersionRef.current += 1;
    if (successTimerRef.current) clearTimeout(successTimerRef.current);
    setSettingsState(EMPTY_FOLDER_SETTINGS);
    setIsDirty(false);
    setIsLoaded(false);
    setError(null);
    setSaveState('idle');

    getFolderSettings(collection, folderPath)
      .then((loaded) => {
        if (!active) return;
        settingsRef.current = loaded;
        loadedRef.current = true;
        setSettingsState(loaded);
        setIsLoaded(true);
      })
      .catch((err) => {
        if (!active) return;
        console.error('[useFolderSettings] load failed', err);
        setError('Failed to load folder settings.');
      });

    return () => {
      active = false;
    };
  }, [collection, folderPath]);

  const setSettings = useCallback(
    (next: FolderSettings | ((prev: FolderSettings) => FolderSettings)) => {
      // Never edit before a successful load, so an empty object cannot be saved over the file.
      if (!loadedRef.current) return;
      const value = typeof next === 'function' ? next(settingsRef.current) : next;
      settingsRef.current = value;
      editVersionRef.current += 1;
      dirtyRef.current = true;
      setSettingsState(value);
      setIsDirty(true);
    },
    [],
  );

  const save = useCallback(async () => {
    if (savingRef.current || !loadedRef.current || !dirtyRef.current) return;
    const savedKey = keyRef.current;
    const savedCollection = collectionRef.current;
    const savedPath = folderPathRef.current;
    const snapshot = settingsRef.current;
    const savedVersion = editVersionRef.current;

    savingRef.current = true;
    if (successTimerRef.current) clearTimeout(successTimerRef.current);
    setSaveState('saving');
    try {
      await saveFolderSettings(savedCollection, savedPath, snapshot);
      if (!mountedRef.current || keyRef.current !== savedKey) return;
      if (editVersionRef.current === savedVersion) {
        dirtyRef.current = false;
        setIsDirty(false);
      }
      setSaveState('success');
      successTimerRef.current = setTimeout(() => {
        if (mountedRef.current) setSaveState('idle');
      }, 2000);
    } catch (err) {
      console.error('[useFolderSettings] save failed', err);
      toast.error('Failed to save folder settings');
      if (mountedRef.current && keyRef.current === savedKey) setSaveState('idle');
    } finally {
      savingRef.current = false;
      // A save that ended for another folder must not leave this one stuck in saving.
      if (mountedRef.current && keyRef.current !== savedKey) setSaveState('idle');
    }
  }, []);

  return { settings, setSettings, isDirty, isLoaded, error, save, saveState };
}
```

- [ ] **Step 5: Run the hook tests and confirm they pass**

Run: `yarn vitest run src/hooks/__tests__/useFolderSettings.test.ts`

Expected: PASS, 10 tests. If `a save finishing after the folder changed does not clean the new folder` leaves `saveState` at `saving`, the `finally` guard above is the fix, check it was copied.

- [ ] **Step 6: Write the failing `FolderSaveButton` tests**

Create `src/components/collections/folder-settings/__tests__/FolderSaveButton.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { FolderSaveButton } from '../FolderSaveButton';

describe('FolderSaveButton', () => {
  it('is disabled and shows no indicator when nothing changed', () => {
    render(<FolderSaveButton isDirty={false} isLoaded saveState='idle' onSave={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    expect(screen.queryByText('Unsaved changes')).not.toBeInTheDocument();
  });

  it('is disabled until the settings are loaded', () => {
    render(<FolderSaveButton isDirty isLoaded={false} saveState='idle' onSave={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
  });

  it('shows the indicator and calls onSave when dirty', async () => {
    const onSave = vi.fn();
    render(<FolderSaveButton isDirty isLoaded saveState='idle' onSave={onSave} />);
    expect(screen.getByText('Unsaved changes')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(onSave).toHaveBeenCalledTimes(1);
  });

  it('shows Saved after a successful save', () => {
    render(<FolderSaveButton isDirty={false} isLoaded saveState='success' onSave={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Saved' })).toBeInTheDocument();
  });

  it('is disabled while saving', () => {
    render(<FolderSaveButton isDirty isLoaded saveState='saving' onSave={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
  });
});
```

- [ ] **Step 7: Run and confirm failure**

Run: `yarn vitest run src/components/collections/folder-settings/__tests__/FolderSaveButton.test.tsx`

Expected: FAIL with `Failed to resolve import "../FolderSaveButton"`.

- [ ] **Step 8: Implement `FolderSaveButton`**

Create `src/components/collections/folder-settings/FolderSaveButton.tsx`:

```tsx
import { Check, Loader2, Save } from 'lucide-react';
import { Button } from '@/components/ui/button';
import type { SaveButtonState } from '@/hooks/use-save-button';
import { cn } from '@/lib/utils';

interface FolderSaveButtonProps {
  isDirty: boolean;
  isLoaded: boolean;
  saveState: SaveButtonState;
  onSave: () => void;
}

/** The one Save button and dirty indicator for the folder settings tab header. */
export function FolderSaveButton({ isDirty, isLoaded, saveState, onSave }: FolderSaveButtonProps) {
  return (
    <div className='flex items-center gap-2'>
      {isDirty && (
        <span className='flex items-center gap-1.5 text-xs text-muted-foreground'>
          <span aria-hidden='true' className='h-1.5 w-1.5 rounded-full bg-amber-500' />
          Unsaved changes
        </span>
      )}
      <Button
        size='sm'
        onClick={onSave}
        disabled={!isLoaded || !isDirty || saveState !== 'idle'}
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
  );
}
```

Note: while `saveState === 'success'` and the user edits again, the button stays disabled for the rest of the two-second success window. Cmd/Ctrl+S is not affected, because it calls the hook's `save` directly and the hook does not block during success.

- [ ] **Step 9: Run and confirm the button tests pass**

Run: `yarn vitest run src/components/collections/folder-settings/__tests__/FolderSaveButton.test.tsx`

Expected: PASS, 5 tests.

- [ ] **Step 10: Write the failing shell wiring tests**

Create `src/components/collections/__tests__/FolderSettingsTab.save.test.tsx`. It stubs `DocsSection` so it does not depend on the Docs editor built in Task 3:

```tsx
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { FolderSettingsTab } from '@/components/collections/FolderSettingsTab';
import type * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { FolderTab } from '@/types/pane-types';

const { mockGet, mockSave, mockMarkDirty, mockMarkClean } = vi.hoisted(() => ({
  mockGet: vi.fn(),
  mockSave: vi.fn(),
  mockMarkDirty: vi.fn(),
  mockMarkClean: vi.fn(),
}));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, getFolderSettings: mockGet, saveFolderSettings: mockSave };
});

// A stub section that edits docs, so the shell wiring is tested on its own.
vi.mock('../folder-settings/DocsSection', () => ({
  DocsSection: ({ onChange }: { onChange: (patch: { docs: string }) => void }) => (
    <button type='button' onClick={() => onChange({ docs: 'edited' })}>
      edit-docs
    </button>
  ),
}));

const base: tauriApi.FolderSettings = { headers: [], variables: [], docs: 'base' };

const tab: FolderTab = {
  id: 'folder-tab-1',
  title: 'b',
  isDirty: false,
  tabType: 'folder',
  collectionName: 'col',
  folderPath: 'a/b',
  activeSection: 'docs',
};

function pressSave(tabId: string) {
  window.dispatchEvent(new CustomEvent('rocket:save-draft', { detail: { tabId } }));
}

beforeEach(() => {
  mockGet.mockReset().mockResolvedValue(base);
  mockSave.mockReset().mockResolvedValue(undefined);
  mockMarkDirty.mockReset();
  mockMarkClean.mockReset();
  usePaneStore.setState({ markDirty: mockMarkDirty, markClean: mockMarkClean });
});

describe('FolderSettingsTab save wiring', () => {
  it('has a disabled Save button until something is edited', async () => {
    render(<FolderSettingsTab tab={tab} />);
    await screen.findByText('edit-docs');
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    expect(screen.queryByText('Unsaved changes')).not.toBeInTheDocument();
  });

  it('shows the dirty indicator and saves the whole object from the header button', async () => {
    render(<FolderSettingsTab tab={tab} />);
    await userEvent.click(await screen.findByText('edit-docs'));
    expect(screen.getByText('Unsaved changes')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() =>
      expect(mockSave).toHaveBeenCalledWith('col', 'a/b', { ...base, docs: 'edited' }),
    );
    await waitFor(() => expect(screen.queryByText('Unsaved changes')).not.toBeInTheDocument());
  });

  it('saves on Cmd/Ctrl+S for its own tab only', async () => {
    render(<FolderSettingsTab tab={tab} />);
    await userEvent.click(await screen.findByText('edit-docs'));
    act(() => pressSave('some-other-tab'));
    expect(mockSave).not.toHaveBeenCalled();
    act(() => pressSave(tab.id));
    await waitFor(() => expect(mockSave).toHaveBeenCalledTimes(1));
  });

  it('keeps the pane store dirty marker in sync with the hook', async () => {
    render(<FolderSettingsTab tab={tab} />);
    await userEvent.click(await screen.findByText('edit-docs'));
    await waitFor(() => expect(mockMarkDirty).toHaveBeenCalledWith(tab.id));
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(mockMarkClean).toHaveBeenLastCalledWith(tab.id));
  });

  it('shows an error instead of the sections when the load fails', async () => {
    const errSpy = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    mockGet.mockRejectedValue(new Error('missing'));
    render(<FolderSettingsTab tab={tab} />);
    expect(await screen.findByText('Failed to load folder settings.')).toBeInTheDocument();
    expect(screen.queryByText('edit-docs')).not.toBeInTheDocument();
    errSpy.mockRestore();
  });
});
```

If plan 08's shell needs providers (QueryClient, tooltip), wrap the render in the same provider the plan 08 shell test uses.

- [ ] **Step 11: Run and confirm failure**

Run: `yarn vitest run src/components/collections/__tests__/FolderSettingsTab.save.test.tsx`

Expected: FAIL. The shell does not call the hook yet, so `edit-docs` is not rendered with an `onChange` that edits, and there is no Save button.

- [ ] **Step 12: Wire the hook into `FolderSettingsTab`**

Edit `src/components/collections/FolderSettingsTab.tsx` (from plan 08). Add these imports, merging with the existing ones and keeping Biome's import order:

```tsx
import { useCallback, useEffect } from 'react';
import { FolderSaveButton } from '@/components/collections/folder-settings/FolderSaveButton';
import { useFolderSettings } from '@/hooks/useFolderSettings';
import type { FolderSettings } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
```

Inside the component body, before any early return, add:

```tsx
  const { settings, setSettings, isDirty, isLoaded, error, save, saveState } = useFolderSettings(
    tab.collectionName,
    tab.folderPath,
  );
  const markDirty = usePaneStore((s) => s.markDirty);
  const markClean = usePaneStore((s) => s.markClean);

  // Sections send partial patches. Merge them into the whole settings object.
  const handleChange = useCallback(
    (patch: Partial<FolderSettings>) => setSettings((prev) => ({ ...prev, ...patch })),
    [setSettings],
  );

  // Show the unsaved marker on the tab strip.
  useEffect(() => {
    if (isDirty) markDirty(tab.id);
    else markClean(tab.id);
  }, [isDirty, tab.id, markDirty, markClean]);

  // Cmd/Ctrl+S is dispatched by useKeyboardShortcuts for the active tab.
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<{ tabId: string }>).detail;
      if (detail?.tabId !== tab.id) return;
      void save();
    };
    window.addEventListener('rocket:save-draft', handler);
    return () => window.removeEventListener('rocket:save-draft', handler);
  }, [tab.id, save]);
```

In the shell's header row (the element that holds the folder name and the section tab bar from plan 08), add the button at the right end:

```tsx
<FolderSaveButton isDirty={isDirty} isLoaded={isLoaded} saveState={saveState} onSave={() => void save()} />
```

Replace the section render so the sections only mount once loaded, and every section gets the same four props. Where plan 08 renders the active section placeholder, use:

```tsx
{error ? (
  <div className='flex h-full items-center justify-center text-sm text-destructive'>{error}</div>
) : !isLoaded ? (
  <div className='flex h-full items-center justify-center text-sm text-muted-foreground'>
    Loading...
  </div>
) : (
  /* The existing section switch from plan 08 goes here. Pass these props to every section. */
  <ActiveSection
    collectionName={tab.collectionName}
    folderPath={tab.folderPath}
    settings={settings}
    onChange={handleChange}
  />
)}
```

`ActiveSection` stands for plan 08's existing switch over `tab.activeSection` (for example a `switch` returning `<VarsSection ... />`). Keep that switch and add the four props to each case. Do not change which section file is rendered for which value.

- [ ] **Step 13: Run the shell tests and confirm they pass**

Run: `yarn vitest run src/components/collections/__tests__/FolderSettingsTab.save.test.tsx src/hooks/__tests__/useFolderSettings.test.ts src/components/collections/folder-settings/__tests__/FolderSaveButton.test.tsx`

Expected: PASS. Also run any plan 08 shell test: `yarn vitest run src/components/collections/__tests__/FolderSettingsTab` and fix mocks if it now needs `getFolderSettings` mocked.

- [ ] **Step 14: Type check and lint**

Run: `yarn tsc --noEmit` then `yarn check`

Expected: both pass with no errors.

- [ ] **Step 15: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only:

```bash
git add src/hooks/useFolderSettings.ts \
  src/hooks/__tests__/useFolderSettings.test.ts \
  src/components/collections/folder-settings/FolderSaveButton.tsx \
  src/components/collections/folder-settings/__tests__/FolderSaveButton.test.tsx \
  src/components/collections/FolderSettingsTab.tsx \
  src/components/collections/__tests__/FolderSettingsTab.save.test.tsx
```

Pathspec commit with those same paths, message: `feat(ui): add useFolderSettings hook and folder tab save button`.

---

## Task 2: Vars sub-tab

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src/components/collections/folder-settings/VarsSection.tsx` (plan 08 placeholder)
- Test: `src/components/collections/folder-settings/__tests__/VarsSection.test.tsx`

**Interfaces:**
- Consumes:
  - `CollectionVariablesEditor` props `{ variables: CollectionVariable[]; onChange: (variables: CollectionVariable[]) => void; showDescription?: boolean }` (`src/components/collections/CollectionVariablesEditor.tsx`).
  - `CollectionVariable { key; value; initialValue; enabled; secret }` and `FolderSettings` from `@/lib/tauri-api`.
  - shadcn `Card`, `CardHeader`, `CardTitle`, `CardContent`, `ScrollArea`.
- Produces: `VarsSection` with props `{ collectionName: string; folderPath: string; settings: FolderSettings; onChange: (patch: Partial<FolderSettings>) => void }`. Every change calls `onChange({ variables })` and nothing else.

**Decisions.**
- Bruno parity: the section is titled "Pre Request" with a one-paragraph description. There is no Post Response block, because the spec makes folder vars pre-request only. The description points users to scripts for values set after a response.
- Secret flag: `CollectionVariablesEditor` spreads its patch (`{ ...v, ...patch }`), so a row's `secret`, `initialValue` and `enabled` survive edits to other fields. The tests lock that in.
- Variable-aware inputs: `CollectionVariablesEditor` renders plain shadcn `Input` fields and takes no variable context, so no `scopedContext` is passed. Making these fields variable-aware means swapping them for `SingleLineEditor` and threading `buildScopedContext({ collectionVars, folderVars })` through, which would also change the collection Variables tab. That is out of scope here. Collection vars would be cheap (`getCollection(...).settings.variables`), parent-folder vars would need `getFolderChainVariables`, which is keyed by a request path, not a folder path. Both are left for a follow-up.

- [ ] **Step 1: Read the placeholder and the editor**

Run:

```bash
cat src/components/collections/folder-settings/VarsSection.tsx
sed -n 1,40p src/components/collections/CollectionVariablesEditor.tsx
```

Expected: the placeholder exports `VarsSection`, and the editor props match the Interfaces above.

- [ ] **Step 2: Write the failing tests**

Create `src/components/collections/folder-settings/__tests__/VarsSection.test.tsx`:

```tsx
import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { CollectionVariable, FolderSettings } from '@/lib/tauri-api';
import { VarsSection } from '../VarsSection';

const token: CollectionVariable = {
  key: 'token',
  value: 'abc',
  initialValue: 'init',
  enabled: true,
  secret: true,
};
const host: CollectionVariable = {
  key: 'host',
  value: 'localhost',
  initialValue: '',
  enabled: false,
  secret: false,
};

function renderSection(variables: CollectionVariable[]) {
  const onChange = vi.fn();
  const settings: FolderSettings = { headers: [], variables, docs: 'keep me' };
  render(
    <VarsSection collectionName='col' folderPath='a/b' settings={settings} onChange={onChange} />,
  );
  return onChange;
}

describe('VarsSection', () => {
  it('shows the Pre Request heading with its description', () => {
    renderSection([]);
    expect(screen.getByText('Pre Request')).toBeInTheDocument();
    expect(screen.getByText(/resolved before each request/i)).toBeInTheDocument();
    expect(screen.queryByText(/Post Response/i)).not.toBeInTheDocument();
  });

  it('shows the empty state and adds a variable', async () => {
    const onChange = renderSection([]);
    await userEvent.click(screen.getByRole('button', { name: /Add Variable/ }));
    expect(onChange).toHaveBeenCalledWith({
      variables: [{ key: '', value: '', initialValue: '', enabled: true, secret: false }],
    });
  });

  it('renders one row per variable', () => {
    renderSection([token, host]);
    expect(screen.getByLabelText('Variable name, row 1')).toHaveValue('token');
    expect(screen.getByLabelText('Variable name, row 2')).toHaveValue('host');
  });

  it('keeps the secret flag when another field is edited', () => {
    const onChange = renderSection([token, host]);
    fireEvent.change(screen.getByLabelText('Variable name, row 1'), {
      target: { value: 'token2' },
    });
    expect(onChange).toHaveBeenCalledWith({
      variables: [{ ...token, key: 'token2' }, host],
    });
  });

  it('toggles the secret flag without touching the other fields', async () => {
    const onChange = renderSection([host]);
    await userEvent.click(screen.getByTitle('Hide value (mark as secret)'));
    expect(onChange).toHaveBeenCalledWith({ variables: [{ ...host, secret: true }] });
  });

  it('patches only variables', () => {
    const onChange = renderSection([token]);
    fireEvent.change(screen.getByLabelText('Current value, row 1'), { target: { value: 'xyz' } });
    const patch = onChange.mock.calls[0][0] as Record<string, unknown>;
    expect(Object.keys(patch)).toEqual(['variables']);
  });

  it('removes a variable', async () => {
    const onChange = renderSection([token, host]);
    await userEvent.click(screen.getByRole('button', { name: 'Delete variable 1' }));
    expect(onChange).toHaveBeenCalledWith({ variables: [host] });
  });
});
```

- [ ] **Step 3: Run and confirm failure**

Run: `yarn vitest run src/components/collections/folder-settings/__tests__/VarsSection.test.tsx`

Expected: FAIL. The plan 08 placeholder renders no "Pre Request" heading and no editor, so the first assertion fails with `Unable to find an element with the text: Pre Request`.

- [ ] **Step 4: Implement `VarsSection`**

Replace the contents of `src/components/collections/folder-settings/VarsSection.tsx`:

```tsx
import { CollectionVariablesEditor } from '@/components/collections/CollectionVariablesEditor';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { ScrollArea } from '@/components/ui/scroll-area';
import type { CollectionVariable, FolderSettings } from '@/lib/tauri-api';

interface VarsSectionProps {
  collectionName: string;
  folderPath: string;
  settings: FolderSettings;
  onChange: (patch: Partial<FolderSettings>) => void;
}

/** Pre-request variables for every request in this folder and its sub-folders. */
export function VarsSection({ settings, onChange }: VarsSectionProps) {
  const handleChange = (variables: CollectionVariable[]) => onChange({ variables });

  return (
    <ScrollArea className='h-full'>
      <div className='p-6 max-w-3xl mx-auto'>
        <Card>
          <CardHeader className='pb-3 pt-4 px-4 border-b border-border/40'>
            <CardTitle className='text-sm font-medium'>Pre Request</CardTitle>
            <p className='text-xs text-muted-foreground'>
              These variables are resolved before each request in this folder and its sub-folders
              runs. A request's own variables win over them, and they win over environment and
              collection variables. To set a variable after a response, use a script.
            </p>
          </CardHeader>
          <CardContent className='p-4'>
            <CollectionVariablesEditor
              variables={settings.variables}
              onChange={handleChange}
              showDescription={false}
            />
          </CardContent>
        </Card>
      </div>
    </ScrollArea>
  );
}
```

- [ ] **Step 5: Run and confirm pass**

Run: `yarn vitest run src/components/collections/folder-settings/__tests__/VarsSection.test.tsx`

Expected: PASS, 7 tests.

- [ ] **Step 6: Type check and lint**

Run: `yarn tsc --noEmit` then `yarn check`

Expected: both pass. If Biome flags the unused `collectionName` and `folderPath` props, they are intentionally omitted from the destructuring, the interface keeps them so every section has the same props.

- [ ] **Step 7: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only:

```bash
git add src/components/collections/folder-settings/VarsSection.tsx \
  src/components/collections/folder-settings/__tests__/VarsSection.test.tsx
```

Pathspec commit with those paths, message: `feat(ui): add folder settings Vars sub-tab`.

---

## Task 3: Docs sub-tab and removal of `FolderVariablesPopover`

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src/components/collections/folder-settings/DocsSection.tsx` (plan 08 placeholder)
- Modify: `src/components/collections/FolderNode.tsx`
- Delete: `src/components/collections/FolderVariablesPopover.tsx`
- Test: `src/components/collections/folder-settings/__tests__/DocsSection.test.tsx`

**Interfaces:**
- Consumes:
  - `MarkdownEditor` props `{ value: string; onChange: (value: string) => void; mode?: 'edit' | 'preview'; onModeChange?: (mode: 'edit' | 'preview') => void; onSave?; saveState?; isDirty?; onBlur? }` (`src/components/collections/MarkdownEditor.tsx`). Without `onSave` it renders no Save button, which is what we want: the tab header owns saving.
  - `FolderSettings.docs` (`string | null | undefined`).
- Produces: `DocsSection` with props `{ collectionName: string; folderPath: string; settings: FolderSettings; onChange: (patch: Partial<FolderSettings>) => void }`. Every change calls `onChange({ docs })` with the text, or `undefined` when the editor is empty, so an empty section is omitted from `folder.yml`.

**Decisions.**
- Edit and preview toggle works like the collection Documentation tab: preview is the default, and the mode resets to preview when the folder changes.
- Saving: the tab header Save button and Cmd/Ctrl+S save. The collection tab's save-on-blur is not used, because a folder save writes headers, scripts and auth too, and an accidental blur should not write them.
- The scroll shadow stays with the shell. The collection tab draws it for its Variables and Auth ScrollAreas, and the editor card here scrolls internally.

- [ ] **Step 1: Write the failing `DocsSection` tests**

Create `src/components/collections/folder-settings/__tests__/DocsSection.test.tsx`:

```tsx
import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { FolderSettings } from '@/lib/tauri-api';
import { DocsSection } from '../DocsSection';

function renderSection(docs: string | undefined, folderPath = 'a/b') {
  const onChange = vi.fn();
  const settings: FolderSettings = { headers: [], variables: [], docs };
  const utils = render(
    <DocsSection
      collectionName='col'
      folderPath={folderPath}
      settings={settings}
      onChange={onChange}
    />,
  );
  return { onChange, settings, ...utils };
}

describe('DocsSection', () => {
  it('starts in preview mode and renders the markdown', () => {
    renderSection('# Folder notes');
    expect(screen.getByRole('heading', { name: 'Folder notes' })).toBeInTheDocument();
  });

  it('shows the empty state when there are no docs', () => {
    renderSection(undefined);
    expect(screen.getByText('No documentation yet')).toBeInTheDocument();
  });

  it('switches to edit mode with the Edit tab', async () => {
    renderSection('hello');
    await userEvent.click(screen.getByRole('tab', { name: 'Edit' }));
    expect(screen.getByPlaceholderText(/Add documentation/)).toHaveValue('hello');
  });

  it('shows no Save button of its own', async () => {
    renderSection('hello');
    await userEvent.click(screen.getByRole('tab', { name: 'Edit' }));
    expect(screen.queryByRole('button', { name: /save/i })).not.toBeInTheDocument();
  });

  it('typing patches only docs', async () => {
    const { onChange } = renderSection('hello');
    await userEvent.click(screen.getByRole('tab', { name: 'Edit' }));
    fireEvent.change(screen.getByPlaceholderText(/Add documentation/), {
      target: { value: 'hello world' },
    });
    expect(onChange).toHaveBeenCalledWith({ docs: 'hello world' });
  });

  it('clearing the editor stores undefined', async () => {
    const { onChange } = renderSection('hello');
    await userEvent.click(screen.getByRole('tab', { name: 'Edit' }));
    fireEvent.change(screen.getByPlaceholderText(/Add documentation/), { target: { value: '' } });
    expect(onChange).toHaveBeenCalledWith({ docs: undefined });
  });

  it('returns to preview mode when the folder changes', async () => {
    const { rerender, settings, onChange } = renderSection('hello', 'a');
    await userEvent.click(screen.getByRole('tab', { name: 'Edit' }));
    expect(screen.getByPlaceholderText(/Add documentation/)).toBeInTheDocument();
    rerender(
      <DocsSection collectionName='col' folderPath='b' settings={settings} onChange={onChange} />,
    );
    expect(screen.queryByPlaceholderText(/Add documentation/)).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run and confirm failure**

Run: `yarn vitest run src/components/collections/folder-settings/__tests__/DocsSection.test.tsx`

Expected: FAIL. The plan 08 placeholder has no Markdown editor, so `starts in preview mode and renders the markdown` fails with `Unable to find an accessible element with the role "heading"`.

- [ ] **Step 3: Implement `DocsSection`**

Replace the contents of `src/components/collections/folder-settings/DocsSection.tsx`:

```tsx
import { useEffect, useState } from 'react';
import { MarkdownEditor } from '@/components/collections/MarkdownEditor';
import type { FolderSettings } from '@/lib/tauri-api';

interface DocsSectionProps {
  collectionName: string;
  folderPath: string;
  settings: FolderSettings;
  onChange: (patch: Partial<FolderSettings>) => void;
}

/** Markdown documentation for the folder, stored as the top-level `docs` of folder.yml. */
export function DocsSection({ collectionName, folderPath, settings, onChange }: DocsSectionProps) {
  const [mode, setMode] = useState<'edit' | 'preview'>('preview');

  // Each folder starts in preview mode, like the collection Documentation tab.
  // biome-ignore lint/correctness/useExhaustiveDependencies: the dependencies are the reset triggers.
  useEffect(() => {
    setMode('preview');
  }, [collectionName, folderPath]);

  return (
    <div className='flex h-full min-h-0 flex-col overflow-hidden p-6'>
      <MarkdownEditor
        value={settings.docs ?? ''}
        onChange={(value) => onChange({ docs: value === '' ? undefined : value })}
        mode={mode}
        onModeChange={setMode}
      />
    </div>
  );
}
```

- [ ] **Step 4: Run and confirm pass**

Run: `yarn vitest run src/components/collections/folder-settings/__tests__/DocsSection.test.tsx`

Expected: PASS, 7 tests.

- [ ] **Step 5: Find every user of the popover**

Run:

```bash
grep -rn "FolderVariablesPopover" src docs/superpowers/specs docs/superpowers/plans/folder-settings .claude 2>/dev/null
grep -n "varsOpen\|setVarsOpen\|FolderVariablesPopover" src/components/collections/FolderNode.tsx
```

Expected: only `FolderVariablesPopover.tsx` itself and `FolderNode.tsx` reference the component (the spec and plan files may mention it in prose, which is fine). If any other `.ts` or `.tsx` file under `src` imports it, stop and remove that use as well.

- [ ] **Step 6: Remove the popover wiring from `FolderNode.tsx`**

Edit `src/components/collections/FolderNode.tsx`:

1. Delete the import `import { FolderVariablesPopover } from './FolderVariablesPopover';`.
2. Delete the state line `const [varsOpen, setVarsOpen] = useState(false);`.
3. Delete the whole block that renders it, including its comment:

```tsx
      {/* Variables dialog for this folder. */}
      <FolderVariablesPopover
        open={varsOpen}
        onClose={() => setVarsOpen(false)}
        collection={collectionName}
        folderPath={basePath}
        folderName={name}
      />
```

4. The two menu items labeled "Variables" (one `DropdownMenuItem`, one `ContextMenuItem`) must open the tab. If plan 08 already changed them, leave them. If any still call `setVarsOpen(true)`, change the handler to open the Vars sub-tab, using the store action plan 08 added:

```tsx
onClick={() => openFolderTab(collectionName, basePath, 'vars')}
```

`openFolderTab` is read with a narrow selector, `const openFolderTab = usePaneStore((s) => s.openFolderTab);`, only if the component does not already have it from plan 08.

Then confirm nothing is left:

```bash
grep -n "varsOpen\|setVarsOpen\|FolderVariablesPopover" src/components/collections/FolderNode.tsx
```

Expected: no output.

- [ ] **Step 7: Delete the popover file**

```bash
git rm src/components/collections/FolderVariablesPopover.tsx
```

The file had no test of its own (no `FolderVariablesPopover` test exists under `src`). `getFolderVariables` and `saveFolderVariables` in `src/lib/tauri-api.ts` stay: they are still public wrappers over live Tauri commands and are not part of this deletion.

- [ ] **Step 8: Prove nothing still imports it**

Run: `grep -rn "FolderVariablesPopover" src`

Expected: no output.

- [ ] **Step 9: Run the related tests**

Run: `yarn vitest run src/components/collections src/hooks`

Expected: PASS. A failing `FolderNode`-related test that mocked the popover means that test must drop the mock.

- [ ] **Step 10: Type check and lint**

Run: `yarn tsc --noEmit` then `yarn check`

Expected: both pass. Biome will flag a now-unused `Variable` icon or `useState` import in `FolderNode.tsx` if plan 08 left it unused, remove only what it flags.

- [ ] **Step 11: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only:

```bash
git add src/components/collections/folder-settings/DocsSection.tsx \
  src/components/collections/folder-settings/__tests__/DocsSection.test.tsx \
  src/components/collections/FolderNode.tsx
```

The deletion of `FolderVariablesPopover.tsx` is already staged by the `git rm` in Step 7.

Pathspec commit with these paths, including the deleted file: `src/components/collections/FolderVariablesPopover.tsx`, message: `feat(ui): add folder settings Docs sub-tab and drop the variables dialog`.

---

## Next Plan

**Execution order:** this is plan 09 of 12. The series runs strictly one plan at a time, in numeric order (see [00-plan-index.md](00-plan-index.md)).

**Next to execute:** [Plan 10: Headers and Auth sub-tabs](2026-10-07-folder-settings-plan-10-headers-auth-sections.md).

**Instruction for the executing Claude:** when every task in this plan is done and its verification commands (listed in the last step of each task) pass, record "Task N: complete" for each task in the ledger at `.superpowers/sdd/folder-settings/progress.md`, then open Plan 10 and start its first task immediately. Do not ask the user for consent between plans. Stop and report to the user instead if a verification command fails, if a plan's stated prerequisite is missing, or if you must deviate from the locked contract in the index.

**Notes for the next plan (from the plan author):**

`docs/superpowers/plans/folder-settings/2026-10-07-folder-settings-plan-10-headers-auth-sections.md` (Plan 10: Headers and Auth sub-tabs). It builds on the `useFolderSettings` hook and the `onChange(patch)` convention from this plan. Plan 11 (Script and Test sub-tabs) also depends on this plan and can run after Plan 10.
