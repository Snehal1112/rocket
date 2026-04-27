# BreadcrumbBar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a VSCode-style breadcrumb bar below the tab strip in every editor pane, showing contextual path segments for the active tab using VSCode 2026 breadcrumb tokens.

**Architecture:** A single pure `BreadcrumbBar` component derives its segments from the active `Tab` prop — no store subscriptions, no side effects. It is mounted in `EditorGroup.tsx` between `<TabBar>` and the content `<div>`. CSS vars are added to `globals.css` and registered in `tailwind.config.js` before the component is built.

**Tech Stack:** React + TypeScript, Tailwind CSS, lucide-react icons, existing `@/types/pane-types` tab types.

---

### Task 1: Add breadcrumb CSS vars and Tailwind tokens

**Files:**
- Modify: `src/globals.css`
- Modify: `tailwind.config.js`

- [ ] **Step 1: Add CSS vars to `:root` in `globals.css`**

In the `:root` block, after the `--text-preformat-fg` line, add:

```css
    /* VSCode 2026 Light: breadcrumb tokens */
    --breadcrumb-bg: 0 0% 100%;        /* breadcrumb.background #FFFFFF */
    --breadcrumb-fg: 0 0% 37.6%;      /* breadcrumb.foreground #606060 */
    --breadcrumb-focus-fg: 0 0% 12.5%; /* breadcrumb.focusForeground #202020 */
```

- [ ] **Step 2: Add CSS vars to `.dark` in `globals.css`**

In the `.dark` block, after the `--text-preformat-fg` dark line, add:

```css
    /* VSCode 2026 Dark: breadcrumb tokens */
    --breadcrumb-bg: 210 5% 7.5%;     /* breadcrumb.background #121314 */
    --breadcrumb-fg: 0 0% 55%;        /* breadcrumb.foreground #8C8C8C */
    --breadcrumb-focus-fg: 0 0% 75%;  /* breadcrumb.focusForeground #bfbfbf */
```

- [ ] **Step 3: Register in `tailwind.config.js`**

Inside the `colors` object, after the `statusbar` group, add:

```js
      breadcrumb: {
        bg:       'hsl(var(--breadcrumb-bg))',
        fg:       'hsl(var(--breadcrumb-fg))',
        'focus-fg': 'hsl(var(--breadcrumb-focus-fg))',
      },
```

- [ ] **Step 4: Verify TypeScript is clean**

```bash
yarn tsc --noEmit
```

Expected: no output (no errors).

- [ ] **Step 5: Commit**

```bash
git add src/globals.css tailwind.config.js
git commit -m "feat(theme): add breadcrumb CSS vars from VSCode 2026 tokens"
```

---

### Task 2: Create the BreadcrumbBar component

**Files:**
- Create: `src/components/panes/BreadcrumbBar.tsx`

The component derives an array of `{ label: string; icon?: React.ReactNode }` segments from the `Tab` prop, then renders them separated by `›` chevrons. The last segment uses `text-breadcrumb-focus-fg`; all others use `text-breadcrumb-fg`.

**Segment derivation rules per tab type:**

| Tab type | Segments |
|---|---|
| `request` with `source` | `source.collection` › each part of `source.path` split on `/` |
| `request` without `source` | `"Unsaved Request"` |
| `history` | same as `request` with `source` |
| `collection` | `collectionName` › `activeSection` label (`"Overview"` / `"Authorization"` / `"Variables"` / `"Documentation"`) |
| `workspace` | workspace name (from `useWorkspaceStore`) › `activeSection` label (`"Overview"` / `"Environments"` / `"Git"` / `"Audit"`) |
| `git` | `collectionName` › `"Git"` |
| `diff` | `collectionName` (from `diffState.collectionPath` basename) › `"Git"` › `diffState.filePath` |
| `conflict` | `collectionName` (from `conflictState.collectionPath` basename) › `"Git"` › `conflictState.filePath` |
| `contract` | `collectionName` › `"Contracts"` |

- [ ] **Step 1: Create `src/components/panes/BreadcrumbBar.tsx`**

