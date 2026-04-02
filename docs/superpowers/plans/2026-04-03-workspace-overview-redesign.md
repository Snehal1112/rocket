# Workspace Overview Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign `WorkspaceOverviewTab.tsx` into a Bruno-inspired two-column layout with real environment count, plain stats, Quick Actions section, and a Documentation card with markdown edit/preview.

**Architecture:** Full replacement of a single component — `WorkspaceOverviewTab.tsx`. No new files. Left column holds stats, quick actions, and collections list. Right column holds a full-height Documentation card with a lightweight inline markdown renderer, Edit/Preview toggle (default: Preview), and auto-save on blur.

**Tech Stack:** React, TypeScript, Zustand (`useEnvStore`, `useWorkspaceStore`, `usePaneStore`), Tailwind CSS, Shadcn UI (`Card`, `CardHeader`, `CardContent`, `Button`, `ScrollArea`, `DropdownMenu`)

---

## File Map

| File | Change |
|---|---|
| `src/components/workspace/WorkspaceOverviewTab.tsx` | Full redesign — two-column layout, env count, Quick Actions, Documentation card with markdown |

---

## Task 1: Wire environment count from `useEnvStore`

**Files:**
- Modify: `src/components/workspace/WorkspaceOverviewTab.tsx`

The current `Environments` stat is hardcoded as `—`. Fix it by reading from `useEnvStore`.

- [ ] **Step 1: Add the import and store selector**

At the top of the file, after the existing store imports, add:

```typescript
import { useEnvStore } from '@/stores/env-store';
```

Then inside the component function, after the existing store selections:

```typescript
const globalEnvironments = useEnvStore((s) => s.globalEnvironments);
const loadGlobalEnvironments = useEnvStore((s) => s.loadGlobalEnvironments);
```

- [ ] **Step 2: Load environments on mount**

Inside the `useEffect` that calls `refresh()`, also call `loadGlobalEnvironments()`:

```typescript
useEffect(() => {
  refresh().catch(console.error);
  loadGlobalEnvironments().catch(console.error);
}, [refresh, loadGlobalEnvironments]);
```

- [ ] **Step 3: Use the count in the stats**

Replace the hardcoded `—` in the environments stat:

```tsx
<p className="text-2xl font-semibold">{globalEnvironments.length}</p>
```

- [ ] **Step 4: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no new errors

- [ ] **Step 5: Commit**

```bash
git add src/components/workspace/WorkspaceOverviewTab.tsx
git commit -m "fix: wire environment count from useEnvStore in workspace overview"
```

---

## Task 2: Add inline markdown renderer

**Files:**
- Modify: `src/components/workspace/WorkspaceOverviewTab.tsx`

Add a small self-contained markdown renderer. No external dependencies. HTML-escape raw input before processing to prevent XSS via `dangerouslySetInnerHTML`.

- [ ] **Step 1: Add the `renderMarkdown` function**

Add this helper above the component function in `WorkspaceOverviewTab.tsx`:

```typescript
function escapeHtml(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

function renderMarkdown(raw: string): string {
  if (!raw.trim()) return '';

  const lines = raw.split('\n');
  let html = '';
  let inList = false;
  let listType = '';

  function closeList() {
    if (inList) { html += `</${listType}>`; inList = false; listType = ''; }
  }

  function inlineFormat(line: string): string {
    // Escape HTML first, then apply safe inline markdown
    let s = escapeHtml(line);
    s = s.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
    s = s.replace(/\*(.+?)\*/g, '<em>$1</em>');
    s = s.replace(/`(.+?)`/g, '<code>$1</code>');
    // Links: [text](url) — URL already escaped by escapeHtml, restore the parens
    s = s.replace(/\[(.+?)\]\((.+?)\)/g, '<a href="$2" rel="noopener noreferrer">$1</a>');
    return s;
  }

  for (const rawLine of lines) {
    const line = rawLine;
    if (/^### /.test(line)) { closeList(); html += `<h3>${inlineFormat(line.slice(4))}</h3>`; }
    else if (/^## /.test(line)) { closeList(); html += `<h2>${inlineFormat(line.slice(3))}</h2>`; }
    else if (/^# /.test(line)) { closeList(); html += `<h1>${inlineFormat(line.slice(2))}</h1>`; }
    else if (/^---$/.test(line.trim())) { closeList(); html += '<hr />'; }
    else if (/^- /.test(line)) {
      if (!inList || listType !== 'ul') { closeList(); html += '<ul>'; inList = true; listType = 'ul'; }
      html += `<li>${inlineFormat(line.slice(2))}</li>`;
    }
    else if (/^\d+\. /.test(line)) {
      if (!inList || listType !== 'ol') { closeList(); html += '<ol>'; inList = true; listType = 'ol'; }
      html += `<li>${inlineFormat(line.replace(/^\d+\. /, ''))}</li>`;
    }
    else if (line.trim() === '') { closeList(); }
    else { closeList(); html += `<p>${inlineFormat(line)}</p>`; }
  }
  closeList();
  return html;
}
```

- [ ] **Step 2: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors

- [ ] **Step 3: Commit**

```bash
git add src/components/workspace/WorkspaceOverviewTab.tsx
git commit -m "feat: add inline markdown renderer for workspace documentation"
```

---

## Task 3: Add Documentation card state and component logic

**Files:**
- Modify: `src/components/workspace/WorkspaceOverviewTab.tsx`

Add state for the documentation panel: mode toggle (`'edit' | 'preview'`), controlled doc content, and the save handler.

- [ ] **Step 1: Add new `FileText` import from lucide-react**

Update the lucide-react import line:

```typescript
import {
  Plus, FolderOpen, Layers, MoreHorizontal, Trash2, ExternalLink, FileText,
} from 'lucide-react';
```

- [ ] **Step 2: Add Card imports from Shadcn**

After the existing Shadcn imports, add:

```typescript
import { Card, CardHeader, CardContent } from '@/components/ui/card';
```

- [ ] **Step 3: Add documentation state inside the component**

After the existing `useState` declarations, add:

```typescript
const [docMode, setDocMode] = useState<'edit' | 'preview'>('preview');
const [docContent, setDocContent] = useState<string>(workspace?.description ?? '');
```

- [ ] **Step 4: Sync `docContent` when workspace loads/changes**

Add a `useEffect` that syncs the description from the store when the workspace changes (e.g. switching workspaces):

```typescript
useEffect(() => {
  setDocContent(workspace?.description ?? '');
}, [workspace?.description]);
```

- [ ] **Step 5: Add save handler**

```typescript
function handleSaveDoc() {
  updateDescription(workspaceId, docContent.trim() || null);
}
```

- [ ] **Step 6: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors

- [ ] **Step 7: Commit**

```bash
git add src/components/workspace/WorkspaceOverviewTab.tsx
git commit -m "feat: add documentation panel state and save handler"
```

---

## Task 4: Full layout restructure — two-column with left column

**Files:**
- Modify: `src/components/workspace/WorkspaceOverviewTab.tsx`

Replace the existing single-column `<ScrollArea>` layout with the full two-column design. The outer container becomes a flex row filling the viewport height. The left column gets its own `ScrollArea`. This task replaces the entire `return` statement.

- [ ] **Step 1: Replace the full `return` of the component**

Replace everything from `return (` to the closing `);` with the following. Read each section carefully — the logic (handlers, state) from the original component is preserved exactly:

