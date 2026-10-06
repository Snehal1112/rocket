# JS Script Files Plan 03: Script File UI

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Users see `.js` files in the sidebar, create them from a "New Script" context menu item, edit them in a Monaco tab with Save, and rename or delete them.

**Architecture:** A new `script` tab type in `pane-store` holds the file content and the last saved content. A lazy `ScriptFilePane` renders Monaco and saves through `saveScriptFile`. A `ScriptNode` tree item and a shared `NewScriptDialog` plug into `CollectionNode` and `FolderNode`. Deleting and renaming go through the existing sidebar flows.

**Tech Stack:** React 18, TypeScript, Zustand, shadcn/ui, lucide-react, Monaco (`MonacoWrapper`), Vitest, Yarn.

**Spec:** `docs/superpowers/specs/2026-10-06-js-script-files-design.md` ("Frontend"). Prerequisites: Plans 01 and 02 are merged; `createScriptFile`, `readScriptFile`, `saveScriptFile`, `renameScriptFile`, `deleteScriptFile` and the `scriptFile` `CollectionItem` member exist in `src/lib/tauri-api.ts`.

## Global Constraints

- shadcn/ui primitives only: no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`. Icons from `lucide-react` only. Multi-line editor is Monaco only.
- Zustand: never fully destructure store state at component top level. Use narrow selectors.
- Package manager is Yarn.
- Code comments: short full sentences ending in a period.
- Commits through `dev-workflow-skills:1-git-commit`, conventional subjects, staged by path.
- Verification: `yarn tsc --noEmit` and `yarn check` before every commit. Run focused tests with `yarn test <pattern>`.

## Review Focus

- Unsaved edits in a script tab must survive switching tabs and panes (content lives in the store, not in component state).
- Deleting a script that is open must close its tab; renaming one must retarget its tab, not leave a tab pointing at a missing file (Task 2).
- A script name with a path separator or a duplicate name shows the backend error in the dialog and keeps the dialog open (Task 2).
- Saving a script that failed to save must keep the dirty marker (Task 1).
- The sidebar filter must match script names (Task 2).

---

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `src/types/pane-types.ts` | modify | `ScriptTab`, `isScriptTab`, `Tab` union |
| `src/lib/pane-utils.ts` | modify | `findScriptTab` |
| `src/stores/pane-store.ts` | modify | `openScriptTab`, `updateScriptContent`, `markScriptSaved`, `renameScriptTabs` |
| `src/components/scripts/ScriptFilePane.tsx` | create | Monaco editor, Save, require hint |
| `src/components/panes/EditorGroup.tsx` | modify | render `ScriptFilePane` |
| `src/components/panes/TabItem.tsx` | modify | `FileCode` icon |
| `src/components/panes/BreadcrumbBar.tsx` | modify | breadcrumb for script tabs |
| `src/components/collections/ScriptNode.tsx` | create | sidebar row with rename and delete |
| `src/components/collections/NewScriptDialog.tsx` | create | name prompt |
| `src/components/collections/CollectionNode.tsx` | modify | New Script menu item, render nodes, filter |
| `src/components/collections/FolderNode.tsx` | modify | same, for folders |
| `src/components/collections/tree-utils.ts` | modify | `DeleteTarget` gains `'script'` |
| `src/components/layout/CollectionsSidebar.tsx` | modify | delete a script |
| `.claude/script-files.md` | create | developer doc |
| `CLAUDE.md` | modify | one-line pointer |
| `crates/rocket-scripting/CLAUDE.md` | modify | `ScriptFileScope` row |

---

### Task 1: Script tab, store actions and editor pane

**Files:**
- Modify: `src/types/pane-types.ts`, `src/lib/pane-utils.ts`, `src/stores/pane-store.ts`, `src/components/panes/EditorGroup.tsx`, `src/components/panes/TabItem.tsx`, `src/components/panes/BreadcrumbBar.tsx`
- Create: `src/components/scripts/ScriptFilePane.tsx`
- Test: `src/stores/__tests__/pane-store-script.test.ts`, `src/components/scripts/__tests__/ScriptFilePane.test.tsx`

**Interfaces:**
- Produces (`pane-types.ts`):

```ts
export interface ScriptTab extends BaseTab {
  tabType: 'script';
  collectionName: string;
  /** Collection-relative path, for example `lib/utils.js`. */
  scriptPath: string;
  content: string;
  savedContent: string;
}
export function isScriptTab(tab: Tab): tab is ScriptTab;
```

- Produces (`pane-utils.ts`): `findScriptTab(node: PaneNode, collection: string, path: string): { leaf: LeafNode; tab: ScriptTab } | null`.
- Produces (`pane-store`): `openScriptTab(collectionName: string, path: string): Promise<void>`, `updateScriptContent(tabId: string, content: string): void`, `markScriptSaved(tabId: string, content: string): void`, `renameScriptTabs(collection: string, oldPath: string, newPath: string): void`.

- [ ] **Step 1: Write the failing store tests**

Create `src/stores/__tests__/pane-store-script.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { findScriptTab } from '@/lib/pane-utils';
import { readScriptFile } from '@/lib/tauri-api';
import { isScriptTab } from '@/types/pane-types';
import { usePaneStore } from '../pane-store';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn(), readScriptFile: vi.fn(), endAgentSession: vi.fn() };
});

