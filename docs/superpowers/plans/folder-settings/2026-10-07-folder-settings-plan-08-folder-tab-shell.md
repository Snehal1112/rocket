# Folder settings, Plan 08: Folder tab shell, store action and sidebar click

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clicking a folder in the sidebar opens a Folder Settings tab with six sub-tabs (Headers, Script, Test, Vars, Auth, Docs). Each sub-tab shows a short placeholder. The real editors arrive in Plans 09 to 11. This plan makes no backend call.

**Architecture:** A new `folder` tab type (`FolderTab`) keyed by collection name and folder path, with an `activeSection` field like the collection tab. `pane-store` gets `openFolderTab` and `updateFolderSection` (the locked contract) plus one additive action, `renameFolderTabs`, so a folder rename retargets open tabs the way `renameScriptTabs` does. Folder deletion closes matching tabs through the existing `findAffectedTabs` path in the sidebar. `FolderSettingsTab` is a shadcn `Tabs` shell. Each section body is its own tiny component file under `src/components/collections/folder-settings/`, so later plans only replace file contents. `FolderNode` gets a separate expand chevron, a row click that opens the tab, and a "Settings" menu item.

**Tech Stack:** React + TypeScript, Zustand, shadcn/ui (`Tabs`), lucide-react, Vitest and Testing Library. Use Yarn.

**Spec:** `docs/superpowers/specs/2026-10-07-folder-settings-design.md` (section "Frontend"). Locked names are in `docs/superpowers/plans/folder-settings/00-plan-index.md` ("Frontend (plans 08 to 11)").

## Global Constraints

- Contract names are used verbatim: `FolderSection`, `FolderTab { tabType: 'folder'; collectionName: string; folderPath: string; activeSection: FolderSection }`, `openFolderTab(collection: string, folderPath: string, section?: FolderSection): boolean`, `updateFolderSection(tabId: string, section: FolderSection): void`, component `src/components/collections/FolderSettingsTab.tsx`.
- Additive deviation from the index: `renameFolderTabs(collection, oldPath, newPath)` in `pane-store.ts`, plus helpers `isFolderTab`, `findFolderTab`, `findFolderTabsWithin`. Update the index when this plan lands.
- `openFolderTab` returns `true` when an already-open tab was reused and `false` when a new tab was created. It always leaves the tab focused.
- A folder tab has no `source`. `source` makes `updateTabTitle` call `renameRequest` and shows the request "Save" menu entry, both wrong for a folder. Deletion and rename are handled by explicit folder matching instead.
- Folder tabs are not restored after an app restart. `src/lib/ui-state.ts` persists only collection tabs, and a folder tab is cheap to reopen. Do not change `ui-state.ts`.
- Tab identity is `(collectionName, folderPath)`. The tab `id` is `folder:<uuid>` and stays stable across renames, so panes keep their active tab.
- Hard rules: shadcn/ui primitives only (no raw `<button>`, `<input>`, `<select>`), `lucide-react` icons only, narrow Zustand selectors, no full destructuring of store state in components. The one `span role='button'` in `FolderNode` follows the existing `ContractBadge` pattern, because the tree row is already a `<button>` and HTML forbids nesting buttons.
- Code comments are short full sentences ending with a punctuation mark. No emojis.
- Commit with the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only (never `git add -A` or `.`) and use a pathspec commit. The harness runs `yarn tsc --noEmit` and `yarn check` for staged `.ts` and `.tsx` files before the commit.
- `FolderVariablesPopover.tsx` stays on disk. Only its use in `FolderNode` goes away here. Plan 09 deletes the file once the Vars section replaces it.

## Review Focus

1. Opening the same folder twice yields one tab, and a second open can switch the section (Task 1 tests `opens a new folder tab`, `reuses the open tab for the same collection and folder`, `switches the section of a reused tab only when one is given`).
2. The same folder path in two collections stays two tabs, and a tab parked in another collection's snapshot is found instead of duplicated (Task 1 tests `keeps same-named folders of two collections apart` and `finds a tab parked in a collection snapshot`).
3. Opening a folder tab while in workspace mode leaves workspace mode like any other non-workspace tab (Task 1 test `leaves workspace mode`).
4. Renaming a folder retargets the folder's own tab and tabs of nested folders by whole path segments, and never a sibling that merely shares a prefix (Task 1 test `retargets the renamed folder and nested folder tabs by whole segments`, Task 3 test `retargets the open folder tab when the folder is renamed`).
5. Deleting a folder or its collection closes matching folder tabs (Task 1 test `findAffectedTabs matches folder tabs`).
6. The tab shows six triggers in the order Headers, Script, Test, Vars, Auth, Docs, bound to `activeSection` (Task 2 tests `shows the six sections in order` and `switching a section updates the tab in the store`).
7. A row click opens the tab and expands a collapsed folder but never collapses an expanded one, and the chevron toggles without opening a tab (Task 3 tests `row click opens the tab and expands the folder`, `row click never collapses an expanded folder`, `chevron toggles without opening a tab`).
8. "Settings" exists in both menus, and "Variables" opens the tab on the Vars section (Task 3 tests `the Settings menu item opens the tab`, `the context menu has Settings` and `the Variables menu item opens the Vars section`).
9. Folder tabs cannot be renamed from the tab bar, because the title is the folder name (Task 1 step 6 edit, covered by `yarn tsc --noEmit` and by the `updateTabTitle` guard, which cannot run `renameRequest` because a folder tab has no `source`).

---

## Task 1: `FolderTab` type, store actions and tab plumbing

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src/types/pane-types.ts`
- Modify: `src/lib/pane-utils.ts`
- Modify: `src/stores/pane-store.ts`
- Modify: `src/components/collections/tree-utils.ts`
- Modify: `src/components/panes/TabItem.tsx`, `src/components/panes/TabBar.tsx`, `src/components/panes/BreadcrumbBar.tsx`, `src/components/panes/EditorGroup.tsx`
- Create: `src/components/collections/folder-settings/sections.ts`
- Create: `src/components/collections/FolderSettingsTab.tsx` (a one-line stub that Task 2 replaces, needed so `EditorGroup` compiles)
- Test: `src/stores/__tests__/pane-store-folder.test.ts`

**Interfaces:**
- Consumes: `usePaneStore` internals `updateTabEverywhere`, `openTab`, `switchCollection`, `isWorkspaceMode`; `isPathWithin` from `@/lib/pane-utils`.
- Produces:

```ts
// src/types/pane-types.ts
export type FolderSection = 'headers' | 'script' | 'test' | 'vars' | 'auth' | 'docs';
export interface FolderTab extends BaseTab {
  tabType: 'folder';
  collectionName: string;
  folderPath: string;
  activeSection: FolderSection;
}
export function isFolderTab(tab: Tab): tab is FolderTab;

