# Remove Sidebar Tabs Wrapper Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the now-redundant `<Tabs>` wrapper from `CollectionsSidebar` since the Git tab was removed and only one tab remains.

**Architecture:** Strip the `Tabs`, `TabsList`, `TabsTrigger`, and `TabsContent` components. Replace `TabsContent` with a plain `<div>` carrying the same layout classes. Remove unused imports.

**Tech Stack:** TypeScript, React, shadcn/ui

**Spec:** `2026-03-29-remove-sidebar-tabs-wrapper-design.md`

---

### Task 1: Remove Tabs wrapper from CollectionsSidebar

**Files:**
- Modify: `src/components/layout/CollectionsSidebar.tsx`

- [ ] **Step 1: Remove the Tabs import**

In `src/components/layout/CollectionsSidebar.tsx`, remove this import line (line 39):

```typescript
// REMOVE this entire line:
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
```

- [ ] **Step 2: Replace the Tabs/TabsList/TabsTrigger/TabsContent with a plain div**

In the return JSX, the current structure (starting around line 288) is:

```tsx
<div className="h-full flex flex-col bg-card/50 backdrop-blur-sm border-r border-border/50">
  <Tabs
    defaultValue="collections"
    className="flex-1 flex flex-col overflow-hidden"
  >
    <TabsList className="w-full shrink-0 rounded-none border-b border-border/50 h-9 px-2">
      <TabsTrigger value="collections" className="flex-1 text-xs">
        Collections
      </TabsTrigger>
    </TabsList>

    <TabsContent
      value="collections"
      className="flex-1 flex flex-col overflow-hidden mt-0"
    >
      {/* ... sidebar content ... */}
    </TabsContent>
  </Tabs>

  <AlertDialog ...>
```

Replace the `<Tabs>` through `</Tabs>` block with a single `<div>` that carries the combined layout classes:

```tsx
<div className="h-full flex flex-col bg-card/50 backdrop-blur-sm border-r border-border/50">
  <div className="flex-1 flex flex-col overflow-hidden">
    {/* ... sidebar content stays exactly as-is ... */}
  </div>

  <AlertDialog ...>
```

The `<div className="flex-1 flex flex-col overflow-hidden">` replaces both `<Tabs className="flex-1 flex flex-col overflow-hidden">` and `<TabsContent className="flex-1 flex flex-col overflow-hidden mt-0">`. Since the Tabs and TabsContent both contributed `flex-1 flex flex-col overflow-hidden`, a single div with those classes preserves the layout. The `mt-0` from TabsContent is not needed on a plain div.

Everything between the old `<TabsContent>` and `</TabsContent>` (the view selector, search bar, collection tree, history panel) stays untouched.

- [ ] **Step 3: Verify the app compiles**

Run: `npx tsc --noEmit`
Expected: No type errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/layout/CollectionsSidebar.tsx
git commit -m "refactor: remove redundant Tabs wrapper from CollectionsSidebar"
```
