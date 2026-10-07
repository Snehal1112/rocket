# Folder settings, Plan 11: Script and Test sub-tabs (frontend)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Script and Test sub-tabs of the Folder Settings tab edit `FolderSettings.preRequestScript`, `postResponseScript` and `testsScript` with the same Monaco editors, rok IntelliSense and snippet sidebar that request tabs use, and show a one-sentence hint about execution order.

**Architecture:** `ScriptsTab` is reused, not duplicated. It gains two optional props: `phases?: ScriptPhase[]` (default all three) limits the visible phase tabs, and `agentAssist?: boolean` (default `true`) hides the AI Assist button and panel. `ScriptSection` passes `['pre-request', 'post-response']` and `TestSection` passes `['tests']`. Each section keys its `ScriptsTab` on a per-folder-tab id so a reused component instance can never carry editor refs, the active phase or sidebar state from one folder tab to another. A small `FolderScriptHint` component renders the static order hint.

**Tech Stack:** React + TypeScript, shadcn/ui `Tabs`, lucide-react, Monaco via `MonacoWrapper`, Vitest + Testing Library. Run frontend checks with `yarn`.

**Spec:** `docs/superpowers/specs/2026-10-07-folder-settings-design.md` (Script and Test sections, runtime order table). Index and locked contract: `docs/superpowers/plans/folder-settings/00-plan-index.md`. Depends on plan 09 (the section props contract and `useFolderSettings`) and plan 08 (placeholder `ScriptSection.tsx` and `TestSection.tsx`, which this plan overwrites).

## Global Constraints

- Each section file under `src/components/collections/folder-settings/` is a component taking `{ collectionName: string; folderPath: string; settings: FolderSettings; onChange: (patch: Partial<FolderSettings>) => void }` (plan 09 contract). Do not change that signature.
- Reuse `ScriptsTab`; do not copy its editor stack. Request-tab behavior must stay byte-for-byte the same when the new props are omitted. `RequestPanel.tsx` is not modified.
- Do not touch `MonacoWrapper`. The rok typings are registered by `MonacoWrapper` per `phase` under the key `ts:rok-${phase}.d.ts`. That per-phase key is critical (a shared key made phases clobber each other, see the Scripts IntelliSense project note). `ScriptsTab` must keep passing `phase` to every editor.
- Findings that shaped this plan, verified in the source:
  - `MonacoWrapper` does not pass a `path` to `@monaco-editor/react`, so Monaco models are anonymous. `tabId` is therefore NOT a model URI; it only reaches `AgentChatPanel`. The collision hazard is React instance reuse (state in `ScriptsTab`: `editorRefs`, `activeTab`, `snippetSidebars`), which is solved by `key`.
  - `AgentChatPanel` drives `beginAgentSession`, `activateAgentSession` and friends in `pane-store.ts`. Every one of them guards on `isRequestTab(tab)`, so for a folder tab they are silent no-ops and the panel would sit on "starting" forever. The AI Assist coupling is therefore hidden for folders via `agentAssist={false}`.
  - Radix `TabsContent` unmounts inactive phases, so only one Monaco per phase exists inside one `ScriptsTab`. The global `extraLib` key is shared across mounted editors of the same phase, which is an existing property of the app (two request tabs behave the same); this plan does not change it.
- Hard rules: shadcn/ui primitives only (no raw `<button>`, `<input>`, `<form>`), lucide-react icons only, Monaco for multi-line editors, narrow Zustand selectors (this plan uses no store).
- Code comments are short full sentences ending in a punctuation mark.
- Never `git add -A` or `git add .`. Commit with the `dev-workflow-skills:1-git-commit` skill, explicit paths, pathspec commit, conventional message. Peer sessions share this repo's index.
- Frontend tasks end with `yarn tsc --noEmit` and `yarn check`.

## Review Focus

