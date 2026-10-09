# Flow Save and Shortcuts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Ctrl+S save a flow tab, guard closing a dirty flow tab, and add Ctrl+Enter to run a flow, with Stop disabled when nothing is running.

**Architecture:** The global keyboard hook already dispatches `rocket:save-draft` for non-request tabs and `rocket:request-close-tab` for dirty script tabs. This plan adds a flow listener for save, extends the close guard to flow tabs, and adds a new `rocket:flow-run` event handled by `FlowToolbar`. All changes are frontend-only.

**Tech Stack:** React, TypeScript, Zustand (`pane-store`), Vitest and Testing Library, shadcn `AlertDialog`, `sonner` toasts.

**Spec:** Roadmap items F-07 and F-33 in `.claude/flow-roadmap.md`. Design notes: `docs/superpowers/plans/flow-tier3/00-plan-index.md` (plan P1).

## Global Constraints

- shadcn/ui primitives only, `lucide-react` icons only, no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`.
- Zustand: never fully destructure store state at component top level. Use narrow selectors.
- No `unwrap()` and no Rust changes in this plan.
- Code comments are short full sentences that end with a punctuation mark.
- Commits use conventional commit format and are created through the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only. Never `git add -A`, `--all` or `.`.
- Gates before each commit: `yarn tsc --noEmit`, `yarn check`, and the targeted `yarn test <pattern>` listed in the task.
- Work in the worktree or branch the human partner names. Only one implementer at a time touches `FlowPane.tsx` and `FlowToolbar.tsx`.
- Not in scope: a "Re-run" button or relabel (open decision D6 in the index), run-result strip (plan P2), undo, any backend change.

## Review Focus

Failure modes the spec implies but the obvious tests would miss, most likely first:

1. Ctrl+S on a flow picker tab (no flow chosen yet) must do nothing and must not crash. Test pinned in Task 1.
2. Two quick Ctrl+S presses must save once, not twice. Test pinned in Task 1.
3. A failed save from "Save and close" (validation error or disk error) must keep the tab open and the dialog open, and show an error. Test pinned in Task 2.
4. Ctrl+Enter pressed twice, or while a run or sign-in is in progress, must start exactly one run. Test pinned in Task 3.
5. Ctrl+Enter on a flow tab whose toolbar is not mounted (picker state), or a request tab, must not start a flow run. Tests pinned in Task 3.

---

## File Structure

| File | Responsibility |
|---|---|
| `src/lib/flow-save.ts` (new) | `flowPayloadFromTab(tab)`: builds the `save_flow` arguments from a flow tab. Shared by `FlowPane` and `EditorGroup`. |
| `src/components/flow/FlowSaveShortcut.tsx` (new) | Renders nothing. Listens for `rocket:save-draft` for one tab and runs the save callback, ignoring overlapping presses. |
| `src/components/flow/FlowPane.tsx` (modify) | Uses `flowPayloadFromTab`, mounts `FlowSaveShortcut`, toasts when Run auto-saves, passes `tabId` to `FlowToolbar`. |
| `src/components/panes/EditorGroup.tsx` (modify) | Flow wording in the unsaved dialog and a "Save and close" action for flows. |
| `src/hooks/useKeyboardShortcuts.ts` (modify) | Ctrl+W guard for dirty flow tabs. Ctrl+Enter dispatches `rocket:flow-run` for flow tabs. |
| `src/components/flow/FlowToolbar.tsx` (modify) | `tabId` prop, `rocket:flow-run` listener, Stop disabled when idle. |

Existing tests to know: `src/components/flow/__tests__/FlowPane.test.tsx` (store setup, `flowTab` fixture, mocks), `src/components/flow/__tests__/FlowToolbar.test.tsx` (`renderToolbar`, `started`, mocked handlers), `src/hooks/__tests__/useKeyboardShortcuts-close.test.tsx`, `src/components/panes/__tests__/EditorGroup-script-close.test.tsx`.

---

### Task 1: Ctrl+S saves a flow, and Run reports its auto-save

**Files:**
- Create: `src/lib/flow-save.ts`
- Create: `src/lib/__tests__/flow-save.test.ts`
- Create: `src/components/flow/FlowSaveShortcut.tsx`
- Create: `src/components/flow/__tests__/FlowPane.saveShortcut.test.tsx`
- Modify: `src/components/flow/FlowPane.tsx` (imports near line 20-33, `handleSave` at line 303, `handleBeforeRun` at line 334, JSX near line 371)

**Interfaces:**
- Produces: `flowPayloadFromTab(tab: FlowTab): { collection: string; flow: Flow } | null`. Returns null when `collectionName` or `flowName` is null.
- Produces: `<FlowSaveShortcut tabId={string} onSave={() => Promise<boolean>} isDirty={() => boolean} />`.

- [ ] **Step 1: Write the failing test for the payload helper**

Create `src/lib/__tests__/flow-save.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { FlowTab } from '@/types/pane-types';
import { flowPayloadFromTab } from '../flow-save';

