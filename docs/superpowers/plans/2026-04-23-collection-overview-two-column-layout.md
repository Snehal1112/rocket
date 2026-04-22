# Collection Overview Two-Column Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor the Collection Overview tab's `overview` section into a two-column layout — left column with existing content cards (MethodBreakdown, Default Headers, Requests), right column with a persistent Documentation panel that reads/writes the collection's `readme` field, identical in style to the WorkspaceOverviewTab's Documentation card.

**Architecture:** The only file changed is `src/components/collections/CollectionOverviewTab.tsx`. No new components are created — all building blocks (MethodBreakdown, HeadersEditor, RequestList, Card, ScrollArea, Tabs, ReactMarkdown) already exist. The `overview` section's outer `<ScrollArea>` is replaced with a `<div className='flex h-full overflow-hidden'>` containing two child columns; the Documentation panel introduces its own `useSaveButton` hook instance (`saveDocState`/`triggerSaveDoc`) wired to `readme` state so it saves independently from the rest of the form. The `description` textarea is removed from the overview section (but `description` stays in the save payload to avoid breaking the backend).

**Tech Stack:** React 18, TypeScript, shadcn/ui (Card, CardHeader, CardContent, ScrollArea, Tabs, TabsList, TabsTrigger, Button), lucide-react (FileText, Check, Loader2, Save), ReactMarkdown + remark-gfm (already installed), `useSaveButton` hook, Tailwind CSS.

---

## File Map

| File | Change |
|---|---|
| `src/components/collections/CollectionOverviewTab.tsx` | Only file modified — overview section gets two-column layout + Documentation panel |

No backend changes — `readme` field is already in `CollectionSettings`, persisted by `save_collection_settings`, and loaded/saved by the existing `saveSettings` callback.

---

### Task 1: Add Documentation panel state and save wiring

**Files:**
- Modify: `src/components/collections/CollectionOverviewTab.tsx:1-43` (imports)
- Modify: `src/components/collections/CollectionOverviewTab.tsx:207-221` (state declarations)
- Modify: `src/components/collections/CollectionOverviewTab.tsx:318-321` (save hook)

**Context:** The component already has `readme` state and loads/saves it via `saveSettings`. We need to add a *second*, independent save hook (`saveDocState` / `triggerSaveDoc`) that saves only the `readme` field, plus `docMode` state for the Edit/Preview toggle, and `isDocDirty` for enabling the save button. The existing `saveSettings` continues to save all fields (including `readme`) when the user hits the global Save button on other tabs — no change needed there.

- [ ] **Step 1: Add imports for Documentation panel**

  Open `src/components/collections/CollectionOverviewTab.tsx`. The current import block at lines 1–43 imports `BoxIcon, Check, Loader2, Save` from lucide-react and `Card, CardContent` from ui/card. Add `FileText` to the lucide-react import and `CardHeader` to the ui/card import, and add `ReactMarkdown` and `remarkGfm` and the `Tabs` family.

  Replace the import block at the top of the file with:

  ```tsx
  import { BoxIcon, Check, FileText, Loader2, Save } from 'lucide-react';
  import { useCallback, useEffect, useRef, useState } from 'react';
  import ReactMarkdown from 'react-markdown';
  import remarkGfm from 'remark-gfm';
  import { MarkdownEditor } from '@/components/collections/MarkdownEditor';
  import { TagsList } from '@/components/collections/TagsList';
  import { AuthEditor } from '@/components/request/AuthEditor';
  import { HeadersEditor } from '@/components/request/HeadersEditor';
  import { Button } from '@/components/ui/button';
  import { Card, CardContent, CardHeader } from '@/components/ui/card';
  import { ScrollArea } from '@/components/ui/scroll-area';
  import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
  } from '@/components/ui/select';
  import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
  import { useSaveButton } from '@/hooks/use-save-button';
  import {
    type ApiOAuth2Auth,
    apiAuthToOAuth2State,
    oauth2StateToApiAuth,
  } from '@/lib/oauth2-mapping';
  import type { Auth } from '@/lib/tauri-api';
  import {
    type Collection,
    type CollectionItem,
    type CollectionVariable,
    getCollection,
    saveCollectionSettings,
  } from '@/lib/tauri-api';
  import { cn } from '@/lib/utils';
  import { useCollectionAuthStore } from '@/stores/collection-auth-store';
  import { usePaneStore } from '@/stores/pane-store';
  import type {
    AuthState,
    CollectionSection,
    CollectionTab,
    KeyValueEntry,
  } from '@/types/pane-types';
  import { CollectionVariablesEditor } from './CollectionVariablesEditor';
  import { MethodBreakdown } from './MethodBreakdown';
  import { RequestList } from './RequestList';
  ```

