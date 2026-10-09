# Backend Lint Feed Implementation Plan

> **Execution:** reuse the existing worktree `/home/numericlabs/data/rocket/rocket/.claude/worktrees/flow-p20` (user instruction: no new worktree); start the branch there with `git switch -c worktree-flow-p21 main` after P23 is merged.

> **Facts changed since this plan was written:** P20 (run from a node) and P22 (F-03, client run ids) are merged, so run-related code in `FlowToolbar` and `FlowPane` has changed. Locate every edit by the quoted code, not by line numbers or by what this plan assumes those files looked like. F-20 is now planned as P23 and its "Deviations" section is applied below.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **BLOCKED. Do not start before roadmap F-20 (`validate_with_warnings`, the lint tier) lands, and P12 (client issue badges) is merged.** F-20 is plan P23 (`2026-10-09-p23-flow-lint-tier.md`). This plan consumes the exact F-20 contract in "Required from F-20" below. If F-20 lands with a different shape, amend F-20 to match (or update this plan's Task 1 mapping before starting); do not add a second lint engine here. P12 provides `src/lib/flow-issues.ts` with `FlowIssue = { code; severity: 'error' | 'warning'; nodeId?; edgeId?; message; hint? }` and `computeFlowIssues`.

**Goal:** Feed the backend lint tier into the canvas issue badges while the user edits, merge it with the client rules without duplicates, and delete the client rules the backend now owns, so each rule has one source.

**Architecture:** A thin IPC command `lint_flow(collection, flow)` lints the unsaved canvas graph and returns `FlowLintDto[]` (camelCase). A debounced hook (500 ms) calls it when a flow opens and after edits, ignores out-of-date responses, and never throws. `mergeFlowIssues` combines client and backend issues by `(code, nodeId, edgeId)`, backend first. A golden JSON fixture is read by a Rust test and a TypeScript test, so the DTO cannot drift silently (a small slice of roadmap F-49).

**Tech Stack:** Rust (`src-tauri`, package `rocket`), React, TypeScript, Vitest with fake timers, Testing Library `renderHook`.

**Spec:** Roadmap items F-37 (split F-37b) and F-20 in `.claude/flow-roadmap.md`. Design notes: `docs/superpowers/plans/flow-tier3/01-design-notes.md` sections "P12" and "P21". F-20 plan: `docs/superpowers/plans/flow-tier3/2026-10-09-p23-flow-lint-tier.md`.

## Required from F-20

P21 relies on exactly these items. F-20's plan must produce them.

```rust
// crates/rocket-flow/src/lint.rs, exported as `rocket_flow::lint`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowLint {
    pub code: String,
    pub severity: LintSeverity,
    pub node_id: Option<String>,
    pub edge_id: Option<String>,
    /// Names the node by label, never by a resolved value.
    pub message: String,
    pub hint: Option<String>,
}

// crates/rocket-app/src/flow_service.rs
impl FlowService {
    /// Lints `flow` as given (not the saved file). A structural `validate`
    /// failure is one `invalid_graph` error lint per offending node and per
    /// offending wire, each carrying its node or edge id. Then the warnings of `validate_with_warnings`. Never fails: a
    /// lint the context cannot evaluate is skipped.
    pub fn lint(&self, collection: &str, flow: &Flow) -> Vec<FlowLint>;
}
```

Lint codes (shared with the client `FlowIssue.code`):

| Code | Severity | Source | Client rule in P12 today |
|---|---|---|---|
| `invalid_graph` | error | `validate` (V1 to V15) | partly copied (V1, V7, V8, V9, V10); kept, see Task 3 |
| `dangling_saved_request` | error | F-21 | none |
| `unknown_variable` | warning | F-22 | none |
| `exit_without_edge` | warning | P23 (F-20 core, F-23 subset) | `exit-unwired` (If exits and Switch cases), deleted in Task 3 |
| `switch_without_default` | warning | P23 (F-20 core, F-23 subset) | `exit-unwired` (Switch default part), deleted in Task 3 |
| `no_path_to_output` | warning | P23 (F-20 core, F-23 subset) | `no-path-to-output`, deleted in Task 3 |
| `auth_no_effect` | warning | F-24 | none |
| `auth_wire_overrides_auth` | warning | F-24 | none |
| `callback_not_wired` | error | F-25 | none |

P12's client code `save` (a rejected save) is the same rule as `invalid_graph`; `mergeFlowIssues` treats them as one.

P12's other client codes are kebab-case (`exit-unwired`, `no-path-to-output`), so `mergeFlowIssues` never dedupes them against the snake_case backend codes. Task 3 deletes those client rules instead. On a Switch with unwired cases and an unwired default, the backend reports two lints where the client reported one `exit-unwired` issue, so that node's issue count goes from 1 to 2.

## Global Constraints

- Every task that touches the command or the DTO starts with: Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.
- shadcn/ui primitives only, `lucide-react` icons only (this plan adds no UI elements; badges are P12's).
- Zustand: narrow selectors only. Rust: no unwrap calls in production paths; `serde(rename_all = "camelCase")` on the DTO only, never on `FlowLint`.
- Lints never block Run or Save. A failed lint call shows no backend issues and logs a warning.
- Code comments are short full sentences that end with a punctuation mark.
- Commits use conventional commit format through the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only. Never `git add -A`, `--all` or `.`.
- Gates before each commit: `yarn tsc --noEmit`, `yarn check`, the targeted `yarn test <pattern>`, and `cargo check -j4 -p rocket` when Rust changes. Never `--workspace`.
- Not in scope: new lint rules (F-21 to F-25), the full golden-fixture contract suite (F-49), lint on save in the backend.

## Review Focus

Failure modes the spec implies but the obvious tests would miss, most likely first:

1. An older lint response that arrives after a newer one must not overwrite it, and a response after unmount must be dropped. Pinned in Task 2: `ignores a response that is out of date`.
2. Typing must cause one lint call per pause, not one per keystroke, and opening a flow must lint once. Pinned in Task 2: `lints once after a pause in editing`.
3. A failing `lint_flow` call must not throw, block Run or keep stale issues on screen. Pinned in Task 2: `shows no backend issues when the lint call fails`.
4. The same rule from both sides must show once (`code`, `nodeId`, `edgeId`, with `save` equal to `invalid_graph`), and a rule only the client knows must stay. Pinned in Task 2: `flow-lint.test.ts`.
5. A renamed DTO key or severity string must fail a test on both sides. Pinned in Task 1: the golden fixture tests in Rust and TypeScript.

---

## File Structure

| File | Responsibility |
|---|---|
| `src/lib/__tests__/fixtures/flow-lint.json` (new) | Golden `lint_flow` payload read by both test suites. |
| `src-tauri/src/commands/flow.rs` (modify) | `FlowLintSeverityDto`, `FlowLintDto`, `lint_flow` command, tests. |
| `src-tauri/src/lib.rs` (modify) | Registers `lint_flow` next to `save_flow`. |
| `src/lib/tauri-api.ts` (modify) | `FlowLint`, `FlowLintSeverity`, `lintFlow`. |
| `src/lib/flow-lint.ts` (new) | `toFlowIssue`, `mergeFlowIssues`, `BACKEND_LINT_CODES`, `LINT_DEBOUNCE_MS`. |
| `src/hooks/useBackendFlowLints.ts` (new) | Debounced lint hook with stale-response guard. |
| `src/components/flow/FlowPane.tsx` (modify) | Merges backend issues into P12's issue list. |
| `src/lib/flow-issues.ts` (modify) | Deletes the three client rules the backend owns. |
| Tests: `src/lib/__tests__/flow-lint.test.ts`, `src/hooks/__tests__/useBackendFlowLints.test.tsx` (new), `src/lib/__tests__/flow-issues.test.ts` (modify) | |

---

### Task 1: `lint_flow` command and DTO contract

**Files:**
- Create: `src/lib/__tests__/fixtures/flow-lint.json`
- Modify: `src-tauri/src/commands/flow.rs` (after `save_flow`, near line 456; tests module)
- Modify: `src-tauri/src/lib.rs` (handler list; `commands::flow::save_flow,` is near line 683)
- Modify: `src/lib/tauri-api.ts` (after `saveFlow`, near line 2175)
- Create: `src/lib/__tests__/tauri-api.flow-lint.test.ts`

**Interfaces:**
- Consumes: `rocket_flow::lint::{FlowLint, LintSeverity}` and `FlowService::lint` from F-20.
- Produces: Tauri command `lint_flow(collection: String, flow: FlowDto) -> Result<Vec<FlowLintDto>, DomainError>`.
- Produces: wire shape `{ code: string, severity: "error" | "warning", nodeId?: string, edgeId?: string, message: string, hint?: string }`; optional keys are omitted, never `null`.
- Produces: TS `lintFlow(collection: string, flow: Flow): Promise<FlowLint[]>`.

- [ ] **Step 1: Read the spec reference**

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Add the golden fixture**

Create `src/lib/__tests__/fixtures/flow-lint.json`:

```json
[
  {
    "code": "exit_without_edge",
    "severity": "warning",
    "nodeId": "check",
    "message": "The 'false' exit of 'Check status' has no wire.",
    "hint": "Wire it to a node, or remove the branch."
  },
  {
    "code": "invalid_graph",
    "severity": "error",
    "edgeId": "e7",
    "message": "an 'auth' wire must go from an Auth node into a Request node"
  }
]
```

- [ ] **Step 3: Write the failing Rust tests**

In `src-tauri/src/commands/flow.rs`, inside `mod tests`:

```rust
    use rocket_flow::lint::{FlowLint, LintSeverity};

    const LINT_FIXTURE: &str = include_str!("../../../src/lib/__tests__/fixtures/flow-lint.json");

    #[test]
    fn lint_dto_matches_the_golden_fixture() {
        let parsed: Vec<FlowLintDto> = serde_json::from_str(LINT_FIXTURE).expect("fixture parses");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].severity, FlowLintSeverityDto::Warning);
        assert_eq!(parsed[1].node_id, None);
        let reserialized = serde_json::to_value(&parsed).expect("serialize");
        let original: serde_json::Value = serde_json::from_str(LINT_FIXTURE).expect("fixture json");
        assert_eq!(reserialized, original, "keys, casing and omitted optionals must match");
    }

    #[test]
    fn lint_dto_maps_every_field_of_a_domain_lint() {
        let dto = FlowLintDto::from(FlowLint {
            code: "exit_without_edge".to_string(),
            severity: LintSeverity::Warning,
            node_id: Some("check".to_string()),
            edge_id: None,
            message: "The 'false' exit of 'Check status' has no wire.".to_string(),
            hint: Some("Wire it to a node, or remove the branch.".to_string()),
        });
        let json = serde_json::to_string(&dto).expect("serialize");
        assert_eq!(
            json,
            r#"{"code":"exit_without_edge","severity":"warning","nodeId":"check","message":"The 'false' exit of 'Check status' has no wire.","hint":"Wire it to a node, or remove the branch."}"#
        );
    }

    #[test]
    fn an_error_lint_serializes_its_severity_as_error() {
        let dto = FlowLintDto::from(FlowLint {
            code: "invalid_graph".to_string(),
            severity: LintSeverity::Error,
            node_id: None,
            edge_id: Some("e7".to_string()),
            message: "bad wire".to_string(),
            hint: None,
        });
        let json = serde_json::to_string(&dto).expect("serialize");
        assert_eq!(
            json,
            r#"{"code":"invalid_graph","severity":"error","edgeId":"e7","message":"bad wire"}"#
        );
    }
```

- [ ] **Step 4: Run them to verify they fail**

Run: `cargo test -j4 -p rocket lint_dto`
Expected: FAIL to compile (`FlowLintDto` does not exist).

- [ ] **Step 5: Add the DTO and the command**

In `src-tauri/src/commands/flow.rs`, after `save_flow`:

```rust
/// Severity of one lint. IPC DTO.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FlowLintSeverityDto {
    Error,
    Warning,
}

/// One finding of the lint tier, shaped like the client `FlowIssue`. IPC DTO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowLintDto {
    pub code: String,
    pub severity: FlowLintSeverityDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_id: Option<String>,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl From<rocket_flow::lint::FlowLint> for FlowLintDto {
    fn from(lint: rocket_flow::lint::FlowLint) -> Self {
        Self {
            code: lint.code,
            severity: match lint.severity {
                rocket_flow::lint::LintSeverity::Error => FlowLintSeverityDto::Error,
                rocket_flow::lint::LintSeverity::Warning => FlowLintSeverityDto::Warning,
            },
            node_id: lint.node_id,
            edge_id: lint.edge_id,
            message: lint.message,
            hint: lint.hint,
        }
    }
}

/// Lints the graph the canvas holds now, saved or not. Never fails because
/// of what the graph contains: problems come back as lints.
#[tauri::command]
pub fn lint_flow(
    collection: String,
    flow: FlowDto,
    svc: State<'_, FlowService>,
) -> Result<Vec<FlowLintDto>, DomainError> {
    Ok(svc
        .lint(&collection, &flow.into())
        .into_iter()
        .map(FlowLintDto::from)
        .collect())
}
```

In `src-tauri/src/lib.rs`, add after `commands::flow::save_flow,`:

```rust
            commands::flow::lint_flow,
```

- [ ] **Step 6: Run the Rust tests**

Run: `cargo test -j4 -p rocket lint_dto && cargo test -j4 -p rocket an_error_lint && cargo check -j4 -p rocket`
Expected: PASS and no errors.

- [ ] **Step 7: Write the failing TypeScript wrapper test**

Create `src/lib/__tests__/tauri-api.flow-lint.test.ts`:

```ts
import { invoke } from '@tauri-apps/api/core';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { type Flow, lintFlow } from '../tauri-api';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const flow: Flow = { name: 'f', nodes: [], edges: [] };

describe('lintFlow', () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it('sends the unsaved graph and returns the lints', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await expect(lintFlow('demo', flow)).resolves.toEqual([]);
    expect(invoke).toHaveBeenCalledWith('lint_flow', { collection: 'demo', flow });
  });
});
```

Run: `yarn test src/lib/__tests__/tauri-api.flow-lint.test.ts`
Expected: FAIL (`lintFlow` is not exported).

- [ ] **Step 8: Add the wrapper**

In `src/lib/tauri-api.ts`, after `saveFlow`:

```ts
export type FlowLintSeverity = 'error' | 'warning';

/** One finding of the backend lint tier. Optional keys are omitted, never null. */
export interface FlowLint {
  code: string;
  severity: FlowLintSeverity;
  nodeId?: string;
  edgeId?: string;
  message: string;
  hint?: string;
}

/** Lints the graph as the canvas holds it now, saved or not. */
export const lintFlow = (collection: string, flow: Flow) =>
  invoke<FlowLint[]>('lint_flow', { collection, flow });
```

Run: `yarn test src/lib/__tests__/tauri-api.flow-lint.test.ts`
Expected: PASS.

- [ ] **Step 9: Gates and commit**

Run: `yarn tsc --noEmit && yarn check && cargo check -j4 -p rocket`
Expected: no errors.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/lib/__tests__/fixtures/flow-lint.json src-tauri/src/commands/flow.rs src-tauri/src/lib.rs src/lib/tauri-api.ts src/lib/__tests__/tauri-api.flow-lint.test.ts`
Suggested subject: `feat(flow): add the lint_flow command`.

---

### Task 2: Debounced hook and merge

**Files:**
- Create: `src/lib/flow-lint.ts`
- Create: `src/lib/__tests__/flow-lint.test.ts`
- Create: `src/hooks/useBackendFlowLints.ts`
- Create: `src/hooks/__tests__/useBackendFlowLints.test.tsx`
- Modify: `src/components/flow/FlowPane.tsx` (where P12 computes issues)
- Modify: every `src/components/flow/__tests__/FlowPane.*.test.tsx` that renders the open-flow path (mock `lintFlow`)

**Interfaces:**
- Consumes: `FlowLint`, `lintFlow` from Task 1; `FlowIssue`, `computeFlowIssues` from P12; `flowPayloadFromTab` from P1 (`src/lib/flow-save.ts`).
- Produces: `toFlowIssue(lint: FlowLint): FlowIssue`, `mergeFlowIssues(client: FlowIssue[], backend: FlowIssue[]): FlowIssue[]`, `BACKEND_LINT_CODES: readonly string[]`, `LINT_DEBOUNCE_MS = 500`.
- Produces: `useBackendFlowLints(collection: string | null, flow: Flow | null, delayMs?: number): FlowIssue[]`.

- [ ] **Step 1: Write the failing merge tests**

Create `src/lib/__tests__/flow-lint.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { FlowIssue } from '@/lib/flow-issues';
import type { FlowLint } from '@/lib/tauri-api';
import { BACKEND_LINT_CODES, mergeFlowIssues, toFlowIssue } from '../flow-lint';
import fixture from './fixtures/flow-lint.json';

const issue = (over: Partial<FlowIssue>): FlowIssue => ({
  code: 'x',
  severity: 'warning',
  message: 'm',
  ...over,
});

describe('toFlowIssue', () => {
  it('maps the golden lint_flow payload without loss', () => {
    const issues = (fixture as FlowLint[]).map(toFlowIssue);
    expect(issues).toEqual([
      {
        code: 'exit_without_edge',
        severity: 'warning',
        nodeId: 'check',
        message: "The 'false' exit of 'Check status' has no wire.",
        hint: 'Wire it to a node, or remove the branch.',
      },
      {
        code: 'invalid_graph',
        severity: 'error',
        edgeId: 'e7',
        message: "an 'auth' wire must go from an Auth node into a Request node",
      },
    ]);
  });
});

describe('mergeFlowIssues', () => {
  it('shows a rule reported by both sides once, with the backend text', () => {
    const client = [issue({ code: 'exit_without_edge', nodeId: 'n1', message: 'client' })];
    const backend = [issue({ code: 'exit_without_edge', nodeId: 'n1', message: 'backend' })];
    expect(mergeFlowIssues(client, backend)).toEqual(backend);
  });

  it('treats a rejected save and an invalid_graph lint on one node as one issue', () => {
    const client = [issue({ code: 'save', severity: 'error', nodeId: 'n1' })];
    const backend = [issue({ code: 'invalid_graph', severity: 'error', nodeId: 'n1' })];
    expect(mergeFlowIssues(client, backend)).toEqual(backend);
  });

  it('keeps rules only the client knows and issues on other nodes', () => {
    const blank = issue({ code: 'blank_condition', severity: 'error', nodeId: 'n1' });
    const other = issue({ code: 'exit_without_edge', nodeId: 'n2' });
    const backend = [issue({ code: 'exit_without_edge', nodeId: 'n1' })];
    expect(mergeFlowIssues([blank, other], backend)).toEqual([blank, other, ...backend]);
  });

  it('tells edge issues apart from node issues', () => {
    const onEdge = issue({ code: 'invalid_graph', severity: 'error', edgeId: 'e1' });
    const onNode = issue({ code: 'invalid_graph', severity: 'error', nodeId: 'n1' });
    expect(mergeFlowIssues([onEdge], [onNode])).toEqual([onEdge, onNode]);
  });

  it('lists the codes the backend owns', () => {
    expect(BACKEND_LINT_CODES).toEqual(
      expect.arrayContaining(['exit_without_edge', 'switch_without_default', 'no_path_to_output']),
    );
  });
});
```

Run: `yarn test src/lib/__tests__/flow-lint.test.ts`
Expected: FAIL (`../flow-lint` does not exist).

- [ ] **Step 2: Write the merge module**

Create `src/lib/flow-lint.ts`:

```ts
import type { FlowIssue } from '@/lib/flow-issues';
import type { FlowLint } from '@/lib/tauri-api';

/** Wait this long after the last edit before linting in the backend. */
export const LINT_DEBOUNCE_MS = 500;

/** Rules only the backend reports. The client rule set must not repeat them. */
export const BACKEND_LINT_CODES = [
  'invalid_graph',
  'dangling_saved_request',
  'unknown_variable',
  'exit_without_edge',
  'switch_without_default',
  'no_path_to_output',
  'auth_no_effect',
  'auth_wire_overrides_auth',
  'callback_not_wired',
] as const;

// A rejected save names the same structural rule as an invalid_graph lint.
const CODE_ALIASES: Record<string, string> = { save: 'invalid_graph' };

const issueKey = (issue: FlowIssue) =>
  `${CODE_ALIASES[issue.code] ?? issue.code}|${issue.nodeId ?? ''}|${issue.edgeId ?? ''}`;

/** Maps one backend lint to the client issue shape, leaving out absent keys. */
export function toFlowIssue(lint: FlowLint): FlowIssue {
  return {
    code: lint.code,
    severity: lint.severity,
    ...(lint.nodeId ? { nodeId: lint.nodeId } : {}),
    ...(lint.edgeId ? { edgeId: lint.edgeId } : {}),
    message: lint.message,
    ...(lint.hint ? { hint: lint.hint } : {}),
  };
}

/**
 * Client issues first, then backend issues. A client issue the backend also
 * reports, by code, node and edge, is dropped in favour of the backend one.
 */
export function mergeFlowIssues(client: FlowIssue[], backend: FlowIssue[]): FlowIssue[] {
  const backendKeys = new Set(backend.map(issueKey));
  return [...client.filter((issue) => !backendKeys.has(issueKey(issue))), ...backend];
}
```

If `resolveJsonModule` is off in `tsconfig.json` and the fixture import fails to type-check, read the fixture in the test with `JSON.parse(readFileSync(new URL('./fixtures/flow-lint.json', import.meta.url), 'utf8'))` from `node:fs` instead.

Run: `yarn test src/lib/__tests__/flow-lint.test.ts`
Expected: PASS (6 tests).

- [ ] **Step 3: Write the failing hook tests**

Create `src/hooks/__tests__/useBackendFlowLints.test.tsx`:

```tsx
import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { type Flow, type FlowLint, lintFlow } from '@/lib/tauri-api';
import { useBackendFlowLints } from '../useBackendFlowLints';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, lintFlow: vi.fn() };
});

