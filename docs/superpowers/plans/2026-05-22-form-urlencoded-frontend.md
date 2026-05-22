# Form URL Encoded Body Mode — Frontend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose the already-implemented `application/x-www-form-urlencoded` body mode in the frontend UI so users can create and edit form-urlencoded request bodies.

**Architecture:** The Rust backend (`BodyMode::FormUrlEncoded`, `reqwest::RequestBuilder::form()`) and OpenCollection YAML persistence already handle this mode completely. The only work is wiring the `'formurlencoded'` string into 4 frontend files: the shared type union, the pane state type, the body mode selector dropdown, and the body editor renderer. The UI pattern is identical to `formdata` — both use `KeyValueEditor` over `body.formData`.

**Tech Stack:** TypeScript, React, Vitest, React Testing Library. No Rust changes. No IPC changes.

---

## File Map

| File | Action | What changes |
|---|---|---|
| `src/lib/tauri-api.ts` | Modify line 22 | Add `'formurlencoded'` to `BodyMode` union |
| `src/types/pane-types.ts` | Modify line 172 | Add `'formurlencoded'` to `BodyState['mode']` union |
| `src/components/request/RequestPanel.tsx` | Modify lines 84–91 | Add `Form URL Encoded` entry to `BODY_MODES` array |
| `src/components/request/BodyEditor.tsx` | Modify lines 83–93 | Add `formurlencoded` render block after `formdata` block |

---

## Task 1: Add `'formurlencoded'` to type definitions

**Files:**
- Modify: `src/lib/tauri-api.ts:22`
- Modify: `src/types/pane-types.ts:172`

These are the two type-level changes. Do them together — they're both one-liners in the type layer.

- [ ] **Step 1: Update `BodyMode` in tauri-api.ts**

In `src/lib/tauri-api.ts`, line 22 currently reads:

```typescript
export type BodyMode = 'none' | 'json' | 'xml' | 'text' | 'formdata' | 'binary';
```

Change it to:

```typescript
export type BodyMode = 'none' | 'json' | 'xml' | 'text' | 'formdata' | 'formurlencoded' | 'binary';
```

- [ ] **Step 2: Update `BodyState['mode']` in pane-types.ts**

In `src/types/pane-types.ts`, lines 171–177 currently read:

```typescript
export interface BodyState {
  mode: 'none' | 'json' | 'xml' | 'text' | 'formdata' | 'binary';
  content: string;
  formData: KeyValueEntry[];
  filePath?: string;
  fileName?: string;
}
```

Change the `mode` field to:

```typescript
export interface BodyState {
  mode: 'none' | 'json' | 'xml' | 'text' | 'formdata' | 'formurlencoded' | 'binary';
  content: string;
  formData: KeyValueEntry[];
  filePath?: string;
  fileName?: string;
}
```

- [ ] **Step 3: Verify TypeScript is still clean**

```bash
yarn tsc --noEmit
```

Expected: no output, exit 0. If there are errors, they'll point to any places in the codebase that exhaustively switch on `BodyMode` and need a new case — fix those before continuing.

- [ ] **Step 4: Commit**

```bash
git add src/lib/tauri-api.ts src/types/pane-types.ts
git commit -m "feat(body): add formurlencoded to BodyMode and BodyState types"
```

---

## Task 2: Add Form URL Encoded to the body mode selector

**Files:**
- Modify: `src/components/request/RequestPanel.tsx:84–91`

- [ ] **Step 1: Write a failing test**

There are no existing tests for `RequestPanel.tsx`. Create `src/components/request/__tests__/RequestPanel.test.tsx`:

```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

// Minimal mock — RequestPanel depends on many Tauri APIs.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@/stores/paneStore', () => ({
  usePaneStore: vi.fn(() => ({
    activeTabId: 'tab1',
    tabs: [],
  })),
}));

describe('RequestPanel body mode selector', () => {
  it('includes Form URL Encoded in the body mode dropdown', async () => {
    // This test verifies the dropdown option exists.
    // We look for the label string in BODY_MODES directly since
    // rendering the full component requires extensive mocking.
    const { BODY_MODES } = await import('../RequestPanel');
    const modes = BODY_MODES.map((m: { label: string; value: string }) => m.value);
    expect(modes).toContain('formurlencoded');
  });
});
```

- [ ] **Step 2: Export `BODY_MODES` from RequestPanel.tsx for the test**

`BODY_MODES` is currently a module-level `const`. Add `export` to it:

```typescript
export const BODY_MODES: { label: string; value: BodyState['mode'] }[] = [
  { label: 'None', value: 'none' },
  { label: 'JSON', value: 'json' },
  { label: 'XML', value: 'xml' },
  { label: 'Text', value: 'text' },
  { label: 'Form Data', value: 'formdata' },
  { label: 'Binary', value: 'binary' },
];
```

- [ ] **Step 3: Run the test to verify it fails**

```bash
yarn test RequestPanel --reporter=verbose
```

Expected: FAIL — `expect(modes).toContain('formurlencoded')` fails because `'formurlencoded'` is not in the array yet.

