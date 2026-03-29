# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

The `rocket-environment` crate is a pure domain crate in the Rocket HTTP client workspace. It owns the `Environment` aggregate, the `Variable` entity, the `EnvironmentRepository` trait, and the `{{variable}}` template resolver. It has no I/O — the filesystem implementation lives in `rocket-infra`.

## Commands

```bash
# Check this crate
cargo check -p rocket-environment

# Run all tests in this crate
cargo test -p rocket-environment

# Run a single test
cargo test -p rocket-environment <test_name>
```

## Architecture

### Module Map

| Module | Responsibility |
|---|---|
| `environment.rs` | `Environment` aggregate root — holds a `Vec<Variable>`, exposes `set_variable`, `remove_variable`, `get_value`, `enabled_variables` |
| `variable.rs` | `Variable` entity — key/value with `enabled`, `secret`, optional `description`, `value_variants`, `secret_type` |
| `resolver.rs` | `resolve(template, &HashMap)` — replaces `{{var}}` placeholders; returns `ResolveResult { output, unresolved }` |
| `repository.rs` | `EnvironmentRepository` trait — `list`, `get`, `save`, `delete` returning `DomainResult<T>` |

### Key Design Points

- **`Variable` deserialization** handles a legacy `disabled: bool` field alongside the current `enabled: bool`. The rule is `enabled = enabled && !disabled`. Do not break this backward-compat logic when editing `variable.rs`.
- **Resolver** leaves unresolved `{{placeholders}}` as-is and reports them in `ResolveResult::unresolved` — callers decide how to surface warnings. Whitespace inside `{{ var }}` is trimmed before lookup.
- **`resolve_with_env`** is the convenience wrapper that pulls only enabled variables from an `Environment`.
- **`extends` and `dot_env_file_path`** on `Environment` are stored but not acted upon in this crate; inheritance and `.env` loading are handled upstream (in `rocket-app` / `rocket-infra`).
- All new fields on `Environment` and `Variable` must use `#[serde(default, skip_serializing_if = ...)]` to maintain backward compatibility with persisted JSON files.

### Dependencies

- `rocket-shared` — `DomainError`, `DomainResult`, `Description`, `VariableValue`, `VariableValueVariant`
- `serde` / `serde_json` — serialization and the `Extensions` (`serde_json::Value`) alias
- `async-trait` — available for repository trait if async variants are needed
