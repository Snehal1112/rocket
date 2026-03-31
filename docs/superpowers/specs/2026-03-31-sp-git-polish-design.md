# SP-Git: Git UI Polish & Missing Dialogs — Design Spec

**Date:** 2026-03-31
**Status:** Approved
**Scope:** Independent from workspace feature work

## Overview

This spec covers three new features and a UI polish pass for RocketAPI's Git integration, bringing it closer to Bruno's Git UI quality.

**What already exists (fully implemented):**
- `rocket-git` crate with `git2` — full `GitService` trait: init, status, diff, stage/unstage/discard, commit, log, push/pull/fetch, branches (create/switch/delete/merge), stash (save/pop/apply/drop), conflicts (detect/resolve), clone
- `rocket-app` — `GitAppService` wrapping all operations with domain events
- `src-tauri` — all Git Tauri commands wired
- Frontend: `WorkspaceGitTab` (full panel with Changes/Log/Stash sub-tabs), `BranchSelector`, `GitRemoteActions`, `GitCommitForm`, `GitStagedFiles`, `GitChangedFiles`, `GitFileRow`, `GitCommitLog`, `GitStashSection`, `DiffViewer` (Monaco side-by-side), `DiffHeader`, `ConflictResolver` (3-pane), `GitCredentialsDialog`, `GitToolbarButton`
- `git-store.ts` — full Zustand store with all Git operations

**What needs to be built:**
1. Backend remote CRUD (new trait methods + Tauri commands)
2. Connect Remote dialog (frontend)
3. Clone Repository dialog (frontend + one new Tauri command)
4. Holistic UI polish pass (deferred until 1–3 are done)

---

## Constraints

- **File format: `.yml` only.** No `.json` files anywhere in the backend. All persistent file I/O uses YAML. Serde `camelCase` annotations on structs are for Tauri IPC (in-memory serialization over the bridge), not for file storage.
- **shadcn/ui only** for all frontend components — no raw HTML elements.
- **Lucide icons only** — no inline SVGs.

---

## Phase 1: Backend — Remote CRUD

### New domain type

File: `crates/rocket-git/src/remote.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteInfo {
    pub name: String,
    pub url: String,
}
```

Re-export via `lib.rs`.

### New trait methods on `GitService`

File: `crates/rocket-git/src/service.rs`

```rust
fn list_remotes(&self, path: &str) -> DomainResult<Vec<RemoteInfo>>;
fn add_remote(&self, path: &str, name: &str, url: &str) -> DomainResult<()>;
fn remove_remote(&self, path: &str, name: &str) -> DomainResult<()>;
fn set_remote_url(&self, path: &str, name: &str, url: &str) -> DomainResult<()>;
```

### Implementation in `Git2Service`

File: `crates/rocket-git/src/git2_service.rs`

- `list_remotes`: calls `repo.remotes()` → iterates, reads name + URL via `repo.find_remote(name)`
- `add_remote`: calls `repo.remote(name, url)`
- `remove_remote`: calls `repo.remote_delete(name)`
- `set_remote_url`: calls `repo.remote_set_url(name, url)`

### Design decision: no `rename_remote`

Renaming a remote is rare and `git2` doesn't have a clean rename API (requires delete + recreate). Editing the URL is the 99% use case. Users who need to rename can remove + add.

### New domain events

File: `crates/rocket-shared/src/events.rs`

```rust
GitRemoteAdded { collection: String, name: String, url: String },
GitRemoteRemoved { collection: String, name: String },
```

### `GitAppService` wrapper methods

File: `crates/rocket-app/src/git_service.rs`

Four new methods wrapping trait calls + event publishing for add/remove.

### Tauri commands

File: `src-tauri/src/commands/git.rs` (or equivalent)

- `git_list_remotes(collection_path: String) -> Vec<RemoteInfo>`
- `git_add_remote(collection_path: String, name: String, url: String)`
- `git_remove_remote(collection_path: String, name: String)`
- `git_set_remote_url(collection_path: String, name: String, url: String)`

### Frontend wiring

**`src/lib/tauri-api.ts`** — 4 new exports + `RemoteInfo` type:

