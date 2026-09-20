# Duplicate Git Action Guards Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop a rapid double-click on branch, file-staging, and remote actions from firing the same mutation twice concurrently.

**Architecture:** `GitStashSection.tsx` and `BranchSelector`'s remote-checkout row already track an in-flight action locally (`isSaving`/`isBatchRunning`, `checkingOutRemote`) and disable the triggering control while the promise is outstanding — that's the established convention in this codebase for preventing duplicate mutations (there is no centralized "operations keyed by path+action" store slice, and introducing one is out of scope for this plan). `BranchSelector`'s create/switch/merge/delete handlers, `GitFileList`'s stage/unstage/discard handlers (bulk and per-file), and `GitRemotesDialog`'s add/edit/delete handlers don't follow that convention yet — they fire the store action with no busy flag and no `disabled` guard, so a second click before the first `await` resolves starts a second concurrent call. This plan applies the same local-busy-state pattern to those three files.

**Tech Stack:** React 18, TypeScript, Vitest, React Testing Library, `@/test/deferred`.

**Spec:** `docs/reports/git-integration-review/02-frontend-architecture.md` (F-09 "Loading and error state is fragmented and permits overlapping operations"). Verified present in current `src/components/git/BranchSelector.tsx`, `src/components/git/GitFileList.tsx`, `src/components/git/GitRemotesDialog.tsx`.

**Depends on:** `docs/superpowers/plans/2026-09-20-branch-selector-result-handling.md` and `docs/superpowers/plans/2026-09-20-git-remotes-dialog-failure-preserves-state.md` having already been applied — this plan's `BranchSelector`/`GitRemotesDialog` handler bodies build on the `clearError()`/error-checking versions those plans introduce. If they have not been applied yet, apply the same busy-state wrapping to the handler bodies as they currently exist in source — the disabling mechanics below are independent of that other change.

**Next Plan:** After this plan is fully implemented and verified, execute `docs/superpowers/plans/2026-09-20-git-store-selector-narrowing.md` next (plan 12 of 22 in the git-frontend-architecture remediation sequence).

## Global Constraints

- All UI components use shadcn/ui primitives only.
- Commits use conventional commits format.
- `yarn tsc --noEmit` and `yarn test <pattern>` must pass before each commit.

---

### Task 1: `BranchSelector` — busy-state guards for create/switch/merge/delete

**Files:**
- Modify: `src/components/git/BranchSelector.tsx`
- Test: `src/components/git/__tests__/BranchSelector.test.tsx`

**Interfaces:**
- Produces: no new exports. Adds local state `creating: boolean`, `switchingTo: string | null`, `mergingName: string | null`, `deletingName: string | null`, disabling the corresponding trigger button(s) while set.

- [ ] **Step 1: Write the failing test**

Add to `src/components/git/__tests__/BranchSelector.test.tsx`:

```tsx
describe('BranchSelector duplicate-action guards', () => {
  it('disables Create while a create is in flight and only calls createBranch once', async () => {
    const deferred = createDeferred<void>();
    const createBranch = vi.fn(() => deferred.promise);
    renderWithStore({ createBranch });
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /main/ }));
    await user.type(screen.getByLabelText('New branch name'), 'feature-x');
    const createButton = screen.getByLabelText('Create branch');
    await user.click(createButton);

    expect(createButton).toBeDisabled();
    await user.click(createButton); // no-op — button is disabled

    deferred.resolve();
    await vi.waitFor(() => expect(createButton).not.toBeDisabled());
    expect(createBranch).toHaveBeenCalledTimes(1);
  });
});
```

Add `import { createDeferred } from '@/test/deferred';` to the test file if not already present.

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/BranchSelector.test.tsx -t "disables Create while a create is in flight"`
Expected: FAIL — the Create button has no `disabled` binding tied to an in-flight call, so the assertion `expect(createButton).toBeDisabled()` fails immediately.

- [ ] **Step 3: Implement the guard**

In `src/components/git/BranchSelector.tsx`, add busy-state hooks next to the existing `checkingOutRemote` state:

```tsx
  const [checkingOutRemote, setCheckingOutRemote] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [switchingTo, setSwitchingTo] = useState<string | null>(null);
  const [mergingName, setMergingName] = useState<string | null>(null);
  const [deletingName, setDeletingName] = useState<string | null>(null);
```

Wrap `handleCreate`:

```tsx
  const handleCreate = async () => {
    if (!newBranchName.trim() || creating) return;
    setCreating(true);
    setCreateError(null);
    clearError();
    try {
      await createBranch(newBranchName.trim());
      const nextError = gitStoreApi.getState().error;
      if (nextError) {
        setCreateError(nextError);
      } else {
        setNewBranchName('');
      }
    } finally {
      setCreating(false);
    }
  };
