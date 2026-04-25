# Git Implementation — UX & Discoverability Improvements

**Date:** 2026-04-25  
**Scope:** Frontend (`src/components/git/`, `src/stores/git-store.ts`, `src/components/layout/`)  
**Purpose:** Close gaps where implemented git features are invisible, hard to reach, or provide insufficient feedback for the user to understand what happened.

---

## UX Gap Inventory

### U1 — No global keyboard shortcut to open the Git panel

**Current state:** The only programmatic entry point is the `GitToolbarButton` in the top bar — a small git-branch icon that requires a mouse click. There is no keyboard shortcut defined anywhere in the app.

**Impact:** Power users who work keyboard-first have no way to reach the git panel without the mouse.

**Fix:** Register a global keyboard shortcut (e.g. `Ctrl+Shift+G` / `Cmd+Shift+G`) that invokes the same logic as `GitToolbarButton.onClick`. Wire it through the existing keyboard shortcut system (check `.claude/frontend.md` for the shortcut registration pattern). Add a tooltip on the `GitToolbarButton` showing the shortcut.

---

### U2 — Ahead/behind counts are invisible unless the Git panel is open

**Current state:** The `RepoStatus` type includes `ahead` and `behind` fields. These are rendered inside `GitLandingPanel` as `↑N` / `↓N` badges, but only when the git tab is active. The rest of the app has no indication that there are commits to push or pull.

**Impact:** Users working in the request editor have no ambient signal that they are N commits behind remote — they miss opportunities to pull before making conflicting changes.

**Fix:** Add an ahead/behind indicator to the status bar (where the git branch name or the layout toggle already lives). Show `↑N` in amber when ahead > 0, `↓N` in amber when behind > 0. The status bar already subscribes to git store state in `GitToolbarButton` — extend the same pattern. If neither ahead nor behind, show nothing (no clutter).

---

### U3 — Error from `merge_branch` (BranchSelector) drops into the store but never surfaces back to BranchSelector

**File:** `src/components/git/BranchSelector.tsx:151–155`

```tsx
onClick={(e) => {
  e.stopPropagation();
  mergeBranch(branch.name);  // ← no await, no error capture
  setOpen(false);             // ← popover closes before merge completes
}}
```

**Problem:** `mergeBranch` is called without `await` and the popover is immediately closed. If the merge fails (e.g. conflicts or dirty working tree), the error goes into `state.error` in the git store but `BranchSelector` has already closed — the user has no idea the merge failed. The `switchError` state that BranchSelector uses for inline errors is never set for merge failures.