const base: FlowTab = {
  id: 't1',
  title: 'Flow: my-flow',
  isDirty: true,
  tabType: 'flow',
  collectionName: 'demo',
  flowName: 'my-flow',
  nodes: [],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
};

describe('flowPayloadFromTab', () => {
  it('builds the save arguments from the tab', () => {
    expect(flowPayloadFromTab(base)).toEqual({
      collection: 'demo',
      flow: { name: 'my-flow', nodes: [], edges: [] },
    });
  });

  it('includes the callback host only when set', () => {
    const withHost = flowPayloadFromTab({ ...base, callbackHost: '10.0.0.5' });
    expect(withHost?.flow.callbackHost).toBe('10.0.0.5');
    const withNull = flowPayloadFromTab({ ...base, callbackHost: null });
    expect(withNull?.flow).not.toHaveProperty('callbackHost');
  });

  it('returns null for a picker tab', () => {
    expect(flowPayloadFromTab({ ...base, flowName: null })).toBeNull();
    expect(flowPayloadFromTab({ ...base, collectionName: null })).toBeNull();
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `yarn test src/lib/__tests__/flow-save.test.ts`
Expected: FAIL, cannot resolve `../flow-save`.

- [ ] **Step 3: Write the helper**

Create `src/lib/flow-save.ts`:

```ts
import type { Flow } from '@/lib/tauri-api';
import type { FlowTab } from '@/types/pane-types';

// Builds the `save_flow` arguments from a flow tab. Null while the tab is still a picker.
export function flowPayloadFromTab(tab: FlowTab): { collection: string; flow: Flow } | null {
  if (!tab.collectionName || !tab.flowName) return null;
  return {
    collection: tab.collectionName,
    flow: {
      name: tab.flowName,
      nodes: tab.nodes,
      edges: tab.edges,
      ...(tab.callbackHost ? { callbackHost: tab.callbackHost } : {}),
    },
  };
}
```

- [ ] **Step 4: Run it to verify it passes**

Run: `yarn test src/lib/__tests__/flow-save.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 5: Write the failing FlowPane tests**

Create `src/components/flow/__tests__/FlowPane.saveShortcut.test.tsx`:

```tsx
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  listCollections,
  listFlows,
  onFlowRunStarted,
  onFlowStepCompleted,
  onFlowStepProgress,
  onFlowStepStarted,
  runFlow,
  saveFlow,
} from '@/lib/tauri-api';
import { findTabInTree } from '@/lib/pane-utils';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    listCollections: vi.fn(),
    listFlows: vi.fn(),
    saveFlow: vi.fn(),
    runFlow: vi.fn(),
    onFlowRunStarted: vi.fn(),
    onFlowStepStarted: vi.fn(),
    onFlowStepCompleted: vi.fn(),
    onFlowStepProgress: vi.fn(),
  };
});
vi.mock('@/lib/execute-request', () => ({ getActiveGlobalEnvName: vi.fn() }));
vi.mock('@/lib/flow-auth-preflight', () => ({ collectFlowAuthTokens: vi.fn(async () => ({})) }));
vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() } }));

const outputNode = {
  id: 'a',
  kind: { kind: 'Output' as const, label: 'Out a' },
  position: { x: 0, y: 0 },
};

const flowTab = (over: Partial<FlowTab> = {}): FlowTab => ({
  id: 'flow-save-1',
  title: 'Flow: my-flow',
  isDirty: true,
  tabType: 'flow',
  collectionName: 'demo',
  flowName: 'my-flow',
  nodes: [outputNode],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
  ...over,
});

function pressSave(tabId: string) {
  act(() => {
    window.dispatchEvent(new CustomEvent('rocket:save-draft', { detail: { tabId } }));
  });
}

