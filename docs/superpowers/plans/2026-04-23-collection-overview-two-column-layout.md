# Collection Overview Two-Column Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor the Collection Overview tab: two-column overview section (left: MethodBreakdown + Default Headers + Requests + Tags cards; right: Documentation panel), remove the Readme and Tags tabs, add a Documentation tab (full-page editor), upgrade `MarkdownEditor` to the Documentation card style, and fix the backend to store collection documentation in the spec-compliant `docs:` field instead of the non-standard `readme:` field.

**Architecture:** Three layers of change: (1) **Backend** — `CollectionSettings` drops `description` and `readme`, adds `docs: Option<String>`; `oc_conversions.rs` maps `OcCollection.docs` ↔ `CollectionSettings.docs`; `OcCollection` serde struct removes the non-standard `readme` field. (2) **Frontend types** — `CollectionSettings` interface updated to match. (3) **UI** — `MarkdownEditor` upgraded to Documentation card style and reused in `WorkspaceOverviewTab`, the Overview right-column panel, and the new Documentation tab; `CollectionSection` type updated; Readme/Tags tabs removed.

**Tech Stack:** Rust (serde_yaml, thiserror), React 18, TypeScript, shadcn/ui (Card, CardHeader, CardContent, Tabs, TabsList, TabsTrigger, Button), lucide-react (FileText, Check, Loader2, Save), ReactMarkdown + remark-gfm, `useSaveButton` hook, Tailwind CSS.

---

## File Map

| File | Change |
|---|---|
| `crates/rocket-collection/src/settings.rs` | **Modify** — replace `description` + `readme` with `docs: Option<String>` |
| `crates/rocket-infra/src/opencollection.rs` | **Modify** — remove `readme` field from `OcCollection` |
| `crates/rocket-infra/src/oc_conversions.rs` | **Modify** — map `oc.docs` ↔ `settings.docs`; remove all `readme` references |
| `src/lib/tauri-api.ts` | **Modify** — update `CollectionSettings` interface: remove `description`/`readme`, add `docs` |
| `src/types/pane-types.ts` | **Modify** — update `CollectionSection` type: remove `'readme'`/`'tags'`, add `'documentation'` |
| `src/components/collections/MarkdownEditor.tsx` | **Modify** — upgrade to Documentation card style; add optional `mode`/`onModeChange`/`onSave`/`saveState`/`isDirty` props |
| `src/components/workspace/WorkspaceOverviewTab.tsx` | **Modify** — replace inline Documentation card with `<MarkdownEditor>` |
| `src/components/collections/CollectionOverviewTab.tsx` | **Modify** — use `docs` state throughout; update TABS/validSections; two-column overview; Documentation tab; remove Readme/Tags tabs |

---

### Task 1: Fix backend — replace description/readme with docs in CollectionSettings

**Files:**
- Modify: `crates/rocket-collection/src/settings.rs`
- Modify: `crates/rocket-infra/src/opencollection.rs`
- Modify: `crates/rocket-infra/src/oc_conversions.rs`

**Context:** Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

The OpenCollection spec defines `docs:` as the standard string field for collection-level markdown documentation. Bruno stores docs in `docs {}` in `.bru` which serializes to `docs:` in YAML. Currently Rocket misuses this: `OcCollection.docs` maps to `CollectionSettings.description` (a short summary field), and `CollectionSettings.readme` uses a non-standard `readme:` field in the YML.

Fix: collapse both into a single `docs: Option<String>` field that round-trips correctly through `OcCollection.docs`.

- [ ] **Step 1: Write a failing test in rocket-infra**

  In `crates/rocket-infra/src/oc_conversions.rs`, find the `#[cfg(test)]` block at the bottom and add:

  ```rust
  #[test]
  fn collection_docs_roundtrips_through_docs_field() {
      use crate::opencollection::OcCollection;
      use rocket_collection::settings::CollectionSettings;

      // Build a minimal OcCollection with docs set
      let oc = OcCollection {
          opencollection: Some("1.0.0".into()),
          uid: None,
          info: None,
          config: None,
          items: None,
          request: None,
          docs: Some("# Hello\nWorld".into()),
          bundled: None,
          extensions: None,
      };

      // Convert to domain
      let col = oc_collection_to_collection(oc, "test-col".into(), vec![]);
      assert_eq!(col.settings.docs, Some("# Hello\nWorld".into()));

      // Convert back to OC
      let oc2 = collection_to_oc_collection(col);
      assert_eq!(oc2.docs, Some("# Hello\nWorld".into()));
  }
  ```

