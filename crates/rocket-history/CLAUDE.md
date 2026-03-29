# rocket-history

Domain crate for request execution history and reusable request templates. It defines the domain types and repository traits only; no I/O or filesystem logic lives here.

## Public Types

| Type | Description |
|---|---|
| `HistoryEntry` | A single recorded HTTP execution: method, URL, status, duration, response size, timestamp, and optional collection/request name. |
| `HistoryFilter` | Optional filter criteria (method, URL substring, status range) passed to `HistoryRepository::search`. |
| `Template` | A saved request template: name, `HttpMethod`, URL, headers, and optional body. Keyed by name, not UUID. |
| `HistoryRepository` | Trait for persisting and querying `HistoryEntry` records (`list`, `get`, `save`, `clear`, `search`). |
| `TemplateRepository` | Trait for CRUD on `Template` records (`list`, `get`, `save`, `delete`). |

## Key Patterns

- Both repository traits are synchronous and `Send + Sync`, allowing `Box<dyn Trait>` use in service structs.
- All methods return `DomainResult<T>` from `rocket-shared`.
- All structs derive `serde::{Serialize, Deserialize}` with `#[serde(rename_all = "camelCase")]` to match the frontend JSON convention.
- `HistoryEntry::new` generates a UUID `id` and captures `Utc::now()` at construction; use `.with_collection()` to attach collection metadata.
- `Template` is keyed by `name` (a `String`), not a UUID — the repository `get` and `delete` methods accept `&str` names.

## Workspace Relationships

- Depends only on `rocket-shared` (for `DomainResult`, `HttpMethod`, `Header`, `Body`).
- Concrete filesystem implementations live in `rocket-infra`.
- `rocket-app` services receive `Box<dyn HistoryRepository>` and `Box<dyn TemplateRepository>` via constructor injection.
- The Tauri shell in `src-tauri` wires the `rocket-infra` implementations to these traits at startup.
