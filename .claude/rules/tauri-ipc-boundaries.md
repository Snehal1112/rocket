# Tauri IPC Boundaries

## Required Reads

1. [../../CLAUDE.md](../../CLAUDE.md)
2. Relevant crate guidance in [../../crates](../../crates)
3. For yml/collection/environment/request/auth/variable-resolution scope, read [../../docs/superpowers/specs/opencollection-spec-reference.md](../../docs/superpowers/specs/opencollection-spec-reference.md) first.

## Command-Layer Rules

- Keep commands thin: validate, call services, map output/error.
- No domain business logic in command modules.
- Route via rocket-app traits/services.
- Keep concrete I/O in rocket-infra.

## DTO and Serde Rules

- Use dedicated IPC DTOs when contract differs.
- camelCase rename only on IPC DTOs.
- Do not apply camelCase rename on persistence structs.

## Error Handling

- Avoid unwrap in production command paths.
- Map failures to stable IPC-facing errors.

## OpenCollection Trigger

If command changes touch collections/environments or persistence behavior, include this first planning step:

📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.

## Verification

- cargo check and focused checks
- targeted Rust tests
- yarn tsc --noEmit for DTO/frontend binding changes
