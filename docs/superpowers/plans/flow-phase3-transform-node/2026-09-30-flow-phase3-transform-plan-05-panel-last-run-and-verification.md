# Flow Phase 3 — Plan 05: Properties Panel, Last Run and Final Verification — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. **This is the last plan. Run it on its own, and stop after the final report.**

**Goal:** Let a user edit a Transform node's script in the properties panel with a Monaco editor, see the returned value and logs in the Last run tab, and confirm the whole Phase 3 feature passes every check.

**Architecture:** A new `TransformNodeEditor` (label field plus a Monaco script editor with `response` typings) replaces the label-only placeholder that plan 03 added to `NodePropertiesPanel`. `LastRunTab` already shows a value section for Input and Output nodes and a logs section for all nodes, so Transform joins the value section with one condition change. A final task runs every check and hands the user a manual test list.

**Tech Stack:** React, TypeScript, Monaco (`MonacoWrapper`), shadcn/ui, Vitest, Yarn, Rust (verification only).

**Spec:** `docs/superpowers/specs/2026-09-30-flow-phase3-transform-node-design.md` (§9 Frontend, §11 Testing). Plan index and cross-plan contract: `docs/superpowers/plans/flow-phase3-transform-node/00-plan-index.md`.

## Global Constraints

- Plans 01 to 04 are merged. If `TransformNode` or the Rust `Transform` arm is missing, stop and report.
- Multi-line editors use Monaco only (`MonacoWrapper`). Never use `SingleLineEditor` for the script.
- All UI uses shadcn/ui primitives and `lucide-react` icons. No raw `<button>`, `<input>`, `<select>`, `<form>` or `<dialog>`.
- The editor reuses `WIRE_SCRIPT_TYPES` from `src/components/flow/wire-script-types.ts`, because the script sees the same `response` object as a wire script.
- Use Yarn. Never fully destructure a Zustand store at component top level.
- Do not close or comment on issue #32. Report to the user instead.
- Commits: conventional-commit subjects, created through the `dev-workflow-skills:1-git-commit` skill. Stage only the task's own paths.
- Always pass `-j4` to cargo. Never run `cargo test --workspace`.
- Code comments: short full sentences ending with a punctuation mark.

## Review Focus

1. **An empty script typed in the editor.** The user must see why the flow will not save, before pressing Save. Pinned in Task 1 (`warns about an empty script`).
2. **A script edit must keep the label and the rest of the node.** Editing one field must not reset the other. Pinned in Task 1 (`keeps the label when the script changes`).
3. **A Transform that returned an object.** The Last run tab must show it pretty-printed, like an Output value. Pinned in Task 2 (`shows a Transform value pretty-printed`).
4. **A Transform that logged and then failed.** The logs must still show next to the error. Pinned in Task 2 (`shows the logs of a failed Transform`).
5. **A Transform that was skipped.** The Last run tab must give the branch-not-taken reason, with no value section. Pinned in Task 2 (`explains a skipped Transform without showing a value`).

---

### Task 1: The script editor in the properties panel

**Files:**
- Create: `src/components/flow/properties/TransformNodeEditor.tsx`
- Create: `src/components/flow/properties/__tests__/TransformNodeEditor.test.tsx`
- Modify: `src/components/flow/properties/NodePropertiesPanel.tsx` (import and the `Transform` case)
- Modify: `src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx`

**Interfaces:**
- Consumes: `LabelField` (`{ value: string; onChange: (label: string) => void }`), `MonacoWrapper` from `@/components/editor/MonacoWrapper`, `WIRE_SCRIPT_TYPES` and `WIRE_SCRIPT_TYPES_PATH` from `../wire-script-types`, and the `Transform` kind type.
- Produces: `TransformNodeEditor({ kind, onChange })`, where `onChange` receives the whole updated `FlowNodeKind`.

- [ ] **Step 1: Write the failing tests**

