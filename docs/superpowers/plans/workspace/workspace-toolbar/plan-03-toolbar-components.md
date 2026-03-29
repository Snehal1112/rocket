# Workspace Toolbar Plan 3: WorkspaceToolbar, CollectionDropdown & SandboxPopover

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the three new toolbar components — the main `WorkspaceToolbar` container, the `CollectionDropdown` with workspace/collection sections, and the `SandboxPopover` for Safe/Developer mode toggle.

**Architecture:** All components use shadcn/ui primitives (Popover, Button, RadioGroup). `CollectionDropdown` reads from `usePaneStore` for tab counts and `listCollections` Tauri API for the collection list. `SandboxPopover` reads/writes `useSandboxStore`. `WorkspaceToolbar` composes all toolbar items in a flex container.

**Tech Stack:** TypeScript, React, shadcn/ui, Zustand, Tauri API

**Spec:** `workspace-toolbar-design.md`

---

## Task 1: Build CollectionDropdown

**Files:**
- Create: `src/components/layout/CollectionDropdown.tsx`

**Depends on:** Plan 1 Task 3 (pane store has `activeCollection`, `switchCollection`, `getOpenTabCount`)

- [ ] **Step 1: Create CollectionDropdown component**

Create `src/components/layout/CollectionDropdown.tsx`:

```typescript
import { useState, useEffect, useCallback } from 'react';
import { ChevronDown } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Separator } from '@/components/ui/separator';
import { cn } from '@/lib/utils';
import { listCollections, type CollectionSummary } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import { useGitStore } from '@/stores/git-store';

export function CollectionDropdown() {
  const [open, setOpen] = useState(false);
  const [summaries, setSummaries] = useState<CollectionSummary[]>([]);

  const activeCollection = usePaneStore((s) => s.activeCollection);
  const switchCollection = usePaneStore((s) => s.switchCollection);
  const getOpenTabCount = usePaneStore((s) => s.getOpenTabCount);

  const activeWorkspace = useWorkspaceStore((s) => {
    const ws = s.workspaces.find((w) => w.id === s.activeWorkspaceId);
    return ws?.name ?? 'Untitled Workspace';
  });

  const fetchCollections = useCallback(async () => {
    try {
      const results = await listCollections();
      setSummaries(results);
    } catch (err) {
      console.error('[CollectionDropdown] list error', err);
    }
  }, []);

  useEffect(() => {
    if (open) void fetchCollections();
  }, [open, fetchCollections]);

  const handleSelect = (summary: CollectionSummary) => {
    switchCollection(summary.name);
    useGitStore.getState().setCollection(summary.path);
    setOpen(false);
  };

  const activeTabCount = activeCollection ? getOpenTabCount(activeCollection) : 0;

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          size="sm"
          className="h-7 gap-1.5 px-2.5 text-xs font-medium"
        >
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" className="shrink-0">
            <circle cx="8" cy="8" r="6" stroke="currentColor" strokeWidth="1.2" />
            <path d="M8 4v4l3 1.5" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
          </svg>
          <span className="max-w-[120px] truncate">{activeCollection ?? 'Select collection'}</span>
          {activeCollection && (
            <span className="text-muted-foreground">{activeTabCount}</span>
          )}
          <ChevronDown className="h-3 w-3 text-muted-foreground" />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-72 p-0" align="start">
        {/* Workspace section */}
        <div className="px-3 py-2.5 border-b border-border">
          <p className="text-[10px] text-muted-foreground uppercase tracking-wider font-medium mb-1.5">
            Workspace
          </p>
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <svg width="14" height="14" viewBox="0 0 16 16" fill="none" className="text-muted-foreground">
                <rect x="1" y="3" width="14" height="10" rx="1.5" stroke="currentColor" strokeWidth="1.2" />
                <path d="M4 3V2a1 1 0 011-1h6a1 1 0 011 1v1" stroke="currentColor" strokeWidth="1.2" />
              </svg>
              <span className="text-sm font-medium">{activeWorkspace}</span>
            </div>
            <span className="text-xs text-muted-foreground">{summaries.length}</span>
          </div>
        </div>

        {/* Collections section */}
        <div className="py-1">
          <p className="text-[10px] text-muted-foreground uppercase tracking-wider font-medium px-3 py-1.5">
            Collections
          </p>
          {summaries.length === 0 ? (
            <p className="text-xs text-muted-foreground px-3 py-2">No collections</p>
          ) : (
            summaries.map((summary) => {
              const isActive = summary.name === activeCollection;
              const tabCount = getOpenTabCount(summary.name);
              return (
                <button
                  key={summary.name}
                  type="button"
                  onClick={() => handleSelect(summary)}
                  className={cn(
                    'w-full flex items-center justify-between px-3 py-1.5 text-sm hover:bg-accent/50 transition-colors',
                    isActive && 'bg-accent',
                  )}
                >
                  <div className="flex items-center gap-2 min-w-0">
                    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" className={cn('shrink-0', isActive ? 'text-primary' : 'text-muted-foreground')}>
                      <circle cx="8" cy="8" r="5" stroke="currentColor" strokeWidth="1.2" />
                      <circle cx="8" cy="8" r="2" fill="currentColor" />
                    </svg>
                    <span className={cn('truncate', isActive && 'font-medium')}>{summary.name}</span>
                  </div>
                  {tabCount > 0 ? (
                    <span className={cn(
                      'text-xs px-1.5 rounded-full min-w-[20px] text-center',
                      isActive
                        ? 'bg-primary text-primary-foreground font-semibold'
                        : 'text-muted-foreground',
                    )}>
                      {tabCount}
                    </span>
                  ) : (
                    <span className="text-xs text-muted-foreground">{tabCount}</span>
                  )}
                </button>
              );
            })
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
}
```