- [ ] **Step 2: Run the test to confirm it fails**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && cargo test -p rocket-infra collection_docs_roundtrips 2>&1 | tail -15
  ```

  Expected: compile error — `CollectionSettings` has no `docs` field yet, and `OcCollection` still has `readme`.

- [ ] **Step 3: Update CollectionSettings**

  In `crates/rocket-collection/src/settings.rs`, replace the `description` and `readme` fields:

  ```rust
  /// Per-collection default auth, headers, and variables, stored in opencollection.yml.
  #[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
  #[serde(rename_all = "camelCase")]
  pub struct CollectionSettings {
      /// Markdown documentation for this collection (maps to `docs:` in opencollection.yml).
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub docs: Option<String>,

      /// Optional auth applied to all requests in this collection.
      #[serde(default)]
      pub auth: Option<Auth>,

      /// Default headers prepended to every request in this collection.
      #[serde(default)]
      pub headers: Vec<Header>,

      /// Collection-scoped variables, resolved alongside environment variables.
      #[serde(default)]
      pub variables: Vec<CollectionVariable>,
  }
  ```

- [ ] **Step 4: Remove readme from OcCollection**

  In `crates/rocket-infra/src/opencollection.rs`, find `OcCollection` (around line 950). Remove the `readme` field:

  ```rust
  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  pub struct OcCollection {
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub opencollection: Option<String>,
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub uid: Option<String>,
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub info: Option<OcInfo>,
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub config: Option<OcCollectionConfig>,
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub items: Option<Vec<OcItem>>,
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub request: Option<OcRequestDefaults>,
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub docs: Option<String>,
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub bundled: Option<bool>,
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub extensions: Option<serde_yaml::Value>,
  }
  ```

- [ ] **Step 5: Fix oc_conversions.rs**

  In `crates/rocket-infra/src/oc_conversions.rs`, find the two conversion functions and update them.

  **`oc_collection_to_collection`** — replace the settings block (two occurrences, the `if let Some(defaults)` branch and the `else` branch):

  ```rust
  // if let Some(defaults) = oc.request branch:
  let settings = if let Some(defaults) = oc.request {
      CollectionSettings {
          docs: oc.docs,
          auth: defaults.auth.map(Auth::from),
          headers: defaults
              .headers
              .unwrap_or_default()
              .into_iter()
              .map(Header::from)
              .collect(),
          variables: defaults
              .variables
              .unwrap_or_default()
              .into_iter()
              .map(oc_variable_to_collection_variable)
              .collect(),
      }
  } else {
      CollectionSettings {
          docs: oc.docs,
          ..CollectionSettings::default()
      }
  };
  ```

  **`collection_to_oc_collection`** — update the `OcCollection { ... }` construction at the end of the function:

  ```rust
  OcCollection {
      opencollection: Some("1.0.0".into()),
      uid: None,
      info: Some(OcInfo {
          name: col.name,
          summary: None,
          version: None,
          authors: None,
      }),
      config: None,
      items: if items.is_empty() { None } else { Some(items) },
      request,
      docs: col.settings.docs,
      bundled: None,
      extensions: None,
  }
  ```

- [ ] **Step 6: Run the test — should pass now**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && cargo test -p rocket-infra collection_docs_roundtrips 2>&1 | tail -10
  ```

  Expected: PASS.

- [ ] **Step 7: Run all infra tests to check for regressions**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && cargo test -p rocket-infra 2>&1 | tail -20
  ```

  Fix any test that references `description`, `readme`, or `oc.readme` — update them to use `docs` instead.

- [ ] **Step 8: Run cargo check across the workspace**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && cargo check 2>&1 | grep "^error" | head -20
  ```

  Fix all compile errors (any code referencing `settings.description`, `settings.readme`, `oc.readme`).

- [ ] **Step 9: Commit**

  ```bash
  git add crates/rocket-collection/src/settings.rs crates/rocket-infra/src/opencollection.rs crates/rocket-infra/src/oc_conversions.rs
  git commit -m "fix(collection): store documentation in spec-compliant docs field, remove non-standard readme"
  ```

