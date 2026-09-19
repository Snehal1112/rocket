# Git integration user-journey review

**Date:** 2026-09-19  
**Scope:** Git discoverability and workflows from workspace/collection selection through repository setup, status, staging, diff, commit/identity, branches, network operations/credentials, remotes, stash, conflicts, and completion/error feedback.

## Method and evidence standard

This is a static, end-to-end code trace. The review covered:

- `src/components/git/*`
- `src/components/layout/GitToolbarButton.tsx`
- `src/components/layout/CollectionDropdown.tsx`
- `src/components/workspace/WorkspaceGitTab.tsx`
- pane/tab integration in `src/components/panes`, `src/stores/pane-store.ts`, and `src/types/pane-types.ts`
- `src/stores/git-store.ts`
- Git and workspace wrappers in `src/lib/tauri-api.ts` and `src/lib/queries/workspace-queries.ts`
- relevant Tauri commands and `rocket-git` implementations needed to verify user-visible behavior

No production code was changed. No unsupported screens are assumed: where no progress, confirmation, success state, selector, or recovery control exists in the reviewed code, the report says so explicitly.

### Severity scale

- **Critical:** credible wrong-repository mutation, unrecoverable data loss, or a workflow that can continue after a failed safety step.
- **High:** core journey is blocked, reports false success, loses user input, or lacks a necessary recovery path.
- **Medium:** materially confusing, inaccessible, or inconsistent but with bounded immediate impact.
- **Low:** polish or consistency issue with limited workflow impact.

## Executive summary

The Git integration exposes most expected operations, but the complete journey is not safe or reliably completable yet. The highest-risk problems are:

1. A singleton Git store can bind a visible panel to another panel's repository, and the collection toolbar can reuse a stale path from a previously selected collection.
2. Clone can start twice, cannot open detected standalone collections, and can remain permanently on a progress screen after credential cancellation.
3. Store actions commonly catch failures and resolve normally. Callers then clear commit input, show fetch completion, continue from failed stash/fetch safety steps, and close remote editors as if the action succeeded.
4. Destructive actions are under-protected: individual discard permanently deletes untracked files without confirmation, remote checkout uses a forced checkout without a dirty-tree preflight, branch and stash deletion lack confirmation, and abort merge hard-resets the working tree without confirmation.
5. Conflict resolution remains interactive after success. Repeating an action can overwrite a resolved file with empty content; changing conflict files while in manual mode can apply the previous file's content to the newly selected file.
6. Collection and workspace Git use the same panel but different roots while the UI consistently calls both a “collection.” This hides the scope of status, commits, discard, branch, remote, and stash operations.

### Prioritized finding index

| ID | Severity | Finding |
|---|---:|---|
| UJ-01 | Critical | Repository scope can silently switch to another collection/workspace |
| UJ-02 | Critical | Clone can start twice against the same destination |
| UJ-03 | High | Standalone or multi-collection clones cannot be opened by the clone completion flow |
| UJ-04 | High | Credential cancellation and failed clone authentication create dead ends |
| UJ-05 | Critical | Failed operations are treated as success; safety chains continue after failure |
| UJ-06 | Critical | Individual discard permanently destroys untracked files without confirmation |
| UJ-07 | Critical | Remote-branch checkout can overwrite local changes without warning |
| UJ-08 | Critical | Conflict resolver remains armed after success and can overwrite the wrong content |
| UJ-09 | Critical | Abort Merge hard-resets local work without confirmation |
| UJ-10 | High | Freshly initialized repositories cannot unstage initial files |
| UJ-11 | High | Branch deletion and stash dropping are insufficiently protected |
| UJ-12 | High | Remote choice is implicit; no-remote and multi-remote states are not actionable |
| UJ-13 | High | Non-repository and initialization errors are presented as “not a repository” or no-op |
| UJ-14 | High | Commit failures clear the message and may be invisible |
| UJ-15 | High | Remote management closes/clears controls even when backend changes fail |
| UJ-16 | High | Credential persistence failure is hidden and credential fields accept empty values |
| UJ-17 | Medium | Collection-vs-workspace scope and entry semantics are inconsistent |
| UJ-18 | Medium | Status and completion language can be false or ambiguous |
| UJ-19 | Medium | Error visibility depends on the selected right-hand view |
| UJ-20 | Medium | Git workflows contain material keyboard and screen-reader barriers |
| UJ-21 | Medium | Test coverage does not exercise the critical user journeys |

---

## Verified journey map

### 1. Selecting scope and discovering Git

#### Collection mode

1. The top-left dropdown lists the active workspace and collections (`src/components/layout/CollectionDropdown.tsx:47-149`).
2. Selecting a collection switches pane state and proactively calls `useGitStore.getState().setCollection(summary.path)` (`src/components/layout/CollectionDropdown.tsx:39-43`).
3. The toolbar Git icon is enabled only while `activeCollection` is truthy (`src/components/layout/GitToolbarButton.tsx:63-76`).
4. The button and `Cmd/Ctrl+Shift+G` shortcut call `openGitPanel()` (`src/components/layout/GitToolbarButton.tsx:23-61`; `src/hooks/useKeyboardShortcuts.ts:75-81`).
5. A `GitTab` is opened with the collection name and a filesystem path (`src/components/layout/GitToolbarButton.tsx:39-60`; `src/types/pane-types.ts:77-81`).
6. `EditorGroup` renders `GitPanel` from those tab values (`src/components/panes/EditorGroup.tsx:184-190`).

#### Workspace mode

1. Clicking the workspace row opens four workspace tabs: Overview, Environments, Git UI, and Audit Log (`src/components/layout/CollectionDropdown.tsx:65-84`; `src/stores/pane-store.ts:598-657`).
2. The workspace Git tab resolves the workspace path and passes it to the same `GitPanel` used by collection tabs (`src/components/workspace/WorkspaceGitTab.tsx:9-25`).
3. Workspace mode clears `activeCollection` (`src/stores/pane-store.ts:658-663`), so the global toolbar Git button is disabled even while a workspace Git tab is available (`src/components/layout/GitToolbarButton.tsx:63-76`).

