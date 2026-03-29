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

# Run a single test in a crate
cargo test -p rocket-app <test_name>

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
