# Run From Node, IPC and UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **F-03 landed in P22** (`2026-10-09-p22-client-run-id.md`). Before starting, read its section "What P20 needs from this plan": `RunFlowOptions` already exists with `runId`, `runFlow` already has the sixth parameter, the toolbar test mock returns `'run-123'`, and Rust dispatch goes through `RunFlowInputDto::take_options` and `run_with_options`.
>
> Also required: P19 merged (backend `run_partial`, `FlowPartialMode`, `FlowPartialRunInfo`, `clear_run_cache`), and P1 merged (`FlowToolbar` `tabId` prop and its `rocket:flow-run` listener).

**Goal:** Add "Run this node" and "Run from here" to the node menu, send them to the backend as a partial run on top of the tab's last run, show which node results are kept from that earlier run, and highlight the nodes a refused partial run names.

**Architecture:** `run_flow` accepts an optional `partial` object and calls `FlowExecutionService::run_partial`. The node menu does not run anything itself: it dispatches the existing `rocket:flow-run` window event (added by P1 for Ctrl+Enter) with a `partial` field, so the toolbar keeps one run lifecycle, one `isStartingRef` guard and one set of listeners. When `flow-run-started` carries `partial.nodeIds`, a new store action clears only those nodes and marks the rest "from an earlier run". A refusal names nodes in the save-error format, so `parseGraphErrorMessage` and the existing red-ring path highlight them.

**Tech Stack:** Rust (`src-tauri`, package `rocket`), React, TypeScript, Zustand (`pane-store`), Vitest and Testing Library, shadcn `DropdownMenu`, lucide icons.

**Spec:** Roadmap item F-41 in `.claude/flow-roadmap.md`. Design notes: `docs/superpowers/plans/flow-tier3/01-design-notes.md` section "P20". Backend contract: `docs/superpowers/plans/flow-tier3/2026-10-08-p19-run-from-node-backend.md`.

**Assumed F-03 contract:** `runFlow` sends a client-chosen run id and the toolbar matches `flow-run-*` events by that id. If F-03 put the run id in a `RunFlowOptions` object, add the `partial` field from Task 1 to that object instead of adding a sixth parameter; every other step is unchanged. If F-03 renamed `onRunStateChange` or moved event matching, keep F-03's names and apply the same edits at the new place.

**Decisions assumed:** D1 (refuse, no ancestor re-run) and D5 (refuse on upstream change) from P19. The UI never tries to recover from a refusal; it shows the message and highlights the named nodes.

**Deviations from the design notes (and why):**
- No `useFlowRun` hook extraction. P1 already routes Ctrl+Enter through the `rocket:flow-run` event into `handleRun`. Adding `partial` to that event gives the node menu the same single lifecycle with far less churn in a file P2, P8, P10 and P11 also edit.
- The node button becomes a menu only while the tab has a run to build on (`tab.runId`). Without one, a click still opens the properties panel directly, as today. That keeps the existing single-click behaviour and about 14 existing FlowPane tests unchanged, and the run items would be disabled anyway.

## Global Constraints

- shadcn/ui primitives only, `lucide-react` icons only, no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`.
- Zustand: never fully destructure store state at component top level. Use narrow selectors.
- Rust: no `unwrap()` in production paths; `serde(rename_all = "camelCase")` on the new IPC DTO only.
- Code comments are short full sentences that end with a punctuation mark.
- Commits use conventional commit format through the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only. Never `git add -A`, `--all` or `.`.
- Gates before each commit: `yarn tsc --noEmit`, `yarn check`, the targeted `yarn test <pattern>` listed in the task, and `cargo check -j4 -p rocket` when Rust changes. Never `--workspace`.
- One implementer at a time touches `FlowToolbar.tsx`, `FlowPane.tsx` and `FlowCanvas.tsx`.
- Line numbers are from HEAD b047bbc6. P1, P2, P8, P10, P11, P12, P16 and F-03 change these files first; locate edits by the quoted code.
- Not in scope: "run to here", a "cached from HH:MM" timestamp (the tab stores no run time; P11 history can add it later), persisting anything.

## Review Focus

Failure modes the spec implies but the obvious tests would miss, most likely first:

1. A refused partial run must not wipe the tab's `runId`. Today a failed start calls `onRunStateChange('done')` with no id, which would leave nothing to retry from. Pinned in Task 2: `keeps the base run id when the backend refuses`.
2. Starting a partial run must keep the results of nodes outside it and mark them as from an earlier run, and the next full run must clear those marks. Pinned in Task 3: `pane-store-partial-run.test.ts`.
3. A menu click plus Ctrl+Enter, or two quick clicks, must start one run only. Pinned in Task 2: `starts one run when asked twice`.
4. A node with no run to build on must keep opening its properties on one click, and "Run this node" must be disabled for a Wait node. Pinned in Task 2: `NodeMenuButton.test.tsx`.
5. The nodes a refusal names must be highlighted and the highlight cleared when the next run starts. Pinned in Task 3: `FlowPane.partialRun.test.tsx`.

---

## File Structure

| File | Responsibility |
|---|---|
| `src-tauri/src/commands/flow.rs` (modify) | `PartialRunDto`, `RunFlowInputDto.partial`, `run_flow` dispatch. |
| `src-tauri/src/commands/workspaces.rs` (modify) | `switch_workspace` clears the run cache. |
| `src/lib/tauri-api.ts` (modify) | `FlowPartialMode`, `FlowPartialRunRequest`, `FlowPartialRunInfo`, `RunFlowOptions`, `partial` on summary and started event, `runFlow` sixth parameter. |
| `src/lib/__tests__/tauri-api.flow-run.test.ts` (new) | `runFlow` payload tests. |
| `src/lib/flow-run-request.ts` (new) | `FLOW_RUN_EVENT`, `PartialRunRequest`, `FlowRunRequestDetail`, `requestFlowRun`. |
| `src/components/flow/FlowToolbar.tsx` (modify) | Partial requests, `onRunError`, partial info to `onRunStateChange`. |
| `src/components/flow/nodes/FlowNodeActionsContext.tsx` (modify) | `runNode`, `runBusy`. |
| `src/components/flow/nodes/NodeMenuButton.tsx` (modify) | Run items. |
| `src/components/flow/nodes/WaitForCallbackNode.tsx` (modify) | `isWait` on its menu button, `cached` caption. |
| `src/components/flow/FlowCanvas.tsx` (modify) | `onRunNode`, `runBusy` props into the context. |
| `src/components/flow/FlowPane.tsx` (modify) | Dispatches partial requests, `startPartialFlowRun`, refusal highlighting. |
| `src/types/pane-types.ts` (modify) | `FlowNodeDetail.cached`. |
| `src/stores/pane-store.ts` (modify) | `startPartialFlowRun`. |
| `src/components/flow/nodes/NodeStatusCaption.tsx` (modify) | "from an earlier run" caption. |
| 7 other node files in `src/components/flow/nodes/` (modify) | Pass `cached` to `NodeStatusCaption`. |

Existing tests to know: `src/components/flow/__tests__/FlowToolbar.test.tsx` (`renderToolbar`, `startedHandler`, `onRunStateChange`), `src/components/flow/nodes/__tests__/RequestNode.test.tsx` (node menu with Radix in jsdom), `src/components/flow/__tests__/FlowPane.delete.test.tsx` (FlowPane `Harness` and React Flow jsdom stubs), `src/components/flow/nodes/__tests__/NodeStatusCaption.test.tsx`, `src/lib/__tests__/tauri-api.test.ts` (invoke mock).

---

### Task 1: IPC contract and TypeScript types

**Files:**
- Modify: `src-tauri/src/commands/flow.rs` (`RunFlowInputDto` near line 466, `run_flow` near line 508, tests near line 530)
- Modify: `src-tauri/src/commands/workspaces.rs:73-91`
- Modify: `src/lib/tauri-api.ts` (`FlowRunSummary` `:2244`, after `FlowAuthToken` `:2251`, `runFlow` `:2260-2276`, `FlowRunStartedEvent` `:2283`)
- Create: `src/lib/__tests__/tauri-api.flow-run.test.ts`

**Interfaces:**
- Consumes: `rocket_app::PartialRun`, `rocket_shared::events::FlowPartialMode`, `FlowExecutionService::{run_partial, clear_run_cache}` from P19.
- Produces: IPC input key `partial: { baseRunId, startNodeId, mode: 'node' | 'fromHere' }` (optional).
- Produces: TS `FlowPartialMode`, `FlowPartialRunRequest`, `FlowPartialRunInfo` (`nodeIds`), `RunFlowOptions { partial? }`, `runFlow(collection, flowName, environmentName?, globalEnvName?, authTokens?, options?)`.

- [ ] **Step 1: Read the spec reference**

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing Rust DTO tests**

In `src-tauri/src/commands/flow.rs`, inside `mod tests`, add:

```rust
    #[test]
    fn run_flow_input_carries_a_partial_run() {
        let json = r#"{
            "collection": "c",
            "flowName": "f",
            "environmentName": null,
            "globalEnvName": null,
            "partial": { "baseRunId": "01A", "startNodeId": "n2", "mode": "fromHere" }
        }"#;
        let mut dto: RunFlowInputDto = serde_json::from_str(json).expect("deserialize");
        let partial = dto.partial.take().map(PartialRun::from).expect("partial");
        assert_eq!(
            partial,
            PartialRun {
                base_run_id: "01A".to_string(),
                start_node_id: "n2".to_string(),
                mode: FlowPartialMode::FromHere,
            }
        );
    }

    #[test]
    fn run_flow_input_without_partial_is_a_full_run() {
        let json =
            r#"{"collection":"c","flowName":"f","environmentName":null,"globalEnvName":null}"#;
        let dto: RunFlowInputDto = serde_json::from_str(json).expect("deserialize");
        assert!(dto.partial.is_none());
    }

    #[test]
    fn an_unknown_partial_mode_is_rejected() {
        let json = r#"{"collection":"c","flowName":"f","environmentName":null,"globalEnvName":null,"partial":{"baseRunId":"01A","startNodeId":"n2","mode":"everything"}}"#;
        assert!(serde_json::from_str::<RunFlowInputDto>(json).is_err());
    }
