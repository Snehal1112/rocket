# Rocket User Manual

Rocket is a fast, native API client for testing and debugging HTTP APIs. Built with Tauri 2, it runs on Linux, macOS, and Windows. Your data stays on your machine — no cloud, no account.

![App Overview](screenshots/01-app-overview-dark.png)

---

## Table of Contents

1. [Interface Overview](#interface-overview)
2. [Workspaces](#workspaces)
3. [Collections](#collections)
4. [Creating Requests](#creating-requests)
5. [Request Editor](#request-editor)
6. [Response Viewer](#response-viewer)
7. [Environments](#environments)
8. [Git Integration](#git-integration)
9. [Load Testing](#load-testing)
10. [Themes and Appearance](#themes-and-appearance)
11. [Keyboard Shortcuts](#keyboard-shortcuts)
12. [Data Storage](#data-storage)

---

## Interface Overview

The Rocket window is divided into four areas:

```
 ┌──────────────────────────────────────────────────────┐
 │  Title Bar  [Workspace Switcher]                     │
 ├────────┬─────────────────────────────────────────────┤
 │        │  [Collection Dropdown]  [Tabs]  [+]         │
 │ Side-  ├─────────────────────────────────────────────┤
 │  bar   │  Request Editor                             │
 │        │  [Method] [URL]               [Send] [Zap]  │
 │        │  [Params] [Headers] [Body] [Auth]           │
 │        ├─────────────────────────────────────────────┤
 │        │  Response Viewer                            │
 │        │  [Pretty] [Raw] [Preview] [Headers]         │
 ├────────┴─────────────────────────────────────────────┤
 │  Status Bar  [Theme Toggle] [Console]                │
 └──────────────────────────────────────────────────────┘
```

- **Title Bar** — app logo, workspace switcher, window controls
- **Sidebar** (left, resizable) — collection tree with search, action buttons
- **Collection Dropdown** — shows current workspace or collection name. Click to switch context or return to workspace overview.
- **Tab Bar** — open request tabs with a `+` button for new requests
- **Request Editor** — URL bar, params, headers, body, auth tabs
- **Response Viewer** — formatted response with status, timing, headers
- **Status Bar** — theme toggle (Sun/Moon icon), console toggle with log count

---

## Workspaces

A workspace groups related collections, environments, and settings into one folder on disk.

### Workspace Overview

Access the overview by clicking the workspace name in the Collection Dropdown. The dashboard shows:

- **Stats** — three cards: collection count, environment count, total request count
- **Quick actions** — "Create Collection" (inline name input) and "Open Collection" (folder picker for linking external collections)
- **Collections list** — name, filesystem path, request count, type badge (embedded/external), action menu (Open, Delete)
- **Description** — editable text area for workspace notes, saved on blur

### Managing Workspaces

**Create:** Open the workspace switcher in the title bar, click "New Workspace", enter a name. The folder defaults to `~/.rocket-api/<name>/` — each workspace gets its own subfolder.

**Switch:** Use the workspace switcher. The active workspace has a checkmark. Pinned workspaces appear at the top. When switching, your open tabs are snapshotted and restored when you return.

**Collection Dropdown behavior:** Shows the workspace name (with briefcase icon) when in workspace mode, or the collection name (with clock icon) when editing requests. The dropdown updates automatically when you click items in the sidebar.

---

## Collections

Collections organize API requests into folders. Each collection is a directory on disk containing `.yml` request files in OpenCollection format.

**Create** from the workspace overview ("Create Collection" button) or the `+` icon in the sidebar toolbar.

**Browse** by expanding collections in the sidebar tree. Single-click to expand/collapse, double-click to open settings.

**Settings** (double-click a collection) include five tabs:
- **Overview** — name and description
- **Authorization** — default auth for all requests (overridable per request)
- **Variables** — collection-scoped variables resolved alongside environment variables
- **Readme** — markdown documentation with edit/preview toggle
- **Tags** — aggregated tag view across all requests with counts

**Context menu** (right-click or `...` button): New Request, New Folder, Rename, Delete.

---

## Creating Requests

### Method 1: From Collection Menu (saves to disk immediately)

Click `...` on any collection or folder and select "New Request". A dialog opens with:

- **Request Type** — HTTP, GraphQL, gRPC, WebSocket, or From cURL
- **Request Name** — display name. Unsafe filesystem characters (`/`, `:`, `*`, etc.) are auto-replaced with `-`. A hint shows the sanitized filename when it differs from the display name.
- **HTTP Method** — shown only for HTTP and cURL types
- **URL** — optionally pre-fill

Click "Create" to save the file and open a new tab.

### Method 2: Quick Tab (`+` button — no dialog, no disk file)

- **Left-click `+`** in the tab bar — instant blank HTTP request tab
- **Right-click `+`** — choose type: HTTP, GraphQL, gRPC, or WebSocket

These "ephemeral" tabs have no collection binding. A "Save to Collection" button appears in the toolbar.

### Method 3: Sidebar `FilePlus` Button

Click the `FilePlus` icon in the sidebar toolbar for an instant ephemeral HTTP tab (same as left-clicking `+`).

### Saving Ephemeral Requests

Click "Save to Collection" in the toolbar or press `Cmd/Ctrl+S` on an unsaved tab:

1. Enter a request name (filesystem hint shown if sanitized)
2. Select an existing collection or create a new one
3. Click "Save"

The tab becomes collection-bound with auto-save enabled. Closing an ephemeral tab always shows a confirmation dialog to prevent accidental data loss.

---

## Request Editor

### URL Bar

Select an HTTP method from the dropdown, type your URL, click **Send** (or `Cmd/Ctrl+Enter`).

The URL bar highlights:
- `:paramName` segments — auto-extracted to the Params tab
- `{{variable}}` tokens — resolved from environments at send time
- `?key=value` query parameters — synced with the Params tab

### Params Tab

Two sections:

**Path** — read-only keys derived from URL `:param` segments. Only values and the enable/disable toggle are editable. Add or remove path params by editing the URL directly.

**Query** — full add/remove/toggle capability with column headers (Name, Value). Edits sync bidirectionally with the URL bar.

### Headers Tab

Key-value table with column headers (Header name, Value) and per-header enable/disable toggles. `Content-Type` and `Authorization` headers are set automatically based on body mode and auth settings.

### Body Tab

- **JSON** — Monaco editor with syntax highlighting, code folding, bracket matching
- **XML** — Monaco editor with XML syntax
- **Text** — plain text editor
- **Form Data** — key-value pairs as `multipart/form-data`
- **Binary** — file picker for upload
- **None** — no body (default for GET)

### Auth Tab

- **None** — no authentication
- **Basic** — username + password (Base64 Authorization header)
- **Bearer** — token with configurable prefix
- **API Key** — key-value pair in header or query parameter
- **OAuth 2.0** — Client Credentials, Password, and Authorization Code with PKCE. Token refresh supported.
- **AWS Signature V4** — access key, secret, region, service, optional session token

Collection-level auth applies to all requests unless overridden.

---

## Response Viewer

After sending, the response panel shows:

**Status bar:** color-coded status code (green 2xx, yellow 3xx, red 4xx/5xx), response time (green <=200ms, yellow 200-1000ms, red >1s), response size.

**Tabs:**
- **Pretty** — formatted response in a read-only Monaco editor with syntax highlighting
- **Raw** — unformatted response text
- **Preview** — HTML rendered in a sandboxed iframe. Safe mode (default) restricts JavaScript; Developer mode allows full execution.
- **Headers** — searchable response headers table with copy-to-clipboard per value

---

## Environments

Environments define variables (`BASE_URL`, `API_KEY`, etc.) resolved in URLs, headers, body, and auth using `{{variable}}` syntax.

- **Switch** via the dropdown in the header bar
- **Secret variables** hide values in the UI
- **Priority:** environment variables override collection variables of the same name
- **Storage:** YAML files in the workspace's `environments/` directory

---

## Git Integration

Rocket has full git support built in — stage, commit, push, pull, branch, stash, and resolve conflicts without leaving the app.

Open the **Git UI** tab from workspace tabs or the git toolbar button.

### Layout

Resizable two-panel split (drag the border between 200px and 500px):

| Left Panel | Right Panel |
|---|---|
| Collection name + branch selector | Landing overview (default) |
| Commit form (collapsible) | File diff (when file clicked) |
| Staged changes (with unstage/count) | Commit history (when "Commits" clicked) |
| Unstaged changes (with stage/discard/count) | Stash list (when "Stashes" clicked) |
| Navigation links (Commits, Stashes, Remotes) | Breadcrumb: "← Overview \| context" |

### Staging

Staged changes appear **above** unstaged (matching VS Code/Bruno convention). Per-file actions appear on hover:

| Icon | Action | Available on |
|---|---|---|
| `+` | Stage file | Unstaged files |
| `-` | Unstage file | Staged files |
| Trash | Discard changes | Unstaged files |

Bulk actions in section headers: stage all, unstage all, discard all.

### Branches

Click the branch name to open the selector:

- **Local branches** — click to switch. Current branch has a checkmark. Hover shows merge and delete buttons.
- **Remote branches** — appear after fetch in a "Remote" section. Click to create a local tracking branch and switch to it. Already-checked-out branches are hidden.
- **Create** — type a name in the bottom input, press Enter.
- **Search** — filter by typing in the search input.

### Remote Operations

Landing panel buttons with live commit counts:

| Button | Shows | Safety prompt |
|---|---|---|
| **Fetch** | Always "Fetch" | None |
| **Pull** | "Pull ↓N" when behind | Stash dialog if working tree is dirty |
| **Push** | "Push ↑N" when ahead | Fetch-first dialog if not recently fetched |

Status line shows "↑ N Ahead | ↓ N Behind" with a branch health badge.

### Diff Viewer

Click a file to see its diff. Two modes:

- **Text** — Monaco side-by-side diff editor with syntax highlighting
- **Visual** — structured field-by-field comparison for `.yml` request files (shows which fields changed: method, URL, headers, body, auth)

Toggle between **Working** (uncommitted) and **Staged** (indexed) views. Breadcrumb at the top: "← Overview | path/to/file.yml".

### Conflict Resolution

When merge conflicts occur:

- Side-by-side "Ours" vs "Theirs" in read-only Monaco editors
- **Accept Ours** / **Accept Theirs** / **Edit Manually** (opens full editor)
- **Abort Merge** cancels and returns to the pre-merge state
- Resolved files are auto-staged

---

## Load Testing

Stress-test any API endpoint directly from the request editor.

### How to Run

1. Open any request
2. Click the **Zap** icon (lightning bolt) next to Send
3. Set **Concurrent requests** (1, 5, 10, 25, 50, or 100) and **Total requests** (default 100)
4. Click **Run**

The request is sent exactly as configured — same URL, method, headers, body, auth.

### Understanding Results

| Metric | What it means |
|---|---|
| **Total / Succeeded / Failed** | How many completed vs errored (connection failures, timeouts) |
| **Min / Avg / Max** | Fastest, mean, and slowest response times |
| **P50** | Median — half of requests were faster |
| **P95** | 95th percentile — the metric that matters most for tail latency |
| **P99** | 99th percentile — only 1% were slower |
| **Req/sec** | Throughput (successful requests per second) |
| **Duration** | Total wall-clock time |

### Tips

- Start with concurrency 5-10 to establish a baseline, then increase
- A large gap between P50 and P95 indicates inconsistent response times
- High failure count at high concurrency may indicate rate limiting
- Each request uses a fresh connection (no pooling)

---

## Themes and Appearance

Toggle between light and dark themes using the **Sun/Moon** icon in the bottom-left corner of the status bar.

- **First launch** reads your system preference (`prefers-color-scheme`)
- **Manual toggle** persists across sessions via localStorage
- **Monaco editor** uses custom `rocket-light` / `rocket-dark` themes defined at startup — no flash of wrong theme on first render
- **All components** (sidebar, git panel, dialogs, diff viewer) respect the active theme

---

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Cmd/Ctrl+Enter` | Send active request |
| `Cmd/Ctrl+S` | Save request (opens Save to Collection dialog if unsaved) |
| `Cmd/Ctrl+W` | Close active tab (confirmation dialog for unsaved tabs) |
| `Cmd/Ctrl+Tab` | Next tab (wraps around) |
| `Cmd/Ctrl+Shift+Tab` | Previous tab (wraps around) |
| `Cmd/Ctrl+1-9` | Jump to tab by 1-based index |

---

## Data Storage

All data is stored locally on your filesystem. Nothing is sent to the cloud.

```
~/.rocket-api/
  workspaces.yml                    # Workspace registry
  <workspace-name>/
    workspace.yml                    # Workspace config (name, description, collections)
    collections/
      <collection-name>/
        opencollection.yml           # Collection settings (auth, headers, variables, readme)
        get-users.yml                # Request files (OpenCollection YAML format)
        auth/
          login.yml                  # Requests in subfolders
          folder.yml                 # Folder metadata
    environments/
      production.yml                 # Environment variables
      staging.yml
    history/                         # Request execution history (one file per entry)
    cookies/                         # Cookie jar storage
    templates/                       # Saved request templates
```

Share collections with your team by initializing a git repository in the collection directory and using Rocket's built-in git UI to commit, push, and pull.
