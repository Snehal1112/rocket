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
| `client_certificate_validation.rs` | `validate_client_certificates(certs, bindings)` — save-time rules for certificate entries (one source per piece, references match the environment's bindings, no key text in path or reference fields, non-empty domain) |
| `flatten.rs` | `flatten_environment` / `flatten_with_repo` merge an environment with its `extends` chain (furthest ancestor first, child wins) into `Flattened` (variables, external secret bindings and client certificates, each tagged with its source); `find_extends_cycle` rejects a self or cyclic parent on save |
| `dot_env.rs` | `collect_dot_env` layers the `.env` files of the chain through the `DotEnvLoader` trait (parent under child, child wins); `collect_default_dot_env` reads the default `.env` of a base folder through `DotEnvLoader::load_default` (a failure is a warning); the file I/O is in `rocket-infra` |
| `invalid.rs` | `pub(crate) invalid(message)` — the one way this crate builds `DomainError::InvalidInput`; used by `client_certificate_validation.rs` and `external_secret.rs` |

### Key Design Points

- **`Variable` deserialization** handles a legacy `disabled: bool` field alongside the current `enabled: bool`. The rule is `enabled = enabled && !disabled`. Do not break this backward-compat logic when editing `variable.rs`.
- **Resolver** leaves unresolved `{{placeholders}}` as-is and reports them in `ResolveResult::unresolved` — callers decide how to surface warnings. Whitespace inside `{{ var }}` is trimmed before lookup.
- **`resolve_with_env`** is the convenience wrapper that pulls only enabled variables from an `Environment`.
- **`client_certificates`** are consumed by the executor for mutual TLS: `rocket-app` resolves them into `ResolvedClientCertificate`s (`rocket-http`) on the request options, and `rocket-infra` loads the matching one. Paths and passphrase may hold `{{placeholders}}`. A relative path is relative to the collection folder, which is the parent of `environments/`; `..` is not allowed, and absolute and `~/` paths are used as written. Each piece of material (certificate, private key, PKCS12 bundle) has exactly one source: a file path (`certificateFilePath`, `privateKeyFilePath`, `pkcs12FilePath`) or a RocketVault reference (`certificateSecret`, `privateKeySecret`, `pkcs12Secret`, always `alias.secretName`, never a value and never a `{{placeholder}}`). `validate_client_certificates(certs, bindings)` (`client_certificate_validation.rs`, called by `EnvironmentService` next to `validate_external_secret_bindings`) rejects a piece with no source or two sources, a reference whose alias or name is not in the environment's bindings, a path or reference field that starts with `-----BEGIN` (key text must never reach the environment file), and an empty domain. The three reference keys are Rocket extensions outside the OpenCollection schema. A `vault` entry names a certificate in a RocketVault vault (alias of a connection binding, certificate name, format `pem` or `pkcs12`, default `pem`) and is checked by the same function. `VaultSecretFetcher` has `list_certificates`, `fetch_certificate` and `forget_connection`, with defaults that refuse or do nothing. This crate never sees key bytes.
- **`extends` and `dot_env_file_path`** are acted on. `flatten.rs` merges raw variables along the `extends` chain before `{{var}}` interpolation, so a parent value may reference a child variable. Variables are inherited with their secret flag kept, and so are external secret bindings (merged by alias, child wins) and client certificates (merged by trimmed, lowercased domain, child wins); each row is tagged with its source. Parents come from the same store only (collection to collection, global to global). A missing parent warns and the child's own variables are used; a cycle is rejected on save and fails a send. Saving never writes inherited variables, bindings or certificates into the child's file. `dot_env.rs` layers `.env` files along the same chain. The file is read only when the host environment gate allows it, a relative path resolves against the collection folder, `..` is refused, a missing file only warns, and the real OS environment wins over a `.env` value. `.env` values are never merged into the plain variable scope; they are exposed as `{{process.env.NAME}}`.
- All new fields on `Environment` and `Variable` must use `#[serde(default, skip_serializing_if = ...)]` to maintain backward compatibility with persisted JSON files.

### Dependencies

- `rocket-shared` — `DomainError`, `DomainResult`, `Description`, `VariableValue`, `VariableValueVariant`
- `serde` / `serde_json` — serialization and the `Extensions` (`serde_json::Value`) alias
- `async-trait` — available for repository trait if async variants are needed