function findTab(collection: string, path: string) {
  return findScriptTab(usePaneStore.getState().root, collection, path);
}

describe('pane-store script tabs', () => {
  beforeEach(() => {
    usePaneStore.getState().closeAll();
    vi.mocked(readScriptFile).mockReset();
  });

  it('opens a script tab with the file content and a clean state', async () => {
    vi.mocked(readScriptFile).mockResolvedValue('module.exports = 1;');
    await usePaneStore.getState().openScriptTab('col', 'lib/utils.js');
    const found = findTab('col', 'lib/utils.js');
    expect(found).not.toBeNull();
    expect(found?.tab.title).toBe('utils.js');
    expect(found?.tab.content).toBe('module.exports = 1;');
    expect(found?.tab.isDirty).toBe(false);
    expect(found?.tab.source).toEqual({ collection: 'col', path: 'lib/utils.js' });
  });

  it('focuses the existing tab instead of reading the file again', async () => {
    vi.mocked(readScriptFile).mockResolvedValue('a');
    await usePaneStore.getState().openScriptTab('col', 'a.js');
    await usePaneStore.getState().openScriptTab('col', 'a.js');
    expect(readScriptFile).toHaveBeenCalledTimes(1);
  });

  it('does not open a tab when the read fails', async () => {
    vi.mocked(readScriptFile).mockRejectedValue('boom');
    await expect(usePaneStore.getState().openScriptTab('col', 'x.js')).rejects.toBe('boom');
    expect(findTab('col', 'x.js')).toBeNull();
  });

  it('tracks dirty state against the last saved content', async () => {
    vi.mocked(readScriptFile).mockResolvedValue('one');
    await usePaneStore.getState().openScriptTab('col', 'a.js');
    const id = findTab('col', 'a.js')?.tab.id ?? '';

    usePaneStore.getState().updateScriptContent(id, 'two');
    expect(findTab('col', 'a.js')?.tab.isDirty).toBe(true);

    usePaneStore.getState().updateScriptContent(id, 'one');
    expect(findTab('col', 'a.js')?.tab.isDirty).toBe(false);

    usePaneStore.getState().updateScriptContent(id, 'three');
    usePaneStore.getState().markScriptSaved(id, 'three');
    const tab = findTab('col', 'a.js')?.tab;
    expect(tab?.isDirty).toBe(false);
    expect(tab?.savedContent).toBe('three');
  });

  it('retargets tabs when a script is renamed and keeps the id', async () => {
    vi.mocked(readScriptFile).mockResolvedValue('x');
    await usePaneStore.getState().openScriptTab('col', 'lib/a.js');
    const before = findTab('col', 'lib/a.js')?.tab;

    usePaneStore.getState().renameScriptTabs('col', 'lib/a.js', 'lib/b.js');

    expect(findTab('col', 'lib/a.js')).toBeNull();
    const after = findTab('col', 'lib/b.js')?.tab;
    expect(after?.id).toBe(before?.id);
    expect(after?.title).toBe('b.js');
    expect(after && isScriptTab(after) && after.source).toEqual({
      collection: 'col',
      path: 'lib/b.js',
    });
  });
});
```

Run: `yarn test pane-store-script`
Expected: FAIL (`openScriptTab` is not a function).

- [ ] **Step 2: Add the tab type**

In `src/types/pane-types.ts`, after `FlowTab`/`isFlowTab` add:

```ts
export interface ScriptTab extends BaseTab {
  tabType: 'script';
  collectionName: string;
  /** Collection-relative path, for example `lib/utils.js`. */
  scriptPath: string;
  content: string;
  savedContent: string;
}

export function isScriptTab(tab: Tab): tab is ScriptTab {
  return tab.tabType === 'script';
}
```

and add `| ScriptTab` to the `Tab` union.

In `src/lib/pane-utils.ts`, import `ScriptTab` and `isScriptTab` from `@/types/pane-types` and add after `findTabInTree`:

```ts
// Finds the open script tab for a collection-relative path, if any.
export function findScriptTab(
  node: PaneNode,
  collection: string,
  path: string,
): { leaf: LeafNode; tab: ScriptTab } | null {
  if (node.type === 'leaf') {
    for (const tab of node.tabs) {
      if (isScriptTab(tab) && tab.collectionName === collection && tab.scriptPath === path) {
        return { leaf: node, tab };
      }
    }
    return null;
  }
  return (
    findScriptTab(node.children[0], collection, path) ??
    findScriptTab(node.children[1], collection, path)
  );
}
```

- [ ] **Step 3: Add the store actions**

In `src/stores/pane-store.ts`:
1. Import `readScriptFile` from `@/lib/tauri-api`, `findScriptTab` from `@/lib/pane-utils`, and `ScriptTab` plus `isScriptTab` from `@/types/pane-types`.
2. Add to the `PaneState` interface, near `openContractTab`:

```ts
  openScriptTab: (collectionName: string, path: string) => Promise<void>;
  updateScriptContent: (tabId: string, content: string) => void;
  markScriptSaved: (tabId: string, content: string) => void;
  renameScriptTabs: (collection: string, oldPath: string, newPath: string) => void;