function storedTab(id: string): FlowTab | undefined {
  const found = findTabInTree(usePaneStore.getState().root, id);
  return found && isFlowTab(found.tab) ? found.tab : undefined;
}

function openAndRender(tab: FlowTab) {
  usePaneStore.getState().openTab(tab);
  return render(<FlowPane tab={tab} groupId={usePaneStore.getState().activeGroupId} />);
}

describe('FlowPane save shortcut', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.clearAllMocks();
    vi.mocked(listCollections).mockResolvedValue([]);
    vi.mocked(listFlows).mockResolvedValue([]);
    vi.mocked(saveFlow).mockResolvedValue(undefined);
  });

  it('saves a dirty flow when its tab gets rocket:save-draft', async () => {
    openAndRender(flowTab());
    pressSave('flow-save-1');
    await waitFor(() =>
      expect(saveFlow).toHaveBeenCalledWith('demo', {
        name: 'my-flow',
        nodes: [outputNode],
        edges: [],
      }),
    );
    await waitFor(() => expect(storedTab('flow-save-1')?.isDirty).toBe(false));
  });

  it('ignores the event for another tab', async () => {
    openAndRender(flowTab());
    pressSave('some-other-tab');
    await act(async () => {});
    expect(saveFlow).not.toHaveBeenCalled();
  });

  it('does nothing for a clean flow', async () => {
    openAndRender(flowTab({ isDirty: false }));
    pressSave('flow-save-1');
    await act(async () => {});
    expect(saveFlow).not.toHaveBeenCalled();
  });

  it('saves once when the shortcut is pressed twice quickly', async () => {
    let finish: () => void = () => undefined;
    vi.mocked(saveFlow).mockImplementation(
      () =>
        new Promise<undefined>((resolve) => {
          finish = () => resolve(undefined);
        }),
    );
    openAndRender(flowTab());
    pressSave('flow-save-1');
    pressSave('flow-save-1');
    await act(async () => {});
    expect(saveFlow).toHaveBeenCalledTimes(1);
    await act(async () => finish());
  });

  it('does nothing on a picker tab that has no flow yet', async () => {
    openAndRender(flowTab({ flowName: null, nodes: [] }));
    pressSave('flow-save-1');
    await act(async () => {});
    expect(saveFlow).not.toHaveBeenCalled();
  });
});

