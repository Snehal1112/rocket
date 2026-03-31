# Bruno's Git UI: complete feature-by-feature blueprint

Bruno API client provides a built-in Git GUI that wraps the system-installed Git binary, enabling both technical and non-technical team members to collaborate on API collections through version control without leaving the app. **The Git UI opens as a dedicated tab** accessed via a Git Icon in the top-right navbar, and features a file change list, dual-mode diff viewer, branch selector (bottom-left), Quick Links menu, and action buttons for staging, committing, pushing, and pulling. Core features (init, view diffs, clone, pull) are free since Bruno 3.0.0; advanced features (commit, push, branch, stash, conflict resolution) require Pro or Ultimate editions. Below is the complete feature-by-feature breakdown for replicating this UI in RocketAPI.

---

## 1. Initializing Git kicks off the entire workflow

**What it does:** Sets up a `.git` directory for a Bruno collection, enabling all subsequent Git operations.

**Exact UI flow:**

1. User launches Bruno and navigates to the target collection in the sidebar.
2. User clicks the **Git Icon** — a dedicated icon positioned in the **top-right corner of the navbar**. This is the single entry point to the entire Git UI.
3. Since the collection is not yet Git-initialized, the panel displays an **"Initialize" button**.
4. User clicks **Initialize**. Bruno runs `git init` on the collection's filesystem directory.
5. After initialization, the Git UI becomes fully active — the user can now view diffs, stage changes, and access all other Git operations.

**UI elements:** Git Icon (navbar, top-right), Initialize button (prominent CTA in the uninitialized state). The Git Icon serves dual purpose: entry point for initialization when no repo exists, and entry point to the full Git UI once initialized.

**Screenshot reference:** `1-init-git.webp` — shows the Git panel with the Initialize button visible.

**Implementation note for RocketAPI:** This is a gating state. The Git panel should detect whether `.git` exists in the collection directory. If not, show only the Initialize button. Once initialized, switch to the full Git UI view with all operations available.

---

## 2. Viewing diffs with dual-mode comparison

**What it does:** Before committing, users review differences between their local working changes and the last committed version. Bruno provides **two distinct diff viewing modes** that users can toggle between.

**Exact UI flow:**

1. User navigates to the **Git UI** (by clicking the Git Icon or opening the Git tab).
2. The Git UI displays a **list of modified files** in a panel.
3. User clicks on any **modified file** in the list.
4. The **diff viewer** opens, showing changes with syntax highlighting. Additions and deletions are visually distinguished.
5. A toggle or switch control allows the user to flip between **Text-Based** and **Visual-Based** diff modes.

**UI elements:** Modified file list (left panel), diff viewer (main content area), mode toggle (to switch between text and visual diff views). Changes are "highlighted in the diff viewer."

**Screenshot reference:** `2-git-view.webp` — shows the main Git view with file list and diff viewer.

**Implementation note:** The file list should show modification status indicators. Clicking a file loads its diff into the main viewer area. The mode toggle should persist user preference across sessions.

---

## 3. Text-based diff view shows line-by-line code changes

**What it does:** Displays changes in a **traditional side-by-side or unified format**, showing additions and deletions line-by-line. The documentation describes this as "ideal for reviewing detailed code changes."

**Visual characteristics:**

- **Format:** Side-by-side or unified (the docs mention both, suggesting Bruno may support toggling between them or uses one by default).
- **Additions** are highlighted (typically green in standard diff UIs).
- **Deletions** are highlighted (typically red).
- Changes shown **line-by-line**, making it easy to pinpoint exactly what changed in the `.bru` file markup.
- Best suited for reviewing the raw Bru markup language changes, JSON body modifications, header changes, and script edits.

**Screenshot reference:** `git-text-based-diff.webp` — shows the traditional diff format with line-by-line comparison.

**Implementation note for RocketAPI:** Use a library like `diff2html` or Monaco's built-in diff editor for this view. Support both unified and split (side-by-side) layouts. Since Bruno collections are plain-text `.bru` files, syntax highlighting for the Bru format would be valuable.

---

## 4. Visual-based diff view highlights structural changes

