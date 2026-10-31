# Elevation — Plan 3: Overlay

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 3 of 4.** Previous plan: `docs/superpowers/plans/elevation/2026-10-31-elevation-plan-02-raised-sidebar.md` (must be merged and green).
**Next plan to run: `2026-10-31-elevation-plan-04-sunken-default-cards-docs.md`.**
**Recommended model: Sonnet.**

**Goal:** Every floating surface shares one overlay surface and one overlay shadow: menus, popovers,
selects, dialogs, the sheet, toasts and the editor hover tip. Tooltips stay inverse.

**Architecture:** Each primitive swaps its own surface and shadow classes for `elevation-overlay`.
`--dropdown-bg` becomes an alias of the overlay token in dark mode, so the toast (which reads
`--dropdown-bg` through Sonner variables) follows without class changes. Contract tests read the
primitive source files, because Radix content portals make rendering every primitive in jsdom
brittle. One rendered test covers the dropdown as a smoke check.

**Tech Stack:** shadcn/ui, Radix, Sonner, CodeMirror 6 (hover tip), Vitest.

**Spec:** `docs/superpowers/specs/2026-10-31-elevation-design.md`, sections "Rules", "Tokens",
"Exemptions and non-goals". Prerequisites from plan 01: `.elevation-overlay`,
`--elevation-overlay-bg`, `--elevation-shadow-overlay`.

## Global Constraints

- Keep each primitive's border, radius, padding and z-index. Remove only its background, blur and
  shadow classes: `bg-dropdown-bg/50`, `backdrop-blur-sm`, `shadow-[0_2px_8px_...]`,
  `dark:shadow-[...]`, `bg-card`, `bg-background`, `shadow-lg`, `shadow-md`.
- Do not touch the dialog and sheet scrims, and do not touch the tooltip surface.
- Do not delete the opaque fallback rules in `globals.css` (lines 450-470); they are harmless now.
- shadcn/ui files are edited in place, only the class strings named below.
- Verification per task: `bun run tsc --noEmit`, `bun run check`, the task's `bun run test <path>`.

## Review Focus

- A dropdown opened inside a dialog: both are overlay level, so the border must keep them apart.
  Manual check in plan 04.
- A select inside a Settings form (secret manager and credential pickers) is readable on the
  Settings page. Manual check in plan 04.
- Light theme: menus change from translucent to opaque white with a cool shadow. Check text contrast.

---

### Task 1: Menu-like primitives

**Files:**
- Modify: `src/components/ui/popover.tsx:22`, `src/components/ui/dropdown-menu.tsx:45,64`, `src/components/ui/context-menu.tsx:45,62`, `src/components/ui/select.tsx:73`
- Modify: `src/globals.css` (dark `--dropdown-bg` at line 298)
- Create: `src/components/ui/__tests__/elevation-overlay.test.tsx`

**Interfaces:**
- Consumes: `.elevation-overlay`, `--elevation-overlay-bg` (plan 01).
- Produces: a shared list `OVERLAY_FILES` inside the new test file that Task 2 extends.

- [ ] **Step 1: Write the failing tests**

```tsx
const read = (name: string) => readFileSync(new URL(`../${name}.tsx`, import.meta.url), 'utf8');
const OVERLAY_FILES = ['popover', 'dropdown-menu', 'context-menu', 'select'];

describe.each(OVERLAY_FILES)('%s', (name) => {
  it('uses the overlay level and no surface or shadow of its own', () => {
    const src = read(name);
    expect(src).toContain('elevation-overlay');
    expect(src).not.toMatch(/bg-dropdown-bg\/50|backdrop-blur|shadow-\[|shadow-(md|lg)|bg-card|bg-background/);
  });
});

it('renders dropdown content on the overlay level', async () => {
  render(
    <DropdownMenu defaultOpen>
      <DropdownMenuTrigger>Open</DropdownMenuTrigger>
      <DropdownMenuContent><DropdownMenuItem>Item</DropdownMenuItem></DropdownMenuContent>
    </DropdownMenu>,
  );
  expect(await screen.findByRole('menu')).toHaveClass('elevation-overlay');
});

it('aliases --dropdown-bg to the overlay token in dark', () => {
  const css = readFileSync(new URL('../../../globals.css', import.meta.url), 'utf8');
  expect(css).toMatch(/--dropdown-bg:\s*var\(--elevation-overlay-bg\)/);
});
```

- [ ] **Step 2: Run them and confirm they fail**

Run: `bun run test src/components/ui/__tests__/elevation-overlay.test.tsx`. Expected: FAIL for all
four files, the dropdown render and the alias.

- [ ] **Step 3: Replace the classes**

In each file, in the content and sub-content class strings, replace
`bg-dropdown-bg/50 backdrop-blur-sm ... shadow-[0_2px_8px_rgba(0,0,0,0.16)] dark:shadow-[0_2px_8px_rgba(0,0,0,0.4)]`
with `elevation-overlay`, keeping `text-dropdown-fg`, `border`, `border-dropdown-border`, `rounded-sm`,
`z-50`, padding and the animation classes. In `src/globals.css` set the dark `--dropdown-bg` to
`var(--elevation-overlay-bg)` and keep its trailing comment style.