1. With no `phases` prop, `ScriptsTab` renders Pre Request, Post Response and Tests exactly as before (Task 1 test `renders all three phase tabs by default`, plus the untouched existing `ScriptsTab.test.tsx` suite).
2. With `phases={['tests']}` only the Tests tab exists and the Tests editor is the initial one; with `['pre-request','post-response']` Tests is absent and Pre Request is initial (Task 1 tests `phases limits the visible phase tabs`, `starts on the first allowed phase`).
3. Each editor keeps its own `phase` prop so the per-phase rok typing key is intact (Task 1 test `passes the phase to every editor`).
4. `agentAssist={false}` removes the AI Assist button and panel; the default keeps them (Task 1 tests `agentAssist false hides AI Assist`, existing AI Assist tests).
5. Editing in the Script section calls `onChange` with exactly `{ preRequestScript }` or `{ postResponseScript }`; the Test section calls it with `{ testsScript }` (Task 2 tests `ScriptSection writes pre-request and post-response patches`, `TestSection writes the tests patch`).
6. Switching from one folder tab to another does not leak the active phase or editor value from the first (Task 2 test `a different folder gets a fresh ScriptsTab instance`).
7. Folder editors never show the AI Assist panel (Task 2 test `folder sections hide AI Assist`).
8. The execution-order hint renders, with an Info icon, in both sections (Task 3 tests).

---

## Task 1: `phases` and `agentAssist` props on `ScriptsTab`

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src/components/request/ScriptsTab.tsx`
- Test: `src/components/request/__tests__/ScriptsTab.test.tsx`

**Interfaces:**
- Consumes: `ScriptPhase` from `@/components/editor/rok-types` (`'pre-request' | 'post-response' | 'tests'`, already imported in `ScriptsTab.tsx`).
- Produces: two new optional props on `ScriptsTabProps`:
  - `phases?: ScriptPhase[]` (default `ALL_PHASES`, which is `['pre-request', 'post-response', 'tests']`).
  - `agentAssist?: boolean` (default `true`).

- [ ] **Step 1: Write the failing tests**

Append to `src/components/request/__tests__/ScriptsTab.test.tsx` (the file already mocks `MonacoWrapper` with a `data-testid` of `monaco-${phase}` and mocks `../AgentChatPanel`). Add a render helper that accepts extra props and a new `describe` block:

```tsx
function renderWith(extra: Partial<ComponentProps<typeof ScriptsTab>> = {}) {
  return render(
    <ScriptsTab
      tabId='tab-1'
      collectionName='my-collection'
      preRequestScript=''
      postResponseScript=''
      testsScript=''
      onChangePreRequest={vi.fn()}
      onChangePostResponse={vi.fn()}
      onChangeTests={vi.fn()}
      {...extra}
    />,
  );
}

describe('ScriptsTab phases and agentAssist props', () => {
  it('renders all three phase tabs by default', async () => {
    renderWith();
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    expect(screen.getByRole('tab', { name: 'Pre Request' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Post Response' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Tests' })).toBeInTheDocument();
  });

  it('phases limits the visible phase tabs', async () => {
    renderWith({ phases: ['pre-request', 'post-response'] });
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    expect(screen.getByRole('tab', { name: 'Pre Request' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Post Response' })).toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: 'Tests' })).not.toBeInTheDocument();
  });

  it('starts on the first allowed phase', async () => {
    renderWith({ phases: ['tests'] });
    await waitFor(() => expect(screen.getByTestId('monaco-tests')).toBeInTheDocument());
    expect(screen.queryByTestId('monaco-pre-request')).not.toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: 'Pre Request' })).not.toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Tests' })).toBeInTheDocument();
  });

  it('passes the phase to every editor', async () => {
    renderWith({ phases: ['pre-request', 'post-response'] });
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    fireEvent.mouseDown(screen.getByRole('tab', { name: 'Post Response' }));
    await waitFor(() => expect(screen.getByTestId('monaco-post-response')).toBeInTheDocument());
  });

  it('agentAssist false hides AI Assist', async () => {
    renderWith({ agentAssist: false });
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    expect(screen.queryByRole('button', { name: 'AI Assist' })).not.toBeInTheDocument();
    expect(screen.queryByTestId('agent-chat-panel')).not.toBeInTheDocument();
  });

  it('keeps AI Assist visible by default', async () => {
    renderWith();
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    expect(screen.getByRole('button', { name: 'AI Assist' })).toBeInTheDocument();
  });
});
```

Add `import type { ComponentProps } from 'react';` to the imports at the top of the test file.

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `yarn test src/components/request/__tests__/ScriptsTab.test.tsx`
Expected: FAIL. `phases limits the visible phase tabs`, `starts on the first allowed phase` and `agentAssist false hides AI Assist` fail (all tabs and the button still render; TypeScript also flags the unknown props).

- [ ] **Step 3: Implement the props in `ScriptsTab.tsx`**

In `src/components/request/ScriptsTab.tsx`:

1. Below `MIN_EDITOR_WIDTH`, add the default:

```tsx
const ALL_PHASES: ScriptPhase[] = ['pre-request', 'post-response', 'tests'];
```

2. In `ScriptsTabProps`, add after `agentSession?`:

```tsx
  /** Phase tabs to show. Defaults to all three. */
  phases?: ScriptPhase[];
  /** Shows the AI Assist button and panel. Defaults to true. */
  agentAssist?: boolean;
