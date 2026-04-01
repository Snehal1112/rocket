# Rocket User Manual

Rocket is a fast, native API client for testing and debugging HTTP APIs. Built with Tauri 2, it runs on Linux, macOS, and Windows. Your data stays on your machine — no cloud, no account.

## Table of Contents

- [Getting Started](#getting-started)
- [Workspaces](#workspaces)
- [Collections](#collections)
- [Creating Requests](#creating-requests)
- [Request Builder](#request-builder)
- [Query and Path Parameters](#query-and-path-parameters)
- [Headers](#headers)
- [Request Body](#request-body)
- [Authentication](#authentication)
- [Response Viewer](#response-viewer)
- [Environments](#environments)
- [Git Integration](#git-integration)
- [Load Testing](#load-testing)
- [Themes](#themes)
- [Keyboard Shortcuts](#keyboard-shortcuts)

---

## Getting Started

When you first launch Rocket, a default workspace is created at `~/.rocket-api/My Workspace/`. The interface has three main areas:

- **Collections sidebar** (left) — browse and manage your API collections
- **Request editor** (center-top) — build and send HTTP requests
- **Response viewer** (center-bottom) — inspect responses

The **Collection Dropdown** above the tab bar shows your current context. Click the workspace name to return to the workspace overview. Click a collection name to switch between collections.

| Light Mode | Dark Mode |
|:---:|:---:|
| ![App Overview Light](screenshots/01-app-overview-light.png) | ![App Overview Dark](screenshots/01-app-overview-dark.png) |

---

## Workspaces

A workspace is a container that groups related collections, environments, and settings together. Think of it as a project folder — each workspace maps to a directory on your filesystem. This makes it easy to organize APIs by team, client, or project, and to share them via git.

### Workspace Overview

The workspace overview is your home screen. Access it by clicking the workspace name in the **Collection Dropdown** above the tab bar, or by opening the **Overview** workspace tab.

The overview dashboard shows:

- **Stats row** — three cards displaying the number of collections, environments, and total requests across all collections. This gives you an at-a-glance summary of the workspace's scope.
- **Quick actions** — **Create Collection** opens an inline name input to create a new collection instantly. **Open Collection** launches a folder picker to link an existing collection from anywhere on your filesystem (useful for shared team collections in git repos).
- **Collections list** — every collection in the workspace is listed with its name, filesystem path, request count (e.g. "12 requests"), and a type badge (embedded or external). Click any collection to open its settings. Use the `...` action menu on each row to Open or Delete.
- **Description** — a free-form text area for workspace notes. Changes are saved automatically when you click away.

| Light Mode | Dark Mode |
|:---:|:---:|
| ![Workspace Overview Light](screenshots/12-workspace-overview-light.png) | ![Workspace Overview Dark](screenshots/12-workspace-overview-dark.png) |

### Creating a Workspace

1. Open the workspace switcher dropdown in the title bar
2. Click **New Workspace**
3. Enter a name — the folder path defaults to `~/.rocket-api/<name>/`, creating a dedicated subfolder so each workspace can be deleted safely without affecting others
4. Optionally browse to a different location
5. Click **Create**

The new workspace is activated immediately. It starts empty — use the overview to create your first collection.

### Switching Workspaces

Use the workspace switcher dropdown in the title bar. The active workspace is indicated with a checkmark. Pinned workspaces appear at the top for quick access. When you switch workspaces, all collection tabs are snapshotted and restored when you switch back — no work is lost.

### Collection Dropdown

The **Collection Dropdown** sits above the tab bar and reflects your current context:

- When viewing workspace tabs, it shows the **workspace name** with a briefcase icon
- When editing a request, it shows the **collection name** with a clock icon
- Clicking the dropdown lists the workspace (clickable to return to workspace mode) and all collections (clickable to switch)
- The active collection updates automatically when you click requests in the sidebar

---

## Collections

Collections organize your API requests into folders. Each collection is a directory on disk containing `.yml` request files in OpenCollection format.

### Creating a Collection

From the **Workspace Overview**, click **Create Collection** and type a name. Or use the `+` button in the sidebar toolbar.

### Collection Tree

The sidebar shows your collections as an expandable tree:

- **Single-click** a collection to expand/collapse it
- **Double-click** a collection to open its Overview tab (settings, auth, variables, readme, tags)
- **Right-click** for context menu: New Request, New Folder, Rename, Delete
- Click the `...` menu on any collection for the same actions

### Collection Settings

Double-click a collection to open its settings tabs:

- **Overview** — description and name
- **Authorization** — default auth applied to all requests (overridable per request)
- **Variables** — collection-scoped variables
- **Readme** — markdown documentation with edit/preview toggle
- **Tags** — aggregated view of all tags across requests

| Light Mode | Dark Mode |
|:---:|:---:|
| ![Collections Light](screenshots/10-collections-sidebar-light.png) | ![Collections Dark](screenshots/10-collections-sidebar-dark.png) |

---

## Creating Requests

Rocket offers multiple ways to create requests, matching different workflows. You can plan ahead with a structured dialog, or jump straight in and save later.

### 1. From the Collection Menu (Structured)

This is the most intentional way to create a request — it saves directly to a collection on disk.

1. Click the `...` button on any collection or folder in the sidebar
2. Select **New Request** from the menu
3. A dialog opens with four fields:
   - **Request Type** — choose HTTP, GraphQL, gRPC, WebSocket, or From cURL. The Method selector only appears for HTTP and cURL types.
   - **Request Name** — the display name shown in the sidebar and tab title. If your name contains characters that are unsafe for filenames (like `/`, `:`, `*`, `?`), Rocket sanitizes them automatically and shows a hint: "Saved as: GET -users--id.yml".
   - **HTTP Method** — GET, POST, PUT, PATCH, DELETE, OPTIONS, HEAD
   - **URL** — optionally pre-fill the endpoint URL
4. Click **Create** (or press `Enter`)

The request file is written to disk immediately and a new tab opens with the URL pre-filled. Since it has a collection binding from the start, auto-save is active.

| Light Mode | Dark Mode |
|:---:|:---:|
| ![Create Request Dialog Light](screenshots/17-create-request-dialog-light.png) | ![Create Request Dialog Dark](screenshots/17-create-request-dialog-dark.png) |

### 2. Quick New Tab (`+` button)

For fast iteration when you just want to try something without committing to a collection yet.

Click the `+` button at the end of the tab bar:

- **Left-click** creates an instant HTTP request tab. No dialog, no file on disk. You can start typing a URL and sending requests immediately.
- **Right-click** opens a context menu to choose the request type: HTTP, GraphQL, gRPC, or WebSocket.

These are "ephemeral" tabs — they exist only in memory until you save them. The tab title shows "Untitled" and a **Save to Collection** button appears in the toolbar. Closing an ephemeral tab always shows a confirmation dialog, even if you haven't made any changes, because the data would be lost permanently.

### 3. Sidebar `FilePlus` Button

The `FilePlus` icon in the sidebar toolbar is a shortcut for creating an ephemeral HTTP tab. Same behavior as left-clicking `+` in the tab bar — one click, instant blank request.

### Saving Ephemeral Requests

When viewing an unsaved (ephemeral) request, you have two ways to anchor it to a collection:

**Via button:** Click **Save to Collection** in the request toolbar.

**Via keyboard:** Press `Cmd/Ctrl+S`. On a normal saved request this triggers auto-save, but on an ephemeral tab it opens the Save to Collection dialog instead.

The dialog lets you:

1. Enter a **request name** (with filesystem-safe name hint)
2. Choose an **existing collection** from a dropdown
3. Or select **+ New Collection** to create one inline
4. Click **Save**

After saving, the tab title updates, the collection binding is set, the dirty indicator clears, and auto-save activates for future edits. The request appears in the sidebar tree immediately.

| Light Mode | Dark Mode |
|:---:|:---:|
| ![Save to Collection Light](screenshots/18-save-to-collection-light.png) | ![Save to Collection Dark](screenshots/18-save-to-collection-dark.png) |

---

## Request Builder

The request builder sits at the top of the editor. Select an HTTP method from the dropdown, type your URL, and click **Send** or press `Cmd/Ctrl+Enter`.

The URL bar supports:

- **Path parameters** — type `:paramName` and values appear in the Params tab
- **Variable highlighting** — `{{variable}}` tokens are visually highlighted
- **Query parameters** — `?key=value` pairs are parsed into the Params tab

| Light Mode | Dark Mode |
|:---:|:---:|
| ![Request Builder Light](screenshots/02-request-builder-light.png) | ![Request Builder Dark](screenshots/02-request-builder-dark.png) |

---

## Query and Path Parameters

The **Params** tab has two sections:

### Path Parameters

Auto-extracted from `:param` segments in the URL. Keys are read-only (they come from the URL structure). Only values and the enable/disable toggle are editable. No add/remove buttons — modify the URL to change path params.

### Query Parameters

Full add/remove/toggle capability. Each parameter has a name, value, and enabled checkbox. Editing parameters here automatically updates the URL bar, and vice versa.

| Light Mode | Dark Mode |
|:---:|:---:|
| ![Params Editor Light](screenshots/03-params-editor-light.png) | ![Params Editor Dark](screenshots/03-params-editor-dark.png) |

---

## Headers

The **Headers** tab shows a table with column headers (Name, Value) and each header has an enable/disable checkbox. Common headers like `Content-Type` and `Authorization` are set automatically based on your body mode and auth settings.

| Light Mode | Dark Mode |
|:---:|:---:|
| ![Headers Editor Light](screenshots/04-headers-editor-light.png) | ![Headers Editor Dark](screenshots/04-headers-editor-dark.png) |

---

## Request Body

The **Body** tab supports multiple content types:

- **JSON** — Full Monaco editor with syntax highlighting, code folding, and bracket matching
- **XML** — Monaco editor with XML syntax support
- **Text** — Plain text editor
- **Form Data** — Key-value pairs sent as `multipart/form-data`
- **Binary** — File upload from your local filesystem
- **None** — No request body (default for GET requests)

| Light Mode | Dark Mode |
|:---:|:---:|
| ![Body Editor Light](screenshots/05-body-editor-light.png) | ![Body Editor Dark](screenshots/05-body-editor-dark.png) |

---

## Authentication

The **Auth** tab supports multiple authentication methods:

- **None** — No authentication
- **Basic** — Username and password, sent as a Base64-encoded `Authorization` header
- **Bearer** — Token-based auth with a configurable prefix
- **API Key** — Custom key-value pair added to headers or query parameters
- **OAuth 2.0** — Supports Client Credentials, Password, and Authorization Code flows with PKCE
- **AWS Signature V4** — Signs requests with your AWS access key, secret, region, and service

Collections can define default auth that applies to all requests unless overridden at the request level.

| Light Mode | Dark Mode |
|:---:|:---:|
| ![Auth Editor Light](screenshots/06-auth-editor-light.png) | ![Auth Editor Dark](screenshots/06-auth-editor-dark.png) |

---

## Response Viewer

After sending a request, the response panel shows:

**Status bar** with color-coded indicators:
- Status code badge (green for 2xx, yellow for 3xx, red for 4xx/5xx)
- Response time (green <=200ms, yellow 200-1000ms, red >1s)
- Response size

**Tabs:**
- **Pretty** — Formatted response body in a read-only Monaco editor
- **Raw** — Unformatted response body
- **Preview** — HTML preview in a sandboxed iframe (Safe mode by default, Developer mode available)
- **Headers** — Searchable response headers table with copy-to-clipboard

| Light Mode | Dark Mode |
|:---:|:---:|
| ![Response Pretty Light](screenshots/07-response-pretty-light.png) | ![Response Pretty Dark](screenshots/07-response-pretty-dark.png) |

---

## Environments

Environments let you define variables (like `BASE_URL`, `API_KEY`) that are resolved in URLs, headers, body content, and auth fields using `{{variable}}` syntax.

- Switch environments from the dropdown in the header bar
- Variables can be marked as **secret** to hide their values in the UI
- Environment variables override collection variables when both define the same key
- Each environment is stored as a YAML file in the workspace's `environments/` directory

| Light Mode | Dark Mode |
|:---:|:---:|
| ![Environment Switcher Light](screenshots/11-environment-switcher-light.png) | ![Environment Switcher Dark](screenshots/11-environment-switcher-dark.png) |

---

## Git Integration

Rocket has full built-in git support so you can version-control your collections without leaving the app. Every collection directory is a potential git repository. Open the **Git UI** tab from the workspace tabs or click the git branch icon in the toolbar.

The git panel replaces the need for a separate terminal or git GUI — you can stage, commit, push, pull, switch branches, resolve conflicts, and review diffs all within Rocket.

| Light Mode | Dark Mode |
|:---:|:---:|
| ![Git Panel Light](screenshots/13-git-panel-light.png) | ![Git Panel Dark](screenshots/13-git-panel-dark.png) |

### Two-Panel Layout

The git UI uses a resizable split layout:

- **Left panel** — drag the border to resize between 200px and 500px. Contains the collection name with branch selector, a collapsible commit form, the file list (staged + unstaged), and navigation links to commits, stashes, and remotes.
- **Right panel** — shows the landing overview by default. Switches to a file diff, commit history, or stash list when you navigate. A breadcrumb header appears with a **"back to Overview"** button so you always have a way back.

### Staging and Committing

The file list shows two sections (staged files appear first, matching VS Code and Bruno conventions):

1. **Staged Changes** — files ready to commit. Each row shows the filename, a color-coded status label (Added, Modified, Deleted), and a `-` button to unstage on hover.
2. **Unstaged Changes** — modified files not yet staged. Each row shows a trash icon (discard changes) and a `+` icon (stage) on hover.

The section headers have bulk actions: `+` to stage all, `-` to unstage all, and trash to discard all unstaged changes. A file count badge appears next to each section header.

To commit: expand the **Changes** collapsible section, type a commit message, and click **Commit**. The commit form only appears when the Changes section is expanded.

### Branches

Click the branch name in the left panel header to open the branch selector dropdown:

- **Local branches** — click to switch. The current branch shows a checkmark. Hover to reveal merge (branch icon) and delete (trash icon) buttons.
- **Remote branches** — appear in a "Remote" section below local branches after you fetch. These are branches that exist on the remote but don't have a local counterpart yet. Click any remote branch to automatically create a local tracking branch and switch to it. Remote branches that already have a local equivalent are hidden.
- **Create branch** — type a name in the input at the bottom and press Enter.
- **Search** — filter branches by typing in the search input at the top.

| Light Mode | Dark Mode |
|:---:|:---:|
| ![Branch Selector Light](screenshots/15-branch-selector-light.png) | ![Branch Selector Dark](screenshots/15-branch-selector-dark.png) |

### Remote Operations

The landing panel (right side, default view) shows three action buttons with live commit counts:

- **Fetch** — retrieves all refs from the remote and immediately refreshes the branch list and ahead/behind counts. The button shows "Fetch" with a refresh icon.
- **Pull** — pulls changes from the remote. If your working tree has uncommitted changes, a dialog asks whether to stash them first ("Stash & Pull"), pull anyway, or cancel. Shows "Pull ↓N" when behind by N commits.
- **Push** — pushes local commits to the remote. If you haven't fetched recently or are behind the remote, a safety dialog suggests fetching first. Shows "Push ↑N" when ahead by N commits.

Below the buttons, a status line shows "↑ N Ahead | ↓ N Behind" and a branch status badge ("Your branch is up to date", "N commits behind", or "N commits ahead").

If credentials are needed, a credentials dialog appears automatically for SSH key, username/password, or token authentication.

### Diff Viewer

Click any changed file in the left panel to see its diff in the right panel:

- **Text mode** — a full Monaco side-by-side diff editor showing the old and new versions. Supports syntax highlighting for JSON, YAML, TypeScript, JavaScript, Markdown, XML, HTML, and CSS.
- **Visual mode** — a structured field-by-field comparison specifically designed for YAML request files. Shows which fields changed (method, URL, headers, body, auth) with color-coded added/removed/changed indicators. Available only for `.yml` files.
- **Staged/Working toggle** — switch between viewing the working tree diff (uncommitted changes) and the staged diff (changes in the index).

A breadcrumb header at the top shows "← Overview | path/to/file.yml". Click **← Overview** to return to the landing panel.

| Light Mode | Dark Mode |
|:---:|:---:|
| ![Git Diff Light](screenshots/14-git-diff-light.png) | ![Git Diff Dark](screenshots/14-git-diff-dark.png) |

### Conflict Resolution

When a merge or pull results in conflicts, the conflict resolver opens automatically:

- **Side-by-side view** — "Ours" (your version) on the left, "Theirs" (incoming version) on the right, both in read-only Monaco editors.
- **Resolution options** — click **Accept Ours**, **Accept Theirs**, or **Edit Manually** to open a full editor where you can craft the merged result.
- **Abort** — click **Abort Merge** to cancel the merge entirely and return to the pre-merge state.

After resolving, the file is automatically staged and the conflict marker is cleared.

---

## Load Testing

Rocket includes a built-in load testing feature that lets you stress-test any API endpoint directly from the request editor. No external tools needed — configure concurrency, hit Run, and see latency percentiles in seconds.

### Running a Load Test

1. Open any saved or ephemeral request in the editor
2. Click the **Zap** (lightning bolt) icon in the toolbar, next to the Send button
3. The Load Test dialog opens with two settings:
   - **Concurrent requests** — select from 1, 5, 10, 25, 50, or 100 simultaneous connections. Start low and increase to find your API's breaking point.
   - **Total requests** — enter the total number of requests to send (default: 100). The load tester distributes these across the concurrent connections using a semaphore-based throttle.
4. Click **Run** — the dialog shows a spinner while the test executes
5. Results appear when all requests complete

The request is sent exactly as configured in the editor — same URL, method, headers, body, and auth. Environment variables are NOT resolved in load test mode (the request is sent as-is).

### Understanding Results

The results panel shows a grid of statistics:

**Counts:**
- **Total** — number of requests attempted
- **Succeeded** — requests that received an HTTP response (any status code)
- **Failed** — connection errors, timeouts, or other transport failures (shown in red if > 0)

**Latency (milliseconds):**
- **Min** — fastest response time
- **Avg** — mean response time across all successful requests
- **Max** — slowest response time
- **P50** — median (50th percentile) — half of all requests were faster than this
- **P95** — 95th percentile — 95% of requests were faster than this. This is the most important metric for understanding tail latency.
- **P99** — 99th percentile — only 1% of requests were slower

**Throughput:**
- **Req/sec** — successful requests per second (higher is better)
- **Duration** — total wall-clock time from first to last request

### Tips

- Start with low concurrency (5-10) to establish a baseline, then increase
- Compare P50 vs P95 — a large gap indicates inconsistent response times
- A high failure count at high concurrency may indicate server rate limiting
- The load test uses a fresh HTTP connection per request (no connection pooling)

| Light Mode | Dark Mode |
|:---:|:---:|
| ![Load Test Light](screenshots/16-load-test-light.png) | ![Load Test Dark](screenshots/16-load-test-dark.png) |

---

## Themes

Rocket supports light and dark themes with full consistency across every component.

### Toggling

Click the **Sun** (in dark mode) or **Moon** (in light mode) icon in the **bottom-left corner of the status bar**. The toggle is always accessible regardless of which tab or view is active.

### Behavior

- **First launch** — Rocket reads your system preference (`prefers-color-scheme`) and applies it automatically.
- **Manual toggle** — your choice is saved to `localStorage` and persisted across sessions.
- **Monaco editor** — uses custom `rocket-light` and `rocket-dark` themes that are defined before any editor mounts, ensuring no flash of wrong theme. All Monaco instances (request body editor, response viewer, diff editor, conflict resolver) share the same theme and update in real-time when you toggle.
- **Editor skeleton** — while Monaco is loading, a skeleton placeholder matches the current theme's background color to prevent visual flicker.

---

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Cmd/Ctrl+Enter` | Send active request |
| `Cmd/Ctrl+S` | Save request (or Save to Collection if unsaved) |
| `Cmd/Ctrl+W` | Close active tab |
| `Cmd/Ctrl+Tab` | Next tab (wraps) |
| `Cmd/Ctrl+Shift+Tab` | Previous tab (wraps) |
| `Cmd/Ctrl+1-9` | Jump to tab by index |

---

## Data Storage

All data is stored locally on your filesystem:

```
~/.rocket-api/
  workspaces.yml              # Workspace registry
  <workspace-name>/
    workspace.yml              # Workspace config
    collections/               # Collection directories with .yml request files
    environments/              # Environment .yml files
    history/                   # Request execution history
    cookies/                   # Cookie jars
    templates/                 # Saved request templates
```

No data is ever sent to the cloud. Share collections with your team via git.