```

3. Add the implementations next to `openContractTab`:

```ts
  async openScriptTab(collectionName, path) {
    // An open tab only needs focusing, so skip reading the file again.
    const existing = findScriptTab(get().root, collectionName, path);
    if (existing) {
      get().openTab(existing.tab);
      return;
    }
    const content = await readScriptFile(collectionName, path);
    // A concurrent call may have opened the same file while the read was in flight.
    const raced = findScriptTab(get().root, collectionName, path);
    if (raced) {
      get().openTab(raced.tab);
      return;
    }
    const tab: ScriptTab = {
      id: `script:${crypto.randomUUID()}`,
      title: path.split('/').pop() ?? path,
      tabType: 'script',
      collectionName,
      scriptPath: path,
      content,
      savedContent: content,
      isDirty: false,
      source: { collection: collectionName, path },
    };
    get().openTab(tab);
  },

  updateScriptContent(tabId, content) {
    set(
      updateTabEverywhere(get(), tabId, (tab) => {
        if (!isScriptTab(tab)) return tab;
        return { ...tab, content, isDirty: content !== tab.savedContent };
      }),
    );
  },

  markScriptSaved(tabId, content) {
    set(
      updateTabEverywhere(get(), tabId, (tab) => {
        if (!isScriptTab(tab)) return tab;
        return { ...tab, savedContent: content, isDirty: tab.content !== content };
      }),
    );
  },

  renameScriptTabs(collection, oldPath, newPath) {
    // Matching by path (not id) keeps the id stable, so panes keep their active tab.
    const found = findScriptTab(get().root, collection, oldPath);
    if (!found) return;
    set(
      updateTabEverywhere(get(), found.tab.id, (tab) => {
        if (!isScriptTab(tab)) return tab;
        return {
          ...tab,
          scriptPath: newPath,
          title: newPath.split('/').pop() ?? newPath,
          source: { collection, path: newPath },
        };
      }),
    );
  },
```

Note `markScriptSaved` compares against the current `content`, so typing during an in-flight save keeps the dirty marker.

4. In `openTab`, the collection-name derivation already falls back to `tab.source?.collection`, which script tabs set, so no change is needed there.

- [ ] **Step 4: Run the store tests**

Run: `yarn test pane-store-script`
Expected: PASS.

- [ ] **Step 5: Write the failing pane test**

Create `src/components/scripts/__tests__/ScriptFilePane.test.tsx`:

```tsx
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ScriptFilePane } from '@/components/scripts/ScriptFilePane';
import { findScriptTab } from '@/lib/pane-utils';
import { readScriptFile, saveScriptFile } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { ScriptTab } from '@/types/pane-types';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    getCollection: vi.fn(),
    readScriptFile: vi.fn(),
    saveScriptFile: vi.fn(),
    endAgentSession: vi.fn(),
  };
});
vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn() } }));
// A plain stub replaces Monaco, which needs a browser layout engine.
vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: ({ value, onChange }: { value: string; onChange?: (v: string) => void }) => (
    <textarea aria-label='editor' value={value} onChange={(e) => onChange?.(e.target.value)} />
  ),
}));

async function openTab(path = 'lib/utils.js'): Promise<ScriptTab> {
  vi.mocked(readScriptFile).mockResolvedValue('module.exports = 1;');
  await usePaneStore.getState().openScriptTab('col', path);
  const found = findScriptTab(usePaneStore.getState().root, 'col', path);
  if (!found) throw new Error('tab not opened');
  return found.tab;
}

