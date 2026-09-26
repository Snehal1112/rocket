<p align="center">
  <img src="public/rocket.png" alt="Rocket" width="80" />
</p>

<h1 align="center">Rocket</h1>

<p align="center">
  Modern API testing workspace inspired by Bruno. Fast native desktop app with file-based collections, git integration, and offline-first design.
</p>

<p align="center">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-Tauri_2-orange?logo=rust" />
  <img alt="React" src="https://img.shields.io/badge/React_19-TypeScript-blue?logo=react" />
  <img alt="Platform" src="https://img.shields.io/badge/Platform-Linux_macOS_Windows-green" />
  <img alt="Offline" src="https://img.shields.io/badge/Offline-First-purple" />
  <img alt="Memory" src="https://img.shields.io/badge/Runtime-Tauri_%2B_WebKitGTK-brightgreen" />
  <img alt="License" src="https://img.shields.io/badge/License-MIT-yellow" />
</p>

---

## Features

- **File-based collections** stored as OpenCollection YAML on disk
- **Multi-workspace** support with embedded and external collections
- **Import from Postman or Bruno** — collections, environments, and zipped exports, with optional new-workspace creation
- **Full git integration** — staging, commits, branches, merges, push/pull/fetch (force-with-lease), stash, remote management, SSH key discovery, and real per-file conflict resolution, all via `libgit2` (no shell-outs)
- **Collection Runner** — execute a whole collection or folder sequentially with live streamed progress and a results summary
- **Scripting & testing** — pre-request/post-response/test scripts in Monaco with real TypeScript IntelliSense for the request/response API, a snippet sidebar, plus a visual no-code assertions builder for scriptless checks
- **Environment variables** with `{{variable}}` template syntax, secret masking, and optional external secret-manager integration for pulling values from a vault
- **Multi-tab editor** with split panes, auto-save, and keyboard-driven navigation
- **Authentication** — Basic, Bearer, API Key, OAuth 2.0 (client credentials grant), and AWS SigV4
- **Load testing** — phase-based (ramp-up/hold/ramp-down) or fixed-concurrency/RPS runs, a live dashboard with latency/throughput/error-rate/concurrency charts and percentile stats, and export to HTML/CSV/JSON/PDF
- **Monaco editor** for JSON/XML/text bodies with syntax highlighting and theme sync
- **Contract Lock** — attach SLA/API contracts to collections or folders, track drift with a changelog, preview PDFs natively, and export contracts as OpenAPI YAML
- **Security & compliance audit trail** — tamper-evident, hash-chained event log with configurable compliance profiles and evidence export
- **Light/dark theme** with system preference detection
- **Cross-platform** native desktop app (Linux, macOS, Windows)
- **No cloud, no account** — your data stays on your machine

## Benchmarked vs. Bruno and Postman

Most API clients are Electron apps — they ship a full copy of Chromium with every install. Rocket uses the OS WebView instead (WebKitGTK on Linux, WKWebView on macOS, WebView2 on Windows), so there's no bundled browser engine to ship.

These aren't estimates — they're measured, head-to-head, on the same machine (Rocket 0.9.2 release build vs. locally installed Bruno 4.2.0 and Postman 11.71.7):

| | Rocket | Bruno | Postman |
|---|---|---|---|
| **Runtime** | Rust + OS WebView | Electron | Electron |
| **Install size** | **38.9 MB** (.deb) | 527 MB | 653 MB |
| **Startup → window visible** | **6.38 s** | 7.86 s | **0.99 s** |
| **Idle memory (PSS)** | ~332 MB (3 processes) | **169.6 MB** (6 processes) | 507.7 MB (8 processes) |

**Methodology:** idle memory is PSS (proportional set size, via `/proc/<pid>/smaps_rollup`) summed across each app's full process tree, measured ~15s after its window became viewable with no requests sent — PSS avoids double-counting memory pages shared between an app's own forked processes, which a naive RSS sum would inflate. Startup is wall-clock time from process launch to the app's main window reaching `IsViewable` with real dimensions (verified via `xdotool`/`xwininfo`, filtering out transient placeholder windows some of these apps create first) — not a "process launched" proxy. Rocket's figure was measured in an isolated `systemd-run --user --scope` (`memory.high=max`, confirmed 0 bytes swapped), flat across 20s–180s of idle time; Bruno and Postman were measured back-to-back on the same machine without that isolation, so their numbers carry a smaller version of the caveat below.

