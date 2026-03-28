# Variable-Aware URL Input Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Highlight `{{variable}}` tokens inline in request URL and support hover-edit updates with env-first fallback behavior.

**Architecture:** Introduce a dedicated URL input component with overlay token rendering and hover popover editor. Keep existing substitution/send logic intact and update variables through existing collections/environment store functions.

**Tech Stack:** React, TypeScript, shadcn/ui Popover, Zustand, Tailwind, Yarn

---

### Task 1: Build variable parser + source resolution utilities

**Files:**
- Create: `frontend/src/lib/url-variables.ts`
- Optional test: `frontend/src/lib/__tests__/url-variables.test.ts`

**Step 1: Write the failing test**

Add tests for:
- extracting `{{var}}` tokens from URL
- source precedence (env over collection)
- unresolved token detection

**Step 2: Run test to verify it fails**

Run: `cd frontend && yarn -s test src/lib --run`
Expected: FAIL before utility implementation.

**Step 3: Write minimal implementation**

Implement pure helpers for token parsing and resolved metadata.

**Step 4: Run test to verify pass**

Run: `cd frontend && yarn -s test src/lib --run`
Expected: PASS.

**Step 5: Commit**

```bash
git add frontend/src/lib/url-variables.ts frontend/src/lib/__tests__/url-variables.test.ts
git commit -m "feat(frontend): add URL variable parsing and source resolution helpers"
```

### Task 2: Implement VariableAwareUrlInput component

**Files:**
- Create: `frontend/src/components/request-builder/VariableAwareUrlInput.tsx`

**Step 1: Write the failing test**

If component test harness exists, add tests for:
- token highlight rendering
- missing token style
- popover open on token hover

**Step 2: Run test to verify it fails**

Run: `cd frontend && yarn -s test src/components/request-builder --run`
Expected: FAIL before component implementation.

**Step 3: Write minimal implementation**

- controlled URL input API
- overlay token spans for highlights
- hover popover with editable variable value

**Step 4: Run test to verify pass**

Run: `cd frontend && yarn -s test src/components/request-builder --run`
Expected: PASS (or N/A if no component tests).

**Step 5: Commit**

```bash
git add frontend/src/components/request-builder/VariableAwareUrlInput.tsx
git commit -m "feat(frontend): add variable-aware URL input with hover edit popover"
```

### Task 3: Integrate with RequestBuilder and store update paths

**Files:**
- Modify: `frontend/src/components/request-builder/RequestBuilder.tsx`

**Step 1: Write the failing test**

Add/adjust tests (if available) for update behavior:
- env variable update when token exists in env
- collection fallback when not in env

**Step 2: Run test to verify red state**

Run: `cd frontend && yarn -s test src/components/request-builder --run`
Expected: FAIL before integration.

**Step 3: Write minimal implementation**

- Replace URL `Input` with `VariableAwareUrlInput`
- Pass active env + collection vars context
- On save in popover, apply env-first update path using existing store methods

**Step 4: Run test to verify pass**

Run: `cd frontend && yarn -s test src/components/request-builder --run`
Expected: PASS (or N/A if no tests).

**Step 5: Commit**

```bash
git add frontend/src/components/request-builder/RequestBuilder.tsx
git commit -m "feat(frontend): integrate variable hover edit into request URL field"
```

### Task 4: Verification and regressions

**Files:**
- Verify only

**Step 1: Write the failing test**

No new tests required.

**Step 2: Run checks**

Run:
- `cd frontend && yarn -s lint`
- `cd frontend && yarn -s test`
- `cd frontend && yarn -s build`

Expected: PASS.

**Step 3: Write minimal implementation**

Fix any regressions found.

**Step 4: Re-run checks**

Run same commands until stable.

**Step 5: Commit**

```bash
# only if regression fixes were needed
git add -A
git commit -m "chore(frontend): finalize variable-aware URL input verification"
```