---

### Task 2: Update frontend CollectionSettings interface

**Files:**
- Modify: `src/lib/tauri-api.ts`

**Context:** The `CollectionSettings` TypeScript interface must match the updated Rust struct. Replace `description?: string` and `readme?: string` with `docs?: string`.

- [ ] **Step 1: Update the interface**

  In `src/lib/tauri-api.ts`, find the `CollectionSettings` interface (around line 59):

  ```ts
  export interface CollectionSettings {
    description?: string;
    readme?: string;
    // ... other fields
  }
  ```

  Replace the `description` and `readme` lines with:

  ```ts
  export interface CollectionSettings {
    docs?: string;
    // ... other fields unchanged
  }
  ```

- [ ] **Step 2: Verify TypeScript compiles**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn tsc --noEmit 2>&1 | head -30
  ```

  Expected: errors only in `CollectionOverviewTab.tsx` where `description` and `readme` are still referenced — those are fixed in Task 5.

- [ ] **Step 3: Commit**

  ```bash
  git add src/lib/tauri-api.ts
  git commit -m "fix(api): update CollectionSettings interface — replace description/readme with docs"
  ```

---

### Task 3: Update CollectionSection type

**Files:**
- Modify: `src/types/pane-types.ts`

**Context:** Remove `'readme'` and `'tags'`, add `'documentation'`. The `validSections` guard in `CollectionOverviewTab` already falls back unknown stored values to `'overview'`, so users with `'readme'` or `'tags'` persisted will land on Overview automatically.

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
  cd /Users/snehaldangroshiya/data/rocket && yarn tsc --noEmit 2>&1 | head -20
  ```

  Expected: errors only in `CollectionOverviewTab.tsx` — fixed in Task 5.

- [ ] **Step 3: Commit**

  ```bash
  git add src/types/pane-types.ts
  git commit -m "feat(types): update CollectionSection — remove readme/tags, add documentation"
  ```

---

### Task 4: Upgrade MarkdownEditor to Documentation card style

**Files:**
- Modify: `src/components/collections/MarkdownEditor.tsx`
- Create: `src/components/collections/MarkdownEditor.test.tsx`

**Context:** The current `MarkdownEditor` uses raw `<button>` tabs, a shadcn `Textarea`, and a prose div. It needs to become a full Card: `CardHeader` with FileText icon + "Documentation" label on left and shadcn `Tabs` on right, `CardContent` with a monospace `<textarea>` in edit mode with an optional save-on-blur footer, and a ReactMarkdown preview with empty state. New props `mode`, `onModeChange`, `onSave`, `saveState`, `isDirty` are optional — when `onSave` is absent the footer is not rendered; when `mode`/`onModeChange` are absent the component manages its own mode state internally.

