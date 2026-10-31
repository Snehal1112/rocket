# Elevation — Plan 2: Raised sidebar

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 2 of 4.** Previous plan: `docs/superpowers/plans/elevation/2026-10-31-elevation-plan-01-tokens.md` (must be merged and green).
**Next plan to run: `2026-10-31-elevation-plan-03-overlay.md`.**
**Recommended model: Sonnet.**

**Goal:** The Collections sidebar is a raised surface in the dark theme: lighter than the editor and
the shell, with a shadow that stays visible over the editor and over the Settings page.

**Architecture:** The sidebar wrapper in `src/App.tsx` swaps `sidebar-elevation` for
`elevation-raised`. `--sidebar-bg` becomes an alias of the raised token, so the sidebar root keeps its
`bg-sidebar-bg` class. The old `.sidebar-elevation` rules, including the dark `box-shadow: none`, are
deleted. The Settings overlay in `MainArea` drops from `z-20` to `z-[1]`, below the sidebar's `z-[5]`,
so the shadow is not painted over.

**Tech Stack:** React 19, Tailwind, CSS custom properties, Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-10-31-elevation-design.md`, sections "Tokens" and "Findings
that shape the plan". Prerequisites from plan 01: the tokens, `.elevation-raised`, the test file
`src/lib/__tests__/elevation-tokens.test.ts` with `cssBlock`.

## Global Constraints

- Only the sidebar wrapper uses `elevation-raised` in this plan. Cards move in plan 04.
- The editor's `isolate` wrapper in `MainArea` stays, and Settings stays above it.
- The resize handle (`z-10`) keeps its z-index and its 4px hit area. Do not raise the sidebar above 10.
- Comments are short full sentences ending with a period.
- Verification per task: `bun run tsc --noEmit`, `bun run check`, the task's `bun run test <path>`.

## Review Focus

- Settings open: the shadow must still show on its left edge. Manual check in Task 2 step 5.
- Sidebar resized to the 200px minimum and 500px maximum: the shadow follows the wrapper edge.
- Light theme: the sidebar looks as before (same raised value, same shadow).

---

### Task 1: Sidebar uses the raised level

**Files:**
- Modify: `src/App.tsx:204` (wrapper class)
- Modify: `src/globals.css` (`--sidebar-bg` at lines 149 and 334; delete the `.sidebar-elevation` comment and both rules at lines 528-541)
- Test: `src/lib/__tests__/elevation-tokens.test.ts`

**Interfaces:**
- Consumes: `.elevation-raised`, `--elevation-raised-bg` (plan 01).
- Produces: nothing new. `--sidebar-bg` now equals `--elevation-raised-bg` in both themes.

- [ ] **Step 1: Add the failing tests**

```ts
describe('raised sidebar', () => {
  it('aliases --sidebar-bg to the raised token in both themes', () => {
    // cssBlock(css, ':root') and cssBlock(css, '.dark') each contain
    // `--sidebar-bg: var(--elevation-raised-bg)`.
  });
  it('no longer switches the sidebar shadow off in dark', () => {
    expect(css).not.toContain('sidebar-elevation');
  });
  it('puts elevation-raised on the sidebar wrapper and keeps its stacking', () => {
    const app = readFileSync(new URL('../../App.tsx', import.meta.url), 'utf8');
    expect(app).toContain('elevation-raised w-(--sidebar-w) shrink-0 relative z-[5]');
    expect(app).not.toContain('sidebar-elevation');
  });
});
```

- [ ] **Step 2: Run them and confirm they fail**

Run: `bun run test src/lib/__tests__/elevation-tokens.test.ts -t "raised sidebar"`. Expected: 3 FAIL.

- [ ] **Step 3: Make the change**

In `src/App.tsx` replace `sidebar-elevation` with `elevation-raised` in the wrapper class string. In
`src/globals.css` set `--sidebar-bg: var(--elevation-raised-bg);` in `:root` and `.dark`, keeping the
trailing comment style (`/* sideBar.background, raised level */`). Delete the old comment block and the
two `.sidebar-elevation` rules. Leave `--sidebar-border` and the border on the resize handle alone.

- [ ] **Step 4: Run the test file and the checks**

Run: `bun run test src/lib/__tests__/elevation-tokens.test.ts && bun run tsc --noEmit && bun run check`.
Expected: all pass.

- [ ] **Step 5: Commit**

Use the commit skill, staging `src/App.tsx src/globals.css src/lib/__tests__/elevation-tokens.test.ts`.
Subject: `feat(theme): raise the collections sidebar`.

---

### Task 2: Settings no longer paints over the sidebar shadow

**Files:**
- Modify: `src/components/layout/MainArea.tsx:18` (overlay wrapper class)
- Create: `src/components/layout/__tests__/main-area-stacking.test.tsx`

**Interfaces:**
- Consumes: `useSettingsStore` (`open`), `MainArea({ children })`.
- Produces: nothing new.

- [ ] **Step 1: Write the failing test**

```tsx
vi.mock('@/components/settings/SettingsPage', () => ({
  SettingsPage: () => <section aria-label='Settings' />,
}));