describe('ScriptFilePane', () => {
  beforeEach(() => {
    usePaneStore.getState().closeAll();
    vi.mocked(saveScriptFile).mockReset();
  });

  it('shows the require hint relative to the collection root', async () => {
    const tab = await openTab();
    render(<ScriptFilePane tab={tab} />);
    expect(screen.getByText("require('./lib/utils.js')")).toBeInTheDocument();
  });

  it('marks the tab dirty on edit and saves through the command', async () => {
    vi.mocked(saveScriptFile).mockResolvedValue(undefined);
    const tab = await openTab();
    const { rerender } = render(<ScriptFilePane tab={tab} />);

    fireEvent.change(await screen.findByLabelText('editor'), { target: { value: 'edited' } });
    const edited = findScriptTab(usePaneStore.getState().root, 'col', 'lib/utils.js')?.tab;
    expect(edited?.isDirty).toBe(true);
    if (edited) rerender(<ScriptFilePane tab={edited} />);

    fireEvent.click(screen.getByRole('button', { name: /save/i }));
    await waitFor(() =>
      expect(saveScriptFile).toHaveBeenCalledWith('col', 'lib/utils.js', 'edited'),
    );
    await waitFor(() =>
      expect(findScriptTab(usePaneStore.getState().root, 'col', 'lib/utils.js')?.tab.isDirty).toBe(
        false,
      ),
    );
  });

  it('keeps the dirty marker when saving fails', async () => {
    vi.mocked(saveScriptFile).mockRejectedValue('disk full');
    const tab = await openTab();
    usePaneStore.getState().updateScriptContent(tab.id, 'edited');
    const edited = findScriptTab(usePaneStore.getState().root, 'col', 'lib/utils.js')?.tab;
    if (!edited) throw new Error('missing tab');
    render(<ScriptFilePane tab={edited} />);

    fireEvent.click(screen.getByRole('button', { name: /save/i }));
    await waitFor(() => expect(saveScriptFile).toHaveBeenCalled());
    expect(findScriptTab(usePaneStore.getState().root, 'col', 'lib/utils.js')?.tab.isDirty).toBe(
      true,
    );
  });

  it('disables Save when there are no changes', async () => {
    const tab = await openTab();
    render(<ScriptFilePane tab={tab} />);
    expect(screen.getByRole('button', { name: /save/i })).toBeDisabled();
  });
});
```

Run: `yarn test ScriptFilePane`
Expected: FAIL (module not found).

- [ ] **Step 6: Implement the pane**

Create `src/components/scripts/ScriptFilePane.tsx`:

```tsx
import { FileCode, Save } from 'lucide-react';
import { lazy, Suspense, useCallback, useState } from 'react';
import { toast } from 'sonner';
import { EditorSkeleton } from '@/components/editor/EditorSkeleton';
import { Button } from '@/components/ui/button';
import { findTabInTree } from '@/lib/pane-utils';
import { saveScriptFile } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { isScriptTab, type ScriptTab } from '@/types/pane-types';

const MonacoWrapper = lazy(() =>
  import('@/components/editor/MonacoWrapper').then((m) => ({ default: m.MonacoWrapper })),
);