// src/lib/pane-utils.ts
export function findFolderTab(node: PaneNode, collection: string, folderPath: string): { leaf: LeafNode; tab: FolderTab } | null;
export function findFolderTabsWithin(node: PaneNode, collection: string, folderPath: string): FolderTab[];

// src/stores/pane-store.ts (PaneState)
openFolderTab: (collection: string, folderPath: string, section?: FolderSection) => boolean;
updateFolderSection: (tabId: string, section: FolderSection) => void;
renameFolderTabs: (collection: string, oldPath: string, newPath: string) => void;

// src/components/collections/folder-settings/sections.ts
export const FOLDER_SECTIONS: ReadonlyArray<{ id: FolderSection; label: string }>;
export function folderSectionLabel(section: FolderSection): string;
export function isFolderSection(value: string): value is FolderSection;
export interface FolderSectionProps {
  collectionName: string;
  folderPath: string;
  settings?: FolderSettings;
  onChange?: (patch: Partial<FolderSettings>) => void;
}
```

- [ ] **Step 1: Write the failing store tests**

Create `src/stores/__tests__/pane-store-folder.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { findAffectedTabs } from '@/components/collections/tree-utils';
import { collectAllTabs, findFolderTab } from '@/lib/pane-utils';
import type { FolderTab } from '@/types/pane-types';
import { isFolderTab } from '@/types/pane-types';
import { usePaneStore } from '../pane-store';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn(), endAgentSession: vi.fn() };
});

function folderTabs(): FolderTab[] {
  return collectAllTabs(usePaneStore.getState().root).filter(isFolderTab);
}

function activeTabId(): string {
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('Expected a single leaf');
  return root.activeTabId;
}

describe('pane-store folder tabs', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
  });

  it('opens a new folder tab', () => {
    const reused = usePaneStore.getState().openFolderTab('col', 'auth/oauth');
    expect(reused).toBe(false);
    const tabs = folderTabs();
    expect(tabs).toHaveLength(1);
    expect(tabs[0]).toMatchObject({
      tabType: 'folder',
      title: 'oauth',
      collectionName: 'col',
      folderPath: 'auth/oauth',
      activeSection: 'headers',
      isDirty: false,
    });
    expect(tabs[0].source).toBeUndefined();
    expect(tabs[0].id.startsWith('folder:')).toBe(true);
    expect(activeTabId()).toBe(tabs[0].id);
  });

  it('opens a new folder tab on the requested section', () => {
    usePaneStore.getState().openFolderTab('col', 'auth', 'vars');
    expect(folderTabs()[0].activeSection).toBe('vars');
  });

  it('reuses the open tab for the same collection and folder', () => {
    usePaneStore.getState().openFolderTab('col', 'auth');
    const firstId = folderTabs()[0].id;
    usePaneStore.getState().openFolderTab('col', 'other');
    expect(activeTabId()).not.toBe(firstId);

    const reused = usePaneStore.getState().openFolderTab('col', 'auth');
    expect(reused).toBe(true);
    expect(folderTabs().filter((t) => t.folderPath === 'auth')).toHaveLength(1);
    expect(activeTabId()).toBe(firstId);
  });

  it('switches the section of a reused tab only when one is given', () => {
    usePaneStore.getState().openFolderTab('col', 'auth', 'docs');
    usePaneStore.getState().openFolderTab('col', 'auth');
    expect(folderTabs()[0].activeSection).toBe('docs');
    usePaneStore.getState().openFolderTab('col', 'auth', 'vars');
    expect(folderTabs()[0].activeSection).toBe('vars');
  });

  it('keeps same-named folders of two collections apart', () => {
    usePaneStore.getState().openFolderTab('col-a', 'auth');
    usePaneStore.getState().openFolderTab('col-b', 'auth');
    expect(usePaneStore.getState().activeCollection).toBe('col-b');
    expect(folderTabs().map((t) => t.collectionName)).toEqual(['col-b']);
    expect(findFolderTab(usePaneStore.getState().root, 'col-a', 'auth')).toBeNull();
  });

  it('finds a tab parked in a collection snapshot', () => {
    usePaneStore.getState().openFolderTab('col-a', 'auth');
    const id = folderTabs()[0].id;
    usePaneStore.getState().switchCollection('col-b');
    expect(folderTabs()).toHaveLength(0);

    const reused = usePaneStore.getState().openFolderTab('col-a', 'auth');
    expect(reused).toBe(true);
    expect(folderTabs().map((t) => t.id)).toEqual([id]);
  });

  it('leaves workspace mode', () => {
    usePaneStore.getState().openWorkspaceTabs('ws-1');
    expect(usePaneStore.getState().isWorkspaceMode()).toBe(true);
    usePaneStore.getState().openFolderTab('col', 'auth');
    expect(usePaneStore.getState().isWorkspaceMode()).toBe(false);
    expect(folderTabs()).toHaveLength(1);
  });

  it('updateFolderSection changes only the named tab', () => {
    usePaneStore.getState().openFolderTab('col', 'a');
    usePaneStore.getState().openFolderTab('col', 'b');
    const [a, b] = folderTabs();
    usePaneStore.getState().updateFolderSection(a.id, 'auth');
    const after = folderTabs();
    expect(after.find((t) => t.id === a.id)?.activeSection).toBe('auth');
    expect(after.find((t) => t.id === b.id)?.activeSection).toBe('headers');
  });

  it('retargets the renamed folder and nested folder tabs by whole segments', () => {
    usePaneStore.getState().openFolderTab('col', 'auth');
    usePaneStore.getState().openFolderTab('col', 'auth/oauth');
    usePaneStore.getState().openFolderTab('col', 'authx');
    usePaneStore.getState().openFolderTab('col', 'auth');
    const idsBefore = folderTabs().map((t) => t.id);

    usePaneStore.getState().renameFolderTabs('col', 'auth', 'login');

    const tabs = folderTabs();
    expect(tabs.map((t) => t.id)).toEqual(idsBefore);
    expect(tabs.map((t) => t.folderPath)).toEqual(['login', 'login/oauth', 'authx']);
    expect(tabs.map((t) => t.title)).toEqual(['login', 'oauth', 'authx']);
  });

  it('findAffectedTabs matches folder tabs', () => {
    usePaneStore.getState().openFolderTab('col', 'auth');
    usePaneStore.getState().openFolderTab('col', 'auth/oauth');
    usePaneStore.getState().openFolderTab('col', 'authx');
    const { root } = usePaneStore.getState();
    const paths = (target: Parameters<typeof findAffectedTabs>[1]) =>
      findAffectedTabs(root, target).map(({ tab }) => (isFolderTab(tab) ? tab.folderPath : '?'));

    expect(paths({ type: 'folder', collection: 'col', path: 'auth', name: 'auth' })).toEqual([
      'auth',
      'auth/oauth',
    ]);
    expect(paths({ type: 'collection', collection: 'col', name: 'col' })).toEqual([
      'auth',
      'auth/oauth',
      'authx',
    ]);
    expect(paths({ type: 'folder', collection: 'other', path: 'auth', name: 'auth' })).toEqual([]);
    expect(paths({ type: 'request', collection: 'col', path: 'auth', name: 'r' })).toEqual([]);
  });
});
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `yarn test src/stores/__tests__/pane-store-folder.test.ts --run`
Expected: FAIL. `isFolderTab`, `findFolderTab`, `openFolderTab` and `renameFolderTabs` do not exist yet (import errors or "is not a function").

