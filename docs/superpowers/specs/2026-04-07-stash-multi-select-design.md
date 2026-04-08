# Git Stash Multi-Select Design

## Goal

Allow users to select multiple stashes in `GitStashSection` and apply, pop, or drop them all in one action.

## Scope

- Frontend: `src/components/git/GitStashSection.tsx`, `src/stores/git-store.ts`
- Backend: no changes — the existing single-index Tauri commands are sufficient

---

## UI Behavior

### Checkbox reveal

Each stash row has a fixed-width slot on the left that shows the `@{N}` index badge by default.

- **Idle** — `@{N}` badge is visible; no checkbox present
- **Hover** — `@{N}` fades out, an unchecked checkbox fades in (same slot, no layout shift)
- **Any box checked** — every row switches to checkbox mode (checkboxes always visible, no hover required), so the user can quickly tick more rows without re-hovering each one
- **All unchecked / × pressed** — every row returns to idle (checkbox hidden, `@{N}` badge visible)

Selection state (`selectedIndices: Set<number>`) lives in `GitStashSection` local component state. It is not persisted to the git store.

### Action bar

A sticky bar renders at the bottom of the stash section whenever `selectedIndices.size ≥ 1`:

```
  2 selected   [Apply]  [Pop]  [Drop]  ×
```

- Slides in from the bottom with a 150 ms ease-out transition; disappears when selection clears
- **Apply** — outline button; restores changes, keeps stashes in the list
- **Pop** — outline button; restores changes and removes applied stashes from the list
- **Drop** — destructive (red) button; deletes selected stashes without touching the working tree
- **×** — clears selection and returns all rows to idle

The per-row `···` hover menu continues to work on any unchecked row.

---

## Action Semantics

### Apply order

Stashes are always processed **newest first**: indices are sorted ascending (`stash@{0}` before `stash@{1}` before `stash@{2}`). The most recent work lands last in the working tree, sitting "on top."

### Apply (multi)

Calls `stash_apply` for each selected index in order. All stashes remain in the list regardless of outcome.

### Pop (multi)

Calls `stash_pop` for each index in order. Each stash is removed from the list as soon as its pop succeeds. If the operation fails mid-batch, previously popped stashes are already consumed — this is expected git behavior and the error message makes it explicit.

### Drop (multi)

Calls `stash_drop` for each index in order. No working-tree changes. Stops on error.

### Conflict handling — stop on first

If any operation in the batch fails:

1. The loop stops immediately.
2. The error banner in `GitStashSection` shows:
   `"Failed at stash@{N}: <error>. Stashes processed before this one were already applied."`
3. `refreshStashes()` runs so the list reflects the real state.
4. The user resolves conflicts via the Conflict Resolver in the git panel, then retries remaining stashes individually.

---

## Store Changes

Three new methods added to `git-store.ts`:

```ts
applyStashMany(indices: number[]): Promise<void>
popStashMany(indices: number[]): Promise<void>
dropStashMany(indices: number[]): Promise<void>
```

Each method:

1. Sorts `indices` ascending (newest first).
2. Iterates, calling the existing single-index store action (`applyStash`, `popStash`, `dropStash`) for each index.
3. Stops on first error — the error is already set in the store by the single-index action.
4. Calls `refreshStashes()` after the loop (success or failure) to sync the displayed list.
5. Apply and Pop additionally call `refreshStatus()` so the changed-files list updates.

No new Tauri IPC commands. No Rust changes. `StashEntry` is unchanged.

---

## Files Changed

| File | Change |
|---|---|
| `src/stores/git-store.ts` | Add `applyStashMany`, `popStashMany`, `dropStashMany` |
| `src/components/git/GitStashSection.tsx` | Add `selectedIndices` state, checkbox reveal logic, sticky action bar |