There is no repository-scope selector inside `GitPanel`. The scope is whichever path the entry route supplied.

### 2. Initial loading and non-repository state

1. `GitPanel` calls the singleton store's `setCollection(path)` and reads `isRepo` back from global state (`src/components/git/GitPanel.tsx:71-94`).
2. While the check runs, an aria-hidden skeleton is rendered (`src/components/git/GitPanel.tsx:173-175`; `src/components/git/GitPanelSkeleton.tsx:1-4`).
3. A non-repository state offers exactly two actions: **Initialize Git** and **Clone Repository** (`src/components/git/GitPanel.tsx:177-207`).
4. Initialize invokes `git_init`, then reloads the path (`src/stores/git-store.ts:693-700`; `src/lib/tauri-api.ts:715-721`).
5. Clone opens a URL/destination dialog and requires credentials before `git_clone` (`src/components/git/GitCloneDialog.tsx:120-135`, `223-260`).

There is no loading state on Initialize, no inline initialization error, and no explicit credential control in the non-repository screen.

### 3. Repository status and navigation

For a recognized repository, the left panel contains:

- supplied collection/workspace name and branch selector (`src/components/git/GitPanel.tsx:220-225`)
- merge-conflict banner when status contains conflicted files (`src/components/git/GitPanel.tsx:227-235`)
- commit form (`src/components/git/GitPanel.tsx:237-240`)
- staged and unstaged file lists (`src/components/git/GitPanel.tsx:242-246`)
- links to commit history, stashes, and remotes (`src/components/git/GitPanel.tsx:248-254`; `src/components/git/GitLinksSection.tsx:9-40`)

The right panel contains the overview/sync card, a file diff, a conflict resolver, commit history, commit diff, or stash manager (`src/components/git/GitPanel.tsx:284-340`).

Status is initially loaded in parallel with branches, remotes, and stashes (`src/stores/git-store.ts:133-163`). File-system collection events refresh status after a 300 ms debounce (`src/components/git/GitPanel.tsx:139-157`).

### 4. Staging, unstaging, discard, and diff

- Staged files have per-row Unstage and section-level Unstage All controls (`src/components/git/GitFileList.tsx:79-140`).
- Unstaged files have per-row Discard and Stage controls plus Discard All and Stage All (`src/components/git/GitFileList.tsx:144-255`).
- Discard All has a confirmation dialog (`src/components/git/GitFileList.tsx:259-274`). Individual Discard does not (`src/components/git/GitFileList.tsx:217-234`).
- Selecting a non-conflicted file loads either working-tree or staged content based on the selected row (`src/components/git/DiffViewForFile.tsx:19-50`).
- The diff supports text and visual views for `.yml` files, and a Working/Staged content toggle (`src/components/git/DiffViewer.tsx:60-105`; `src/components/git/DiffHeader.tsx:29-55`). The toggle changes which diff is displayed; it does not stage or unstage the file.

Backend discard restores tracked files from `HEAD` but recursively deletes untracked directories and deletes untracked files (`crates/rocket-git/src/git2_service/staging.rs:58-89`).

### 5. Commit and identity

1. Commit is enabled only with a non-empty message and at least one staged status entry (`src/components/git/GitCommitForm.tsx:15-29`, `84-101`).
2. Before commit, the UI reads repository/global identity. Missing identity opens `GitIdentityDialog` (`src/components/git/GitCommitForm.tsx:31-45`).
3. Identity is saved into repository-local Git config, then the original commit is attempted (`src/components/git/GitCommitForm.tsx:48-58`; `src-tauri/src/commands/git.rs:344-371`).
4. A successful store commit refreshes status and commit history (`src/stores/git-store.ts:300-310`).

There is no dedicated identity settings entry in the Git panel. Identity is discoverable only when commit detects it is missing or when SSH-key credentials trigger the additional identity dialog (`src/stores/git-store.ts:552-609`).

### 6. Branch operations

The branch popover supports:

- search and local branch switch (`src/components/git/BranchSelector.tsx:119-153`)
- merge into current and delete on non-current local branches (`src/components/git/BranchSelector.tsx:154-191`)
- checkout of remote branches that do not have a same-named local branch (`src/components/git/BranchSelector.tsx:194-214`)
- create-and-switch (`src/components/git/BranchSelector.tsx:217-247`)

Local branch switch rejects tracked staged/unstaged changes (`crates/rocket-git/src/git2_service/branch.rs:52-105`). Remote checkout does not use that preflight and performs a forced checkout (`crates/rocket-git/src/git2_service/branch.rs:108-152`). Branch delete calls libgit2 delete directly (`crates/rocket-git/src/git2_service/branch.rs:181-190`).

### 7. Fetch, pull, push, credentials, and remotes

- The overview card offers Fetch, Pull, Push, and credential settings (`src/components/git/GitLandingPanel.tsx:180-289`).
- If credentials are absent, the store opens a shared credential dialog and records a pending network operation for automatic retry (`src/stores/git-store.ts:611-689`).
- Credential types are SSH agent, SSH key, username/password, and token (`src/components/git/GitCredentialsDialog.tsx:147-285`). Credentials are persisted under active workspace ID and then activated in memory (`src/components/git/GitCredentialsDialog.tsx:105-130`; `src-tauri/src/commands/git.rs:314-341`).
- Pull with a dirty tree offers Cancel, Pull Anyway, or Stash & Pull (`src/components/git/GitLandingPanel.tsx:69-123`, `335-351`).
- Push recommends fetch first when the panel has not fetched during its current mount or status reports behind (`src/components/git/GitLandingPanel.tsx:125-173`, `353-370`).
- Remotes can be listed, added, URL-edited, and removed (`src/components/git/GitRemotesDialog.tsx:15-53`, `55-205`).

There is no network-remote selector. Store actions default to `remotes[0]` (`src/stores/git-store.ts:619-622`, `645-648`, `674-677`).

### 8. Stash

The stash view supports:

- saving with a required message (`src/components/git/GitStashSection.tsx:62-75`, `136-156`)
- per-entry Pop, Apply, and Drop (`src/components/git/GitStashSection.tsx:281-323`)
- hover-revealed multi-selection and batch Apply, Pop, and Drop (`src/components/git/GitStashSection.tsx:178-205`, `329-371`)
- inline display of the global Git error (`src/components/git/GitStashSection.tsx:158-164`)

Stash save includes untracked files (`crates/rocket-git/src/git2_service/stash.rs:93-107`). Drop is irreversible and has no confirmation in either single or batch flows.

### 9. Conflicts and merge completion

1. A conflicted status row refreshes conflict payloads and opens the resolver (`src/components/git/GitFileList.tsx:64-73`, `181-253`).
2. The resolver offers Accept Ours, Accept Theirs, manual editing, and Abort Merge (`src/components/git/ConflictResolver.tsx:15-31`, `93-167`).
3. Resolving writes the chosen content and stages the file (`crates/rocket-git/src/git2_service/conflict.rs:62-156`).
4. Abort Merge hard-resets index and working tree to `HEAD`, then cleans merge state (`crates/rocket-git/src/git2_service/conflict.rs:159-181`).
5. After all conflict rows disappear, merge completion depends on the user entering a commit message in the normal commit form. There is no explicit “all conflicts resolved,” “continue merge,” or default merge-message state.

### 10. Completion and error feedback

- Initial repository loading has a skeleton (`src/components/git/GitPanel.tsx:173-175`).
- Fetch/pull/push have local spinners only when initiated with credentials already present (`src/components/git/GitLandingPanel.tsx:53-173`, `245-289`).
- The overview card, stash view, and conflict resolver render the shared store error (`src/components/git/GitLandingPanel.tsx:291-305`; `src/components/git/GitStashSection.tsx:158-164`; `src/components/git/ConflictResolver.tsx:67-80`, `111-124`).
- The file list, commit form, branch delete action, remotes dialog, non-repository state, commit log, and normal diff do not provide a general error surface.
- There is no general success toast, operation log, or persistent completion announcement for init, stage, unstage, discard, commit, branch, remotes, stash, conflict resolution, or network operations.

---

## Findings

### UJ-01 — Repository scope can silently switch to another collection/workspace

**Severity:** Critical  
**Confidence:** High

**Evidence**

- One global store owns `collectionPath`, status, branches, remotes, stashes, credentials, errors, and every mutation action: `src/stores/git-store.ts:46-107`, `109-164`.
- Every mounted `GitPanel` initializes that same store from its own prop: `src/components/git/GitPanel.tsx:42-94`.
- Both collection Git tabs and workspace Git tabs mount `GitPanel`: `src/components/panes/EditorGroup.tsx:184-208`.
- Pane state supports path-bearing Git tabs and split leaves: `src/types/pane-types.ts:1-19`, `77-81`.
- Store mutations use the current global `collectionPath`, not the invoking panel's prop; examples include stage/discard/commit at `src/stores/git-store.ts:264-310` and branch operations at `src/stores/git-store.ts:453-514`.
- The toolbar takes any already-loaded Git store path before looking up the active collection: `src/components/layout/GitToolbarButton.tsx:24-37`.
- Breadcrumb collection switching calls only `switchCollection`, not `setCollection`: `src/components/panes/BreadcrumbBar.tsx:140-152`. The toolbar can therefore reuse the previous collection's Git path while naming the tab after the new collection (`src/components/layout/GitToolbarButton.tsx:52-58`).
- `setCollection` has no request token/path guard, and its nested refresh methods re-read mutable global state: `src/stores/git-store.ts:133-163`, `166-262`.

**User impact**

A panel labeled for collection/workspace A can display or mutate repository B. In split-pane or rapid-navigation cases, status from one path can be combined with branches/remotes/stashes from another. Stage, discard, commit, branch, stash, remote, and conflict operations can target whichever path last won the global-store race. This is a direct wrong-repository and data-loss risk.

**Acceptance criteria**

- Every Git action is bound to an explicit immutable repository identity/path owned by the panel that invoked it.
- Two simultaneously mounted Git panels for different roots display independent status, branches, remotes, stashes, credentials state, errors, and pending operations.
- Async responses for an old root cannot update a newer root's state.
- Opening Git after switching collections through the dropdown, breadcrumb, sidebar, restored UI state, or keyboard shortcut always resolves the active collection's current path; it never reuses an unverified store path.
- Automated tests interleave A/B initialization and mutations and assert every IPC call and rendered state remains correctly scoped.

### UJ-02 — Clone can start twice against the same destination

**Severity:** Critical  
**Confidence:** High

**Evidence**

- `handleClone` sets `step` to `progress` and directly calls `gitClone` when credentials already exist: `src/components/git/GitCloneDialog.tsx:120-135`.
- A separate effect calls `gitClone` whenever `step === 'progress'` and credentials exist: `src/components/git/GitCloneDialog.tsx:97-111`.

**User impact**

With credentials already loaded, one click can launch two concurrent clone operations into the same destination. Users can receive an inconsistent failure after an apparent success, or be left with a partial/corrupt destination.

**Acceptance criteria**

- One Clone activation creates exactly one operation ID and one `git_clone` IPC call.
- Credential acquisition resumes that same operation rather than starting a second owner/effect path.
- Clone controls cannot submit again while an operation is active.
- Closing/reopening the dialog cannot let stale clone completion update the new dialog session.
- A component test covers credentials-present and credentials-requested cases and asserts one clone call in each.

### UJ-03 — Standalone or multi-collection clones cannot be opened by the clone completion flow

**Severity:** High  
**Confidence:** High

**Evidence**

- Detection explicitly returns `kind: "collection"` when `opencollection.yml` exists at clone root and `kind: "multi_collection"` for collection directories: `src-tauri/src/commands/collections.rs:233-304`.
- The clone dialog sends both a detected standalone collection path and a selected multi-collection path to `handleOpenWorkspace`: `src/components/git/GitCloneDialog.tsx:55-95`, `137-139`, `176-215`.
- `handleOpenWorkspace` calls `openWorkspaceFromDisk`: `src/components/git/GitCloneDialog.tsx:55-66`; `src/lib/queries/workspace-queries.ts:136-141`.
- The backend open-workspace operation requires `workspace.yml` and rejects paths without it: `crates/rocket-app/src/workspace_service.rs:203-210`.