- [ ] **Step 3: Add the types**

In `src/types/pane-types.ts`, add after the `CollectionTab` interface (after `activeSection?: CollectionSection;\n}`):

```ts
export type FolderSection = 'headers' | 'script' | 'test' | 'vars' | 'auth' | 'docs';

export interface FolderTab extends BaseTab {
  tabType: 'folder';
  collectionName: string;
  /** Folder path relative to the collection root, for example `auth/oauth`. */
  folderPath: string;
  activeSection: FolderSection;
}
```

Add `| FolderTab` to the `Tab` union (after `| ScriptTab`):

```ts
  | FlowTab
  | ScriptTab
  | FolderTab;
```

Add after `isCollectionTab`:

```ts
export function isFolderTab(tab: Tab): tab is FolderTab {
  return tab.tabType === 'folder';
}
```

- [ ] **Step 4: Add the sections module and the tab stub**

Create `src/components/collections/folder-settings/sections.ts`:

```ts
import type { FolderSettings } from '@/lib/tauri-api';
import type { FolderSection } from '@/types/pane-types';

/** The six folder sections in display order. */
export const FOLDER_SECTIONS: ReadonlyArray<{ id: FolderSection; label: string }> = [
  { id: 'headers', label: 'Headers' },
  { id: 'script', label: 'Script' },
  { id: 'test', label: 'Test' },
  { id: 'vars', label: 'Vars' },
  { id: 'auth', label: 'Auth' },
  { id: 'docs', label: 'Docs' },
];

export function folderSectionLabel(section: FolderSection): string {
  return FOLDER_SECTIONS.find((s) => s.id === section)?.label ?? section;
}

export function isFolderSection(value: string): value is FolderSection {
  return FOLDER_SECTIONS.some((s) => s.id === value);
}

/** Props every folder section body receives. */
export interface FolderSectionProps {
  collectionName: string;
  /** Folder path relative to the collection root. */
  folderPath: string;
  /** Supplied by the tab once plan 09 adds the settings hook. Placeholders ignore it. */
  settings?: FolderSettings;
  onChange?: (patch: Partial<FolderSettings>) => void;
}
```

Create `src/components/collections/FolderSettingsTab.tsx` (replaced in Task 2):

```tsx
import type { FolderTab } from '@/types/pane-types';

// Stub so the tab router compiles. Task 2 of plan 08 replaces it with the real shell.
export function FolderSettingsTab({ tab }: { tab: FolderTab }) {
  return <div className='p-4 text-sm text-muted-foreground'>{tab.folderPath}</div>;
}
```

- [ ] **Step 5: Add the pane helpers**

In `src/lib/pane-utils.ts`, change the imports:

```ts
import type {
  BodyState,
  FolderTab,
  GrpcState,
  LeafNode,
  PaneNode,
  RequestState,
  ScriptTab,
  SplitNode,
  Tab,
} from '@/types/pane-types';
import { isFolderTab, isScriptTab } from '@/types/pane-types';
```

Add after `findScriptTab` (before `// Returns the leftmost/topmost leaf in the tree.`):

```ts
// Collects every open folder tab of a collection whose folder is `folderPath` or below it.
export function findFolderTabsWithin(
  node: PaneNode,
  collection: string,
  folderPath: string,
): FolderTab[] {
  if (node.type !== 'leaf') {
    return [
      ...findFolderTabsWithin(node.children[0], collection, folderPath),
      ...findFolderTabsWithin(node.children[1], collection, folderPath),
    ];
  }
  return node.tabs.filter(
    (tab): tab is FolderTab =>
      isFolderTab(tab) &&
      tab.collectionName === collection &&
      isPathWithin(tab.folderPath, folderPath),
  );
}

// Finds the open folder tab for a collection and folder path, if any.
export function findFolderTab(
  node: PaneNode,
  collection: string,
  folderPath: string,
): { leaf: LeafNode; tab: FolderTab } | null {
  if (node.type === 'leaf') {
    for (const tab of node.tabs) {
      if (isFolderTab(tab) && tab.collectionName === collection && tab.folderPath === folderPath) {
        return { leaf: node, tab };
      }
    }
    return null;
  }
  return (
    findFolderTab(node.children[0], collection, folderPath) ??
    findFolderTab(node.children[1], collection, folderPath)
  );
}
```

- [ ] **Step 6: Add the store actions**

In `src/stores/pane-store.ts`:

1. Add `findFolderTab` and `findFolderTabsWithin` to the `@/lib/pane-utils` import (after `findActiveLeaf`), add `FolderSection` and `FolderTab` to the type import (after `FlowTab`), and add `isFolderTab` to the value import:

```ts
import { isFlowTab, isFolderTab, isRequestTab, isRunnerTab, isScriptTab } from '@/types/pane-types';
```

2. In `PaneState`, after the `renameScriptTabs` line, add:

```ts
  /** Opens or focuses the settings tab of a folder. Returns true when an open tab was reused. */
  openFolderTab: (collection: string, folderPath: string, section?: FolderSection) => boolean;
  updateFolderSection: (tabId: string, section: FolderSection) => void;
  /** Retargets open folder tabs when a folder is renamed, matching whole path segments. */
  renameFolderTabs: (collection: string, oldPath: string, newPath: string) => void;
```

3. In `openTab`, extend the collection-name derivation:

```ts
    const collectionName =
      tab.tabType === 'collection'
        ? (tab as CollectionTab).collectionName
        : tab.tabType === 'contract'
          ? (tab as ContractTab).collectionName
          : tab.tabType === 'folder'
            ? tab.collectionName
            : (tab.source?.collection ?? null);
```

4. At the end of the store, replace

```ts
  updateCollectionSection(tabId, section) {
    const { root } = get();
    set({
      root: updateTabInTree(root, tabId, (tab) => {
        if (tab.tabType !== 'collection') return tab;
        return { ...tab, activeSection: section };
      }),
    });
  },
}));
```

with

```ts
  updateCollectionSection(tabId, section) {
    const { root } = get();
    set({
      root: updateTabInTree(root, tabId, (tab) => {
        if (tab.tabType !== 'collection') return tab;
        return { ...tab, activeSection: section };
      }),
    });
  },

  openFolderTab(collection, folderPath, section) {
    // Switch first, so a tab parked in another collection's snapshot is found and not duplicated.
    if (!get().isWorkspaceMode() && get().activeCollection !== collection) {
      get().switchCollection(collection);
    }
    const existing = findFolderTab(get().root, collection, folderPath);
    if (existing) {
      if (section) get().updateFolderSection(existing.tab.id, section);
      // The id is all openTab reads for an open tab, so it only activates it.
      get().openTab(existing.tab);
      return true;
    }
    const tab: FolderTab = {
      id: `folder:${crypto.randomUUID()}`,
      title: folderPath.split('/').pop() ?? folderPath,
      tabType: 'folder',
      collectionName: collection,
      folderPath,
      activeSection: section ?? 'headers',
      isDirty: false,
    };
    get().openTab(tab);
    return false;
  },

  updateFolderSection(tabId, section) {
    set(
      updateTabEverywhere(get(), tabId, (tab) =>
        isFolderTab(tab) ? { ...tab, activeSection: section } : tab,
      ),
    );
  },

  renameFolderTabs(collection, oldPath, newPath) {
    // Matches the folder itself or any folder below it, by whole segments.
    // Matching by tab id keeps the id stable, so panes keep their active tab.
    const tabs = findFolderTabsWithin(get().root, collection, oldPath);
    if (tabs.length === 0) return;
    let next = get();
    for (const found of tabs) {
      const target = `${newPath}${found.folderPath.slice(oldPath.length)}`;
      next = {
        ...next,
        ...updateTabEverywhere(next, found.id, (tab) => {
          if (!isFolderTab(tab)) return tab;
          return { ...tab, folderPath: target, title: target.split('/').pop() ?? target };
        }),
      };
    }
    set({ root: next.root, collectionTabState: next.collectionTabState });
  },
}));
```

- [ ] **Step 7: Close folder tabs on delete**

In `src/components/collections/tree-utils.ts`, change the import and the loop in `findAffectedTabs`.

```ts
import { isFolderTab, isScriptTab } from '@/types/pane-types';
```

Replace the start of the `for (const tab of node.tabs)` body:

```ts
    for (const tab of node.tabs) {
      // Folder tabs have no source. They match by collection and folder path.
      if (isFolderTab(tab)) {
        if (tab.collectionName !== target.collection) continue;
        const folderMatches =
          target.type === 'collection' ||
          (target.type === 'folder' && isPathWithin(tab.folderPath, target.path ?? ''));
        if (folderMatches) found.push({ tab, groupId: node.groupId });
        continue;
      }
      if (!tab.source || tab.source.collection !== target.collection) continue;
```

(The remaining lines of the loop stay as they are.)

- [ ] **Step 8: Wire the tab into the tab bar, breadcrumb and router**

`src/components/panes/TabItem.tsx`: add `FolderCog` to the lucide import (alphabetical, before `GitBranch`... after `FileLock`), add `isFolderTab` to the `@/types/pane-types` import list (after `isContractTab`), and add a branch before the final `BoxIcon` fallback:

```tsx
      ) : isScriptTab(tab) ? (
        <FileCode aria-hidden='true' className='h-4 w-4 shrink-0' />
      ) : isFolderTab(tab) ? (
        <FolderCog aria-hidden='true' className='h-4 w-4 shrink-0' />
      ) : (
```

`src/components/panes/TabBar.tsx`: change the import to `import { isFolderTab, isScriptTab, isWorkspaceTab } from '@/types/pane-types';` and replace both guards. A folder tab title is the folder name, so it is never renamed from the tab bar.

```tsx
                  onDoubleClick={() => {
                    // Script and folder tabs take their title from the file or folder name.
                    if (isScriptTab(tab) || isFolderTab(tab)) return;
```

```tsx
              disabled={isScriptTab(tab) || isFolderTab(tab)}
```

`src/components/panes/BreadcrumbBar.tsx`:

1. Imports: `import type { CollectionSection, FolderSection, Tab, WorkspaceTabSection } from '@/types/pane-types';`, add `isFolderTab` after `isFlowTab` in the value import, and add:

```ts
import { FOLDER_SECTIONS, folderSectionLabel } from '@/components/collections/folder-settings/sections';
```