**What it does:** Provides a **"more intuitive, visual representation of changes"** that highlights modifications in a **"user-friendly format"** designed for understanding **"structural changes at a glance."**

**How it differs from text-based:**

| Aspect | Text-Based | Visual-Based |
|--------|-----------|--------------|
| Format | Side-by-side or unified, line-by-line | Intuitive visual representation |
| Best for | Detailed code/markup review | Structural changes at a glance |
| Target user | Developers comfortable with diffs | Non-technical team members |
| Granularity | Line-level additions/deletions | Higher-level structural modifications |

The Visual-Based view likely renders the API request structure (headers, params, body, auth, scripts) as visual components rather than raw markup, then highlights which components changed. This makes it accessible to non-engineers who collaborate on API collections but don't read `.bru` file syntax.

**Toggle mechanism:** Users **"can switch between these views to choose the format that best suits your workflow"** — this confirms a toggle/switch control exists in the diff viewer area.

**Screenshot reference:** `git-visual-based-diff.webp` — shows the visual diff mode with user-friendly change visualization.

**Implementation note:** This is a key differentiator. For RocketAPI, consider rendering the request object as a structured form view (showing method, URL, headers, body, etc.) with change indicators on each field, rather than showing raw file diffs.

---

## 5. Connecting to a remote repository via Quick Links

**What it does:** Links the local Git-initialized collection to a remote repository (GitHub, GitLab, Bitbucket, or Azure DevOps) using HTTPS or SSH.

**Exact UI flow:**

1. **Prerequisites:** User creates an empty Git repository on the hosting platform and sets up either an **SSH key** or **Personal Access Token (PAT)**.
2. User opens a **Git-initialized** Bruno collection.
3. User clicks **"Quick Links"** — located in the **bottom-left corner** of the Git UI.
4. A dropdown menu appears. User selects **"Remotes"** from this dropdown.
5. The Remotes management panel opens. User clicks the **"Add Remote"** button.
6. An **"Add Remote" dialog box** appears with two input fields:
   - **Remote Name** — text input (typically "origin")
   - **URL** — text input accepting either **HTTPS or SSH** format
7. User fills in both fields and clicks **"Save"**.
8. The collection is now connected. A confirmation state shows the remote is linked.

**UI elements in detail:**

- **Quick Links** button/link — bottom-left corner of Git UI, opens a dropdown menu
- **Remotes** — dropdown item within Quick Links
- **Add Remote** button — within the Remotes management panel
- **Add Remote Dialog Box** — modal with Remote Name field, URL field, and Save button
- Two authentication methods supported: SSH keys and Personal Access Tokens

**Screenshot references:** `5-add-remote-option.webp` (Quick Links dropdown showing Remotes), `6-add-remote-dialogbox.webp` (the Add Remote dialog with name and URL fields).

**Implementation note:** The Quick Links menu in the bottom-left is a key navigation pattern in Bruno's Git UI. It provides quick access to configuration. For RocketAPI, consider a similar "settings/config" shortcut area in the Git panel footer.

---

## 6. Staging and committing with per-file or bulk actions

**What it does:** Allows users to stage individual or all changed files, write a commit message, and commit — a complete staging-to-commit workflow.

**Exact UI flow:**

1. User navigates to the **Git UI**.
2. The file list shows all modified/new/deleted files with change indicators.
3. **Per-file staging:** Each file has an **"Add Icon"** (a clickable icon next to the filename). Clicking it stages that individual file.
4. **Bulk staging:** An **"Add All Changes"** button stages every changed file in a single click.
5. After staging, user clicks the **"Commit Changes"** button.
6. A **commit message input field** appears (or becomes active).
7. User writes a descriptive commit message.
8. User clicks the **"Commit"** button to finalize.
9. Changes are now committed locally, ready to be pushed to remote.

**UI elements in detail:**

- **Add Icon** — per-file button/icon next to each modified file in the change list (likely a "+" icon)
- **Add All Changes** button — bulk action, stages everything at once
- **Commit Changes** button — triggers the commit flow after staging
- **Commit message input** — text field for the commit message
- **Commit** button — final confirmation button

