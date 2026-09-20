# Accessible Names and Keyboard Behavior Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix three concrete keyboard/screen-reader gaps in the Git panel: (1) pressing Space on a `role='button'` row also scrolls the page because the handler doesn't call `preventDefault`; (2) several icon-only buttons and compact form fields have no accessible name at all; (3) the stash multi-select checkbox is only rendered on pointer hover, so it cannot be reached by keyboard.

**Architecture:** `GitFileList.tsx`, `BranchSelector.tsx`, and `GitCommitLog.tsx` each have a `role='button'` row (used instead of a real `<button>` because the row nests an actual interactive `Button`, and nesting a native `<button>` inside another `<button>` is invalid HTML that breaks hover tracking in WebKitGTK — see the existing `biome-ignore` comments on those rows) whose `onKeyDown` handles `' '` but never calls `e.preventDefault()`, so the browser's default "Space scrolls the page" behavior still fires alongside the row's own action. Several icon-only buttons across `GitFileList`, `BranchSelector`, and `GitStashSection` rely only on a `Tooltip` for their label, which is not an accessible name; a handful of compact inputs (`GitCloneDialog`'s URL/destination fields, `GitRemotesDialog`'s inline name/url fields, `GitStashSection`'s message field) have only a placeholder or an unassociated `Label`. `GitStashSection`'s stash-row checkbox is conditionally rendered (`showCheckbox = isSelecting || hoveredIndex === stash.index`) — when neither is true, the `Checkbox` isn't in the DOM at all, so `Tab` can never land on it to start a selection; the fix keeps it always mounted and uses CSS opacity (which doesn't affect tab order) plus a `peer-focus-visible` reveal so it's invisible-but-reachable until hovered, selecting, or focused.

**Tech Stack:** React 18, TypeScript, Tailwind CSS (`peer`/`focus-visible` variants), Radix `Checkbox`, Vitest, React Testing Library, `@testing-library/user-event`.

**Spec:** `docs/reports/git-integration-review/02-frontend-architecture.md` (F-12 "Keyboard and screen-reader behavior is incomplete"). Verified present in current `src/components/git/GitFileList.tsx:116-118,204-212`, `src/components/git/BranchSelector.tsx:151-156`, `src/components/git/GitCommitLog.tsx:56-58`, `src/components/git/GitStashSection.tsx:181-207`, `src/components/git/GitCloneDialog.tsx:247-267`, `src/components/git/GitRemotesDialog.tsx:186-203`.