**Fix:** `await mergeBranch(branch.name)` before `setOpen(false)`. Check store error after (same pattern as `handleSwitch`). If an error occurred, keep the popover open and display `switchError`. If a conflict occurred, close the popover and navigate the right panel to the conflict resolver (emit a navigation event or call the parent's `setRightPanel`).

---

### U4 — Commit form has no validation: empty message allowed, no staged-files guard

**File:** `src/components/git/GitCommitForm.tsx`

**Current state:** The commit button is presumably enabled whenever the message input is non-empty, but there is no guard against committing when zero files are staged. On the backend, `commit()` with no staged changes will succeed and create an empty commit.

**Impact:** Users can accidentally create empty commits. Users can also commit with a single-space message (passes length check but is semantically invalid).

**Fix:**
1. Disable the commit button when `status?.files.filter(f => f.staged).length === 0`.
2. Disable the commit button when `message.trim().length === 0`.
3. Show a subtle inline hint below the text area: _"No files staged"_ (instead of just a disabled button with no explanation).

---

### U5 — `GitCommitLog` shows `files_changed: 0` for all commits and has no way to expand a commit to see its diff

**File:** `src/components/git/GitCommitLog.tsx`

**Current state:** The commit log renders each `CommitInfo` with author, timestamp, and message. `files_changed` is always 0 (see bug B6). There is no way to click a commit and see its diff — the component is read-only history display.

**Impact:** Users expect to be able to inspect what changed in a past commit. The commit log is informational only, making it less useful than `git log --stat`.

**Fix (depends on B6 fix):** Once `files_changed` is populated, display it as a stat badge (e.g. "3 files changed"). For the diff-on-click feature: add a `diff_commit` Tauri command that diffs a commit OID against its parent, returning `Vec<FileDiff>`. Clicking a commit in the log opens a read-only diff view in the right panel showing all changed files for that commit.

---

### U6 — No visual indication when a branch has commits behind its upstream

**File:** `src/components/git/BranchSelector.tsx`

**Current state:** The branch list shows branch names and a checkmark for the current branch. No indicator shows whether a branch is ahead or behind its upstream tracking branch.

**Impact:** Users can't tell at a glance which branches need to be pulled from the branch list.

**Fix:** In `branches()` response, the `Branch` struct already includes `upstream: Option<String>`. Add `ahead: usize, behind: usize` fields to `Branch`. Populate them in `Git2Service::branches()` via `repo.graph_ahead_behind()` for each local branch with a configured upstream. In `BranchSelector`, render a small `↑N↓N` indicator next to each branch name that has a non-zero count.

---

### U7 — No confirmation dialog before destructive `discard` actions

**File:** `src/components/git/GitFileList.tsx:30–33` (discard all)

```tsx
const handleDiscardAll = (e: React.MouseEvent) => {
  e.stopPropagation();
  discardFiles(unstaged.filter(f => f.status !== 'conflicted').map(f => f.path));
```

**Current state:** Clicking the trash icon on an individual file or "Discard all" immediately calls `discardFiles` with no confirmation. Discarded changes cannot be recovered (they are not stashed).

**Impact:** One misclick permanently destroys uncommitted work.

**Fix:**
- Individual file discard: no confirmation needed (single file, low blast radius).
- "Discard all": show an `AlertDialog` with "This will permanently discard all N unstaged changes. This cannot be undone." and require explicit confirmation. Mirror the "Stash & Pull" pattern already used in `GitLandingPanel`.

---

### U8 — Credential dialog does not persist credentials across sessions

**File:** `src/stores/git-store.ts` — `credentials` is in-memory Zustand state only.

**Current state:** Users must re-enter credentials every time the app restarts. There is no "remember credentials" option.

**Impact:** Users with token-based auth (most common for GitHub/GitLab) must paste their token every session, which is friction-heavy.

**Fix:** Add a "Remember credentials" checkbox to `GitCredentialsDialog`. When checked, persist the credentials to Tauri's secure store (use `tauri-plugin-stronghold` or `tauri-plugin-store` with the OS keychain). Load them on app startup into the git store. Credentials stored this way should be scoped to the workspace/collection path to avoid leaking credentials across different remotes.

---

### U9 — Visual diff mode label is misleading: the button says "Visual" but applies only to `.yml` files

**File:** `src/components/git/DiffViewer.tsx:86`

```tsx
const canShowVisual = diffState.filePath.endsWith('.yml');
```

**Current state:** The toggle shows "Text / Visual" for all files, but clicking "Visual" on a non-.yml file renders a fallback message "Visual diff not available." The toggle renders as if it's going to do something, then disappoints.

**Fix:** Hide the mode toggle entirely when `!canShowVisual`. The user never needs to see "Visual" for files where it can't work. When `canShowVisual` is true, the toggle remains — no change needed for `.yml` files.

---

## Summary Table

| ID | Priority | Area | Description |
|----|----------|------|-------------|
| U1 | High | Keyboard | No global shortcut to open git panel |
| U2 | High | Status bar | Ahead/behind invisible outside git tab |
| U3 | High | BranchSelector | Merge errors silently dropped, popover closes early |
| U4 | Medium | Commit form | No guard against empty commits or whitespace-only message |
| U5 | Medium | Commit log | `files_changed` always 0; no drill-down to commit diff |
| U6 | Low | BranchSelector | No ahead/behind indicator per branch in list |
| U7 | High | FileList | "Discard all" has no confirmation — data-loss risk |
| U8 | Medium | Credentials | Credentials are session-only; no keychain persistence |
| U9 | Low | DiffViewer | Visual/Text toggle shown for all files even when not applicable |

---

## Notes on Dependencies

- **U5** depends on bug fix **B6** (files_changed must be populated before it's useful to display).
- **U6** depends on adding `ahead/behind` fields to the `Branch` domain type in `rocket-git`.
- **U8** requires adding a Tauri plugin dependency — check `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json` before implementing.