```

Wrap `handleSwitch`:

```tsx
  const handleSwitch = async (name: string) => {
    if (switchingTo) return;
    setSwitchingTo(name);
    setSwitchError(null);
    clearError();
    try {
      await switchBranch(name);
      const nextError = gitStoreApi.getState().error;
      if (nextError) {
        setSwitchError(nextError);
      } else {
        setOpen(false);
      }
    } finally {
      setSwitchingTo(null);
    }
  };
```

Wrap `handleMerge`:

```tsx
  const handleMerge = async (name: string) => {
    if (mergingName) return;
    setMergingName(name);
    setSwitchError(null);
    clearError();
    try {
      await mergeBranch(name);
      const nextError = gitStoreApi.getState().error;
      if (nextError) {
        if (nextError.toLowerCase().includes('conflict')) {
          setOpen(false);
        } else {
          setSwitchError(nextError);
        }
      } else {
        setOpen(false);
      }
    } finally {
      setMergingName(null);
    }
  };
```

Wrap `handleDelete`:

```tsx
  const handleDelete = async (name: string) => {
    if (deletingName) return;
    setDeletingName(name);
    setSwitchError(null);
    clearError();
    try {
      await deleteBranch(name);
      const nextError = gitStoreApi.getState().error;
      if (nextError) {
        setSwitchError(nextError);
      }
    } finally {
      setDeletingName(null);
    }
  };
```

Disable the corresponding controls in the JSX: the Create button (`disabled={!newBranchName.trim() || creating}`), each local-branch row's click/keydown handlers (skip when `switchingTo !== null`), and the per-row Merge/Delete buttons (`disabled={mergingName === branch.name || deletingName === branch.name}`):

```tsx
            <Button
              variant='outline'
              size='sm'
              className='h-7 shrink-0'
              onClick={handleCreate}
              disabled={!newBranchName.trim() || creating}
              aria-label='Create branch'
            >
              <Plus className='h-3.5 w-3.5' />
            </Button>
```

```tsx
              onClick={() => {
                if (switchingTo) return;
                if (!branch.isHead) void handleSwitch(branch.name);
                else setOpen(false);
              }}
              onKeyDown={(e) => {
                if ((e.key === 'Enter' || e.key === ' ') && !switchingTo) {
                  if (!branch.isHead) void handleSwitch(branch.name);
                  else setOpen(false);
                }
              }}
```

```tsx
                        <Button
                          variant='ghost'
                          size='icon'
                          className='h-5 w-5'
                          disabled={mergingName === branch.name || deletingName === branch.name}
                          onClick={(e) => {
                            e.stopPropagation();
                            void handleMerge(branch.name);
                          }}
                        >
                          <GitMerge className='h-3.5 w-3.5 text-muted-foreground' />
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent>Merge into current</TooltipContent>
                    </Tooltip>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          variant='ghost'
                          size='icon'
                          className='h-5 w-5 text-destructive'
                          disabled={mergingName === branch.name || deletingName === branch.name}
                          onClick={(e) => {
                            e.stopPropagation();
                            void handleDelete(branch.name);
                          }}
                        >
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/git/__tests__/BranchSelector.test.tsx`
Expected: PASS (all tests in the file)

- [ ] **Step 5: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/BranchSelector.tsx src/components/git/__tests__/BranchSelector.test.tsx
```

Commit message: `fix(git): disable branch actions while an identical action is in flight`.

---

### Task 2: `GitFileList` — busy-state guards for stage/unstage/discard

**Files:**
- Modify: `src/components/git/GitFileList.tsx`
- Test: `src/components/git/__tests__/GitFileList.test.tsx`