**Depends on:** `docs/superpowers/plans/2026-09-20-git-raw-controls-checkbox-close.md` (Task 1) having already converted `GitStashSection`'s raw `<input type='checkbox'>` to shadcn `Checkbox` — Task 3 below builds on that. If it hasn't been applied yet, apply the same always-mounted/opacity-driven visibility change to the raw `<input type='checkbox'>` instead — the reachability fix is otherwise identical, and the `Checkbox`-only bits (Step 1's import, its exact `className` prop) can be skipped.

**Next Plan:** After this plan is fully implemented and verified, execute `docs/superpowers/plans/2026-09-20-workspace-git-tab-query-states.md` next (plan 17 of 22 in the git-frontend-architecture remediation sequence).

## Global Constraints

- All UI components use shadcn/ui primitives only.
- Icons: `lucide-react` only.
- Commits use conventional commits format.
- `yarn tsc --noEmit`, `yarn check`, and `yarn test <pattern>` must pass before each commit.

---

### Task 1: `preventDefault` on Space for role=button rows

**Files:**
- Modify: `src/components/git/GitFileList.tsx:116-118,204-212`
- Modify: `src/components/git/BranchSelector.tsx:151-156`
- Modify: `src/components/git/GitCommitLog.tsx:56-58`
- Test: `src/components/git/__tests__/GitFileList.test.tsx`, `src/components/git/__tests__/BranchSelector.test.tsx`, `src/components/git/__tests__/GitCommitLog.test.tsx`

**Interfaces:**
- Produces: no new exports, no behavior change to which action fires — only stops the browser's default Space-scrolls-page behavior from also firing.

- [ ] **Step 1: Write the failing test**

Add to `src/components/git/__tests__/GitCommitLog.test.tsx` (created by the selector-narrowing plan; extend it):

```tsx
  it('prevents the default Space-scroll behavior when activating a row', async () => {
    const store = createGitStore();
    store.setState({
      commitLog: [
        {
          id: 'abc1234',
          fullId: 'abc1234full',
          message: 'initial commit',
          author: 'Test',
          authorEmail: 'test@test.com',
          timestamp: new Date().toISOString(),
          filesChanged: 1,
        },
      ],
    });
    const onCommitClick = vi.fn();
    render(
      <GitStoreProvider store={store}>
        <GitCommitLog onCommitClick={onCommitClick} />
      </GitStoreProvider>,
    );
    const row = screen.getByRole('button', { name: /initial commit/ });
    const event = new KeyboardEvent('keydown', { key: ' ', bubbles: true, cancelable: true });
    row.dispatchEvent(event);

    expect(onCommitClick).toHaveBeenCalledWith(expect.objectContaining({ id: 'abc1234' }));
    expect(event.defaultPrevented).toBe(true);
  });
```

Add matching tests to `GitFileList.test.tsx` and `BranchSelector.test.tsx` for their respective rows, following the same `dispatchEvent` + `defaultPrevented` assertion pattern, adapted to each file's existing render helper.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test src/components/git/__tests__/GitCommitLog.test.tsx src/components/git/__tests__/GitFileList.test.tsx src/components/git/__tests__/BranchSelector.test.tsx`
Expected: FAIL — `event.defaultPrevented` is `false` in each case; the action itself already fires correctly (that part isn't broken).

- [ ] **Step 3: Add `preventDefault` to each row's Space/Enter handler**

In `src/components/git/GitCommitLog.tsx`:

```tsx
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                onCommitClick(commit);
              }
            }}
```

In `src/components/git/BranchSelector.tsx`:

```tsx
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  if (!branch.isHead) void handleSwitch(branch.name);
                  else setOpen(false);
                }
              }}
```

In `src/components/git/GitFileList.tsx`, the staged-section row:

```tsx
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      onFileClick(file);
                    }
                  }}
```

and the unstaged-section row:

```tsx
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    if (isConflicted) {
                      void handleConflictClick(file);
                    } else {
                      onFileClick(file);
                    }
                  }
                }}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test src/components/git/__tests__/GitCommitLog.test.tsx src/components/git/__tests__/GitFileList.test.tsx src/components/git/__tests__/BranchSelector.test.tsx`
Expected: PASS

- [ ] **Step 5: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/GitFileList.tsx src/components/git/BranchSelector.tsx src/components/git/GitCommitLog.tsx src/components/git/__tests__/GitFileList.test.tsx src/components/git/__tests__/BranchSelector.test.tsx src/components/git/__tests__/GitCommitLog.test.tsx
```

Commit message: `fix(git): prevent page scroll when activating a row with Space`.

---

### Task 2: Accessible names for icon-only buttons and compact form fields

**Files:**
- Modify: `src/components/git/GitFileList.tsx` (Stage all / Unstage all / Discard all / per-row Stage / per-row Unstage buttons)
- Modify: `src/components/git/BranchSelector.tsx` (Merge / Delete icon buttons)
- Modify: `src/components/git/GitStashSection.tsx` (per-stash actions-menu trigger, message input)
- Modify: `src/components/git/GitCloneDialog.tsx` (Repository URL, Destination inputs)
- Modify: `src/components/git/GitRemotesDialog.tsx` (new-remote name/url inputs)
- Test: `src/components/git/__tests__/GitFileList.test.tsx`, `src/components/git/__tests__/BranchSelector.test.tsx`, `src/components/git/__tests__/GitStashSection.test.tsx`, `src/components/git/__tests__/GitCloneDialog.test.tsx`, `src/components/git/__tests__/GitRemotesDialog.test.tsx`

**Interfaces:**
- Produces: no new exports. Every listed control gains an `aria-label` matching its existing `Tooltip`/placeholder text, so it becomes queryable by accessible name.

- [ ] **Step 1: Write the failing tests**

Add to `src/components/git/__tests__/GitFileList.test.tsx`:

```tsx
  it('gives the bulk stage/unstage/discard controls accessible names', () => {
    renderFileList({
      status: {
        branch: 'main',
        files: [
          { path: 'staged.txt', staged: true, status: 'modified' },
          { path: 'unstaged.txt', staged: false, status: 'modified' },
        ],
        ahead: 0,
        behind: 0,
        isClean: false,
      },
    });
    expect(screen.getByRole('button', { name: 'Unstage all' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Stage all' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Discard all unstaged' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Unstage' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Stage' })).toBeInTheDocument();
  });
```

(Use the file's existing render helper name in place of the placeholder `renderFileList(...)` above.)

Add to `src/components/git/__tests__/BranchSelector.test.tsx`:

```tsx
  it('gives the per-branch Merge and Delete icon buttons accessible names', async () => {
    renderWithStore({
      branches: {
        current: 'main',
        local: [
          { name: 'main', isHead: true, isRemote: false },
          { name: 'develop', isHead: false, isRemote: false },
        ],
        remote: [],
      },
    });
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /main/ }));

    expect(screen.getByRole('button', { name: 'Merge into current' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Delete branch' })).toBeInTheDocument();
  });
```

Add to `src/components/git/__tests__/GitStashSection.test.tsx`:

```tsx
  it('gives the stash actions-menu trigger and message input accessible names', () => {
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
    expect(screen.getByLabelText('Stash message')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Stash actions' })).toBeInTheDocument();
  });
```

Add to `src/components/git/__tests__/GitCloneDialog.test.tsx`:

```tsx
  it('gives the URL and destination fields accessible names', () => {
    renderDialog(vi.fn(), true);
    expect(screen.getByLabelText('Repository URL')).toBeInTheDocument();
    expect(screen.getByLabelText('Destination')).toBeInTheDocument();
  });
```

Add to `src/components/git/__tests__/GitRemotesDialog.test.tsx`:

```tsx
  it('gives the new-remote name and url fields accessible names', () => {
    const store = createGitStore();
    store.setState({ remotes: [], refreshRemotes: vi.fn().mockResolvedValue(undefined) });
    renderDialog(store);
    expect(screen.getByLabelText('Remote name')).toBeInTheDocument();
    expect(screen.getByLabelText('Remote URL')).toBeInTheDocument();
  });
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test src/components/git/__tests__/GitFileList.test.tsx src/components/git/__tests__/BranchSelector.test.tsx src/components/git/__tests__/GitStashSection.test.tsx src/components/git/__tests__/GitCloneDialog.test.tsx src/components/git/__tests__/GitRemotesDialog.test.tsx`
Expected: FAIL — none of these controls currently expose that accessible name.

- [ ] **Step 3: Add the accessible names**

In `src/components/git/GitFileList.tsx`, add `aria-label` to the five buttons that currently only have a `Tooltip`:

```tsx
                      <Button
                        variant='ghost'
                        size='icon'
                        className='h-5 w-5'
                        aria-label='Unstage all'
                        onClick={handleUnstageAll}
                      >
```

```tsx
                  <Button
                    variant='ghost'
                    size='icon'
                    className='h-5 w-5'
                    aria-label='Discard all unstaged'
                    onClick={handleDiscardAll}
                    disabled={discardableFiles.length === 0 || bulkBusy !== null}
                  >
```

```tsx
                  <Button variant='ghost' size='icon' className='h-5 w-5' aria-label='Stage all' onClick={handleStageAll} disabled={bulkBusy !== null}>
```

```tsx
                        <Button
                          variant='ghost'
                          size='icon'
                          className='h-5 w-5'
                          aria-label='Unstage'
                          disabled={busyPaths.has(file.path)}
                          onClick={(e) => {
                            e.stopPropagation();
                            void withPathBusy(file.path, () => unstageFiles([file.path]));
                          }}
                        >
```

```tsx
                        <Button
                          variant='ghost'
                          size='icon'
                          className='h-5 w-5'
                          aria-label='Stage'
                          disabled={busyPaths.has(file.path)}
                          onClick={(e) => {
                            e.stopPropagation();
                            void withPathBusy(file.path, () => stageFiles([file.path]));
                          }}
                        >
```

(`aria-label='Discard'` already exists on the per-row Discard button — no change needed there. The `disabled`/`onClick` bodies shown above match the state introduced by `docs/superpowers/plans/2026-09-20-git-duplicate-action-guards.md`; if that plan hasn't been applied yet, add only the `aria-label` line to each button's existing props.)

In `src/components/git/BranchSelector.tsx`:

```tsx
                        <Button
                          variant='ghost'
                          size='icon'
                          className='h-5 w-5'
                          aria-label='Merge into current'
                          disabled={mergingName === branch.name || deletingName === branch.name}
                          onClick={(e) => {
                            e.stopPropagation();
                            void handleMerge(branch.name);
                          }}
                        >
```

```tsx
                        <Button
                          variant='ghost'
                          size='icon'
                          className='h-5 w-5 text-destructive'
                          aria-label='Delete branch'
                          disabled={mergingName === branch.name || deletingName === branch.name}
                          onClick={(e) => {
                            e.stopPropagation();
                            void handleDelete(branch.name);
                          }}
                        >
```

(Again, adapt the `disabled`/`onClick` bodies to whatever `docs/superpowers/plans/2026-09-20-git-duplicate-action-guards.md` and `docs/superpowers/plans/2026-09-20-branch-selector-result-handling.md` left in place if applied — this task only adds the `aria-label` line.)

In `src/components/git/GitStashSection.tsx`, label the message input and the actions-menu trigger:

```tsx
        <Input
          placeholder='Describe your stash…'
          aria-label='Stash message'
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          className='h-7 text-xs flex-1'
          disabled={isSaving}
          onKeyDown={(e) => e.key === 'Enter' && !isSaving && void handleSave()}
        />
```

```tsx
                  <DropdownMenuTrigger asChild>
                    <Button variant='ghost' size='icon' className='h-6 w-6' aria-label='Stash actions'>
                      <MoreHorizontal className='h-3.5 w-3.5' />
                    </Button>
                  </DropdownMenuTrigger>
```

In `src/components/git/GitCloneDialog.tsx`, label the two inputs:

```tsx
          <div>
            <Label className='text-sm'>Repository URL</Label>
            <Input
              placeholder='https://github.com/user/repo.git'
              aria-label='Repository URL'
              value={repoUrl}
              onChange={(e) => setRepoUrl(e.target.value)}
              className='h-8 text-sm'
            />
          </div>
          <div>
            <Label className='text-sm'>Destination</Label>
            <div className='flex gap-2'>
              <Input
                value={destination?.displayPath ?? ''}
                readOnly
                aria-label='Destination'
                placeholder='Select an empty folder'
                className='h-8 text-sm flex-1'
              />
```

In `src/components/git/GitRemotesDialog.tsx`, label the two new-remote inputs:

```tsx
              <Input
                placeholder='name'
                aria-label='Remote name'
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                className='h-8 text-sm flex-[2] min-w-0'
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && canAdd) handleAdd();
                }}
              />
              <Input
                placeholder='https://github.com/...'
                aria-label='Remote URL'
                value={newUrl}
                onChange={(e) => setNewUrl(e.target.value)}
                className='h-8 text-sm flex-[5] min-w-0'
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && canAdd) handleAdd();
                }}
              />
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test src/components/git/__tests__/GitFileList.test.tsx src/components/git/__tests__/BranchSelector.test.tsx src/components/git/__tests__/GitStashSection.test.tsx src/components/git/__tests__/GitCloneDialog.test.tsx src/components/git/__tests__/GitRemotesDialog.test.tsx`
Expected: PASS

- [ ] **Step 5: Run the full git component suite and typecheck**

Run: `yarn test src/components/git`
Expected: PASS

Run: `yarn tsc --noEmit`
Expected: no errors

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/GitFileList.tsx src/components/git/BranchSelector.tsx src/components/git/GitStashSection.tsx src/components/git/GitCloneDialog.tsx src/components/git/GitRemotesDialog.tsx src/components/git/__tests__/GitFileList.test.tsx src/components/git/__tests__/BranchSelector.test.tsx src/components/git/__tests__/GitStashSection.test.tsx src/components/git/__tests__/GitCloneDialog.test.tsx src/components/git/__tests__/GitRemotesDialog.test.tsx
```

