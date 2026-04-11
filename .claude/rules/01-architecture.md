# Architecture rules

Rust DDD layering for the Rocket workspace. Read before touching any crate.

## Layering (hard rules)

- `rocket-app` must never depend on `rocket-infra`. Services take `Arc<dyn Trait>` / `Box<dyn Trait>` only. Concrete impls are wired in `src-tauri/src/lib.rs`.
- `rocket-collection`, `rocket-environment`, `rocket-history`, `rocket-workspace`, `rocket-http`, `rocket-git` are pure domain crates. No I/O, no filesystem, no `tokio::fs`, no `reqwest` calls.
- Repository traits live in the matching domain crate. Their filesystem implementations live in `rocket-infra` (naming pattern: `Fs<Thing>Repo`).
- `src-tauri` depends on `rocket-app` and `rocket-infra`. It never imports domain crates directly to do logic — only to construct DTOs for IPC.

## Service pattern

- All fallible service methods return `DomainResult<T>` (= `Result<T, DomainError>` from `rocket-shared`).
- Services hold dependencies as `Arc<dyn Trait>` fields. Construction takes them via `new(...)`.
- Services never call back into Tauri. They publish events via `EventPublisher` trait (`TauriEventBus` in prod, `NullEventPublisher` in tests) — fire-and-forget, errors never propagate.

## Active workspace path

- The active workspace path lives in `Arc<Mutex<PathBuf>>` shared across all services.
- Switching workspaces only requires updating this pointer — no service graph rebuild.
- When wiring a new service that needs the workspace path, clone the existing `Arc` rather than creating a new one.

## Adding a new domain crate

1. Create the crate under `crates/<name>/`.
2. Define domain types and a repository trait (no `async`, no I/O).
3. Add an `Fs<Thing>Repo` in `rocket-infra` that implements the trait.
4. Add a service in `rocket-app` that takes the trait as `Arc<dyn Trait>`.
5. Wire the concrete `Fs<Thing>Repo` into the service in `src-tauri/src/lib.rs`.
6. Expose IPC commands in `src-tauri/src/commands/<name>.rs`.
7. Register the commands in `src-tauri/src/lib.rs` under the `tauri::generate_handler!` call.

## Reference

- Root `CLAUDE.md` has the crate table.
- Per-crate `CLAUDE.md` files auto-load when you touch that crate's files — trust them, don't re-derive rules.