**User impact**

The dialog says “Repository cloned successfully” and offers Open, but standalone collection repositories and selected collections from multi-collection repositories deterministically fail with `workspace.yml not found`. Only a clone already structured as a Rocket workspace can complete the open/switch journey.

**Acceptance criteria**

- Post-clone handling has distinct, supported actions for workspace roots, standalone collection roots, and multi-collection roots.
- A standalone collection can be registered/imported into a workspace or opened through a supported collection path without calling the workspace-only API.
- A multi-collection result explains whether the repository root or selected collection becomes the managed scope.
- Successful Open switches the application to the newly registered content and opens the correct Git root.
- Integration tests cover all four detection kinds: workspace, collection, multi-collection, and unknown.

### UJ-04 — Credential cancellation and failed clone authentication create dead ends

**Severity:** High  
**Confidence:** High

**Evidence**

- Clone enters `progress` before checking credentials: `src/components/git/GitCloneDialog.tsx:120-126`.
- If credentials are dismissed, `setShowCredentialsDialog(false)` clears only the pending network operation; clone step is not notified or reset: `src/stores/git-store.ts:588-592`.
- The clone progress screen has only a spinner/message and no retry, back, cancellation, or credential control: `src/components/git/GitCloneDialog.tsx:141-155`.
- After clone authentication failure, the dialog returns to input while retaining the same in-memory credentials: `src/components/git/GitCloneDialog.tsx:128-134`. The input screen has no Change Credentials action (`src/components/git/GitCloneDialog.tsx:223-260`), and the surrounding non-repository state has no explicit credential button (`src/components/git/GitPanel.tsx:177-207`).

**User impact**

Canceling credential entry leaves “Cloning repository...” indefinitely even though no clone is running. If stored credentials are wrong, retry repeats with the same credentials and the user has no discoverable way to replace them from the clone/non-repo journey.

**Acceptance criteria**

- Canceling credentials returns clone to the input step with an explanatory, focus-managed message and no operation in flight.
- Clone progress exposes a supported cancellation/back behavior, or the dialog is intentionally non-dismissible while a real operation is active.
- Authentication errors provide a Change Credentials action in context.
- Retry can replace credentials before invoking clone again.
- Progress is driven by an actual operation state, not used as a credential-retry trigger.

### UJ-05 — Failed operations are treated as success; safety chains continue after failure

**Severity:** Critical  
**Confidence:** High

**Evidence**

- Store actions catch errors, set global `error`, and resolve instead of rejecting or returning a result; examples: stash (`src/stores/git-store.ts:343-390`), commit (`src/stores/git-store.ts:300-310`), and network operations (`src/stores/git-store.ts:611-689`).
- Fetch always sets `lastFetched` after awaiting the resolving store action: `src/components/git/GitLandingPanel.tsx:53-67`.
- Pull likewise sets `lastFetched` after a failed store action: `src/components/git/GitLandingPanel.tsx:69-90`, `114-123`.
- Stash & Pull continues to `pull()` after `saveStash()` even when stash failed: `src/components/git/GitLandingPanel.tsx:92-110`.
- Fetch & Push continues to status checking/push after `fetch()` failed: `src/components/git/GitLandingPanel.tsx:147-163`.
- The backend fast-forward pull uses forced checkout: `crates/rocket-git/src/git2_service/remote.rs:205-230`.
- The outer catch in Stash & Pull cannot catch a failed `popStash`, because `popStash` catches internally: `src/components/git/GitLandingPanel.tsx:105-109`; `src/stores/git-store.ts:356-366`.

**User impact**

The UI can claim a fetch time after authentication/network failure. More seriously, “Stash & Pull” can pull after the protective stash failed, defeating the safety choice and exposing local work to a forced checkout. “Fetch & Push” can push after its safety fetch failed. These flows communicate that prerequisites succeeded when they did not.

**Acceptance criteria**

- Every async action returns a typed success/error result or rejects; callers can reliably stop a sequence on failure.
- Stash & Pull does not invoke pull unless stash creation is confirmed and the created stash is identified.
- Fetch & Push does not invoke push unless fetch succeeds and fresh ahead/behind state has been loaded.
- `lastFetched` changes only after a successful fetch (including the fetch phase of pull) and is not updated on auth, transport, remote, or merge errors unless explicitly labeled “remote contacted.”
- Commit, remotes, branch, stash, conflict, and network handlers only clear/close controls after confirmed success.
- Tests force each first step to fail and assert the subsequent IPC call is not made.

### UJ-06 — Individual discard permanently destroys untracked files without confirmation

**Severity:** Critical  
**Confidence:** High

**Evidence**

- The per-file trash action immediately calls `discardFiles([file.path])`: `src/components/git/GitFileList.tsx:217-234`.
- Only Discard All uses a confirmation dialog: `src/components/git/GitFileList.tsx:259-274`.
- Backend discard force-restores tracked content and permanently deletes untracked files/directories: `crates/rocket-git/src/git2_service/staging.rs:58-89`.

**User impact**

A single small hover-only icon can permanently delete a newly created request, folder, or other untracked content with no confirmation and no undo/trash recovery. The less obvious “untracked means delete” behavior is not disclosed.

**Acceptance criteria**

- Every discard action requires confirmation unless a genuine recoverable undo mechanism exists.
- Confirmation names the file and clearly differentiates “restore tracked content” from “permanently delete untracked file/directory.”
- The destructive action uses destructive styling and receives initial focus only according to safe dialog conventions (Cancel remains the safe default).
- The action is disabled while running and reports success/failure in context.
- Tests verify cancel performs no IPC and confirm performs exactly one discard.

### UJ-07 — Remote-branch checkout can overwrite local changes without warning

**Severity:** Critical  
**Confidence:** High

**Evidence**

- Clicking a remote branch immediately calls checkout: `src/components/git/BranchSelector.tsx:194-211`.
- Local switching has a dirty tracked-file preflight and safe checkout: `crates/rocket-git/src/git2_service/branch.rs:52-105`.
- Remote checkout lacks that preflight and uses `CheckoutBuilder::force()`: `crates/rocket-git/src/git2_service/branch.rs:108-152`.