describe('FlowPane run auto-save', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.clearAllMocks();
    vi.mocked(listCollections).mockResolvedValue([]);
    vi.mocked(listFlows).mockResolvedValue([]);
    const unlisten = async () => () => undefined;
    vi.mocked(onFlowRunStarted).mockImplementation(unlisten);
    vi.mocked(onFlowStepStarted).mockImplementation(unlisten);
    vi.mocked(onFlowStepCompleted).mockImplementation(unlisten);
    vi.mocked(onFlowStepProgress).mockImplementation(unlisten);
    // Keep the run pending so the test ends mid-run.
    vi.mocked(runFlow).mockImplementation(() => new Promise(() => undefined));
  });

  it('tells the user when Run saved unsaved edits first', async () => {
    vi.mocked(saveFlow).mockResolvedValue(undefined);
    openAndRender(flowTab());
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(toast.info).toHaveBeenCalledWith('Flow saved before run.'));
    expect(runFlow).toHaveBeenCalled();
  });

  it('does not toast when the flow was already saved', async () => {
    openAndRender(flowTab({ isDirty: false }));
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(runFlow).toHaveBeenCalled());
    expect(toast.info).not.toHaveBeenCalled();
  });

  it('does not run or toast info when the auto-save fails', async () => {
    vi.mocked(saveFlow).mockRejectedValue('Invalid input: bad graph');
    openAndRender(flowTab());
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(toast.error).toHaveBeenCalled());
    expect(toast.info).not.toHaveBeenCalled();
    expect(runFlow).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 6: Run the tests to verify they fail**

Run: `yarn test src/components/flow/__tests__/FlowPane.saveShortcut.test.tsx`
Expected: the shortcut tests and the toast test FAIL (no listener, no toast). The "already saved" and "fails" toast tests may already pass.

- [ ] **Step 7: Create the shortcut component**

Create `src/components/flow/FlowSaveShortcut.tsx`:

```tsx
import { useEffect, useRef } from 'react';

interface FlowSaveShortcutProps {
  tabId: string;
  // Saves the flow. Resolves true on success.
  onSave: () => Promise<boolean>;
  // Reads the latest dirty state, since the listener outlives renders.
  isDirty: () => boolean;
}

// Renders nothing. The global Ctrl+S handler and the tab menu dispatch
// `rocket:save-draft` with a tab id, and this saves when the id is ours.
export function FlowSaveShortcut({ tabId, onSave, isDirty }: FlowSaveShortcutProps) {
  const onSaveRef = useRef(onSave);
  onSaveRef.current = onSave;
  const isDirtyRef = useRef(isDirty);
  isDirtyRef.current = isDirty;
  // Blocks a second save while one is still pending.
  const savingRef = useRef(false);

  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<{ tabId?: string }>).detail;
      if (detail?.tabId !== tabId || savingRef.current || !isDirtyRef.current()) return;
      savingRef.current = true;
      void onSaveRef.current().finally(() => {
        savingRef.current = false;
      });
    };
    window.addEventListener('rocket:save-draft', handler);
    return () => window.removeEventListener('rocket:save-draft', handler);
  }, [tabId]);

  return null;
}
```

- [ ] **Step 8: Wire it into FlowPane and add the toast**

In `src/components/flow/FlowPane.tsx`:

1. Add imports (keep imports sorted the way Biome expects):

```tsx
import { flowPayloadFromTab } from '@/lib/flow-save';
import { FlowSaveShortcut } from './FlowSaveShortcut';
```

2. In `handleSave`, replace the `await saveFlow(collectionName, {...})` call (lines 305-310) with:

```tsx
      const payload = flowPayloadFromTab(tab);
      if (!payload) return false;
      await saveFlow(payload.collection, payload.flow);
```

3. Replace `handleBeforeRun` (line 334) with:

```tsx
  const handleBeforeRun = async () => {
    if (!tab.isDirty) return true;
    const saved = await handleSave(true);
    if (saved) toast.info('Flow saved before run.');
    return saved;
  };
```

4. Inside the `<div ref={canvasAreaRef} className='relative h-full'>` element, as its first child, mount the shortcut:

```tsx
          <FlowSaveShortcut
            tabId={tab.id}
            onSave={() => handleSave()}
            isDirty={() => latestFlowTab()?.isDirty ?? false}
          />
```

`FlowSaveShortcut` is only mounted in the open-flow render path, because it sits after the picker early return. That is why it is a component and not a hook.

- [ ] **Step 9: Run the tests to verify they pass**

Run: `yarn test src/components/flow src/lib/__tests__/flow-save.test.ts`
Expected: PASS. The existing `FlowPane.test.tsx` save tests must still pass, since the `saveFlow` call shape is unchanged.

- [ ] **Step 10: Gates and commit**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/lib/flow-save.ts src/lib/__tests__/flow-save.test.ts src/components/flow/FlowSaveShortcut.tsx src/components/flow/FlowPane.tsx src/components/flow/__tests__/FlowPane.saveShortcut.test.tsx`
Suggested subject: `feat(flow): save with Ctrl+S and tell the user when Run auto-saves`.

---

### Task 2: Guard closing a dirty flow tab

**Files:**
- Modify: `src/hooks/useKeyboardShortcuts.ts:9,70`
- Modify: `src/components/panes/EditorGroup.tsx` (imports, `pendingScript` near line 188, `saveScriptAndClose` near line 190, dialog text near line 334, footer near line 349)
- Test: `src/hooks/__tests__/useKeyboardShortcuts-close.test.tsx` (extend)
- Create: `src/components/panes/__tests__/EditorGroup-flow-close.test.tsx`

**Interfaces:**
- Consumes: `flowPayloadFromTab` from Task 1.

- [ ] **Step 1: Write the failing Ctrl+W tests**

In `src/hooks/__tests__/useKeyboardShortcuts-close.test.tsx`, add the type import and a fixture, then two tests inside the existing `describe('Ctrl+W close guard', ...)` block:

```tsx
import type { FlowTab } from '@/types/pane-types';

const flowTab = (isDirty: boolean): FlowTab => ({
  id: 'flow-close-1',
  title: 'Flow: my-flow',
  isDirty,
  tabType: 'flow',
  collectionName: 'demo',
  flowName: 'my-flow',
  nodes: [],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
});
```

```tsx
  it('requests a guarded close for a dirty flow tab', () => {
    usePaneStore.getState().openTab(flowTab(true));
    renderHook(() => useKeyboardShortcuts(), { wrapper });
    pressCtrlW();
    expect(requested).toEqual(['flow-close-1']);
    expect(closeTab).not.toHaveBeenCalled();
  });

  it('closes a clean flow tab directly', () => {
    usePaneStore.getState().openTab(flowTab(false));
    renderHook(() => useKeyboardShortcuts(), { wrapper });
    pressCtrlW();
    expect(requested).toEqual([]);
    expect(closeTab).toHaveBeenCalledWith('flow-close-1', usePaneStore.getState().activeGroupId);
  });
```

- [ ] **Step 2: Run them to verify the dirty-flow test fails**

Run: `yarn test src/hooks/__tests__/useKeyboardShortcuts-close.test.tsx`
Expected: "requests a guarded close for a dirty flow tab" FAILS (the hook closes it directly).

- [ ] **Step 3: Extend the Ctrl+W guard**

In `src/hooks/useKeyboardShortcuts.ts`, change the import on line 9 to:

```ts
import { isFlowTab, isRequestTab, isScriptTab } from '@/types/pane-types';
```

and change the guard on line 70 and its comment to:

```ts
        if (tab && (isScriptTab(tab) || isFlowTab(tab)) && tab.isDirty) {
          // Script and flow tabs have no autosave, so the owning group must confirm first.
```

- [ ] **Step 4: Run to verify the tests pass**

Run: `yarn test src/hooks/__tests__/useKeyboardShortcuts-close.test.tsx`
Expected: PASS.

- [ ] **Step 5: Write the failing EditorGroup tests**

Create `src/components/panes/__tests__/EditorGroup-flow-close.test.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { findTabInTree } from '@/lib/pane-utils';
import { saveFlow } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { FlowTab } from '@/types/pane-types';
import { EditorGroup } from '../EditorGroup';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn(), saveFlow: vi.fn(), endAgentSession: vi.fn() };
});
vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() } }));
vi.mock('@/components/flow/FlowPane', () => ({ FlowPane: () => <div /> }));

