# RocketAPI Tauri 2.0 Migration — Design Spec

**Date:** 2026-03-24
**Status:** Approved

## Goal

Migrate RocketAPI (React + Go backend) into a native Tauri 2.0 desktop application using Full DDD architecture with separate bounded contexts in Rust.

## Architecture Decisions

### Pattern: Full DDD with Rust-adapted idioms
- Traits replace interfaces (repository pattern)
- Enums replace domain events (algebraic data types)
- Ownership model provides natural value objects
- Composition over inheritance

### Bounded Contexts (Cargo workspace — 7 crates for SP1)
1. **rocket-shared** — Error types, domain event enum, shared VOs (HttpMethod, Header, Body)
2. **rocket-collection** — Collection aggregate, Request/Folder VOs, CollectionRepository trait
3. **rocket-environment** — Environment aggregate, Variable VOs, EnvironmentRepository trait, resolver
4. **rocket-http** — HttpRequest/HttpResponse VOs, Auth enum, CookieJar aggregate, HttpExecutor trait
5. **rocket-history** — HistoryEntry + Template aggregates, repository traits
6. **rocket-app** — Application services (use cases), orchestration, dependency injection
7. **rocket-infra** — Filesystem repos, reqwest executor, notify file watcher, TauriEventBus

Plus: `src-tauri` (binary) and `frontend` (React).

### Dependency Rule
Arrows only point downward: src-tauri → rocket-app → domain crates → rocket-shared. rocket-infra implements domain traits (depends on domain crates). Domain crates never import infrastructure.

### Repository Traits
- Local-only for now (simpler traits)
- Designed for filesystem storage in `~/.rocket-api/`
- Can be refactored for cloud sync later without changing domain layer

### Application Services
- CollectionService — collection CRUD, move, rename
- RequestExecutionService — the cross-context use case (env resolution → HTTP execute → history save → cookie update)
- EnvironmentService — environment CRUD
- HistoryService — history list/clear
- TemplateService — template CRUD
- Dependency injection via `Box<dyn Trait>` constructor parameters

### Frontend Migration
- React frontend stays largely intact
- `tauri-api.ts` bridge wraps all `invoke()` calls
- `api.ts` re-exports from bridge (zero component changes)
- WebSocket replaced with Tauri event system (`listen('file-change')`)

### Data Compatibility
- Reads from same `~/.rocket-api/` directory as Go backend
- Zero data migration needed

## Future Sub-Projects (SP2–SP6)
Architecture accommodates 3 additional crates in future:
- rocket-scripting (SP3)
- rocket-git (SP4)
- rocket-identity (SP6)

No architectural restructuring needed — each SP extends existing crates or adds new ones.
