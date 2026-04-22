# Collection Overview Two-Column Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor the Collection Overview tab's `overview` section into a two-column layout — left column with existing content cards (MethodBreakdown, Default Headers, Requests), right column with a persistent Documentation panel — while extracting the Documentation card into a shared component so `WorkspaceOverviewTab` and `CollectionOverviewTab` share the same implementation.

**Architecture:** Extract the Documentation card JSX (currently only in `WorkspaceOverviewTab`) into `src/components/ui/documentation-panel.tsx` with props for value, onChange, onSave, saveState, isDirty, mode, and onModeChange. Both overview tabs consume this component. `CollectionOverviewTab`'s `overview` section gets a second independent `useSaveButton` instance for the doc panel wired to the `readme` field (the backend already persists `readme` in `CollectionSettings`). No backend changes required.

**Tech Stack:** React 18, TypeScript, shadcn/ui (Card, CardHeader, CardContent, Tabs, TabsList, TabsTrigger, Button), lucide-react (FileText, Check, Loader2, Save), ReactMarkdown + remark-gfm, `useSaveButton` hook, Tailwind CSS.

---

## File Map

| File | Change |
|---|---|
| `src/components/ui/documentation-panel.tsx` | **Create** — shared Documentation card component |
| `src/components/workspace/WorkspaceOverviewTab.tsx` | **Modify** — replace inline Documentation card with `<DocumentationPanel>` |
| `src/components/collections/CollectionOverviewTab.tsx` | **Modify** — add doc panel state/save wiring, refactor overview section to two-column layout using `<DocumentationPanel>` |

No backend changes — `readme` field is already in `CollectionSettings`, persisted by `save_collection_settings`, and loaded/saved by the existing `saveSettings` callback in `CollectionOverviewTab`.

---

### Task 1: Create shared DocumentationPanel component

**Files:**
- Create: `src/components/ui/documentation-panel.tsx`

**Context:** The Documentation card in `WorkspaceOverviewTab` (lines 352–442) is a self-contained Card with Edit/Preview tabs, a monospace textarea that auto-saves on blur, a save button with loading/success states, and a ReactMarkdown preview pane with an empty state. All of this will be extracted verbatim and made prop-driven. The parent manages all state (`value`, `mode`, `saveState`, `isDirty`) — the component is purely presentational.

- [ ] **Step 1: Write a test for DocumentationPanel**

  Create `src/components/ui/documentation-panel.test.tsx`:

  ```tsx
  import { render, screen } from '@testing-library/react';
  import userEvent from '@testing-library/user-event';
  import { describe, expect, it, vi } from 'vitest';
  import { DocumentationPanel } from './documentation-panel';

  const baseProps = {
    value: '',
    onChange: vi.fn(),
    onSave: vi.fn(),
    saveState: 'idle' as const,
    isDirty: false,
    mode: 'preview' as const,
    onModeChange: vi.fn(),
  };

  describe('DocumentationPanel', () => {
    it('renders empty state in preview mode when value is empty', () => {
      render(<DocumentationPanel {...baseProps} />);
      expect(screen.getByText('No documentation yet')).toBeInTheDocument();
    });

    it('renders markdown content in preview mode when value is set', () => {
      render(<DocumentationPanel {...baseProps} value='# Hello' />);
      expect(screen.getByRole('heading', { name: 'Hello' })).toBeInTheDocument();
    });

    it('renders textarea in edit mode', () => {
      render(<DocumentationPanel {...baseProps} mode='edit' />);
      expect(screen.getByPlaceholderText(/Add documentation/)).toBeInTheDocument();
    });

    it('calls onModeChange when Edit tab is clicked', async () => {
      const onModeChange = vi.fn();
      render(<DocumentationPanel {...baseProps} onModeChange={onModeChange} />);
      await userEvent.click(screen.getByRole('tab', { name: 'Edit' }));
      expect(onModeChange).toHaveBeenCalledWith('edit');
    });

    it('save button is disabled when not dirty', () => {
      render(<DocumentationPanel {...baseProps} mode='edit' isDirty={false} />);
      expect(screen.getByRole('button', { name: /save/i })).toBeDisabled();
    });

    it('save button is enabled when dirty', () => {
      render(<DocumentationPanel {...baseProps} mode='edit' isDirty={true} />);
      expect(screen.getByRole('button', { name: /save/i })).toBeEnabled();
    });

    it('calls onSave when save button clicked', async () => {
      const onSave = vi.fn().mockResolvedValue(undefined);
      render(<DocumentationPanel {...baseProps} mode='edit' isDirty={true} onSave={onSave} />);
      await userEvent.click(screen.getByRole('button', { name: /save/i }));
      expect(onSave).toHaveBeenCalled();
    });

    it('clicking Add Documentation button calls onModeChange with edit', async () => {
      const onModeChange = vi.fn();
      render(<DocumentationPanel {...baseProps} onModeChange={onModeChange} />);
      await userEvent.click(screen.getByRole('button', { name: /add documentation/i }));
      expect(onModeChange).toHaveBeenCalledWith('edit');
    });
  });
  ```