```tsx
  return (
    <div className="flex h-full overflow-hidden">

      {/* ── LEFT COLUMN ── */}
      <div className="flex-1 border-r border-border overflow-hidden flex flex-col">
        <ScrollArea className="flex-1">
          <div className="p-5 flex flex-col gap-5">

            {/* Page header */}
            <h2 className="text-base font-semibold leading-tight">
              {workspace?.name ?? 'Workspace'}
            </h2>

            {/* Stats — plain Bruno style */}
            <div className="flex gap-7 pb-4 border-b border-border">
              <div className="flex flex-col gap-0.5">
                <span className="text-[22px] font-bold leading-tight tabular-nums">{collectionCount}</span>
                <span className="text-[11px] text-muted-foreground">Collections</span>
              </div>
              <div className="flex flex-col gap-0.5">
                <span className="text-[22px] font-bold leading-tight tabular-nums">{globalEnvironments.length}</span>
                <span className="text-[11px] text-muted-foreground">Environments</span>
              </div>
            </div>

            {/* Quick Actions */}
            <div>
              <p className="text-[11px] font-medium text-muted-foreground mb-2">Quick Actions</p>
              {isCreating ? (
                <div className="flex gap-2">
                  <input
                    autoFocus
                    placeholder="Collection name"
                    value={newName}
                    onChange={(e) => setNewName(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') void handleCreateCollection();
                      if (e.key === 'Escape') { setIsCreating(false); setNewName(''); }
                    }}
                    className="flex-1 rounded-md border border-input bg-transparent px-3 py-1.5 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                  />
                  <Button size="sm" onClick={() => void handleCreateCollection()} disabled={!newName.trim()}>
                    Create
                  </Button>
                  <Button size="sm" variant="ghost" onClick={() => { setIsCreating(false); setNewName(''); }}>
                    Cancel
                  </Button>
                </div>
              ) : (
                <div className="flex flex-wrap gap-1.5">
                  <Button variant="outline" size="sm" className="text-xs h-7" onClick={() => setIsCreating(true)}>
                    <Plus className="h-3 w-3 mr-1.5" />
                    Create Collection
                  </Button>
                  <Button variant="outline" size="sm" className="text-xs h-7" onClick={handleLinkExternal}>
                    <FolderOpen className="h-3 w-3 mr-1.5" />
                    Open Collection
                  </Button>
                </div>
              )}
            </div>

            {/* Collections */}
            <div>
              <p className="text-[11px] font-medium text-muted-foreground mb-2">Collections</p>
              {summaries.length > 0 ? (
                <div>
                  {summaries.map((col) => (
                    <div
                      key={col.name}
                      onClick={() => handleOpenCollection(col.name)}
                      className="group flex items-center gap-2.5 py-2 px-1.5 -mx-1.5 rounded-md border-b border-border last:border-b-0 hover:bg-muted/50 cursor-pointer transition-colors"
                    >
                      <div className={`w-7 h-7 rounded-md flex items-center justify-center shrink-0 ${col.refType === 'external' ? 'bg-orange-950/40 border border-orange-900/40' : 'bg-primary/10 border border-primary/20'}`}>
                        <Layers className="h-3.5 w-3.5 text-primary" />
                      </div>
                      <div className="flex-1 min-w-0">
                        <span className="text-sm font-medium truncate block">{col.name}</span>
                        {col.path && (
                          <span className="text-[10px] text-muted-foreground truncate block">{col.path}</span>
                        )}
                      </div>
                      <span className="text-[10px] text-muted-foreground shrink-0">
                        {col.requestCount} req
                      </span>
                      {col.refType === 'external' && (
                        <span className="shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400">
                          external
                        </span>
                      )}
                      <DropdownMenu>
                        <DropdownMenuTrigger asChild>
                          <button
                            type="button"
                            className="h-5 w-5 flex items-center justify-center rounded-sm opacity-0 group-hover:opacity-100 hover:bg-muted text-muted-foreground"
                            onClick={(e) => e.stopPropagation()}
                          >
                            <MoreHorizontal className="h-3 w-3" />
                          </button>
                        </DropdownMenuTrigger>
                        <DropdownMenuContent onClick={(e) => e.stopPropagation()}>
                          <DropdownMenuItem onClick={() => handleOpenCollection(col.name)}>
                            <ExternalLink className="h-3.5 w-3.5 mr-2" /> Open
                          </DropdownMenuItem>
                          <DropdownMenuSeparator />
                          <DropdownMenuItem
                            className="text-destructive"
                            onClick={() => handleDeleteCollection(col.name)}
                          >
                            <Trash2 className="h-3.5 w-3.5 mr-2" /> Delete
                          </DropdownMenuItem>
                        </DropdownMenuContent>
                      </DropdownMenu>
                    </div>
                  ))}
                </div>
              ) : (
                <p className="text-sm text-muted-foreground">No collections yet.</p>
              )}
            </div>

          </div>
        </ScrollArea>
      </div>

      {/* ── RIGHT COLUMN — Documentation ── */}
      <div className="flex-1 p-4 flex flex-col overflow-hidden">
        <Card className="flex-1 flex flex-col overflow-hidden">
          <CardHeader className="flex flex-row items-center justify-between py-2.5 px-4 border-b border-border shrink-0">
            <div className="flex items-center gap-2">
              <FileText className="h-3.5 w-3.5 text-muted-foreground" />
              <span className="text-xs font-semibold text-muted-foreground">Documentation</span>
            </div>
            <div className="flex gap-0.5 bg-muted/30 border border-border rounded-md p-0.5">
              <button
                type="button"
                onClick={() => setDocMode('edit')}
                className={`px-2.5 py-0.5 rounded text-[10px] transition-colors ${docMode === 'edit' ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'}`}
              >
                Edit
              </button>
              <button
                type="button"
                onClick={() => setDocMode('preview')}
                className={`px-2.5 py-0.5 rounded text-[10px] transition-colors ${docMode === 'preview' ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'}`}
              >
                Preview
              </button>
            </div>
          </CardHeader>

          <CardContent className="flex-1 p-0 overflow-hidden flex flex-col">
            {/* Edit pane */}
            {docMode === 'edit' && (
              <div className="flex-1 flex flex-col overflow-hidden">
                <textarea
                  className="flex-1 w-full bg-transparent border-none resize-none px-4 py-3.5 text-xs font-mono text-muted-foreground placeholder:text-muted-foreground/40 focus-visible:outline-none leading-relaxed"
                  placeholder={"Add documentation...\n\nSupports **Markdown**"}
                  value={docContent}
                  onChange={(e) => setDocContent(e.target.value)}
                  onBlur={handleSaveDoc}
                />
                <div className="flex justify-end items-center gap-2 px-3 py-2 border-t border-border shrink-0">
                  <span className="text-[10px] text-muted-foreground/50">Markdown supported · saves on blur</span>
                  <Button size="sm" className="h-6 text-[10px] px-3" onClick={handleSaveDoc}>Save</Button>
                </div>
              </div>
            )}

            {/* Preview pane */}
            {docMode === 'preview' && (
              <div className="flex-1 overflow-y-auto px-4 py-3.5">
                {docContent.trim() ? (
                  <div
                    className="prose-doc text-xs leading-relaxed"
                    dangerouslySetInnerHTML={{ __html: renderMarkdown(docContent) }}
                  />
                ) : (
                  <div className="h-full flex flex-col items-center justify-center gap-3 text-center py-8">
                    <FileText className="h-9 w-9 text-muted-foreground/20" />
                    <div className="space-y-1">
                      <p className="text-xs font-medium text-muted-foreground/60">Add documentation to help your team work smoothly.</p>
                      <p className="text-[11px] text-muted-foreground/40">You can include project overview, setup instructions, key workflows, and FAQs.</p>
                    </div>
                    <Button variant="outline" size="sm" className="text-xs h-7 mt-1" onClick={() => setDocMode('edit')}>
                      + Add Documentation
                    </Button>
                  </div>
                )}
              </div>
            )}
          </CardContent>
        </Card>
      </div>

    </div>
  );
