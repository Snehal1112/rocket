# Collection Overview Two-Column Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor the Collection Overview tab: two-column overview section (left: MethodBreakdown + Default Headers + Requests + Tags cards; right: Documentation panel), remove the Readme and Tags tabs, add a Documentation tab (full-page editor), upgrade `MarkdownEditor` to the Documentation card style and reuse it everywhere.

**Architecture:** `MarkdownEditor` is upgraded to render as a full Card with shadcn Tabs, monospace textarea, save-on-blur footer, and empty state — replacing the inline Documentation block in `WorkspaceOverviewTab` and powering both the overview right-column panel and the new Documentation tab in `CollectionOverviewTab`. `CollectionSection` type drops `'readme'` and `'tags'`, adds `'documentation'`. The `validSections` guard already falls back to `'overview'` for unknown stored values so persisted `'readme'`/`'tags'` states migrate automatically. No backend changes — `readme` is already persisted in `CollectionSettings`.

**Tech Stack:** React 18, TypeScript, shadcn/ui (Card, CardHeader, CardContent, Tabs, TabsList, TabsTrigger, Button), lucide-react (FileText, Check, Loader2, Save), ReactMarkdown + remark-gfm, `useSaveButton` hook, Tailwind CSS.

---

## File Map

| File | Change |
|---|---|
| `src/types/pane-types.ts` | **Modify** — update `CollectionSection` type: remove `'readme'` and `'tags'`, add `'documentation'` |
| `src/components/collections/MarkdownEditor.tsx` | **Modify** — upgrade to Documentation card style; add optional `mode`/`onModeChange`/`onSave`/`saveState`/`isDirty` props |
| `src/components/workspace/WorkspaceOverviewTab.tsx` | **Modify** — replace inline Documentation card (~90 lines) with `<MarkdownEditor>` |
| `src/components/collections/CollectionOverviewTab.tsx` | **Modify** — update `TABS`, `validSections`, add doc panel state wiring, refactor overview section to two-column layout with Tags card, replace Readme+Tags tabs with Documentation tab |

---

### Task 1: Update CollectionSection type

**Files:**
- Modify: `src/types/pane-types.ts`

**Context:** `CollectionSection` is a union type used in `CollectionTab` and consumed by `pane-store`. Removing `'readme'` and `'tags'` and adding `'documentation'` is the only change. No store logic needs updating — `updateCollectionSection` accepts any `CollectionSection` value and `CollectionOverviewTab` already has a `validSections` guard that falls back unknown stored values to `'overview'`.

- [ ] **Step 1: Update the type**

  In `src/types/pane-types.ts`, find:

  ```ts
  export type CollectionSection = 'overview' | 'auth' | 'variables' | 'readme' | 'tags';
  ```

  Replace with:

  ```ts
  export type CollectionSection = 'overview' | 'auth' | 'variables' | 'documentation';
  ```