**Screenshot references:** `3-stage-changes.webp` (shows the Add Icon for individual file staging), `4-commit-changes.webp` (shows the commit message input and Commit button).

**Key design pattern:** The staging workflow is a two-phase process: (1) select what to stage via individual Add Icons or the bulk Add All button, then (2) write message and commit. This mirrors the `git add` → `git commit` CLI workflow but with visual file selection.

**Implementation note:** The per-file Add Icon is important for selective commits. Implement a checkbox or "+" icon per file row. The "Add All Changes" should be a prominent button above or near the file list. The commit message input appears after or alongside the staging area, with the final Commit button clearly labeled.

---

## 7. Pushing changes requires a fetch-first pattern

**What it does:** Pushes locally committed changes to the remote repository, with a mandatory fetch step first.

**Exact UI flow:**

1. **Prerequisite:** Remote repository must already be linked (via the Add Remote flow).
2. User navigates to the **Git UI**.
3. User clicks the **"Fetch"** button first — this checks the remote for any new changes and ensures the local state is aware of the remote's current status.
4. User clicks the **"Push"** button to upload local commits to the remote.
5. Local changes are now visible in the remote repository.

**UI elements:** **Fetch** button and **Push** button, both visible in the Git UI. The screenshot filename `8-push-pull.webp` suggests that **Push and Pull buttons are co-located** in the same UI area, possibly as a button group or toolbar.

**Design decision — fetch before push:** Bruno enforces a **fetch-then-push** pattern. This is a safety mechanism that ensures users are aware of any remote changes before pushing, reducing the chance of conflicts. The documentation explicitly states the two-step process: "Go to Git UI and click the Fetch button" → "Click the Push button."

**Additional context from the second blog post:** The push flow may include a **"Push to"** selector that allows confirming **Remote Server configurations** before executing, suggesting a confirmation step or dropdown where users verify which remote/branch they're pushing to.

**Screenshot reference:** `8-push-pull.webp` — shows both Push and Pull buttons in the Git UI.

**Implementation note:** Implement Fetch and Push as sequential actions in the same toolbar area. Consider disabling the Push button until Fetch completes, or showing a status indicator after Fetch (e.g., "0 commits behind, 3 commits ahead"). A "Push to" remote/branch selector dropdown adds safety for multi-remote setups.

---

## 8. Branch creation lives in the bottom-left selector

**What it does:** Creates new Git branches for isolating features or tasks, accessed through the branch selector.

**Exact UI flow:**

1. User navigates to the **Git UI**.
2. In the **bottom-left corner**, the current branch name is displayed (e.g., **"main"**). This is a clickable element.
3. User clicks on the branch name (**"main"** or whatever the current branch is).
4. A **branch management panel/dropdown** appears, showing a text input field.
5. User types a new **Branch Name** into the input field.
6. User clicks the **"Create branch"** button.
7. The new branch is created and the user can immediately start working on it.

**UI elements:**

- **Branch indicator** — bottom-left corner of Git UI, shows current branch name as a clickable label
- **Branch Name input** — text field that appears in the branch management dropdown
- **"Create branch" button** — confirms creation

**Screenshot reference:** `7-create-branch.webp` — shows the branch creation interface with the input field and Create branch button.

**Implementation note:** The branch selector in the bottom-left serves triple duty: (1) displays current branch, (2) creates new branches, and (3) switches between existing branches (checkout). This is a compact, efficient design pattern. For RocketAPI, implement this as a dropdown/popover triggered by clicking the branch name, with a text input at top for new branch creation and a list of existing branches below for checkout.

---

## 9. Branch checkout is a three-click operation

**What it does:** Switches the working directory to a different existing branch.

**Exact UI flow:**

1. User clicks on the **current branch name** (e.g., "main") in the **bottom-left corner** — the same location as branch creation.
2. A **dropdown/list of available branches** appears.
3. User selects the **target branch** from the list.
4. Bruno switches to the selected branch immediately. The UI updates to reflect the new branch name.

**UI elements:** Branch name label (bottom-left, clickable), branch list dropdown showing all available branches, each branch as a selectable item. The current branch is likely visually distinguished from other branches in the list.

**Screenshot reference:** `9-branch-checkout.webp` — shows the branch list with selectable branches.