Commit message: `fix(git): add accessible names to icon-only buttons and compact form fields`.

---

### Task 3: Make the stash-selection checkbox reachable by keyboard without hovering

**Files:**
- Modify: `src/components/git/GitStashSection.tsx`
- Test: `src/components/git/__tests__/GitStashSection.test.tsx`

**Interfaces:**
- Consumes: `cn` from `@/lib/utils` (new import in this file).
- Produces: no new exports. The stash-row checkbox is now always mounted; its visibility is CSS-driven (`opacity-0` unless hovered, selecting, or `focus-visible`) instead of conditionally rendered.

- [ ] **Step 1: Write the failing test**

Add to `src/components/git/__tests__/GitStashSection.test.tsx`:

```tsx
  it('reaches and toggles the first stash checkbox via keyboard without hovering', async () => {
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

    // No hover — the checkbox must already be present and focusable.
    const checkbox = screen.getByRole('checkbox');
    checkbox.focus();
    expect(checkbox).toHaveFocus();

    await user.keyboard(' ');
    expect(checkbox).toBeChecked();
    expect(await screen.findByText('1 selected')).toBeInTheDocument();
  });
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/GitStashSection.test.tsx -t "reaches and toggles the first stash checkbox"`
Expected: FAIL — `screen.getByRole('checkbox')` throws, since the checkbox is not rendered at all until `hoveredIndex === stash.index` or `isSelecting`.