- [ ] **Step 2: Verify it compiles**

Run: `npx tsc --noEmit`
Expected: No type errors

- [ ] **Step 3: Commit**

```bash
git add src/components/layout/CollectionDropdown.tsx
git commit -m "feat: add CollectionDropdown component with workspace/collection sections"
```

---

## Task 2: Build SandboxPopover

**Files:**
- Create: `src/components/layout/SandboxPopover.tsx`

**Depends on:** Plan 1 Task 2 (sandbox store exists)

- [ ] **Step 1: Create SandboxPopover component**

Create `src/components/layout/SandboxPopover.tsx`:

```typescript
import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { cn } from '@/lib/utils';
import { useSandboxStore } from '@/stores/sandbox-store';
import { ShieldCheck, Code } from 'lucide-react';

export function SandboxPopover() {
  const mode = useSandboxStore((s) => s.mode);
  const setMode = useSandboxStore((s) => s.setMode);

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7"
          title="JavaScript Sandbox"
        >
          <ShieldCheck className={cn('h-4 w-4', mode === 'safe' ? 'text-green-500' : 'text-amber-500')} />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-80 p-3" align="end">
        <p className="text-sm font-semibold mb-3">JavaScript Sandbox</p>

        {/* Safe Mode option */}
        <button
          type="button"
          onClick={() => setMode('safe')}
          className={cn(
            'w-full rounded-lg border p-3 text-left transition-colors mb-2',
            mode === 'safe'
              ? 'border-green-500 bg-green-500/5'
              : 'border-border hover:border-green-500/50',
          )}
        >
          <div className="flex items-center gap-2 mb-1">
            <div className={cn(
              'w-4 h-4 rounded-full border-2 flex items-center justify-center',
              mode === 'safe' ? 'border-green-500' : 'border-muted-foreground/40',
            )}>
              {mode === 'safe' && <div className="w-2 h-2 rounded-full bg-green-500" />}
            </div>
            <ShieldCheck className="h-4 w-4 text-green-500" />
            <span className="text-sm font-medium">Safe Mode</span>
            <span className="text-[10px] font-medium text-green-600 bg-green-500/10 px-1.5 py-0.5 rounded">
              Recommended
            </span>
          </div>
          <p className="text-xs text-muted-foreground pl-6">
            JavaScript code is executed in a secure sandbox and cannot access your filesystem or execute system commands.
          </p>
        </button>

        {/* Developer Mode option */}
        <button
          type="button"
          onClick={() => setMode('developer')}
          className={cn(
            'w-full rounded-lg border p-3 text-left transition-colors',
            mode === 'developer'
              ? 'border-amber-500 bg-amber-500/5'
              : 'border-border hover:border-amber-500/50',
          )}
        >
          <div className="flex items-center gap-2 mb-1">
            <div className={cn(
              'w-4 h-4 rounded-full border-2 flex items-center justify-center',
              mode === 'developer' ? 'border-amber-500' : 'border-muted-foreground/40',
            )}>
              {mode === 'developer' && <div className="w-2 h-2 rounded-full bg-amber-500" />}
            </div>
            <Code className="h-4 w-4 text-amber-500" />
            <span className="text-sm font-medium">Developer Mode</span>
          </div>
          <p className="text-[10px] text-amber-600 bg-amber-500/10 px-1.5 py-0.5 rounded inline-block mb-1 ml-6">
            Use only if you trust the authors of the collection
          </p>
          <p className="text-xs text-muted-foreground pl-6">
            JavaScript code has access to the filesystem, can execute system commands and access sensitive information.
          </p>
        </button>
      </PopoverContent>
    </Popover>
  );
}
```