interface ScriptFilePaneProps {
  tab: ScriptTab;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** Editor for a shared `.js` file. Scripts load it with `require('./path.js')`. */
export function ScriptFilePane({ tab }: ScriptFilePaneProps) {
  const updateScriptContent = usePaneStore((s) => s.updateScriptContent);
  const markScriptSaved = usePaneStore((s) => s.markScriptSaved);
  const [saving, setSaving] = useState(false);

  const save = useCallback(async () => {
    // Read the latest content from the store, since the keyboard handler can be stale.
    const latest = findTabInTree(usePaneStore.getState().root, tab.id)?.tab;
    if (!latest || !isScriptTab(latest) || !latest.isDirty) return;
    const content = latest.content;
    setSaving(true);
    try {
      await saveScriptFile(latest.collectionName, latest.scriptPath, content);
      markScriptSaved(latest.id, content);
    } catch (err) {
      toast.error(`Could not save "${latest.title}": ${errorMessage(err)}`);
    } finally {
      setSaving(false);
    }
  }, [tab.id, markScriptSaved]);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
      e.preventDefault();
      void save();
    }
  };

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: captures Ctrl+S from the editor inside.
    <div className='flex h-full min-h-0 flex-col' onKeyDown={onKeyDown}>
      <div className='flex items-center gap-2 border-b px-3 py-1.5 text-xs'>
        <FileCode aria-hidden='true' className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />
        <span className='truncate font-medium'>{tab.scriptPath}</span>
        <span className='truncate text-muted-foreground'>
          {`require('./${tab.scriptPath}')`}
        </span>
        <Button
          type='button'
          size='sm'
          variant='outline'
          className='ml-auto h-6 gap-1 px-2 text-xs'
          disabled={!tab.isDirty || saving}
          onClick={() => void save()}
        >
          <Save aria-hidden='true' className='h-3 w-3' /> Save
        </Button>
      </div>
      <div className='min-h-0 flex-1'>
        <Suspense fallback={<EditorSkeleton />}>
          <MonacoWrapper
            language='javascript'
            value={tab.content}
            onChange={(value) => updateScriptContent(tab.id, value)}
            height='100%'
            phase='tests'
          />
        </Suspense>
      </div>
    </div>
  );
}
```

`phase='tests'` gives the widest `rok` and `res` typings, since a shared module can be loaded from any phase.

- [ ] **Step 7: Wire the tab into the pane chrome**

`src/components/panes/EditorGroup.tsx`: add a lazy import near `ContractDiffPane`:

```tsx
const ScriptFilePane = lazy(() =>
  import('@/components/scripts/ScriptFilePane').then((m) => ({ default: m.ScriptFilePane })),
);
```
import `isScriptTab` with the other `pane-types` guards, and add a branch before `isWorkspaceTab(activeTab)`:

```tsx
          ) : isScriptTab(activeTab) ? (
            <Suspense fallback={<EditorSkeleton />}>
              <ScriptFilePane key={activeTab.id} tab={activeTab} />
            </Suspense>
```

`src/components/panes/TabItem.tsx`: add `FileCode` to the lucide import, `isScriptTab` to the guards, and add a branch before the final `BoxIcon` fallback:

```tsx
      ) : isScriptTab(tab) ? (
        <FileCode aria-hidden='true' className='h-4 w-4 shrink-0' />
```

`src/components/panes/BreadcrumbBar.tsx`: import `FileCode` and `isScriptTab`, and add this before the `const _exhaustive: never = tab;` line:

```tsx
  if (isScriptTab(tab)) {
    return [
      {
        label: tab.collectionName,
        picker: {
          loadItems: async () => {
            const summaries = await listCollections();
            return summaries.map((s) => ({
              id: s.name,
              label: s.name,
              isActive: s.name === tab.collectionName,
            }));
          },
          onSelect: (item) => nav.switchCollection(item.id),
        },
      },
      { label: tab.scriptPath, icon: <FileCode className='h-3 w-3' /> },
    ];
  }
```

- [ ] **Step 8: Run the checks**

Run: `yarn test pane-store-script ScriptFilePane && yarn tsc --noEmit && yarn check`
Expected: PASS and clean. If `yarn check` flags the `biome-ignore` comment text, adjust it to the rule Biome names.

- [ ] **Step 9: Commit**

Invoke `dev-workflow-skills:1-git-commit` for `src/types/pane-types.ts src/lib/pane-utils.ts src/stores/pane-store.ts src/stores/__tests__/pane-store-script.test.ts src/components/scripts src/components/panes/EditorGroup.tsx src/components/panes/TabItem.tsx src/components/panes/BreadcrumbBar.tsx`.
Suggested subject: `feat(ui): add script file editor tab`

---

### Task 2: Sidebar nodes, New Script, rename, delete and docs

**Files:**
- Create: `src/components/collections/ScriptNode.tsx`, `src/components/collections/NewScriptDialog.tsx`
- Modify: `src/components/collections/CollectionNode.tsx`, `FolderNode.tsx`, `tree-utils.ts`, `src/components/layout/CollectionsSidebar.tsx`
- Create: `.claude/script-files.md`; modify `CLAUDE.md`, `crates/rocket-scripting/CLAUDE.md`
- Test: `src/components/collections/__tests__/ScriptNode.test.tsx`, `src/components/collections/__tests__/NewScriptDialog.test.tsx`

**Interfaces:**
- Consumes: `openScriptTab`, `renameScriptTabs` (Task 1); `createScriptFile`, `renameScriptFile` (Plan 02).
- Produces:
  - `<ScriptNode name collectionName path onDelete />` where `path` is the collection-relative path.
  - `<NewScriptDialog open collectionName folderPath onClose />`. `folderPath` is `''` for the collection root.
  - `DeleteTarget.type` gains `'script'`.

- [ ] **Step 1: Write the failing tests**

Create `src/components/collections/__tests__/NewScriptDialog.test.tsx`:

```tsx
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { NewScriptDialog } from '@/components/collections/NewScriptDialog';
import { createScriptFile } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, createScriptFile: vi.fn(), readScriptFile: vi.fn().mockResolvedValue('x') };
});