const flowNamed = (name: string): Flow => ({ name, nodes: [], edges: [] });
const lint = (code: string): FlowLint => ({ code, severity: 'warning', message: code });

describe('useBackendFlowLints', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.mocked(lintFlow).mockReset();
  });
  afterEach(() => vi.useRealTimers());

  const flush = () =>
    act(async () => {
      await vi.runAllTimersAsync();
    });

  it('lints once when a flow opens', async () => {
    vi.mocked(lintFlow).mockResolvedValue([lint('exit_without_edge')]);
    const { result } = renderHook(() => useBackendFlowLints('demo', flowNamed('a')));
    expect(lintFlow).not.toHaveBeenCalled();
    await flush();
    expect(lintFlow).toHaveBeenCalledTimes(1);
    expect(result.current.map((i) => i.code)).toEqual(['exit_without_edge']);
  });

  it('lints once after a pause in editing', async () => {
    vi.mocked(lintFlow).mockResolvedValue([]);
    const { rerender } = renderHook(({ flow }) => useBackendFlowLints('demo', flow), {
      initialProps: { flow: flowNamed('a') },
    });
    rerender({ flow: flowNamed('ab') });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(200);
    });
    rerender({ flow: flowNamed('abc') });
    await flush();
    expect(lintFlow).toHaveBeenCalledTimes(1);
    expect(lintFlow).toHaveBeenCalledWith('demo', flowNamed('abc'));
  });

  it('does not lint again when a render brings an equal graph', async () => {
    vi.mocked(lintFlow).mockResolvedValue([]);
    const { rerender } = renderHook(({ flow }) => useBackendFlowLints('demo', flow), {
      initialProps: { flow: flowNamed('a') },
    });
    await flush();
    rerender({ flow: flowNamed('a') });
    await flush();
    expect(lintFlow).toHaveBeenCalledTimes(1);
  });

  it('ignores a response that is out of date', async () => {
    let answerOld: (lints: FlowLint[]) => void = () => undefined;
    vi.mocked(lintFlow)
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            answerOld = resolve;
          }),
      )
      .mockResolvedValueOnce([lint('new')]);
    const { result, rerender } = renderHook(({ flow }) => useBackendFlowLints('demo', flow), {
      initialProps: { flow: flowNamed('a') },
    });
    await flush();
    rerender({ flow: flowNamed('b') });
    await flush();
    expect(result.current.map((i) => i.code)).toEqual(['new']);
    await act(async () => answerOld([lint('old')]));
    expect(result.current.map((i) => i.code)).toEqual(['new']);
  });

  it('shows no backend issues when the lint call fails', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    vi.mocked(lintFlow).mockResolvedValueOnce([lint('first')]).mockRejectedValueOnce('boom');
    const { result, rerender } = renderHook(({ flow }) => useBackendFlowLints('demo', flow), {
      initialProps: { flow: flowNamed('a') },
    });
    await flush();
    rerender({ flow: flowNamed('b') });
    await flush();
    expect(result.current).toEqual([]);
    expect(warn).toHaveBeenCalled();
    warn.mockRestore();
  });

  it('does nothing without a collection or a flow', async () => {
    renderHook(() => useBackendFlowLints(null, flowNamed('a')));
    renderHook(() => useBackendFlowLints('demo', null));
    await flush();
    expect(lintFlow).not.toHaveBeenCalled();
  });
});
```

Run: `yarn test src/hooks/__tests__/useBackendFlowLints.test.tsx`
Expected: FAIL (`../useBackendFlowLints` does not exist).

- [ ] **Step 4: Write the hook**

Create `src/hooks/useBackendFlowLints.ts`:

```ts
import { useEffect, useRef, useState } from 'react';
import type { FlowIssue } from '@/lib/flow-issues';
import { LINT_DEBOUNCE_MS, toFlowIssue } from '@/lib/flow-lint';
import { type Flow, lintFlow } from '@/lib/tauri-api';

