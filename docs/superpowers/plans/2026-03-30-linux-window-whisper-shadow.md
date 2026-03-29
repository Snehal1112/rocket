# Linux Window Whisper Shadow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the heavy directional box-shadow on the Linux app window with a near-invisible whisper shadow that reads as gentle elevation rather than a cast shadow.

**Architecture:** Single CSS property change in `src/index.css` on the `html.linux #root` rule. No Tauri, Rust, or component changes needed.

**Tech Stack:** TailwindCSS 4.2, plain CSS custom rule

---

### Task 1: Update the box-shadow value

**Files:**
- Modify: `src/index.css:193`

- [ ] **Step 1: Open the file and verify the current value**

Read `src/index.css` lines 189–196. Confirm line 193 contains:

```css
box-shadow: 0 2px 8px rgba(0, 0, 0, 0.25), 0 8px 32px rgba(0, 0, 0, 0.35);
```

- [ ] **Step 2: Replace the box-shadow value**

In `src/index.css`, inside the `html.linux #root` rule, change line 193 to:

```css
box-shadow: 0 0 0 1px rgba(0,0,0,0.04), 0 4px 20px rgba(0,0,0,0.08);
```

The full rule after the change:

```css
html.linux #root {
  background: var(--background);
  border-radius: var(--radius-sm);
  border: 1px solid color-mix(in srgb, var(--border) 80%, transparent);
  box-shadow: 0 0 0 1px rgba(0,0,0,0.04), 0 4px 20px rgba(0,0,0,0.08);
  overflow: hidden;
  height: 100%;
}
```

- [ ] **Step 3: Verify TypeScript and build are clean**

```bash
yarn tsc --noEmit
yarn build
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/index.css
git commit -m "fix(linux): replace heavy shadow with whisper box-shadow"
```

- [ ] **Step 5: Visual verification**

Run `yarn tauri dev` on Linux and confirm the window shows a barely-visible outer ring with a faint ambient haze — not a strong directional shadow.