(place it with the other `@/components` imports, so Biome's import order stays valid).

2. `NavActions`: add `updateFolderSection: (tabId: string, section: FolderSection) => void;`.

3. In `BreadcrumbBar`, add `const updateFolderSection = usePaneStore((s) => s.updateFolderSection);`, and add `updateFolderSection` to both the `nav` object and the `useMemo` dependency list.

4. In `deriveSegments`, add before `const _exhaustive: never = tab;`:

```tsx
  if (isFolderTab(tab)) {
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
      { label: tab.folderPath, icon: <FolderOpen className='h-3 w-3' /> },
      {
        label: folderSectionLabel(tab.activeSection),
        picker: {
          loadItems: () =>
            FOLDER_SECTIONS.map((s) => ({
              id: s.id,
              label: s.label,
              isActive: s.id === tab.activeSection,
            })),
          onSelect: (item) => nav.updateFolderSection(tab.id, item.id as FolderSection),
        },
      },
    ];
  }
```

`src/components/panes/EditorGroup.tsx`: add `import { FolderSettingsTab } from '@/components/collections/FolderSettingsTab';` next to the `CollectionOverviewTab` import, add `isFolderTab` to the `@/types/pane-types` value import (after `isFlowTab`), and route the tab before the `CollectionOverviewTab` fallback:

```tsx
          ) : isFlowTab(activeTab) ? (
            <Suspense fallback={<EditorSkeleton />}>
              <FlowPane tab={activeTab} groupId={node.groupId} />
            </Suspense>
          ) : isFolderTab(activeTab) ? (
            <FolderSettingsTab key={activeTab.id} tab={activeTab} />
          ) : (
            <CollectionOverviewTab tab={activeTab} />
          )
```

- [ ] **Step 9: Run the tests and the checks**

Run: `yarn test src/stores/__tests__/pane-store-folder.test.ts --run`
Expected: PASS, 10 tests.

Run: `yarn test src/stores --run`
Expected: PASS (existing store tests unchanged).

Run: `yarn tsc --noEmit`
Expected: no errors. A leftover error in `BreadcrumbBar.tsx` (`never`) or `EditorGroup.tsx` (`CollectionOverviewTab` prop type) means a branch from step 8 is missing.

Run: `yarn check`
Expected: no errors. If only import order or formatting is reported, run `yarn lint` and re-run `yarn check`.

- [ ] **Step 10: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths and commit with a pathspec:

```bash
git add src/types/pane-types.ts src/lib/pane-utils.ts src/stores/pane-store.ts \
  src/components/collections/tree-utils.ts \
  src/components/collections/FolderSettingsTab.tsx \
  src/components/collections/folder-settings/sections.ts \
  src/components/panes/TabItem.tsx src/components/panes/TabBar.tsx \
  src/components/panes/BreadcrumbBar.tsx src/components/panes/EditorGroup.tsx \
  src/stores/__tests__/pane-store-folder.test.ts
```

Suggested subject: `feat(ui): add a folder tab type with store actions`.

---

## Task 2: `FolderSettingsTab` shell and six placeholder sections

**Files:**
- Modify (replace contents): `src/components/collections/FolderSettingsTab.tsx`
- Create: `src/components/collections/folder-settings/HeadersSection.tsx`
- Create: `src/components/collections/folder-settings/ScriptSection.tsx`
- Create: `src/components/collections/folder-settings/TestSection.tsx`
- Create: `src/components/collections/folder-settings/VarsSection.tsx`
- Create: `src/components/collections/folder-settings/AuthSection.tsx`
- Create: `src/components/collections/folder-settings/DocsSection.tsx`
- Test: `src/components/collections/__tests__/FolderSettingsTab.test.tsx`

**Interfaces:**
- Consumes: `FolderTab`, `FolderSection`, `isFolderSection`, `FOLDER_SECTIONS`, `FolderSectionProps` (Task 1); `usePaneStore.updateFolderSection` (Task 1); shadcn `Tabs`, `TabsList`, `TabsTrigger`, `TabsContent` from `@/components/ui/tabs`.
- Produces:

```ts
// src/components/collections/FolderSettingsTab.tsx
export function FolderSettingsTab(props: { tab: FolderTab }): JSX.Element;

// src/components/collections/folder-settings/*.tsx (each takes FolderSectionProps)
export function HeadersSection(props: FolderSectionProps): JSX.Element; // replaced by plan 10
export function ScriptSection(props: FolderSectionProps): JSX.Element;  // replaced by plan 11
export function TestSection(props: FolderSectionProps): JSX.Element;    // replaced by plan 11
export function VarsSection(props: FolderSectionProps): JSX.Element;    // replaced by plan 09
export function AuthSection(props: FolderSectionProps): JSX.Element;    // replaced by plan 10
export function DocsSection(props: FolderSectionProps): JSX.Element;    // replaced by plan 09
```

- [ ] **Step 1: Write the failing component test**

Create `src/components/collections/__tests__/FolderSettingsTab.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { FolderSettingsTab } from '@/components/collections/FolderSettingsTab';
import { collectAllTabs } from '@/lib/pane-utils';
import { usePaneStore } from '@/stores/pane-store';
import type { FolderTab } from '@/types/pane-types';
import { isFolderTab } from '@/types/pane-types';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn(), endAgentSession: vi.fn() };
});

function storedTab(): FolderTab {
  const tab = collectAllTabs(usePaneStore.getState().root).find(isFolderTab);
  if (!tab) throw new Error('No folder tab in the store');
  return tab;
}

// The tab reads its section from the store, so the test re-renders with the stored tab.
function renderStoredTab() {
  const view = render(<FolderSettingsTab tab={storedTab()} />);
  const unsubscribe = usePaneStore.subscribe(() => {
    view.rerender(<FolderSettingsTab tab={storedTab()} />);
  });
  return { ...view, unsubscribe };
}

describe('FolderSettingsTab', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    usePaneStore.getState().openFolderTab('my-col', 'auth/oauth');
  });

  it('shows the folder name and the collection breadcrumb', () => {
    const { unsubscribe } = renderStoredTab();
    expect(screen.getByRole('heading', { name: 'oauth' })).toBeInTheDocument();
    expect(screen.getByText('my-col / auth / oauth')).toBeInTheDocument();
    unsubscribe();
  });

  it('shows the six sections in order', () => {
    const { unsubscribe } = renderStoredTab();
    const labels = screen.getAllByRole('tab').map((t) => t.textContent);
    expect(labels).toEqual(['Headers', 'Script', 'Test', 'Vars', 'Auth', 'Docs']);
    expect(screen.getByRole('tab', { name: 'Headers' })).toHaveAttribute('aria-selected', 'true');
    unsubscribe();
  });

  it('shows the placeholder of the active section', () => {
    usePaneStore.getState().openFolderTab('my-col', 'auth/oauth', 'docs');
    const { unsubscribe } = renderStoredTab();
    expect(screen.getByRole('tab', { name: 'Docs' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByText('Docs for this folder will be editable here.')).toBeInTheDocument();
    unsubscribe();
  });

  it('switching a section updates the tab in the store', async () => {
    const { unsubscribe } = renderStoredTab();
    await userEvent.click(screen.getByRole('tab', { name: 'Auth' }));
    expect(storedTab().activeSection).toBe('auth');
    expect(screen.getByRole('tab', { name: 'Auth' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByText('Auth for this folder will be editable here.')).toBeInTheDocument();
    unsubscribe();
  });
});
```

- [ ] **Step 2: Run the test and confirm it fails**

Run: `yarn test src/components/collections/__tests__/FolderSettingsTab.test.tsx --run`
Expected: FAIL. The stub renders only the folder path, so the heading and tab roles are missing.

- [ ] **Step 3: Create the six placeholder sections**

These files are intentional scaffolding. Each is a placeholder that a later plan replaces in place. Keep the export name and the `FolderSectionProps` parameter, so `FolderSettingsTab` never changes when they are filled.

`src/components/collections/folder-settings/HeadersSection.tsx`:

```tsx
import type { FolderSectionProps } from './sections';

// Placeholder from plan 08. Plan 10 replaces this body with the headers editor.
export function HeadersSection(_props: FolderSectionProps) {
  return (
    <p className='p-4 text-sm text-muted-foreground'>
      Headers for this folder will be editable here.
    </p>
  );
}
```

`ScriptSection.tsx` (plan 11, text `Script for this folder will be editable here.`), `TestSection.tsx` (plan 11, `Test for this folder will be editable here.`), `VarsSection.tsx` (plan 09, `Vars for this folder will be editable here.`), `AuthSection.tsx` (plan 10, `Auth for this folder will be editable here.`), `DocsSection.tsx` (plan 09, `Docs for this folder will be editable here.`) use the same shape. For example `ScriptSection.tsx`:

```tsx
import type { FolderSectionProps } from './sections';

// Placeholder from plan 08. Plan 11 replaces this body with the pre-request and post-response editors.
export function ScriptSection(_props: FolderSectionProps) {
  return (
    <p className='p-4 text-sm text-muted-foreground'>
      Script for this folder will be editable here.
    </p>
  );
}
```

Use the matching export name and plan number in the comment of each remaining file (`TestSection` plan 11 "the tests editor", `VarsSection` plan 09 "the variables editor", `AuthSection` plan 10 "the auth editor", `DocsSection` plan 09 "the docs editor").

- [ ] **Step 4: Replace the tab stub with the shell**

Overwrite `src/components/collections/FolderSettingsTab.tsx`:

```tsx
import { FolderCog } from 'lucide-react';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { usePaneStore } from '@/stores/pane-store';
import type { FolderTab } from '@/types/pane-types';
import { AuthSection } from './folder-settings/AuthSection';
import { DocsSection } from './folder-settings/DocsSection';
import { HeadersSection } from './folder-settings/HeadersSection';
import { ScriptSection } from './folder-settings/ScriptSection';
import { TestSection } from './folder-settings/TestSection';
import { VarsSection } from './folder-settings/VarsSection';
import { FOLDER_SECTIONS, isFolderSection } from './folder-settings/sections';

interface FolderSettingsTabProps {
  tab: FolderTab;
}

// Folder settings shell: a header and one sub-tab per section. The sections fill in over plans 09 to 11.
export function FolderSettingsTab({ tab }: FolderSettingsTabProps) {
  const updateFolderSection = usePaneStore((s) => s.updateFolderSection);
  const folderName = tab.folderPath.split('/').pop() ?? tab.folderPath;
  const breadcrumb = [tab.collectionName, ...tab.folderPath.split('/')].join(' / ');
  const section = { collectionName: tab.collectionName, folderPath: tab.folderPath };

  return (
    <div className='flex h-full min-h-0 flex-col'>
      <div className='flex shrink-0 items-center gap-2 border-b border-border px-4 py-3'>
        <FolderCog aria-hidden='true' className='h-5 w-5 shrink-0 text-muted-foreground' />
        <div className='min-w-0'>
          <h1 className='truncate text-base font-semibold text-foreground'>{folderName}</h1>
          <p className='truncate text-xs text-muted-foreground'>{breadcrumb}</p>
        </div>
      </div>
      <Tabs
        value={tab.activeSection}
        onValueChange={(value) => {
          if (isFolderSection(value)) updateFolderSection(tab.id, value);
        }}
        className='flex min-h-0 flex-1 flex-col px-4 pt-3'
      >
        <TabsList className='self-start'>
          {FOLDER_SECTIONS.map((s) => (
            <TabsTrigger key={s.id} value={s.id}>
              {s.label}
            </TabsTrigger>
          ))}
        </TabsList>
        <TabsContent value='headers' className='min-h-0 flex-1 overflow-auto'>
          <HeadersSection {...section} />
        </TabsContent>
        <TabsContent value='script' className='min-h-0 flex-1 overflow-auto'>
          <ScriptSection {...section} />
        </TabsContent>
        <TabsContent value='test' className='min-h-0 flex-1 overflow-auto'>
          <TestSection {...section} />
        </TabsContent>
        <TabsContent value='vars' className='min-h-0 flex-1 overflow-auto'>
          <VarsSection {...section} />
        </TabsContent>
        <TabsContent value='auth' className='min-h-0 flex-1 overflow-auto'>
          <AuthSection {...section} />
        </TabsContent>
        <TabsContent value='docs' className='min-h-0 flex-1 overflow-auto'>
          <DocsSection {...section} />
        </TabsContent>
      </Tabs>
    </div>
  );
}
```

- [ ] **Step 5: Run the tests and the checks**

Run: `yarn test src/components/collections/__tests__/FolderSettingsTab.test.tsx --run`
Expected: PASS, 4 tests.

Run: `yarn tsc --noEmit`
Expected: no errors.

Run: `yarn check`
Expected: no errors. If only formatting or import order is reported, run `yarn lint` and re-run `yarn check`.

- [ ] **Step 6: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths and commit with a pathspec:

```bash
git add src/components/collections/FolderSettingsTab.tsx \
  src/components/collections/folder-settings/HeadersSection.tsx \
  src/components/collections/folder-settings/ScriptSection.tsx \
  src/components/collections/folder-settings/TestSection.tsx \
  src/components/collections/folder-settings/VarsSection.tsx \
  src/components/collections/folder-settings/AuthSection.tsx \
  src/components/collections/folder-settings/DocsSection.tsx \
  src/components/collections/__tests__/FolderSettingsTab.test.tsx
```

Suggested subject: `feat(ui): add the folder settings tab shell`.

---

## Task 3: `FolderNode` click, chevron and Settings menu items

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Click behavior decision.** Today a row click toggles expansion, and there is no chevron: `TreeItem` only draws one when it has nested `Tree` children, and `FolderNode` renders its children outside the `TreeItem`. The spec says the chevron "keeps its current behavior", but there is nothing to keep, and a row click must now open the tab. So:

- A new chevron span toggles expansion and nothing else. It is keyboard reachable (Enter and Space).
- A row click opens the settings tab and expands a collapsed folder. It never collapses one. A row click that collapsed the folder would hide the contents the user just clicked to work in, and the chevron is the explicit collapse control.
- Keyboard users reach the tab through the "Settings" menu item. A row `<button>` fires only the tree's own handler on Enter, which already did not toggle folders before this plan.

**Files:**
- Modify: `src/components/collections/FolderNode.tsx`
- Test: `src/components/collections/__tests__/FolderNode.test.tsx`

**Interfaces:**
- Consumes: `usePaneStore.openFolderTab` and `renameFolderTabs` (Task 1); `FolderSection` (Task 1); `cn` from `@/lib/utils`.
- Produces: no new exports. `FolderNode` props are unchanged.

- [ ] **Step 1: Write the failing component tests**

Create `src/components/collections/__tests__/FolderNode.test.tsx`:

```tsx
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { FolderNode } from '@/components/collections/FolderNode';
import { Tree } from '@/components/ui/tree';
import type { CollectionItem, CollectionSummary } from '@/lib/tauri-api';
import { moveItem } from '@/lib/tauri-api';
import { collectAllTabs } from '@/lib/pane-utils';
import { usePaneStore } from '@/stores/pane-store';
import type { FolderTab } from '@/types/pane-types';
import { isFolderTab } from '@/types/pane-types';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    getCollection: vi.fn(),
    endAgentSession: vi.fn(),
    moveItem: vi.fn().mockResolvedValue(undefined),
  };
});

const childFolder: CollectionItem = { type: 'folder', uid: 'f-child', name: 'child', items: [] };

function folderTabs(): FolderTab[] {
  return collectAllTabs(usePaneStore.getState().root).filter(isFolderTab);
}

function renderNode() {
  const summaries: CollectionSummary[] = [];
  render(
    <Tree aria-label='tree'>
      <FolderNode
        name='parent'
        items={[childFolder]}
        collectionName='col'
        collectionRoot='/ws/col'
        basePath='parent'
        depth={0}
        filter=''
        summaries={summaries}
        onNewFolder={vi.fn()}
        onMove={vi.fn()}
        onDelete={vi.fn()}
        onDuplicate={vi.fn()}
      />
    </Tree>,
  );
}

describe('FolderNode', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.mocked(moveItem).mockClear();
  });

  it('row click opens the tab and expands the folder', () => {
    renderNode();
    expect(screen.queryByText('child')).not.toBeInTheDocument();

    fireEvent.click(screen.getByText('parent'));

    expect(folderTabs()).toHaveLength(1);
    expect(folderTabs()[0]).toMatchObject({
      collectionName: 'col',
      folderPath: 'parent',
      activeSection: 'headers',
    });
    expect(screen.getByText('child')).toBeInTheDocument();
  });

  it('row click never collapses an expanded folder', () => {
    renderNode();
    fireEvent.click(screen.getByText('parent'));
    fireEvent.click(screen.getByText('parent'));

    expect(screen.getByText('child')).toBeInTheDocument();
    expect(folderTabs()).toHaveLength(1);
  });

  it('chevron toggles without opening a tab', () => {
    renderNode();
    fireEvent.click(screen.getByRole('button', { name: 'Expand parent' }));
    expect(screen.getByText('child')).toBeInTheDocument();
    expect(folderTabs()).toHaveLength(0);

    fireEvent.click(screen.getByRole('button', { name: 'Collapse parent' }));
    expect(screen.queryByText('child')).not.toBeInTheDocument();
    expect(folderTabs()).toHaveLength(0);
  });

  it('the Settings menu item opens the tab', async () => {
    renderNode();
    await userEvent.click(screen.getByRole('button', { name: 'Actions for parent' }));
    await userEvent.click(await screen.findByText('Settings'));
    expect(folderTabs().map((t) => t.activeSection)).toEqual(['headers']);
  });

  it('the Variables menu item opens the Vars section', async () => {
    renderNode();
    await userEvent.click(screen.getByRole('button', { name: 'Actions for parent' }));
    await userEvent.click(await screen.findByText('Variables'));
    expect(folderTabs().map((t) => t.activeSection)).toEqual(['vars']);
  });

  it('the context menu has Settings', async () => {
    renderNode();
    fireEvent.contextMenu(screen.getByText('parent'));
    await userEvent.click(await screen.findByText('Settings'));
    expect(folderTabs()).toHaveLength(1);
  });

  it('retargets the open folder tab when the folder is renamed', async () => {
    usePaneStore.getState().openFolderTab('col', 'parent');
    const id = folderTabs()[0].id;
    renderNode();

    await userEvent.click(screen.getByRole('button', { name: 'Actions for parent' }));
    await userEvent.click(await screen.findByText('Rename'));
    const input = await screen.findByDisplayValue('parent');
    fireEvent.change(input, { target: { value: 'renamed' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    await waitFor(() => expect(folderTabs()[0].folderPath).toBe('renamed'));
    expect(moveItem).toHaveBeenCalledWith('col', 'parent', 'col', 'renamed');
    expect(folderTabs()[0].id).toBe(id);
    expect(folderTabs()[0].title).toBe('renamed');
  });
});
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `yarn test src/components/collections/__tests__/FolderNode.test.tsx --run`
Expected: FAIL. No folder tab is opened by a row click, there is no chevron button, and there is no "Settings" item.

- [ ] **Step 3: Edit `FolderNode.tsx`**

1. Imports. Add `ChevronRight` and `Settings` to the lucide import (keep `Variable`), add `cn`, add the `FolderSection` type, and drop the popover import:

```tsx
import {
  ChevronRight,
  FileCode,
  Folder,
  FolderOpen,
  FolderPlus,
  MoreHorizontal,
  Pencil,
  Plus,
  Settings,
  Trash2,
  Variable,
} from 'lucide-react';
```

```tsx
import { createDefaultRequest } from '@/lib/pane-utils';
import type { CollectionItem, CollectionSummary } from '@/lib/tauri-api';
import { moveItem, saveRequest } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { useContractStore } from '@/stores/contract-store';
import { usePaneStore } from '@/stores/pane-store';
import type { FolderSection } from '@/types/pane-types';
import { RequestNode } from './RequestNode';
import type { DeleteTarget } from './tree-utils';
```

(Remove the line `import { FolderVariablesPopover } from './FolderVariablesPopover';`.)

2. Remove the `varsOpen` state line `const [varsOpen, setVarsOpen] = useState(false);`, and add after the `useEffect` that auto-expands on filter:

```tsx
  // Opens or focuses this folder's settings tab. A given section switches the tab to it.
  const openSettings = (section?: FolderSection) => {
    usePaneStore.getState().openFolderTab(collectionName, basePath, section);
  };
```

3. In `handleRename`, retarget the folder tabs next to the script tabs:

```tsx
      usePaneStore.getState().renameScriptTabs(collectionName, basePath, newPath);
      usePaneStore.getState().renameFolderTabs(collectionName, basePath, newPath);
```

4. Row content. Replace the `TreeItemContent` opening and the folder icons:

```tsx
              <TreeItemContent
                className='flex items-center gap-1 w-full px-2 py-1 text-sm rounded-sm cursor-pointer'
                onClick={() => {
                  // A row click opens the settings and expands. Only the chevron collapses.
                  openSettings();
                  setOpen(true);
                }}
              >
                {/* biome-ignore lint/a11y/useSemanticElements: nested inside TreeItem's <button> row, HTML forbids button-in-button */}
                <span
                  role='button'
                  tabIndex={0}
                  aria-label={`${open ? 'Collapse' : 'Expand'} ${name}`}
                  aria-expanded={open}
                  className='flex h-4 w-4 shrink-0 items-center justify-center rounded-sm hover:bg-accent'
                  onClick={(e) => {
                    e.stopPropagation();
                    setOpen((prev) => !prev);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      e.stopPropagation();
                      setOpen((prev) => !prev);
                    }
                  }}
                >
                  <ChevronRight
                    aria-hidden='true'
                    className={cn('h-3 w-3 transition-transform', open && 'rotate-90')}
                  />
                </span>
                {open ? (
```

(The existing `FolderOpen` and `Folder` icon lines that follow `{open ? (` stay unchanged.)

5. Dropdown menu. Replace

```tsx
                <DropdownMenuSeparator />
                <DropdownMenuItem onClick={() => setVarsOpen(true)}>
                  <Variable className='h-3.5 w-3.5 mr-2' /> Variables
                </DropdownMenuItem>
```

with

```tsx
                <DropdownMenuSeparator />
                <DropdownMenuItem onClick={() => openSettings()}>
                  <Settings aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Settings
                </DropdownMenuItem>
                <DropdownMenuItem onClick={() => openSettings('vars')}>
                  <Variable className='h-3.5 w-3.5 mr-2' /> Variables
                </DropdownMenuItem>
```

6. Context menu. Replace

```tsx
          <ContextMenuSeparator />
          <ContextMenuItem onClick={() => setVarsOpen(true)}>
            <Variable className='h-3.5 w-3.5 mr-2' /> Variables
          </ContextMenuItem>
```

with

```tsx
          <ContextMenuSeparator />
          <ContextMenuItem onClick={() => openSettings()}>
            <Settings aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Settings
          </ContextMenuItem>
          <ContextMenuItem onClick={() => openSettings('vars')}>
            <Variable className='h-3.5 w-3.5 mr-2' /> Variables
          </ContextMenuItem>
```

7. Delete the popover block:

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

- [ ] **Step 4: Run the tests and the checks**

Run: `yarn test src/components/collections --run`
Expected: PASS, including the new 7 `FolderNode` tests and the existing `CollectionNode`, `RequestNode`, `ScriptNode` tests. A failing existing test that clicks a folder row to expand it still works, because a row click expands.

Run: `yarn tsc --noEmit`
Expected: no errors (`setVarsOpen` and `varsOpen` are gone, `FolderVariablesPopover` is unused but still compiles).

Run: `yarn check`
Expected: no errors. If only formatting or import order is reported, run `yarn lint` and re-run `yarn check`.

- [ ] **Step 5: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths and commit with a pathspec:

```bash
git add src/components/collections/FolderNode.tsx \
  src/components/collections/__tests__/FolderNode.test.tsx
```

Suggested subject: `feat(ui): open folder settings from the sidebar`.

---

## Next Plan

[Plan 09: Settings hook, Vars and Docs sub-tabs](2026-10-07-folder-settings-plan-09-settings-hook-vars-docs.md). It depends on this plan (`FolderTab`, `FolderSettingsTab`, the section files under `folder-settings/`) and on Plan 04 (`getFolderSettings`, `saveFolderSettings`). It replaces `VarsSection.tsx` and `DocsSection.tsx`, adds `src/hooks/useFolderSettings.ts`, and deletes `FolderVariablesPopover.tsx`. Chain to it automatically when this one finishes.
