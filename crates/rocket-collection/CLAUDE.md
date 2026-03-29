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
- **`find_request` / `find_folder`** on `Folder` are non-recursive — they search the current level only.
- **`CollectionItem` serde tag:** Uses `#[serde(tag = "type")]` with values `"request"`, `"folder"`, `"opaque"`. The `type` field appears in serialized JSON.
- **`OpaqueProtocolItem.raw`** holds a `serde_yaml::Value` for lossless roundtrip of GraphQL/gRPC/WebSocket items — do not parse or transform it.
- **`CollectionSummary.ref_type`** defaults to `"embedded"`; `"external"` is set by the workspace layer for collections referenced by path rather than owned.
- **`save_request`** on the repository returns the actual filename written, which may differ from `path` when the infra layer generates a unique name to avoid collisions.
- **`reorder_items`** takes the full ordered list of entry names including `.json` extensions; pass `""` for `folder_path` to target the collection root.

## Request Fields of Note

`Request` carries several fields beyond the basic HTTP definition:

- **`file_name`**: The on-disk filename (e.g. `"Get Users.json"`). `None` at construction; populated by `build_folder_tree` in `rocket-infra` when the collection is loaded from disk.
- **`runtime_auth`**: An auth override applied at execution time (e.g. from `runtime.auth` in OC YAML). Not persisted as the primary auth; kept separate from `auth`.
- **`variables`**: Request-level variables typed as `Vec<serde_json::Value>` because `rocket-environment` is not a dependency of this crate. Resolved upstream in `rocket-app`.
- **Scripting/testing fields**: `pre_request_script`, `post_response_script`, `tests` (JS strings), `assertions` (`Vec<Assertion>`), `actions` (`Vec<ActionSetVariable>`), and `examples` (`Vec<HttpRequestExample>`) are all optional and default to empty/`None`. They are executed by `rocket-app`, not this crate.

## CollectionSettings Variable Scope

`CollectionSettings` (stored as `collection.json`) includes a `variables: Vec<CollectionVariable>` field. These collection-scoped variables sit below environment variables in the resolution hierarchy — environment variables override them. Each `CollectionVariable` has a `secret: bool` field to suppress UI display.