Create `src/components/flow/properties/__tests__/TransformNodeEditor.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { FlowNodeKind } from '@/lib/tauri-api';
import { TransformNodeEditor } from '../TransformNodeEditor';

// Monaco cannot run in jsdom. A textarea with the same value/onChange contract stands in.
vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: (props: {
    value: string;
    onChange?: (value: string) => void;
    language?: string;
  }) => (
    <textarea
      aria-label='Script editor'
      data-language={props.language}
      value={props.value}
      onChange={(e) => props.onChange?.(e.target.value)}
    />
  ),
}));

type TransformKind = Extract<FlowNodeKind, { kind: 'Transform' }>;
const kind: TransformKind = { kind: 'Transform', label: 'Pick', script: 'return 1;' };

describe('TransformNodeEditor', () => {
  it('shows the script in a JavaScript editor', () => {
    render(<TransformNodeEditor kind={kind} onChange={vi.fn()} />);
    const editor = screen.getByLabelText('Script editor');
    expect(editor).toHaveValue('return 1;');
    expect(editor).toHaveAttribute('data-language', 'javascript');
  });

  it('reports the whole node when the script changes', async () => {
    const onChange = vi.fn();
    render(<TransformNodeEditor kind={kind} onChange={onChange} />);
    await userEvent.type(screen.getByLabelText('Script editor'), '2');
    expect(onChange).toHaveBeenLastCalledWith({ ...kind, script: 'return 1;2' });
  });

  it('keeps the label when the script changes', async () => {
    const onChange = vi.fn();
    render(<TransformNodeEditor kind={kind} onChange={onChange} />);
    await userEvent.type(screen.getByLabelText('Script editor'), 'x');
    expect(onChange.mock.lastCall?.[0]).toMatchObject({ label: 'Pick' });
  });

  it('reports the whole node when the label changes', async () => {
    const onChange = vi.fn();
    render(<TransformNodeEditor kind={kind} onChange={onChange} />);
    await userEvent.type(screen.getByLabelText('Label'), 'x');
    expect(onChange).toHaveBeenLastCalledWith({ ...kind, label: 'Pickx' });
  });

  it('warns about an empty script', () => {
    render(<TransformNodeEditor kind={{ ...kind, script: ' \n ' }} onChange={vi.fn()} />);
    expect(screen.getByRole('alert')).toHaveTextContent(/script is empty/i);
  });

  it('shows no warning for a script with content', () => {
    render(<TransformNodeEditor kind={kind} onChange={vi.fn()} />);
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });
});
```

Append to `src/components/flow/properties/__tests__/NodePropertiesPanel.test.tsx`, inside the top-level `describe('NodePropertiesPanel', …)` block (the file already mocks `MonacoWrapper` as `() => null`):

```tsx
  it('shows the label and script sections for a Transform node', () => {
    renderPanel(node('t1', { kind: 'Transform', label: 'Pick', script: 'return 1;' }));
    expect(screen.getByLabelText('Label')).toHaveValue('Pick');
    expect(screen.getByText('Script')).toBeInTheDocument();
  });

  it('edits the label of a Transform node', async () => {
    const { onChange } = renderPanel(
      node('t1', { kind: 'Transform', label: 'Pick', script: 'return 1;' }),
    );
    await userEvent.type(screen.getByLabelText('Label'), 'x');
    expect(onChange).toHaveBeenLastCalledWith({
      kind: 'Transform',
      label: 'Pickx',
      script: 'return 1;',
    });
  });
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test TransformNodeEditor NodePropertiesPanel`
Expected: FAIL. `TransformNodeEditor` does not exist, and the panel test fails because the placeholder shows no `Script` section.

- [ ] **Step 3: Write the editor**

Create `src/components/flow/properties/TransformNodeEditor.tsx`:

```tsx
import { MonacoWrapper } from '@/components/editor/MonacoWrapper';
import type { FlowNodeKind } from '@/lib/tauri-api';
import { WIRE_SCRIPT_TYPES, WIRE_SCRIPT_TYPES_PATH } from '../wire-script-types';
import { LabelField } from './LabelField';

type TransformKind = Extract<FlowNodeKind, { kind: 'Transform' }>;

// The script sees the same `response` object as a wire script.
const EXTRA_LIB = { content: WIRE_SCRIPT_TYPES, filePath: WIRE_SCRIPT_TYPES_PATH };

export function TransformNodeEditor({
  kind,
  onChange,
}: {
  kind: TransformKind;
  onChange: (kind: FlowNodeKind) => void;
}) {
  return (
    <div className='space-y-3'>
      <LabelField value={kind.label} onChange={(label) => onChange({ ...kind, label })} />
      <div className='space-y-1'>
        <span className='text-xs font-medium'>Script</span>
        <p className='text-xs text-muted-foreground'>
          Write one expression, or several lines that end with return value. The upstream value is
          available as response. console.log output appears in Last run.
        </p>
        <div className='h-64 overflow-hidden rounded border'>
          <MonacoWrapper
            value={kind.script}
            onChange={(script) => onChange({ ...kind, script })}
            language='javascript'
            height='100%'
            extraLib={EXTRA_LIB}
          />
        </div>
        {kind.script.trim() === '' && (
          <p role='alert' className='text-xs text-amber-600'>
            The script is empty, so the flow cannot be saved.
          </p>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Wire it into the panel**

In `src/components/flow/properties/NodePropertiesPanel.tsx`, add `import { TransformNodeEditor } from './TransformNodeEditor';` before the `WaitForCallbackEditor` import, and replace the placeholder case:

```tsx
    case 'Transform':
      return <TransformNodeEditor kind={kind} onChange={onChange} />;
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `yarn test TransformNodeEditor NodePropertiesPanel`
Expected: PASS, including every existing panel test.

- [ ] **Step 6: Commit**

Stage the four files, then commit through the `dev-workflow-skills:1-git-commit` skill with subject `feat(flow): edit Transform scripts in the panel`.

---

### Task 2: The Last run tab for Transform

**Files:**
- Modify: `src/components/flow/properties/LastRunTab.tsx` (the value-section condition near line 320)
- Modify: `src/components/flow/properties/__tests__/LastRunTab.test.tsx`

**Interfaces:**
- Consumes: `ValueSection`, `LogsSection` and `skipText` already in `LastRunTab.tsx`, and the `FlowNodeDetail` fields `value`, `logs`, `error` and `skipReason`.
- Produces: a Transform node whose Last run tab shows its returned value, its logs, its error or its skip reason.

- [ ] **Step 1: Write the failing tests**

Append to `src/components/flow/properties/__tests__/LastRunTab.test.tsx`, reusing its `node` helper and existing mocks:

```tsx
describe('LastRunTab for a Transform node', () => {
  const transform = node({ kind: 'Transform', label: 'Pick', script: 'return 1;' });

  it('shows a Transform value pretty-printed', () => {
    render(<LastRunTab node={transform} status='success' detail={{ value: '{"a":1}' }} />);
    expect(screen.getByTestId('last-run-value')).toHaveTextContent('"a": 1');
    expect(screen.getByRole('button', { name: 'Copy value' })).toBeInTheDocument();
  });

  it('shows a plain text value as is', () => {
    render(<LastRunTab node={transform} status='success' detail={{ value: 'PRO' }} />);
    expect(screen.getByTestId('last-run-value')).toHaveTextContent('PRO');
  });

  it('shows the logs of a failed Transform', () => {
    render(
      <LastRunTab
        node={transform}
        status='failed'
        detail={{
          error: 'script returned no value',
          logs: [{ level: 'log', message: 'checking token' }],
        }}
      />,
    );
    expect(screen.getByTestId('last-run-error')).toHaveTextContent('script returned no value');
    expect(screen.getByText('checking token')).toBeInTheDocument();
    expect(screen.queryByTestId('last-run-value')).not.toBeInTheDocument();
  });

  it('explains a skipped Transform without showing a value', () => {
    render(
      <LastRunTab
        node={transform}
        status='skipped'
        detail={{ skipReason: 'branch_not_taken' }}
      />,
    );
    expect(screen.getByText(/branch was not taken/i)).toBeInTheDocument();
    expect(screen.queryByTestId('last-run-value')).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test LastRunTab`
Expected: FAIL for `shows a Transform value pretty-printed` and `shows a plain text value as is`, because `last-run-value` is only rendered for Input and Output nodes. The other two tests may already pass.

- [ ] **Step 3: Show the value for Transform**

In `LastRunTab.tsx`, replace the value-section condition:

```tsx
      {(node.kind.kind === 'Output' ||
        node.kind.kind === 'Input' ||
        node.kind.kind === 'Transform') &&
        detail?.value !== undefined && <ValueSection value={detail.value} />}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test LastRunTab`
Expected: PASS. If `shows the logs of a failed Transform` fails on the log entry shape, open `LogsSection` in `LastRunTab.tsx`, read how it renders an entry, and correct the fixture (not the component) to match.