- [ ] **Step 2: Verify it compiles**

Run: `npx tsc --noEmit`
Expected: No type errors

- [ ] **Step 3: Commit**

```bash
git add src/components/layout/SandboxPopover.tsx
git commit -m "feat: add SandboxPopover with safe/developer mode toggle"
```

---

## Task 3: Build WorkspaceToolbar and wire it into the layout

**Files:**
- Create: `src/components/layout/WorkspaceToolbar.tsx`
- Modify: Main layout file (wherever `TitleBar` and the pane area are rendered — likely `src/App.tsx` or a layout wrapper)

- [ ] **Step 1: Create GitToolbarButton**

This is small enough to inline in WorkspaceToolbar, but we create it as a sub-component for clarity:

Create `src/components/layout/GitToolbarButton.tsx`:

```typescript
import { GitBranch } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { usePaneStore } from '@/stores/pane-store';
import { useGitStore } from '@/stores/git-store';
import type { GitTab } from '@/types/pane-types';

export function GitToolbarButton() {
  const activeCollection = usePaneStore((s) => s.activeCollection);
  const openTab = usePaneStore((s) => s.openTab);
  const collectionPath = useGitStore((s) => s.collectionPath);

  const handleClick = () => {
    if (!activeCollection) return;
    const tab: GitTab = {
      id: `git:${activeCollection}`,
      title: 'Git',
      tabType: 'git',
      collectionName: activeCollection,
      collectionPath: collectionPath ?? '',
      isDirty: false,
    };
    openTab(tab);
  };

  return (
    <Button
      variant="ghost"
      size="icon"
      className="h-7 w-7"
      onClick={handleClick}
      disabled={!activeCollection}
      title="Open Git panel"
    >
      <GitBranch className="h-4 w-4" />
    </Button>
  );
}
```

- [ ] **Step 2: Create WorkspaceToolbar**

Create `src/components/layout/WorkspaceToolbar.tsx`:

```typescript
import { CollectionDropdown } from './CollectionDropdown';
import { GitToolbarButton } from './GitToolbarButton';
import { SandboxPopover } from './SandboxPopover';
import { EnvironmentSwitcher } from './EnvironmentSwitcher';

export function WorkspaceToolbar() {
  return (
    <div className="h-9 border-b border-border/70 bg-card/85 backdrop-blur-sm px-3 flex items-center justify-between shrink-0">
      {/* Left side */}
      <div className="flex items-center gap-2">
        <CollectionDropdown />
      </div>

      {/* Right side */}
      <div className="flex items-center gap-1">
        <GitToolbarButton />
        <SandboxPopover />
        <EnvironmentSwitcher />
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Wire WorkspaceToolbar into the main layout**

Find the main layout file where `<TitleBar />` is rendered and add `<WorkspaceToolbar />` right after it and before the pane/content area.

The structure should be:
```typescript
import { WorkspaceToolbar } from '@/components/layout/WorkspaceToolbar';

// In the JSX:
<div className="flex flex-col h-screen">
  <TitleBar />
  <WorkspaceToolbar />
  {/* ... sidebar + pane area ... */}
  <StatusBar />
</div>
```

- [ ] **Step 4: Verify the app compiles**

Run: `npx tsc --noEmit`
Expected: No type errors

- [ ] **Step 5: Commit**

```bash
git add src/components/layout/WorkspaceToolbar.tsx src/components/layout/GitToolbarButton.tsx
git add <main-layout-file>
git commit -m "feat: add WorkspaceToolbar with collection dropdown, git, sandbox, and environment"
```