```typescript
export interface RemoteInfo {
  name: string;
  url: string;
}

export const gitListRemotes = (collectionPath: string) =>
  invoke<RemoteInfo[]>("git_list_remotes", { collectionPath });

export const gitAddRemote = (collectionPath: string, name: string, url: string) =>
  invoke<void>("git_add_remote", { collectionPath, name, url });

export const gitRemoveRemote = (collectionPath: string, name: string) =>
  invoke<void>("git_remove_remote", { collectionPath, name });

export const gitSetRemoteUrl = (collectionPath: string, name: string, url: string) =>
  invoke<void>("git_set_remote_url", { collectionPath, name, url });
```

**`src/stores/git-store.ts`** — new state + actions:

- State: `remotes: RemoteInfo[]`
- Actions: `refreshRemotes()`, `addRemote(name, url)`, `removeRemote(name)`, `setRemoteUrl(name, url)`

### Tests

- Unit tests in `git2_service.rs` for all four operations (using `tempfile::TempDir`)
- Verify `list_remotes` returns empty for fresh repo
- Verify add → list shows the remote
- Verify remove → list no longer shows it
- Verify `set_remote_url` changes the URL

---

## Phase 2: Connect Remote Dialog

### Trigger

A settings/gear icon or "Remotes" link in the Git tab header area (near branch selector or remote actions). Opens a shadcn `Dialog`.

### Component

File: `src/components/git/GitRemotesDialog.tsx`

### Layout

- **Title:** "Manage Remotes"
- **Remote list:** Each row shows `name` (bold) and `url` (truncated), with:
  - Pencil icon (`Pencil` from lucide-react) — enters edit mode for that row
  - Trash icon (`Trash2` from lucide-react) — inline delete confirmation
- **Empty state:** "No remotes configured."
- **Add Remote form** (bottom of dialog):
  - `Input` for "Remote Name" (placeholder: `origin`)
  - `Input` for "Remote URL" (placeholder: `https://github.com/user/repo.git`)
  - "Add" `Button`
- **Edit mode:** Clicking pencil swaps the URL cell to an `Input` with Save (`Check`) / Cancel (`X`) icon buttons. Name is read-only during edit.
- **Delete confirmation:** Inline within the row — "Remove 'origin'?" with Confirm/Cancel buttons. No separate dialog.
- **Validation:**
  - Remote name: non-empty, no spaces, no duplicates
  - Remote URL: non-empty
- **Close:** Standard dialog close button. Each action (add/edit/delete) is immediate (no global submit).

### Data flow

- On dialog open: call `refreshRemotes()` from git-store
- Add/Edit/Delete: call respective git-store action → on success, `refreshRemotes()` to update the list
- No credentials handling in this dialog — credentials are managed separately via `GitCredentialsDialog` during push/pull/fetch

---

## Phase 3: Clone Repository Dialog

### Trigger

Inside the Git tab's "not a repo" empty state. Currently shows "This workspace is not a Git repository" with an "Initialize" button. A second button is added: "Clone Repository". Opens a shadcn `Dialog`.

### Component

File: `src/components/git/GitCloneDialog.tsx`

### 3-step flow

**Step 1: Input**
- `Input` for "Repository URL" (placeholder: `https://github.com/user/repo.git` or `git@github.com:user/repo.git`)
- `Input` for "Destination Folder" with a "Browse" `Button` that triggers Tauri's `dialog.open` (directory picker)
- "Clone" `Button` — disabled until both fields are filled
- When user clicks Clone and the repo requires authentication, the existing `GitCredentialsDialog` pops up as a sub-step. After credentials are provided, cloning proceeds.

**Step 2: Progress**
- `Loader2` spinner with "Cloning repository..." text
- Clone button disabled
- On failure: inline error message with "Try Again" button

**Step 3: Collection picker**
- After successful clone, call `scan_collections_in_path` to discover collections
- **One collection found:** Auto-select, show name, "Open" button
- **Multiple collections found:** List with radio buttons to pick one, then "Open"
- **No collections found:** Message "No collections found in this repository." with "Close" button
- "Open" triggers existing collection-open flow and closes the dialog

### Backend addition

A new Tauri command for collection discovery:

```
scan_collections_in_path(path: String) -> Vec<CollectionInfo>
```

This scans a directory for `.yml` collection manifest files only (no `.json`). The logic likely exists in `rocket-collection` already — the plan will verify the exact function and wire it as a Tauri command.

### `CollectionInfo` type

```typescript
export interface CollectionInfo {
  name: string;
  path: string;
}
```

---

## Phase 4: Git Tab Layout Redesign (Bruno-style)