**User impact**

Choosing a remote branch can overwrite tracked working-tree changes without the warning users receive for local branches. The UI presents local and remote selections as equivalent branch navigation even though their safety behavior differs.

**Acceptance criteria**

- Remote checkout uses the same dirty-tree preflight and safe checkout guarantees as local switching.
- If checkout would overwrite any tracked or untracked path, it is blocked with an actionable commit/stash/discard choice; it never silently forces.
- The UI shows an in-progress state and keeps the popover open on failure.
- Tests cover dirty tracked files, staged files, and untracked path collisions.

### UJ-08 — Conflict resolver remains armed after success and can overwrite the wrong content

**Severity:** Critical  
**Confidence:** High

**Evidence**

- Resolve only awaits the store action; it does not disable controls, navigate away, advance to the next conflict, or mark this resolver completed: `src/components/git/ConflictResolver.tsx:25-31`, `157-166`.
- Store resolution refreshes status/conflicts but returns no result: `src/stores/git-store.ts:190-200`.
- The parent right-panel state remains `kind: 'conflict'`; only branch changes reset it: `src/components/git/GitPanel.tsx:159-171`, `316-328`.
- Backend Ours/Theirs resolution defaults to an empty string if the file is no longer found among conflicts, then writes and stages that empty content: `crates/rocket-git/src/git2_service/conflict.rs:70-138`, `140-154`.
- Manual content is initialized only once from `conflictState.ours`: `src/components/git/ConflictResolver.tsx:15-18`. There is no effect/key to reset `manualMode` or `manualContent` when another conflict file is selected.

**User impact**

After a successful Accept Ours/Theirs, the same enabled button remains visible. Pressing it again can write an empty file because the conflict no longer exists. While in manual mode, selecting another conflict can retain the previous file's manual content and save it into the newly selected file. Both paths can silently corrupt collection files.

**Acceptance criteria**

- Resolve validates that the target is still an unresolved index conflict before any write; a non-conflict target performs no filesystem mutation.
- Resolution controls disable during the operation and cannot be submitted twice.
- On success, the UI advances to the next unresolved conflict or exits to a clear completion state.
- Changing `conflictState.filePath` resets manual mode/content from the new conflict, or the resolver is keyed by repository + path.
- Ours/Theirs correctly handles add/delete conflicts rather than converting a missing side into an empty file unless that is an explicitly confirmed resolution.
- Tests cover double-submit, switching files in manual mode, deleted-side conflicts, and stale conflict payloads.

### UJ-09 — Abort Merge hard-resets local work without confirmation

**Severity:** Critical  
**Confidence:** High

**Evidence**

- Abort Merge invokes `abortMerge()` immediately from both conflict modes: `src/components/git/ConflictResolver.tsx:21-23`, `42-49`, `100-108`.
- No alert/confirmation dialog wraps either action in the resolver: `src/components/git/ConflictResolver.tsx:33-170`.
- Backend implementation performs `ResetType::Hard` on index and working directory: `crates/rocket-git/src/git2_service/conflict.rs:159-181`.

**User impact**

A user can lose conflict resolutions and any other tracked staged/unstaged work since `HEAD` with one click. “Abort Merge” does not explain that implementation is a hard reset or inventory what will be discarded.

**Acceptance criteria**

- Abort requires a destructive confirmation that explains exactly which staged/unstaged tracked changes will be lost.
- Before confirmation, the application computes and displays the affected file count/list.
- Untracked files are explicitly characterized as retained or removed according to actual implementation.
- The operation is disabled while running and success exits the resolver; failure keeps the resolver with an inline alert.
- Backend behavior matches Git's expected merge-abort preservation semantics where possible, rather than using an unconditional hard reset.

### UJ-10 — Freshly initialized repositories cannot unstage initial files

**Severity:** High  
**Confidence:** High

**Evidence**

- Initialize creates an empty repository and reloads it: `src/stores/git-store.ts:693-700`; `crates/rocket-git/src/git2_service/repo.rs:15-19`.
- The UI permits Stage and then Unstage/Unstage All in the normal file list: `src/components/git/GitFileList.tsx:79-140`, `144-169`.
- Backend unstage requires `HEAD` to peel to a commit: `crates/rocket-git/src/git2_service/staging.rs:45-55`.
- A newly initialized repository has no `HEAD` commit.
- File-list actions do not render the store error (`src/components/git/GitFileList.tsx:25-277`).

**User impact**

During the initial-commit journey, a user can stage files but cannot unstage them. The click appears to do nothing unless they happen to navigate to a right-hand view that renders the global error.

**Acceptance criteria**

- Unstage works in an unborn repository by removing selected paths from the index without requiring `HEAD`.
- Unstage All restores the initial-repository state without deleting working-tree files.
- Failures appear next to the file-list action that failed.
- Integration tests cover init → stage → unstage → stage → initial commit.

### UJ-11 — Branch deletion and stash dropping are insufficiently protected

**Severity:** High  
**Confidence:** High

**Evidence**

- Branch trash immediately invokes `deleteBranch` with no confirmation: `src/components/git/BranchSelector.tsx:173-188`.
- Backend calls branch delete directly and performs no merged/unmerged policy check: `crates/rocket-git/src/git2_service/branch.rs:181-190`.
- Single stash Drop is immediate: `src/components/git/GitStashSection.tsx:301-320`.
- Batch Drop is immediate: `src/components/git/GitStashSection.tsx:110-119`, `353-361`.
- Backend stash drop permanently removes the entry: `crates/rocket-git/src/git2_service/stash.rs:128-134`.
- Batch pop/drop stops after partial progress and leaves an error while stash indices may have renumbered: `src/stores/git-store.ts:414-451`; selection remains when `error` is present (`src/components/git/GitStashSection.tsx:99-118`).

**User impact**

Users can irreversibly delete an unmerged branch or one/many stashes through compact hover actions without reviewing the target. After partial batch mutation, retained selections refer to numeric stash indices that may no longer identify the same stash, making a retry hazardous.

