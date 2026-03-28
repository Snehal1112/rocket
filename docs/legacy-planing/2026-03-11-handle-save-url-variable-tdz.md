# Handle Save URL Variable TDZ Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Remove the request builder runtime TDZ error without changing variable persistence behavior.

**Architecture:** Keep the existing request-builder hook design and fix the issue by reordering callback declarations so `handleSaveUrlVariable` is initialized before any callback reads it. Add a focused frontend regression test that fails if the hook reintroduces the same initialization-order bug.

**Tech Stack:** React 19, TypeScript, Vitest, Testing Library, Zustand

---

### Task 1: Identify the smallest regression-test seam

**Files:**
- Modify: `frontend/src/components/request-builder/useRequestBuilderState.ts`
- Test: `frontend/src/components/request-builder/*.test.tsx`

**Step 1: Find existing tests around the request builder**

Run: `rg -n "useRequestBuilderState|RequestBuilder" frontend/src -g "*.test.ts*"`
Expected: existing request-builder tests or confirmation that a new focused test file is needed

**Step 2: Choose the narrowest render path**

Use either the hook directly through a minimal harness or render `RequestBuilder` with mocked stores, depending on which requires less setup.

**Step 3: Define the failing test**

Write a test that renders the chosen path and asserts render does not throw the initialization error.

**Step 4: Run the focused test to verify failure on the current bug**

Run: `cd frontend && yarn test --run <targeted-test-file>`
Expected: FAIL with `Cannot access 'handleSaveUrlVariable' before initialization`

**Step 5: Commit the test**

```bash
git add <targeted-test-file>
git commit -m "test(request-builder): cover URL variable TDZ"
```

### Task 2: Reorder the callback declarations

**Files:**
- Modify: `frontend/src/components/request-builder/useRequestBuilderState.ts`
- Test: `frontend/src/components/request-builder/*.test.tsx`

**Step 1: Move the shared callback**

Move the `handleSaveUrlVariable` `useCallback` block above `handleSubmit` so it is initialized before use.

**Step 2: Keep logic unchanged**

Do not alter the callback body, persistence routing, or dependencies unless required by the move.

**Step 3: Run the focused regression test**

Run: `cd frontend && yarn test --run <targeted-test-file>`
Expected: PASS

**Step 4: Run any adjacent request-builder tests**

Run: `cd frontend && yarn test --run <broader-request-builder-tests>`
Expected: PASS

**Step 5: Commit the implementation**

```bash
git add frontend/src/components/request-builder/useRequestBuilderState.ts <targeted-test-file>
git commit -m "fix(request-builder): avoid URL variable TDZ"
```

### Task 3: Final verification

**Files:**
- Modify: `frontend/src/components/request-builder/useRequestBuilderState.ts`
- Test: `frontend/src/components/request-builder/*.test.tsx`

**Step 1: Run the final targeted checks**

Run: `cd frontend && yarn test --run <targeted-test-file> <broader-request-builder-tests>`
Expected: PASS

**Step 2: Confirm no unintended diff in behavior**

Review the final diff to confirm only callback ordering and test coverage changed.

**Step 3: Prepare summary**

Record the exact test command used and the files changed for handoff.
