# Collection Overview Two-Column Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor the Collection Overview tab's `overview` section into a two-column layout — left column with existing content cards (MethodBreakdown, Default Headers, Requests), right column with a persistent Documentation panel — by upgrading the existing `MarkdownEditor` component to the Documentation card style and reusing it in both `WorkspaceOverviewTab` and `CollectionOverviewTab`.

**Architecture:** `MarkdownEditor` is upgraded to render as a full Card with shadcn Tabs, a monospace textarea, save-on-blur footer, and empty state — matching the Documentation card in `WorkspaceOverviewTab`. New props (`onSave`, `saveState`, `isDirty`) are optional so the existing Readme tab usage continues to work. `WorkspaceOverviewTab` replaces its ~90-line inline Documentation block with `<MarkdownEditor>`. `CollectionOverviewTab` gets a second `useSaveButton` instance for the overview Documentation panel and a two-column layout in the overview section. No backend changes — `readme` is already persisted in `CollectionSettings`.

**Tech Stack:** React 18, TypeScript, shadcn/ui (Card, CardHeader, CardContent, Tabs, TabsList, TabsTrigger, Button), lucide-react (FileText, Check, Loader2, Save), ReactMarkdown + remark-gfm, `useSaveButton` hook, Tailwind CSS.

---

## File Map

| File | Change |
|---|---|
| `src/components/collections/MarkdownEditor.tsx` | **Modify** — upgrade to Documentation card style; add optional `onSave`/`saveState`/`isDirty` props |
| `src/components/workspace/WorkspaceOverviewTab.tsx` | **Modify** — replace inline Documentation card (~90 lines) with `<MarkdownEditor>` |
| `src/components/collections/CollectionOverviewTab.tsx` | **Modify** — add doc panel state wiring, refactor overview section to two-column layout using `<MarkdownEditor>`, update Readme tab usage |

No backend changes — `readme` field is already in `CollectionSettings`, persisted by `save_collection_settings`, and loaded/saved by the existing `saveSettings` callback.

---

### Task 1: Upgrade MarkdownEditor to Documentation card style

**Files:**
- Modify: `src/components/collections/MarkdownEditor.tsx`

**Context:** The current `MarkdownEditor` is a simple div with raw `<button>` tabs, a shadcn `Textarea`, and a prose div. It needs to become the full Documentation card: a `Card` wrapper with `CardHeader` containing a `FileText` icon + "Documentation" label on the left and shadcn `Tabs` on the right, a `CardContent` with a monospace `<textarea>` (not `Textarea`) in edit mode with a save-on-blur footer, and a `ReactMarkdown` preview with empty state. The new props `onSave`, `saveState`, and `isDirty` are **optional** — when absent, the footer save button is not rendered and the component behaves as a controlled editor only (preserving the existing Readme tab usage).