**Acceptance criteria**

- Branch deletion requires confirmation and identifies whether the branch is fully merged; unmerged deletion requires a stronger explicit choice.
- Single and batch stash Drop require confirmation listing messages/identities, not only mutable numeric indices.
- Batch operations track stable stash OIDs/identities and reconcile selection after every mutation or partial failure.
- Partial success reports exactly which entries changed and clears/remaps stale selections before retry is possible.
- Destructive actions are disabled while active and announce completion.

### UJ-12 — Remote choice is implicit; no-remote and multi-remote states are not actionable

**Severity:** High  
**Confidence:** High

**Evidence**

- Fetch/Pull/Push buttons remain available regardless of remote count: `src/components/git/GitLandingPanel.tsx:245-289`.
- Store operations silently choose `remotes[0]?.name`: `src/stores/git-store.ts:619-622`, `645-648`, `674-677`.
- IPC wrappers and Tauri commands require a string remote: `src/lib/tauri-api.ts:750-757`; `src-tauri/src/commands/git.rs:80-93`.
- Remotes management is a separate left-panel link, not an inline prerequisite or target selector: `src/components/git/GitLinksSection.tsx:32-40`.
- The overview does not name the target remote anywhere: `src/components/git/GitLandingPanel.tsx:180-333`.

**User impact**

With no remote, network buttons lead to credential prompts and then backend argument/remote errors instead of guiding the user to Add Remote. With multiple remotes, users cannot see or choose which one will receive a push or supply fetch/pull data; array ordering silently determines the target.

**Acceptance criteria**

- No-remote state disables network actions and presents a direct Add Remote action.
- One-remote state names the target in the overview and confirmations.
- Multi-remote state provides a persistent or per-action remote selector with a clear default.
- Push shows both remote and destination branch before execution, especially when upstream and local names differ.
- Store/API actions reject an absent remote before IPC and return an actionable typed error.

### UJ-13 — Non-repository and initialization errors are presented as “not a repository” or no-op

**Severity:** High  
**Confidence:** High

**Evidence**

- `setCollection` catches `gitIsRepo`/load failures, stores `error`, and resolves: `src/stores/git-store.ts:133-164`.
- `GitPanel.checkAndLoad` therefore reads the default/previous `isRepo`; its own catch is normally unreachable for store-caught failures: `src/components/git/GitPanel.tsx:74-85`.
- Any false result renders “This collection is not a Git repository,” even for workspace scope or I/O/permission/load errors: `src/components/git/GitPanel.tsx:177-208`.
- The non-repository screen does not render `error`: `src/components/git/GitPanel.tsx:177-208`.
- Initialize has no busy flag/disabled state and reads global `isRepo` after an action that also swallows failure: `src/components/git/GitPanel.tsx:182-191`; `src/stores/git-store.ts:693-700`.

**User impact**

A real repository that cannot be read can be mislabeled as non-repository, encouraging initialization in the wrong place. Failed initialization leaves the screen unchanged with no explanation and permits repeated clicks.

**Acceptance criteria**

- Loading distinguishes `checking`, `repo`, `notRepo`, and `loadError` states.
- Permission, corrupt repository, missing path, and IPC errors render actionable messages and never offer initialization as if absence were confirmed.
- Workspace scope says workspace/repository, not collection.
- Initialize has an in-progress state, submits once, and reports success/failure inline.
- The panel's repository determination is scoped to its own path, not read back from unrelated global state.

### UJ-14 — Commit failures clear the message and may be invisible

**Severity:** High  
**Confidence:** High

**Evidence**

- `doCommit` clears the message after awaiting `commitChanges`: `src/components/git/GitCommitForm.tsx:17-24`.
- `commitChanges` catches failure and resolves: `src/stores/git-store.ts:300-310`.
- The commit form renders no error or success message: `src/components/git/GitCommitForm.tsx:64-104`.
- The shared error appears only in selected right-hand views, not universally: `src/components/git/GitLandingPanel.tsx:291-305`; `src/components/git/GitPanel.tsx:310-340`.

**User impact**

A failed commit—missing identity race, filesystem/config error, invalid repository state, or backend error—can erase the user's commit message. If a diff or commit history is open, there may be no visible explanation.

**Acceptance criteria**

- Commit input clears only after a confirmed commit and keeps the exact message on failure.
- Failure appears adjacent to the commit form and receives an accessible alert announcement.
- Success provides a concise confirmation including short SHA/message or an equivalent observable state transition.
- Commit cannot submit twice while active.
- Tests assert message retention on backend rejection.

### UJ-15 — Remote management closes/clears controls even when backend changes fail

**Severity:** High  
**Confidence:** High

**Evidence**

- Add always clears name/URL after awaiting `addRemote`: `src/components/git/GitRemotesDialog.tsx:37-41`.
- Edit always exits edit mode after awaiting `setRemoteUrl`: `src/components/git/GitRemotesDialog.tsx:43-47`.
- Remove always exits confirmation after awaiting `removeRemote`: `src/components/git/GitRemotesDialog.tsx:49-53`.
- Store methods catch and resolve failures: `src/stores/git-store.ts:516-549`.
- The dialog renders no error state: `src/components/git/GitRemotesDialog.tsx:55-210`.

**User impact**

A failed add loses both entered values, a failed edit closes and loses the attempted URL, and a failed removal looks completed until the unchanged list is noticed. There is no in-dialog explanation or retry context.

**Acceptance criteria**

- Add/edit/remove controls clear or close only on confirmed success.
- Errors render inside the dialog beside the relevant row/form and preserve entered data.
- Each action has a busy state and prevents duplicate submission.
- Removal confirmation names the remote and URL.
- Tests cover backend rejection for all three mutations.

### UJ-16 — Credential persistence failure is hidden and credential fields accept empty values

**Severity:** High  
**Confidence:** High

**Evidence**

