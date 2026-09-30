# Flow Phase 3 — Plan 03: Frontend Types and Wiring Rules — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. **Run this plan on its own. Do not start plan 04 in the same run.**

**Goal:** Teach the frontend model about the Transform node: the TypeScript type, the default script, and the client-side wiring rules that mirror `rocket_flow::validate`, so an invalid Transform wire can never be drawn.

**Architecture:** `FlowNodeKind` in `tauri-api.ts` gains the `Transform` variant. `flow-handles.ts` gets `takesSingleInput`, which widens the "exactly one input" rule from If/Switch to Transform. `flow-wiring.ts` uses it and learns Transform's handles. No UI component is added here except a label-only placeholder in the properties panel, which plan 05 replaces.

**Tech Stack:** TypeScript, React, Vitest, Biome, Yarn.

**Spec:** `docs/superpowers/specs/2026-09-30-flow-phase3-transform-node-design.md` (§6 Validation, §9 Frontend). Plan index and cross-plan contract: `docs/superpowers/plans/flow-phase3-transform-node/00-plan-index.md`.

## Global Constraints

- Plan 01 is merged: the backend DTO is `Transform { label, script }` and its JSON uses `kind: 'Transform'`, `label`, `script`. If it is not merged, stop and report.
- Handle names come from `src/lib/flow-handles.ts`. Do not write handle strings inline.
- The client-side rules are a convenience copy of `rocket_flow::validate`. Save still runs the real validation.
- No raw `<button>`, `<input>`, `<select>` or `<dialog>`, and only `lucide-react` icons (this plan adds no new UI).
- Use Yarn. Never fully destructure a Zustand store at component top level.
- Commits: conventional-commit subjects, created through the `dev-workflow-skills:1-git-commit` skill. Stage only the task's own paths.
- Code comments: short full sentences ending with a punctuation mark.

## Review Focus

1. **Drawing a second wire into a Transform's input.** The canvas must refuse it, as it does for If and Switch, instead of letting the user save a graph the backend rejects. Pinned in Task 2 (`rejects a second wire into a Transform input`).
2. **Dragging a Run when (`trigger`) or `url` wire into a Transform, or a `true` exit out of a Transform.** Both must be refused. Pinned in Task 1 (`a Transform exits through result only`, `rejects other fields into a Transform`).
3. **A wire into another node's `input` field from a node that is not If, Switch or Transform.** Still refused (this rule must not loosen). Pinned in Task 2 (`still refuses an input wire into a Request`).
4. **A saved wire into a Transform's `input`.** It carries no expression, so the wire must not prompt for a script. Pinned in Task 3 (`does not prompt for an expression on a Transform input wire`).
5. **A Transform result wire.** It must omit `sourceHandle`, like every plain `result` wire, so saved files stay minimal. Pinned in Task 3 (`omits the result exit on a Transform wire`).

---

### Task 1: The `Transform` type, default script and basic wiring

**Files:**
- Modify: `src/lib/tauri-api.ts` (`FlowNodeKind` union near line 1779)
- Create: `src/lib/flow-transform.ts`
- Create: `src/lib/__tests__/flow-transform.test.ts`
- Modify: `src/lib/flow-wiring.ts` (`sourceHandleExists`, `targetAccepts`)
- Modify: `src/lib/__tests__/flow-wiring.test.ts` (new `describe` at the end)
- Modify: `src/components/flow/properties/NodePropertiesPanel.tsx` (a placeholder `Transform` case, so `yarn tsc --noEmit` stays green)

**Interfaces:**
- Consumes: nothing from earlier plans beyond the backend JSON shape.
- Produces: `{ kind: 'Transform'; label: string; script: string }` in `FlowNodeKind`, and `DEFAULT_TRANSFORM_SCRIPT` exported from `src/lib/flow-transform.ts`. Plans 04 and 05 import both.

- [ ] **Step 1: Write the failing tests**