const dirtyFlow: FlowTab = {
  id: 'flow-eg-1',
  title: 'Flow: my-flow',
  isDirty: true,
  tabType: 'flow',
  collectionName: 'demo',
  flowName: 'my-flow',
  nodes: [],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
};

function setup() {
  usePaneStore.getState().reset();
  usePaneStore.getState().openTab(dirtyFlow);
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('Expected leaf root');
  render(
    <QueryClientProvider client={new QueryClient()}>
      <EditorGroup node={root} />
    </QueryClientProvider>,
  );
  fireEvent.click(screen.getByLabelText(/close/i));
}

const tabStillOpen = () => findTabInTree(usePaneStore.getState().root, 'flow-eg-1') !== null;

describe('EditorGroup unsaved flow close dialog', () => {
  beforeEach(() => vi.clearAllMocks());

  it('uses flow wording', () => {
    setup();
    expect(screen.getByText(/This flow has unsaved changes/)).toBeInTheDocument();
  });

  it('saves the flow and then closes the tab', async () => {
    setup();
    vi.mocked(saveFlow).mockResolvedValue(undefined);
    fireEvent.click(screen.getByRole('button', { name: 'Save and close' }));
    await waitFor(() =>
      expect(saveFlow).toHaveBeenCalledWith('demo', { name: 'my-flow', nodes: [], edges: [] }),
    );
    await waitFor(() => expect(tabStillOpen()).toBe(false));
  });

  it('keeps the tab and the dialog open when saving fails', async () => {
    setup();
    vi.mocked(saveFlow).mockRejectedValue('Invalid input: flow contains a cycle');
    fireEvent.click(screen.getByRole('button', { name: 'Save and close' }));
    await waitFor(() => expect(toast.error).toHaveBeenCalled());
    expect(tabStillOpen()).toBe(true);
    expect(screen.getByRole('button', { name: 'Save and close' })).toBeInTheDocument();
  });

  it('Close discards the changes without saving', async () => {
    setup();
    fireEvent.click(screen.getByRole('button', { name: 'Close' }));
    await waitFor(() => expect(tabStillOpen()).toBe(false));
    expect(saveFlow).not.toHaveBeenCalled();
  });
});
```

Note: `screen.getByLabelText(/close/i)` matches the tab's close button, as in `EditorGroup-script-close.test.tsx`. If the dialog's own "Close" button also matches after it opens, the first `fireEvent` runs before the dialog opens, so it is safe.

- [ ] **Step 6: Run them to verify they fail**

Run: `yarn test src/components/panes/__tests__/EditorGroup-flow-close.test.tsx`
Expected: FAIL (wording is the request wording, no "Save and close" button for flows).

- [ ] **Step 7: Implement flow wording and Save and close**

In `src/components/panes/EditorGroup.tsx`:

1. Imports: add `FlowTab` to the type import from `@/types/pane-types` (next to `ScriptTab`), and add:

```tsx
import { flowPayloadFromTab } from '@/lib/flow-save';
```

and add `saveFlow` to the existing `@/lib/tauri-api` import that already brings in `saveScriptFile`.

2. After the `pendingScript` line (about line 188):

```tsx
  const pendingFlow = pendingTab && isFlowTab(pendingTab) ? pendingTab : null;