- [ ] **Step 2: Verify TypeScript compiles**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn tsc --noEmit 2>&1 | head -30
  ```

  Expected: errors only in `CollectionOverviewTab.tsx` where `'readme'` and `'tags'` are referenced — those will be fixed in Task 4. If errors appear anywhere else, fix them now.

- [ ] **Step 3: Commit**

  ```bash
  git add src/types/pane-types.ts
  git commit -m "feat(types): update CollectionSection — remove readme/tags, add documentation"
  ```

---

### Task 2: Upgrade MarkdownEditor to Documentation card style

**Files:**
- Modify: `src/components/collections/MarkdownEditor.tsx`
- Create: `src/components/collections/MarkdownEditor.test.tsx`

**Context:** The current `MarkdownEditor` uses raw `<button>` tabs, a shadcn `Textarea`, and a prose div. It needs to become a full Card: `CardHeader` with FileText icon + "Documentation" label on left and shadcn `Tabs` on right, `CardContent` with a monospace `<textarea>` in edit mode with an optional save-on-blur footer, and a ReactMarkdown preview with empty state. The new props `mode`, `onModeChange`, `onSave`, `saveState`, `isDirty` are **optional** — when `onSave` is absent the footer is not rendered. When `mode`/`onModeChange` are absent the component manages its own mode state internally (preserving the existing Readme tab usage pattern).

- [ ] **Step 1: Write tests**

  Create `src/components/collections/MarkdownEditor.test.tsx`:

  ```tsx
  import { render, screen } from '@testing-library/react';
  import userEvent from '@testing-library/user-event';
  import { describe, expect, it, vi } from 'vitest';
  import { MarkdownEditor } from './MarkdownEditor';

  const baseProps = {
    value: '',
    onChange: vi.fn(),
  };

  describe('MarkdownEditor', () => {
    it('renders Documentation label', () => {
      render(<MarkdownEditor {...baseProps} />);
      expect(screen.getByText('Documentation')).toBeInTheDocument();
    });

    it('defaults to preview mode and shows empty state when value is empty', () => {
      render(<MarkdownEditor {...baseProps} />);
      expect(screen.getByText('No documentation yet')).toBeInTheDocument();
    });

    it('renders markdown content in preview mode when value is set', () => {
      render(<MarkdownEditor {...baseProps} value='# Hello' />);
      expect(screen.getByRole('heading', { name: 'Hello' })).toBeInTheDocument();
    });

    it('renders textarea when mode=edit is passed', () => {
      render(<MarkdownEditor {...baseProps} mode='edit' />);
      expect(screen.getByPlaceholderText(/Add documentation/)).toBeInTheDocument();
    });

    it('calls onChange when user types in edit mode', async () => {
      const onChange = vi.fn();
      render(<MarkdownEditor {...baseProps} mode='edit' onChange={onChange} />);
      await userEvent.type(screen.getByPlaceholderText(/Add documentation/), 'x');
      expect(onChange).toHaveBeenCalled();
    });

    it('calls onModeChange when Edit tab is clicked', async () => {
      const onModeChange = vi.fn();
      render(<MarkdownEditor {...baseProps} onModeChange={onModeChange} />);
      await userEvent.click(screen.getByRole('tab', { name: 'Edit' }));
      expect(onModeChange).toHaveBeenCalledWith('edit');
    });

    it('does not render save button when onSave is not provided', () => {
      render(<MarkdownEditor {...baseProps} mode='edit' />);
      expect(screen.queryByRole('button', { name: /save/i })).not.toBeInTheDocument();
    });

    it('renders save button when onSave is provided', () => {
      render(
        <MarkdownEditor {...baseProps} mode='edit' onSave={vi.fn()} saveState='idle' isDirty={false} />,
      );
      expect(screen.getByRole('button', { name: /save/i })).toBeInTheDocument();
    });

    it('save button is disabled when isDirty is false', () => {
      render(
        <MarkdownEditor {...baseProps} mode='edit' onSave={vi.fn()} saveState='idle' isDirty={false} />,
      );
      expect(screen.getByRole('button', { name: /save/i })).toBeDisabled();
    });

    it('save button is enabled when isDirty is true', () => {
      render(
        <MarkdownEditor {...baseProps} mode='edit' onSave={vi.fn()} saveState='idle' isDirty={true} />,
      );
      expect(screen.getByRole('button', { name: /save/i })).toBeEnabled();
    });

    it('clicking Add Documentation calls onModeChange with edit', async () => {
      const onModeChange = vi.fn();
      render(<MarkdownEditor {...baseProps} onModeChange={onModeChange} />);
      await userEvent.click(screen.getByRole('button', { name: /add documentation/i }));
      expect(onModeChange).toHaveBeenCalledWith('edit');
    });
  });
  ```

- [ ] **Step 2: Run tests to confirm they fail**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn test MarkdownEditor 2>&1 | tail -10
  ```

  Expected: multiple failures — component doesn't yet have the new structure.

- [ ] **Step 3: Rewrite MarkdownEditor**

  Replace the entire contents of `src/components/collections/MarkdownEditor.tsx`:

  ```tsx
  import { Check, FileText, Loader2, Save } from 'lucide-react';
  import { useState } from 'react';
  import ReactMarkdown from 'react-markdown';
  import remarkGfm from 'remark-gfm';
  import { Button } from '@/components/ui/button';
  import { Card, CardContent, CardHeader } from '@/components/ui/card';
  import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
  import type { SaveButtonState } from '@/hooks/use-save-button';
  import { cn } from '@/lib/utils';

  interface MarkdownEditorProps {
    value: string;
    onChange: (value: string) => void;
    onBlur?: () => void;
    /** Controlled mode — when omitted the component manages mode internally */
    mode?: 'edit' | 'preview';
    onModeChange?: (mode: 'edit' | 'preview') => void;
    /** When provided, a Save button is shown in the edit footer */
    onSave?: () => void;
    saveState?: SaveButtonState;
    isDirty?: boolean;
  }

  export function MarkdownEditor({
    value,
    onChange,
    onBlur,
    mode: controlledMode,
    onModeChange,
    onSave,
    saveState = 'idle',
    isDirty = false,
  }: MarkdownEditorProps) {
    const [internalMode, setInternalMode] = useState<'edit' | 'preview'>('preview');
    const mode = controlledMode ?? internalMode;

    function handleModeChange(next: 'edit' | 'preview') {
      if (onModeChange) {
        onModeChange(next);
      } else {
        setInternalMode(next);
      }
    }

    return (
      <Card className='flex-1 flex flex-col overflow-hidden'>
        <CardHeader className='flex flex-row items-center justify-between py-2.5 px-4 shrink-0'>
          <div className='flex items-center gap-2'>
            <FileText className='h-3.5 w-3.5 text-muted-foreground' />
            <span className='text-xs font-semibold text-muted-foreground'>Documentation</span>
          </div>
          <Tabs value={mode} onValueChange={(v) => handleModeChange(v as 'edit' | 'preview')}>
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
              {onSave && (
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
              )}
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
                    onClick={() => handleModeChange('edit')}
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

- [ ] **Step 4: Run tests — all should pass**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn test MarkdownEditor 2>&1 | tail -15
  ```

  Expected: 11 tests pass, 0 failures.

