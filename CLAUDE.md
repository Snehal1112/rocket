# CLAUDE.md

Guidance for Claude Code when working in this repository.

## What This Is

A Postman/Insomnia-like HTTP API client desktop app built with Tauri (Rust backend + React/TypeScript frontend). Supports collections, environments, request history, git integration, and multi-workspace management. Data is stored locally at `~/.rocket-api/`.

## Commands

```bash
# Dev
yarn tauri dev            # full desktop + Vite HMR
yarn dev                  # frontend only (http://localhost:1420)

# Checks
yarn tsc --noEmit         # TypeScript
yarn check                # Biome lint + format (read-only)
yarn lint / yarn format   # auto-fix variants
yarn test [pattern]       # Vitest
cargo check               # fast Rust validation
cargo test -p <crate> <test_name>
yarn tauri build          # release build
```

## Architecture

### Crate Layout (DDD)

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
| `rocket-import` | Bruno importer — parses `.bru`/`.yml`, converts to domain types |
| `src-tauri` | Tauri IPC commands, app initialization, managed state |

Per-crate design rules live in `crates/*/CLAUDE.md` and load automatically when you touch files in that crate.

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

## Rules and conventions

Rules live in `.claude/rules/` — one file per concern, numbered by read order. Start with [`.claude/rules/00-shortcuts.md`](.claude/rules/00-shortcuts.md) for a pointer map, then read the relevant rule file for your task. See [`.claude/rules/README.md`](.claude/rules/README.md) for the full index.

## Frontend-specific notes

See `.claude/frontend.md` for Zustand stores, tab system, keyboard shortcuts, sandbox mode, and UI state persistence. See `.claude/tauri-commands.md` for IPC command modules and service wiring.
