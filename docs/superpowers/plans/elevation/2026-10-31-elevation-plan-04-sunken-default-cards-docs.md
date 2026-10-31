# Elevation — Plan 4: Sunken, default, cards and docs

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 4 of 4.** Previous plan: `docs/superpowers/plans/elevation/2026-10-31-elevation-plan-03-overlay.md` (must be merged and green).
**Next plan to run: none. After this plan, run the final whole-branch review.**
**Recommended model: Sonnet.**

**Goal:** Sunken surfaces use the sunken token, the default surfaces have one source, cards sit on the
raised token, and the whole system is documented and checked by eye in both themes.

**Architecture:** Class swaps to `bg-elevation-sunken` where a surface should be recessed and is not
(the Settings page, its sticky Save bar, the console output well). In the dark block, `--background`,
`--panel-bg`, `--titlebar-bg` and `--statusbar-bg` become aliases of `--elevation-surface-bg`. The dark
card rule reads `--elevation-raised-bg` instead of `--popover`. A feature doc records the rules.

**Tech Stack:** Tailwind, CSS custom properties, Vitest, Markdown.

**Spec:** `docs/superpowers/specs/2026-10-31-elevation-design.md`, sections "Rules", "Tokens",
"Findings that shape the plan". Prerequisites from plans 01 to 03: the tokens, `bg-elevation-*`
aliases, `src/lib/__tests__/elevation-tokens.test.ts`.

## Global Constraints

- Light theme surfaces do not change in this plan. The default aliases are dark only, because the
  light title bar already shares the sidebar value (`240 50% 99%`), not the light `--background`.
- Do not change `Input`, `Textarea` or the select trigger (spec: non-goals).
- Raw `bg-card` on plain divs (`RequestPanel`, editor groups) stays as it is. It already resolves to
  the sunken value in dark, and `globals.css` says not to "fix" it.
- Nested cards keep `--accent`.
- Docs go in `docs/features/`, with a one-line pointer in the project `CLAUDE.md`.
- Verification per task: `bun run tsc --noEmit`, `bun run check`, the task's `bun run test <path>`.

## Review Focus

- `hsl(var(--background) / 0.7)` (translucent headers, `src/globals.css:474`) still renders when
  `--background` is an alias. Manual check in Task 3.
- A menu opened inside a dialog opened from Settings stays readable. Manual check in Task 3.
- The console output well is visible in dark mode (it was not). Pinned in Task 1, checked by eye in Task 3.

---

### Task 1: Sunken surfaces

**Files:**
- Modify: `src/components/settings/SettingsPage.tsx:59` (root `bg-editor-bg` to `bg-elevation-sunken`)
- Modify: `src/components/settings/sections/AiAgentsSection.tsx:311` (sticky bar `bg-editor-bg` to `bg-elevation-sunken`)
- Modify: `src/components/layout/ConsolePanel.tsx:134` (output well `bg-background/60` to `bg-elevation-sunken`)
- Create: `src/lib/__tests__/elevation-adoption.test.ts`

**Interfaces:**
- Consumes: `bg-elevation-sunken` (plan 01).
- Produces: the adoption test file that Tasks 2 and 3 extend.

- [ ] **Step 1: Write the failing test**

```ts
const src = (path: string) => readFileSync(new URL(`../../${path}`, import.meta.url), 'utf8');

describe('sunken surfaces', () => {
  it.each([
    ['components/settings/SettingsPage.tsx'],
    ['components/settings/sections/AiAgentsSection.tsx'],
    ['components/layout/ConsolePanel.tsx'],
  ])('%s uses the sunken token', (path) => {
    expect(src(path)).toContain('bg-elevation-sunken');
  });
  it('console output well is no longer a translucent background', () => {
    expect(src('components/layout/ConsolePanel.tsx')).not.toContain('bg-background/60');
  });
  it('settings no longer names the editor token directly', () => {
    expect(src('components/settings/SettingsPage.tsx')).not.toContain('bg-editor-bg');
    expect(src('components/settings/sections/AiAgentsSection.tsx')).not.toContain('bg-editor-bg');
  });
});
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `bun run test src/lib/__tests__/elevation-adoption.test.ts`. Expected: FAIL on all five cases.

- [ ] **Step 3: Swap the classes**

One class swap per file as listed. Leave every other class on those elements untouched. In
`ConsolePanel.tsx` the `<pre>` keeps its padding, radius and text classes.

- [ ] **Step 4: Run the tests and the checks**

Run: `bun run test src/lib/__tests__/elevation-adoption.test.ts src/components/settings src/components/layout && bun run tsc --noEmit && bun run check`.
Expected: all pass.

- [ ] **Step 5: Commit**

Use the commit skill, staging the three components and the new test. Subject:
`feat(theme): use the sunken level for settings and the console well`.

---

### Task 2: One source for the default surface

**Files:**
- Modify: `src/globals.css` (`.dark` block: `--background` line 207, `--statusbar-bg` line 267, `--panel-bg` line 328, `--titlebar-bg` line 337)
- Test: `src/lib/__tests__/elevation-tokens.test.ts`

**Interfaces:**
- Consumes: `--elevation-surface-bg` (plan 01).
- Produces: nothing new.

- [ ] **Step 1: Add the failing test**

```ts
it('aliases the dark shell tokens to the default surface', () => {
  const dark = cssBlock(css, '.dark');
  for (const name of ['--background', '--statusbar-bg', '--panel-bg', '--titlebar-bg']) {
    expect(dark).toMatch(new RegExp(`${name}:\\s*var\\(--elevation-surface-bg\\)`));
  }
});
it('keeps the same visible default surface value', () => {
  expect(hslTriple(cssBlock(css, '.dark'), '--elevation-surface-bg')).toEqual([210, 5, 10]);
});
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `bun run test src/lib/__tests__/elevation-tokens.test.ts -t "default surface"`. Expected: the alias case FAILS; the value case passes.

