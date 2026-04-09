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
  <img alt="License" src="https://img.shields.io/badge/License-MIT-yellow" />
</p>

---

## Features

- **File-based collections** stored as OpenCollection YAML on disk
- **Multi-workspace** support with embedded and external collections
- **Full git integration** with staging, commits, branches, push/pull, stash, and conflict resolution
- **Environment variables** with `{{variable}}` template syntax and secret masking
- **Multi-tab editor** with split panes, auto-save, and keyboard-driven navigation
- **Authentication** support for Basic, Bearer, API Key, OAuth 2.0, and AWS SigV4
- **Load testing** with concurrent request execution and percentile latency stats
- **Monaco editor** for JSON/XML/text bodies with syntax highlighting and theme sync
- **Light/dark theme** with system preference detection
- **Cross-platform** native desktop app (Linux, macOS, Windows)
- **No cloud, no account** — your data stays on your machine

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
| `rocket-collection` | Collection/folder/request domain model |
| `rocket-environment` | Environment variables and `{{var}}` resolution |
| `rocket-history` | Request execution history |
| `rocket-workspace` | Workspace domain model |
| `rocket-http` | HTTP executor, auth schemes, load testing |
| `rocket-git` | Git operations via libgit2 |
| `rocket-app` | Orchestration services |
| `rocket-infra` | Filesystem implementations |
| `rocket-import` | Bruno collection importer |
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
    environments/
      production.yml
      staging.yml
    history/
    cookies/
```

## Project Structure

```
src/                           # React frontend
  components/
    collections/               # Collection sidebar tree
    editor/                    # Monaco wrapper and themes
    git/                       # Git UI panel
    layout/                    # App shell, sidebar, status bar
    panes/                     # Tab system and editor groups
    request/                   # Request editor, params, auth, body
    response/                  # Response viewer
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

## Documentation

- [User Manual](docs/manual/README.md) — how to use the app
- [Architecture](CLAUDE.md) — codebase guide for contributors
- Crate-level docs in each `crates/*/CLAUDE.md`

## License

MIT
