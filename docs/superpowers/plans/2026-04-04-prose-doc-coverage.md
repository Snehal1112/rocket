# prose-doc Markdown Coverage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add 10 CSS rules to `src/globals.css` so that h4/h5/h6, GFM strikethrough, task list checkboxes, and images render correctly inside `.prose-doc`.

**Architecture:** Pure CSS addition — append to the `.prose-doc` block in `src/globals.css`. No TypeScript, no Rust, no dependency changes.

**Tech Stack:** CSS (custom properties already defined via `hsl(var(--muted-foreground))` etc.), Tailwind v4 (preflight context), Tauri WebView (Chromium ≥ 105, supports CSS `:has()`).

**Spec:** `docs/superpowers/specs/2026-04-04-prose-doc-coverage-design.md`

---

### Task 1: Add missing `.prose-doc` CSS rules

**Files:**
- Modify: `src/globals.css` — append after the last `.prose-doc td` rule (currently line 164)

This is a CSS-only change with no tests to write first (pure visual rendering). The verification step is manual visual inspection in the running app.

- [ ] **Step 1: Append the 10 new rules to `src/globals.css`**

Open `src/globals.css`. Find the last `.prose-doc` rule (`.prose-doc td { ... }`). Append the following block immediately after it:

```css
.prose-doc h4 { font-size: 0.6875rem; font-weight: 600; margin: 0.5rem 0 0.25rem; }
.prose-doc h5 { font-size: 0.6875rem; font-weight: 600; margin: 0.5rem 0 0.25rem; color: hsl(var(--muted-foreground)); }
.prose-doc h6 { font-size: 0.6875rem; font-weight: 500; margin: 0.5rem 0 0.25rem; color: hsl(var(--muted-foreground)); }
.prose-doc del { text-decoration: line-through; }
.prose-doc ul:has(li > input[type="checkbox"]) { list-style: none; padding-left: 0.25rem; }
.prose-doc li:has(> input[type="checkbox"]) { display: flex; align-items: baseline; gap: 0.375rem; }
.prose-doc li > input[type="checkbox"] { width: 0.75rem; height: 0.75rem; flex-shrink: 0; }
.prose-doc img { max-width: 100%; height: auto; border-radius: 4px; margin-bottom: 0.5rem; }
```

The existing `.prose-doc` block ends at:
```css
.prose-doc td { padding: 0.25rem 0.5rem; border-bottom: 1px solid hsl(var(--border) / 0.5); color: hsl(var(--muted-foreground)); }
```

Append the new rules directly after that line (no blank line needed between, for consistency with the existing style).

- [ ] **Step 2: Verify TypeScript compiles clean**

Run:
```bash
yarn tsc --noEmit
```
Expected: no output (exit 0). These are CSS-only changes so TypeScript should be unaffected.

- [ ] **Step 3: Verify the frontend builds**

Run:
```bash
yarn build
```
Expected: build succeeds with no errors.

- [ ] **Step 4: Manual visual verification**

Start the dev server:
```bash
yarn tauri dev
```

Open a workspace, navigate to the Documentation panel. Paste the following test markdown into the workspace README and switch to preview:

```markdown
#### h4 heading
##### h5 heading
###### h6 heading

~~This text should have a strikethrough~~

- [x] Completed task
- [ ] Pending task
- Regular bullet (should still have disc)

![Test image](https://via.placeholder.com/800x200)
```

Check each element:
- h4: bold, same size as body text, foreground colour
- h5: bold, same size, muted colour
- h6: medium weight, same size, muted colour
- `~~strikethrough~~`: line through the text
- `- [x]` / `- [ ]`: checkbox visible, no disc marker, checkbox is small (~12px)
- Regular bullet: disc marker still present (`:has()` selector should not affect it)
- Image: fills width of the panel, does not overflow

- [ ] **Step 5: Commit**

```bash
git add src/globals.css
git commit -m "fix(css): add prose-doc rules for h4-h6, del, task lists, images"
```
