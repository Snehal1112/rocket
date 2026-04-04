# Bruno Import — Design Spec

> **Type:** Spec (reference only — never executed directly)
> **Date:** 2026-04-04
> **Status:** Approved

---

## Overview

RocketAPI needs to import Bruno workspaces and collections, converting the Bruno `.bru` DSL and Bruno `.yml` formats into RocketAPI's OpenCollection `.yml` format. This covers full-fidelity import: requests, environments, collection/folder variables, auth configurations, and pre/post scripts.

---

## Goals

| Goal | Priority |
|---|---|
| Import a Bruno collection into the active RocketAPI workspace | Must have |
| Import a Bruno workspace — create a new RocketAPI workspace or add to current | Must have |
| Convert `.bru` request files → OpenCollection `.yml` via domain types | Must have |
| Convert Bruno `.yml` request files → OpenCollection `.yml` via domain types | Must have |
| Import environments (regular vars + secret vars) | Must have |
| Import collection-level and folder-level variables | Must have |
| Import auth configurations (Bearer, Basic, AWS v4, API Key, Digest) | Must have |
| Import pre-request and post-response scripts | Must have |
| Auto-rename on collection name conflict (append -1, -2) | Must have |
| Skip unsupported features and surface them in an ImportReport | Must have |
| Entry points in: File menu, Workspace overview, Collections toolbar | Must have |

---

## Out of Scope

- OAuth2 auth import (skipped with report entry — not yet supported in RocketAPI)
- gRPC, WebSocket, GraphQL request types (skipped with report entry)
- Postman / Insomnia import (separate future feature)

---

## Architecture

### New crate: `rocket-import`

A dedicated crate that owns all import logic. Nothing depends on it; it depends on existing domain crates for writing.

```
crates/rocket-import/
  Cargo.toml
  src/
    lib.rs
    error.rs                  ← ImportError enum
    report.rs                 ← ImportReport + SkippedItem + SkipReason
    importer.rs               ← ImportService: orchestrates parse → convert → write
    bru/
      mod.rs
      lexer.rs                ← tokenises .bru DSL → token stream
      parser.rs               ← token stream → BruDocument (AST)
      ast.rs                  ← BruDocument, BruMeta, BruMethod, BruBody, BruAuth, BruVar
      yml_adapter.rs          ← Bruno .yml structs (serde) → BruDocument
    converter/
      mod.rs
      request.rs              ← BruDocument → rocket_collection::Request (domain type)
      environment.rs          ← BruDocument → rocket_environment::Environment (domain type)
      collection.rs           ← bruno.json vars/auth/scripts → rocket_collection::Collection
  tests/
    fixtures/
      sample.bru              ← real Bruno .bru sample
      sample_bruno.yml        ← real Bruno .yml sample
      collection/             ← minimal Bruno collection directory fixture
    integration_test.rs
```

### Dependency graph

```
rocket-import
  ├── rocket-shared      (domain types)
  ├── rocket-collection  (CollectionService / FsCollectionRepo for writes)
  ├── rocket-workspace   (WorkspaceService for creating new workspaces)
  └── rocket-environment (EnvironmentService / FsEnvironmentRepo for writes)

rocket-app
  └── rocket-import      (Tauri command handlers)
```

**Zero raw YAML writes inside `rocket-import`.** All disk writes are delegated to `FsCollectionRepo`, `FsEnvironmentRepo`, and `FsWorkspaceRepo`. OpenCollection schema compliance is enforced by the existing infra layer automatically.

---

## The `.bru` Parser

Bruno's `.bru` format is a custom block-based DSL. Blocks never nest (except raw-text body blocks). Example:

```
meta {
  name: Get Users
  type: http
  seq: 1
}

get {
  url: {{baseUrl}}/users
}

headers {
  Content-Type: application/json
}

body:json {
  { "page": 1 }
}

auth:bearer {
  token: {{authToken}}
}

script:pre-request {
  bru.setVar("ts", Date.now());
}
```

### Two-stage parse: lexer → parser

**Lexer** (`lexer.rs`) produces a flat token stream:
```rust
pub enum Token {
    BlockOpen { name: String, subtype: Option<String> },
    KeyValue { key: String, value: String },
    RawText(String),
    BlockClose,
}
```