```

- [ ] **Step 3: Run them to verify they fail**

Run: `cargo test -j4 -p rocket run_flow_input`
Expected: FAIL to compile (`partial` field and `PartialRun` import missing).

- [ ] **Step 4: Add the DTO and dispatch**

In `src-tauri/src/commands/flow.rs`, add to the imports:

```rust
use rocket_app::PartialRun;
use rocket_shared::events::FlowPartialMode;
```

Add before `RunFlowInputDto`:

```rust
/// "Run this node" or "Run from here", on top of the run `base_run_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialRunDto {
    pub base_run_id: String,
    pub start_node_id: String,
    /// `"node"` or `"fromHere"`.
    pub mode: FlowPartialMode,
}

impl From<PartialRunDto> for PartialRun {
    fn from(dto: PartialRunDto) -> Self {
        Self {
            base_run_id: dto.base_run_id,
            start_node_id: dto.start_node_id,
            mode: dto.mode,
        }
    }
}
```

Add to `RunFlowInputDto`, after `auth_tokens`:

```rust
    /// Set for a partial run. Absent for a full run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial: Option<PartialRunDto>,
```

Change `run_flow` to:

```rust
#[tauri::command]
pub async fn run_flow(
    mut input: RunFlowInputDto,
    flow_exec: State<'_, FlowExecutionService>,
    exec: State<'_, RequestExecutionService>,
) -> Result<FlowRunSummary, DomainError> {
    let partial = input.partial.take().map(PartialRun::from);
    let (run_input, tokens) = input.into_parts();
    match partial {
        Some(partial) => {
            flow_exec
                .run_partial(&exec, run_input, tokens, partial)
                .await
        }
        None => flow_exec.run_with_auth(&exec, run_input, tokens).await,
    }
}
```

Keep any run id handling F-03 added to `into_parts` or `run_flow`.

- [ ] **Step 5: Clear the cache on workspace switch**

In `src-tauri/src/commands/workspaces.rs`, add `use rocket_app::FlowExecutionService;` next to `use rocket_app::WorkspaceService;`, add a parameter to `switch_workspace` after `watcher`:

```rust
    flow_exec: State<'_, FlowExecutionService>,
```

and, right after the `.switch(&id)?;` statement:

```rust
    // Cached flow runs belong to the old workspace. A same-named flow in the
    // new one is a different flow.
    flow_exec.clear_run_cache();
```

The frontend `invoke('switch_workspace', ...)` call is unchanged, because Tauri injects `State` parameters.

- [ ] **Step 6: Run the Rust tests**

Run: `cargo test -j4 -p rocket run_flow_input && cargo check -j4 -p rocket`
Expected: PASS and no errors.

- [ ] **Step 7: Write the failing TypeScript test**

Create `src/lib/__tests__/tauri-api.flow-run.test.ts`:

```ts
import { invoke } from '@tauri-apps/api/core';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { runFlow } from '../tauri-api';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