- [ ] **Step 5: Verify TypeScript compiles**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn tsc --noEmit 2>&1 | head -20
  ```

  Expected: errors only in `CollectionOverviewTab.tsx` (Task 4) and `WorkspaceOverviewTab.tsx` (Task 3) — not in `MarkdownEditor.tsx` itself.

- [ ] **Step 6: Commit**

  ```bash
  git add src/components/collections/MarkdownEditor.tsx src/components/collections/MarkdownEditor.test.tsx
  git commit -m "feat(markdown-editor): upgrade to Documentation card style with optional save/mode props"
  ```

---

### Task 3: Replace inline Documentation card in WorkspaceOverviewTab with MarkdownEditor

**Files:**
- Modify: `src/components/workspace/WorkspaceOverviewTab.tsx`

**Context:** `WorkspaceOverviewTab` has ~90 lines of inline Documentation card JSX (lines 352–442). Replace with `<MarkdownEditor>` passing existing local state: `docMode`, `docContent`, `isDocDirty`, `saveDocState`, `triggerSaveDoc`. The outer `<div className='flex-1 p-4 flex flex-col overflow-hidden'>` wrapper stays unchanged.

- [ ] **Step 1: Add MarkdownEditor import**

  In `src/components/workspace/WorkspaceOverviewTab.tsx`, add:

  ```tsx
  import { MarkdownEditor } from '@/components/collections/MarkdownEditor';
  ```

  Remove these imports that are now only used inside `MarkdownEditor` (verify each is not used elsewhere in the file before removing):
  - `Check`, `FileText`, `Loader2`, `Save` from lucide-react
  - `ReactMarkdown` from react-markdown
  - `remarkGfm` from remark-gfm
  - `CardHeader` from `@/components/ui/card`
  - `Tabs`, `TabsList`, `TabsTrigger` from `@/components/ui/tabs`

  The remaining lucide-react import becomes:
  ```tsx
  import { Box, ExternalLink, FolderOpen, MoreHorizontal, Plus, Trash2, Upload } from 'lucide-react';
  ```

- [ ] **Step 2: Replace the right column JSX**

  Find the block starting with `{/* ── RIGHT COLUMN — Documentation ── */}` (line ~351). Replace the entire block (through the closing `</div>` of the right column) with:

  ```tsx
  {/* ── RIGHT COLUMN — Documentation ── */}
  <div className='flex-1 p-4 flex flex-col overflow-hidden'>
    <MarkdownEditor
      value={docContent}
      onChange={setDocContent}
      mode={docMode}
      onModeChange={setDocMode}
      onSave={() => void triggerSaveDoc()}
      saveState={saveDocState}
      isDirty={isDocDirty}
      onBlur={() => { if (isDocDirty) void triggerSaveDoc(); }}
    />
  </div>
  ```

- [ ] **Step 3: Verify TypeScript compiles clean**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn tsc --noEmit 2>&1 | head -20
  ```

  Expected: errors only in `CollectionOverviewTab.tsx` (Task 4) — not here.

