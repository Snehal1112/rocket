# Raw Control Cleanup — Checkbox and Tab Close Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the raw `<input type='checkbox'>` in the stash multi-select row with the shadcn `Checkbox` primitive, and replace the raw `<button>` tab-close control with shadcn `Button`, per `CLAUDE.md:87`'s hard rule against raw form/interactive tags.

**Architecture:** `GitStashSection.tsx`'s stash-row checkbox is a plain `<input type='checkbox'>` — the one clear "raw form control" instance in the Git panel (the other F-11 refs are row/button elements, handled in `docs/superpowers/plans/2026-09-20-git-raw-controls-list-rows.md`). `src/components/ui/checkbox.tsx` already wraps `@radix-ui/react-checkbox` with the project's styling, exposing a Radix-standard `checked`/`onCheckedChange` API (`onCheckedChange` receives `boolean | 'indeterminate'`, so the handler must check `=== true` rather than treating it as a plain boolean). `TabItem.tsx`'s close button is a raw `<button>` — this one is shared by every tab type (request, collection, workspace's non-git tabs, runner, contract, and Git tabs), not Git-specific, so this task is deliberately scoped to a minimal, purely visual-preserving swap to avoid regressing tab-bar UI elsewhere in the app.

**Tech Stack:** React 18, TypeScript, `@radix-ui/react-checkbox` (via `src/components/ui/checkbox.tsx`), shadcn `Button`, Vitest, React Testing Library.

**Spec:** `docs/reports/git-integration-review/02-frontend-architecture.md` (F-11 "Raw interactive controls violate the shadcn-only hard rule", `CLAUDE.md:87`). Verified present in current `src/components/git/GitStashSection.tsx:195-201` and `src/components/panes/TabItem.tsx:107-117`.

**Next Plan:** After this plan is fully implemented and verified, execute `docs/superpowers/plans/2026-09-20-git-error-live-regions.md` next (plan 15 of 22 in the git-frontend-architecture remediation sequence).

## Global Constraints

- All UI components use shadcn/ui primitives only.
- Icons: `lucide-react` only.
- Commits use conventional commits format.
- `yarn tsc --noEmit`, `yarn check`, and `yarn test <pattern>` must pass before each commit.

---

### Task 1: `GitStashSection` stash-row checkbox

**Files:**
- Modify: `src/components/git/GitStashSection.tsx`
- Test: `src/components/git/__tests__/GitStashSection.test.tsx`

**Interfaces:**
- Consumes: `Checkbox` from `@/components/ui/checkbox` (`checked: boolean | 'indeterminate'`, `onCheckedChange: (checked: boolean | 'indeterminate') => void`, `disabled?: boolean`).
- Produces: no new exports, no behavior change to `toggleSelect`.

- [ ] **Step 1: Write a test that exercises the checkbox by accessible role**

