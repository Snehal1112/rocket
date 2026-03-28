# URL Input Path/Query Token Highlighting Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Highlight Bruno-style path and query tokens inside URL input text (alongside `{{...}}`) with resolved/missing status parity.

**Architecture:** Extend `VariableAwareUrlInput` token parsing and rendering to support `:pathToken` and query token candidates while preserving existing env/collection variable highlight and edit behavior.

**Tech Stack:** React, TypeScript, existing RequestBuilder + VariableAwareUrlInput, Vitest.

---

### Task 1: Add failing URL input tests for path/query token highlighting

**Files:**
- Modify/Create tests near: `frontend/src/components/request-builder/` (existing test file or new `VariableAwareUrlInput.test.tsx`)

**Step 1: Write failing test for path token highlight**
- URL: `{{BASE_URL}}/api/v3/network_invitations/:token/resend`
- Assert `:token` renders as highlighted token span.

**Step 2: Write failing test for query token highlight**
- URL with query token candidate (for example `?token=:token` or configured token reference).
- Assert query token is highlighted.

**Step 3: Write failing test for resolved/missing styles**
- Provide `pathParams`/`queryParams` fixtures with enabled and missing values.
- Assert class reflects resolved vs missing.

**Step 4: Run targeted test to verify RED state**
Run: `yarn -s test src/components/request-builder/VariableAwareUrlInput.test.tsx --run`
Expected: FAIL on missing tokenization/highlight behavior.

**Step 5: Commit failing tests**
```bash
git add frontend/src/components/request-builder/VariableAwareUrlInput.test.tsx
git commit -m "test(frontend): add failing URL path/query token highlight cases"
```

### Task 2: Extend token model and parser in VariableAwareUrlInput

**Files:**
- Modify: `frontend/src/components/request-builder/VariableAwareUrlInput.tsx`

**Step 1: Extend component props**
- Accept `pathParams` and `queryParams` inputs.

**Step 2: Extend token parsing**
- Keep existing `{{...}}` regex logic.
- Add parser for path tokens (`:name`) and query token candidates from URL string.

**Step 3: Add token resolution priority**
- Resolve token values from:
  1. enabled path/query params
  2. env vars
  3. collection vars
- Mark unresolved as missing.

**Step 4: Update render logic**
- Ensure path/query tokens render with same badge/status system.
- Preserve existing popover edit behavior for `{{...}}` tokens only.

**Step 5: Run targeted tests**
Run: `yarn -s test src/components/request-builder/VariableAwareUrlInput.test.tsx --run`
Expected: PASS.

**Step 6: Commit implementation**
```bash
git add frontend/src/components/request-builder/VariableAwareUrlInput.tsx frontend/src/components/request-builder/VariableAwareUrlInput.test.tsx
git commit -m "feat(frontend): highlight path and query tokens in URL input"
```

### Task 3: Wire RequestBuilder state into VariableAwareUrlInput

**Files:**
- Modify: `frontend/src/components/request-builder/RequestBuilder.tsx`

**Step 1: Pass params to URL input**
- Pass local `pathParams` and `queryParams` state to `VariableAwareUrlInput`.

**Step 2: Verify no behavior regression**
- Ensure URL editing and save/send flow still works unchanged.

**Step 3: Run focused tests + typecheck**
Run:
- `yarn -s test src/components/request-builder/VariableAwareUrlInput.test.tsx --run`
- `yarn -s tsc --noEmit`
Expected: PASS.

**Step 4: Commit wiring**
```bash
git add frontend/src/components/request-builder/RequestBuilder.tsx
git commit -m "refactor(frontend): wire request path/query params into URL token highlighter"
```

### Task 4: Regression verification

**Files:**
- No new files required

**Step 1: Run relevant frontend tests**
Run:
- `yarn -s test src/components/request-builder --run`
- `yarn -s test src/store/__tests__/tabs-store.save-target-and-dirty.test.ts --run`
Expected: PASS.

**Step 2: Manual acceptance check**
- Open request with URL `{{BASE_URL}}/api/v3/network_invitations/:token/resend`.
- Verify both `{{BASE_URL}}` and `:token` highlight in URL input.
- Toggle/mutate `pathParams.token`, verify resolved/missing visual state updates.

**Step 3: Capture final status**
Run:
- `git status --short`
- `git log --oneline -n 8`