```

- [ ] **Step 2: Add prose-doc styles for markdown rendering**

The markdown preview uses `prose-doc` class. Add styles to `src/index.css` below the existing global styles (after the `:root` block and body rules):

```css
/* Workspace documentation markdown */
.prose-doc h1 { font-size: 0.9375rem; font-weight: 700; margin-bottom: 0.5rem; }
.prose-doc h2 { font-size: 0.8125rem; font-weight: 600; margin: 0.75rem 0 0.375rem; padding-bottom: 0.25rem; border-bottom: 1px solid hsl(var(--border)); }
.prose-doc h3 { font-size: 0.75rem; font-weight: 600; margin: 0.625rem 0 0.25rem; }
.prose-doc p { margin-bottom: 0.5rem; color: hsl(var(--muted-foreground)); }
.prose-doc ul, .prose-doc ol { padding-left: 1rem; margin-bottom: 0.5rem; }
.prose-doc li { line-height: 1.7; color: hsl(var(--muted-foreground)); }
.prose-doc ul li { list-style: disc; }
.prose-doc ol li { list-style: decimal; }
.prose-doc code { font-family: 'Menlo', 'Consolas', 'Monaco', monospace; font-size: 0.6875rem; background: hsl(var(--muted)); border: 1px solid hsl(var(--border)); border-radius: 3px; padding: 1px 4px; }
.prose-doc strong { font-weight: 600; }
.prose-doc hr { border: none; border-top: 1px solid hsl(var(--border)); margin: 0.75rem 0; }
.prose-doc a { color: hsl(var(--primary)); text-decoration: underline; }
```

- [ ] **Step 3: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors. Fix any type errors that appear.

- [ ] **Step 4: Lint**

```bash
yarn lint
```

Expected: no new lint errors

- [ ] **Step 5: Commit**

```bash
git add src/components/workspace/WorkspaceOverviewTab.tsx src/index.css
git commit -m "feat: two-column workspace overview with documentation card and markdown preview"
```

---

## Task 5: Verify Card component exists

**Files:**
- Check: `src/components/ui/card.tsx`

The plan uses `Card`, `CardHeader`, `CardContent` from Shadcn. Confirm the component exists before Task 4.

- [ ] **Step 1: Check for the card component**

```bash
ls src/components/ui/card.tsx
```

Expected: file exists. If it does not exist, run:

```bash
npx shadcn@latest add card
```

Then re-run Task 4.

- [ ] **Step 2: Confirm the exports**

```bash
grep "export" src/components/ui/card.tsx | head -20
```

Expected to see: `export { Card, CardHeader, CardContent, ... }`

---

## Self-Review Checklist

After writing the plan, verify:

- [x] **Environments count**: Task 1 wires `useEnvStore` → `globalEnvironments.length`
- [x] **Two-column layout**: Task 4 restructures to `flex h-full` with left/right halves
- [x] **Stats — plain style**: Stats are plain `text-[22px] font-bold` numbers with border-bottom, no cards
- [x] **Quick Actions**: `Create Collection` (inline form) and `Open Collection` (external link picker) are Bruno-style outline buttons under a "Quick Actions" label
- [x] **Collections list**: Preserved with tightened styling (border-bottom rows, smaller icon tile)
- [x] **Documentation card**: Full-height `Card`, `FileText` icon in header, Edit/Preview toggle pills
- [x] **Preview default**: `useState<'edit' | 'preview'>('preview')` sets Preview as default
- [x] **Markdown renderer**: `renderMarkdown()` with `escapeHtml()` guard — no external deps
- [x] **Empty state**: When `docContent.trim()` is falsy in preview mode, shows FileText icon + help text + "+ Add Documentation" button
- [x] **Save**: `onBlur` calls `handleSaveDoc()`, explicit Save button also calls it; syncs via `updateDescription(workspaceId, value)`
- [x] **Sync on workspace change**: `useEffect` on `workspace?.description` resets `docContent`
- [x] **No placeholders**: All steps have complete code
- [x] **Type safety**: `yarn tsc --noEmit` step after each task
- [x] **Security**: `escapeHtml()` called before markdown parsing prevents XSS via `dangerouslySetInnerHTML`

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-04-03-workspace-overview-redesign.md`.

**Two execution options:**

**1. Subagent-Driven (recommended)** — Fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints

Which approach?
