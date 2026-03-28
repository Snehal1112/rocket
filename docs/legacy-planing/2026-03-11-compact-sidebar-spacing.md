# Compact Sidebar Spacing Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Improve the collections sidebar look and feel by regularizing
padding, margins, row heights, and indentation while keeping the compact
Postman-like density.

**Architecture:** Keep the existing sidebar structure and behavior, but
apply a consistent compact spacing system across the header, search,
collection rows, tree rows, and history rows. Remove ad hoc indentation
offsets and align hover-action gutters so dense layout feels deliberate.

**Tech Stack:** React 19, TypeScript, Tailwind CSS, Vitest

---

### Task 1: Add visual-structure tests where existing coverage is stable

**Files:**
- Check: `frontend/src/components/collections/CollectionsSidebar.tsx`
- Create or Modify: `frontend/src/components/collections/CollectionsSidebar.test.tsx`

**Step 1: Add focused structural assertions if practical**

If there is a stable component test harness for the sidebar, add
lightweight tests that assert class-level structure for:

- collection row container classes
- request row indentation hooks
- history row compact spacing classes

Keep these tests narrow. Do not snapshot the entire sidebar.

**Step 2: Run the focused test**

Run:

```bash
cd frontend && yarn test src/components/collections/CollectionsSidebar.test.tsx
```

Expected: either FAIL because classes differ from the intended compact
system, or skip this task if a stable test harness is not worth the
setup cost.

**Step 3: Commit test-only updates if needed**

```bash
git add frontend/src/components/collections/CollectionsSidebar.test.tsx
git commit -m "test(sidebar): cover compact spacing structure"
```

### Task 2: Normalize header, search, and collection row spacing

**Files:**
- Modify: `frontend/src/components/collections/CollectionsSidebar.tsx`

**Step 1: Refactor top-level spacing**

Update the sidebar header and search section so they use a consistent
compact inset and control rhythm.

Adjust:

- button heights
- gaps between controls
- section padding
- search input inset alignment

**Step 2: Normalize collection row spacing**

Make collection rows share:

- one compact row height
- one horizontal inset
- one aligned action gutter

Keep active and hover states intact.

**Step 3: Run targeted tests**

Run any existing sidebar/component tests if present, otherwise run the
existing frontend regression set that touches request-builder/sidebar
rendering.

```bash
cd frontend && yarn test src/components/request-builder/VariableAwareUrlInput.test.tsx src/App.test.tsx
```

Expected: PASS.

**Step 4: Commit**

```bash
git add frontend/src/components/collections/CollectionsSidebar.tsx
git commit -m "style(sidebar): normalize compact top-level spacing"
```

### Task 3: Unify tree indentation and row rhythm

**Files:**
- Modify: `frontend/src/components/collections/CollectionsSidebar.tsx`

**Step 1: Replace ad hoc nesting offsets**

Refactor tree indentation so folder and request rows use one consistent
indentation step per level.

Remove or reduce mixed combinations of:

- wrapper margins
- border offsets
- hard-coded padding bumps

where a single indentation rule can do the job.

**Step 2: Align request and folder rows**

Make folder and request rows feel part of the same compact system while
preserving:

- method label for requests
- active request rail
- folder expand/collapse affordance

**Step 3: Run targeted tests**

Run:

```bash
cd frontend && yarn test src/components/request-builder/VariableAwareUrlInput.test.tsx src/App.test.tsx
```

Expected: PASS.

**Step 4: Commit**

```bash
git add frontend/src/components/collections/CollectionsSidebar.tsx
git commit -m "style(sidebar): unify compact tree spacing"
```

### Task 4: Bring history rows onto the same compact spacing language

**Files:**
- Modify: `frontend/src/components/collections/CollectionsSidebar.tsx`

**Step 1: Tighten and align history rows**

Update history-row spacing so it matches the compact density of the
collections tab.

Preserve readability, but reuse the same inset and row-rhythm logic
where practical.

**Step 2: Verify no layout regressions**

Run:

```bash
cd frontend && yarn test src/lib/curl-parser.test.ts src/components/request-builder/VariableAwareUrlInput.test.tsx src/components/request-builder/useRequestBuilderState.test.tsx src/store/__tests__/collections-store.fetch-dedupe.test.ts src/App.test.tsx src/store/__tests__/history-store.fetch-dedupe.test.ts
```

Expected: PASS.

**Step 3: Manual verification**

Run:

```bash
cd frontend && yarn dev --host 127.0.0.1 --port 5173
```

Check:

- collection tab compactness
- nested tree scanability
- hover action alignment
- history tab density
- narrow-width appearance

**Step 4: Commit final changes**

```bash
git add frontend/src/components/collections/CollectionsSidebar.tsx frontend/src/components/collections/CollectionsSidebar.test.tsx
git commit -m "style(sidebar): tighten compact spacing rhythm"
```