```

3. After `saveScriptAndClose` (about line 200):

```tsx
  // Returns true when the flow was saved and its tab closed.
  const saveFlowAndClose = async (tab: FlowTab): Promise<boolean> => {
    const payload = flowPayloadFromTab(tab);
    if (!payload) {
      toast.error(`Could not save "${tab.title}": no flow is open in this tab.`);
      return false;
    }
    try {
      await saveFlow(payload.collection, payload.flow);
      closeTab(tab.id, node.groupId);
      return true;
    } catch (err) {
      toast.error(
        `Could not save "${tab.title}": ${err instanceof Error ? err.message : String(err)}`,
      );
      return false;
    }
  };
```

4. In the dialog text IIFE, add before the script check:

```tsx
                if (found && isFlowTab(found)) {
                  return 'This flow has unsaved changes. Save them before closing?';
                }
```

5. In the footer, after the `pendingScript` action block, add:

```tsx
            {pendingFlow && (
              <AlertDialogAction
                onClick={(e) => {
                  // Keep the dialog open until the save finishes, and after it fails.
                  e.preventDefault();
                  void saveFlowAndClose(pendingFlow).then((saved) => {
                    if (saved) setPendingCloseTabId(null);
                  });
                }}
              >
                Save and close
              </AlertDialogAction>
            )}
```

- [ ] **Step 8: Run to verify the tests pass**

Run: `yarn test src/components/panes src/hooks`
Expected: PASS, including the existing script-close tests.

- [ ] **Step 9: Gates and commit**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/hooks/useKeyboardShortcuts.ts src/hooks/__tests__/useKeyboardShortcuts-close.test.tsx src/components/panes/EditorGroup.tsx src/components/panes/__tests__/EditorGroup-flow-close.test.tsx`
Suggested subject: `feat(flow): confirm before closing a dirty flow tab`.

---

### Task 3: Ctrl+Enter runs a flow, and Stop is disabled when idle

**Files:**
- Modify: `src/hooks/useKeyboardShortcuts.ts:9,29-49`
- Modify: `src/components/flow/FlowToolbar.tsx` (props near line 21, after `handleRun` near line 290, Stop button at line 308)
- Modify: `src/components/flow/FlowPane.tsx` (`<FlowToolbar ... tabId={tab.id}`)
- Test: `src/components/flow/__tests__/FlowToolbar.test.tsx` (extend, and change one existing test)
- Create: `src/hooks/__tests__/useKeyboardShortcuts-flow-run.test.tsx`

**Interfaces:**
- Produces: window event `rocket:flow-run` with `detail: { tabId: string }`.
- Produces: `FlowToolbar` prop `tabId?: string`. Without it the toolbar ignores the event.

- [ ] **Step 1: Write the failing hook tests**

