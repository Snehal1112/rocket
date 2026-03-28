# URL Variable Hover Popover Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace inline variable edit panel with Postman-like non-modal floating popover anchored to URL variable tokens.

**Architecture:** Add a shadcn HoverCard UI primitive wrapper and use it inside VariableAwareUrlInput so each highlighted token owns an anchored editor popover with current save behavior preserved.

**Tech Stack:** React, TypeScript, Radix Hover Card, shadcn/ui wrappers, Yarn

---

### Task 1: Add shadcn hover-card component

**Files:**
- Create: `frontend/src/components/ui/hover-card.tsx`

**Step 1: Write the failing test**

No direct tests; rely on compile + lint.

**Step 2: Run baseline check**

Run: `cd frontend && yarn -s lint`
Expected: PASS.

**Step 3: Write minimal implementation**

Add Radix Hover Card wrapper with `HoverCard`, `HoverCardTrigger`, `HoverCardContent`.

**Step 4: Run checks**

Run: `cd frontend && yarn -s lint`
Expected: PASS.

**Step 5: Commit**

```bash
git add frontend/src/components/ui/hover-card.tsx
git commit -m "feat(frontend): add shadcn hover-card primitive"
```

### Task 2: Refactor VariableAwareUrlInput to anchored popover

**Files:**
- Modify: `frontend/src/components/request-builder/VariableAwareUrlInput.tsx`

**Step 1: Write the failing test**

If component tests exist, assert hover card appears on token hover.

**Step 2: Run baseline checks**

Run: `cd frontend && yarn -s lint`
Expected: PASS before refactor.

**Step 3: Write minimal implementation**

- Wrap token spans with `HoverCardTrigger asChild`
- Move editor controls into `HoverCardContent`
- Keep existing save callback and source labels

**Step 4: Run checks**

Run:
- `cd frontend && yarn -s lint`
- `cd frontend && yarn -s test`
- `cd frontend && yarn -s build`

Expected: PASS.

**Step 5: Commit**

```bash
git add frontend/src/components/request-builder/VariableAwareUrlInput.tsx
git commit -m "fix(frontend): use anchored hover popover for URL variable editing"
```

### Task 3: Final verification

**Files:**
- Verify only

**Step 1: Write the failing test**

No new tests.

**Step 2: Run full checks**

Run:
- `cd frontend && yarn -s lint`
- `cd frontend && yarn -s test`
- `cd frontend && yarn -s build`

Expected: PASS.

**Step 3: Write minimal implementation**

Only regression fixes if needed.

**Step 4: Re-run checks**

Run same commands.

**Step 5: Commit**

```bash
# only if additional fixes required
git add -A
git commit -m "chore(frontend): finalize URL variable hover popover verification"
```