- Keychain save failure sets `saveError`, but `setCredentials` is called immediately afterward: `src/components/git/GitCredentialsDialog.tsx:122-130`.
- `setCredentials` closes the dialog immediately: `src/stores/git-store.ts:552-560`.
- The error is only rendered inside that now-closed dialog: `src/components/git/GitCredentialsDialog.tsx:139-145`.
- Connect is always enabled and constructs credentials without validating SSH key path, username/password, or token: `src/components/git/GitCredentialsDialog.tsx:105-120`, `288-290`.
- The overview icon's accessible label always says “Change SSH credentials,” even for agent, password, or token credentials: `src/components/git/GitLandingPanel.tsx:211-229`.

**User impact**

Users are told neither that credentials were not persisted nor that they will need to re-enter them next session. Empty credentials can close the dialog and auto-retry a pending operation, producing an avoidable backend failure rather than field guidance.

**Acceptance criteria**

- Keychain failure remains visible after the dialog closes (persistent alert/toast) or keeps the dialog open with explicit **Use Once** and **Retry Save** choices.
- Credential type has type-specific required validation before Connect.
- Pending operations retry only after valid credentials are activated.
- The settings control uses protocol-neutral accessible text (“Change Git credentials”).
- Secrets are cleared from inactive form types and dialog state when safely possible.

### UJ-17 — Collection-vs-workspace scope and entry semantics are inconsistent

**Severity:** Medium  
**Confidence:** High

**Evidence**

- Collection entry passes a collection path/name: `src/components/layout/GitToolbarButton.tsx:52-58`.
- Workspace entry passes workspace path/name to the same props called `collectionPath`/`collectionName`: `src/components/workspace/WorkspaceGitTab.tsx:9-25`; `src/components/git/GitPanel.tsx:37-42`.
- The non-repository state always says “This collection”: `src/components/git/GitPanel.tsx:177-181`.
- The header always uses a Package icon and a comment/prop naming the value as collection: `src/components/git/GitPanel.tsx:220-225`.
- Workspace Git is one of four automatically opened workspace tabs (`src/stores/pane-store.ts:632-657`), while the toolbar Git button is disabled because workspace mode clears `activeCollection` (`src/stores/pane-store.ts:658-663`; `src/components/layout/GitToolbarButton.tsx:63-76`).

**User impact**

Users cannot confidently tell whether status/discard/commit/branch/remotes/stash affect one collection or the entire workspace. Workspace users see collection terminology, while the prominent Git toolbar becomes unavailable in workspace mode even though a workspace Git UI exists.

**Acceptance criteria**

- GitPanel receives an explicit scope model such as `{ kind: 'workspace' | 'collection', id, displayName, repoRoot }`.
- The header visibly states scope and repository root (or a safely abbreviated path).
- All empty/error/destructive copy uses the correct scope noun.
- The toolbar either opens the current workspace Git panel in workspace mode or clearly explains why it is collection-only.
- Collection and workspace entry points converge on documented repository-root semantics and cannot share ambiguous stale state.

### UJ-18 — Status and completion language can be false or ambiguous

**Severity:** Medium  
**Confidence:** High

**Evidence**

- `isUpToDate` requires clean status and zero ahead/behind, but every other zero/zero dirty state falls through to “0 commits ahead”: `src/components/git/GitLandingPanel.tsx:175-178`, `307-330`.
- Ahead/behind returns `(0, 0)` when `HEAD`, local branch, upstream, or matching remote ref cannot be resolved: `crates/rocket-git/src/git2_service/helpers.rs:192-228`.
- “Never fetched” is local component state and resets whenever the landing panel remounts: `src/components/git/GitLandingPanel.tsx:46-50`, `327-330`.
- Credential-triggered automatic retry bypasses local spinner/timestamp handlers because the store invokes pending actions directly: `src/stores/git-store.ts:552-609`.
- Init, stage, unstage, discard, commit, branch, remotes, stash, and conflict resolution have no explicit success announcement.

**User impact**

A dirty working tree can be summarized as “0 commits ahead,” no-upstream state can look synchronized, and a successful credential-retried fetch can still say “Never fetched.” Users cannot reliably distinguish clean, dirty, unsynchronized, no-upstream, in-progress, failed, and completed states.

**Acceptance criteria**

- Status copy separately reports working-tree state, upstream relationship, ahead/behind, and last successful network result.
- Zero/zero without an upstream is labeled “No upstream” or “Sync status unavailable,” not “Up to date.”
- Dirty zero/zero state says “Local changes” rather than “0 commits ahead.”
- Pending-operation retries update the same operation state/spinner/result as direct clicks.
- Success and failure results remain visible long enough to be perceived and are announced accessibly.

### UJ-19 — Error visibility depends on the selected right-hand view

**Severity:** Medium  
**Confidence:** High

**Evidence**

- One global `error` is shared by all operation types: `src/stores/git-store.ts:55-56`, `124`.
- The overview, stash, and conflict screens render it: `src/components/git/GitLandingPanel.tsx:291-305`; `src/components/git/GitStashSection.tsx:158-164`; `src/components/git/ConflictResolver.tsx:67-80`, `111-124`.
- The file list, commit form, commit log, remotes dialog, and non-repository state do not render that error.
- Branch create/switch compare old/new global error values instead of receiving an action result: `src/components/git/BranchSelector.tsx:41-76`. Store branch actions do not clear stale error before starting: `src/stores/git-store.ts:453-514`.
- If the same error text occurs twice, `nextError === prevError`, so branch UI can treat the repeated failure as success and close/clear.

**User impact**

The same failure can be visible, hidden, or misclassified depending on the selected right pane and previous error text. Users may continue operating on stale state because the action that failed appears complete.

**Acceptance criteria**

- Actions return operation-scoped results; UI does not infer success from mutation of a shared string.
- Errors render at the action source and remain associated with repository, operation, and target.
- A panel-level alert center may supplement but not replace local errors.
- Repeating an identical failure remains a failure and does not close the control.
- Stale errors are cleared only when a new operation starts in the same scope or the user dismisses them.

### UJ-20 — Git workflows contain material keyboard and screen-reader barriers

**Severity:** Medium  
**Confidence:** High

**Evidence**