Create `src/hooks/__tests__/useKeyboardShortcuts-flow-run.test.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook } from '@testing-library/react';
import type { ReactNode } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { sendRequest } from '@/lib/execute-request';
import { usePaneStore } from '@/stores/pane-store';
import type { FlowTab } from '@/types/pane-types';
import { useKeyboardShortcuts } from '../useKeyboardShortcuts';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/execute-request', () => ({ sendRequest: vi.fn() }));

const wrapper = ({ children }: { children: ReactNode }) => (
  <QueryClientProvider client={new QueryClient()}>{children}</QueryClientProvider>
);

const flowTab: FlowTab = {
  id: 'flow-run-1',
  title: 'Flow: my-flow',
  isDirty: false,
  tabType: 'flow',
  collectionName: 'demo',
  flowName: 'my-flow',
  nodes: [],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
};

function pressCtrlEnter() {
  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', ctrlKey: true }));
}

describe('Ctrl+Enter on a flow tab', () => {
  let runRequests: string[];
  const listener = (e: Event) => runRequests.push((e as CustomEvent).detail.tabId);

  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.clearAllMocks();
    runRequests = [];
    window.addEventListener('rocket:flow-run', listener);
  });
  afterEach(() => window.removeEventListener('rocket:flow-run', listener));

  it('asks the flow toolbar to run', () => {
    usePaneStore.getState().openTab(flowTab);
    renderHook(() => useKeyboardShortcuts(), { wrapper });
    pressCtrlEnter();
    expect(runRequests).toEqual(['flow-run-1']);
    expect(sendRequest).not.toHaveBeenCalled();
  });

  it('does not ask for a flow run from a request tab', () => {
    usePaneStore.getState().openEphemeralTab();
    renderHook(() => useKeyboardShortcuts(), { wrapper });
    pressCtrlEnter();
    expect(runRequests).toEqual([]);
    expect(sendRequest).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] **Step 2: Run to verify the first test fails**

Run: `yarn test src/hooks/__tests__/useKeyboardShortcuts-flow-run.test.tsx`
Expected: "asks the flow toolbar to run" FAILS.

- [ ] **Step 3: Dispatch the event for flow tabs**

In `src/hooks/useKeyboardShortcuts.ts`, in the Ctrl+Enter branch, add an `else if` after the request-tab block (the block that ends with `sendRequest(tab.id, tab.request); } }`). The branch becomes:

```ts
        if (tab && isRequestTab(tab)) {
          // ...existing websocket, graphql and sendRequest code unchanged...
        } else if (tab && isFlowTab(tab)) {
          // The flow toolbar owns the run lifecycle, so it starts the run.
          window.dispatchEvent(new CustomEvent('rocket:flow-run', { detail: { tabId: tab.id } }));
        }
        return;
```

(`isFlowTab` is already imported by Task 2.)

- [ ] **Step 4: Run to verify the hook tests pass**

Run: `yarn test src/hooks`
Expected: PASS.

- [ ] **Step 5: Write the failing toolbar tests**

In `src/components/flow/__tests__/FlowToolbar.test.tsx`:

1. Change the import on line 1 to `import { act, render, screen, waitFor } from '@testing-library/react';`.

2. Replace the existing test `'Stop is a no-op when no run is active'` with:

```tsx
  it('Stop is disabled when no run is active', () => {
    renderToolbar();
    expect(screen.getByRole('button', { name: 'Stop' })).toBeDisabled();
  });

  it('Stop is enabled once a run is active', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(startedHandler).toBeDefined());
    started('run-1');
    await waitFor(() => expect(screen.getByRole('button', { name: 'Stop' })).toBeEnabled());
  });
```

3. Add a new describe at the end of the file's top-level `describe('FlowToolbar', ...)` block (inside it, so it shares `beforeEach`):

```tsx
  describe('rocket:flow-run shortcut', () => {
    const fire = (tabId: string) =>
      act(() => {
        window.dispatchEvent(new CustomEvent('rocket:flow-run', { detail: { tabId } }));
      });

    it('starts a run for its own tab', async () => {
      renderToolbar({ tabId: 'tab-1' });
      fire('tab-1');
      await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalledTimes(1));
    });

    it('ignores an event for another tab', async () => {
      renderToolbar({ tabId: 'tab-1' });
      fire('tab-2');
      await act(async () => {});
      expect(tauriApi.runFlow).not.toHaveBeenCalled();
    });

    it('starts only one run when pressed twice, and none while running', async () => {
      renderToolbar({ tabId: 'tab-1' });
      fire('tab-1');
      fire('tab-1');
      await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalledTimes(1));
      await waitFor(() => expect(startedHandler).toBeDefined());
      started('run-1');
      fire('tab-1');
      await act(async () => {});
      expect(tauriApi.runFlow).toHaveBeenCalledTimes(1);
    });

    it('does not start a run while sign-in is pending', async () => {
      const onPrepareAuth = vi.fn(() => new Promise<null>(() => undefined));
      renderToolbar({ tabId: 'tab-1', onPrepareAuth });
      fire('tab-1');
      await waitFor(() => expect(onPrepareAuth).toHaveBeenCalledTimes(1));
      fire('tab-1');
      await act(async () => {});
      expect(onPrepareAuth).toHaveBeenCalledTimes(1);
      expect(tauriApi.runFlow).not.toHaveBeenCalled();
    });

    it('does nothing without a tab id, and stops listening after unmount', async () => {
      const { unmount } = renderToolbar();
      fire('tab-1');
      await act(async () => {});
      expect(tauriApi.runFlow).not.toHaveBeenCalled();
      unmount();
      renderToolbar({ tabId: 'tab-1' }).unmount();
      fire('tab-1');
      await act(async () => {});
      expect(tauriApi.runFlow).not.toHaveBeenCalled();
    });
  });
