# rocket-shared

Foundational crate that provides the common domain types, error handling, and event infrastructure shared across all other crates in the workspace. No other crate in the workspace is a dependency of this one.

## Key Public Types

### Error handling
- `DomainError` — top-level error enum with variants: `NotFound`, `InvalidInput`, `AlreadyExists`, `Io`, `Serialization`, `Http`, `Internal`. Implements `std::error::Error` via `thiserror` and `Serialize` (serializes to its `Display` string for Tauri IPC).
- `DomainResult<T>` — type alias for `Result<T, DomainError>`. Used as the return type for every fallible service and repository method across the workspace.

### Events
- `DomainEvent` — exhaustive enum of all domain events (collection, request, environment, workspace, HTTP execution, file-system, history, git). Tagged with `#[serde(tag = "type", rename_all = "camelCase")]` so variants serialize as `{"type": "collectionCreated", ...}` for Tauri's event bus.
- `EventPublisher` — object-safe trait (`fn publish(&self, event: DomainEvent)`). Implemented by `TauriEventBus` in `rocket-infra`; `NullEventPublisher` is the no-op used in tests.

### HTTP primitives
- `HttpMethod` — enum for GET/POST/PUT/PATCH/DELETE/OPTIONS/HEAD; implements `Display` and `FromStr` (case-insensitive).
- `Header`, `QueryParam`, `PathParam` — keyed, enabled-flag structures with optional `Description`.
- `Body` / `BodyMode` — request body with modes: `none`, `json`, `xml`, `text`, `sparql`, `formurlencoded`, `formdata`, `binary`.
- `Auth` — tagged enum covering `None`, `Basic`, `Bearer`, `ApiKey`, `OAuth2`, `AwsSigV4`, `Wsse`, `Digest`, `Ntlm`, `Inherit`. Tag field is `authType` with kebab-case values.
- `RequestSettings` / `RequestSettingValue<T>` — per-request execution settings (timeout, redirects, URL encoding), each value is either a concrete value or `"inherit"`.

### Descriptions and documentation
- `Description` / `Documentation` — polymorphic type that serializes as a JSON string, a `{content, type}` object, or `null`. Uses a custom serde visitor to handle all three forms. `Documentation` is a type alias for `Description`.

### Variables
- `VariableValue` — polymorphic: `Simple(String)` serializes as a plain JSON string; `Typed { data, value_type }` serializes as `{"type": "...", "data": "..."}`. Custom serde visitor handles both forms.
- `VariableValueVariant` — a named variant (title + selected flag + `VariableValue`). Used for multi-value environment variables.

### Assertions and actions
- `Assertion` — expression + operator + optional value; used in request test scripts.
- `ActionSetVariable` / `ActionSelector` / `ActionVariable` — describe pre/post-request actions that extract a value and store it in a scoped variable.
- `HttpRequestExample` — a saved request/response pair attached to a request definition.

### OAuth2
- `OAuth2Flow` — tagged enum (`flow` field, snake_case) covering `client_credentials`, `resource_owner_password_credentials`, `authorization_code`, `implicit`. Each variant carries its own credential and token configuration structs.

### Certificates and proxies
- `ClientCertificate` — tagged enum (`type` field): `pem` or `pkcs12`.
- `Proxy` / `ProxyConnectionConfig` / `ProxyAuth` — proxy settings; `auth` serializes as `false` when absent (matching the OpenCollection wire format).

## Patterns and Conventions

- **Custom serde visitors** are used wherever the JSON wire format is polymorphic (plain string vs. object). See `Description`, `VariableValue`, and `ProxyAuth`. This is intentional to stay compatible with the OpenCollection format.
- **`skip_serializing_if = "Option::is_none"`** is applied on all optional fields to keep serialized output minimal.
- **Tagged enums** use `#[serde(tag = "...")]` consistently: `DomainEvent` uses `type`/`camelCase`, `Auth` uses `authType`/`kebab-case`, `OAuth2Flow` uses `flow`/`snake_case`, `ClientCertificate` uses `type` with literal strings.
- All types derive `Debug`, `Clone`, and `Serialize`/`Deserialize`. Most derive `PartialEq` to support test assertions.
- This crate has no dependencies on other workspace crates and no I/O. It is safe to use in unit tests without any setup.

## Workspace Role

Every other crate depends on `rocket-shared`. Domain crates (`rocket-collection`, `rocket-environment`, etc.) use `DomainResult` and `DomainError` as their return types. Services in `rocket-app` accept a `Box<dyn EventPublisher>` and publish `DomainEvent` values. The Tauri shell (`src-tauri`) wires in `TauriEventBus` from `rocket-infra` as the live publisher.