**Interfaces:**
- Produces: no new exports. Adds local state `bulkBusy: 'stage' | 'unstage' | 'discard' | null` (disables the three bulk-action buttons while set) and `busyPaths: Set<string>` (disables a row's per-file stage/unstage/discard-trigger buttons while its path is in that set).

- [ ] **Step 1: Write the failing test**

`src/components/git/__tests__/GitFileList.test.tsx` already exists — read it first for its render helper and store-mocking conventions, then add:

```tsx
describe('GitFileList duplicate-action guards', () => {
  it('disables Stage all while a bulk stage is in flight and only calls stageAll once', async () => {
    const deferred = createDeferred<void>();
    const stageAll = vi.fn(() => deferred.promise);
    // Adapt to this file's existing render helper — pass `stageAll` (and any
    // other required store fields such as `status` with an unstaged file) the
    // same way the existing tests in this file already inject store overrides.
    renderFileList({
      status: { branch: 'main', files: [{ path: 'a.txt', staged: false, status: 'modified' }], ahead: 0, behind: 0, isClean: false },
      stageAll,
    });
    const user = userEvent.setup();

    const stageAllButton = screen.getByRole('button', { name: /stage all/i });
    await user.click(stageAllButton);
    expect(stageAllButton).toBeDisabled();
    await user.click(stageAllButton); // no-op — disabled

    deferred.resolve();
    await vi.waitFor(() => expect(stageAllButton).not.toBeDisabled());
    expect(stageAll).toHaveBeenCalledTimes(1);
  });

  it('disables a row while its own stage action is in flight', async () => {
    const deferred = createDeferred<void>();
    const stageFiles = vi.fn(() => deferred.promise);
    renderFileList({
      status: { branch: 'main', files: [{ path: 'a.txt', staged: false, status: 'modified' }], ahead: 0, behind: 0, isClean: false },
      stageFiles,
    });
    const user = userEvent.setup();

    const stageButton = screen.getByRole('button', { name: 'Stage' });
    await user.click(stageButton);
    expect(stageButton).toBeDisabled();

    deferred.resolve();
    await vi.waitFor(() => expect(stageFiles).toHaveBeenCalledTimes(1));
  });
});
```

Use whichever store-injection helper name the existing test file already defines in place of the placeholder `renderFileList(...)` above (check the top of `GitFileList.test.tsx` for its actual helper name and signature before writing this step, and adapt the calls to match exactly — do not introduce a second, differently-named helper).

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/GitFileList.test.tsx -t "duplicate-action guards"`
Expected: FAIL — neither the bulk nor the per-row buttons have a `disabled` binding tied to an in-flight call.

- [ ] **Step 3: Implement the guard**

In `src/components/git/GitFileList.tsx`, add busy-state hooks next to the existing dialog state:

```tsx
  const [showDiscardAllDialog, setShowDiscardAllDialog] = useState(false);
  const [discardingFile, setDiscardingFile] = useState<FileStatus | null>(null);
  const [bulkBusy, setBulkBusy] = useState<'stage' | 'unstage' | 'discard' | null>(null);
  const [busyPaths, setBusyPaths] = useState<Set<string>>(new Set());

  const withPathBusy = async (path: string, action: () => Promise<void>) => {
    setBusyPaths((prev) => new Set(prev).add(path));
    try {
      await action();
    } finally {
      setBusyPaths((prev) => {
        const next = new Set(prev);
        next.delete(path);
        return next;
      });
    }
  };
```

Update the bulk handlers:

```tsx
  const handleConfirmDiscardAll = async () => {
    if (bulkBusy) return;
    setBulkBusy('discard');
    setShowDiscardAllDialog(false);
    try {
      await discardFiles(discardableFiles.map((f) => f.path));
    } finally {
      setBulkBusy(null);
    }
  };

  const handleConfirmDiscardFile = () => {
    if (!discardingFile) return;
    const path = discardingFile.path;
    setDiscardingFile(null);
    void withPathBusy(path, () => discardFiles([path]));
  };

  const handleStageAll = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (bulkBusy) return;
    setBulkBusy('stage');
    try {
      await stageAll();
    } finally {
      setBulkBusy(null);
    }
  };

  const handleUnstageAll = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (bulkBusy) return;
    setBulkBusy('unstage');
    try {
      await unstageAll();
    } finally {
      setBulkBusy(null);
    }
  };
```

Update the bulk-action button `disabled` props:

```tsx
                  <Button
                    variant='ghost'
                    size='icon'
                    className='h-5 w-5'
                    onClick={handleUnstageAll}
                    disabled={bulkBusy !== null}
                  >
```

```tsx
                  <Button
                    variant='ghost'
                    size='icon'
                    className='h-5 w-5'
                    onClick={handleDiscardAll}
                    disabled={discardableFiles.length === 0 || bulkBusy !== null}
                  >
```

```tsx
                  <Button variant='ghost' size='icon' className='h-5 w-5' onClick={handleStageAll} disabled={bulkBusy !== null}>
```

Update the per-row stage/unstage/discard-trigger buttons to route through `withPathBusy` and disable while their own path is busy — for the staged-section unstage button:

```tsx
                        <Button
                          variant='ghost'
                          size='icon'
                          className='h-5 w-5'
                          disabled={busyPaths.has(file.path)}
                          onClick={(e) => {
                            e.stopPropagation();
                            void withPathBusy(file.path, () => unstageFiles([file.path]));
                          }}
                        >
                          <Minus className='h-3.5 w-3.5' />
                        </Button>