- [ ] **Step 5: Commit**

Stage the two files, then commit through the `dev-workflow-skills:1-git-commit` skill with subject `feat(flow): show Transform values in Last run`.

---

### Task 3: Final verification and hand-off

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `docs/superpowers/specs/2026-09-30-flow-phase3-transform-node-design.md` (the `Status` line only)

**Interfaces:**
- Consumes: everything from plans 01 to 05.
- Produces: a passing check run, a spec marked implemented, and a manual test list for the user. It changes no production code unless a check fails.

- [ ] **Step 1: Run the Rust checks**

Run each command and record the counts:

```bash
cargo fmt --all -- --check
cargo clippy -j4 -p rocket-flow -p rocket-app -p rocket-infra -- -D warnings
cargo check -j4 --workspace
cargo test -j4 -p rocket-flow
cargo test -j4 -p rocket-app flow
cargo test -j4 -p rocket-infra transform
```

Also run the `src-tauri` DTO test: `cargo test -j4 -p rocket transform_node_dto` (use the package name from `src-tauri/Cargo.toml` if it is not `rocket`).
Expected: no fmt or clippy output, a clean check, and all tests passing. Do not run `cargo test --workspace`.

- [ ] **Step 2: Run the frontend checks**

```bash
yarn tsc --noEmit
yarn check
yarn test flow
```

Expected: no type errors, no lint or format errors, and every flow test file passing.

- [ ] **Step 3: Fix anything that failed**

If a check fails, find the plan and task that owns the failing file, fix the production code (not the test, unless the test is wrong), re-run that check and then the full list above. Commit each fix through the `dev-workflow-skills:1-git-commit` skill as its own commit, for example `fix(flow): <what was wrong>`.

- [ ] **Step 4: Mark the spec implemented**

In `docs/superpowers/specs/2026-09-30-flow-phase3-transform-node-design.md`, change the `Status` line to:

```
**Status:** Implemented (plans 01-05 in `docs/superpowers/plans/flow-phase3-transform-node/`). Manual check of the real script engine is pending, see the plan 05 hand-off.
```

Commit through the `dev-workflow-skills:1-git-commit` skill with subject `docs(flow): mark Phase 3 spec implemented`.

- [ ] **Step 5: Write the hand-off report**

Report to the user, in plain text, with no claims beyond what you ran:
- The exact commands from Steps 1 and 2 and their results (pass counts).
- Anything you skipped or could not run, and why.
- This **manual check list**, which no automated test covers because the fake script engine used in tests cannot tell `undefined` from `null` and does not run real JavaScript. The user runs it with `yarn tauri dev`:
  1. Add an Input node with the value `pro`, a Transform node with the default script, and an Output node. Wire Input to Transform `input`, and Transform `result` to Output `value`. Run the flow. Expect the Output to show `pro`.
  2. Change the script to `return { plan: response.body };`. Run again. Expect the Transform and Output values to show `{"plan":"pro"}` as pretty-printed JSON.
  3. Change the script to `const x = 1;` (no return). Run again. Expect the Transform to fail with `script returned no value` and the Output to be skipped with the upstream-failed reason.
  4. Change the script to `console.log('hi'); return 0;`. Expect `0` as the value and `hi` in the Last run logs.
  5. Put the Transform after an If whose condition is false. Expect the Transform and Output to read "Not taken".
  6. Save, close and reopen the flow. Expect the multi-line script to be unchanged.
- That issue #32 is left open for the user to close.

Do not close or comment on issue #32.

---

## Next Plan

**None. This is the last plan of Flow Phase 3.** After the review below, report to the user and stop.

## Post-Implementation Review

Dispatch one Opus-model subagent (read and fix allowed) with this brief: "Review the whole Phase 3 feature against `docs/superpowers/specs/2026-09-30-flow-phase3-transform-node-design.md` using `git log` for the commits of plans 01 to 05 under `docs/superpowers/plans/flow-phase3-transform-node/`. For each spec section (§5 to §11), point to the code and test that implement it and list any gap. Check that old flows still load and re-save unchanged, that the guard for `undefined` is only called for `FlowCoercion::Required`, that no panicking calls were added outside tests, and that the frontend follows `.claude/rules/frontend-component-guardrails.md`. Fix any defect you find and report what changed."
