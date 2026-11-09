# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

`rocket-collection` is a domain crate with no I/O. `openapi_consent.rs` takes the collection root as given; `FsOpenApiConsentRepo` canonicalizes it first (D-52 in `docs/roadmaps/ddd-roadmap.md`). It defines the Collection aggregate and its repository trait. The filesystem implementation lives in `rocket-infra`.

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
    ├── GraphQl           — a saved GraphQL request
    ├── WebSocket         — a saved WebSocket request
    ├── Folder            — nested subfolder
    ├── Grpc              — a saved gRPC request
    └── OpaqueProtocolItem — raw YAML passthrough for protocols without a typed variant

CollectionSummary         — lightweight listing type (no full tree)
```

All structs use `#[serde(rename_all = "camelCase")]` for JSON serialization. Optional fields use `#[serde(default, skip_serializing_if = ...)]` to maintain backward compatibility with older saved files.

## Templates (`template.rs`, `template_repository.rs`)

| Type | Description |
|---|---|
| `Template` | A saved request blueprint: name, `HttpMethod`, URL, headers, and optional body. Not part of any collection. |
| `TemplateRepository` | Trait for CRUD on `Template` records (`list`, `get`, `save`, `delete`). Synchronous and `Send + Sync`. `rocket-app`'s `TemplateService` receives it as `Box<dyn TemplateRepository>`. |

`Template` is keyed by `name` (a `String`), not a UUID. The repository `get` and `delete` methods take `&str` names. `Template.method` is the typed `HttpMethod` from `rocket-shared`. The concrete repositories (`FsTemplateRepo`, `SharedPathTemplateRepo`) live in `rocket-infra`.

## Events

`events.rs` defines `CollectionEvent` (17 variants: collection, request, folder and variable events plus the file watcher events). It implements `PublishedEvent` and overrides `written_collections()`, which the echo guard reads: `TauriEventBus` passes each event to `RecentWrites::record`. Its `wire_contract_*` tests pin the JSON keys. `FileChangeKind` (the file watcher change kind) is defined in the same file and re-exported as `rocket_collection::FileChangeKind`. `rocket-import` has its own unrelated `FileChangeKind` (`Put`, `Delete`).

## Key Design Rules

- **No I/O here.** `CollectionRepository` is a trait only; the concrete `FsCollectionRepo` is in `rocket-infra`.
- **Identity:** `Collection` is identified by `name` (unique per workspace). `Folder` and `Request` use a UUID `uid` field.
- **`Request` builder pattern:** Use `Request::new(name, method, url).with_header(...).with_body(...).with_auth(...)` for construction in tests.
- **Serde backward compat:** New optional fields on `Request` must have `#[serde(default)]` so old JSON files without those fields still deserialize correctly.
- **`request_count()`** on `Folder` is recursive; on `Collection` it delegates to `root`.
- **`find_request` / `find_folder`** on `Folder` are non-recursive — they search the current level only. Use `subfolder_names()` to get all folder names at current level.
- **`Folder::dir_or_name()`** returns the on-disk directory name, falling back to the display name. `rocket-acp`'s `proposal_rules.rs` uses it. `rocket-collection` must not depend on `rocket-acp`.
- **`CollectionItem` serde tag:** Uses `#[serde(tag = "type")]` with values `"request"`, `"folder"`, `"graphql"`, `"grpc"`, `"opaque"`. The `type` field appears in serialized JSON.
- **`OpaqueProtocolItem.raw`** holds a `serde_yaml::Value` for lossless roundtrip of items without a typed variant — do not parse or transform it.
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
- **`script_flow: ScriptFlow`** — `Sandwich` (default) or `Sequential`, serde lowercase. Stored at `extensions.bruno.scripts.flow` in opencollection.yml, not in a Rocket-only field.

## FolderSettings

`folder_settings.rs` holds `FolderSettings` (headers, auth, variables, three phase scripts, docs), `ScriptFlow` (`sandwich` default, `sequential`) and `ScriptPhase`. It has no serde derives: `rocket-infra` owns the `folder.yml` shape and `src-tauri` owns the IPC DTO.

- `inherited_headers` merges collection then folder headers (outermost first). Inner replaces outer by exact key. Disabled headers never shadow and are dropped.
- `resolve_folder_auth` returns the innermost folder auth that is not `None` or `Inherit`. `effective_auth(request_auth, folders, collection_auth)` is the full chain used at send time: an explicit request auth, else the innermost folder auth, else the collection auth, else `None` (a request set to `None` or `Inherit` takes the chain).
- `chain_scripts` orders one phase's scripts for a `ScriptFlow`, request script included, blanks skipped.
- `CollectionRepository::{get_folder_settings, save_folder_settings, get_folder_chain_settings}` have defaults so test doubles keep compiling.