- [ ] **Step 3: Always mount the checkbox, hidden via opacity until hover/selection/focus**

In `src/components/git/GitStashSection.tsx`, add the `cn` import:

```tsx
import { cn } from '@/lib/utils';
```

Replace the checkbox/index-badge slot:

```tsx
              {/* Checkbox / index badge slot — fixed width, no layout shift */}
              <div className='shrink-0 w-6 h-4 flex items-center justify-end relative'>
                <Checkbox
                  checked={isSelected}
                  disabled={isBatchRunning}
                  onCheckedChange={(checked) => toggleSelect(stash.index, checked === true)}
                  aria-label={`Select stash @{${stash.index}}`}
                  className={cn(
                    'peer absolute transition-opacity',
                    showCheckbox ? 'opacity-100' : 'opacity-0 focus-visible:opacity-100',
                  )}
                />
                <span
                  className={cn(
                    'text-[10px] font-mono text-muted-foreground/35 select-none leading-none transition-opacity pointer-events-none',
                    showCheckbox ? 'opacity-0' : 'peer-focus-visible:opacity-0',
                  )}
                >
                  @{stash.index}
                </span>
              </div>
```

(`Checkbox` and its import from `@/components/ui/checkbox` come from `docs/superpowers/plans/2026-09-20-git-raw-controls-checkbox-close.md` Task 1 — this replaces that task's conditional `{showCheckbox ? <Checkbox .../> : <span>...}` with the always-mounted version above. If that plan has not been applied yet, apply the same always-mounted/opacity-driven structure to the raw `<input type='checkbox'>` instead, adding `peer` to its `className` and the matching `peer-focus-visible:opacity-0` span.)

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/git/__tests__/GitStashSection.test.tsx -t "reaches and toggles the first stash checkbox"`
Expected: PASS

- [ ] **Step 5: Run the full stash suite, typecheck, and lint**

Run: `yarn test src/components/git/__tests__/GitStashSection.test.tsx`
Expected: PASS (all tests in the file, including the earlier "selects a stash via an accessible checkbox" hover-based test — hovering still works, it's an additional path to visibility, not a replacement)

Run: `yarn tsc --noEmit`
Expected: no errors

Run: `yarn check`
Expected: no new findings

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/GitStashSection.tsx src/components/git/__tests__/GitStashSection.test.tsx
```

Commit message: `fix(git): make the stash selection checkbox reachable by keyboard without hovering`.
