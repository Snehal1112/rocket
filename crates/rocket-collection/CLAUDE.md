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
