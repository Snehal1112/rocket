# Elevation — Plan 1: Tokens

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 1 of 4.** Previous plan: none. Branch: `feat/settings-shell` after commit `3b999fb8`.
**Next plan to run: `2026-10-31-elevation-plan-02-raised-sidebar.md`.**
**Recommended model: Sonnet.**

**Goal:** Define the four elevation levels as tokens, classes and Tailwind aliases, and pin them with
a test that reads `src/globals.css`. Nothing visible changes yet.

**Architecture:** New custom properties in the existing `:root` and `.dark` blocks of
`src/globals.css`, next to the `--editor-*` group. Four unlayered classes beside the existing
`.sidebar-elevation` rule (that one stays until plan 02). A Vitest file parses the CSS text, so the
rules about ordering, shadows and contrast cannot drift.

**Tech Stack:** CSS custom properties, Tailwind (`tailwind.config.js`), Vitest.

**Spec:** `docs/superpowers/specs/2026-10-31-elevation-design.md`, sections "Tokens", "Contrast" and
"Rules".

## Global Constraints

- Token names and values exactly as in the spec table. Surface tokens are raw HSL triples
  (`210 5% 12.5%`), shadow tokens are full `box-shadow` values.
- The global `--muted-foreground` (48%) is not changed. Only the scoped override below.
- No `color-mix()`, no `backdrop-filter` in anything added.
- Biome only formats `src/`. `globals.css` is hand-formatted: two-space indent inside blocks, a
  trailing comment on each token naming its role.
- Verification per task: `bun run tsc --noEmit`, `bun run check`, the task's `bun run test <path>`.

## Review Focus

- Light surfaces unchanged: the test pins each light surface token to the value it aliases today.
- Secondary text below 4.5:1 on raised or overlay: pinned by the contrast tests in Task 1.
- Blur or `color-mix` creeping into the new rules: pinned by `forbids color-mix and backdrop-filter`.

---

### Task 1: Tokens, classes and the contract test

**Files:**
- Create: `src/lib/__tests__/elevation-tokens.test.ts`
- Modify: `src/globals.css` (`:root` block after the `--editor-*` group near line 164; `.dark` block
  after the `--editor-*` group near line 349; new classes after `.dark .sidebar-elevation` near line 541)

**Interfaces:**
- Consumes: nothing.
- Produces: the tokens and classes listed in the index under "Names shared across plans", and the
  test helpers `cssBlock`, `hslTriple`, `contrast` that plans 02 and 04 import by copying the same
  file (they add cases to this file, they do not import across files).

- [ ] **Step 1: Write the failing test file**

`src/lib/__tests__/elevation-tokens.test.ts` reads `src/globals.css` with `readFileSync(new URL('../../globals.css', import.meta.url), 'utf8')`. Helpers: `cssBlock(css, selector)` returns the body of the first rule whose selector is exactly `selector` (match `^\s*<selector>\s*\{`, take text to the first `}`); `hslTriple(block, name)` parses `--name: H S% L%` into numbers; `contrast(a, b)` converts HSL to sRGB and applies the WCAG ratio. Cases, each with the spec's exact values:

```ts
describe('elevation tokens', () => {
  it('defines all four surfaces in light and dark', () => { /* 4 names x 2 blocks are present */ });
  it('makes each dark level lighter than the one below by at least 2 points', () => {
    // L(sunken) < L(surface) < L(raised) < L(overlay), each gap >= 2
    // and the values are 7.5, 10, 12.5, 15.
  });
  it('keeps light surfaces on today\'s values', () => {
    // sunken 0 0% 100%, surface 220 21.4% 94.5%, raised 240 50% 99%, overlay 0 0% 100%.
  });
  it('defines raised and overlay shadows in both themes, darker in dark', () => {
    // all four --elevation-shadow-* exist; max rgba alpha in dark > max alpha in light for each.
  });
  it('keeps body text at 4.5:1 or better on every dark level', () => {
    // contrast(--foreground, each dark surface) >= 4.5
  });
  it('keeps secondary text at 4.5:1 on raised and overlay in dark', () => {
    // read --muted-foreground from the block `.dark .elevation-raised, .dark .elevation-overlay`
    // expect 0 0% 56% and contrast >= 4.5 against raised and overlay.
  });
  it('gives each class its surface, and raised and overlay their shadow', () => {
    // `.elevation-raised` contains background-color: hsl(var(--elevation-raised-bg)) and
    // box-shadow: var(--elevation-shadow-raised); same for overlay; sunken and surface have no box-shadow.
  });
  it('forbids color-mix and backdrop-filter in elevation rules', () => {
    // the text of every block that mentions "elevation" has neither string.
  });
});
```