- [ ] **Step 3: Replace the four dark values with `var(--elevation-surface-bg)`**

Keep each trailing comment and add `, default level` to it. Do not change the light block.

- [ ] **Step 4: Run the tests and the checks**

Run: `bun run test src/lib/__tests__ && bun run tsc --noEmit && bun run check`. Expected: all pass.

- [ ] **Step 5: Commit**

Use the commit skill, staging `src/globals.css src/lib/__tests__/elevation-tokens.test.ts`. Subject:
`refactor(theme): source the dark shell colour from one token`.

---

### Task 3: Cards, documentation and manual checks

**Files:**
- Modify: `src/globals.css` (`.dark [data-slot="card"]` at line 586; rewrite the comment block at lines 566-580)
- Create: `docs/features/elevation.md`
- Modify: `CLAUDE.md` (one pointer line after the settings-page line)
- Modify: `docs/features/settings-page.md` (one sentence: Settings is the sunken level, the sidebar is raised)
- Test: `src/lib/__tests__/elevation-tokens.test.ts`

**Interfaces:**
- Consumes: `--elevation-raised-bg` (plan 01).
- Produces: `docs/features/elevation.md`, linked from `CLAUDE.md`.

- [ ] **Step 1: Add the failing test**

```ts
it('puts dark cards on the raised token and nested cards on --accent', () => {
  expect(css).toMatch(/\.dark \[data-slot="card"\]\s*\{[^}]*background-color:\s*hsl\(var\(--elevation-raised-bg\)\)/);
  expect(css).toMatch(/\.dark \[data-slot="card"\] \[data-slot="card"\]\s*\{[^}]*hsl\(var\(--accent\)\)/);
});
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `bun run test src/lib/__tests__/elevation-tokens.test.ts -t "dark cards"`. Expected: FAIL (the rule still uses `--popover`).

- [ ] **Step 3: Move the cards and rewrite the comment**

In `.dark [data-slot="card"]` replace `hsl(var(--popover))` with `hsl(var(--elevation-raised-bg))`; keep the
ring `box-shadow` and the border. Rewrite the comment block above the card rules so it names the
four levels, says cards are raised, raw `bg-card` on plain divs is sunken in dark, and nested cards
step to `--accent`.

- [ ] **Step 4: Write `docs/features/elevation.md`**

Sections: the four levels with the token table (copy from the spec), the rules, which surfaces use
which level (sidebar and cards raised; menus, popovers, dialogs, sheet, toast, hover tip overlay;
Settings, editor and console well sunken; title bar, status bar and console panel default), the
exemptions (tooltip, scrims, inputs), the contrast numbers, the WebKitGTK note, and how to add a new
surface ("pick the level, use its `.elevation-*` class or `bg-elevation-*`, never mix levels").
Add the `CLAUDE.md` pointer: `See docs/features/elevation.md for the four elevation levels (tokens, classes, which surface uses which, contrast).`
Add the one sentence to `docs/features/settings-page.md`.

- [ ] **Step 5: Run the full frontend checks**

Run: `bun run test src/lib src/components/ui src/components/settings src/components/layout && bun run tsc --noEmit && bun run check`.
Expected: all pass.

- [ ] **Step 6: Check it by eye in both themes**

Run `bun run tauri dev`. For each theme, confirm and write down the result:
1. Settings open: sidebar raised and shadowed, Settings sunken, nav readable.
2. A dropdown, a select and a popover open over Settings: opaque overlay with a shadow.
3. A dropdown opened inside a dialog opened from Settings: both readable, border separates them.
4. The console panel with output: the output well is visibly darker than the panel in dark mode.
5. A collection page with cards: cards are one step above the shell; nested cards one more.
6. A toast: overlay surface and shadow.
7. The translucent header that uses `hsl(var(--background) / 0.7)` still blends (dark mode).
8. Secondary text (placeholder, descriptions) on the sidebar and in menus is readable.
Do not mark the task done on tests alone. List anything that looks wrong.

- [ ] **Step 7: Commit**

Use the commit skill, staging `src/globals.css src/lib/__tests__/elevation-tokens.test.ts docs/features/elevation.md CLAUDE.md docs/features/settings-page.md`.
Subject: `docs(theme): document the elevation levels`.