Restructure `WorkspaceGitTab` from its current vertical scrolling panel into Bruno's two-panel layout, using RocketAPI's existing shadcn/ui design system, Lucide icons, and color tokens.

**Reference:** Bruno Git UI screenshot (user-provided) — the approved target layout.

### Layout structure

Two-panel split: fixed-width left panel (~320px) + flexible right panel.

**Left panel** (top to bottom, single scrollable column):

1. **Collection name row** — icon + collection name, borderless header
2. **"Changes" collapsible section** (expanded by default):
   - Commit message `Input` (shadcn)
   - "Commit Changes" `Button` (shadcn, primary variant) with check icon
   - "Unstaged Changes" sub-header with count + discard-all icon + stage-all icon
   - File list: each row shows file path + status badge letter (M/A/D/U/R) with color coding:
     - M (modified) = amber/warning
     - A (added) = green/success
     - D (deleted) = red/destructive
     - U (untracked) = amber/warning
     - R (renamed) = blue/info
     - C (conflicted) = red/destructive
   - "Staged Changes" sub-header (only visible when staged files exist) with count + unstage-all icon
   - Staged file list (same row format)
3. **Spacer** — pushes Links to the bottom
4. **"Links" collapsible section** (pinned at bottom):
   - Commits — navigates right panel to commit log view
   - Stashes — navigates right panel to stash management view
   - Remotes — opens `GitRemotesDialog`

**Right panel** — context-dependent content:

- **Default/landing state** (no file selected): centered layout with:
  - Git branch icon (large, muted)
  - Helper text: "Perform git actions or open files from sidebar to view"
  - Fetch / Pull / Push `Button` group (shadcn, outline variant) with Lucide icons
  - "Last fetched: {time}" with clock icon
  - "↑ N Ahead | ↓ N Behind" from `RepoStatus`
  - Branch status badge: "Your branch is up to date" (success) or "N commits behind" (warning)
- **File selected** (click a file in left panel): Monaco diff viewer (reuse existing `DiffViewer`)
- **Commits** (click "Commits" in Links): reuse existing `GitCommitLog`
- **Stashes** (click "Stashes" in Links): reuse existing `GitStashSection`

### What changes from current implementation

- **Remove** the `Tabs`/`TabsList`/`TabsTrigger`/`TabsContent` sub-tab layout (Changes/Log/Stash tabs)
- **Remove** the current header bar with `BranchSelector` + `GitRemoteActions` inline (these move to the right panel landing state and top toolbar respectively)
- **Add** the two-panel grid layout
- **Add** the right panel landing state component
- **Add** the Links section at the bottom of the left panel
- **Add** file status color-coded badges (letter + color)
- **Move** commit form above the file list (currently below)
- **Refactor** existing components to fit new layout (they mostly get reused, just repositioned)

### New components needed

- `GitLandingPanel` — right panel default state (Fetch/Pull/Push, ahead/behind, branch status)
- `GitLinksSection` — collapsible Links list at bottom of left panel
- `GitFileList` — unified file list with staged/unstaged sections and status badges (replaces `GitStagedFiles` + `GitChangedFiles` or wraps them)

### Components reused as-is

- `DiffViewer` — shown in right panel when file selected
- `GitCommitLog` — shown in right panel when Commits clicked
- `GitStashSection` — shown in right panel when Stashes clicked
- `GitRemotesDialog` — opened when Remotes clicked in Links
- `GitCredentialsDialog` — still triggered by push/pull/fetch when no credentials
- `BranchSelector` — moves to the top toolbar area (already exists there via `GitToolbarButton`)

### Design constraints

- All UI uses shadcn/ui primitives (Button, Input, ScrollArea, Collapsible, Separator, Tooltip, etc.)
- Lucide React icons only
- Existing color tokens from the RocketAPI theme — no custom colors
- No raw HTML interactive elements
- Collapsible sections use shadcn `Collapsible` component

---

## Reference

- **Bruno Git UI docs:** https://docs.usebruno.com/git-integration/using-gui/provider
- **Existing frontend components:** `src/components/git/`
- **Existing store:** `src/stores/git-store.ts`
- **Existing Tauri API:** `src/lib/tauri-api.ts`
- **Git service trait:** `crates/rocket-git/src/service.rs`
- **Git2 implementation:** `crates/rocket-git/src/git2_service.rs`
- **App service:** `crates/rocket-app/src/git_service.rs`
- **Domain events:** `crates/rocket-shared/src/events.rs`
