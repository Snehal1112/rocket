# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

`rocket-collection` is a pure domain crate — no I/O, no filesystem access. It defines the Collection aggregate and its repository trait. The filesystem implementation lives in `rocket-infra`.

## Commands

```bash
# Check this crate
cargo check -p rocket-collection

# Run all tests in this crate
cargo test -p rocket-collection

# Run a single test by name
cargo test -p rocket-collection <test_name>
```

## Domain Model

```
Collection (aggregate root)
├── name: String          — unique within workspace, validated by Collection::validate_name
├── root: Folder          — recursive tree of CollectionItems
└── settings: CollectionSettings  — default auth/headers/variables (stored in collection.json)

Folder
└── items: Vec<CollectionItem>
    ├── Request           — a saved HTTP request definition
    ├── Folder            — nested subfolder
    └── OpaqueProtocolItem — raw YAML passthrough for GraphQL/gRPC/WebSocket

CollectionSummary         — lightweight listing type (no full tree)
```

All structs use `#[serde(rename_all = "camelCase")]` for JSON serialization. Optional fields use `#[serde(default, skip_serializing_if = ...)]` to maintain backward compatibility with older saved files.

## Key Design Rules

- **No I/O here.** `CollectionRepository` is a trait only; the concrete `FsCollectionRepo` is in `rocket-infra`.
- **Identity:** `Collection` is identified by `name` (unique per workspace). `Folder` and `Request` use a UUID `uid` field.
- **`Request` builder pattern:** Use `Request::new(name, method, url).with_header(...).with_body(...).with_auth(...)` for construction in tests.
- **Serde backward compat:** New optional fields on `Request` must have `#[serde(default)]` so old JSON files without those fields still deserialize correctly.
- **`request_count()`** on `Folder` is recursive; on `Collection` it delegates to `root`.
- **`find_request` / `find_folder`** on `Folder` are non-recursive — they search the current level only. Use `subfolder_names()` to get all folder names at current level.
- **`CollectionItem` serde tag:** Uses `#[serde(tag = "type")]` with values `"request"`, `"folder"`, `"opaque"`. The `type` field appears in serialized JSON.
- **`OpaqueProtocolItem.raw`** holds a `serde_yaml::Value` for lossless roundtrip of GraphQL/gRPC/WebSocket items — do not parse or transform it.
- **`CollectionSummary.ref_type`** defaults to `"embedded"`; `"external"` is set by the workspace layer for collections referenced by path rather than owned.
- **`save_request`** on the repository returns the actual filename written, which may differ from `path` when the infra layer generates a unique name to avoid collisions.
- **`reorder_items`** takes the full ordered list of entry names including `.json` extensions; pass `""` for `folder_path` to target the collection root.

## Request Fields of Note

`Request` carries several fields beyond the basic HTTP definition:

- **`file_name`**: The on-disk filename (e.g. `"Get Users.json"`). `None` at construction; populated by `build_folder_tree` in `rocket-infra` when the collection is loaded from disk.
- **`runtime_auth`**: An auth override applied at execution time (e.g. from `runtime.auth` in OC YAML). Not persisted as the primary auth; kept separate from `auth`.
- **`variables`**: Request-level variables typed as `Vec<CollectionVariable>`. Resolved upstream in `rocket-app`.
- **Scripting/testing fields**: `pre_request_script`, `post_response_script`, `tests` (JS strings), `assertions` (`Vec<Assertion>`), `actions` (`Vec<ActionSetVariable>`), and `examples` (`Vec<HttpRequestExample>`) are all optional and default to empty/`None`. They are executed by `rocket-app`, not this crate.
- **`settings`**: `Option<RequestSettings>` — per-request execution settings (timeout, encode URL, etc.).
- **`docs`**: `Option<Documentation>` — structured documentation separate from the free-text `description`.
- **`seq`**: `Option<u32>` — optional ordering hint used by the infra layer.

## CollectionSettings

`CollectionSettings` (stored as `collection.json`) includes:

- **`variables: Vec<CollectionVariable>`** — collection-scoped variables sit below environment variables in the resolution hierarchy. Each `CollectionVariable` has a `secret: bool` field to suppress UI display and an `initial_value` field for Postman export compatibility.
- **`auth: Option<Auth>`** — default auth applied to all requests in the collection.
- **`headers: Vec<Header>`** — default headers prepended to every request.
- **`docs: Option<String>`** — optional markdown documentation for the collection (maps to `docs:` in opencollection.yml).

## CollectionRepository

The trait is **synchronous** (no `async`). Despite `async-trait` being in `Cargo.toml`, all methods return `DomainResult<T>` directly. The concrete implementation is `FsCollectionRepo` in `rocket-infra`.

## Contract Module (`src/contract/`)

### New public types (SP1)
- `ContractParty` — replaces bare `String` for provider/consumer. Custom `Deserialize` accepts both plain strings (old format) and objects (new format).
- `PartyKind` — `Team | Company | Service`
- `ContractPolicy` — `breaking_change_policy`, `notice_days`, `uptime_sla`
- `BreakingChangePolicy` — `Strict | Lenient | AdditiveOk`
- `ContractStatus` — now has 8 variants including `Draft`, `Drift`, `Breach`, `InReview`, `Paused`. Status is **stored** in YAML, not computed at runtime.
- `ChangelogEntry.is_breaking: bool` — defaults `false` for backward compat.

### State machine (`state_machine.rs`)
- `transition(current: &ContractStatus, event: &StatusEvent) -> Result<ContractStatus, InvalidTransition>`
- Pure function — no I/O. Call from `ContractService` (in `rocket-app`) when handling lifecycle commands.

### Backward compatibility rules
- Old YAML `provider: "string"` → `ContractParty::from_name(string)`
- Old YAML `consumer: "string"` → `consumers: vec![ContractParty::from_name(string)]`
- Old YAML with no `status` field → defaults to `ContractStatus::Active`
- Old YAML `ChangelogEntry` with no `isBreaking` → defaults to `false`