- [ ] **Step 2: Run the test and confirm it fails**

Run: `bun run test src/lib/__tests__/elevation-tokens.test.ts`
Expected: FAIL on missing tokens, not on a helper error. If a helper throws (for example the `.dark`
block cannot be found), fix the helper first against the real file.

- [ ] **Step 3: Add the tokens to `src/globals.css`**

In `:root` and in `.dark`, after the `--editor-*` group, add the four `--elevation-*-bg` triples and
the two `--elevation-shadow-*` values from the spec table, one comment per line naming the role
(for example `/* Elevation: raised, sidebar and cards */`).

- [ ] **Step 4: Add the classes**

After the `.dark .sidebar-elevation` rule add `.elevation-sunken`, `.elevation-surface`,
`.elevation-raised` and `.elevation-overlay` as plain unlayered rules (surface via
`background-color: hsl(var(--elevation-<level>-bg))`, shadow via `box-shadow: var(--elevation-shadow-<level>)`
for raised and overlay), then one rule `.dark .elevation-raised, .dark .elevation-overlay { --muted-foreground: 0 0% 56%; }`.
A short comment above the group says: surface and shadow travel together, do not add a `bg-*` utility
to the same element, secondary text is lifted because 48% is under 4.5:1 on these levels.

- [ ] **Step 5: Run the test and confirm it passes**

Run: `bun run test src/lib/__tests__/elevation-tokens.test.ts`
Expected: PASS, 8 tests.

- [ ] **Step 6: Run the checks and commit**

Run: `bun run tsc --noEmit && bun run check`. Expected: no errors.
Commit with the `dev-workflow-skills:1-git-commit` skill, staging
`src/globals.css src/lib/__tests__/elevation-tokens.test.ts`. Subject: `feat(theme): add elevation tokens`.

---

### Task 2: Tailwind aliases

**Files:**
- Modify: `tailwind.config.js` (the `colors` group in `theme.extend`, next to `editor`, near line 119)
- Modify: `src/lib/__tests__/elevation-tokens.test.ts`

**Interfaces:**
- Consumes: the four `--elevation-*-bg` tokens from Task 1.
- Produces: `bg-elevation-sunken`, `bg-elevation-surface`, `bg-elevation-raised`,
  `bg-elevation-overlay` (also usable as `text-`, `border-` colours). Plan 04 uses
  `bg-elevation-sunken`.

- [ ] **Step 1: Add the failing test**

```ts
it('exposes the four surfaces as Tailwind colours', async () => {
  const config = (await import('../../../tailwind.config.js')).default;
  const elevation = config.theme.extend.colors.elevation;
  expect(elevation).toEqual({
    sunken: 'hsl(var(--elevation-sunken-bg))',
    surface: 'hsl(var(--elevation-surface-bg))',
    raised: 'hsl(var(--elevation-raised-bg))',
    overlay: 'hsl(var(--elevation-overlay-bg))',
  });
});
```

If `config.theme.extend.colors` is not the real path, adjust the path in the test to where `editor`
is defined, and keep the same `toEqual` assertion.

- [ ] **Step 2: Run it and confirm it fails**

Run: `bun run test src/lib/__tests__/elevation-tokens.test.ts -t "Tailwind"`. Expected: FAIL, `elevation` is undefined.

- [ ] **Step 3: Add `elevation: { sunken, surface, raised, overlay }` to the config**

Same format as the neighbouring `editor` group, one `hsl(var(--elevation-<level>-bg))` string each.

- [ ] **Step 4: Run the file and the checks**

Run: `bun run test src/lib/__tests__/elevation-tokens.test.ts && bun run tsc --noEmit && bun run check`.
Expected: all pass. If `tsc` rejects the `.js` import in the test, add `// @ts-expect-error` with a
one-line reason, since the config is plain JavaScript.

- [ ] **Step 5: Commit**

Use the commit skill, staging `tailwind.config.js src/lib/__tests__/elevation-tokens.test.ts`.
Subject: `feat(theme): expose elevation colours to Tailwind`.