- [ ] **Step 1: Write tests for the upgraded MarkdownEditor**

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
    it('renders Documentation label and FileText icon', () => {
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

    it('renders textarea in edit mode', () => {
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
        <MarkdownEditor
          {...baseProps}
          mode='edit'
          onSave={vi.fn()}
          saveState='idle'
          isDirty={false}
        />,
      );
      expect(screen.getByRole('button', { name: /save/i })).toBeInTheDocument();
    });

    it('save button is disabled when isDirty is false', () => {
      render(
        <MarkdownEditor
          {...baseProps}
          mode='edit'
          onSave={vi.fn()}
          saveState='idle'
          isDirty={false}
        />,
      );
      expect(screen.getByRole('button', { name: /save/i })).toBeDisabled();
    });

    it('save button is enabled when isDirty is true', () => {
      render(
        <MarkdownEditor
          {...baseProps}
          mode='edit'
          onSave={vi.fn()}
          saveState='idle'
          isDirty={true}
        />,
      );
      expect(screen.getByRole('button', { name: /save/i })).toBeEnabled();
    });

    it('clicking Add Documentation switches to edit mode', async () => {
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

  Expected: multiple failures — component doesn't yet have the new props or structure.

- [ ] **Step 3: Rewrite MarkdownEditor**

  Replace the entire contents of `src/components/collections/MarkdownEditor.tsx`:

  ```tsx
  import { Check, FileText, Loader2, Save } from 'lucide-react';
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
    /** Controlled mode — if omitted the component manages mode internally */
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
    mode = 'preview',
    onModeChange,
    onSave,
    saveState = 'idle',
    isDirty = false,
  }: MarkdownEditorProps) {
    return (
      <Card className='flex-1 flex flex-col overflow-hidden'>
        <CardHeader className='flex flex-row items-center justify-between py-2.5 px-4 shrink-0'>
          <div className='flex items-center gap-2'>
            <FileText className='h-3.5 w-3.5 text-muted-foreground' />
            <span className='text-xs font-semibold text-muted-foreground'>Documentation</span>
          </div>
          <Tabs value={mode} onValueChange={(v) => onModeChange?.(v as 'edit' | 'preview')}>
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
                    onClick={() => onModeChange?.('edit')}
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

  Expected: 0 errors.

- [ ] **Step 6: Commit**

  ```bash
  git add src/components/collections/MarkdownEditor.tsx src/components/collections/MarkdownEditor.test.tsx
  git commit -m "feat(markdown-editor): upgrade to Documentation card style with optional save props"
  ```

---

### Task 2: Replace inline Documentation card in WorkspaceOverviewTab with MarkdownEditor

**Files:**
- Modify: `src/components/workspace/WorkspaceOverviewTab.tsx`

**Context:** `WorkspaceOverviewTab` currently has ~90 lines of inline Documentation card JSX (lines 352–442). We replace it with `<MarkdownEditor>` passing the existing local state. The component already has `docMode`, `docContent`, `isDocDirty`, `saveDocState`, `triggerSaveDoc`. The outer `<div className='flex-1 p-4 flex flex-col overflow-hidden'>` wrapper stays.

- [ ] **Step 1: Add MarkdownEditor import**

  In `src/components/workspace/WorkspaceOverviewTab.tsx`, add:

  ```tsx
  import { MarkdownEditor } from '@/components/collections/MarkdownEditor';
  ```

  Then remove these imports that are now only used inside `MarkdownEditor` (verify each is not used elsewhere in the file before removing):
  - `Check`, `FileText`, `Loader2`, `Save` from lucide-react
  - `ReactMarkdown` from react-markdown
  - `remarkGfm` from remark-gfm
  - `CardHeader` from `@/components/ui/card`
  - `Tabs`, `TabsList`, `TabsTrigger` from `@/components/ui/tabs`

  The remaining lucide-react imports are:
  ```tsx
  import { Box, ExternalLink, FolderOpen, MoreHorizontal, Plus, Trash2, Upload } from 'lucide-react';
  ```

- [ ] **Step 2: Replace the right column JSX**

  Find the right column block starting with `{/* ── RIGHT COLUMN — Documentation ── */}` (line ~351). Replace the entire block with:

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

  Expected: 0 errors.

- [ ] **Step 4: Run linter**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn check 2>&1 | grep "error" | head -10
  ```

  Expected: 0 errors.

- [ ] **Step 5: Commit**

  ```bash
  git add src/components/workspace/WorkspaceOverviewTab.tsx
  git commit -m "refactor(workspace-overview): use shared MarkdownEditor for Documentation panel"
  ```

---

### Task 3: Add Documentation panel state wiring to CollectionOverviewTab

**Files:**
- Modify: `src/components/collections/CollectionOverviewTab.tsx`

**Context:** The component already has `readme` state loaded from `collection.settings.readme` and saved via `saveSettings`. We need: (1) `docMode` state for the Edit/Preview toggle; (2) `isDocDirty` derived value comparing live `readme` to the persisted value on disk; (3) a second `useSaveButton` instance (`saveDocState`/`triggerSaveDoc`) wired to a `saveDocFn` that saves only `readme` independently. The existing `saveSettings` continues to save all fields when the user hits Save on other tabs.

- [ ] **Step 1: Add docMode state**

  After the existing `const [readme, setReadme] = useState('');` line, add:

  ```tsx
  const [docMode, setDocMode] = useState<'edit' | 'preview'>('preview');
  ```

- [ ] **Step 2: Add saveDocFn and second useSaveButton hook**

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

- [ ] **Step 3: Add persistedReadme and isDocDirty**

  These derived values depend on `collection` being loaded. Add them after the `const statsLine = ...` line (after the early-return guards):

  ```tsx
  const persistedReadme = collection.settings.readme ?? '';
  const isDocDirty = readme !== persistedReadme;
  ```

- [ ] **Step 4: Reset docMode when collection changes**

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

- [ ] **Step 5: Verify TypeScript compiles**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn tsc --noEmit 2>&1 | head -20
  ```

  Expected: 0 errors.

- [ ] **Step 6: Commit**

  ```bash
  git add src/components/collections/CollectionOverviewTab.tsx
  git commit -m "feat(collection-overview): add docMode, saveDocFn, isDocDirty wiring"
  ```

---

### Task 4: Refactor overview section to two-column layout and update Readme tab

**Files:**
- Modify: `src/components/collections/CollectionOverviewTab.tsx`

**Context:** The `overview` section currently lives inside a single outer `<ScrollArea>` with `max-w-3xl mx-auto`. We split so the overview section manages its own two-column layout (left scrollable column with three cards, right column with `<MarkdownEditor>`), while auth/variables/readme/tags keep the existing outer `ScrollArea`. The Readme tab currently uses `<MarkdownEditor>` with the old props (`value`, `onChange`, `onBlur`) — we update it to also pass `mode`/`onModeChange` using the existing `isDirty`/`triggerSave` pattern.

- [ ] **Step 1: Replace the overview section with two-column layout**

  Find the current outer tab content structure:
  ```tsx
  <ScrollArea className='h-full'>
    <div className='p-6 max-w-3xl mx-auto space-y-6'>
      {activeSection === 'overview' && ( ... )}
      {activeSection === 'auth' && ( ... )}
      {activeSection === 'variables' && ( ... )}
      {activeSection === 'readme' && ( ... )}
      {activeSection === 'tags' && ( ... )}
    </div>
  </ScrollArea>
  ```

  Replace with this structure (keep auth/variables/readme/tags content exactly as-is, only restructure the wrapper):

  ```tsx
  {/* Overview tab — two-column layout with its own internal scroll */}
  {activeSection === 'overview' && (
    <div className='flex h-full overflow-hidden'>
      {/* LEFT column — scrollable cards */}
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

  {/* All other tabs — shared single-column ScrollArea */}
  {activeSection !== 'overview' && (
    <ScrollArea className='h-full'>
      <div className='p-6 max-w-3xl mx-auto space-y-6'>
        {activeSection === 'auth' && (
          /* PASTE EXISTING AUTH TAB CONTENT HERE — no changes */
        )}
        {activeSection === 'variables' && (
          /* PASTE EXISTING VARIABLES TAB CONTENT HERE — no changes */
        )}
        {activeSection === 'readme' && (
          /* see Step 2 below */
        )}
        {activeSection === 'tags' && (
          /* PASTE EXISTING TAGS TAB CONTENT HERE — no changes */
        )}
      </div>
    </ScrollArea>
  )}
  ```

  **Important:** The auth, variables, and tags tab JSX must be copied exactly as they exist today — do not alter any of that content.

- [ ] **Step 2: Update the Readme tab to use MarkdownEditor's new props**

  The current Readme tab (inside the `activeSection === 'readme'` block) renders:

  ```tsx
  <div className='space-y-4'>
    <MarkdownEditor
      value={readme}
      onChange={(v) => {
        setReadme(v);
        setIsDirty(true);
      }}
      onBlur={() => {
        if (isDirty) void triggerSave();
      }}
    />
    <div className='flex justify-end'>
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
  </div>
  ```

  Replace with (the save button is now inside `MarkdownEditor`, and mode is controlled by `docMode`/`setDocMode`):

  ```tsx
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
  ```

  Note: The Readme tab uses `triggerSave`/`saveState`/`isDirty` (the global settings save) not the doc-specific ones — this is correct because the Readme tab saves all settings together via `saveSettings`.

- [ ] **Step 3: Remove the description textarea from the overview section**

  The description `<textarea id='col-description' ...>` block is not included in the new left column JSX above. Confirm `description` state and its inclusion in `saveSettings` payload are untouched — only the textarea render is gone.

- [ ] **Step 4: Verify TypeScript compiles clean**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn tsc --noEmit 2>&1 | head -30
  ```

  Expected: 0 errors.

- [ ] **Step 5: Run linter**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn check 2>&1 | grep "error" | head -20
  ```

  Expected: 0 errors.

- [ ] **Step 6: Run all tests**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn test 2>&1 | tail -15
  ```

  Expected: all pass (including the 11 MarkdownEditor tests from Task 1).

- [ ] **Step 7: Commit**

  ```bash
  git add src/components/collections/CollectionOverviewTab.tsx
  git commit -m "feat(collection-overview): two-column overview with MarkdownEditor Documentation panel"
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

  Expected:
  - Right column shows the Documentation card (rendered by `MarkdownEditor`) — same look as before (FileText icon, "Documentation" label, Edit/Preview tabs)
  - Edit mode: monospace textarea with save-on-blur footer
  - Preview mode: renders markdown; empty state with "Add Documentation" button

- [ ] **Step 3: Verify Collection Overview tab — two-column layout**

  Open any collection → Overview tab.

  Expected:
  - Left column: MethodBreakdown card, Default Headers card (KeyValueEditor rows + Save button), Requests card (search + request rows)
  - Right column (320px): Documentation card (`MarkdownEditor`) with Edit/Preview tabs
  - Vertical border divider between columns
  - No description textarea

- [ ] **Step 4: Verify Documentation panel saves to readme**

  In the Collection Overview tab:
  - Click Edit, type `# Hello`, blur or click Save — button shows Saved
  - Close and reopen the collection — text persists
  - Switch to the **Readme** tab — same text appears (same `readme` field, same `MarkdownEditor` component)

- [ ] **Step 5: Verify Readme tab still works**

  Open the Readme tab on the collection.

  Expected: `MarkdownEditor` renders with the readme content, Edit/Preview toggle works, Save button inside the editor saves correctly.

- [ ] **Step 6: Verify non-overview tabs still work**

  Click Authorization, Variables, Tags tabs.

  Expected: each shows its full-width single-column content, unmodified.

- [ ] **Step 7: Verify Default Headers save**

  Add a header in the Default Headers card on the Overview tab, click Save.

  Expected: persists after reopening the collection.