Create `src/lib/__tests__/flow-transform.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { DEFAULT_TRANSFORM_SCRIPT } from '../flow-transform';

describe('DEFAULT_TRANSFORM_SCRIPT', () => {
  it('matches TRANSFORM_DEFAULT_SCRIPT in rocket-flow', () => {
    expect(DEFAULT_TRANSFORM_SCRIPT).toBe('return response.body;');
  });
});
```

Append to `src/lib/__tests__/flow-wiring.test.ts`:

```ts
describe('Transform wiring', () => {
  const transform = node('tf', {
    kind: 'Transform',
    label: 'Pick',
    script: 'return response.body;',
  });
  const all = [...nodes, transform];
  const conn = (
    source: string,
    sourceHandle: string | null,
    target: string,
    targetHandle: string,
  ) => ({ source, sourceHandle, target, targetHandle });

  it('accepts any node result, and a routing exit, into input', () => {
    expect(isValidFlowConnection(conn('req', 'result', 'tf', 'input'), all, [])).toBe(true);
    expect(isValidFlowConnection(conn('inp', null, 'tf', 'input'), all, [])).toBe(true);
    expect(isValidFlowConnection(conn('iff', 'true', 'tf', 'input'), all, [])).toBe(true);
    expect(isValidFlowConnection(conn('sw', 'case:c1', 'tf', 'input'), all, [])).toBe(true);
  });

  it('rejects other fields into a Transform', () => {
    for (const field of ['url', 'body', 'headers', 'value', 'trigger']) {
      expect(isValidFlowConnection(conn('req', 'result', 'tf', field), all, [])).toBe(false);
    }
  });

  it('a Transform exits through result only', () => {
    expect(isValidFlowConnection(conn('tf', 'result', 'out', 'value'), all, [])).toBe(true);
    expect(isValidFlowConnection(conn('tf', null, 'req', 'url'), all, [])).toBe(true);
    expect(isValidFlowConnection(conn('tf', 'true', 'out', 'value'), all, [])).toBe(false);
    expect(isValidFlowConnection(conn('tf', 'default', 'out', 'value'), all, [])).toBe(false);
    expect(isValidFlowConnection(conn('tf', 'case:c1', 'out', 'value'), all, [])).toBe(false);
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test flow-transform flow-wiring`
Expected: FAIL. `flow-transform.test.ts` cannot import `../flow-transform`, and the Transform wiring tests fail because `targetAccepts` has no `Transform` case (it returns `undefined`, so connections are refused).

- [ ] **Step 3: Add the type and the constant**

In `src/lib/tauri-api.ts`, add to the `FlowNodeKind` union after the `WaitForCallback` member:

```ts
  | {
      kind: 'Transform';
      label: string;
      /** One expression or a function body that returns a value. It reads `response`. */
      script: string;
    };
```