**Takeaways:**

- **Rocket still ships far smaller** (~14–17x) than either Electron competitor, and starts faster than Bruno.
- **A previous "171 MB" figure for Rocket was a measurement artifact, not a real number.** All development on this machine runs inside a shared cgroup (`memory.high` capped at 2 GB) alongside the tooling doing the measuring, concurrent builds, and other processes. Under that pressure the kernel reclaims and swaps out pages, and PSS only counts resident memory — so a reading taken mid-pressure can come in dramatically lower than the app's real footprint. We proved this directly: the *same running Rocket instance*, with zero user activity, read 292 MB and then 159 MB twenty seconds later purely from swap, with no code change involved. Measured outside that cgroup, Rocket is flat at **~332 MB** from 20 seconds to 180 seconds of idle time, with zero bytes swapped — this is the real number. The original 171 MB figure was a single sample taken immediately after a release build, inside the same pressured cgroup; an empty, freshly-installed Rocket with no data at all already needs more memory than that once actually measured cleanly.
- **The `WebKitCacheModel::DocumentViewer` and `WEBKIT_DISABLE_COMPOSITING_MODE=1` fixes are still real and still necessary** — with compositing left on, WebKitWebProcess's real footprint (measured the same clean way) balloons to **3.5 GB**, not something visible under cgroup pressure. Keep both.
- **One small, real optimization did come out of this investigation:** the sidebar kept its History panel permanently mounted (rendering up to 200 entries, ~2,000 DOM nodes — 77% of the app's entire DOM) even when the user was looking at the Collections tab. It's now deferred until the History tab is actually opened.
- **Startup times used real window-visibility detection** (via `xdotool`/`xwininfo`, filtering decoy placeholder windows) rather than whatever weaker proxy an earlier pass used — this moved Bruno's number up (2.05s → 7.86s) and Postman's down (4.09s → 0.99s) versus previously-published figures, which says more about the old methodology's inconsistency across apps than about either app changing.
- **Postman required signing in** (via an external OAuth browser flow) during an earlier measurement pass, spawning a separate Brave browser process that was excluded from its footprint; the run reported here reached its idle state without triggering that flow. Bruno and Rocket both work fully offline with no account.

**How Rocket stays small:**

- **Tauri uses the OS WebView** — WebKitGTK on Linux, WKWebView on macOS, WebView2 on Windows — instead of bundling Chromium.
- **Rust backend** — no garbage collector, no JVM, no Node runtime. Services are small structs wired via trait objects and dropped when not needed.
- **No cloud, no polling** — there is no background process phoning home. Every I/O operation is triggered by user action, against local files only.

## Quick Start

### Prerequisites

- [Node.js](https://nodejs.org/) 18+
- [Yarn](https://yarnpkg.com/)
- [Rust](https://www.rust-lang.org/tools/install) (stable toolchain)
- Tauri 2 system dependencies ([see guide](https://v2.tauri.app/start/prerequisites/))

### Development

```bash
# Install frontend dependencies
yarn install

# Full Tauri dev mode (launches desktop window + Vite HMR)
yarn tauri dev

# Frontend only (Vite server at http://localhost:1420)
yarn dev
```

### Build

```bash
# Production build
yarn tauri build

# Frontend only
yarn build
```

### Checks

```bash
# TypeScript
yarn tsc --noEmit

# Rust
cargo check

# Frontend tests
yarn test

# Rust tests
cargo test
```

## Architecture

```
Frontend (React 19)  -->  Tauri IPC  -->  Rust Services  -->  Filesystem (~/.rocket-api/)
```

### Crate Layout

| Crate | Role |
|---|---|
| `rocket-shared` | Common types, errors, events |
| `rocket-collection` | Collection/folder/request domain model + contract types |
| `rocket-environment` | Environment variables and `{{var}}` resolution |
| `rocket-history` | Request execution history |
| `rocket-workspace` | Workspace domain model |
| `rocket-http` | HTTP executor, auth schemes, load testing |
| `rocket-git` | Git operations via libgit2 |
| `rocket-app` | Orchestration services |
| `rocket-infra` | Filesystem implementations |
| `rocket-import` | Bruno & Postman collection importer |
| `src-tauri` | Tauri commands and app initialization |

### Frontend Stack

| Technology | Purpose |
|---|---|
| React 19 + TypeScript 5.8 | UI framework |
| Zustand 5.0 | State management |
| shadcn/ui + Radix UI | Component library |
| TailwindCSS 4.2 | Styling |
| Monaco Editor | Code editing |
| Lucide React | Icons |

### Data Storage

```
~/.rocket-api/
  workspaces.yml              # Workspace registry
  My Workspace/
    workspace.yml              # Workspace config
    collections/
      my-api/
        opencollection.yml     # Collection settings
        get-users.yml          # Request files
        auth/
          login.yml
        .rocket/
          contracts/
            <id>.yml           # Contract definition (git-tracked)
            attachments/
              <id>/
                document.pdf  # Attached files (max 2 MB each)
    environments/
      production.yml
      staging.yml
    history/
    cookies/
```

## Contract Lock

Contract Lock lets you attach formal agreements (SLAs, API contracts) to a collection or any folder within it, track changes over time, and surface violations directly in your workspace.

### Key capabilities

- Attach one or more reference documents (PDF, DOCX, TXT, Markdown, PNG/JPG) up to 2 MB each
- Scope a contract to the entire collection, a specific folder, or a single request
- Track provider, consumer, project, version, effective date, and optional expiry
- Automatic status badges: **Active**, **Expiring soon** (≤ 30 days), **Expired**
- Changelog records API drift against the locked baseline
- Click a `.pdf` attachment to open it in a native preview window
- Attachment files are copied into `.rocket/contracts/attachments/<id>/` inside the collection — fully git-portable, deleted automatically when the contract is removed

### Storage

Contracts are stored as YAML files under `.rocket/contracts/` inside the collection directory. Because the `.rocket/` folder lives alongside your request files, it is committed alongside them in any git workflow.

## Project Structure

```
src/                           # React frontend
  components/
    audit/                     # Security/compliance audit log UI
    collections/               # Collection sidebar tree
    contract/                  # Contract status badge
    contracts/                 # Contract Lock workflow (list, diff, changelog, export)
    editor/                    # Monaco wrapper, themes, scripting IntelliSense
    environments/              # Environment editor + external secret-manager binding
    git/                       # Git UI panel
    history/                   # Request history panel
    import/                    # Postman/Bruno import dialog
    layout/                    # App shell, sidebar, status bar
    panes/                     # Tab system and editor groups
    request/                   # Request editor, params, auth, body, scripts, runner, load testing
    response/                  # Response viewer
    settings/                  # External secret-manager connections
    workspace/                 # Workspace overview, environments
  hooks/                       # Custom React hooks
  lib/                         # Utilities, Tauri API bridge
  stores/                      # Zustand stores
  types/                       # TypeScript type definitions
crates/                        # Rust backend
  rocket-shared/
  rocket-collection/
  rocket-environment/
  rocket-history/
  rocket-workspace/
  rocket-http/
  rocket-git/
  rocket-app/
  rocket-infra/
  rocket-import/                # Bruno & Postman importers
src-tauri/                     # Tauri shell
  src/
    commands/                  # IPC command handlers
    lib.rs                     # App initialization
docs/
  manual/                      # User manual with screenshots
```

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Cmd/Ctrl+Enter` | Send request |
| `Cmd/Ctrl+S` | Save request / Save to Collection |
| `Cmd/Ctrl+W` | Close tab |
| `Cmd/Ctrl+Tab` | Next tab |
| `Cmd/Ctrl+Shift+Tab` | Previous tab |
| `Cmd/Ctrl+1-9` | Jump to tab by index |
| `Cmd/Ctrl+L` | Open Contracts tab |
| `Cmd/Ctrl+Shift+G` | Open git panel |

## Documentation

- [User Manual](docs/manual/README.md) — how to use the app
- [Architecture](CLAUDE.md) — codebase guide for contributors
- Crate-level docs in each `crates/*/CLAUDE.md`

## License

MIT