```

For the unstaged-section Discard/Stage buttons:

```tsx
                        <Button
                          variant='ghost'
                          size='icon'
                          className='h-5 w-5'
                          aria-label='Discard'
                          disabled={busyPaths.has(file.path)}
                          onClick={(e) => {
                            e.stopPropagation();
                            setDiscardingFile(file);
                          }}
                        >
                          <Trash2 className='h-3.5 w-3.5' />
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent>Discard</TooltipContent>
                    </Tooltip>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          variant='ghost'
                          size='icon'
                          className='h-5 w-5'
                          disabled={busyPaths.has(file.path)}
                          onClick={(e) => {
                            e.stopPropagation();
                            void withPathBusy(file.path, () => stageFiles([file.path]));
                          }}
                        >
                          <Plus className='h-3.5 w-3.5' />
                        </Button>
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/git/__tests__/GitFileList.test.tsx`
Expected: PASS (all tests in the file)

- [ ] **Step 5: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/GitFileList.tsx src/components/git/__tests__/GitFileList.test.tsx
```

Commit message: `fix(git): disable stage/unstage/discard controls while an identical action is in flight`.

---

### Task 3: `GitRemotesDialog` — busy-state guards for add/edit/delete

**Files:**
- Modify: `src/components/git/GitRemotesDialog.tsx`
- Test: `src/components/git/__tests__/GitRemotesDialog.test.tsx`

**Interfaces:**
- Produces: no new exports. Adds local state `adding: boolean`, `savingEdit: boolean`, `removing: boolean`, disabling the corresponding Add/Save/Remove buttons while set.

- [ ] **Step 1: Write the failing test**

Add to `src/components/git/__tests__/GitRemotesDialog.test.tsx`:

```tsx
describe('GitRemotesDialog duplicate-action guards', () => {
  it('disables Add while an add is in flight and only calls addRemote once', async () => {
    const deferred = createDeferred<void>();
    const addRemote = vi.fn(() => deferred.promise);
    const store = createGitStore();
    store.setState({ remotes: [], refreshRemotes: vi.fn().mockResolvedValue(undefined), addRemote });
    renderDialog(store);
    const user = userEvent.setup();

    await user.type(screen.getByPlaceholderText('name'), 'origin');
    await user.type(screen.getByPlaceholderText('https://github.com/...'), 'https://example.com/repo.git');
    const addButton = screen.getByRole('button', { name: /add/i });
    await user.click(addButton);

    expect(addButton).toBeDisabled();
    deferred.resolve();
    await vi.waitFor(() => expect(addButton).not.toBeDisabled());
    expect(addRemote).toHaveBeenCalledTimes(1);
  });
});
```

Add `import { createDeferred } from '@/test/deferred';` to the test file if not already present.

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/GitRemotesDialog.test.tsx -t "disables Add while an add is in flight"`
Expected: FAIL — the Add button's `disabled` prop is only driven by `!canAdd`, not by an in-flight call.

- [ ] **Step 3: Implement the guard**

In `src/components/git/GitRemotesDialog.tsx`, add busy-state hooks next to the existing local state:

```tsx
  const [deletingRemote, setDeletingRemote] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const [savingEdit, setSavingEdit] = useState(false);
  const [removing, setRemoving] = useState(false);
```

Update the three handlers built in the earlier `2026-09-20-git-remotes-dialog-failure-preserves-state.md` plan:

```tsx
  const handleAdd = async () => {
    if (adding) return;
    setAdding(true);
    clearError();
    try {
      await addRemote(newName.trim(), newUrl.trim());
      if (!gitStoreApi.getState().error) {
        setNewName('');
        setNewUrl('');
      }
    } finally {
      setAdding(false);
    }
  };

  const handleSaveEdit = async () => {
    if (!editingRemote || savingEdit) return;
    setSavingEdit(true);
    clearError();
    try {
      await setRemoteUrl(editingRemote, editUrl.trim());
      if (!gitStoreApi.getState().error) {
        setEditingRemote(null);
      }
    } finally {
      setSavingEdit(false);
    }
  };

  const handleConfirmDelete = async () => {
    if (!deletingRemote || removing) return;
    setRemoving(true);
    clearError();
    try {
      await removeRemote(deletingRemote);
      if (!gitStoreApi.getState().error) {
        setDeletingRemote(null);
      }
    } finally {
      setRemoving(false);
    }
  };
```

Update the corresponding buttons' `disabled` props: the Add button (`disabled={!canAdd || adding}`), the edit row's Save button (`disabled={savingEdit}`), and the delete row's Remove button (`disabled={removing}`).

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/git/__tests__/GitRemotesDialog.test.tsx`
Expected: PASS (all tests in the file)

- [ ] **Step 5: Run typecheck and the broader git component suite**

Run: `yarn tsc --noEmit`
Expected: no errors

Run: `yarn test src/components/git`
Expected: PASS

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/GitRemotesDialog.tsx src/components/git/__tests__/GitRemotesDialog.test.tsx
```

Commit message: `fix(git): disable remote add/edit/delete controls while an identical action is in flight`.