```tsx
import { GitBranch, FileLock, FolderOpen, LayoutDashboard, Globe, Braces, Zap, Radio, ChevronRight } from 'lucide-react';
import { useWorkspaceStore } from '@/stores/workspace-store';
import type { Tab } from '@/types/pane-types';
import {
  isCollectionTab,
  isContractTab,
  isConflictTab,
  isDiffTab,
  isGitTab,
  isRequestTab,
  isWorkspaceTab,
} from '@/types/pane-types';
import { METHOD_TEXT_COLOR } from '@/lib/colors';

interface Segment {
  label: string;
  icon?: React.ReactNode;
}

function collectionBasename(absPath: string): string {
  return absPath.split('/').filter(Boolean).pop() ?? absPath;
}

function workspaceSectionLabel(section: string): string {
  switch (section) {
    case 'overview':     return 'Overview';
    case 'environments': return 'Environments';
    case 'git':          return 'Git';
    case 'audit':        return 'Audit';
    default:             return section;
  }
}

function collectionSectionLabel(section: string | undefined): string {
  switch (section) {
    case 'auth':          return 'Authorization';
    case 'variables':     return 'Variables';
    case 'documentation': return 'Documentation';
    default:              return 'Overview';
  }
}

function deriveSegments(tab: Tab, workspaceName: string): Segment[] {
  if (isRequestTab(tab)) {
    if (!tab.source) return [{ label: 'Unsaved Request' }];
    const parts = tab.source.path.split('/').filter(Boolean);
    const segments: Segment[] = [{ label: tab.source.collection }];
    for (let i = 0; i < parts.length - 1; i++) {
      segments.push({ label: parts[i], icon: <FolderOpen className='h-3 w-3' /> });
    }
    const name = parts[parts.length - 1] ?? tab.source.path;
    const methodIcon = (
      <span className={`font-semibold text-2xs ${METHOD_TEXT_COLOR[tab.request.method] ?? ''}`}>
        {tab.request.method}
      </span>
    );
    segments.push({ label: name, icon: methodIcon });
    return segments;
  }

  if (isCollectionTab(tab)) {
    return [
      { label: tab.collectionName },
      { label: collectionSectionLabel(tab.activeSection) },
    ];
  }

  if (isWorkspaceTab(tab)) {
    return [
      { label: workspaceName, icon: <LayoutDashboard className='h-3 w-3' /> },
      { label: workspaceSectionLabel(tab.activeSection) },
    ];
  }

  if (isGitTab(tab)) {
    return [
      { label: tab.collectionName },
      { label: 'Git', icon: <GitBranch className='h-3 w-3' /> },
    ];
  }

  if (isDiffTab(tab)) {
    return [
      { label: collectionBasename(tab.diffState.collectionPath) },
      { label: 'Git', icon: <GitBranch className='h-3 w-3' /> },
      { label: tab.diffState.filePath },
    ];
  }

  if (isConflictTab(tab)) {
    return [
      { label: collectionBasename(tab.conflictState.collectionPath) },
      { label: 'Git', icon: <GitBranch className='h-3 w-3' /> },
      { label: tab.conflictState.filePath },
    ];
  }

  if (isContractTab(tab)) {
    return [
      { label: tab.collectionName },
      { label: 'Contracts', icon: <FileLock className='h-3 w-3' /> },
    ];
  }

  return [{ label: tab.title }];
}

interface BreadcrumbBarProps {
  tab: Tab;
}

export function BreadcrumbBar({ tab }: BreadcrumbBarProps) {
  const workspaces = useWorkspaceStore((s) => s.workspaces);
  const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
  const workspaceName =
    workspaces.find((w) => w.id === activeWorkspaceId)?.name ?? 'Workspace';

  const segments = deriveSegments(tab, workspaceName);

  return (
    <div className='flex items-center h-7 px-3 gap-1 border-b border-border bg-breadcrumb-bg shrink-0 overflow-x-auto overflow-y-hidden'>
      {segments.map((seg, i) => {
        const isLast = i === segments.length - 1;
        return (
          <span key={`${seg.label}-${i}`} className='flex items-center gap-1 shrink-0'>
            {i > 0 && (
              <ChevronRight className='h-3 w-3 text-breadcrumb-fg shrink-0' aria-hidden='true' />
            )}
            {seg.icon && (
              <span className={isLast ? 'text-breadcrumb-focus-fg' : 'text-breadcrumb-fg'}>
                {seg.icon}
              </span>
            )}
            <span
              className={`text-xs ${isLast ? 'text-breadcrumb-focus-fg' : 'text-breadcrumb-fg'}`}
            >
              {seg.label}
            </span>
          </span>
        );
      })}
    </div>
  );
}
```

- [ ] **Step 2: Verify TypeScript compiles cleanly**

```bash
yarn tsc --noEmit
```

Expected: no output.

- [ ] **Step 3: Commit**

```bash
git add src/components/panes/BreadcrumbBar.tsx
git commit -m "feat(panes): add BreadcrumbBar component with per-tab segment derivation"
```

---

### Task 3: Mount BreadcrumbBar in EditorGroup

**Files:**
- Modify: `src/components/panes/EditorGroup.tsx`

- [ ] **Step 1: Add the import to `EditorGroup.tsx`**

At the top of the file, after the existing local imports, add:

```tsx
import { BreadcrumbBar } from './BreadcrumbBar';
```

- [ ] **Step 2: Mount BreadcrumbBar between TabBar and content div**

Find this block in `EditorGroup.tsx`:

```tsx
      {(hasTabs || isInSplitLayout) && <TabBar node={node} onCloseTab={handleCloseTab} />}
      <div className='flex-1 overflow-hidden'>
```

Replace it with:

```tsx
      {(hasTabs || isInSplitLayout) && <TabBar node={node} onCloseTab={handleCloseTab} />}
      {activeTab && <BreadcrumbBar tab={activeTab} />}
      <div className='flex-1 overflow-hidden'>
```

- [ ] **Step 3: Verify TypeScript compiles cleanly**

```bash
yarn tsc --noEmit
```

Expected: no output.

- [ ] **Step 4: Verify Biome is clean**

```bash
yarn check 2>&1 | grep -E "BreadcrumbBar|EditorGroup"
```

Expected: no output (no errors in changed files).

- [ ] **Step 5: Commit**

```bash
git add src/components/panes/EditorGroup.tsx
git commit -m "feat(panes): mount BreadcrumbBar in EditorGroup below tab strip"
```