describe('runFlow', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue({ runId: 'r1', steps: [], stoppedReason: 'completed' });
  });

  it('sends a full run without a partial key', async () => {
    await runFlow('c', 'f', null, null);
    expect(invoke).toHaveBeenCalledWith('run_flow', {
      input: { collection: 'c', flowName: 'f', environmentName: null, globalEnvName: null },
    });
  });

  it('sends a partial run request', async () => {
    await runFlow('c', 'f', null, null, undefined, {
      partial: { baseRunId: '01A', startNodeId: 'n2', mode: 'fromHere' },
    });
    expect(invoke).toHaveBeenCalledWith('run_flow', {
      input: {
        collection: 'c',
        flowName: 'f',
        environmentName: null,
        globalEnvName: null,
        partial: { baseRunId: '01A', startNodeId: 'n2', mode: 'fromHere' },
      },
    });
  });
});
```

If F-03 added keys to `input` (for example a run id), add them to both expected objects.

- [ ] **Step 8: Run it to verify it fails**

Run: `yarn test src/lib/__tests__/tauri-api.flow-run.test.ts`
Expected: the partial test FAILS (`partial` is not sent), and `yarn tsc --noEmit` reports the unknown sixth argument.

- [ ] **Step 9: Add the TypeScript types and parameter**

In `src/lib/tauri-api.ts`, add after the `FlowAuthToken` interface:

```ts
/** Which nodes a partial run executes: one node, or a node and everything below it. */
export type FlowPartialMode = 'node' | 'fromHere';

/** Asks run_flow to re-run part of the flow on top of the run `baseRunId`. */
export interface FlowPartialRunRequest {
  baseRunId: string;
  startNodeId: string;
  mode: FlowPartialMode;
}

/** Describes a partial run on flow-run-started and on the summary. Keys are camelCase in both. */
export interface FlowPartialRunInfo extends FlowPartialRunRequest {
  /** Every node the run executes, in order. */
  nodeIds: string[];
}

export interface RunFlowOptions {
  partial?: FlowPartialRunRequest;
}
```

Add to `FlowRunSummary`:

```ts
  /** Set for a partial run. */
  partial?: FlowPartialRunInfo;
```

Add to `FlowRunStartedEvent`:

```ts
  /** Set for a partial run. `total_nodes` then counts only its nodes. */
  partial?: FlowPartialRunInfo;
```

Change `runFlow` to:

```ts
export const runFlow = (
  collection: string,
  flowName: string,
  environmentName?: string | null,
  globalEnvName?: string | null,
  authTokens?: Record<string, FlowAuthToken>,
  options?: RunFlowOptions,
) =>
  invoke<FlowRunSummary>('run_flow', {
    input: {
      collection,
      flowName,
      environmentName: environmentName ?? null,
      globalEnvName: globalEnvName ?? null,
      // Sent only when there is something to send, so a flow without Auth nodes
      // calls the command exactly as before.
      ...(authTokens && Object.keys(authTokens).length > 0 ? { authTokens } : {}),
      // A full run sends no partial key.
      ...(options?.partial ? { partial: options.partial } : {}),
    },
  });
```

- [ ] **Step 10: Run the tests to verify they pass**

Run: `yarn test src/lib/__tests__/tauri-api.flow-run.test.ts src/components/flow/__tests__/FlowToolbar.test.tsx`
Expected: PASS. The toolbar tests still pass because full runs call `runFlow` with the same four or five arguments.

- [ ] **Step 11: Gates and commit**

Run: `yarn tsc --noEmit && yarn check && cargo check -j4 -p rocket`
Expected: no errors.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src-tauri/src/commands/flow.rs src-tauri/src/commands/workspaces.rs src/lib/tauri-api.ts src/lib/__tests__/tauri-api.flow-run.test.ts`
Suggested subject: `feat(flow): accept partial runs over IPC`.

---

### Task 2: Run items in the node menu

**Files:**
- Create: `src/lib/flow-run-request.ts`
- Modify: `src/components/flow/FlowToolbar.tsx` (props, `handleRun`, the P1 `rocket:flow-run` listener, `onFlowRunStarted` handler, `catch` block)
- Modify: `src/components/flow/nodes/FlowNodeActionsContext.tsx`
- Modify: `src/components/flow/nodes/NodeMenuButton.tsx` (whole file)
- Modify: `src/components/flow/nodes/WaitForCallbackNode.tsx:69`
- Modify: `src/components/flow/FlowCanvas.tsx` (props `:48-80`, `nodeActions` `:319-329`)
- Modify: `src/components/flow/FlowPane.tsx` (`<FlowCanvas` element)
- Create: `src/components/flow/nodes/__tests__/NodeMenuButton.test.tsx`
- Test: `src/components/flow/__tests__/FlowToolbar.test.tsx` (extend)

**Interfaces:**
- Consumes: `FlowPartialMode`, `FlowPartialRunInfo`, `runFlow` sixth parameter from Task 1; P1's `tabId` prop and `rocket:flow-run` listener.
- Produces: `requestFlowRun({ tabId, partial?: { startNodeId, mode } })`, window event `rocket:flow-run` with that detail.
- Produces: `FlowToolbar` props `onRunError?: (message: string) => void` and `onRunStateChange: (state, runId?, partial?: FlowPartialRunInfo) => void`.
- Produces: `FlowNodeActions.runNode?: (nodeId, mode) => void`, `FlowNodeActions.runBusy?: boolean`; `NodeMenuButton` prop `isWait?: boolean`; `FlowCanvas` props `onRunNode?`, `runBusy?`.

- [ ] **Step 1: Write the failing node menu tests**

