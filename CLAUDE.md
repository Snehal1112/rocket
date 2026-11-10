# CLAUDE.md

Guidance for Claude Code when working in this repository.

## What This Is

A Postman/Insomnia-like HTTP API client desktop app built with Tauri (Rust backend + React/TypeScript frontend). Supports collections, environments, request history, git integration, and multi-workspace management. Data is stored locally at `~/.rocket-api/`.

## Commands

```bash
# Dev
bun run tauri dev            # full desktop + Vite HMR
bun run dev                  # frontend only (http://localhost:1420)

# Checks
bun run tsc --noEmit         # TypeScript
bun run check                # Biome lint + format (read-only)
bun run lint / bun run format   # auto-fix variants
bun run test [pattern]       # Vitest
cargo check -j4            # fast Rust validation
cargo test -j4 -p <crate> <test_name>
bun run tauri build          # release build
```

## Architecture

### Crate Layout (DDD)

| Crate | Role |
|---|---|
| `rocket-shared` | `DomainError`, `DomainResult`, events, HTTP primitives |
| `rocket-collection` | Collection/folder/request model + `CollectionRepository` trait |
| `rocket-environment` | Environment model, `{{var}}` resolution via `resolve()` |
| `rocket-history` | Request execution history |
| `rocket-workspace` | Workspace model + `WorkspaceRepository` trait |
| `rocket-http` | `HttpExecutor` trait, `ReqwestExecutor`, auth, cookies |
| `rocket-git` | `GitService` trait + `Git2Service` (libgit2) |
| `rocket-network` | Team network domain: device id, pairing, presence, peer messages, ports (pure, no I/O) |
| `rocket-openapi` | OpenAPI sync context: events and view DTOs (service and apply move here in later plans) |
| `rocket-app` | Orchestration — wires domain traits, no I/O |
| `rocket-infra` | Filesystem impls of all repository/service traits |
| `rocket-import` | Bruno importer (`.bru`/`.yml` → domain types) |
| `src-tauri` | Tauri IPC commands, app init, managed state |

Per-crate rules in `crates/*/CLAUDE.md` — load automatically when you touch files there.

### Data Flow

```
Frontend (React) → Tauri command → rocket-app service → rocket-infra repo → filesystem
```

- `rocket-app` holds `Box<dyn Trait>` only; `rocket-infra` wired at startup in `src-tauri/src/lib.rs`.
- Environment vars override collection vars before HTTP dispatch (`rocket_environment::resolve()`).
- Per-context events (`CollectionEvent`, `AcpEvent`, ...) published fire-and-forget; `TauriEventBus` in prod, `NullEventPublisher` in tests.
- Tests: `tempfile` for fs fixtures, `wiremock` for HTTP mocking. `cargo check -j4` for fast validation.

## Rules and conventions

Rules in `.claude/rules/` — start with `.claude/rules/00-shortcuts.md` for a pointer map.
See `.claude/frontend.md` for Zustand/tabs/UI state. See `.claude/tauri-commands.md` for IPC modules.
See `.claude/docs-index.md` for the feature and engineering docs (script files, folder settings, assistant, collection trust, masking, ACP, export, import, OpenAPI sync, ACL, settings, elevation, DDD rulings, benchmarks, subagents, landing deploy, factory).
See `docs/roadmaps/roadmap.md` for the master roadmap of unimplemented work, and `docs/roadmaps/flow-roadmap.md` for Flow (item IDs, status, batches). Update them when work ships.

Delegation: for asks that mix sub-steps of different difficulty, use the `decompose-and-dispatch` skill and always state the subagent model. See `.claude/rules/delegation.md`.

---

## OpenCollection Spec Reference — Injection Rule

When writing a plan using the `writing-plans` skill, inspect every task before finalising it.

**If the task touches ANY of the following:**
- `.yml` files (read or write)
- Rust backend crates (`rocket-collection`, `rocket-environment`, `rocket-http`, `rocket-import`, `rocket-workspace`, `rocket-infra`)
- `FsCollectionRepo` or `FsEnvironmentRepo`
- Collection, folder, request, or environment data models
- Variable resolution or scope logic
- Auth configuration
- Any Tauri IPC command that deals with collections or environments

**Then prepend this line as the first step of that task:**

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

This applies to every affected task in the plan — not just the first one.

---

## Hard Rules (always enforced)

- All UI components use **shadcn/ui primitives only** — no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`
- Icons: `lucide-react` only — no inline SVGs
- Single-line variable-aware fields: `SingleLineEditor` (CodeMirror 6) — never Monaco
- Multi-line editors: Monaco only. One approved exception: the AI Assistant prompt editor (`PromptEditor`, CodeMirror 6) — see `.claude/rules/frontend-component-guardrails.md`
- Zustand: never fully destructure store state at component top level
- Rust: never `unwrap()` in production paths, never shell out to `git` CLI (use `git2` crate)
- Commits: conventional commits format (`feat:`, `fix:`, `chore:`, etc.)
- Serde: `#[serde(rename_all = "camelCase")]` on IPC DTOs only — never on persistence structs