- [ ] **Step 2: Add docMode and isDocDirty state**

  After the existing `const [readme, setReadme] = useState('');` line (currently around line 211), add two new state declarations. These sit alongside the other editable-settings state:

  ```tsx
  const [docMode, setDocMode] = useState<'edit' | 'preview'>('preview');
  ```

  `isDocDirty` is a derived value (not useState) — it compares live `readme` state to the persisted value on disk. We get the persisted value from `collection.settings.readme`. Add this computed variable inside the component body, after the `const statsLine` line (near line 401, after the early-return guards):

  ```tsx
  const persistedReadme = collection.settings.readme ?? '';
  const isDocDirty = readme !== persistedReadme;
  ```

- [ ] **Step 3: Add dedicated doc save function and hook**

  After the existing `const { state: saveState, trigger: triggerSave } = useSaveButton(...)` block (around line 318), add a second save wiring for the Documentation panel. This saves only `readme` independently without touching other fields:

  ```tsx
  const saveDocFn = useCallback(async () => {
    await saveCollectionSettings(collectionName, {
      readme: readme.trim() || undefined,
    });
  }, [collectionName, readme]);

  const { state: saveDocState, trigger: triggerSaveDoc } = useSaveButton(
    saveDocFn,
    'Failed to save documentation',
  );
  ```

