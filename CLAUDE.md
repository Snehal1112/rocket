# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

A Postman/Insomnia-like HTTP API client desktop app built with Tauri (Rust backend + React/TypeScript frontend). It supports collections, environments, request history, git integration, and multi-workspace management.

## Commands

### Development

```bash
# Full Tauri dev mode (recommended — launches desktop window + Vite HMR)
yarn tauri dev

# Frontend only (Vite server at http://localhost:1420)
yarn dev

# Build frontend only
yarn build

# TypeScript check
yarn tsc --noEmit

# Frontend tests (Vitest)
yarn test
```

### Rust

```bash
# Run from workspace root or a crate directory
cargo check
cargo build
cargo build --release

# Run a single test in any crate
cargo test -p <crate-name> <test_name>

# Full Tauri release build
yarn tauri build
```

### Before Opening a PR

```bash
yarn tsc --noEmit          # TypeScript check
cargo check                # Rust check
yarn build                 # Ensure frontend builds
```

## Architecture

### Crate Layout

The workspace uses Domain-Driven Design across 9 crates + Tauri shell:

| Crate | Role |
|---|---|
| `rocket-shared` | Common types: `DomainError`, `DomainResult`, `Action`, `Assertion`, `VariableValue`, events |
| `rocket-collection` | Collection/folder/request domain model and `CollectionRepository` trait |
| `rocket-environment` | Environment domain model, variable resolution (`resolve()` for `{{var}}` syntax) |
| `rocket-history` | Request execution history with filtering |
| `rocket-workspace` | Workspace domain model and `WorkspaceRepository` trait |
| `rocket-http` | `HttpExecutor` trait + `ReqwestExecutor`, auth (Basic/Bearer/API Key/OAuth2/AWS SigV4), cookies |
| `rocket-git` | `GitService` trait + `Git2Service` for full git operations |
| `rocket-app` | Orchestration services — wires domain traits together (no I/O) |
| `rocket-infra` | Filesystem implementations of all repository/service traits + `NotifyFileWatcher` |
| `src-tauri` | Tauri commands (IPC bridge), app initialization, state management |

### Data Flow

```
Frontend (React) → Tauri command → rocket-app service → rocket-infra repo → filesystem (~/.rocket-api/)
```

- Services in `rocket-app` hold trait objects (`Box<dyn Trait>`), never concrete impls.
- `rocket-infra` provides the concrete impls wired up in `src-tauri/src/lib.rs`.
- The active workspace path is stored in `Arc<Mutex<PathBuf>>` and passed to all services.
- Filesystem layout: `~/.rocket-api/<workspace-name>/{collections,environments,history,templates,cookies}/`

### Variable Resolution

`rocket_environment::resolve()` replaces `{{variable_name}}` placeholders. Environment variables override collection variables. Called in `rocket-app`'s `RequestExecutionService` before HTTP dispatch.

### Events

Services publish `DomainEvent` via an `EventPublisher` trait. `NullEventPublisher` is used in tests; `TauriEventBus` is used in production to push events to the frontend over Tauri's event system.

### Frontend Structure

- **State**: Zustand stores in `src/stores/` (`pane-store`, `workspace-store`, `env-store`, `git-store`, `console-store`)
- **Tauri IPC**: All backend calls go through `src/lib/tauri-api.ts`
- **UI**: shadcn/ui components in `src/components/ui/`, feature components organized by domain (`collections/`, `environments/`, `git/`, `workspace/`, etc.)
- **Pane system**: `pane-store.ts` + `src/components/panes/` implements the split-tab layout

### Key Patterns

- All Rust service methods return `DomainResult<T>` (`Result<T, DomainError>`).
- Repository traits are defined in domain crates; implementations live only in `rocket-infra`.
- `cargo check` is sufficient for most Rust validation; full compilation is slow.
- Tests use `tempfile` for ephemeral filesystem fixtures and `wiremock` for HTTP mocking.

### Tech Stack Versions

- Rust + Tauri 2 (desktop shell)
- React 19 + TypeScript 5.8 (frontend)
- Zustand 5.0 (frontend state management)
- shadcn/ui + Radix UI (component library)
- TailwindCSS 4.2 (styling)
- Monaco Editor (code editing in request/response panels)
- libgit2 via `git2` crate (git operations)
- `reqwest` (HTTP client)

### Tauri Command Modules

Commands live in `src-tauri/src/commands/`:

| Module | Purpose |
|---|---|
| `collections.rs` | CRUD for collections, folders, requests |
| `environments.rs` | CRUD for environments |
| `execution.rs` | HTTP request dispatch |
| `git.rs` | Git operations (status, commit, push/pull, branch, stash) |
| `history.rs` | Request history search and clear |
| `workspaces.rs` | Workspace CRUD, pin/unpin, config management |
| `cookies.rs` | Cookie jar management |
| `oauth2.rs` | OAuth2 token acquisition |
| `templates.rs` | Saved request templates |
| `ui_state.rs` | UI layout and mode persistence across sessions |

### Sandbox Mode

The `ResponseBodyViewer` renders HTML responses in an iframe. `useSandboxStore` (persisted to `localStorage`) holds a `mode: 'safe' | 'developer'`:
- **Safe mode** (default): restricts JavaScript execution in the iframe.
- **Developer mode**: allows full script execution.
Toggled via `SandboxPopover` in the toolbar.

### UI State Persistence

On app launch, `load_ui_state()` (Tauri command) restores the previously active workspace and tab mode. On state changes, `scheduleSaveUiState()` debounces (500 ms) a write via `save_ui_state()`. This ensures the app reopens in the same workspace and mode as last time.

### Tab System

Tab types (`src/types/pane-types.ts`): `RequestTab | CollectionTab | WorkspaceTab | DiffTab | ConflictTab | GitTab`.
- `CollectionTab` sections: `'overview' | 'auth' | 'variables'`.
- `WorkspaceTab` sections: `'overview' | 'environments' | 'git'`. WorkspaceTabs never have a close button and open automatically on workspace switch.
- Guard functions: `isRequestTab()`, `isDiffTab()`, `isConflictTab()`, `isWorkspaceTab()`, `isGitTab()`.
- The `isDirty` flag on request tabs triggers `scheduleAutoSave()` before tab close or switch.
- Mode switching: opening a workspace tab closes all collection tabs (and vice versa).

### Keyboard Shortcuts (`src/hooks/useKeyboardShortcuts.ts`)

| Shortcut | Action |
|---|---|
| Cmd/Ctrl+Enter | Send active request |
| Cmd/Ctrl+S | Save draft (`rocket:save-draft` event) |
| Cmd/Ctrl+W | Close active tab |
| Cmd/Ctrl+Tab | Next tab (wraps) |
| Cmd/Ctrl+Shift+Tab | Previous tab (wraps) |
| Cmd/Ctrl+1–9 | Jump to tab by 1-based index |

### Zustand Stores (`src/stores/`)

| Store | Purpose |
|---|---|
| `pane-store` | Tab groups, active tab, split layout, collection tab state snapshot |
| `workspace-store` | Workspace list, active workspace, subscriptions to workspace events |
| `env-store` | Environments, active environment |
| `git-store` | Git status, staging, commit, push/pull state |
| `console-store` | Request log for the console panel |
| `sandbox-store` | Sandbox mode (`safe` / `developer`), persisted to `localStorage` |

## Crate Documentation

Detailed design rules and domain model notes for individual crates:

- [`crates/rocket-collection/CLAUDE.md`](crates/rocket-collection/CLAUDE.md) — Collection aggregate, `Request` field semantics, `CollectionRepository` trait, serde rules.
- [`crates/rocket-workspace/CLAUDE.md`](crates/rocket-workspace/CLAUDE.md) — Workspace domain model, `WorkspaceConfig`, `CollectionReference`, pinned/description fields, `multi_workspace_mode`.
- [`crates/rocket-shared/CLAUDE.md`](crates/rocket-shared/CLAUDE.md) — `DomainError`, `DomainResult`, `DomainEvent`, `EventPublisher`, HTTP primitives, serde conventions.
- [`crates/rocket-environment/CLAUDE.md`](crates/rocket-environment/CLAUDE.md) — `Environment`, `Variable`, `resolve()` template engine, `EnvironmentRepository` trait.
- [`crates/rocket-app/CLAUDE.md`](crates/rocket-app/CLAUDE.md) — Orchestration services, variable resolution, header/auth merging, event publishing patterns.
- [`crates/rocket-infra/CLAUDE.md`](crates/rocket-infra/CLAUDE.md) — Filesystem implementations, OpenCollection YAML format, path validation, migration, `SharedPathCollectionRepo` pattern.
- [`crates/rocket-git/CLAUDE.md`](crates/rocket-git/CLAUDE.md) — `GitService` trait, `Git2Service` implementation.
- [`crates/rocket-history/CLAUDE.md`](crates/rocket-history/CLAUDE.md) — History storage and filtering.
- [`crates/rocket-http/CLAUDE.md`](crates/rocket-http/CLAUDE.md) — `HttpExecutor` trait, auth schemes, cookie handling.