Create `src/components/flow/nodes/__tests__/NodeMenuButton.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { type FlowNodeActions, FlowNodeActionsContext } from '../FlowNodeActionsContext';
import { NodeMenuButton } from '../NodeMenuButton';

// Radix menus call pointer-capture and scrollIntoView APIs that jsdom lacks.
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.releasePointerCapture ??= () => undefined;
Element.prototype.scrollIntoView ??= () => undefined;

function renderButton(actions: Partial<FlowNodeActions> = {}, isWait = false) {
  const value: FlowNodeActions = {
    updateNodeKind: vi.fn(),
    removeSwitchCase: vi.fn(),
    openProperties: vi.fn(),
    ...actions,
  };
  render(
    <FlowNodeActionsContext.Provider value={value}>
      <NodeMenuButton nodeId='n1' label='Login' isWait={isWait} />
    </FlowNodeActionsContext.Provider>,
  );
  return value;
}

describe('NodeMenuButton', () => {
  it('opens properties directly when there is no run to build on', async () => {
    const actions = renderButton();
    await userEvent.click(screen.getByLabelText('Edit Login'));
    expect(actions.openProperties).toHaveBeenCalledWith('n1');
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  });

  it('offers Run this node and Run from here once the tab has a run', async () => {
    const runNode = vi.fn();
    renderButton({ runNode });
    await userEvent.click(screen.getByLabelText('Edit Login'));
    await userEvent.click(screen.getByRole('menuitem', { name: 'Run this node' }));
    expect(runNode).toHaveBeenCalledWith('n1', 'node');
    await userEvent.click(screen.getByLabelText('Edit Login'));
    await userEvent.click(screen.getByRole('menuitem', { name: 'Run from here' }));
    expect(runNode).toHaveBeenLastCalledWith('n1', 'fromHere');
  });

  it('still opens properties from the menu', async () => {
    const actions = renderButton({ runNode: vi.fn() });
    await userEvent.click(screen.getByLabelText('Edit Login'));
    await userEvent.click(screen.getByRole('menuitem', { name: 'Edit properties' }));
    expect(actions.openProperties).toHaveBeenCalledWith('n1');
  });

  it('disables both run items while a run is in progress', async () => {
    renderButton({ runNode: vi.fn(), runBusy: true });
    await userEvent.click(screen.getByLabelText('Edit Login'));
    expect(screen.getByRole('menuitem', { name: 'Run this node' })).toHaveAttribute(
      'aria-disabled',
      'true',
    );
    expect(screen.getByRole('menuitem', { name: 'Run from here' })).toHaveAttribute(
      'aria-disabled',
      'true',
    );
  });

  it('disables Run this node for a Wait node but keeps Run from here', async () => {
    renderButton({ runNode: vi.fn() }, true);
    await userEvent.click(screen.getByLabelText('Edit Login'));
    expect(screen.getByRole('menuitem', { name: 'Run this node' })).toHaveAttribute(
      'aria-disabled',
      'true',
    );
    expect(screen.getByRole('menuitem', { name: 'Run from here' })).not.toHaveAttribute(
      'aria-disabled',
    );
  });
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `yarn test src/components/flow/nodes/__tests__/NodeMenuButton.test.tsx`
Expected: the run-item tests FAIL (no menu without `debug`), and `yarn tsc --noEmit` reports `runNode`, `runBusy` and `isWait` as unknown.

- [ ] **Step 3: Extend the context and the menu**

In `src/components/flow/nodes/FlowNodeActionsContext.tsx`, change the import to `import type { FlowNodeKind, FlowPartialMode } from '@/lib/tauri-api';` and add to `FlowNodeActions`:

```ts
  /** Starts a partial run from this node. Absent until the tab has a run to build on. */
  runNode?: (nodeId: string, mode: FlowPartialMode) => void;
  /** True while a run is starting or in progress. Disables the run items. */
  runBusy?: boolean;
```

The default context value stays as it is (both fields are optional).

Replace `src/components/flow/nodes/NodeMenuButton.tsx` with:

```tsx
import { FastForward, MoreVertical, Play } from 'lucide-react';
import { useRef } from 'react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { useFlowNodeActions } from './FlowNodeActionsContext';

interface NodeMenuButtonProps {
  nodeId: string;
  label: string;
  /** When set, the menu has a debug mode toggle. */
  debug?: { enabled: boolean; onToggle: (enabled: boolean) => void };
  /** A Wait for callback node cannot run on its own, so "Run this node" is disabled. */
  isWait?: boolean;
}