**Parser** (`parser.rs`) consumes tokens and builds a `BruDocument`:
```rust
pub struct BruDocument {
    pub meta: Option<BruMeta>,
    pub method: Option<BruMethod>,         // get/post/put/patch/delete/head/options
    pub url: Option<String>,
    pub headers: Vec<(String, String)>,
    pub body: Option<BruBody>,
    pub auth: Option<BruAuth>,
    pub vars: Vec<BruVar>,                 // for env files: vars {}
    pub secret_vars: Vec<BruVar>,          // for env files: vars:secret {}
    pub collection_vars: Vec<BruVar>,      // for bruno.json collection-level vars
    pub folder_vars: Vec<BruVar>,          // for bruno.json folder-level vars
    pub pre_request_script: Option<String>,
    pub post_response_script: Option<String>,
    pub unknown_blocks: Vec<BruRawBlock>,  // unrecognised → SkippedItem
}

pub struct BruRawBlock {
    pub name: String,
    pub subtype: Option<String>,
    pub content: String,
}
```

### Bruno `.yml` path

`yml_adapter.rs` defines serde structs matching Bruno's `.yml` schema
(`BruYmlRequest`, `BruYmlEnv`, etc.) and provides `From` impls to normalise
them into `BruDocument`. Both paths produce identical `BruDocument` output —
the converter sees a uniform type regardless of source format.

```
.bru file   →  lexer → parser   ──┐
                                   ├──→  BruDocument  →  converter  →  domain type  →  repo write
Bruno .yml  →  serde_yaml          ┘
               (BruYmlRequest)
               → yml_adapter
```

---

## Conversion Mapping

| Bruno | RocketAPI domain |
|---|---|
| `meta.name` | `Request::name` |
| `meta.type: http` | `Request::request_type: Http` |
| `meta.type: graphql/grpc/ws` | → `SkipReason::UnsupportedRequestType` |
| `get/post/put/… { url }` | `Request::method` + `Request::url` |
| `headers {}` | `Request::headers` |
| `body:json {}` | `Request::body.mode: Json` |
| `body:text {}` | `Request::body.mode: Text` |
| `body:xml {}` | `Request::body.mode: Xml` |
| `body:form-urlencoded {}` | `Request::body.mode: FormUrlEncoded` |
| `body:multipart-form {}` | `Request::body.mode: Multipart` |
| `auth:bearer {}` | `Request::auth: Bearer` |
| `auth:basic {}` | `Request::auth: Basic` |
| `auth:awsv4 {}` | `Request::auth: AwsV4` |
| `auth:apikey {}` | `Request::auth: ApiKey` |
| `auth:digest {}` | `Request::auth: Digest` |
| `auth:oauth2 {}` | → `SkipReason::UnsupportedAuthType("oauth2")` |
| `script:pre-request {}` | `Request::scripts.pre` |
| `script:post-response {}` | `Request::scripts.post` |
| `vars {}` + `vars:secret {}` | `Environment::variables` |
| `bruno.json` collection vars | `opencollection.yml` variables (via `FsCollectionRepo`) |
| `bruno.json` folder vars | `folder.yml` variables (via `FsCollectionRepo`) |

---

## ImportService Orchestration

`importer.rs` walks the Bruno directory tree and drives the full write sequence:

```
1. Validate: bruno.json exists at root (else ImportError::NotABrunoDirectory)
2. Resolve target workspace:
   - "Create new workspace" → WorkspaceService::create(), then use new workspace
   - "Add to current" → use active workspace
3. Walk Bruno directory tree:
   - Collect all .bru + .yml request files (exclude bruno.json, environment files)
   - Collect environments/ directory
   - Detect subfolders recursively
4. For each collection being imported:
   a. Resolve name conflicts: check if collection name exists → append -1, -2 if needed
   b. FsCollectionRepo::create(resolved_name)
   c. Convert bruno.json collection vars → FsCollectionRepo::update() (sets variables)
   d. For each request file:
      - Detect format (.bru or Bruno .yml)
      - Parse → BruDocument
      - convert::request::convert() → rocket_collection::Request
      - FsCollectionRepo::create_request()
      - On ParseError: push SkippedItem, continue
   e. For each subfolder:
      - FsCollectionRepo::create_folder()
      - Convert bruno.json folder vars → FsCollectionRepo::update_folder()
      - Recurse into subfolder requests
   f. For each environment file:
      - Parse → BruDocument (env blocks)
      - convert::environment::convert() → rocket_environment::Environment
      - FsEnvironmentRepo::create()
5. Accumulate ImportReport throughout (counts + skipped items)
6. Return Ok(ImportReport)
```