describe('MainArea stacking', () => {
  beforeEach(() => useSettingsStore.setState({ open: true }));
  afterEach(() => useSettingsStore.getState().reset());

  it('keeps the Settings overlay below the sidebar so its shadow stays visible', () => {
    render(<MainArea><p>tabs</p></MainArea>);
    const overlay = screen.getByRole('region', { name: 'Settings' }).parentElement as HTMLElement;
    expect(overlay.className).toContain('z-[1]');
    expect(overlay.className).not.toContain('z-20');
  });

  it('keeps the tabs inert and isolated while Settings is open', () => {
    const { container } = render(<MainArea><p>tabs</p></MainArea>);
    const tabs = container.querySelector('.isolate') as HTMLElement;
    expect(tabs.hasAttribute('inert')).toBe(true);
  });

  it('renders no overlay when Settings is closed', () => {
    useSettingsStore.getState().closeSettings();
    render(<MainArea><p>tabs</p></MainArea>);
    expect(screen.queryByRole('region', { name: 'Settings' })).toBeNull();
  });
});
```

Imports: `render`, `screen` from Testing Library; `describe`, `it`, `expect`, `vi`, `beforeEach`,
`afterEach` from Vitest; `MainArea` from `@/components/layout/MainArea`; `useSettingsStore` from
`@/stores/settings-store`.

- [ ] **Step 2: Run it and confirm the first case fails**

Run: `bun run test src/components/layout/__tests__/main-area-stacking.test.tsx`.
Expected: the stacking case FAILS on `z-[1]`; the other two pass.

- [ ] **Step 3: Change `z-20` to `z-[1]` on the overlay wrapper in `MainArea.tsx`**

Add one comment above it: `{/* Below the sidebar (z-5) so the sidebar shadow shows over Settings. */}`.
The wrapper makes its own stacking context, so the sticky Save bar's `z-10` inside Settings stays
contained.

- [ ] **Step 4: Run the test, the Settings tests and the checks**

Run: `bun run test src/components/layout/__tests__/main-area-stacking.test.tsx src/components/settings && bun run tsc --noEmit && bun run check`.
Expected: all pass.

- [ ] **Step 5: Look at it in the app**

Run `bun run tauri dev`. In dark mode with Settings open: the sidebar is a step lighter than the
Settings page, and a soft shadow falls on the Settings nav's left edge. Resize the sidebar to both
limits. Switch to light mode and confirm the sidebar looks as before. Report anything that differs;
do not mark the task done on test results alone.

- [ ] **Step 6: Commit**

Use the commit skill, staging `src/components/layout/MainArea.tsx src/components/layout/__tests__/main-area-stacking.test.tsx`.
Subject: `fix(settings): keep the sidebar shadow above settings`.