// Opens a menu when it has more than Edit properties: a debug toggle, or run
// items once the tab has a run to build on. Otherwise a click opens the
// node's properties panel directly. `nodrag nokey` keeps a click from
// dragging the node and keeps key presses on the button away from the canvas.
export function NodeMenuButton({ nodeId, label, debug, isWait = false }: NodeMenuButtonProps) {
  const actions = useFlowNodeActions();
  const openProperties = actions.openProperties;
  const runNode = actions.runNode;
  const runBusy = actions.runBusy ?? false;
  const openedProperties = useRef(false);
  const hasMenu = debug !== undefined || runNode !== undefined;
  const trigger = (
    <Button
      type='button'
      variant='ghost'
      size='icon'
      aria-label={`Edit ${label}`}
      className='nodrag nokey ml-auto h-5 w-5 shrink-0 text-muted-foreground'
      onClick={hasMenu ? undefined : () => openProperties(nodeId)}
    >
      <MoreVertical className='h-3.5 w-3.5' aria-hidden='true' />
    </Button>
  );

  if (!hasMenu) return trigger;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>{trigger}</DropdownMenuTrigger>
      {/* The menu is portalled out of the node, so without `nokey` a Backspace
          inside it would delete the selected node. */}
      <DropdownMenuContent
        className='nokey'
        align='end'
        onCloseAutoFocus={(event) => {
          // Radix returns focus to the trigger after this. Open the panel only
          // now, so its focus request runs last and the trigger cannot steal it.
          if (!openedProperties.current) return;
          openedProperties.current = false;
          event.preventDefault();
          openProperties(nodeId);
        }}
      >
        <DropdownMenuItem
          onSelect={() => {
            openedProperties.current = true;
          }}
        >
          Edit properties
        </DropdownMenuItem>
        {runNode && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              disabled={runBusy || isWait}
              onSelect={() => runNode(nodeId, 'node')}
            >
              <Play className='h-3.5 w-3.5' aria-hidden='true' />
              Run this node
            </DropdownMenuItem>
            <DropdownMenuItem disabled={runBusy} onSelect={() => runNode(nodeId, 'fromHere')}>
              <FastForward className='h-3.5 w-3.5' aria-hidden='true' />
              Run from here
            </DropdownMenuItem>
          </>
        )}
        {debug && (
          <DropdownMenuCheckboxItem
            checked={debug.enabled}
            onCheckedChange={(value) => debug.onToggle(value === true)}
          >
            Debug mode
          </DropdownMenuCheckboxItem>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
```

In `src/components/flow/nodes/WaitForCallbackNode.tsx`, change the menu button (line 69) to:

```tsx
        <NodeMenuButton nodeId={id} label={kind.label} isWait />
```

- [ ] **Step 4: Run the node tests**

Run: `yarn test src/components/flow/nodes`
Expected: PASS, including the existing `RequestNode.test.tsx` menu tests.

- [ ] **Step 5: Write the failing toolbar tests**

In `src/components/flow/__tests__/FlowToolbar.test.tsx`, inside the top-level `describe('FlowToolbar', ...)` (P1 already imports `act`), add:

```tsx
  describe('partial runs', () => {
    const ask = (detail: object) =>
      act(() => {
        window.dispatchEvent(new CustomEvent('rocket:flow-run', { detail }));
      });
    const runB = { tabId: 'tab-1', partial: { startNodeId: 'b', mode: 'node' } };
    const withBase = { tabId: 'tab-1', tabRunState: 'done' as const, tabRunId: 'run-0' };

    it('runs the requested part on top of the tab run', async () => {
      renderToolbar(withBase);
      ask(runB);
      await waitFor(() =>
        expect(tauriApi.runFlow).toHaveBeenCalledWith(
          'my-collection',
          'my-flow',
          null,
          null,
          undefined,
          { partial: { baseRunId: 'run-0', startNodeId: 'b', mode: 'node' } },
        ),
      );
    });

    it('ignores a partial request when the tab has no run to build on', async () => {
      renderToolbar({ tabId: 'tab-1' });
      ask(runB);
      await act(async () => {});
      expect(tauriApi.runFlow).not.toHaveBeenCalled();
    });

    it('starts one run when asked twice', async () => {
      renderToolbar(withBase);
      ask(runB);
      ask({ tabId: 'tab-1' });
      await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalledTimes(1));
    });

    it('passes the partial info from flow-run-started on', async () => {
      renderToolbar(withBase);
      ask(runB);
      await waitFor(() => expect(startedHandler).toBeDefined());
      const info = { baseRunId: 'run-0', startNodeId: 'b', mode: 'node' as const, nodeIds: ['b'] };
      startedHandler?.({
        type: 'flowRunStarted',
        run_id: 'run-1',
        flow_name: 'my-flow',
        collection: 'my-collection',
        total_nodes: 1,
        partial: info,
      });
      expect(onRunStateChange).toHaveBeenCalledWith('running', 'run-1', info);
    });

    it('keeps the base run id when the backend refuses', async () => {
      const refusal = "Invalid input: 'a' changed since the earlier run — node(s): a; edge(s): ";
      vi.mocked(tauriApi.runFlow).mockRejectedValue(refusal);
      const onRunError = vi.fn();
      renderToolbar({ ...withBase, onRunError });
      ask(runB);
      await waitFor(() => expect(onRunError).toHaveBeenCalledWith(refusal));
      expect(onRunStateChange).toHaveBeenLastCalledWith('done', 'run-0');
    });
  });
```

If F-03 matches `flow-run-started` by a client run id, set `run_id` in the `passes the partial info` test to the id the toolbar sent (read it from `vi.mocked(tauriApi.runFlow).mock.calls[0]`, wherever F-03 put it).

- [ ] **Step 6: Run them to verify they fail**

Run: `yarn test src/components/flow/__tests__/FlowToolbar.test.tsx`
Expected: the five new tests FAIL.

- [ ] **Step 7: Add the request helper and teach the toolbar partial runs**

Create `src/lib/flow-run-request.ts`:

```ts
import type { FlowPartialMode } from '@/lib/tauri-api';

/** Window event that asks a flow tab's toolbar to start a run. */
export const FLOW_RUN_EVENT = 'rocket:flow-run';

/** The part of the flow to re-run. The toolbar adds the tab's last run as the base. */
export interface PartialRunRequest {
  startNodeId: string;
  mode: FlowPartialMode;
}

export interface FlowRunRequestDetail {
  tabId: string;
  /** Absent for a full run. */
  partial?: PartialRunRequest;
}

/** Asks the toolbar of `detail.tabId` to run, so every run shares one lifecycle. */
export function requestFlowRun(detail: FlowRunRequestDetail): void {
  window.dispatchEvent(new CustomEvent<FlowRunRequestDetail>(FLOW_RUN_EVENT, { detail }));
}
```

In `src/components/flow/FlowToolbar.tsx`:

1. Imports: add `type FlowPartialRunInfo, type FlowPartialRunRequest, type FlowRunSummary,` to the `@/lib/tauri-api` import, and add:

```tsx
import { FLOW_RUN_EVENT, type FlowRunRequestDetail, type PartialRunRequest } from '@/lib/flow-run-request';
```

2. Props: change `onRunStateChange` and add `onRunError`:

```tsx
  onRunStateChange: (
    state: 'running' | 'done',
    runId?: string,
    partial?: FlowPartialRunInfo,
  ) => void;
  // Receives the message of a run that could not start, such as a refused
  // partial run that names nodes.
  onRunError?: (message: string) => void;
```

and add `onRunError,` to the destructured parameters.

3. Change the start of `handleRun` from

```tsx
  const handleRun = async () => {
    if (isStartingRef.current || liveRunId !== null) return;
    isStartingRef.current = true;
```

to:

```tsx
  const handleRun = async (partialRequest?: PartialRunRequest) => {
    if (isStartingRef.current || liveRunId !== null) return;
    // A partial run builds on the tab's last run. Without one there is
    // nothing to reuse, and the menu does not offer it.
    let partial: FlowPartialRunRequest | undefined;
    if (partialRequest) {
      if (!tabRunId) return;
      partial = { baseRunId: tabRunId, ...partialRequest };
    }
    isStartingRef.current = true;
```

4. In the `onFlowRunStarted` handler, replace `onRunStateChange('running', event.run_id);` with:

```tsx
      if (event.partial) onRunStateChange('running', event.run_id, event.partial);
      else onRunStateChange('running', event.run_id);
```

5. Replace the `const summary = authTokens && ... ? await runFlow(...) : await runFlow(...);` statement with:

```tsx
      const tokens = authTokens && Object.keys(authTokens).length > 0 ? authTokens : undefined;
      const globalEnv = globalEnvName ?? null;
      let summary: FlowRunSummary;
      if (partial) {
        summary = await runFlow(collection, flowName, environmentName, globalEnv, tokens, {
          partial,
        });
      } else if (tokens) {
        summary = await runFlow(collection, flowName, environmentName, globalEnv, tokens);
      } else {
        summary = await runFlow(collection, flowName, environmentName, globalEnv);
      }
```

6. Replace the `catch` block body with:

```tsx
      // A run that cannot start rejects before any event is emitted.
      const message = String(err);
      toast.error(`Could not run flow: ${message}`);
      onRunError?.(message);
      // A refused partial run keeps the tab's last run, so the user can retry.
      if (partial) onRunStateChange('done', partial.baseRunId);
      else onRunStateChange('done');
```

7. In the P1 listener effect, replace the handler body with:

```tsx
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<Partial<FlowRunRequestDetail>>).detail;
      if (detail?.tabId === tabId) void handleRunRef.current(detail.partial);
    };
    window.addEventListener(FLOW_RUN_EVENT, handler);
    return () => window.removeEventListener(FLOW_RUN_EVENT, handler);
```

The Run button keeps `onClick={() => void handleRun()}`, which is a full run.

- [ ] **Step 8: Wire the canvas and the pane**

In `src/components/flow/FlowCanvas.tsx`:

1. Add `FlowPartialMode` to the `@/lib/tauri-api` type import, and add to `FlowCanvasProps`:

```tsx
  // Starts a partial run from a node menu. Absent while the tab has no run to build on.
  onRunNode?: (nodeId: string, mode: FlowPartialMode) => void;
  // True while a run is starting or in progress. Disables the run items.
  runBusy?: boolean;
```

and add `onRunNode, runBusy,` to the destructured props.

2. Replace the `nodeActions` memo with:

```tsx
  // A ref, so a new callback each render does not rebuild every node.
  const onRunNodeRef = useRef(onRunNode);
  onRunNodeRef.current = onRunNode;
  const canRunNode = onRunNode !== undefined;
  const nodeActions = useMemo<FlowNodeActions>(
    () => ({
      updateNodeKind: (nodeId, kind) => onNodeKindChange?.(nodeId, kind),
      removeSwitchCase: (nodeId, caseId) => onRemoveSwitchCase?.(nodeId, caseId),
      openProperties: (nodeId) => {
        selectNodesRef.current(new Set([nodeId]));
        onOpenPropertiesRef.current?.(nodeId);
      },
      runNode: canRunNode
        ? (nodeId, mode) => onRunNodeRef.current?.(nodeId, mode)
        : undefined,
      runBusy,
    }),
    [onNodeKindChange, onRemoveSwitchCase, canRunNode, runBusy],
  );
```

In `src/components/flow/FlowPane.tsx`:

1. Add imports:

```tsx
import { requestFlowRun } from '@/lib/flow-run-request';
```

and add `type FlowPartialMode` to the `@/lib/tauri-api` import.

2. Before the `return (` of the open-flow path, add:

```tsx
  // The toolbar owns the run lifecycle, so the node menu asks it to run.
  const handleRunNode = (nodeId: string, mode: FlowPartialMode) =>
    requestFlowRun({ tabId: tab.id, partial: { startNodeId: nodeId, mode } });
```

3. Add to the `<FlowCanvas` element:

```tsx
            onRunNode={tab.runId ? handleRunNode : undefined}
            runBusy={tab.runState === 'running'}
```

- [ ] **Step 9: Run the tests to verify they pass**

Run: `yarn test src/components/flow`
Expected: PASS. The existing `FlowPane.properties.test.tsx` and `FlowPane.delete.test.tsx` tests click node buttons on tabs without `runId`, so they still open properties directly.

- [ ] **Step 10: Gates and commit**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/lib/flow-run-request.ts src/components/flow/FlowToolbar.tsx src/components/flow/nodes/FlowNodeActionsContext.tsx src/components/flow/nodes/NodeMenuButton.tsx src/components/flow/nodes/WaitForCallbackNode.tsx src/components/flow/FlowCanvas.tsx src/components/flow/FlowPane.tsx src/components/flow/nodes/__tests__/NodeMenuButton.test.tsx src/components/flow/__tests__/FlowToolbar.test.tsx`
Suggested subject: `feat(flow): run one node or run from a node from its menu`.

---

### Task 3: Canvas states and refusal highlighting

**Files:**
- Modify: `src/types/pane-types.ts` (`FlowNodeDetail` `:168-185`)
- Modify: `src/stores/pane-store.ts` (interface near `:291`, implementation after `setFlowRunState` `:969-980`)
- Create: `src/stores/__tests__/pane-store-partial-run.test.ts`
- Modify: `src/components/flow/nodes/NodeStatusCaption.tsx`
- Test: `src/components/flow/nodes/__tests__/NodeStatusCaption.test.tsx` (extend)
- Modify: the data type and the `<NodeStatusCaption` element of `AuthNode.tsx` (`:11`, `:42`), `IfNode.tsx` (`:14`, `:62`), `InputNode.tsx` (`:9`, `:37`), `OutputNode.tsx` (`:13`, `:64`), `RequestNode.tsx` (`:12`, `:100`), `SwitchNode.tsx` (`:17`, `:110`), `TransformNode.tsx` (`:11`, `:60`), `WaitForCallbackNode.tsx` (`:13`, `:78`), all in `src/components/flow/nodes/`
- Modify: `src/components/flow/FlowPane.tsx` (store selectors `:47-56`, `<FlowToolbar` props `:380-387`)
- Create: `src/components/flow/__tests__/FlowPane.partialRun.test.tsx`

**Interfaces:**
- Consumes: `onRunStateChange(state, runId, partial)` and `onRunError(message)` from Task 2; `parseGraphErrorMessage` from `src/lib/flow-wiring.ts`.
- Produces: `startPartialFlowRun(tabId: string, runId: string, nodeIds: string[]): void`; `FlowNodeDetail.cached?: boolean`; `NodeStatusCaption` prop `cached?: boolean`.

- [ ] **Step 1: Write the failing store tests**

Create `src/stores/__tests__/pane-store-partial-run.test.ts`:

```ts
import { beforeEach, describe, expect, it } from 'vitest';
import { findTabInTree } from '@/lib/pane-utils';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';

const output = (id: string) => ({
  id,
  position: { x: 0, y: 0 },
  kind: { kind: 'Output' as const, label: id },
});

const tab: FlowTab = {
  id: 'flow-partial-1',
  title: 'Flow: f',
  isDirty: false,
  tabType: 'flow',
  collectionName: 'demo',
  flowName: 'f',
  nodes: [output('a'), output('b'), output('c')],
  edges: [],
  nodeStatus: { a: 'success', b: 'success', c: 'failed' },
  nodeDetail: { a: { value: 'one' }, b: { value: 'two' }, c: { error: 'boom' } },
  runState: 'done',
  runId: 'run-0',
};

function stored(): FlowTab {
  const found = findTabInTree(usePaneStore.getState().root, tab.id);
  if (!found || !isFlowTab(found.tab)) throw new Error('Expected the flow tab');
  return found.tab;
}

describe('startPartialFlowRun', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(tab);
  });

  it('clears only the nodes the run executes and marks the rest as earlier results', () => {
    usePaneStore.getState().startPartialFlowRun(tab.id, 'run-1', ['b']);
    const next = stored();
    expect(next.runState).toBe('running');
    expect(next.runId).toBe('run-1');
    expect(next.nodeStatus).toEqual({ a: 'success', c: 'failed' });
    expect(next.nodeDetail?.b).toBeUndefined();
    expect(next.nodeDetail?.a).toEqual({ value: 'one', cached: true });
    expect(next.nodeDetail?.c).toEqual({ error: 'boom', cached: true });
  });

  it('a new result for a node replaces its earlier-run mark', () => {
    usePaneStore.getState().startPartialFlowRun(tab.id, 'run-1', ['b']);
    usePaneStore.getState().patchFlowNodeStatus(tab.id, 'b', 'success', { value: 'new' });
    expect(stored().nodeDetail?.b).toEqual({ value: 'new' });
  });

  it('a full run clears every earlier-run mark', () => {
    usePaneStore.getState().startPartialFlowRun(tab.id, 'run-1', ['b']);
    usePaneStore.getState().setFlowRunState(tab.id, 'running', 'run-2');
    expect(stored().nodeStatus).toEqual({});
    expect(stored().nodeDetail).toEqual({});
  });
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `yarn test src/stores/__tests__/pane-store-partial-run.test.ts`
Expected: FAIL (`startPartialFlowRun` is not a function).

- [ ] **Step 3: Add the detail flag and the store action**

In `src/types/pane-types.ts`, add to `FlowNodeDetail`:

```ts
  /** True while a partial run is in progress and this result is from the earlier run it builds on. */
  cached?: boolean;
```

In `src/stores/pane-store.ts`, add to the store interface after `setFlowRunState`:

```ts
  startPartialFlowRun: (tabId: string, runId: string, nodeIds: string[]) => void;
```

and the implementation after `setFlowRunState`:

```ts
  // A partial run keeps every result it does not re-run, marked as from the
  // earlier run, and clears the nodes it executes.
  startPartialFlowRun(tabId, runId, nodeIds) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) => {
        if (!isFlowTab(tab)) return tab;
        const rerun = new Set(nodeIds);
        const nodeStatus: FlowTab['nodeStatus'] = {};
        const nodeDetail: Record<string, FlowNodeDetail> = {};
        for (const [id, status] of Object.entries(tab.nodeStatus)) {
          if (rerun.has(id)) continue;
          nodeStatus[id] = status;
          nodeDetail[id] = { ...tab.nodeDetail?.[id], cached: true };
        }
        return { ...tab, runState: 'running', runId, nodeStatus, nodeDetail };
      }),
    });
  },