Fatal errors (workspace not found, disk write failure) return `Err(ImportError)` immediately. Per-file parse errors are non-fatal and accumulate into the report.

---

## Error Handling

```rust
pub enum ImportError {
    NotABrunoDirectory(PathBuf),
    ParseError { path: PathBuf, message: String },
    IoError(std::io::Error),
    DomainError(DomainError),
}

// ImportError → DomainError at Tauri command boundary (rocket-app)
```

```rust
pub struct ImportReport {
    pub total_files: usize,
    pub imported: usize,
    pub skipped: Vec<SkippedItem>,
    pub created_workspace: Option<String>,
    pub created_collections: Vec<String>,
}

pub struct SkippedItem {
    pub path: String,
    pub reason: SkipReason,
}

pub enum SkipReason {
    UnsupportedRequestType(String),   // e.g. "graphql", "grpc", "websocket"
    UnsupportedAuthType(String),      // e.g. "oauth2"
    ParseError(String),
}
```

---

## Tauri Commands (rocket-app)

```rust
// Import a single Bruno collection directory into the active workspace
#[tauri::command]
async fn import_bruno_collection(
    path: String,
    target_workspace_id: String,
) -> Result<ImportReport, String>

// Import a Bruno workspace directory
#[tauri::command]
async fn import_bruno_workspace(
    path: String,
    create_new_workspace: bool,   // true = create new; false = add to current
    target_workspace_id: Option<String>,  // required if create_new_workspace = false
) -> Result<ImportReport, String>
```

---

## UI Flow

### Entry points (all three trigger the same flow)

- File menu → "Import" → "Bruno Workspace / Collection…"
- Workspace overview screen → "Import from Bruno" button
- Collections panel toolbar → import icon (alongside New Collection)

### `ImportBrunoDialog` — three internal states

**State 1: `picking`**
- Native file picker (directory selection) via Tauri dialog API
- Radio group: "Create new workspace" / "Add to current workspace" (shown only for workspace-level import; collection import always adds to current)
- "Import" button triggers the Tauri command

**State 2: `importing`**
- Spinner + "Importing…" label
- Non-dismissable

**State 3: `complete`**
- ✅ `{n} requests imported across {m} collections`
- ⚠️ `{k} items skipped` — expandable list (path + reason per item); hidden if k = 0
- "Open Collection" button (shown only when a single collection was imported)
- "Close" button

### Component
- Single shadcn/ui `Dialog` with internal `state: 'picking' | 'importing' | 'complete'`
- shadcn/ui `RadioGroup` for workspace choice
- shadcn/ui `Collapsible` for the skipped items list
- Lucide icons only

---

## Testing Strategy

| Layer | Test type | Coverage |
|---|---|---|
| `bru/lexer.rs` | Unit | Each block kind; empty blocks; multiline raw text; Windows line endings (`\r\n`) |
| `bru/parser.rs` | Unit | Each block type → correct `BruDocument` field; unknown blocks → `unknown_blocks` |
| `bru/yml_adapter.rs` | Unit | Bruno `.yml` structs normalise correctly to `BruDocument` |
| `converter/request.rs` | Unit | Each `BruDocument` field → correct domain `Request` field; unsupported types → `SkipReason` |
| `converter/environment.rs` | Unit | Vars + secret vars convert correctly |
| `importer.rs` | Integration | Full import of fixture Bruno collection directory → assert written files match expected state via `FsCollectionRepo::get_request()` |

Fixture files live at `crates/rocket-import/tests/fixtures/`.