- [ ] **Step 4: Run the tests, the UI tests and the checks**

Run: `bun run test src/components/ui && bun run tsc --noEmit && bun run check`. Expected: all pass.

- [ ] **Step 5: Commit**

Use the commit skill, staging the four primitive files, `src/globals.css` and the new test.
Subject: `feat(theme): put menus on the overlay level`.

---

### Task 2: Modal primitives

**Files:**
- Modify: `src/components/ui/dialog.tsx:38`, `src/components/ui/alert-dialog.tsx:36`, `src/components/ui/sheet.tsx:37`
- Test: `src/components/ui/__tests__/elevation-overlay.test.tsx`

**Interfaces:**
- Consumes: `OVERLAY_FILES` from Task 1.
- Produces: nothing new.

- [ ] **Step 1: Extend the test**

Add `'dialog'`, `'alert-dialog'`, `'sheet'` to `OVERLAY_FILES`. Add a rendered case:

```tsx
it('renders a dialog on the overlay level and leaves its scrim alone', async () => {
  render(
    <Dialog defaultOpen>
      <DialogContent><DialogTitle>Title</DialogTitle><DialogDescription>Body</DialogDescription></DialogContent>
    </Dialog>,
  );
  expect(await screen.findByRole('dialog')).toHaveClass('elevation-overlay');
  expect(read('dialog')).toContain('bg-black/80');
});
```

The scrim assertion is only for the dialog; the sheet scrim keeps `bg-background/20`, and the sheet
test file case does not forbid `bg-background/20` (adjust the shared regex to allow `bg-background/20`
and `bg-black/80`, and still forbid the bare `bg-background` and `bg-card`).

- [ ] **Step 2: Run it and confirm it fails**

Run: `bun run test src/components/ui/__tests__/elevation-overlay.test.tsx`. Expected: FAIL for the
three new files and the dialog render.

- [ ] **Step 3: Replace the classes**

In the dialog, alert-dialog and sheet content class strings replace the background and `shadow-lg`
with `elevation-overlay`. Keep the dialog `border`, `sm:rounded-lg`, the sheet `border-l border-border`
and every animation and positioning class. Leave the `DialogOverlay`, `AlertDialogOverlay` and
`SheetOverlay` class strings untouched.

- [ ] **Step 4: Run the tests and the checks**

Run: `bun run test src/components/ui src/components/settings && bun run tsc --noEmit && bun run check`.
Expected: all pass. The Settings tests render dialogs, so they catch a broken class string.

- [ ] **Step 5: Commit**

Use the commit skill, staging the three primitive files and the test. Subject:
`feat(theme): put dialogs and sheets on the overlay level`.

---

### Task 3: Toast, editor hover tip and the tooltip note

**Files:**
- Modify: `src/components/ui/sonner.tsx:33-50` (toast style)
- Modify: `src/components/editor/extensions/variable-hover.ts:34` (hover tip classes)
- Modify: `src/components/ui/tooltip.tsx:20` (comment only)
- Test: `src/components/ui/__tests__/elevation-overlay.test.tsx`

**Interfaces:**
- Consumes: `--elevation-shadow-overlay`, `.elevation-overlay`.
- Produces: nothing new.

- [ ] **Step 1: Extend the test**

```tsx
it('gives the toast the overlay shadow', () => {
  expect(read('sonner')).toContain('var(--elevation-shadow-overlay)');
});
it('puts the editor hover tip on the overlay level', () => {
  const src = readFileSync(new URL('../../editor/extensions/variable-hover.ts', import.meta.url), 'utf8');
  expect(src).toContain('elevation-overlay');
  expect(src).not.toMatch(/shadow-md|bg-card/);
});
it('keeps the tooltip inverse and says why', () => {
  const src = read('tooltip');
  expect(src).toContain('bg-primary');
  expect(src).toMatch(/inverse/i);
});
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `bun run test src/components/ui/__tests__/elevation-overlay.test.tsx`. Expected: 3 FAIL.

- [ ] **Step 3: Make the changes**

Sonner: add `boxShadow: 'var(--elevation-shadow-overlay)'` to the toast `style` (the surface already
comes from `--dropdown-bg`). Hover tip: replace `bg-card` and `shadow-md` with `elevation-overlay`,
keep `border-border`, `rounded-sm` and `text-popover-foreground`. Tooltip: add one comment above the
class string: `// Inverse surface on purpose, so it is outside the elevation levels.`

- [ ] **Step 4: Run the tests and the checks**

Run: `bun run test src/components/ui src/components/editor && bun run tsc --noEmit && bun run check`.
Expected: all pass.

- [ ] **Step 5: Commit**

Use the commit skill, staging the three source files and the test. Subject:
`feat(theme): put toasts and hover tips on the overlay level`.