**Key UX detail:** Branch switching is described as instant — "Now you're switched to your selected branch." There's no confirmation dialog mentioned, suggesting a direct-switch pattern. The same dropdown that handles branch creation also handles checkout, creating a unified branch management interface.

**Implementation note:** The branch dropdown should show all local (and possibly remote) branches. Consider adding a search/filter for repos with many branches. Show the current branch with a checkmark or highlight. After switching, refresh the file tree and any open editors to reflect the new branch state.

---

## 10. Stash changes preserves work-in-progress safely

**What it does:** Temporarily saves uncommitted local changes (both staged and unstaged modified tracked files), reverts the working directory to a clean state, and allows restoring changes later.

**Exact UI flow — creating a stash:**

1. User navigates to the **Git UI**.
2. When the user has **unstaged changes**, a **"Stashes"** button/tab becomes available.
3. User clicks **"Stashes"**.
4. A stash creation interface appears with a **"Stash Message"** input field.
5. User types a descriptive message and clicks the **"Stash files"** button.
6. All local changes (both staged and unstaged) are saved to the stash. The working directory reverts to a clean state matching the last commit.

**Managing existing stashes:**

Stashes can be **viewed**, **deleted**, or **applied** to restore changes later. The second screenshot (`12-stash-options.webp`) shows the stash management interface with these three action options per stash entry.

**UI elements:**

- **Stashes** button/tab — appears in Git UI when unstaged changes exist
- **Stash Message** input — text field for labeling the stash
- **"Stash files"** button — executes the stash operation
- **Stash list** — shows previously saved stashes
- **Per-stash actions:** View, Delete, Apply buttons/icons

**Screenshot references:** `11-stash-files.webp` (stash creation with message input and Stash files button), `12-stash-options.webp` (stash management showing view/delete/apply options).

**Use cases emphasized in docs:** Quickly switching context, handling urgent tasks, switching branches without committing unfinished work. The consumer guide also notes: "If you have local uncommitted changes when pulling, you may be prompted to stash them first."

**Implementation note:** The stash feature integrates with the pull and branch checkout workflows — Bruno may auto-prompt users to stash when they try to pull with uncommitted changes. For RocketAPI, implement stash as both a manual action and an auto-suggested action when operations would be blocked by dirty working state.

---

## 11. Merge conflict detection shows divergence status

**What it does:** When conflicts arise during pull or stash operations, Bruno detects them and displays a conflict resolution interface with actionable information.

**When conflicts trigger:** Conflicts can arise during **pull operations** (remote changes conflict with local changes) or **stash operations** (applying a stash that conflicts with current state).

**What the conflict detection screen shows:**

- **Commits behind and ahead of remote** — quantifies how far the local branch has diverged from the remote, giving users context about the scope of divergence
- **Conflicting changes identified** — lists which specific files have conflicts
- **Abort button** — allows the user to abort the operation entirely and restore the previous clean state

**UI elements:** Conflict status panel showing behind/ahead commit counts, conflicted file list, and a prominent **Abort** button as an escape hatch.

**Screenshot reference:** `13-merge-remotes.webp` — shows the conflict detection screen with divergence info and abort option.

**Implementation note:** The conflict detection is an intermediate state between triggering a pull/stash-apply and the actual conflict resolution editor. It serves as an informational screen that lets users decide whether to proceed with resolution or abort. For RocketAPI, show this as a banner or modal with clear metrics (X commits behind, Y ahead, Z files conflicting) and Abort / Resolve options.

---

## 12. Visual conflict editor enables three-way resolution

**What it does:** Provides a dedicated editor for resolving each conflict with Accept Incoming/Current/Both options, plus a file list showing conflict status across all files.

**Visual Conflict Editor components:**

**Left panel — File List:**
- Shows **all files** involved in the merge — both conflicted and non-conflicted
- Each file has a **status indicator** (visual marker distinguishing conflicted files from clean ones)
- Users can click through files to resolve conflicts one by one

**Right panel — Diff View per conflict:**
For each conflicted file, the editor presents three resolution options:
- **Accept Incoming** — keeps the remote version (theirs)
- **Accept Current** — keeps the local version (ours)
- **Accept Both** — merges both versions together