- [ ] **Step 4: Run linter**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn check 2>&1 | grep "error" | head -10
  ```

  Expected: 0 errors in `WorkspaceOverviewTab.tsx`.

- [ ] **Step 5: Commit**

  ```bash
  git add src/components/workspace/WorkspaceOverviewTab.tsx
  git commit -m "refactor(workspace-overview): use shared MarkdownEditor for Documentation panel"
  ```

---

### Task 4: Refactor CollectionOverviewTab — two-column overview, new Documentation tab, remove Readme/Tags tabs

**Files:**
- Modify: `src/components/collections/CollectionOverviewTab.tsx`

**Context:** This is the main refactor. Changes:
1. Update `TABS` array: remove Readme and Tags entries, add Documentation entry (`value: 'documentation'`).
2. Update `validSections` to `['overview', 'auth', 'variables', 'documentation']`.
3. Add `docMode` state, `saveDocFn`, second `useSaveButton` instance, `persistedReadme`, `isDocDirty`.
4. Reset `docMode` to `'preview'` on collection load.
5. Replace the overview section's single-column `<ScrollArea>` with a two-column layout: left scrollable column (MethodBreakdown, Default Headers, Requests, Tags cards) + right column (`<MarkdownEditor>` with doc-specific save wiring).
6. Remove the `{activeSection === 'readme' && ...}` block entirely.
7. Remove the `{activeSection === 'tags' && ...}` block entirely.
8. Add `{activeSection === 'documentation' && ...}` block: full-height `<MarkdownEditor>` using the global `triggerSave`/`saveState`/`isDirty` (same as the old Readme tab).
9. Remove the `description` textarea from the overview section.

- [ ] **Step 1: Update TABS array and validSections**

  Find the `TABS` constant (around line 187):
  ```tsx
  const TABS: { label: string; value: CollectionSection }[] = [
    { label: 'Overview', value: 'overview' },
    { label: 'Authorization', value: 'auth' },
    { label: 'Variables', value: 'variables' },
    { label: 'Readme', value: 'readme' },
    { label: 'Tags', value: 'tags' },
  ];
  ```

  Replace with:
  ```tsx
  const TABS: { label: string; value: CollectionSection }[] = [
    { label: 'Overview', value: 'overview' },
    { label: 'Authorization', value: 'auth' },
    { label: 'Variables', value: 'variables' },
    { label: 'Documentation', value: 'documentation' },
  ];
  ```

  Find the `validSections` line (around line 218):
  ```tsx
  const validSections: CollectionSection[] = ['overview', 'auth', 'variables', 'readme', 'tags'];
  ```

  Replace with:
  ```tsx
  const validSections: CollectionSection[] = ['overview', 'auth', 'variables', 'documentation'];
  ```

- [ ] **Step 2: Add docMode state**

  After `const [readme, setReadme] = useState('');`, add:

  ```tsx
  const [docMode, setDocMode] = useState<'edit' | 'preview'>('preview');
  ```

- [ ] **Step 3: Add saveDocFn and second useSaveButton hook**

  After `const { state: saveState, trigger: triggerSave } = useSaveButton(saveSettings, 'Failed to save settings');`, add:

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

  After `const statsLine = ...` (after the early-return guards), add:

  ```tsx
  const persistedReadme = collection.settings.readme ?? '';
  const isDocDirty = readme !== persistedReadme;
  ```

- [ ] **Step 5: Reset docMode on collection load**

  Find the load `useEffect`. Add `setDocMode('preview');` immediately after `setLoading(true);`:

  ```tsx
  useEffect(() => {
    setLoading(true);
    setDocMode('preview');
    setIsLoaded(false);
    setError(null);
    getCollection(collectionName)
      .then((col) => {
  ```

- [ ] **Step 6: Replace the entire tab content section**

  Find the entire block from `<div ref={scrollContainerRef} className='relative flex-1 min-h-0'>` through its closing `</div>` (currently wrapping `<ScrollArea>` with all tab sections inside). Replace with:

  ```tsx
  <div ref={scrollContainerRef} className='relative flex-1 min-h-0'>
    {/* Scroll elevation overlay — visible on non-overview tabs when scrolled */}
    <div
      className={cn(
        'pointer-events-none absolute inset-x-0 top-0 z-10 h-6 transition-opacity duration-200',
        'bg-gradient-to-b from-black/10 to-transparent',
        'dark:from-black/40 dark:to-transparent',
        isScrolled ? 'opacity-100' : 'opacity-0',
      )}
    />

    {/* Overview tab — two-column layout */}
    {activeSection === 'overview' && (
      <div className='flex h-full overflow-hidden'>
        {/* LEFT — scrollable cards */}
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

              <Card>
                <CardHeader className='pb-2 pt-4 px-4'>
                  <span className='text-sm font-medium'>Tags</span>
                </CardHeader>
                <CardContent className='px-4 pb-4'>
                  <TagsList collection={collection} />
                </CardContent>
              </Card>
            </div>
          </ScrollArea>
        </div>

        {/* RIGHT — Documentation panel */}
        <div className='w-80 flex-shrink-0 flex flex-col p-4'>
          <MarkdownEditor
            value={readme}
            onChange={setReadme}
            mode={docMode}
            onModeChange={setDocMode}
            onSave={() => void triggerSaveDoc()}
            saveState={saveDocState}
            isDirty={isDocDirty}
            onBlur={() => { if (isDocDirty) void triggerSaveDoc(); }}
          />
        </div>
      </div>
    )}

    {/* All other tabs — single-column ScrollArea */}
    {activeSection !== 'overview' && (
      <ScrollArea className='h-full'>
        <div className='p-6 max-w-3xl mx-auto space-y-6'>
          {/* Authorization tab */}
          {activeSection === 'auth' && (
            /* KEEP EXISTING AUTH TAB JSX EXACTLY AS-IS */
          )}

          {/* Variables tab */}
          {activeSection === 'variables' && (
            /* KEEP EXISTING VARIABLES TAB JSX EXACTLY AS-IS */
          )}

          {/* Documentation tab — full-page editor */}
          {activeSection === 'documentation' && (
            <div className='h-full flex flex-col'>
              <MarkdownEditor
                value={readme}
                onChange={(v) => {
                  setReadme(v);
                  setIsDirty(true);
                }}
                mode={docMode}
                onModeChange={setDocMode}
                onSave={() => void triggerSave()}
                saveState={saveState}
                isDirty={isDirty}
                onBlur={() => {
                  if (isDirty) void triggerSave();
                }}
              />
            </div>
          )}
        </div>
      </ScrollArea>
    )}
  </div>
  ```

  **Important:** Copy the existing auth and variables tab JSX blocks exactly — do not alter their content. Remove the `{activeSection === 'readme' && ...}` and `{activeSection === 'tags' && ...}` blocks entirely.

- [ ] **Step 7: Remove unused imports**

  After the refactor, `TagsList` is still imported and used. `MarkdownEditor` import already exists. Verify `description` is still declared as state (it is — it stays in the `saveSettings` payload). The `description` textarea render is not in the new overview JSX, so it's gone. No other cleanup needed.

- [ ] **Step 8: Verify TypeScript compiles clean**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn tsc --noEmit 2>&1 | head -30
  ```

  Expected: 0 errors.

- [ ] **Step 9: Run linter**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn check 2>&1 | grep "error" | head -20
  ```

  Expected: 0 errors.

- [ ] **Step 10: Run all tests**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn test 2>&1 | tail -15
  ```

  Expected: all pass (including the 11 MarkdownEditor tests from Task 2).

- [ ] **Step 11: Commit**

  ```bash
  git add src/components/collections/CollectionOverviewTab.tsx
  git commit -m "feat(collection-overview): two-column overview, Documentation tab, remove Readme/Tags tabs"
  ```

---

### Task 5: Manual smoke test

**Files:** None — validation only.

- [ ] **Step 1: Start dev server**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn dev
  ```

  Open `http://localhost:1420`.

- [ ] **Step 2: Verify Workspace Overview tab**

  Open the Workspace Overview tab.

  Expected: right column Documentation card looks identical to before — Edit/Preview tabs, monospace textarea, save-on-blur footer, markdown preview with empty state.

- [ ] **Step 3: Verify Collection Overview tab — two-column layout**

  Open any collection → Overview tab.

  Expected:
  - Tab bar shows: Overview · Authorization · Variables · Documentation (no Readme, no Tags)
  - Left column: MethodBreakdown card, Default Headers card (rows + Save button), Requests card (search + rows), Tags card (read-only tag chips or "No tags found" message)
  - Right column (320px): Documentation card (MarkdownEditor) with Edit/Preview tabs
  - Vertical border divider between columns
  - No description textarea

- [ ] **Step 4: Verify Documentation panel in overview saves to readme**

  In the right column of Overview:
  - Click Edit, type `# Hello world`, blur — button shows Saved
  - Close and reopen the collection — text persists
  - Click the **Documentation** tab — same text appears there (same `readme` field)

- [ ] **Step 5: Verify Documentation tab (full-page)**

  Click the Documentation tab.

  Expected: full-width `MarkdownEditor` with the same content, Edit/Preview toggle works, Save button inside the editor saves correctly.

- [ ] **Step 6: Verify Authorization and Variables tabs still work**

  Click each — content unchanged, save works.

- [ ] **Step 7: Verify Default Headers save**

  Add a header on the Overview tab, click Save — persists after reopening.

- [ ] **Step 8: Verify Tags card**

  On a collection that has requests with tags: Tags card shows tag chips with counts.
  On a collection with no tagged requests: Tags card shows "No tags found. Add tags to requests to see them here."