- Icon-only stage/unstage/discard controls have no `aria-label`; their meaning is provided only by tooltip: `src/components/git/GitFileList.tsx:85-97`, `121-136`, `148-169`, `217-250`.
- Branch merge/delete icon controls lack accessible names: `src/components/git/BranchSelector.tsx:154-188`.
- Remote edit/delete/save/cancel icon controls lack accessible names: `src/components/git/GitRemotesDialog.tsx:100-174`.
- Stash overflow and clear-selection icon controls lack accessible names: `src/components/git/GitStashSection.tsx:281-323`, `362-370`.
- Stash selection checkboxes exist only on mouse hover (until selection has already begun) and have no label: `src/components/git/GitStashSection.tsx:178-205`. Hidden stash actions cannot receive keyboard focus until CSS hover/focus-within reveals them, but the row itself is not focusable: `src/globals.css:625-633`.
- Clone collection choices use visual check opacity but no selected/checked state semantics: `src/components/git/GitCloneDialog.tsx:188-205`.
- Several Git components use raw `<button>`/`<input>` controls despite the repository's shadcn-only hard rule, including clone choices (`src/components/git/GitCloneDialog.tsx:191-205`), remote branches (`src/components/git/BranchSelector.tsx:200-210`), stash checkboxes (`src/components/git/GitStashSection.tsx:194-200`), commit rows (`src/components/git/GitCommitLog.tsx:63-72`), and commit-diff files (`src/components/git/CommitDiffView.tsx:55-67`).
- Role-button file/branch rows contain nested interactive buttons and Space handlers do not prevent page scrolling: `src/components/git/GitFileList.tsx:102-138`, `182-253`; `src/components/git/BranchSelector.tsx:133-192`.
- Most error regions lack `role="alert"`/live-region semantics; credential save error is a positive exception (`src/components/git/GitCredentialsDialog.tsx:139-145`).
- The loading skeleton marks itself `aria-hidden="true"` and `aria-busy="true"`, so assistive technology receives no useful loading announcement: `src/components/git/GitPanelSkeleton.tsx:1-4`.

**User impact**

Keyboard-only users cannot discover/start stash multi-selection and have difficulty reaching hover-only actions. Screen-reader users encounter unnamed destructive icon buttons, unannounced selected states, and inconsistent async/error announcements.

**Acceptance criteria**

- Every icon-only control has a unique accessible name including the target where useful.
- All row actions are keyboard discoverable without hover; stash selection is always reachable and each checkbox has a label.
- Collection choices expose radio/listbox selection semantics and current selection.
- Interactive nesting is removed; row activation and row actions have separate valid focus targets.
- Space handlers prevent scrolling when used for activation.
- Loading, operation progress, success, and errors use appropriate `aria-busy`, status, and alert/live-region patterns.
- Raw form/control elements in Git components are replaced with approved shadcn primitives.
- Automated axe and keyboard-navigation tests cover non-repo, file list, branches, clone picker, remotes, stash, and conflict flows.

### UJ-21 — Test coverage does not exercise the critical user journeys

**Severity:** Medium  
**Confidence:** High

**Evidence**

- The Git frontend test tree contains only `src/stores/__tests__/git-store.test.ts`; no Git component tests were found under `src/**/*.test.{ts,tsx}`.
- Existing store tests primarily assert API invocation and state refresh, for example staging at `src/stores/__tests__/git-store.test.ts:277-353`, branches at `355-423`, stash at `425-476`, and remotes at `478-520`.
- Tests codify resolving error behavior by awaiting store actions and checking global error, but do not test caller decisions after failure: `src/stores/__tests__/git-store.test.ts:131-145`, `345-352`, `405-412`.

**User impact**

Duplicate clone, false-success chaining, input loss, stale path races, destructive confirmations, conflict double-submit, clone completion, focus behavior, and inaccessible hover-only controls can regress without test failures.

**Acceptance criteria**

- Add component/integration tests for every Critical and High finding above.
- Include delayed/interleaved repository A/B responses and split-pane rendering.
- Include failure injection at each step of Stash & Pull, Fetch & Push, commit, remote editing, clone, and conflict resolution.
- Include keyboard-only and accessible-name assertions for all destructive actions.
- Add backend integration tests for unborn unstage, dirty remote checkout, abort semantics, stale conflict resolution, and branch/stash deletion policy.

---

## Recommended remediation order

1. **Contain data-loss/wrong-root risk:** repository-scoped state/actions; validate the invoking root; disable remote forced checkout; guard conflict resolution; confirm/replace hard-reset abort and discard.
2. **Make action results truthful:** return typed outcomes, stop failed sequences, preserve inputs, and introduce operation-scoped busy/error/success state.
3. **Repair clone and initial-repository journeys:** single clone owner, credential cancellation/retry, correct collection/workspace opening, and unborn unstage.
4. **Make remote/destructive intent explicit:** remote selector/no-remote state, branch merge status before delete, stash/drop confirmations, stable stash identities.
5. **Clarify scope and completion:** visible workspace-vs-collection root, merge completion state, upstream/sync state, and persistent operation feedback.
6. **Close accessibility gaps:** named controls, non-hover keyboard paths, valid interaction structure, selected-state semantics, and live announcements.
7. **Lock behavior with journey tests** before expanding Git functionality.

## Definition of a complete Git journey

The integration should be considered journey-complete when a user can:

- identify the exact workspace/collection repository being controlled;
- enter Git from every supported scope without stale-path reuse;
- distinguish checking, not-repo, load-failed, initialized-empty, and normal repository states;
- initialize or clone once, supply/cancel/change credentials, and open every supported cloned structure;
- stage, unstage, inspect, and discard with clear reversible/destructive semantics;
- commit without losing the message on error and configure identity in context;
- switch/create/merge/delete branches without silent overwrite or unconfirmed history loss;
- select/configure a remote and understand the target of fetch/pull/push;
- rely on stash/fetch safety prerequisites actually succeeding before a chained operation continues;
- apply/pop/drop stashes with stable identity and confirmations for irreversible actions;
- resolve each conflict exactly once, move through remaining conflicts, safely abort, and clearly complete the merge commit;
- perceive progress, success, failure, and recovery actions regardless of the currently selected right-hand view; and
- complete all of the above with keyboard and assistive technology.