These options are presented as clickable actions (likely buttons) above or within each conflict block in the diff view.

**Finalizing the merge:**
1. User resolves all conflicts across all files using the Accept options.
2. User enters a **commit message** (for the merge commit).
3. User clicks the **Lock icon** to complete the merge.

**UI elements in detail:**

- **File list with status indicators** — left panel, visual differentiation between conflicted (likely red/warning) and non-conflicted (likely green/clean) files
- **Diff view** — main content area showing the conflict content
- **Accept Incoming** button — resolves conflict with remote version
- **Accept Current** button — resolves conflict with local version
- **Accept Both** button — merges both versions
- **Commit message input** — text field for the merge commit message
- **Lock icon** — the specific button used to finalize/complete the merge (notable: Bruno uses a Lock icon rather than a standard "Merge" or "Commit" button)

**Screenshot reference:** `14-merge-conflicts.webp` — shows the full conflict editor with file list, diff view, and resolution options.

**Implementation note for RocketAPI:** The **Lock icon for merge finalization** is an unusual but distinctive design choice — it signals that the merge is being "locked in." Consider whether a more explicit "Complete Merge" button would be clearer. The three-way resolution (Accept Incoming/Current/Both) is standard and should be implemented per-conflict-block, not per-file, since a single file can have multiple conflict regions. The file list with status indicators is critical for navigating multi-file conflicts efficiently.

---

## Architecture and layout for building an equivalent system

**How Bruno structures the Git UI overall:**

The Git UI opens as a **dedicated tab** (not a sidebar or modal) within Bruno's main content area. This tab contains several sub-views organized as inner tabs: **Readme**, **Commit History**, **Tags**, and **Changes**. The Changes view is where most Git operations happen — it shows the file list, diff viewer, and staging/commit controls.

**Key layout zones in the Git UI tab:**

| Zone | Location | Contains |
|------|----------|---------|
| Entry point | Top-right navbar | Git Icon (opens Git UI or shows Initialize) |
| Inner navigation | Top of Git tab | Readme, Commit History, Tags, Changes tabs |
| File changes | Left panel | Modified file list with Add Icons and status indicators |
| Diff viewer | Main content | Text-based or Visual-based diff with mode toggle |
| Action bar | Near file list | Add All Changes, Commit Changes buttons |
| Branch management | Bottom-left | Current branch name (clickable) → dropdown with create/switch |
| Quick Links | Bottom-left | Dropdown with Remotes and other config options |
| Push/Pull controls | Within Git UI | Fetch, Push, Pull buttons (co-located) |

**Technology requirement:** Bruno does not embed Git — it requires **Git to be installed on the user's machine** and wraps the system binary. For RocketAPI with Tauri, you could either call the system Git binary via Tauri commands or use a Rust Git library like `git2` (libgit2 bindings) for a zero-dependency experience.

**Free vs. paid feature gating:** Initialize, View Diffs, Clone, Pull, and Check for Updates are free. All write operations (Commit, Push, Branch, Stash, Connect Remote, Conflict Resolution) are paid. For RocketAPI, consider whether to adopt similar gating or differentiate by offering more in the free tier.

---

## Conclusion

Bruno's Git UI is designed around a **single-tab paradigm** where the Git Icon in the top-right navbar opens a full Git management tab with inner sub-views. The bottom-left corner serves as the **control center** for branch management and configuration (Quick Links → Remotes). The standout design patterns worth replicating are the **dual-mode diff viewer** (text-based for developers, visual-based for non-technical users), the **per-file Add Icon staging model** with a bulk "Add All Changes" fallback, the **fetch-before-push enforcement** for safety, and the **three-option conflict resolution** (Accept Incoming/Current/Both) with a file-level status list. The Lock icon for merge finalization and the Quick Links menu pattern for accessing remote configuration are distinctive Bruno design choices that could be refined in RocketAPI's implementation. All 15 documented screenshots are hosted at `mintcdn.com/bruno-a6972042/lK4oxIJsFt1GYUhN/images/screenshots/git-integration/git-gui/` and can be referenced for pixel-level UI replication.