## GraphQL documents (`graphql_document.rs`)

`list_operations(document)` lists the operations of a GraphQL document in order. `select_operation(document, requested, fallback_first)` picks the `operationName` to send: a requested name must exist, a single operation is chosen without a request, and with several the send path fails while the runner (`fallback_first`) takes the first. Pure text rules, moved here from `rocket-app` (D-86).

## Trust (`trust.rs`)

`resolve_effective` combines what a collection file requests with what the user granted. `effective_capabilities(repo, store, collection)` reads the identity, settings and grant through `CollectionRepository` and `CollectionTrustStore` and resolves them; any error gives `EffectiveCapabilities::untrusted()`. It moved here from `rocket-app` (D-86). `CollectionTrustService` and the write path for grants stay in `rocket-app/src/collection_trust.rs`.

## Duplicate rules (`duplicate.rs`)

`duplicate_candidate(source_path, source_name, n)` gives the path and name of the nth copy (`Login copy`, then `Login copy 2`). The path keeps the folder and file stem and always ends in `.yml`; the name keeps the source name. `duplicate_of(source, name)` clones a request with a new uid, no `seq` and no `file_name`. `CollectionService` owns the retry loop (`MAX_DUPLICATE_ATTEMPTS` = 1000, then `Conflict`).

## Validating types (`names.rs`, `rel_path.rs`)

`CollectionName::parse` is `Collection::validate_name` plus: no NUL byte, not `.` or `..`, exactly one normal path component (so `C:x` is refused on Windows). Use it where a name is joined onto a path. `RelPath::parse` holds the rules of `validate_rel_path` (no empty, absolute, `.` or `..` part, no backslash, NUL or colon, no Windows device name, only owned files in `.rocket`). Both give `as_str()`. They are not stored in `Collection` or `Request`, so the serde shape is unchanged.

## Discovery (`discovery.rs`)

`CollectionDiscovery` finds collections and workspaces inside a folder for the open-folder and git-clone flows. `DirectoryProbe` (`is_dir`, `has_file`, `child_dirs`) is the port; `FsDirectoryProbe` in `rocket-infra` implements it. `scan(dir)` lists the direct subfolders that hold `opencollection.yml` (no dot folders, sorted by lowercase name, a missing folder gives an empty list). `detect(dir)` decides in this order: `workspace.yml` (workspace, with its collections), `opencollection.yml` at the root (one collection), a `collections/` folder with collections, then direct children; otherwise unknown. An unreadable folder is an error.

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
- `apply_transition(status, event) -> ContractResult<ContractStatus>` runs `transition` and maps a rejected event to `ContractError::Internal` that names the status and the event.

### Builders (`builders.rs`)
- `NewContract` (what a person enters) with `into_contract()`, and `ContractEdit` (the editable fields) with `apply_to(existing)`. Both are pure and re-exported as `rocket_collection::contract::{NewContract, ContractEdit}`. `rocket-app` does not re-export them.
- `ContractService` still sets the final id, timestamps, document paths and `endpoint_count`.

### Snapshot helpers (`snapshot.rs`)
- `snapshot_path(requested_path, saved_file_name)` rebuilds the snapshot path after a save (the saved file name replaces the last component).
- `walk_folder(folder, prefix, out)` collects `(path, request)` pairs of a folder tree. It takes HTTP requests only: GraphQL, WebSocket, gRPC and opaque items are skipped, as are requests without a `file_name` and folders without a `dir_name`.

### Scope and version rules
- `ContractScope::covers(request_path)`: a folder scope covers whole path components only (`auth` does not cover `authz/x.yml`), a request scope covers its own path, a collection scope covers every path.
- `bump_patch_version` turns `1.2.3` into `1.2.4`; any other shape (including a `u32::MAX` patch) gives `<version>-copy`.

### Backward compatibility rules
- Old YAML `provider: "string"` → `ContractParty::from_name(string)`
- Old YAML `consumer: "string"` → `consumers: vec![ContractParty::from_name(string)]`
- Old YAML with no `status` field → defaults to `ContractStatus::Active`
- Old YAML `ChangelogEntry` with no `isBreaking` → defaults to `false`
