# AGENTS.md

Guidance for AI coding agents working in this repository.

## Read First

1. Read [CLAUDE.md](CLAUDE.md) for global architecture and hard rules.
2. If a task touches collections, environments, requests, auth config, or any `.yml` persistence file, read [docs/superpowers/specs/opencollection-spec-reference.md](docs/superpowers/specs/opencollection-spec-reference.md) before making a plan.
3. When editing a Rust crate, read that crate's local `CLAUDE.md` first.

## Quick Commands

- Full app dev mode: `yarn tauri dev`
- Frontend-only dev mode: `yarn dev`
- Type check: `yarn tsc --noEmit`
- Lint/format check (read-only): `yarn check`
- Auto-fix lint/format: `yarn lint` and `yarn format`
- Frontend tests: `yarn test [pattern]`
- Rust validation: `cargo check` or `cargo check -p <crate>`
- Rust tests: `cargo test` or `cargo test -p <crate> <test_name>`
- Release build: `yarn tauri build`

## Architecture Boundaries

- Follow DDD crate boundaries described in [CLAUDE.md](CLAUDE.md).
- Domain crates (`rocket-shared`, `rocket-collection`, `rocket-environment`, `rocket-http`, `rocket-history`, `rocket-workspace`, `rocket-git`) are pure domain logic and traits. Do not add filesystem or network I/O there.
- `rocket-infra` is the implementation layer for repository/service traits and I/O.
- `rocket-app` orchestrates through trait objects (`Box<dyn Trait>`) and should not depend on infra concrete types.
- Tauri wiring and IPC commands live under [src-tauri/src](src-tauri/src).
- Frontend lives under [src](src).

Data flow:

React UI -> Tauri command -> rocket-app service -> rocket-infra implementation -> filesystem/network

## Hard Rules

Frontend:

- Use shadcn/ui primitives, not raw HTML form/dialog/button/select/input primitives.
- Use `lucide-react` icons only.
- Use `SingleLineEditor` (CodeMirror 6) for single-line variable-aware fields.
- Use Monaco for multi-line editors.
- In Zustand usage, do not fully destructure store state at component top level.

Rust:

- Do not use `unwrap()` in production paths.
- Do not shell out to git CLI; use `git2` through project services.
- Keep persistence compatibility in mind when adding fields (prefer optional fields with serde defaults when evolving on-disk formats).

Serialization:

- Use `#[serde(rename_all = "camelCase")]` on IPC DTOs only.
- Do not apply camelCase serde rename to persistence structs.

## OpenCollection Read-First Trigger

Before planning or coding, read [docs/superpowers/specs/opencollection-spec-reference.md](docs/superpowers/specs/opencollection-spec-reference.md) whenever a task touches any of the following:

- `.yml` files
- Collection/folder/request/environment data models
- Variable resolution or variable scope logic
- Auth configuration
- Tauri IPC commands for collections or environments

## Useful Deep References

- Architecture and command baseline: [CLAUDE.md](CLAUDE.md)
- Product and stack overview: [README.md](README.md)
- Crate-specific rules:
  - [crates/rocket-app/CLAUDE.md](crates/rocket-app/CLAUDE.md)
  - [crates/rocket-collection/CLAUDE.md](crates/rocket-collection/CLAUDE.md)
  - [crates/rocket-environment/CLAUDE.md](crates/rocket-environment/CLAUDE.md)
  - [crates/rocket-http/CLAUDE.md](crates/rocket-http/CLAUDE.md)
  - [crates/rocket-git/CLAUDE.md](crates/rocket-git/CLAUDE.md)
  - [crates/rocket-infra/CLAUDE.md](crates/rocket-infra/CLAUDE.md)
  - [crates/rocket-workspace/CLAUDE.md](crates/rocket-workspace/CLAUDE.md)

## Agent Behavior in This Repo

- Prefer minimal diffs and preserve existing code style.
- Validate with focused checks relevant to changed areas.
- Do not duplicate existing docs in generated explanations; link to source docs.