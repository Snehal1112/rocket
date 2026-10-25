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
- **Multiple protocols** — HTTP, GraphQL (schema introspection and subscriptions), gRPC, and WebSocket requests in the same collection tree
- **Import from Postman, Bruno or WSDL** — collections, environments, and zipped exports, with auto-detection and optional new-workspace creation
- **Full git integration** — staging, commits, branches, merges, push/pull/fetch (force-with-lease), stash, remote management, SSH key discovery, and real per-file conflict resolution, all via `libgit2` (no shell-outs)
- **Collection Runner** — execute a whole collection or folder sequentially with live streamed progress and a results summary. History entries carry a run-source badge (manual, runner, load test, flow, agent) you can filter on
- **Scripting & testing** — pre-request/post-response/test scripts in Monaco with real TypeScript IntelliSense for the `rok.*` API, a snippet sidebar, shared `.js` script files you can load with a local `require()`, plus a visual no-code assertions builder for scriptless checks
- **Flow** — a visual workflow builder on a canvas. Wire saved requests together with Input, If, Switch, Transform, Auth, Wait-for-callback and Output nodes, run a whole flow or just part of it, and review per-step traces and saved run history. Flows are plain YAML files inside the collection
- **AI Assistant** — a workspace chat panel that connects to an agent over ACP (Claude, Gemini, or a generic agent). The agent can read your workspace through MCP tools and propose changes (new requests and folders, script edits, renames, moves, environment variables, folder settings). Nothing is written until you accept each proposal. Conversations are stored per workspace with secrets masked, and agent credentials come from the vault, an environment variable, or the OS keychain
- **Collection trust** — a trust gate shows which capabilities a collection requests (such as `{{process.env}}` access) and which you have allowed
- **Folder settings** — a per-folder tab for headers, auth, variables, docs, and scripts, stored in `folder.yml` and inherited by everything beneath the folder
- **Environment variables** with `{{variable}}` template syntax, secret masking, and optional external secret-manager integration (RocketVault and Azure Key Vault) for pulling values from a vault
- **Multi-tab editor** with split panes, auto-save, and keyboard-driven navigation
- **Authentication** — Basic, Bearer, API Key, Digest, NTLM, WSSE, OAuth 1.0, OAuth 2.0 (client credentials, password, authorization code with PKCE, and implicit grants), and AWS SigV4, plus mTLS client certificates and proxy support. Requests and folders can inherit auth from their parent
- **Load testing** — phase-based (ramp-up/hold/ramp-down) or fixed-concurrency/RPS runs, a live dashboard with latency/throughput/error-rate/concurrency charts and percentile stats, and export to HTML/CSV/JSON/PDF
- **Monaco editor** for JSON/XML/text bodies with syntax highlighting and theme sync
- **Contract Lock** — attach SLA/API contracts to collections or folders, track drift with a changelog, preview PDFs natively, and export contracts as OpenAPI YAML
- **Security & compliance audit trail** — tamper-evident, hash-chained event log with configurable compliance profiles and evidence export
- **Light/dark theme** with system preference detection
- **Cross-platform** native desktop app (Linux, macOS, Windows)
- **No cloud, no account** — your data stays on your machine

## Benchmarked vs. Bruno and Postman

Most API clients are Electron apps — they ship a full copy of Chromium with every install. Rocket uses the OS WebView instead (WebKitGTK on Linux, WKWebView on macOS, WebView2 on Windows), so there's no bundled browser engine to ship.

Measured on the same Linux machine on 2026-10-21 with the installed builds:

| | Rocket 2.1.1 | Bruno 4.2.0 | Postman 11.71.7 |
|---|---|---|---|
| **Runtime** | Rust + OS WebView | Electron | Electron |
| **Install size (unpacked on disk)** | **146 MB** | 527 MB | 653 MB |
| **Download size** | **48.4 MB** (.deb) | not measured | 174 MB (snap) |
| **Idle memory (PSS)** | **~262 MB** (3 processes) | ~566 MB (7 processes) | ~516 MB (7 processes) |
| **Idle private memory** | **151 MB** | 480 MB | 311 MB |
| **Idle CPU** | ~1% | **~0%** | ~7% |
| **Startup → window visible** | 0.43 s (blank) | ~3.5 s | **~1.0 s** |
| **Startup → UI drawn** | ~10.4 s | ~4.2 s | **~2.2 s** |

**How it was measured:** idle memory is PSS (proportional set size) summed across each app's full process tree, with no requests sent. PSS avoids double-counting pages shared between an app's own processes. The apps ran one after the other. Postman was already signed in. "Startup → UI drawn" is the first moment the app window shows real content, not just a blank window or a flat loading screen. Full method and per-run numbers are in [docs/engineering/benchmarks.md](docs/engineering/benchmarks.md).

**Takeaways:**

- **Rocket uses about half the memory of either Electron app** (~262 MB against ~516 MB and ~566 MB, with 3 processes against 7) and takes about 3.6–4.5x less disk space once installed.
- **Startup is Rocket's weak spot.** The UI is drawn after ~10.4 s, against ~4.2 s for Bruno and ~2.2 s for Postman.
- **Postman's idle CPU is higher** (~7%), most likely background sync because it was signed in.

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
cargo check -j4

# Frontend tests
yarn test

# Rust tests (target a crate; avoid --workspace)
cargo test -j4 -p <crate>
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
| `rocket-http` | HTTP executor, auth schemes, WebSocket, load testing |
| `rocket-grpc` | gRPC engine: `.proto` parsing and protobuf JSON conversion |
| `rocket-flow` | Flow workflow model, validation and lint |
| `rocket-scripting` | JS scripting contract (`ScriptEngine` trait and script types) |
| `rocket-acp` | AI Assistant: ACP agent sessions, proposals, stored conversations |
| `rocket-audit` | Tamper-evident audit log |
| `rocket-git` | Git operations via libgit2 |
| `rocket-app` | Orchestration services |
| `rocket-infra` | Filesystem implementations |
| `rocket-import` | Bruno, Postman and WSDL importer |
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
        flows/
          onboarding.yml       # Flow definitions
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
    assistant/                 # AI Assistant panel, composer, proposals
    audit/                     # Security/compliance audit log UI
    collections/               # Collection sidebar tree, folder settings
    contract/                  # Contract status badge
    contracts/                 # Contract Lock workflow (list, diff, changelog, export)
    editor/                    # Monaco wrapper, themes, scripting IntelliSense
    environments/              # Environment editor + external secret-manager binding
    flow/                      # Flow canvas, node types, run history
    git/                       # Git UI panel
    grpc/                      # gRPC request UI
    history/                   # Request history panel
    import/                    # Postman/Bruno import dialog
    layout/                    # App shell, sidebar, status bar
    panes/                     # Tab system and editor groups
    request/                   # Request editor, params, auth, body, scripts, runner, load testing
    response/                  # Response viewer
    scripts/                   # Shared script files
    settings/                  # External secret-manager connections and agents
    ui/                        # shadcn/ui primitives
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
  rocket-import/                # Bruno, Postman and WSDL importers
  rocket-grpc/
  rocket-flow/
  rocket-scripting/
  rocket-acp/
  rocket-audit/
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