describe('NewScriptDialog', () => {
  beforeEach(() => {
    vi.mocked(createScriptFile).mockReset();
    usePaneStore.getState().closeAll();
  });

  it('creates the script in the folder and opens it', async () => {
    vi.mocked(createScriptFile).mockResolvedValue('lib/utils.js');
    const onClose = vi.fn();
    render(<NewScriptDialog open collectionName='col' folderPath='lib' onClose={onClose} />);

    fireEvent.change(screen.getByLabelText('Script name'), { target: { value: 'utils' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create' }));

    await waitFor(() => expect(createScriptFile).toHaveBeenCalledWith('col', 'lib', 'utils'));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });

  it('shows the backend error and stays open', async () => {
    vi.mocked(createScriptFile).mockRejectedValue('Invalid input: utils.js already exists');
    const onClose = vi.fn();
    render(<NewScriptDialog open collectionName='col' folderPath='' onClose={onClose} />);

    fireEvent.change(screen.getByLabelText('Script name'), { target: { value: 'utils' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create' }));

    expect(await screen.findByText(/already exists/)).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });

  it('disables Create for an empty name', () => {
    render(<NewScriptDialog open collectionName='col' folderPath='' onClose={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Create' })).toBeDisabled();
  });
});
```

Create `src/components/collections/__tests__/ScriptNode.test.tsx`:

```tsx
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ScriptNode } from '@/components/collections/ScriptNode';
import { Tree } from '@/components/ui/tree';
import { findScriptTab } from '@/lib/pane-utils';
import { readScriptFile, renameScriptFile } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    getCollection: vi.fn(),
    readScriptFile: vi.fn(),
    renameScriptFile: vi.fn(),
    endAgentSession: vi.fn(),
  };
});
vi.mock('sonner', () => ({ toast: { error: vi.fn() } }));

function renderNode(onDelete = vi.fn()) {
  render(
    <Tree aria-label='tree'>
      <ScriptNode
        name='utils.js'
        collectionName='col'
        path='lib/utils.js'
        onDelete={onDelete}
      />
    </Tree>,
  );
  return onDelete;
}

describe('ScriptNode', () => {
  beforeEach(() => {
    usePaneStore.getState().closeAll();
    vi.mocked(readScriptFile).mockResolvedValue('module.exports = 1;');
    vi.mocked(renameScriptFile).mockReset();
  });

  it('opens a script tab on click', async () => {
    renderNode();
    fireEvent.click(screen.getByText('utils.js'));
    await waitFor(() =>
      expect(findScriptTab(usePaneStore.getState().root, 'col', 'lib/utils.js')).not.toBeNull(),
    );
  });

  it('asks the sidebar to delete with a script target', async () => {
    const onDelete = renderNode();
    fireEvent.click(screen.getByRole('button', { name: 'Actions for utils.js' }));
    fireEvent.click(await screen.findByText('Delete'));
    expect(onDelete).toHaveBeenCalledWith({
      type: 'script',
      collection: 'col',
      path: 'lib/utils.js',
      name: 'utils.js',
    });
  });

  it('renames the file and retargets the open tab', async () => {
    vi.mocked(renameScriptFile).mockResolvedValue('lib/helpers.js');
    await usePaneStore.getState().openScriptTab('col', 'lib/utils.js');
    renderNode();

    fireEvent.click(screen.getByRole('button', { name: 'Actions for utils.js' }));
    fireEvent.click(await screen.findByText('Rename'));
    const input = await screen.findByDisplayValue('utils.js');
    fireEvent.change(input, { target: { value: 'helpers' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    await waitFor(() =>
      expect(renameScriptFile).toHaveBeenCalledWith('col', 'lib/utils.js', 'helpers'),
    );
    await waitFor(() =>
      expect(findScriptTab(usePaneStore.getState().root, 'col', 'lib/helpers.js')).not.toBeNull(),
    );
  });
});
```

If `Tree` is not the export name in `@/components/ui/tree`, copy the wrapper used in `RequestNode.test.tsx`.

Run: `yarn test ScriptNode NewScriptDialog`
Expected: FAIL (modules not found).

- [ ] **Step 2: Implement `NewScriptDialog`**

Create `src/components/collections/NewScriptDialog.tsx`:

```tsx
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { createScriptFile } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';

interface NewScriptDialogProps {
  open: boolean;
  collectionName: string;
  /** Folder path relative to the collection root. Empty for the root. */
  folderPath: string;
  onClose: () => void;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** Prompts for a script name, creates the file and opens it in a tab. */
export function NewScriptDialog({
  open,
  collectionName,
  folderPath,
  onClose,
}: NewScriptDialogProps) {
  const [name, setName] = useState('');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);

  const close = () => {
    setName('');
    setError('');
    onClose();
  };

  const create = async () => {
    const trimmed = name.trim();
    if (!trimmed || busy) return;
    setBusy(true);
    setError('');
    try {
      const path = await createScriptFile(collectionName, folderPath, trimmed);
      await usePaneStore.getState().openScriptTab(collectionName, path);
      close();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(next) => !next && close()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>New script</DialogTitle>
          <DialogDescription>
            Creates a .js file you can load from any script with require().
          </DialogDescription>
        </DialogHeader>
        <div className='space-y-2'>
          <Label htmlFor='new-script-name'>Script name</Label>
          <Input
            id='new-script-name'
            autoFocus
            value={name}
            placeholder='utils'
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void create();
            }}
          />
          {error && (
            <p role='alert' className='text-xs text-destructive'>
              {error}
            </p>
          )}
        </div>
        <DialogFooter>
          <Button type='button' variant='ghost' onClick={close}>
            Cancel
          </Button>
          <Button type='button' disabled={!name.trim() || busy} onClick={() => void create()}>
            Create
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 3: Implement `ScriptNode`**

Create `src/components/collections/ScriptNode.tsx`. Base the layout on `RequestNode.tsx` lines ~195-330 (a `TreeItem` with `TreeItemContent`, a hover "..." `DropdownMenu`, an inline rename `Input`), but with a `FileCode` icon instead of the method badge and only Rename and Delete actions. Use the shadcn `Button` for the hover trigger (do not copy the raw `<button>` from `RequestNode`):

```tsx
import { FileCode, MoreHorizontal, Pencil, Trash2 } from 'lucide-react';
import { useRef, useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { TreeItem, TreeItemContent } from '@/components/ui/tree';
import { renameScriptFile } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { DeleteTarget } from './tree-utils';

interface ScriptNodeProps {
  name: string;
  collectionName: string;
  /** Collection-relative path, for example `lib/utils.js`. */
  path: string;
  onDelete: (target: DeleteTarget) => void;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export function ScriptNode({ name, collectionName, path, onDelete }: ScriptNodeProps) {
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState(name);
  const renameInFlight = useRef(false);
  // Set on Escape to block the blur that fires when the Input unmounts.
  const renameCancelled = useRef(false);

  const open = async () => {
    if (isRenaming) return;
    try {
      await usePaneStore.getState().openScriptTab(collectionName, path);
    } catch (err) {
      toast.error(`Could not open "${name}": ${errorMessage(err)}`);
    }
  };

  const handleRename = async () => {
    if (renameInFlight.current) return;
    if (renameCancelled.current) {
      renameCancelled.current = false;
      return;
    }
    const trimmed = renameValue.trim();
    if (!trimmed || trimmed === name) {
      setIsRenaming(false);
      return;
    }
    renameInFlight.current = true;
    try {
      const newPath = await renameScriptFile(collectionName, path, trimmed);
      usePaneStore.getState().renameScriptTabs(collectionName, path, newPath);
    } catch (err) {
      toast.error(`Could not rename "${name}": ${errorMessage(err)}`);
    } finally {
      renameInFlight.current = false;
      setIsRenaming(false);
    }
  };

  return (
    <div className='group relative flex items-center'>
      <TreeItem id={`script-${collectionName}-${path}`} textValue={name} className='w-full'>
        <TreeItemContent
          className='flex items-center gap-1 w-full px-2 py-1 text-sm rounded-sm cursor-pointer'
          onClick={() => void open()}
          aria-label={`Open script ${name}`}
        >
          <FileCode aria-hidden='true' className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />
          {isRenaming ? (
            <Input
              autoFocus
              className='h-6 text-sm flex-1'
              value={renameValue}
              onChange={(e) => setRenameValue(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') void handleRename();
                if (e.key === 'Escape') {
                  renameCancelled.current = true;
                  setIsRenaming(false);
                }
              }}
              onBlur={() => void handleRename()}
              onClick={(e) => e.stopPropagation()}
            />
          ) : (
            <span className='truncate text-foreground'>{name}</span>
          )}
        </TreeItemContent>
      </TreeItem>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            type='button'
            variant='ghost'
            size='icon'
            aria-label={`Actions for ${name}`}
            className='absolute right-1 h-5 w-5 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100'
            onClick={(e) => e.stopPropagation()}
          >
            <MoreHorizontal aria-hidden='true' className='h-3 w-3' />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent className='w-48' onClick={(e) => e.stopPropagation()}>
          <DropdownMenuItem
            onClick={() => {
              setRenameValue(name);
              setIsRenaming(true);
            }}
          >
            <Pencil aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Rename
          </DropdownMenuItem>
          <DropdownMenuItem
            className='text-destructive'
            onClick={() => onDelete({ type: 'script', collection: collectionName, path, name })}
          >
            <Trash2 aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Delete
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
```

Match the `TreeItem` props (`id`, `textValue`) to what `RequestNode.tsx` passes to its `TreeItem`; copy that usage if it differs.

- [ ] **Step 4: Extend `DeleteTarget` and the sidebar delete**

`src/components/collections/tree-utils.ts`: change the union to `type: 'collection' | 'folder' | 'request' | 'script';`.

`src/components/layout/CollectionsSidebar.tsx`: import `deleteScriptFile` from `@/lib/tauri-api` and extend `confirmDelete`. Replace the final `else { ... deleteRequest ... }` branch with:

```ts
      } else if (deleteTarget.type === 'script') {
        if (!deleteTarget.path) return;
        await deleteScriptFile(deleteTarget.collection, deleteTarget.path);
      } else {
        if (!deleteTarget.path) return;
        await deleteRequest(deleteTarget.collection, deleteTarget.path);
      }
```

and add `'script'` to the open-tab matching in `closeTabs`, so the deleted script's tab closes:

```ts
              ((deleteTarget.type === 'request' || deleteTarget.type === 'script') &&
                tab.source.collection === deleteTarget.collection &&
                tab.source.path === deleteTarget.path) ||
```

Check the delete confirmation dialog text near line 617: if it builds the message from `deleteTarget.type`, add a `'script'` case reading `script`.

- [ ] **Step 5: Render nodes, menu items and filtering**

`CollectionNode.tsx`:
1. Import `FileCode` from lucide-react, `ScriptNode`, and `NewScriptDialog`.
2. State: `const [newScriptOpen, setNewScriptOpen] = useState(false);`
3. Add a menu item right after each `New Folder` item (both the `DropdownMenuItem` near line 466 and the `ContextMenuItem` near line 525):

```tsx
<ContextMenuItem onClick={() => setNewScriptOpen(true)}>
  <FileCode aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> New Script
</ContextMenuItem>
```
(use `DropdownMenuItem` in the dropdown copy).
4. Render nodes in the item loop, before the `if (item.type === 'opaque') return null;` line:

```tsx
            if (item.type === 'scriptFile') {
              return (
                <ScriptNode
                  key={`script-${item.fileName}`}
                  name={item.name}
                  collectionName={summary.name}
                  path={item.fileName}
                  onDelete={onDelete}
                />
              );
            }
```
5. Make the filter match script names: change the filter predicate to
`(item.type !== 'request' && item.type !== 'summary' && item.type !== 'scriptFile') || item.name.toLowerCase().includes(filter.toLowerCase())`.
6. Render the dialog next to `CreateRequestDialog`:

```tsx
      <NewScriptDialog
        open={newScriptOpen}
        collectionName={summary.name}
        folderPath=''
        onClose={() => setNewScriptOpen(false)}
      />
```

`FolderNode.tsx`: the same five changes, with `collectionName={collectionName}`, `path={`${basePath}/${item.fileName}`}` and `folderPath={basePath}`. The `New Script` items go after the `New Folder` items near lines 237 and 282. Apply the same filter predicate change in its filter code (it mirrors `CollectionNode`; search for `item.type !== 'request'`).

Note `basePath` for a top-level folder is the folder name, so `${basePath}/${item.fileName}` matches the collection-relative paths the backend expects.

- [ ] **Step 6: Run the tests**

Run: `yarn test ScriptNode NewScriptDialog CollectionNode RequestNode`
Expected: PASS, including the pre-existing `CollectionNode` and `RequestNode` tests.

- [ ] **Step 7: Write the developer doc and pointers**

Create `.claude/script-files.md`:

```markdown
# Script files

Plain `.js` files in a collection that scripts load with `require('./x.js')`.
Design: `docs/superpowers/specs/2026-10-06-js-script-files-design.md`.

- Resolution lives in `crates/rocket-infra/src/scripting/local_modules.rs`. The op is
  `op_require_local`; the JS loader is in `scripting/bootstrap.js`.
- Allowed roots: the collection directory (Safe mode) plus `additionalContextRoots`
  (Developer mode only). Paths are canonicalised, so `..` and symlinks cannot escape.
- Only `.js` files load. `package.json` `main`, `index.js` and `.json` are not supported.
- `additionalContextRoots` is stored in `opencollection.yml` at
  `extensions.rocketapi.scripts.additionalContextRoots`. There is no settings UI yet.
- Files show in the tree as `CollectionItem::ScriptFile`. CRUD commands are
  `create/read/save/rename/delete_script_file`. The file watcher reports changes.
- Not covered yet: Flow Transform scripts, OpenCollection `.yml` `ScriptFile` items,
  a delete warning for referenced scripts.
```

Add one line to the end of the "Rules and conventions" section in `CLAUDE.md`: `See .claude/script-files.md for shared .js script files and local require().` Add this row to the key types table in `crates/rocket-scripting/CLAUDE.md`:

```markdown
| `ScriptFileScope` | `context.rs` | Collection root and extra roots for local-file `require()`; plain data, `rocket-infra` does the I/O |
```

- [ ] **Step 8: Run the full frontend checks**

Run: `yarn tsc --noEmit && yarn check && yarn test collections panes scripts stores`
Expected: all clean and passing.

- [ ] **Step 9: Commit**

Invoke `dev-workflow-skills:1-git-commit` for `src/components/collections src/components/layout/CollectionsSidebar.tsx .claude/script-files.md CLAUDE.md crates/rocket-scripting/CLAUDE.md`.
Suggested subject: `feat(ui): add script files to the sidebar`

---

## Manual check (after both tasks, needs the real app)

Run `yarn tauri dev`, then:
1. Right-click a collection, choose New Script, name it `utils`, confirm the tab opens with the template.
2. In a request's pre-request script add `const { greet } = require('./utils.js'); console.log(greet('x'));`, send, and confirm the console shows `Hello, x`.
3. Edit the script, confirm the dirty dot, press Ctrl+S, confirm the dot clears.
4. Rename it from the "..." menu and confirm the open tab title updates. Delete it and confirm the tab closes.
5. In Developer mode, add `additionalContextRoots: ["../shared"]` under `extensions.rocketapi.scripts` in `opencollection.yml`, put a script there, and confirm `require('../shared/x.js')` works. In Safe mode confirm it fails with "outside the allowed script roots".

## Plan 03 Self-Review

- Spec "Frontend" covered: New Script menu (T2), tree node with `FileCode` and rename/delete (T2), `script` tab type (T1), Monaco editor with Ctrl+S and no Send button (T1), require hint (T1).
- Names used across tasks: `ScriptTab`, `isScriptTab`, `findScriptTab`, `openScriptTab`, `updateScriptContent`, `markScriptSaved`, `renameScriptTabs`, `ScriptFilePane`, `ScriptNode`, `NewScriptDialog`, `DeleteTarget` `'script'`.
- Not covered: an open tab does not reload when the file changes on disk (for example after a git pull). Reopening the tab after closing it reads fresh content.
