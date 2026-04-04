# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

A Postman/Insomnia-like HTTP API client desktop app built with Tauri (Rust backend + React/TypeScript frontend). Supports collections, environments, request history, git integration, and multi-workspace management. Data is stored locally at `~/.rocket-api/`.

## Commands

### Development

```bash
# Full Tauri dev mode (recommended — launches desktop window + Vite HMR)
yarn tauri dev

# Frontend only (Vite server at http://localhost:1420)
yarn dev

# TypeScript check
yarn tsc --noEmit

# Lint & format
yarn check          # lint + format check (read-only)
yarn lint           # auto-fix lint issues
yarn format         # auto-format

# Frontend tests (Vitest — all)
yarn test

# Frontend tests (single file or pattern)
yarn test src/stores/pane-store
```

### Rust

```bash
cargo check                              # fast validation
cargo build --release                    # full build
cargo test -p <crate-name> <test_name>  # single test
yarn tauri build                         # full Tauri release
```

### Before Opening a PR

```bash
yarn tsc --noEmit   # TypeScript check
yarn check          # Biome lint + format
yarn build          # Ensure frontend builds
cargo check         # Rust check
cargo test          # Rust tests
```

## Architecture

### Crate Layout (Domain-Driven Design)

| Crate | Role |
|---|---|
| `rocket-shared` | Common types: `DomainError`, `DomainResult`, events, HTTP primitives |
| `rocket-collection` | Collection/folder/request domain model + `CollectionRepository` trait |
| `rocket-environment` | Environment model, `{{var}}` resolution via `resolve()` |
| `rocket-history` | Request execution history with filtering |
| `rocket-workspace` | Workspace domain model + `WorkspaceRepository` trait |
| `rocket-http` | `HttpExecutor` trait, `ReqwestExecutor`, auth schemes, cookies |
| `rocket-git` | `GitService` trait + `Git2Service` (libgit2) |
| `rocket-app` | Orchestration services — wires domain traits, no I/O |
| `rocket-infra` | Filesystem implementations of all repository/service traits |
| `rocket-import` | Bruno API client importer — parses `.bru`/`.yml`, converts to domain types, writes via `rocket-infra` |
| `src-tauri` | Tauri IPC commands, app initialization, managed state |

### Data Flow

```
Frontend (React) → Tauri command → rocket-app service → rocket-infra repo → filesystem
```

- `rocket-app` services hold `Box<dyn Trait>` only — no concrete impls, fully testable.
- `rocket-infra` provides concrete impls wired at startup in `src-tauri/src/lib.rs`.
- Active workspace path lives in `Arc<Mutex<PathBuf>>` shared across all services.
- Environment variables override collection variables before HTTP dispatch (`rocket_environment::resolve()`).
- `DomainEvent` is published fire-and-forget via `EventPublisher`; `TauriEventBus` in prod, `NullEventPublisher` in tests.

### Key Patterns

- All Rust service methods return `DomainResult<T>` (`Result<T, DomainError>`).
- Repository traits defined in domain crates; implementations only in `rocket-infra`.
- Tests use `tempfile` for filesystem fixtures and `wiremock` for HTTP mocking.
- `cargo check` is sufficient for most Rust validation; full compilation is slow.

## Detailed Documentation

### Frontend
- [`.claude/frontend.md`](.claude/frontend.md) — Zustand stores, tab system, keyboard shortcuts, sandbox mode, UI state persistence

### Tauri / Backend Bridge
- [`.claude/tauri-commands.md`](.claude/tauri-commands.md) — IPC command modules and service wiring

### Per-Crate Design Rules
- [`crates/rocket-shared/CLAUDE.md`](crates/rocket-shared/CLAUDE.md) — `DomainError`, `DomainResult`, `DomainEvent`, serde conventions
- [`crates/rocket-collection/CLAUDE.md`](crates/rocket-collection/CLAUDE.md) — Collection aggregate, `Request` field semantics, serde rules
- [`crates/rocket-environment/CLAUDE.md`](crates/rocket-environment/CLAUDE.md) — `Environment`, `Variable`, `resolve()` template engine
- [`crates/rocket-workspace/CLAUDE.md`](crates/rocket-workspace/CLAUDE.md) — Workspace model, `WorkspaceConfig`, `multi_workspace_mode`
- [`crates/rocket-app/CLAUDE.md`](crates/rocket-app/CLAUDE.md) — Services, variable resolution, header/auth merging, event patterns
- [`crates/rocket-infra/CLAUDE.md`](crates/rocket-infra/CLAUDE.md) — OpenCollection YAML format, path validation, migration, `SharedPathCollectionRepo`; serde layer (`opencollection`/`oc_conversions` modules), `OcAuth`/`OcItem` untagged serde gotchas
- [`crates/rocket-git/CLAUDE.md`](crates/rocket-git/CLAUDE.md) — `GitService` trait, `Git2Service`
- [`crates/rocket-history/CLAUDE.md`](crates/rocket-history/CLAUDE.md) — History storage and filtering
- [`crates/rocket-http/CLAUDE.md`](crates/rocket-http/CLAUDE.md) — `HttpExecutor`, auth schemes, cookie handling
- [`crates/rocket-import/CLAUDE.md`](crates/rocket-import/CLAUDE.md) — Bruno importer: `.bru`/`.yml` parsing pipeline, converters, `ImportService`, fixture layout