- [ ] **Step 4: Add Form URL Encoded to `BODY_MODES`**

In `src/components/request/RequestPanel.tsx`, update `BODY_MODES` to:

```typescript
export const BODY_MODES: { label: string; value: BodyState['mode'] }[] = [
  { label: 'None', value: 'none' },
  { label: 'JSON', value: 'json' },
  { label: 'XML', value: 'xml' },
  { label: 'Text', value: 'text' },
  { label: 'Form Data', value: 'formdata' },
  { label: 'Form URL Encoded', value: 'formurlencoded' },
  { label: 'Binary', value: 'binary' },
];
```

- [ ] **Step 5: Run the test to verify it passes**

```bash
yarn test RequestPanel --reporter=verbose
```

Expected: PASS.

- [ ] **Step 6: Type check**

```bash
yarn tsc --noEmit
```

Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add src/components/request/RequestPanel.tsx src/components/request/__tests__/RequestPanel.test.tsx
git commit -m "feat(body): add Form URL Encoded option to body mode selector"
```

---

## Task 3: Render the Form URL Encoded editor in BodyEditor

**Files:**
- Modify: `src/components/request/BodyEditor.tsx:83–93`

- [ ] **Step 1: Write a failing test**

Create `src/components/request/__tests__/BodyEditor.test.tsx`:

```typescript
import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { BodyEditor } from '../BodyEditor';
import type { BodyState } from '@/types/pane-types';

// Monaco is lazy-loaded — mock it so tests don't hang.
vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: () => <div data-testid='monaco' />,
}));

// Tauri dialog plugin — not used in formurlencoded mode but imported at top level.
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));

function makeBody(overrides: Partial<BodyState> = {}): BodyState {
  return {
    mode: 'none',
    content: '',
    formData: [],
    ...overrides,
  };
}

describe('BodyEditor', () => {
  it('renders KeyValueEditor for formurlencoded mode', () => {
    const body = makeBody({
      mode: 'formurlencoded',
      formData: [{ id: '1', key: 'username', value: 'alice', enabled: true }],
    });
    render(<BodyEditor body={body} onChange={vi.fn()} />);
    // KeyValueEditor renders the key value — verify it appears.
    expect(screen.getByDisplayValue('username')).toBeInTheDocument();
  });

  it('does not render form editor for none mode', () => {
    const body = makeBody({ mode: 'none' });
    render(<BodyEditor body={body} onChange={vi.fn()} />);
    expect(screen.queryByDisplayValue('username')).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
yarn test BodyEditor --reporter=verbose
```

Expected: FAIL — `formurlencoded` mode renders nothing (no `KeyValueEditor`), so `getByDisplayValue('username')` throws.

- [ ] **Step 3: Add the `formurlencoded` render block in BodyEditor.tsx**

In `src/components/request/BodyEditor.tsx`, after line 93 (the closing `}` of the `formdata` block), add:

```tsx
      {body.mode === 'formurlencoded' && (
        <KeyValueEditor
          entries={body.formData}
          onChange={setFormData}
          keyPlaceholder='Field name'
          valuePlaceholder='Value'
          addLabel='Add Field'
          variableContext={variableContext}
          onNavigateToSource={onNavigateToSource}
        />
      )}
```

The full section from line 83 onward should now look like:

```tsx
      {body.mode === 'formdata' && (
        <KeyValueEditor
          entries={body.formData}
          onChange={setFormData}
          keyPlaceholder='Field name'
          valuePlaceholder='Value'
          addLabel='Add Field'
          variableContext={variableContext}
          onNavigateToSource={onNavigateToSource}
        />
      )}

      {body.mode === 'formurlencoded' && (
        <KeyValueEditor
          entries={body.formData}
          onChange={setFormData}
          keyPlaceholder='Field name'
          valuePlaceholder='Value'
          addLabel='Add Field'
          variableContext={variableContext}
          onNavigateToSource={onNavigateToSource}
        />
      )}
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
yarn test BodyEditor --reporter=verbose
```

Expected: both tests PASS.

- [ ] **Step 5: Run the full test suite**

```bash
yarn test
```

Expected: all tests pass. No regressions.

- [ ] **Step 6: Type check and lint**

```bash
yarn tsc --noEmit && yarn check
```

Expected: clean (or only pre-existing Biome import-sort warnings unrelated to our files).

- [ ] **Step 7: Commit**

```bash
git add src/components/request/BodyEditor.tsx src/components/request/__tests__/BodyEditor.test.tsx
git commit -m "feat(body): render KeyValueEditor for formurlencoded body mode"
```

---

## Verification summary

```bash
yarn tsc --noEmit   # TypeScript — no new errors
yarn check          # Biome lint — no new errors
yarn test           # All tests pass
```

Manual smoke test:
1. `yarn tauri dev`
2. Open any request → Body tab → click the body mode selector
3. Confirm "Form URL Encoded" appears in the list between "Form Data" and "Binary"
4. Select it → confirm the key-value editor renders
5. Add a field (e.g. `username` = `alice`) → send the request → confirm `Content-Type: application/x-www-form-urlencoded` header appears in the response panel and the body is correctly encoded