Make sure the previous member now ends without the final `;` (the union's last member carries it).

Create `src/lib/flow-transform.ts`:

```ts
/** The script a new Transform node starts with. Matches `TRANSFORM_DEFAULT_SCRIPT` in rocket-flow. */
export const DEFAULT_TRANSFORM_SCRIPT = 'return response.body;';
```

- [ ] **Step 4: Teach the wiring rules the new kind**

In `src/lib/flow-wiring.ts`, in `sourceHandleExists`, add the case to the group that only has `result`:

```ts
    case 'Request':
    case 'Input':
    case 'WaitForCallback':
    case 'Transform':
      return handle === RESULT_HANDLE;
```

In `targetAccepts`, add it to the `input` group:

```ts
    case 'If':
    case 'Switch':
    case 'Transform':
      return handle === INPUT_HANDLE;
```

- [ ] **Step 5: Keep the panel switch exhaustive**

In `src/components/flow/properties/NodePropertiesPanel.tsx`, add after the `WaitForCallback` case:

```tsx
    case 'Transform':
      // Plan 05 replaces this with the script editor.
      return <LabelOnlyEditor kind={kind} onChange={onChange} />;
```

- [ ] **Step 6: Run the checks**

Run: `yarn test flow-transform flow-wiring NodePropertiesPanel`
Expected: PASS.

Run: `yarn tsc --noEmit`
Expected: no errors. If a further exhaustive `switch` on `kind.kind` fails to compile, add the smallest `Transform` case that keeps behavior, and list the file in the commit body.

- [ ] **Step 7: Commit**

Stage `src/lib/tauri-api.ts`, `src/lib/flow-transform.ts`, `src/lib/__tests__/flow-transform.test.ts`, `src/lib/flow-wiring.ts`, `src/lib/__tests__/flow-wiring.test.ts` and `src/components/flow/properties/NodePropertiesPanel.tsx`, then commit through the `dev-workflow-skills:1-git-commit` skill with subject `feat(flow): add Transform node type and wiring`.

---

### Task 2: One input per Transform

**Files:**
- Modify: `src/lib/flow-handles.ts`
- Modify: `src/lib/__tests__/flow-handles.test.ts`
- Modify: `src/lib/flow-wiring.ts` (`isValidFlowConnection` and its import)
- Modify: `src/lib/__tests__/flow-wiring.test.ts`

**Interfaces:**
- Consumes: the `Transform` type from Task 1.
- Produces: `takesSingleInput(kind: FlowNodeKind): kind is SingleInputKind` exported from `src/lib/flow-handles.ts`. `isRoutingKind` keeps meaning "If or Switch" and is not changed.

- [ ] **Step 1: Write the failing tests**

In `src/lib/__tests__/flow-handles.test.ts`, add `takesSingleInput` to the existing import from `../flow-handles`, then add after the `recognises only If and Switch as routing kinds` test:

```ts
  it('recognises If, Switch and Transform as single-input kinds', () => {
    const kinds: FlowNodeKind[] = [
      { kind: 'If', label: 'i', condition: 'true' },
      { kind: 'Switch', label: 's', value: 'x', cases: [] },
      { kind: 'Transform', label: 't', script: 'return 1;' },
      { kind: 'Output', label: 'o' },
      { kind: 'Input', label: 'in', value: 'v' },
      { kind: 'Request', label: 'r', source: { type: 'Saved', requestPath: 'a.yml' } },
    ];
    expect(kinds.map(takesSingleInput)).toEqual([true, true, true, false, false, false]);
  });

  it('does not treat Transform as a routing kind', () => {
    expect(isRoutingKind({ kind: 'Transform', label: 't', script: 'return 1;' })).toBe(false);
  });
```

Append to the `Transform wiring` describe in `src/lib/__tests__/flow-wiring.test.ts`:

```ts
  const edgeInto = (target: string, field: string): FlowEdge => ({
    id: `e-${target}-${field}`,
    sourceNodeId: 'req',
    targetNodeId: target,
    targetField: field,
    expression: '',
  });

  it('rejects a second wire into a Transform input', () => {
    const edges = [edgeInto('tf', 'input')];
    expect(isValidFlowConnection(conn('inp', 'result', 'tf', 'input'), all, edges)).toBe(false);
  });

  it('allows the first wire when only other nodes have wires', () => {
    const edges = [edgeInto('out', 'value')];
    expect(isValidFlowConnection(conn('inp', 'result', 'tf', 'input'), all, edges)).toBe(true);
  });

  it('still refuses an input wire into a Request', () => {
    expect(isValidFlowConnection(conn('inp', 'result', 'req', 'input'), all, [])).toBe(false);
  });
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test flow-handles flow-wiring`
Expected: FAIL. `takesSingleInput` is not exported, and `rejects a second wire into a Transform input` fails because the rule only checks routing kinds.

- [ ] **Step 3: Add the helper**

In `src/lib/flow-handles.ts`, after `isRoutingKind`, add:

```ts
export type SingleInputKind = Extract<FlowNodeKind, { kind: 'If' | 'Switch' | 'Transform' }>;

/** True for the kinds that evaluate one upstream value through an `input` handle. */
export function takesSingleInput(kind: FlowNodeKind): kind is SingleInputKind {
  return kind.kind === 'If' || kind.kind === 'Switch' || kind.kind === 'Transform';
}
```

- [ ] **Step 4: Use it in the connection rule**

In `src/lib/flow-wiring.ts`, replace `isRoutingKind` with `takesSingleInput` in the import list (keep the list sorted the way Biome expects), and change the rule:

```ts
  // An If, Switch or Transform node evaluates exactly one input.
  if (takesSingleInput(targetNode.kind) && edges.some((e) => e.targetNodeId === target)) {
    return false;
  }
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `yarn test flow-handles flow-wiring`
Expected: PASS, including every existing If/Switch/Output/Wait test.

- [ ] **Step 6: Commit**

Stage the four files, then commit through the `dev-workflow-skills:1-git-commit` skill with subject `feat(flow): allow one input per Transform node`.

---

### Task 3: Wire building and full frontend check

**Files:**
- Modify: `src/lib/__tests__/flow-wiring.test.ts`

**Interfaces:**
- Consumes: `buildEdgeFromConnection`, `shouldPromptForExpression` and the `Transform` type.
- Produces: regression tests only, plus a clean `yarn tsc --noEmit`, `yarn check` and targeted test run that plan 04 can rely on.

- [ ] **Step 1: Write the tests**

Append to the `Transform wiring` describe in `src/lib/__tests__/flow-wiring.test.ts`:

```ts
  it('does not prompt for an expression on a Transform input wire', () => {
    const edge = buildEdgeFromConnection(
      { source: 'req', sourceHandle: 'result', target: 'tf', targetHandle: 'input' },
      req,
    );
    expect(edge).not.toBeNull();
    expect(edge?.expression).toBe('');
    expect(edge && shouldPromptForExpression(edge)).toBe(false);
  });

  it('omits the result exit on a Transform wire', () => {
    const edge = buildEdgeFromConnection(
      { source: 'tf', sourceHandle: 'result', target: 'out', targetHandle: 'value' },
      transform,
    );
    expect(edge).not.toBeNull();
    expect(edge?.sourceHandle).toBeUndefined();
    expect(edge?.expression).toBe('response.body');
  });
```

- [ ] **Step 2: Run the tests**

Run: `yarn test flow-wiring`
Expected: PASS. The behavior already exists (`input` is a data-less target, and `result` is omitted), so these tests pin it. If a test fails, fix the production code in `src/lib/flow-wiring.ts`, not the test.

- [ ] **Step 3: Run the full frontend checks**

Run: `yarn tsc --noEmit`
Expected: no errors.

Run: `yarn check`
Expected: no lint or format errors. If Biome reports formatting problems in files this plan touched, run `yarn format` and re-run `yarn check`.

Run: `yarn test flow`
Expected: PASS for every flow test file.

- [ ] **Step 4: Commit**

Stage `src/lib/__tests__/flow-wiring.test.ts` and any file `yarn format` changed in this plan, then commit through the `dev-workflow-skills:1-git-commit` skill with subject `test(flow): cover Transform wire building`.

---

## Next Plan

**Next plan to execute:** `docs/superpowers/plans/flow-phase3-transform-node/2026-09-30-flow-phase3-transform-plan-04-canvas-node-and-palette.md`

Do not start it in this run. Finish the review below, report to the user, and wait for them to start plan 04.

## Post-Implementation Review

Before plan 04 starts, dispatch one Opus-model subagent (read and fix allowed) with this brief: "Review every change made by plan 03 of `docs/superpowers/plans/flow-phase3-transform-node/`, using `git log` for its three commits. Check: (a) the wiring rules match `crates/rocket-flow/src/validate.rs` for Transform (one `input` wire, `result` exit only, no trigger); (b) `isRoutingKind` is unchanged and still means If or Switch; (c) `yarn tsc --noEmit`, `yarn check` and `yarn test flow` are green. Fix any defect you find and report what changed. Do not start plan 04."
