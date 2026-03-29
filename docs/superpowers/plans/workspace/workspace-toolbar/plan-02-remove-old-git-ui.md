# Workspace Toolbar Plan 2: Remove Old Git UI

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the VSCode-style git sidebar tab, git bottom bar from status bar, and file-level git status badges from the collection tree.

**Architecture:** Clean removal of git-related imports and components from `CollectionsSidebar`, `StatusBar`, and `RequestNode`. The sidebar becomes collections-only. The status bar keeps only the Console toggle. Git badge component is kept for reuse inside the future Git tab.

**Tech Stack:** TypeScript, React, shadcn/ui

**Spec:** `workspace-toolbar-design.md`

---

## Task 1: Remove Git tab from CollectionsSidebar

**Files:**
- Modify: `src/components/layout/CollectionsSidebar.tsx`

- [ ] **Step 1: Remove git-related imports**

In `src/components/layout/CollectionsSidebar.tsx`, remove these import lines:

```typescript
// REMOVE these lines:
import { GitSidebarPanel } from "@/components/git/GitSidebarPanel";
import { useGitStore } from "@/stores/git-store";
```

Also remove the `Badge` import if it's only used for the git changed count (check — it's imported from `@/components/ui/badge`). Keep it if used elsewhere in the file.

- [ ] **Step 2: Remove git store usage and changed count**

Remove the git store hook and changedCount variable (around lines 38-40):

```typescript
// REMOVE these lines:
const gitStatus = useGitStore((s) => s.status);
const changedCount =
  gitStatus?.files.filter((f) => f.status !== "unchanged").length ?? 0;
```

- [ ] **Step 3: Remove the "Git" TabsTrigger and TabsContent**

In the `<Tabs>` component, remove the "Git" trigger from the `<TabsList>`:

```typescript
// REMOVE this TabsTrigger:
<TabsTrigger value="git" className="flex-1 text-xs">
  Git
  {changedCount > 0 && (
    <Badge variant="secondary" className="ml-1 text-[9px] px-1 h-4">
      {changedCount}
    </Badge>
  )}
</TabsTrigger>
```

Remove the entire "Git" `TabsContent`:

```typescript
// REMOVE this TabsContent block:
<TabsContent value="git" className="flex-1 overflow-hidden mt-0">
  <GitSidebarPanel />
</TabsContent>
```

The `<TabsList>` should now only have the "Collections" trigger. Since there's only one tab, consider simplifying by removing the `<Tabs>` wrapper entirely and just rendering the collections content directly — but for minimal diff, keeping the single-tab `Tabs` is fine too.

- [ ] **Step 4: Verify the app compiles**

Run: `npx tsc --noEmit`
Expected: No type errors related to removed imports

- [ ] **Step 5: Commit**

```bash
git add src/components/layout/CollectionsSidebar.tsx
git commit -m "refactor: remove git sidebar tab from CollectionsSidebar"
```

---

## Task 2: Remove GitBottomBar and EnvironmentSwitcher from StatusBar

**Files:**
- Modify: `src/components/layout/StatusBar.tsx`

- [ ] **Step 1: Remove git and environment imports**

In `src/components/layout/StatusBar.tsx`, remove:

```typescript
// REMOVE these lines:
import { EnvironmentSwitcher } from "@/components/layout/EnvironmentSwitcher";
import { GitBottomBar } from "@/components/git/GitBottomBar";
```

- [ ] **Step 2: Remove GitBottomBar and EnvironmentSwitcher from the JSX**

The current `StatusBar` renders `GitBottomBar` and `EnvironmentSwitcher`. Remove both from the JSX. The updated component should look like:

```typescript
import { Terminal } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { useConsoleStore } from "@/stores/console-store";

interface StatusBarProps {
  isConsoleOpen?: boolean;
  onConsoleToggle?: () => void;
}

export function StatusBar({ isConsoleOpen, onConsoleToggle }: StatusBarProps) {
  const entryCount = useConsoleStore((s) => s.entries.length);

  return (
    <div className="h-7 border-t border-border/70 bg-card/85 backdrop-blur-sm px-2 flex items-center gap-1.5 shrink-0">
      <Button
        variant="ghost"
        size="sm"
        className={cn(
          "h-5 px-1.5 text-2xs gap-1",
          isConsoleOpen && "bg-accent",
        )}
        onClick={onConsoleToggle}
        aria-label="Toggle Console"
      >
        <Terminal className="h-3 w-3" />
        Console
        {entryCount > 0 && (
          <span className="text-2xs px-1 rounded-full bg-muted text-muted-foreground">
            {entryCount}
          </span>
        )}
      </Button>
    </div>
  );
}
```

- [ ] **Step 3: Verify the app compiles**

Run: `npx tsc --noEmit`
Expected: No type errors

- [ ] **Step 4: Commit**

```bash
git add src/components/layout/StatusBar.tsx
git commit -m "refactor: remove GitBottomBar and EnvironmentSwitcher from StatusBar"
```

---

## Task 3: Remove GitStatusBadge from RequestNode and CollectionNode

**Files:**
- Modify: `src/components/collections/RequestNode.tsx`
- Modify: `src/components/collections/CollectionNode.tsx`

- [ ] **Step 1: Clean up RequestNode**

In `src/components/collections/RequestNode.tsx`:

Remove these imports:
```typescript
// REMOVE:
import { useGitStore } from '@/stores/git-store';
import { GitStatusBadge } from '@/components/git/GitStatusBadge';
```

Remove the git store hook and file lookup (around lines 44-47):
```typescript
// REMOVE:
const gitStatusData = useGitStore((s) => s.status);
const gitFile = gitStatusData?.files.find((f) => f.path === path || f.path.endsWith(`/${path}`));
const gitStatus = gitFile?.status ?? 'unchanged';
```

Remove the `<GitStatusBadge>` from the JSX inside the TreeItemContent (it's rendered after the `<span>` for the name):
```typescript
// REMOVE this line inside the non-renaming branch:
<GitStatusBadge status={gitStatus} />
```

- [ ] **Step 2: Clean up CollectionNode**

In `src/components/collections/CollectionNode.tsx`:

Remove the git store import:
```typescript
// REMOVE:
import { useGitStore } from '@/stores/git-store';
```

Remove the git store call inside `handleClick` (around line 77):
```typescript
// REMOVE this line from inside the setOpen callback:
useGitStore.getState().setCollection(summary.path);
```

- [ ] **Step 3: Verify the app compiles**

Run: `npx tsc --noEmit`
Expected: No type errors

- [ ] **Step 4: Commit**

```bash
git add src/components/collections/RequestNode.tsx src/components/collections/CollectionNode.tsx
git commit -m "refactor: remove git status badges from collection tree nodes"
```