- [ ] **Step 1: Write tests**

  Create `src/components/collections/MarkdownEditor.test.tsx`:

  ```tsx
  import { render, screen } from '@testing-library/react';
  import userEvent from '@testing-library/user-event';
  import { describe, expect, it, vi } from 'vitest';
  import { MarkdownEditor } from './MarkdownEditor';

  const baseProps = { value: '', onChange: vi.fn() };

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
      render(<MarkdownEditor {...baseProps} mode='edit' onSave={vi.fn()} saveState='idle' isDirty={false} />);
      expect(screen.getByRole('button', { name: /save/i })).toBeInTheDocument();
    });

    it('save button disabled when isDirty is false', () => {
      render(<MarkdownEditor {...baseProps} mode='edit' onSave={vi.fn()} saveState='idle' isDirty={false} />);
      expect(screen.getByRole('button', { name: /save/i })).toBeDisabled();
    });

    it('save button enabled when isDirty is true', () => {
      render(<MarkdownEditor {...baseProps} mode='edit' onSave={vi.fn()} saveState='idle' isDirty={true} />);
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

  Expected: multiple failures.

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

  Expected: errors only in `CollectionOverviewTab.tsx` (fixed in Task 5) and `WorkspaceOverviewTab.tsx` (fixed in Task 5 also, after Task 5).

- [ ] **Step 6: Commit**

  ```bash
  git add src/components/collections/MarkdownEditor.tsx src/components/collections/MarkdownEditor.test.tsx
  git commit -m "feat(markdown-editor): upgrade to Documentation card style with optional save/mode props"
  ```

---

### Task 5: Replace inline Documentation card in WorkspaceOverviewTab with MarkdownEditor

**Files:**
- Modify: `src/components/workspace/WorkspaceOverviewTab.tsx`

**Context:** `WorkspaceOverviewTab` has ~90 lines of inline Documentation card JSX (lines 352–442). Replace with `<MarkdownEditor>`. The workspace uses `docs` field already (`workspace.description` maps to `docs:` in workspace.yml — this is already correct and unchanged). Existing local state names `docMode`, `docContent`, `isDocDirty`, `saveDocState`, `triggerSaveDoc` map directly to the new props.

- [ ] **Step 1: Add MarkdownEditor import and remove now-unused imports**

  In `src/components/workspace/WorkspaceOverviewTab.tsx`, add:

  ```tsx
  import { MarkdownEditor } from '@/components/collections/MarkdownEditor';
  ```

  Remove these imports now only used inside `MarkdownEditor`:
  - `Check`, `FileText`, `Loader2`, `Save` from lucide-react
  - `ReactMarkdown` from react-markdown
  - `remarkGfm` from remark-gfm
  - `CardHeader` from `@/components/ui/card`
  - `Tabs`, `TabsList`, `TabsTrigger` from `@/components/ui/tabs`

  Remaining lucide-react import:
  ```tsx
  import { Box, ExternalLink, FolderOpen, MoreHorizontal, Plus, Trash2, Upload } from 'lucide-react';
  ```

- [ ] **Step 2: Replace the right column JSX**

  Find `{/* ── RIGHT COLUMN — Documentation ── */}` (line ~351). Replace the entire block with:

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

  Expected: errors only in `CollectionOverviewTab.tsx` (Task 6).

- [ ] **Step 4: Commit**

  ```bash
  git add src/components/workspace/WorkspaceOverviewTab.tsx
  git commit -m "refactor(workspace-overview): use shared MarkdownEditor for Documentation panel"
  ```

---

### Task 6: Refactor CollectionOverviewTab — use docs field, two-column overview, Documentation tab

**Files:**
- Modify: `src/components/collections/CollectionOverviewTab.tsx`

**Context:** This is the main UI refactor. The `readme` state variable is renamed to `docs` throughout to match the updated `CollectionSettings.docs` field. Changes: (1) rename `readme`→`docs` in all state/handlers; (2) update `TABS` and `validSections`; (3) add `docMode` state, `saveDocFn`, second `useSaveButton`, `persistedDocs`, `isDocDirty`; (4) reset `docMode` on collection load; (5) two-column overview section with Tags card; (6) replace Readme+Tags tabs with Documentation tab.

- [ ] **Step 1: Rename readme state to docs throughout**

  Find every occurrence of `readme` in the component body (not imports) and rename to `docs`:

  - `const [readme, setReadme] = useState('');` → `const [docs, setDocs] = useState('');`
  - `setReadme(s.readme ?? '');` → `setDocs(s.docs ?? '');`
  - `readme: readme || undefined,` in `saveSettings` → `docs: docs || undefined,`
  - The `saveSettings` dependency array: `description, readme` → `docs` (and remove `description`)
  - Remove the `const [description, setDescription] = useState('');` line entirely
  - Remove `setDescription(s.description ?? '');` from the load effect
  - Remove `description: description || undefined,` from `saveSettings`

- [ ] **Step 2: Update TABS and validSections**

  Find the `TABS` constant:
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

  Find `validSections`:
  ```tsx
  const validSections: CollectionSection[] = ['overview', 'auth', 'variables', 'readme', 'tags'];
  ```

  Replace with:
  ```tsx
  const validSections: CollectionSection[] = ['overview', 'auth', 'variables', 'documentation'];
  ```

- [ ] **Step 3: Add docMode state and second save hook**

  After `const [docs, setDocs] = useState('');`, add:
  ```tsx
  const [docMode, setDocMode] = useState<'edit' | 'preview'>('preview');
  ```

  After `const { state: saveState, trigger: triggerSave } = useSaveButton(saveSettings, 'Failed to save settings');`, add:
  ```tsx
  const saveDocFn = useCallback(async () => {
    await saveCollectionSettings(collectionName, {
      docs: docs.trim() || undefined,
    });
  }, [collectionName, docs]);

  const { state: saveDocState, trigger: triggerSaveDoc } = useSaveButton(
    saveDocFn,
    'Failed to save documentation',
  );
  ```

- [ ] **Step 4: Add persistedDocs and isDocDirty**

  After `const statsLine = ...` (after the loading/error early returns), add:
  ```tsx
  const persistedDocs = collection.settings.docs ?? '';
  const isDocDirty = docs !== persistedDocs;
  ```

- [ ] **Step 5: Reset docMode on collection load**

  In the load `useEffect`, add `setDocMode('preview');` immediately after `setLoading(true);`:
  ```tsx
  useEffect(() => {
    setLoading(true);
    setDocMode('preview');
    setIsLoaded(false);
    ...
  ```

- [ ] **Step 6: Replace the entire tab content section**

  Replace the block from `<div ref={scrollContainerRef} ...>` through its closing `</div>` with:

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
            value={docs}
            onChange={setDocs}
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
          {/* Authorization tab — keep existing JSX exactly as-is */}
          {activeSection === 'auth' && (
            /* PASTE EXISTING AUTH TAB CONTENT HERE — NO CHANGES */
          )}

          {/* Variables tab — keep existing JSX exactly as-is */}
          {activeSection === 'variables' && (
            /* PASTE EXISTING VARIABLES TAB CONTENT HERE — NO CHANGES */
          )}

          {/* Documentation tab — full-page editor */}
          {activeSection === 'documentation' && (
            <div className='h-full flex flex-col'>
              <MarkdownEditor
                value={docs}
                onChange={(v) => {
                  setDocs(v);
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

  **Important:** Copy the existing auth and variables tab JSX blocks verbatim. Remove the `{activeSection === 'readme' && ...}` and `{activeSection === 'tags' && ...}` blocks entirely.

- [ ] **Step 7: Verify TypeScript compiles clean**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn tsc --noEmit 2>&1 | head -30
  ```

  Expected: 0 errors.

- [ ] **Step 8: Run linter**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn check 2>&1 | grep "error" | head -20
  ```

  Expected: 0 errors.

- [ ] **Step 9: Run all tests**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn test 2>&1 | tail -15
  ```

  Expected: all pass.

- [ ] **Step 10: Commit**

  ```bash
  git add src/components/collections/CollectionOverviewTab.tsx
  git commit -m "feat(collection-overview): two-column overview, Documentation tab, docs field, remove Readme/Tags tabs"
  ```

---

### Task 7: Manual smoke test

**Files:** None — validation only.

- [ ] **Step 1: Start dev server**

  ```bash
  cd /Users/snehaldangroshiya/data/rocket && yarn dev
  ```

- [ ] **Step 2: Verify documentation saves to docs: in the YML**

  Open any collection → Overview tab → Documentation panel (right column) → Edit → type `# Hello` → blur.

  Then inspect the file on disk:

  ```bash
  find ~/.rocket-api -name "opencollection.yml" | head -3 | xargs grep -l "Hello" 2>/dev/null | head -1 | xargs cat
  ```

  Expected: the file contains `docs: "# Hello"` (or a multiline block scalar) — NOT `readme:`.

- [ ] **Step 3: Verify Workspace Overview tab unchanged**

  Open the Workspace Overview tab. Documentation card looks identical, saves correctly.

- [ ] **Step 4: Verify Collection Overview tab — two-column layout**

  Open any collection → Overview tab.

  Expected:
  - Tab bar: Overview · Authorization · Variables · Documentation
  - Left column: MethodBreakdown, Default Headers, Requests, Tags cards
  - Right column (320px): Documentation panel (MarkdownEditor)

- [ ] **Step 5: Verify Documentation tab full-page**

  Click Documentation tab — full-page MarkdownEditor, same content as right-column panel, saves correctly.

- [ ] **Step 6: Verify Authorization and Variables tabs work**

  Click each — content unchanged, save works.

- [ ] **Step 7: Verify Bruno interop**

  If a Bruno collection with a `docs:` field is available, open it in Rocket and confirm the documentation content appears in the Documentation panel.