`src/components/git/__tests__/GitStashSection.test.tsx` was created by `docs/superpowers/plans/2026-09-20-git-store-selector-narrowing.md` — extend it (if that plan hasn't been applied yet, create the file fresh using the same store/provider setup shown there). Add:

```tsx
import userEvent from '@testing-library/user-event';
```

```tsx
describe('GitStashSection stash selection', () => {
  it('selects a stash via an accessible checkbox', async () => {
    const store = createGitStore();
    store.setState({
      stashes: [
        {
          index: 0,
          message: 'wip',
          timestamp: new Date().toISOString(),
          filesChanged: 1,
          insertions: 1,
          deletions: 0,
          changedFiles: ['a.txt'],
          branch: 'main',
        },
      ],
    });
    render(
      <GitStoreProvider store={store}>
        <GitStashSection />
      </GitStoreProvider>,
    );
    const user = userEvent.setup();

    // The checkbox is hidden until hover/selection; hover the row first.
    await user.hover(screen.getByText('wip'));
    const checkbox = await screen.findByRole('checkbox');
    await user.click(checkbox);

    expect(checkbox).toBeChecked();
    expect(await screen.findByText('1 selected')).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/GitStashSection.test.tsx -t "selects a stash via an accessible checkbox"`
Expected: This may already pass today since a raw `<input type='checkbox'>` also exposes `role='checkbox'` — run it first to confirm it passes both before and after as a baseline regression guard, then proceed with the swap. (Unlike a behavioral bug fix, this task's "red" signal is the grep in Step 3, not this test — see the plan's Architecture note on why a pure markup swap has no jsdom-observable red state.)

- [ ] **Step 3: Confirm the current violation**

Run: `grep -n "type='checkbox'" src/components/git/GitStashSection.tsx`
Expected: one match.

- [ ] **Step 4: Convert to shadcn `Checkbox`**

Add the import:

```tsx
import { Checkbox } from '@/components/ui/checkbox';
```

Replace:

```tsx
                {showCheckbox ? (
                  <input
                    type='checkbox'
                    className='h-3.5 w-3.5 accent-primary cursor-pointer'
                    checked={isSelected}
                    disabled={isBatchRunning}
                    onChange={(e) => toggleSelect(stash.index, e.target.checked)}
                  />
                ) : (
```

with:

```tsx
                {showCheckbox ? (
                  <Checkbox
                    checked={isSelected}
                    disabled={isBatchRunning}
                    onCheckedChange={(checked) => toggleSelect(stash.index, checked === true)}
                    aria-label={`Select stash @{${stash.index}}`}
                  />
                ) : (
```

- [ ] **Step 5: Run the test to verify it still passes, and confirm the tag is gone**

Run: `yarn test src/components/git/__tests__/GitStashSection.test.tsx`
Expected: PASS

Run: `grep -n "type='checkbox'" src/components/git/GitStashSection.tsx`
Expected: no matches.

- [ ] **Step 6: Typecheck and lint**

Run: `yarn tsc --noEmit`
Expected: no errors

Run: `yarn check`
Expected: no new findings

- [ ] **Step 7: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/GitStashSection.tsx src/components/git/__tests__/GitStashSection.test.tsx
```

Commit message: `refactor(git): replace raw stash-selection checkbox with shadcn Checkbox`.

---

### Task 2: `TabItem` close control

**Files:**
- Modify: `src/components/panes/TabItem.tsx`
- Test: `src/components/panes/__tests__/TabItem.test.tsx` if it exists (check first with `ls src/components/panes/__tests__/`); otherwise this task relies on the broader pane-store/TabBar test suite already exercising tab close.

**Interfaces:**
- Consumes: `Button` from `@/components/ui/button` (new import in this file).
- Produces: no new exports, no visual change — `Button`'s default classes are overridden via `className` to reproduce the exact prior look (transparent background, `opacity-0` fading in on hover/focus-within).

- [ ] **Step 1: Confirm the current violation and check for an existing close-button test**

Run: `grep -n "<button" src/components/panes/TabItem.tsx`
Expected: one match at line 107.

Run: `ls src/components/panes/__tests__/ 2>/dev/null | grep -i tabitem`
If a `TabItem.test.tsx` exists, read it first and reuse its render/query conventions for the verification step below instead of writing a new ad hoc test.

- [ ] **Step 2: Convert the close button**

Add the import at the top of `src/components/panes/TabItem.tsx`:

```tsx
import { Button } from '@/components/ui/button';
```

Replace:

```tsx
      {!isWorkspaceTab(tab) && (
        <button
          type='button'
          aria-label='Close tab'
          onClick={(e) => {
            e.stopPropagation();
            onClose();
          }}
          className='shrink-0 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 hover:text-foreground rounded-sm p-0.5 transition-opacity'
        >
          <X aria-hidden='true' className='h-3.5 w-3.5' />
        </button>
      )}
```

with:

```tsx
      {!isWorkspaceTab(tab) && (
        <Button
          type='button'
          variant='ghost'
          size='icon'
          aria-label='Close tab'
          onClick={(e) => {
            e.stopPropagation();
            onClose();
          }}
          className='h-auto w-auto shrink-0 p-0.5 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 hover:bg-transparent hover:text-foreground rounded-sm transition-opacity'
        >
          <X aria-hidden='true' className='h-3.5 w-3.5' />
        </Button>
      )}
```

`variant='ghost'` on shadcn `Button` already applies a `hover:bg-accent` background by default — `hover:bg-transparent` in the override above cancels that so the control keeps its original "text-only, no background" hover look; drop `hover:bg-transparent` instead if the tab bar's design should show shadcn's default ghost hover background (visually confirm against the running app per this plan's Task 3 manual-check step before deciding).

- [ ] **Step 3: Verify the tag is gone and manually confirm the tab bar renders correctly**

Run: `grep -n "<button" src/components/panes/TabItem.tsx`
Expected: no matches.

Run: `yarn test src/components/panes` (runs the existing pane-store/TabBar/EditorGroup suites, which exercise opening and closing tabs)
Expected: PASS

Since this control is shared by every tab type in the app (not Git-specific), also start the dev server and visually confirm the close "×" still appears on hover/focus and closes the tab, across at least one request tab and one Git tab, per this repo's UI-change verification convention (`yarn tauri dev` or `yarn dev`).

- [ ] **Step 4: Typecheck and lint**

Run: `yarn tsc --noEmit`
Expected: no errors

Run: `yarn check`
Expected: no new findings

- [ ] **Step 5: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/panes/TabItem.tsx
```

Commit message: `refactor(panes): replace raw tab-close button with shadcn Button`.