- [ ] **Step 4: Verify TypeScript compiles**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn tsc --noEmit 2>&1 | head -30
  ```

  Expected: no errors related to the new imports or state.

- [ ] **Step 5: Commit**

  ```bash
  git add src/components/collections/CollectionOverviewTab.tsx
  git commit -m "feat(collection-overview): add doc panel state, docMode, isDocDirty, saveDocFn"
  ```

---

### Task 2: Refactor the overview section to two-column layout

**Files:**
- Modify: `src/components/collections/CollectionOverviewTab.tsx:437-515` (the overview section JSX)

**Context:** Currently the entire tab content lives inside a single `<ScrollArea>` with a `max-w-3xl mx-auto` constraint. We replace the `overview` section content with a two-column flex layout. The outer `<div ref={scrollContainerRef} className='relative flex-1 min-h-0'>` and scroll elevation overlay stay exactly as-is — only what's inside the `<ScrollArea>` for the `overview` section changes.

The left column is `flex-1 border-r border-border overflow-hidden flex flex-col` containing a `<ScrollArea className='h-full'>` that scrolls the three cards. The right column is `w-80 flex flex-col p-4` containing the Documentation Card. The `overview` section's outer wrapper changes from `<ScrollArea>` to `<div className='flex h-full overflow-hidden'>` — meaning the scroll elevation overlay needs its viewport query updated (see Task 3).

- [ ] **Step 1: Replace the overview section JSX**

  Find the block starting at `{activeSection === 'overview' && (` (around line 451). Replace the entire `<>…</>` content of the `overview` section (from the opening `<>` through the closing `</>` before `{/* Authorization tab. */}`) with the new two-column layout.

  The new overview section (replace everything between `{activeSection === 'overview' && (` and the matching `)}` that closes it):

  ```tsx
  {activeSection === 'overview' && (
    <div className='flex h-full overflow-hidden'>
      {/* LEFT — scrollable cards */}
      <div className='flex-1 border-r border-border overflow-hidden flex flex-col'>
        <ScrollArea className='h-full'>
          <div className='p-5 flex flex-col gap-5'>
            {/* Method Breakdown */}
            <MethodBreakdown items={items} />

            {/* Default Headers */}
            <Card>
              <CardHeader className='pb-2 pt-4 px-4'>
                <span className='text-sm font-medium'>Default Headers</span>
              </CardHeader>
              <CardContent className='px-4 pb-4'>
                <HeadersEditor
                  headers={headers}
                  onChange={(v) => {
                    setHeaders(v);
                    setIsDirty(true);
                  }}
                />
                <div className='flex justify-end mt-3'>
                  <Button
                    size='sm'
                    onClick={() => void triggerSave()}
                    disabled={!isDirty || saveState !== 'idle'}
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
              </CardContent>
            </Card>

            {/* Requests */}
            <Card>
              <CardHeader className='pb-2 pt-4 px-4'>
                <span className='text-sm font-medium'>Requests</span>
              </CardHeader>
              <CardContent className='px-4 pb-4'>
                <RequestList items={items} collectionName={collectionName} />
              </CardContent>
            </Card>
          </div>
        </ScrollArea>
      </div>

      {/* RIGHT — Documentation panel */}
      <div className='w-80 flex-shrink-0 flex flex-col p-4'>
        <Card className='flex-1 flex flex-col overflow-hidden'>
          <CardHeader className='flex flex-row items-center justify-between py-2.5 px-4 shrink-0'>
            <div className='flex items-center gap-2'>
              <FileText className='h-3.5 w-3.5 text-muted-foreground' />
              <span className='text-xs font-semibold text-muted-foreground'>Documentation</span>
            </div>
            <Tabs value={docMode} onValueChange={(v) => setDocMode(v as 'edit' | 'preview')}>
              <TabsList className='h-6'>
                <TabsTrigger value='edit' className='text-[10px] px-2.5 py-0.5'>
                  Edit
                </TabsTrigger>
                <TabsTrigger value='preview' className='text-[10px] px-2.5 py-0.5'>
                  Preview
                </TabsTrigger>
              </TabsList>
            </Tabs>
          </CardHeader>
          <CardContent className='flex-1 p-0 overflow-hidden flex flex-col'>
            {docMode === 'edit' && (
              <div className='flex-1 flex flex-col overflow-hidden'>
                <textarea
                  className='flex-1 w-full bg-transparent border-none resize-none px-4 py-3.5 text-xs font-mono text-muted-foreground placeholder:text-muted-foreground/40 focus-visible:outline-none leading-relaxed'
                  placeholder={'Add documentation...\n\nSupports **Markdown**'}
                  value={readme}
                  onChange={(e) => setReadme(e.target.value)}
                  onBlur={() => {
                    if (isDocDirty) void triggerSaveDoc();
                  }}
                />
                <div className='flex justify-end items-center gap-2 px-3 py-2 border-t border-border shrink-0'>
                  <span className='text-[10px] text-muted-foreground/50'>
                    Markdown supported · saves on blur
                  </span>
                  <Button
                    size='sm'
                    className={cn(
                      'h-6 text-[10px] px-3 gap-1',
                      saveDocState === 'success' && 'text-green-600',
                    )}
                    onClick={() => void triggerSaveDoc()}
                    disabled={!isDocDirty || saveDocState !== 'idle'}
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
                </div>
              </div>
            )}
            {docMode === 'preview' && (
              <div className='flex-1 overflow-y-auto px-4 py-3.5'>
                {readme.trim() ? (
                  <div className='prose-doc text-xs leading-relaxed'>
                    <ReactMarkdown remarkPlugins={[remarkGfm]}>{readme}</ReactMarkdown>
                  </div>
                ) : (
                  <div className='h-full flex flex-col items-center justify-center gap-3 text-center py-8'>
                    <FileText className='w-9 h-9 text-muted-foreground/50' />
                    <div className='space-y-1.5'>
                      <p className='text-sm font-medium text-foreground'>No documentation yet</p>
                      <p className='text-xs font-medium text-muted-foreground leading-relaxed'>
                        Add an overview, setup instructions, or key workflows.
                      </p>
                    </div>
                    <Button
                      variant='outline'
                      size='sm'
                      className='text-xs h-7'
                      onClick={() => setDocMode('edit')}
                    >
                      <FileText className='h-3 w-3 mr-1.5' />
                      Add Documentation
                    </Button>
                  </div>
                )}
              </div>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  )}
  ```

- [ ] **Step 2: Remove the now-outer ScrollArea and p-6 wrapper for the overview tab**

  Currently the entire section tab content is wrapped in:
  ```tsx
  <ScrollArea className='h-full'>
    <div className='p-6 max-w-3xl mx-auto space-y-6'>
      {/* Overview tab */}
      {activeSection === 'overview' && ( ... )}
      {/* Authorization tab */}
      ...
    </div>
  </ScrollArea>
  ```

  The overview section now manages its own scroll internally. But the other tabs (auth, variables, readme, tags) still need the outer `ScrollArea`. Change the outer wrapper so it only applies the `ScrollArea` / padding wrapper when NOT in overview mode:

  Replace:
  ```tsx
  <ScrollArea className='h-full'>
    <div className='p-6 max-w-3xl mx-auto space-y-6'>
  ```
  with:
  ```tsx
  {activeSection !== 'overview' && (
  <ScrollArea className='h-full'>
    <div className='p-6 max-w-3xl mx-auto space-y-6'>
  ```

  And replace the two closing tags:
  ```tsx
    </div>
  </ScrollArea>
  ```
  with:
  ```tsx
      </div>
    </ScrollArea>
  )}
  ```

  The overview section itself (already a `<div className='flex h-full overflow-hidden'>`) is placed at the same level — a sibling of the conditional `ScrollArea`. The full shape of the tab content area becomes:

  ```tsx
  <div ref={scrollContainerRef} className='relative flex-1 min-h-0'>
    {/* scroll elevation overlay */}
    <div className={cn(...)} />

    {/* Overview tab — manages own layout */}
    {activeSection === 'overview' && (
      <div className='flex h-full overflow-hidden'>
        ...
      </div>
    )}

    {/* All other tabs — shared ScrollArea */}
    {activeSection !== 'overview' && (
      <ScrollArea className='h-full'>
        <div className='p-6 max-w-3xl mx-auto space-y-6'>
          {activeSection === 'auth' && ( ... )}
          {activeSection === 'variables' && ( ... )}
          {activeSection === 'readme' && ( ... )}
          {activeSection === 'tags' && ( ... )}
        </div>
      </ScrollArea>
    )}
  </div>
  ```

- [ ] **Step 3: Update scroll elevation viewport query for the overview tab**

  The current scroll listener (around line 231) targets `[data-radix-scroll-area-viewport]` inside `scrollContainerRef`. Now the overview tab uses a *nested* `ScrollArea` for the left column rather than the top-level one. The scroll elevation effect only makes sense for non-overview tabs (where the outer ScrollArea still exists). Update `handleSectionChange` to reset `isScrolled` on every section change (it already does this), and update the scroll listener `useEffect` to tolerate the case where no viewport is found (when on overview, the outer ScrollArea is gone):

  The existing `useEffect` at lines ~231–241 already handles the no-viewport case gracefully (early returns on `null`). No change needed — it will simply find no viewport when on the overview tab and `isScrolled` will stay `false` (reset by `handleSectionChange`). Verify this is the case by reading the effect:

  ```tsx
  useEffect(() => {
    const container = scrollContainerRef.current;
    if (!container) return;
    const viewport = container.querySelector<HTMLElement>(
      '[data-radix-scroll-area-viewport]',
    );
    if (!viewport) return;  // ← this early return is fine; overview has no outer viewport
    const handleScroll = () => setIsScrolled(viewport.scrollTop > 0);
    viewport.addEventListener('scroll', handleScroll, { passive: true });
    return () => viewport.removeEventListener('scroll', handleScroll);
  }, []);
  ```

  No code change needed here. Just confirm it reads exactly as above.

- [ ] **Step 4: Verify TypeScript compiles clean**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn tsc --noEmit 2>&1 | head -40
  ```

  Expected: 0 errors.

- [ ] **Step 5: Run linter**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn check 2>&1 | grep -E "error|warn" | head -20
  ```

  Expected: 0 errors in `CollectionOverviewTab.tsx`.

- [ ] **Step 6: Commit**

  ```bash
  git add src/components/collections/CollectionOverviewTab.tsx
  git commit -m "feat(collection-overview): two-column overview layout with Documentation panel"
  ```

---

### Task 3: Wire docMode reset on collection change and sync readme on load

**Files:**
- Modify: `src/components/collections/CollectionOverviewTab.tsx` — load effect and handleSectionChange

**Context:** When the user switches to a different collection tab (collectionName changes), `docMode` should reset to `'preview'` so the new collection opens in preview mode rather than inheriting edit mode from the previous tab. Also, `persistedReadme` (used by `isDocDirty`) must be derived from `collection.settings.readme` — which is set once the async `getCollection` resolves. Verify the load effect already sets `readme` state correctly (it does — line 282: `setReadme(s.readme ?? '')`). No extra change needed there.

- [ ] **Step 1: Reset docMode when collectionName changes**

  Find the existing load `useEffect` that starts with:
  ```tsx
  useEffect(() => {
    setLoading(true);
    setIsLoaded(false);
    setError(null);
    getCollection(collectionName)
  ```

  Add `setDocMode('preview');` immediately after `setLoading(true);`:

  ```tsx
  useEffect(() => {
    setLoading(true);
    setDocMode('preview');
    setIsLoaded(false);
    setError(null);
    getCollection(collectionName)
      .then((col) => {
        ...
  ```

- [ ] **Step 2: Verify TypeScript compiles clean**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn tsc --noEmit 2>&1 | head -20
  ```

  Expected: 0 errors.

- [ ] **Step 3: Commit**

  ```bash
  git add src/components/collections/CollectionOverviewTab.tsx
  git commit -m "fix(collection-overview): reset docMode to preview on collection change"
  ```

---

### Task 4: Manual smoke test

**Files:** None — validation only.

**Context:** This is a pure frontend change with no unit-testable logic (it's layout JSX). The test is visual + behavioral. Start the dev server and verify each acceptance criterion.

- [ ] **Step 1: Start dev server**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn dev
  ```

  Open `http://localhost:1420` in a browser.

- [ ] **Step 2: Check two-column layout renders**

  Open any collection → Overview tab.

  Expected:
  - Left column takes available width (flex-1), shows MethodBreakdown card, Default Headers card (with checkbox rows, key/value inputs, Add Header button, Save button), Requests card (with search input and request rows).
  - Right column is 320px wide, shows the Documentation card with Edit/Preview tabs in the header.
  - A `border-r` vertical divider separates the two columns.
  - No description textarea anywhere on the overview tab.

- [ ] **Step 3: Check Documentation panel — Edit mode**

  Click the **Edit** tab in the Documentation card.

  Expected:
  - A monospace textarea appears with placeholder "Add documentation...\n\nSupports **Markdown**"
  - Footer shows "Markdown supported · saves on blur" + Save button (disabled when no changes)
  - Type some text — Save button becomes enabled
  - Click Save or blur the textarea — button shows Loader2 → Check → "Saved" → returns to idle
  - Reload the collection — text persists (saved to `~/.rocket-api/.../<collection>/opencollection.yml`)

- [ ] **Step 4: Check Documentation panel — Preview mode**

  Switch to **Preview** tab.

  Expected:
  - If readme is empty: FileText icon + "No documentation yet" + "Add Documentation" button that switches back to Edit
  - If readme has content: rendered ReactMarkdown output with GFM support (headers, code blocks, lists render correctly)

- [ ] **Step 5: Check non-overview tabs still work**

  Click Authorization, Variables, Readme, Tags tabs.

  Expected:
  - Each tab shows its content in the full-width single-column layout (no two-column split)
  - Authorization tab saves correctly
  - Readme tab (full-page editor) still works independently from the Documentation panel

- [ ] **Step 6: Check scroll elevation on non-overview tabs**

  Switch to a tab with enough content to scroll (e.g. Authorization on a collection with OAuth2). Scroll down.

  Expected: scroll elevation gradient appears at the top of the content area.

  Switch back to Overview.

  Expected: no elevation gradient visible (isScrolled reset to false, no outer viewport).

- [ ] **Step 7: Check Default Headers save**

  Add a header in the Default Headers card on the Overview tab, click Save.

  Expected: Save button shows saving → saved → idle, and the header persists after reopening the collection.