```

Add `FlowNodeDetail` and `FlowTab` to the `@/types/pane-types` type import of `pane-store.ts` if they are not imported yet.

- [ ] **Step 4: Run the store tests to verify they pass**

Run: `yarn test src/stores/__tests__/pane-store-partial-run.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 5: Write the failing caption tests**

In `src/components/flow/nodes/__tests__/NodeStatusCaption.test.tsx`, add inside the existing top-level `describe`:

```tsx
  it('says a success is from an earlier run while a partial run is in progress', () => {
    render(<NodeStatusCaption status='success' cached />);
    expect(screen.getByTestId('node-cached-caption')).toHaveTextContent(
      'Result from an earlier run',
    );
  });

  it('adds the earlier-run note to a skip caption', () => {
    render(<NodeStatusCaption status='skipped' skipReason='branch_not_taken' cached />);
    expect(screen.getByTestId('node-cached-caption')).toHaveTextContent(/from an earlier run/);
  });
```

(Import `render` and `screen` from `@testing-library/react` if the file does not yet.)

Run: `yarn test src/components/flow/nodes/__tests__/NodeStatusCaption.test.tsx`
Expected: the two new tests FAIL.

- [ ] **Step 6: Show the caption on every node**

In `src/components/flow/nodes/NodeStatusCaption.tsx`, add `cached` to the props (`cached?: boolean;` in the type and `cached,` in the parameter list), and replace the last four lines of the function (from `const caption = ...` to the final `);`) with:

```tsx
  const caption = nodeStatusCaption(status, { skipReason });
  // During a partial run, results the run does not redo are from the earlier run.
  if (cached) {
    return (
      <div data-testid='node-cached-caption' className='px-2 pt-1 italic text-muted-foreground'>
        {caption ? `${caption} · from an earlier run` : 'Result from an earlier run'}
      </div>
    );
  }
  if (!caption) return null;
  return (
    <div data-testid='node-status-caption' className='px-2 pt-1 italic text-muted-foreground'>
      {caption}
    </div>
  );
```

A failed node keeps showing its error, and a running node its progress, because those branches return first.

In each of the 8 node files listed under Files, add to the node's data type:

```ts
  /** True while a partial run is in progress and this result is from the earlier run. */
  cached?: boolean;
```

and add `cached={data.cached}` to its `<NodeStatusCaption` element. For `RequestNode.tsx` the element becomes:

```tsx
        <NodeStatusCaption
          status={status}
          skipReason={data.skipReason}
          progress={data.progress}
          cached={data.cached}
        />
```

`toRfNodes` already spreads `nodeDetail[n.id]` into `data`, so no canvas change is needed.

Run: `yarn test src/components/flow/nodes`
Expected: PASS.

- [ ] **Step 7: Write the failing FlowPane test**

Create `src/components/flow/__tests__/FlowPane.partialRun.test.tsx`:

```tsx
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  onFlowRunStarted,
  onFlowStepCompleted,
  onFlowStepProgress,
  onFlowStepStarted,
  runFlow,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    listCollections: vi.fn().mockResolvedValue([]),
    listFlows: vi.fn().mockResolvedValue([]),
    saveFlow: vi.fn().mockResolvedValue(undefined),
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
vi.mock('@/components/editor', () => ({
  SingleLineEditor: (props: { value: string; 'aria-label'?: string }) => (
    <input aria-label={props['aria-label']} value={props.value} readOnly />
  ),
}));

// Radix menus call pointer-capture and scrollIntoView APIs that jsdom lacks.
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.releasePointerCapture ??= () => undefined;
Element.prototype.scrollIntoView ??= () => undefined;
// jsdom has no DOMMatrixReadOnly, which React Flow reads when it re-measures nodes.
vi.stubGlobal(
  'DOMMatrixReadOnly',
  class {
    m22 = 1;
  },
);

const baseTab: FlowTab = {
  id: 'flow-partial-ui',
  tabType: 'flow',
  title: 'Flow: p',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'p',
  nodes: [
    { id: 'in1', position: { x: 0, y: 0 }, kind: { kind: 'Input', label: 'User', value: 'alice' } },
    { id: 'out1', position: { x: 300, y: 0 }, kind: { kind: 'Output', label: 'Result' } },
  ],
  edges: [
    {
      id: 'e1',
      sourceNodeId: 'in1',
      targetNodeId: 'out1',
      targetField: 'value',
      expression: 'response.body',
    },
  ],
  nodeStatus: { in1: 'success', out1: 'success' },
  nodeDetail: { in1: { value: 'alice' }, out1: { value: 'alice' } },
  runState: 'done',
  runId: 'run-0',
};

type StartedHandler = Parameters<typeof onFlowRunStarted>[0];
let startedHandler: StartedHandler | undefined;

function getFlowTab(): FlowTab {
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('Expected root to be a leaf');
  const found = root.tabs.find((t) => t.id === baseTab.id);
  if (!found || !isFlowTab(found)) throw new Error('Expected the seeded flow tab');
  return found;
}

function Harness() {
  const tab = usePaneStore((s) => {
    const root = s.root;
    if (root.type !== 'leaf') return null;
    const t = root.tabs.find((x) => x.id === baseTab.id);
    return t && isFlowTab(t) ? t : null;
  });
  if (!tab) return null;
  return <FlowPane tab={tab} groupId={usePaneStore.getState().activeGroupId} />;
}

const user = () => userEvent.setup({ pointerEventsCheck: 0 });

async function runThisNode(label: string) {
  const u = user();
  await u.click(screen.getByLabelText(`Edit ${label}`));
  await u.click(await screen.findByRole('menuitem', { name: 'Run this node' }));
}

describe('FlowPane partial runs', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    startedHandler = undefined;
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(baseTab);
    const unlisten = async () => () => undefined;
    vi.mocked(onFlowRunStarted).mockImplementation(async (h) => {
      startedHandler = h;
      return () => undefined;
    });
    vi.mocked(onFlowStepStarted).mockImplementation(unlisten);
    vi.mocked(onFlowStepCompleted).mockImplementation(unlisten);
    vi.mocked(onFlowStepProgress).mockImplementation(unlisten);
    // Keep the run pending so each test ends mid-run.
    vi.mocked(runFlow).mockImplementation(() => new Promise(() => undefined));
  });

  it('Run this node starts a partial run on the last run', async () => {
    render(<Harness />);
    await runThisNode('Result');
    await waitFor(() => expect(runFlow).toHaveBeenCalledTimes(1));
    expect(vi.mocked(runFlow).mock.calls[0][5]).toEqual({
      partial: { baseRunId: 'run-0', startNodeId: 'out1', mode: 'node' },
    });
  });

  it('keeps results outside the run and marks them as earlier results', async () => {
    render(<Harness />);
    await runThisNode('Result');
    await waitFor(() => expect(startedHandler).toBeDefined());
    startedHandler?.({
      type: 'flowRunStarted',
      run_id: 'run-1',
      flow_name: 'p',
      collection: 'demo',
      total_nodes: 1,
      partial: { baseRunId: 'run-0', startNodeId: 'out1', mode: 'node', nodeIds: ['out1'] },
    });
    await waitFor(() => expect(getFlowTab().runId).toBe('run-1'));
    expect(getFlowTab().nodeStatus).toEqual({ in1: 'success' });
    expect(getFlowTab().nodeDetail?.in1?.cached).toBe(true);
  });

  it('highlights the nodes a refused partial run names', async () => {
    vi.mocked(runFlow).mockRejectedValue(
      "Invalid input: 'User' changed since the earlier run, or did not run in it. Run the full flow, or Run from the first changed node — node(s): in1; edge(s): ",
    );
    render(<Harness />);
    await runThisNode('Result');
    await waitFor(() => expect(getFlowTab().runId).toBe('run-0'));
    const u = user();
    await u.click(screen.getByLabelText('Edit User'));
    await u.click(await screen.findByRole('menuitem', { name: 'Edit properties' }));
    expect(await screen.findByTestId('node-save-error')).toHaveTextContent(
      'changed since the earlier run',
    );
  });
});
```

