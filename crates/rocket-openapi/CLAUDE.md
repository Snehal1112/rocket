# CLAUDE.md

Guidance for Claude Code when working in the `rocket-openapi` crate.

## What This Is

The `rocket-openapi` crate is the home of the OpenAPI sync context: the sync
service, apply, events and view DTOs. It is being filled in step by step. For
now it holds the events and the read-only view DTOs.

## Dependency Direction

- May use `rocket-shared`, `rocket-collection`, `rocket-import`, `rocket-http`
  and `rocket-environment`.
- Never depends on `rocket-app`. `rocket-app` and `src-tauri` depend on this
  crate.

## Commands

```bash
# Check this crate
cargo check -p rocket-openapi -j4

# Run all tests in this crate
cargo test -p rocket-openapi -j4
```

## Module Map

| Module | Responsibility |
|---|---|
| `events` | `OpenApiSyncEvent`, the refresh signals the sync publishes, with wire contract tests |
| `view` | Read-only views of a pending sync (`PlanView`, `SpecView`, `ChangesView`, ...). No serde; the IPC layer maps them to its own DTOs |