- [ ] **Step 2: Run the test to confirm it fails**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn test documentation-panel 2>&1 | tail -10
  ```

  Expected: FAIL — module `./documentation-panel` not found.

- [ ] **Step 3: Create the component**

  Create `src/components/ui/documentation-panel.tsx`:

  ```tsx
  import { Check, FileText, Loader2, Save } from 'lucide-react';
  import ReactMarkdown from 'react-markdown';
  import remarkGfm from 'remark-gfm';
  import { Button } from '@/components/ui/button';
  import { Card, CardContent, CardHeader } from '@/components/ui/card';
  import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
  import type { SaveButtonState } from '@/hooks/use-save-button';
  import { cn } from '@/lib/utils';

  interface DocumentationPanelProps {
    value: string;
    onChange: (value: string) => void;
    onSave: () => void;
    saveState: SaveButtonState;
    isDirty: boolean;
    mode: 'edit' | 'preview';
    onModeChange: (mode: 'edit' | 'preview') => void;
    onBlur?: () => void;
  }

  export function DocumentationPanel({
    value,
    onChange,
    onSave,
    saveState,
    isDirty,
    mode,
    onModeChange,
    onBlur,
  }: DocumentationPanelProps) {
    return (
      <Card className='flex-1 flex flex-col overflow-hidden'>
        <CardHeader className='flex flex-row items-center justify-between py-2.5 px-4 shrink-0'>
          <div className='flex items-center gap-2'>
            <FileText className='h-3.5 w-3.5 text-muted-foreground' />
            <span className='text-xs font-semibold text-muted-foreground'>Documentation</span>
          </div>
          <Tabs value={mode} onValueChange={(v) => onModeChange(v as 'edit' | 'preview')}>
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
          {mode === 'edit' && (
            <div className='flex-1 flex flex-col overflow-hidden'>
              <textarea
                className='flex-1 w-full bg-transparent border-none resize-none px-4 py-3.5 text-xs font-mono text-muted-foreground placeholder:text-muted-foreground/40 focus-visible:outline-none leading-relaxed'
                placeholder={'Add documentation...\n\nSupports **Markdown**'}
                value={value}
                onChange={(e) => onChange(e.target.value)}
                onBlur={onBlur}
              />
              <div className='flex justify-end items-center gap-2 px-3 py-2 border-t border-border shrink-0'>
                <span className='text-[10px] text-muted-foreground/50'>
                  Markdown supported · saves on blur
                </span>
                <Button
                  size='sm'
                  className={cn(
                    'h-6 text-[10px] px-3 gap-1',
                    saveState === 'success' && 'text-green-600',
                  )}
                  onClick={onSave}
                  disabled={!isDirty || saveState !== 'idle'}
                >
                  {saveState === 'saving' ? (
                    <Loader2 className='h-3 w-3 animate-spin' />
                  ) : saveState === 'success' ? (
                    <Check className='h-3 w-3' />
                  ) : (
                    <Save className='h-3 w-3' />
                  )}
                  {saveState === 'success' ? 'Saved' : 'Save'}
                </Button>
              </div>
            </div>
          )}

          {mode === 'preview' && (
            <div className='flex-1 overflow-y-auto px-4 py-3.5'>
              {value.trim() ? (
                <div className='prose-doc text-xs leading-relaxed'>
                  <ReactMarkdown remarkPlugins={[remarkGfm]}>{value}</ReactMarkdown>
                </div>
              ) : (
                <div className='h-full flex flex-col items-center justify-center gap-3 text-center py-8'>
                  <FileText className='w-9 h-9 text-muted-foreground/50' />
                  <div className='space-y-1.5'>
                    <p className='text-sm font-medium text-foreground'>No documentation yet</p>
                    <p className='text-xs font-medium text-muted-foreground leading-relaxed'>
                      Add an overview, setup instructions, or key workflows to help your team.
                    </p>
                  </div>
                  <Button
                    variant='outline'
                    size='sm'
                    className='text-xs h-7'
                    onClick={() => onModeChange('edit')}
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
    );
  }
  ```

- [ ] **Step 4: Run the tests — all should pass**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn test documentation-panel 2>&1 | tail -15
  ```

  Expected: 8 tests pass, 0 failures.

- [ ] **Step 5: Verify TypeScript compiles**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn tsc --noEmit 2>&1 | head -20
  ```

  Expected: 0 errors.

- [ ] **Step 6: Commit**

  ```bash
  git add src/components/ui/documentation-panel.tsx src/components/ui/documentation-panel.test.tsx
  git commit -m "feat(ui): extract DocumentationPanel shared component"
  ```

---

### Task 2: Replace inline Documentation card in WorkspaceOverviewTab with DocumentationPanel

**Files:**
- Modify: `src/components/workspace/WorkspaceOverviewTab.tsx`

**Context:** `WorkspaceOverviewTab` currently has ~90 lines of Documentation card JSX (lines 352–442). We replace that entire block with `<DocumentationPanel>` passing the existing local state. The component already has `docMode`, `docContent`, `isDocDirty`, `saveDocState`, `triggerSaveDoc` — all map directly to props. The outer `<div className='flex-1 p-4 flex flex-col overflow-hidden'>` wrapper stays.

- [ ] **Step 1: Add DocumentationPanel import**

  In `src/components/workspace/WorkspaceOverviewTab.tsx`, add to the import block:

  ```tsx
  import { DocumentationPanel } from '@/components/ui/documentation-panel';
  ```

  Remove these imports that are no longer needed directly in this file (they are now used inside DocumentationPanel):
  - `FileText` from lucide-react
  - `ReactMarkdown` from react-markdown
  - `remarkGfm` from remark-gfm
  - `CardHeader` from `@/components/ui/card`
  - `Tabs, TabsList, TabsTrigger` from `@/components/ui/tabs`

  Keep: `Check`, `Loader2`, `Save` (still used by WorkspaceOverviewTab itself? — no, those were only in the doc panel). Remove them too if they're not used elsewhere in the file.

  After removing, the lucide-react import line becomes:
  ```tsx
  import {
    Box,
    ExternalLink,
    FolderOpen,
    MoreHorizontal,
    Plus,
    Trash2,
    Upload,
  } from 'lucide-react';
  ```

- [ ] **Step 2: Replace the right column JSX**

  Find the right column block starting with `{/* ── RIGHT COLUMN — Documentation ── */}` (line ~351). Replace the entire block:

  ```tsx
  {/* ── RIGHT COLUMN — Documentation ── */}
  <div className='flex-1 p-4 flex flex-col overflow-hidden'>
    <DocumentationPanel
      value={docContent}
      onChange={setDocContent}
      onSave={() => void triggerSaveDoc()}
      saveState={saveDocState}
      isDirty={isDocDirty}
      mode={docMode}
      onModeChange={setDocMode}
      onBlur={() => { if (isDocDirty) void triggerSaveDoc(); }}
    />
  </div>
  ```

- [ ] **Step 3: Verify TypeScript compiles clean**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn tsc --noEmit 2>&1 | head -20
  ```

  Expected: 0 errors.

- [ ] **Step 4: Run linter**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn check 2>&1 | grep -E "error|warn" | grep -i "workspace\|documentation" | head -10
  ```

  Expected: 0 errors in the modified files.

- [ ] **Step 5: Commit**

  ```bash
  git add src/components/workspace/WorkspaceOverviewTab.tsx
  git commit -m "refactor(workspace-overview): use shared DocumentationPanel component"
  ```

---

### Task 3: Add Documentation panel state wiring to CollectionOverviewTab

**Files:**
- Modify: `src/components/collections/CollectionOverviewTab.tsx:1-43` (imports)
- Modify: `src/components/collections/CollectionOverviewTab.tsx:207-221` (state declarations)
- Modify: `src/components/collections/CollectionOverviewTab.tsx:318-321` (after save hook)
- Modify: `src/components/collections/CollectionOverviewTab.tsx:244-291` (load effect)

**Context:** The component already has `readme` state loaded from `collection.settings.readme` and saved via `saveSettings`. We need to add: (1) `docMode` state for the Edit/Preview toggle; (2) `isDocDirty` derived value comparing live `readme` to the persisted value; (3) a second `useSaveButton` instance (`saveDocState`/`triggerSaveDoc`) wired to a `saveDocFn` that saves only `readme` via `saveCollectionSettings`. The existing `saveSettings` continues to save all fields (including `readme`) when other tab save buttons are used — no change there.

- [ ] **Step 1: Add DocumentationPanel import and remove now-redundant imports**

  In `src/components/collections/CollectionOverviewTab.tsx`, add to the import block:

  ```tsx
  import { DocumentationPanel } from '@/components/ui/documentation-panel';
  ```

  The file currently imports `Card, CardContent` from `@/components/ui/card`. Add `CardHeader`:
  ```tsx
  import { Card, CardContent, CardHeader } from '@/components/ui/card';
  ```

  The overview section will use `DocumentationPanel` for the doc card, so we do NOT need to add ReactMarkdown/remarkGfm imports to this file — they live inside the shared component.

- [ ] **Step 2: Add docMode state**

  After the existing `const [readme, setReadme] = useState('');` line, add:

  ```tsx
  const [docMode, setDocMode] = useState<'edit' | 'preview'>('preview');
  ```

- [ ] **Step 3: Add saveDocFn and second useSaveButton hook**

  After the existing `const { state: saveState, trigger: triggerSave } = useSaveButton(saveSettings, 'Failed to save settings');` block, add:

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

- [ ] **Step 4: Add persistedReadme and isDocDirty**

  These are derived values that depend on `collection` being loaded. Add them after the `const statsLine = ...` line (after the early-return guards for `loading` and `error`):

  ```tsx
  const persistedReadme = collection.settings.readme ?? '';
  const isDocDirty = readme !== persistedReadme;
  ```

- [ ] **Step 5: Reset docMode when collection changes**

  Find the load `useEffect` that starts `setLoading(true);`. Add `setDocMode('preview');` immediately after:

  ```tsx
  useEffect(() => {
    setLoading(true);
    setDocMode('preview');
    setIsLoaded(false);
    setError(null);
    getCollection(collectionName)
      .then((col) => {
  ```

- [ ] **Step 6: Verify TypeScript compiles**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn tsc --noEmit 2>&1 | head -20
  ```

  Expected: 0 errors.

- [ ] **Step 7: Commit**

  ```bash
  git add src/components/collections/CollectionOverviewTab.tsx
  git commit -m "feat(collection-overview): add doc panel state, saveDocFn, isDocDirty wiring"
  ```

---

### Task 4: Refactor the overview section to two-column layout

**Files:**
- Modify: `src/components/collections/CollectionOverviewTab.tsx:437-515` (the overview section JSX)

**Context:** Currently the entire tab content lives inside a single `<ScrollArea>` with a `max-w-3xl mx-auto` constraint (line ~449). We restructure so the `overview` section manages its own two-column layout (left scrollable column, right Documentation panel), and all other sections (auth, variables, readme, tags) keep the existing outer `ScrollArea`. The outer scroll elevation overlay (`scrollContainerRef`, `isScrolled`) stays in place — its `useEffect` queries `[data-radix-scroll-area-viewport]` inside the ref, which will exist on non-overview tabs and be absent on the overview tab (where `isScrolled` is reset to `false` by `handleSectionChange`).

- [ ] **Step 1: Split outer ScrollArea into overview vs non-overview**

  Find the current tab content wrapper (around line 448):
  ```tsx
  <ScrollArea className='h-full'>
    <div className='p-6 max-w-3xl mx-auto space-y-6'>
  ```
  
  And its closing tags:
  ```tsx
      </div>
    </ScrollArea>
  ```

  Replace this structure so the overview section sits outside the ScrollArea, and the other sections remain inside it. The new structure inside `<div ref={scrollContainerRef} className='relative flex-1 min-h-0'>` becomes:

  ```tsx
  {/* Scroll elevation overlay — paints over content when scrolled (non-overview tabs only) */}
  <div
    className={cn(
      'pointer-events-none absolute inset-x-0 top-0 z-10 h-6 transition-opacity duration-200',
      'bg-gradient-to-b from-black/10 to-transparent',
      'dark:from-black/40 dark:to-transparent',
      isScrolled ? 'opacity-100' : 'opacity-0',
    )}
  />

  {/* Overview tab — two-column layout with its own internal scroll */}
  {activeSection === 'overview' && (
    <div className='flex h-full overflow-hidden'>
      {/* LEFT column */}
      <div className='flex-1 border-r border-border overflow-hidden flex flex-col'>
        <ScrollArea className='h-full'>
          <div className='p-5 flex flex-col gap-5'>
            <MethodBreakdown items={items} />

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

      {/* RIGHT column — Documentation panel */}
      <div className='w-80 flex-shrink-0 flex flex-col p-4'>
        <DocumentationPanel
          value={readme}
          onChange={setReadme}
          onSave={() => void triggerSaveDoc()}
          saveState={saveDocState}
          isDirty={isDocDirty}
          mode={docMode}
          onModeChange={setDocMode}
          onBlur={() => { if (isDocDirty) void triggerSaveDoc(); }}
        />
      </div>
    </div>
  )}

  {/* All other tabs — shared single-column ScrollArea */}
  {activeSection !== 'overview' && (
    <ScrollArea className='h-full'>
      <div className='p-6 max-w-3xl mx-auto space-y-6'>
        {activeSection === 'auth' && (
          /* ... existing auth tab content unchanged ... */
        )}
        {activeSection === 'variables' && (
          /* ... existing variables tab content unchanged ... */
        )}
        {activeSection === 'readme' && (
          /* ... existing readme tab content unchanged ... */
        )}
        {activeSection === 'tags' && (
          /* ... existing tags tab content unchanged ... */
        )}
      </div>
    </ScrollArea>
  )}
  ```

  **Important:** Keep the auth, variables, readme, and tags tab JSX exactly as it is today — only move it inside `{activeSection !== 'overview' && (...)}`. Do not alter any of that content.

- [ ] **Step 2: Remove the description textarea from the overview section**

  The description textarea (currently around lines 453–475) is no longer rendered in the overview section (it's replaced by the Documentation panel). It is already absent from the new left-column JSX above. Confirm `description` state and its inclusion in `saveSettings` are untouched — only the `<textarea id='col-description' ...>` render is removed.

- [ ] **Step 3: Verify TypeScript compiles clean**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn tsc --noEmit 2>&1 | head -30
  ```

  Expected: 0 errors.

- [ ] **Step 4: Run linter**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn check 2>&1 | grep -E "error" | head -20
  ```

  Expected: 0 errors.

- [ ] **Step 5: Run all tests**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn test 2>&1 | tail -15
  ```

  Expected: all pass (including the 8 DocumentationPanel tests from Task 1).

- [ ] **Step 6: Commit**

  ```bash
  git add src/components/collections/CollectionOverviewTab.tsx
  git commit -m "feat(collection-overview): two-column overview with shared DocumentationPanel"
  ```

---

### Task 5: Manual smoke test

**Files:** None — validation only.

- [ ] **Step 1: Start dev server**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn dev
  ```

  Open `http://localhost:1420`.

- [ ] **Step 2: Verify Workspace Overview tab still works**

  Open the Workspace Overview tab (default landing).

  Expected:
  - Right column shows Documentation card (rendered by `DocumentationPanel`) — Edit/Preview tabs in header, same look as before
  - Edit, type content, blur → saves (button shows Saved)
  - Preview renders markdown

- [ ] **Step 3: Verify Collection Overview tab — two-column layout**

  Open any collection → Overview tab.

  Expected:
  - Left column: MethodBreakdown card, Default Headers card (KeyValueEditor rows + Save button), Requests card (search + request rows)
  - Right column (320px): Documentation card with Edit/Preview tabs
  - Vertical border divider between columns
  - No description textarea
  - Left column scrolls independently when content overflows

- [ ] **Step 4: Verify Documentation panel saves to readme**

  In the Collection Overview tab:
  - Click Edit, type `# Hello`, blur or click Save
  - Reopen the collection (close tab, reopen) — text persists
  - Switch to the **Readme** tab — the same text appears there (same `readme` field)

- [ ] **Step 5: Verify non-overview tabs still work**

  Click Authorization, Variables, Readme, Tags tabs on the collection.

  Expected: each shows its full-width single-column content, unmodified.

- [ ] **Step 6: Verify Default Headers save**

  Add a header in the Default Headers card on the Overview tab, click Save.

  Expected: persists after reopening the collection.