If F-03 matches `flow-run-started` by a client run id, set `run_id` in the second test to the id the toolbar sent, as in Task 2 Step 5.

- [ ] **Step 8: Run it to verify it fails**

Run: `yarn test src/components/flow/__tests__/FlowPane.partialRun.test.tsx`
Expected: the second and third tests FAIL (FlowPane still calls `setFlowRunState` and has no `onRunError`).

- [ ] **Step 9: Wire partial starts and refusals in FlowPane**

In `src/components/flow/FlowPane.tsx`:

1. Next to the other store selectors (line 55), add:

```tsx
  const startPartialFlowRun = usePaneStore((s) => s.startPartialFlowRun);
```

2. Replace the `onRunStateChange={(state, runId) => setFlowRunState(tab.id, state, runId)}` prop of `<FlowToolbar` with:

```tsx
              onRunStateChange={(state, runId, partial) => {
                if (state === 'running') {
                  // A new run replaces the highlight of an earlier refusal.
                  setCycleNodeIds([]);
                  setCycleEdgeIds([]);
                  setSaveErrorMessage(null);
                }
                if (state === 'running' && partial && runId) {
                  startPartialFlowRun(tab.id, runId, partial.nodeIds);
                } else {
                  setFlowRunState(tab.id, state, runId);
                }
              }}
              onRunError={(message) => {
                // A refused partial run names nodes the same way a save error does.
                const parsed = parseGraphErrorMessage(message);
                if (!parsed) return;
                setCycleNodeIds(parsed.nodeIds);
                setCycleEdgeIds(parsed.edgeIds);
                setSaveErrorMessage(message);
              }}
```

If P12 replaced `cycleNodeIds` with issues built from save errors, call the same setters P12's save path calls; the message format is identical.

- [ ] **Step 10: Run the tests to verify they pass**

Run: `yarn test src/components/flow src/stores`
Expected: PASS.

- [ ] **Step 11: Full gates and commit**

Run: `yarn tsc --noEmit && yarn check && yarn test src/components/flow src/stores src/lib/__tests__/tauri-api.flow-run.test.ts`
Expected: all pass.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/types/pane-types.ts src/stores/pane-store.ts src/stores/__tests__/pane-store-partial-run.test.ts src/components/flow/nodes/NodeStatusCaption.tsx src/components/flow/nodes/__tests__/NodeStatusCaption.test.tsx src/components/flow/nodes/AuthNode.tsx src/components/flow/nodes/IfNode.tsx src/components/flow/nodes/InputNode.tsx src/components/flow/nodes/OutputNode.tsx src/components/flow/nodes/RequestNode.tsx src/components/flow/nodes/SwitchNode.tsx src/components/flow/nodes/TransformNode.tsx src/components/flow/nodes/WaitForCallbackNode.tsx src/components/flow/FlowPane.tsx src/components/flow/__tests__/FlowPane.partialRun.test.tsx`
Suggested subject: `feat(flow): show earlier results during a partial run and highlight refusals`.

---

## Self-Review

- **Spec coverage:** F-41 IPC and UI. DTO and dispatch, workspace cache clear, TS types (Task 1). One run lifecycle through the P1 event, node menu items with disabled states, Wait rule mirrored in the UI (Task 2). Partial canvas state, earlier-result caption, refusal highlighting (Task 3).
- **Placeholders:** none. Every code step shows code. The F-03 notes say exactly which call or field to adapt.
- **Type consistency:** `FlowPartialMode`, `FlowPartialRunRequest` and `FlowPartialRunInfo` (Task 1) are used unchanged in Tasks 2 and 3. `PartialRunRequest { startNodeId, mode }` is the event detail; the toolbar turns it into `FlowPartialRunRequest` by adding `baseRunId`. `onRunStateChange(state, runId?, partial?)` matches between `FlowToolbar` and `FlowPane`. `startPartialFlowRun(tabId, runId, nodeIds)` matches the store test and FlowPane.
- **Review Focus coverage:** item 1 in Task 2 (toolbar test); item 2 in Task 3 (store tests); item 3 in Task 2 (`starts one run when asked twice`); item 4 in Task 2 (`NodeMenuButton.test.tsx`); item 5 in Task 3 (`FlowPane.partialRun.test.tsx`, plus the clear in `onRunStateChange`).

Known follow-ups: P11 run history can label partial records (`partial` is reserved there) and supply a "from HH:MM" time; P16 can add "from an earlier run" to the node aria label.