```

- [ ] **Step 6: Run to verify they fail**

Run: `yarn test src/components/flow/__tests__/FlowToolbar.test.tsx`
Expected: the new shortcut tests and "Stop is disabled when no run is active" FAIL.

- [ ] **Step 7: Implement the toolbar changes**

In `src/components/flow/FlowToolbar.tsx`:

1. Add to `FlowToolbarProps` (after `flowName`):

```tsx
  // The tab this toolbar belongs to. Lets Ctrl+Enter start this tab's run.
  tabId?: string;
```

and add `tabId,` to the destructured parameters.

2. After the `handleRun` function (after its closing `};` at line 290) and before `handleStop`, add:

```tsx
  // The global Ctrl+Enter handler dispatches this event. The ref keeps the
  // listener from using a stale handler, and handleRun guards double starts.
  const handleRunRef = useRef(handleRun);
  handleRunRef.current = handleRun;
  useEffect(() => {
    if (!tabId) return;
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<{ tabId?: string }>).detail;
      if (detail?.tabId === tabId) void handleRunRef.current();
    };
    window.addEventListener('rocket:flow-run', handler);
    return () => window.removeEventListener('rocket:flow-run', handler);
  }, [tabId]);
```

3. Change the Stop button to:

```tsx
      <Button
        size='sm'
        variant='outline'
        onClick={handleStop}
        disabled={liveRunId === null && !preparing}
      >
        Stop
      </Button>
```

Stop stays disabled in the short gap between clicking Run and the run-started event, because Stop does nothing in that gap anyway. The cancellable pre-run phase is roadmap item F-05.

4. In `src/components/flow/FlowPane.tsx`, add the prop to the `<FlowToolbar` element:

```tsx
              tabId={tab.id}
```

- [ ] **Step 8: Run to verify the tests pass**

Run: `yarn test src/components/flow src/hooks`
Expected: PASS. If any existing toolbar test clicked Stop before a run started, update it to expect the disabled state; the only known one is the test replaced in Step 5.

- [ ] **Step 9: Full gates and commit**

Run: `yarn tsc --noEmit && yarn check && yarn test src/components/flow src/components/panes src/hooks src/lib/__tests__/flow-save.test.ts`
Expected: all pass.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/hooks/useKeyboardShortcuts.ts src/hooks/__tests__/useKeyboardShortcuts-flow-run.test.tsx src/components/flow/FlowToolbar.tsx src/components/flow/FlowPane.tsx src/components/flow/__tests__/FlowToolbar.test.tsx`
Suggested subject: `feat(flow): run with Ctrl+Enter and disable Stop when idle`.

---

## Self-Review

- **Spec coverage:** F-07 (save listener: Task 1; close guard and wording and Save and close: Task 2; auto-save toast: Task 1; dirty marker already exists, no work). F-33 (Ctrl+Enter and Stop disabled: Task 3; Re-run deliberately left out, see Global Constraints).
- **Placeholders:** none. Every code step shows code.
- **Type consistency:** `flowPayloadFromTab` returns `{ collection, flow }` and is used with that shape in Tasks 1 and 2. `FlowSaveShortcut` props match the FlowPane usage. The `rocket:flow-run` event detail is `{ tabId }` in the hook, the toolbar and all tests.
- **Review Focus coverage:** items 1 and 2 are tests in Task 1; item 3 is the "keeps the tab and the dialog open" test in Task 2; items 4 and 5 are tests in Task 3 (picker and request-tab cases: the request-tab case is in the hook test, the picker case is the "does nothing without a tab id" toolbar test plus the fact that the toolbar is not mounted for pickers).

Known follow-ups outside this plan: closing other tabs, workspace switch and collection close do not prompt for dirty flows (existing limitation shared with scripts).