/**
 * Backend lint issues for the graph the canvas holds now. Lints when the
 * flow opens and after each pause in editing. A failed call shows no
 * backend issues; lints never block Run or Save.
 */
export function useBackendFlowLints(
  collection: string | null,
  flow: Flow | null,
  delayMs: number = LINT_DEBOUNCE_MS,
): FlowIssue[] {
  const [issues, setIssues] = useState<FlowIssue[]>([]);
  // Bumped by every cleanup, so a response from an older effect is ignored.
  const generationRef = useRef(0);
  // Read when the timer fires, so the effect depends on the graph's content only.
  const flowRef = useRef(flow);
  flowRef.current = flow;
  const key = collection && flow ? JSON.stringify(flow) : null;

  useEffect(() => {
    const generation = generationRef.current;
    if (!collection || key === null) {
      setIssues([]);
      return;
    }
    const timer = setTimeout(() => {
      const current = flowRef.current;
      if (!current) return;
      lintFlow(collection, current)
        .then((lints) => {
          if (generationRef.current === generation) setIssues(lints.map(toFlowIssue));
        })
        .catch((err: unknown) => {
          if (generationRef.current !== generation) return;
          console.warn('[flow-lint] lint_flow failed', err);
          setIssues([]);
        });
    }, delayMs);
    return () => {
      clearTimeout(timer);
      generationRef.current += 1;
    };
  }, [collection, key, delayMs]);

  return issues;
}
```

Run: `yarn test src/hooks/__tests__/useBackendFlowLints.test.tsx`
Expected: PASS (6 tests).

- [ ] **Step 5: Merge backend issues into the canvas issues**

In `src/components/flow/FlowPane.tsx`, find the `computeFlowIssues(` call P12 added (`grep -n computeFlowIssues src/components/flow/*.tsx`). If it lives in `FlowCanvas.tsx`, move it to `FlowPane.tsx` and pass `issues` down, so the canvas and the toolbar count share one list. Rename P12's result to `clientIssues`, then add below it:

```tsx
  // The backend lints the unsaved graph, so its issues follow every edit.
  const lintTarget = useMemo(() => flowPayloadFromTab(tab)?.flow ?? null, [tab]);
  const backendIssues = useBackendFlowLints(tab.collectionName, lintTarget);
  const issues = useMemo(
    () => mergeFlowIssues(clientIssues, backendIssues),
    [clientIssues, backendIssues],
  );
```

with imports:

```tsx
import { useBackendFlowLints } from '@/hooks/useBackendFlowLints';
import { mergeFlowIssues } from '@/lib/flow-lint';
```

(`flowPayloadFromTab` is already imported by P1.) Every place that used P12's issue list now uses `issues`. A new `tab` object on every status patch does not cause extra calls, because the hook keys on the graph's JSON.

In each existing FlowPane test file that renders the open-flow path (`FlowPane.test.tsx`, `FlowPane.delete.test.tsx`, `FlowPane.properties.test.tsx`, `FlowPane.saveShortcut.test.tsx` and the other `FlowPane.*.test.tsx` files), add `lintFlow: vi.fn().mockResolvedValue([])` to the `@/lib/tauri-api` mock, so the real `invoke` is never reached.

- [ ] **Step 6: Run the tests**

Run: `yarn test src/components/flow src/hooks src/lib/__tests__/flow-lint.test.ts`
Expected: PASS.

- [ ] **Step 7: Gates and commit**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/lib/flow-lint.ts src/lib/__tests__/flow-lint.test.ts src/hooks/useBackendFlowLints.ts src/hooks/__tests__/useBackendFlowLints.test.tsx src/components/flow/FlowPane.tsx` plus every `src/components/flow/__tests__/FlowPane.*.test.tsx` file changed in Step 5, listed by name.
Suggested subject: `feat(flow): show backend lint issues while editing`.

---

### Task 3: One source per rule

**Files:**
- Modify: `src/lib/flow-issues.ts`
- Modify: `src/lib/__tests__/flow-issues.test.ts`
- Create: `src/components/flow/__tests__/FlowPane.lintFeed.test.tsx`

**Interfaces:**
- Consumes: `BACKEND_LINT_CODES` from Task 2.
- Produces: `computeFlowIssues` no longer emits any code in `BACKEND_LINT_CODES`, and no longer emits `exit-unwired` or `no-path-to-output`. Its `save` code stays and is folded into `invalid_graph` by `mergeFlowIssues`.

- [ ] **Step 1: Write the failing guard test**

In `src/lib/__tests__/flow-issues.test.ts`, add a test that builds a graph which would trigger the client warnings: an If and a Switch with unwired exits, plus an Output node with no wire into it, so `no-path-to-output` can fire. `computeFlowIssues` takes its `ctx` argument as optional, so the test omits it:

```ts
import { BACKEND_LINT_CODES } from '@/lib/flow-lint';

it('leaves rules the backend owns to the lint feed', () => {
  const nodes = [
    { id: 'in', position: { x: 0, y: 0 }, kind: { kind: 'Input' as const, label: 'In', value: 'v' } },
    { id: 'chk', position: { x: 0, y: 0 }, kind: { kind: 'If' as const, label: 'Check', condition: 'true' } },
    {
      id: 'sw',
      position: { x: 0, y: 0 },
      kind: { kind: 'Switch' as const, label: 'Route', value: 'x', cases: [{ id: 'c1', label: 'one', matches: '1' }] },
    },
    { id: 'out', position: { x: 0, y: 0 }, kind: { kind: 'Output' as const, label: 'Out' } },
  ];
  const edges = [
    { id: 'e1', sourceNodeId: 'in', targetNodeId: 'chk', targetField: 'input', expression: '' },
    { id: 'e2', sourceNodeId: 'in', targetNodeId: 'sw', targetField: 'input', expression: '' },
  ];
  const codes = computeFlowIssues(nodes, edges).map((i) => i.code);
  for (const code of BACKEND_LINT_CODES) {
    expect(codes).not.toContain(code);
  }
  // P12's client codes are kebab case, so the loop above cannot catch them.
  expect(codes).not.toContain('exit-unwired');
  expect(codes).not.toContain('no-path-to-output');
});
```

If the `FlowNode` kind fields in `tauri-api.ts` differ from these literals (for example the Switch case shape), use the shapes the existing tests in this file use for If and Switch nodes.

Run: `yarn test src/lib/__tests__/flow-issues.test.ts`
Expected: FAIL (the client still emits `exit-unwired` and `no-path-to-output`).

- [ ] **Step 2: Delete the duplicated client rules**

In `src/lib/flow-issues.ts`:

1. No rename is needed. Delete `exitIssues` (and its `...exitIssues(node, edges)` call), `nodesReachingOutput`, the `reaching` variable and the `no-path-to-output` block inside `computeFlowIssues`. Then delete the imports that are left unused: `caseHandle`, `DEFAULT_HANDLE`, `FALSE_HANDLE`, `RESULT_HANDLE` and `TRUE_HANDLE` from `@/lib/flow-handles` (each is used only by `exitIssues` at HEAD; keep `takesSingleInput`, which `inputIssues` uses).
2. Keep every other client rule, including the ones that copy `validate` (blank condition, missing input, duplicate cases, Wait names, repeat-until limits) and `output-no-value`, `request-path-empty` and `request-url-empty`, which have no backend rule. `validate` stops at its first violation, so the client rules still show all of them at once while `invalid_graph` shows the first.
3. Add one comment above `computeFlowIssues`:

```ts
// Rules in BACKEND_LINT_CODES (src/lib/flow-lint.ts) come from lint_flow only.
// Do not add client copies of them.
```

In `src/lib/__tests__/flow-issues.test.ts`, delete the five tests in `describe('computeFlowIssues: warnings about the shape of the flow')`. The sample object with `code: 'exit-unwired'` near line 288 of that file is only a fixture for the summary helpers and may stay.

Run: `yarn test src/lib/__tests__/flow-issues.test.ts`
Expected: PASS.

- [ ] **Step 3: Write the end-to-end FlowPane test**

Create `src/components/flow/__tests__/FlowPane.lintFeed.test.tsx` by copying the imports, mocks, jsdom stubs, `baseTab`, `Harness` and `getFlowTab` from `src/components/flow/__tests__/FlowPane.delete.test.tsx`, adding `lintFlow: vi.fn()` to its `@/lib/tauri-api` mock and `lintFlow` to the `@/lib/tauri-api` import, then these tests:

```tsx
describe('FlowPane backend lint feed', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(baseTab);
  });

  it('lints the open flow and shows the backend warning', async () => {
    vi.mocked(lintFlow).mockResolvedValue([
      {
        code: 'no_path_to_output',
        severity: 'warning',
        nodeId: 'req1',
        message: "'Login' does not lead to an Output.",
      },
    ]);
    render(<Harness />);
    await waitFor(() =>
      expect(lintFlow).toHaveBeenCalledWith('demo', expect.objectContaining({ name: 'del' })),
    );
    expect(await screen.findByText(/1 issue/i)).toBeInTheDocument();
  });

  it('keeps the canvas usable when the lint call fails', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    vi.mocked(lintFlow).mockRejectedValue('lint crashed');
    render(<Harness />);
    await waitFor(() => expect(lintFlow).toHaveBeenCalled());
    expect(screen.getByRole('button', { name: 'Run' })).toBeEnabled();
    warn.mockRestore();
  });
});
```

The `/1 issue/i` text is P12's issue-count badge next to Run. If P12 labelled it differently, match P12's accessible name (its own FlowPane test shows it). The `baseTab` from the delete test has no other issue on `req1` once Task 3 removed the client copy of this rule; if P12's client rules still report an issue on that fixture, change the expected count accordingly.

Run: `yarn test src/components/flow/__tests__/FlowPane.lintFeed.test.tsx`
Expected: PASS.

- [ ] **Step 4: Full gates and commit**

Run: `yarn tsc --noEmit && yarn check && yarn test src/components/flow src/hooks src/lib && cargo check -j4 -p rocket`
Expected: all pass.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/lib/flow-issues.ts src/lib/__tests__/flow-issues.test.ts src/components/flow/__tests__/FlowPane.lintFeed.test.tsx`
Suggested subject: `refactor(flow): keep each lint rule in one place`.

---

## Self-Review

- **Spec coverage:** F-37b. Command and DTO with a drift guard (Task 1), debounced hook, stale-response guard and merge by `(code, nodeId, edgeId)` (Task 2), deletion of duplicated client rules with a guard test (Task 3). Lint on flow open is the hook's first run.
- **Placeholders:** none in the code. Steps that depend on P12 names say exactly what to look up (`computeFlowIssues` call site, `ctx` fixture, issue-count label) and what to do with it.
- **Type consistency:** `FlowLintDto` (Rust) and `FlowLint` (TS) share one fixture. `toFlowIssue` returns P12's `FlowIssue`. `mergeFlowIssues(client, backend)` and `useBackendFlowLints(collection, flow, delayMs?)` keep the same signatures in every step. `BACKEND_LINT_CODES` matches the table in "Required from F-20".
- **Review Focus coverage:** items 1 to 3 in Task 2 (hook tests); item 4 in Task 2 (merge tests); item 5 in Task 1 (fixture tests on both sides).
