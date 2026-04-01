# Git Right Panel — Back to Overview Navigation Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a contextual breadcrumb header to the GitPanel right panel so users can navigate back to the overview/landing state from any sub-view (diff, commits, stashes).

**Architecture:** The right panel `<div>` in `GitPanel.tsx` gains a conditional header row (visible when `rightPanel.kind !== 'landing'`) with a "← Overview" button that resets `rightPanel` to `{ kind: 'landing' }`. Pure layout change — no new components, no state model changes, no backend changes.

**Tech Stack:** React, TypeScript, shadcn/ui (`Button`, `Separator`), Lucide React (`ArrowLeft`)

**Spec:** `docs/superpowers/specs/2026-04-01-git-right-panel-back-navigation.md`

**Hard rules:**
- ALL UI elements must use shadcn/ui primitives
- Icons from Lucide React only

---

## Chunk 1: Right Panel Breadcrumb Header

### Task 1: Add breadcrumb header to GitPanel right panel

**Files:**
- Modify: `src/components/git/GitPanel.tsx`

- [ ] **Step 1: Read the current `GitPanel.tsx` fully**

Run: `cat src/components/git/GitPanel.tsx`

Understand:
- The existing imports (line 1–13)
- The `RightPanelView` type (lines ~22–26)
- The right panel JSX section — find the `{/* RIGHT PANEL */}` comment and the `<div className="flex-1 overflow-hidden">` that wraps the four conditional renders (`landing`, `diff`, `commits`, `stashes`)

Note the exact surrounding code so `str_replace` targets are precise.

- [ ] **Step 2: Add `ArrowLeft` to the existing Lucide import**

Find the line:
```typescript
import { Package, ChevronDown } from 'lucide-react';
```

Replace with:
```typescript
import { Package, ChevronDown, ArrowLeft } from 'lucide-react';
```

- [ ] **Step 3: Add `Separator` import**

Find the line:
```typescript
import { Collapsible, CollapsibleTrigger, CollapsibleContent } from '@/components/ui/collapsible';
```

Add below it:
```typescript
import { Separator } from '@/components/ui/separator';
```

If `Separator` is already imported elsewhere in the file, skip this step.

- [ ] **Step 4: Replace the right panel `<div>` with breadcrumb header + content wrapper**

Find the right panel section (the exact code will look like this):

```tsx
        {/* RIGHT PANEL */}
        <div className="flex-1 overflow-hidden">
          {rightPanel.kind === 'landing' && <GitLandingPanel />}
          {rightPanel.kind === 'diff' && (
            <DiffViewForFile file={rightPanel.file} collectionPath={collectionPath} />
          )}
          {rightPanel.kind === 'commits' && <GitCommitLog />}
          {rightPanel.kind === 'stashes' && (
            <ScrollArea className="h-full">
              <div className="p-4">
                <GitStashSection />
              </div>
            </ScrollArea>
          )}
        </div>
```

Replace with:

```tsx
        {/* RIGHT PANEL */}
        <div className="flex-1 overflow-hidden flex flex-col">
          {/* Breadcrumb header — visible when not on landing/overview. */}
          {rightPanel.kind !== 'landing' && (
            <div className="flex items-center gap-2 px-3 py-2 border-b border-border/70 shrink-0">
              <Button
                variant="ghost"
                size="sm"
                className="h-7 gap-1.5 text-xs"
                onClick={() => setRightPanel({ kind: 'landing' })}
              >
                <ArrowLeft className="h-3.5 w-3.5" />
                Overview
              </Button>
              <Separator orientation="vertical" className="h-4" />
              <span className="text-xs text-muted-foreground truncate">
                {rightPanel.kind === 'diff' && rightPanel.file.path}
                {rightPanel.kind === 'commits' && 'Commit History'}
                {rightPanel.kind === 'stashes' && 'Stashes'}
              </span>
            </div>
          )}

          {/* Right panel content. */}
          <div className="flex-1 overflow-hidden">
            {rightPanel.kind === 'landing' && <GitLandingPanel />}
            {rightPanel.kind === 'diff' && (
              <DiffViewForFile file={rightPanel.file} collectionPath={collectionPath} />
            )}
            {rightPanel.kind === 'commits' && <GitCommitLog />}
            {rightPanel.kind === 'stashes' && (
              <ScrollArea className="h-full">
                <div className="p-4">
                  <GitStashSection />
                </div>
              </ScrollArea>
            )}
          </div>
        </div>
```

Key changes:
1. Outer `<div>` adds `flex flex-col` to the existing `flex-1 overflow-hidden` classes
2. Conditional breadcrumb `<div>` is inserted before content (only renders when `rightPanel.kind !== 'landing'`)
3. Existing content is wrapped in a new inner `<div className="flex-1 overflow-hidden">` to take remaining vertical space

- [ ] **Step 5: Verify TypeScript compiles**

Run: `yarn tsc --noEmit`
Expected: no type errors. The only new symbols are `ArrowLeft` (Lucide — already in the project) and `Separator` (shadcn/ui — already used in `GitFileList.tsx`).

- [ ] **Step 6: Verify the app builds**

Run: `yarn build`
Expected: build succeeds with no errors

- [ ] **Step 7: Manual smoke test**

Verify these interactions in the running app:

1. Open a collection's Git tab → right panel shows `GitLandingPanel` (Fetch/Pull/Push) → **no breadcrumb header visible** ✓
2. Click a changed file in the left panel → right panel shows diff → **breadcrumb header appears**: "← Overview | collections/path/to/file.yml" ✓
3. Click "← Overview" button → right panel returns to `GitLandingPanel` → **breadcrumb header disappears** ✓
4. Click "Commits" in Links section → right panel shows commit log → **breadcrumb header shows**: "← Overview | Commit History" ✓
5. Click "← Overview" → back to landing ✓
6. Click "Stashes" in Links section → **breadcrumb header shows**: "← Overview | Stashes" ✓
7. Click "← Overview" → back to landing ✓
8. While viewing a diff, click a different file in the left panel → breadcrumb updates to new file path ✓

- [ ] **Step 8: Commit**

```bash
git add src/components/git/GitPanel.tsx
git commit -m "feat(frontend): add back-to-overview breadcrumb header in git right panel"
```

---

## Milestone Checklist

- [ ] `ArrowLeft` and `Separator` imports added
- [ ] Breadcrumb header renders only when `rightPanel.kind !== 'landing'`
- [ ] "← Overview" button resets right panel to landing state
- [ ] Context label shows file path for diffs, "Commit History" for commits, "Stashes" for stashes
- [ ] No layout regression — landing panel still centered, diff still fills space
- [ ] TypeScript compiles, app builds
