# Remove App Root Blue Gradient Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the blue gradient from the root app container so the center panel renders a flat background.

**Architecture:** Single class change in the root app div. No logic changes, no new files.

**Tech Stack:** React, TypeScript, Tailwind CSS

---

## File Map

| File | Change |
|------|--------|
| `src/App.tsx` | Remove `bg-gradient-to-br from-background via-background to-accent/25`, replace with `bg-background` |

---

## Task 1: Remove gradient from root app container

**Files:**
- Modify: `src/App.tsx:79`

- [ ] **Step 1: Apply the change**

In `src/App.tsx`, find line 79:

```tsx
    <div className="h-full flex flex-col overflow-hidden bg-gradient-to-br from-background via-background to-accent/25 text-sm">
```

Replace with:

```tsx
    <div className="h-full flex flex-col overflow-hidden bg-background text-sm">
```

- [ ] **Step 2: Run TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/App.tsx
git commit -m "fix: remove blue gradient from root app container"
```
