# IPC boundary rules

Rules that span the Tauri boundary between frontend (TypeScript) and backend (Rust).

## Frontend side

- All backend calls go through `src/lib/tauri-api.ts`. Never call `invoke(...)` directly from components or stores.
- Every new command gets a typed wrapper in `tauri-api.ts` with explicit input and output types.
- The types in `tauri-api.ts` are the single source of truth for wire shapes. Components and stores import from there — do not redeclare.
- Input objects use `camelCase` field names (matches Rust serde convention).

## Backend side

- Tauri commands live in `src-tauri/src/commands/<module>.rs`. They are thin IPC bridges: parse inputs, call a service, map errors.
- No business logic in a command. If you find yourself writing an `if` or a loop for anything beyond error mapping, it belongs in a service.
- Command input structs use `#[derive(serde::Deserialize)]` with `#[serde(rename_all = "camelCase")]`.
- Commands return `Result<T, String>` for IPC — errors are mapped from `DomainError` / crate-specific errors via `.map_err(|e| e.to_string())`.
- Services never call back into Tauri. Events go through `EventPublisher`.

## Wiring

- New commands must be registered in `src-tauri/src/lib.rs` inside `tauri::generate_handler!`. A command that compiles but isn't registered will fail at runtime with a confusing error.
- New services must be injected in `src-tauri/src/lib.rs` as managed state via `.manage(...)` so commands can reach them via `State<'_, Service>`.

## Events

- Events are fire-and-forget. A listener failing must not propagate back to the publisher.
- `TauriEventBus` in production, `NullEventPublisher` in tests. Never test event delivery from a service test — that belongs in an integration test.

## DTOs vs domain types

- Crossing the boundary with a domain struct directly is fine if it already has the right serde attributes. Do not invent a parallel DTO unless the frontend needs a different shape.
- If you do need a DTO, put it next to the command in `src-tauri/src/commands/<module>.rs`, not in a domain crate.

## Reference

- `.claude/tauri-commands.md` lists every command module and its purpose.