```

3. In the function signature destructuring, add `phases = ALL_PHASES,` and `agentAssist = true,` (after `agentSession,`).

4. Replace `useState<ScriptPhase>('pre-request')` with a lazy initial value and a derived safe phase:

```tsx
  const [selectedTab, setActiveTab] = useState<ScriptPhase>(phases[0] ?? 'pre-request');
  // Falls back to the first allowed phase if the selection is not in `phases`.
  const activeTab: ScriptPhase = phases.includes(selectedTab)
    ? selectedTab
    : (phases[0] ?? 'pre-request');
```

5. Wrap each trigger in a `phases.includes(...)` check. Replace the three `TabsTrigger` elements with:

```tsx
          {phases.includes('pre-request') && (
            <TabsTrigger value='pre-request' className='text-xs'>
              Pre Request
            </TabsTrigger>
          )}
          {phases.includes('post-response') && (
            <TabsTrigger value='post-response' className='text-xs'>
              Post Response
            </TabsTrigger>
          )}
          {phases.includes('tests') && (
            <TabsTrigger value='tests' className='text-xs'>
              Tests
            </TabsTrigger>
          )}
```

6. Wrap the AI Assist `Button` (the one with `aria-controls='agent-chat-panel'`) in `{agentAssist && ( ... )}` and change the panel condition from `{showAgentChat && (` to `{agentAssist && showAgentChat && (`.

7. Leave the three `TabsContent` blocks unchanged. Radix only mounts the active one, and the active value is always an allowed phase, so a hidden phase's editor never mounts and never registers its `ts:rok-*.d.ts` lib.

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `yarn test src/components/request/__tests__/ScriptsTab.test.tsx`
Expected: PASS, including every pre-existing test in the file.

- [ ] **Step 5: Type and lint checks**

Run: `yarn tsc --noEmit` then `yarn check`
Expected: both clean. `RequestPanel.tsx` needs no change because both new props are optional.

- [ ] **Step 6: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only:
`git add src/components/request/ScriptsTab.tsx src/components/request/__tests__/ScriptsTab.test.tsx`
Pathspec commit of those two paths with a conventional message such as `feat(scripts): let ScriptsTab limit phases and hide AI assist`.

---

## Task 2: `ScriptSection` and `TestSection` wiring

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create or overwrite: `src/components/collections/folder-settings/ScriptSection.tsx` (plan 08 left a placeholder; replace its whole content)
- Create or overwrite: `src/components/collections/folder-settings/TestSection.tsx` (same)
- Test: `src/components/collections/folder-settings/__tests__/ScriptTestSections.test.tsx` (create)

**Interfaces:**
- Consumes:
  - `ScriptsTab` with `phases` and `agentAssist` (Task 1), imported lazily from `@/components/request/ScriptsTab` (named export `ScriptsTab`).
  - `FolderSettings` from `@/lib/tauri-api` (plan 04). The fields used are `preRequestScript`, `postResponseScript`, `testsScript`, each a string or null/undefined; the sections read them with `?? ''`.
  - `EditorSkeleton` from `@/components/editor/EditorSkeleton`.
- Produces:
  - `ScriptSection(props: { collectionName: string; folderPath: string; settings: FolderSettings; onChange: (patch: Partial<FolderSettings>) => void }): JSX.Element`
  - `TestSection(props: same): JSX.Element`
  - Folder editor key (used as `ScriptsTab` `tabId` and React `key`): `folder-script:${collectionName}:${folderPath}` for Script and `folder-test:${collectionName}:${folderPath}` for Test. Folder tabs are unique per `(collectionName, folderPath)` (plan 08 dedups in `openFolderTab`), and the prefixes keep them apart from request tab ids (uuid-style) and from each other.

- [ ] **Step 1: Write the failing tests**

Create `src/components/collections/folder-settings/__tests__/ScriptTestSections.test.tsx`. It mocks `ScriptsTab` so the test checks wiring, not Monaco:

```tsx
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import type { FolderSettings } from '@/lib/tauri-api';
import { ScriptSection } from '../ScriptSection';
import { TestSection } from '../TestSection';

vi.mock('@/components/request/ScriptsTab', () => ({
  ScriptsTab: (props: {
    tabId: string;
    phases?: string[];
    agentAssist?: boolean;
    preRequestScript: string;
    postResponseScript: string;
    testsScript: string;
    onChangePreRequest: (v: string) => void;
    onChangePostResponse: (v: string) => void;
    onChangeTests: (v: string) => void;
  }) => {
    // Local state proves whether React reused or recreated the instance.
    const [phase, setPhase] = useState('initial');
    return (
      <div
        data-testid='scripts-tab'
        data-tab-id={props.tabId}
        data-phases={(props.phases ?? []).join(',')}
        data-agent-assist={String(props.agentAssist)}
        data-local={phase}
        data-pre={props.preRequestScript}
        data-post={props.postResponseScript}
        data-tests={props.testsScript}
      >
        <button type='button' onClick={() => props.onChangePreRequest('pre!')}>
          edit-pre
        </button>
        <button type='button' onClick={() => props.onChangePostResponse('post!')}>
          edit-post
        </button>
        <button type='button' onClick={() => props.onChangeTests('tests!')}>
          edit-tests
        </button>
        <button type='button' onClick={() => setPhase('changed')}>
          mutate-local
        </button>
      </div>
    );
  },
}));

const base: FolderSettings = {
  headers: [],
  auth: null,
  variables: [],
  preRequestScript: 'a',
  postResponseScript: 'b',
  testsScript: 'c',
  docs: null,
};

function sectionProps(over: { collectionName?: string; folderPath?: string } = {}) {
  return {
    collectionName: over.collectionName ?? 'col',
    folderPath: over.folderPath ?? 'users',
    settings: base,
    onChange: vi.fn(),
  };
}

describe('folder Script and Test sections', () => {
  it('ScriptSection writes pre-request and post-response patches', async () => {
    const props = sectionProps();
    render(<ScriptSection {...props} />);
    const tab = await screen.findByTestId('scripts-tab');
    expect(tab.dataset.phases).toBe('pre-request,post-response');
    expect(tab.dataset.pre).toBe('a');
    expect(tab.dataset.post).toBe('b');
    fireEvent.click(screen.getByText('edit-pre'));
    expect(props.onChange).toHaveBeenLastCalledWith({ preRequestScript: 'pre!' });
    fireEvent.click(screen.getByText('edit-post'));
    expect(props.onChange).toHaveBeenLastCalledWith({ postResponseScript: 'post!' });
  });

  it('TestSection writes the tests patch', async () => {
    const props = sectionProps();
    render(<TestSection {...props} />);
    const tab = await screen.findByTestId('scripts-tab');
    expect(tab.dataset.phases).toBe('tests');
    expect(tab.dataset.tests).toBe('c');
    fireEvent.click(screen.getByText('edit-tests'));
    expect(props.onChange).toHaveBeenLastCalledWith({ testsScript: 'tests!' });
  });

  it('treats missing scripts as empty strings', async () => {
    const props = {
      ...sectionProps(),
      settings: { ...base, preRequestScript: null, postResponseScript: undefined, testsScript: null },
    } as unknown as ReturnType<typeof sectionProps>;
    render(<ScriptSection {...props} />);
    const tab = await screen.findByTestId('scripts-tab');
    expect(tab.dataset.pre).toBe('');
    expect(tab.dataset.post).toBe('');
  });

  it('folder sections hide AI Assist', async () => {
    const { unmount } = render(<ScriptSection {...sectionProps()} />);
    expect((await screen.findByTestId('scripts-tab')).dataset.agentAssist).toBe('false');
    unmount();
    render(<TestSection {...sectionProps()} />);
    expect((await screen.findByTestId('scripts-tab')).dataset.agentAssist).toBe('false');
  });

  it('gives each folder and section its own editor key', async () => {
    const a = render(<ScriptSection {...sectionProps({ folderPath: 'users' })} />);
    const idA = (await screen.findByTestId('scripts-tab')).dataset.tabId;
    a.unmount();
    const b = render(<ScriptSection {...sectionProps({ folderPath: 'orders' })} />);
    const idB = (await screen.findByTestId('scripts-tab')).dataset.tabId;
    b.unmount();
    render(<TestSection {...sectionProps({ folderPath: 'users' })} />);
    const idC = (await screen.findByTestId('scripts-tab')).dataset.tabId;
    expect(idA).toBe('folder-script:col:users');
    expect(idB).toBe('folder-script:col:orders');
    expect(idC).toBe('folder-test:col:users');
  });

  it('a different folder gets a fresh ScriptsTab instance', async () => {
    const { rerender } = render(<ScriptSection {...sectionProps({ folderPath: 'users' })} />);
    const first = await screen.findByTestId('scripts-tab');
    fireEvent.click(screen.getByText('mutate-local'));
    await waitFor(() => expect(first.dataset.local).toBe('changed'));
    // Same element position, different folder: the key must remount the editor stack.
    rerender(<ScriptSection {...sectionProps({ folderPath: 'orders' })} />);
    const second = await screen.findByTestId('scripts-tab');
    expect(second.dataset.tabId).toBe('folder-script:col:orders');
    expect(second.dataset.local).toBe('initial');
  });
});
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `yarn test src/components/collections/folder-settings/__tests__/ScriptTestSections.test.tsx`
Expected: FAIL. The plan 08 placeholders render no `scripts-tab`, so every `findByTestId` times out.

- [ ] **Step 3: Implement `ScriptSection.tsx`**

Overwrite `src/components/collections/folder-settings/ScriptSection.tsx`:

```tsx
import { lazy, Suspense } from 'react';
import { EditorSkeleton } from '@/components/editor/EditorSkeleton';
import type { FolderSettings } from '@/lib/tauri-api';

// Lazy so the Monaco and rok type chain loads only when a Script section opens.
const ScriptsTab = lazy(() =>
  import('@/components/request/ScriptsTab').then((m) => ({ default: m.ScriptsTab })),
);

interface ScriptSectionProps {
  collectionName: string;
  folderPath: string;
  settings: FolderSettings;
  onChange: (patch: Partial<FolderSettings>) => void;
}

export function ScriptSection({
  collectionName,
  folderPath,
  settings,
  onChange,
}: ScriptSectionProps) {
  // The key remounts the editor stack per folder so refs and phase state never leak across tabs.
  const editorKey = `folder-script:${collectionName}:${folderPath}`;
  return (
    <div className='flex h-full min-h-0 flex-col'>
      <div className='min-h-0 flex-1 overflow-hidden'>
        <Suspense fallback={<EditorSkeleton />}>
          <ScriptsTab
            key={editorKey}
            tabId={editorKey}
            collectionName={collectionName}
            phases={['pre-request', 'post-response']}
            agentAssist={false}
            preRequestScript={settings.preRequestScript ?? ''}
            postResponseScript={settings.postResponseScript ?? ''}
            testsScript={settings.testsScript ?? ''}
            onChangePreRequest={(v) => onChange({ preRequestScript: v })}
            onChangePostResponse={(v) => onChange({ postResponseScript: v })}
            onChangeTests={(v) => onChange({ testsScript: v })}
          />
        </Suspense>
      </div>
    </div>
  );
}
```

`onChangeTests` is required by the prop type but never fires here because the Tests phase is hidden; it still maps to the right field so the component stays correct if `phases` changes.

- [ ] **Step 4: Implement `TestSection.tsx`**

Overwrite `src/components/collections/folder-settings/TestSection.tsx` with the same shape, changed in four places: component name `TestSection`, `editorKey` prefix `folder-test:`, `phases={['tests']}`, and the same three `onChange*` mappings:

```tsx
import { lazy, Suspense } from 'react';
import { EditorSkeleton } from '@/components/editor/EditorSkeleton';
import type { FolderSettings } from '@/lib/tauri-api';

// Lazy so the Monaco and rok type chain loads only when a Test section opens.
const ScriptsTab = lazy(() =>
  import('@/components/request/ScriptsTab').then((m) => ({ default: m.ScriptsTab })),
);

interface TestSectionProps {
  collectionName: string;
  folderPath: string;
  settings: FolderSettings;
  onChange: (patch: Partial<FolderSettings>) => void;
}

export function TestSection({ collectionName, folderPath, settings, onChange }: TestSectionProps) {
  // The key remounts the editor stack per folder so refs and phase state never leak across tabs.
  const editorKey = `folder-test:${collectionName}:${folderPath}`;
  return (
    <div className='flex h-full min-h-0 flex-col'>
      <div className='min-h-0 flex-1 overflow-hidden'>
        <Suspense fallback={<EditorSkeleton />}>
          <ScriptsTab
            key={editorKey}
            tabId={editorKey}
            collectionName={collectionName}
            phases={['tests']}
            agentAssist={false}
            preRequestScript={settings.preRequestScript ?? ''}
            postResponseScript={settings.postResponseScript ?? ''}
            testsScript={settings.testsScript ?? ''}
            onChangePreRequest={(v) => onChange({ preRequestScript: v })}
            onChangePostResponse={(v) => onChange({ postResponseScript: v })}
            onChangeTests={(v) => onChange({ testsScript: v })}
          />
        </Suspense>
      </div>
    </div>
  );
}
```

- [ ] **Step 5: Run the tests and confirm they pass**

Run: `yarn test src/components/collections/folder-settings/__tests__/ScriptTestSections.test.tsx`
Expected: PASS (6 tests). If plan 09's `FolderSettings` TS type marks the script fields as non-nullable `string`, the `?? ''` still compiles; the cast in `treats missing scripts as empty strings` already covers both shapes.

- [ ] **Step 6: Type and lint checks**

Run: `yarn tsc --noEmit` then `yarn check`
Expected: both clean. If `FolderSettings` in `tauri-api.ts` names the script fields differently from the locked camelCase `preRequestScript`, `postResponseScript`, `testsScript`, stop and fix the contract in the index rather than renaming here.

- [ ] **Step 7: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only:
`git add src/components/collections/folder-settings/ScriptSection.tsx src/components/collections/folder-settings/TestSection.tsx src/components/collections/folder-settings/__tests__/ScriptTestSections.test.tsx`
Pathspec commit of those paths, message such as `feat(folder-settings): wire Script and Test sub-tabs to ScriptsTab`.

---

## Task 3: Execution-order hint

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `src/components/collections/folder-settings/FolderScriptHint.tsx`
- Modify: `src/components/collections/folder-settings/ScriptSection.tsx`, `src/components/collections/folder-settings/TestSection.tsx`
- Test: `src/components/collections/folder-settings/__tests__/FolderScriptHint.test.tsx` (create); extend `ScriptTestSections.test.tsx`

**Interfaces:**
- Consumes: `Info` from `lucide-react`; `cn` is not needed.
- Produces: `FolderScriptHint(props: { kind: 'script' | 'test' }): JSX.Element`. It renders one static sentence with an `Info` icon (`aria-hidden`), wrapped in an element with `data-testid='folder-script-hint'` and `role='note'`.
  - `script`: `Folder scripts use the rok API. By default the folder pre-request script runs before the request's, and post-response scripts run after it, innermost folder first.`
  - `test`: `Folder scripts use the rok API. By default folder tests run after the request's tests, innermost folder first.`

The text states the sandwich default only. The collection-level `extensions.bruno.scripts.flow` switch (plan 03) is not described here on purpose, to keep the hint a single static sentence.

- [ ] **Step 1: Write the failing tests**

Create `src/components/collections/folder-settings/__tests__/FolderScriptHint.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { FolderScriptHint } from '../FolderScriptHint';

describe('FolderScriptHint', () => {
  it('renders the script order sentence and an icon', () => {
    const { container } = render(<FolderScriptHint kind='script' />);
    const hint = screen.getByTestId('folder-script-hint');
    expect(hint).toHaveTextContent('rok API');
    expect(hint).toHaveTextContent('pre-request script runs before the request');
    expect(hint).toHaveTextContent('innermost folder first');
    expect(container.querySelector('svg')).not.toBeNull();
  });

  it('renders the test order sentence', () => {
    render(<FolderScriptHint kind='test' />);
    const hint = screen.getByTestId('folder-script-hint');
    expect(hint).toHaveTextContent('rok API');
    expect(hint).toHaveTextContent('folder tests run after the request');
  });
});
```

Append to `ScriptTestSections.test.tsx`:

```tsx
describe('folder sections show the order hint', () => {
  it('ScriptSection renders the hint', async () => {
    render(<ScriptSection {...sectionProps()} />);
    await screen.findByTestId('scripts-tab');
    expect(screen.getByTestId('folder-script-hint')).toBeInTheDocument();
  });

  it('TestSection renders the hint', async () => {
    render(<TestSection {...sectionProps()} />);
    await screen.findByTestId('scripts-tab');
    expect(screen.getByTestId('folder-script-hint')).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `yarn test src/components/collections/folder-settings/__tests__/FolderScriptHint.test.tsx src/components/collections/folder-settings/__tests__/ScriptTestSections.test.tsx`
Expected: FAIL (module `../FolderScriptHint` not found, and no hint in the sections).

- [ ] **Step 3: Implement the hint component**

Create `src/components/collections/folder-settings/FolderScriptHint.tsx`:

```tsx
import { Info } from 'lucide-react';

const HINT_TEXT = {
  script:
    "Folder scripts use the rok API. By default the folder pre-request script runs before the request's, and post-response scripts run after it, innermost folder first.",
  test: "Folder scripts use the rok API. By default folder tests run after the request's tests, innermost folder first.",
} as const;

interface FolderScriptHintProps {
  kind: keyof typeof HINT_TEXT;
}

export function FolderScriptHint({ kind }: FolderScriptHintProps) {
  return (
    <div
      role='note'
      data-testid='folder-script-hint'
      className='flex shrink-0 items-start gap-2 border-t px-3 py-2 text-xs text-muted-foreground'
    >
      <Info className='mt-0.5 h-3.5 w-3.5 shrink-0' aria-hidden='true' />
      <p>{HINT_TEXT[kind]}</p>
    </div>
  );
}
```

Check the assertion strings against the text: the `script` text contains `pre-request script runs before the request` ("the folder pre-request script runs before the request's"), and the `test` text contains `folder tests run after the request` ("folder tests run after the request's tests"). Both match as substrings.

- [ ] **Step 4: Render the hint in both sections**

In `ScriptSection.tsx`, add `import { FolderScriptHint } from './FolderScriptHint';` and place `<FolderScriptHint kind='script' />` as the last child of the outer `div` (after the `min-h-0 flex-1` wrapper). In `TestSection.tsx`, do the same with `kind='test'`. The hint is `shrink-0`, so the editor wrapper keeps the remaining height.

- [ ] **Step 5: Run the tests and confirm they pass**

Run: `yarn test src/components/collections/folder-settings/__tests__ src/components/request/__tests__/ScriptsTab.test.tsx`
Expected: PASS for all folder-settings tests and the full `ScriptsTab` suite.

- [ ] **Step 6: Type and lint checks**

Run: `yarn tsc --noEmit` then `yarn check`
Expected: both clean. If Biome rewrites the long string literals' quoting, accept its formatting via `yarn lint` and re-run the tests.

- [ ] **Step 7: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only:
`git add src/components/collections/folder-settings/FolderScriptHint.tsx src/components/collections/folder-settings/ScriptSection.tsx src/components/collections/folder-settings/TestSection.tsx src/components/collections/folder-settings/__tests__/FolderScriptHint.test.tsx src/components/collections/folder-settings/__tests__/ScriptTestSections.test.tsx`
Pathspec commit of those paths, message such as `feat(folder-settings): explain folder script execution order`.

---

## Next Plan

**Execution order:** this is plan 11 of 12. The series runs strictly one plan at a time, in numeric order (see [00-plan-index.md](00-plan-index.md)).

**Next to execute:** [Plan 12: Bruno compatibility verification and docs](2026-10-07-folder-settings-plan-12-bruno-compat-verification-docs.md).

**Instruction for the executing Claude:** when every task in this plan is done and its verification commands (listed in the last step of each task) pass, record "Task N: complete" for each task in the ledger at `.superpowers/sdd/folder-settings/progress.md`, then open Plan 12 and start its first task immediately. Do not ask the user for consent between plans. Stop and report to the user instead if a verification command fails, if a plan's stated prerequisite is missing, or if you must deviate from the locked contract in the index.

**Notes for the next plan (from the plan author):**

`docs/superpowers/plans/folder-settings/2026-10-07-folder-settings-plan-12-bruno-compat-verification-docs.md` (Bruno compatibility verification and docs, depends on all earlier plans). Chain to it once this plan is complete. Plan 10 (Headers and Auth sub-tabs) is independent of this plan and may run before or after it.
