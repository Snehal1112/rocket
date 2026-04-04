# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

`rocket-import` orchestrates importing Bruno API client collections into RocketAPI. It parses both `.bru` (DSL) and `.yml` (YAML) Bruno formats, converts the parsed AST into domain types, and delegates all disk writes to `FsCollectionRepo` and `FsEnvironmentRepo` from `rocket-infra`. No raw YAML is written by this crate.

## Commands

```bash
# Check this crate
cargo check -p rocket-import

# Run all tests (unit + integration)
cargo test -p rocket-import

# Run a specific test by name
cargo test -p rocket-import <test_name>
```

## Architecture

### Module Map

| Module | Responsibility |
|---|---|
| `error.rs` | `ImportError` enum + `ImportResult<T>` alias |
| `report.rs` | `ImportReport`, `SkippedItem`, `SkipReason` |
| `bru/ast.rs` | `BruDocument` AST — unified output for both `.bru` and `.yml` |
| `bru/lexer.rs` | Tokeniser for the `.bru` DSL format |
| `bru/parser.rs` | Parser — produces `BruDocument` from the token stream |
| `bru/yml_adapter.rs` | serde structs + adapter for Bruno YAML format |
| `bru/mod.rs` | `parse_file()` and `parse_env_file()` — dispatch by file extension |
| `converter/request.rs` | `convert(doc) → (Option<Request>, Vec<SkipReason>)` |
| `converter/environment.rs` | `convert(name, doc) → Environment` |
| `converter/collection.rs` | `convert_variables(kvs) → Vec<CollectionVariable>` |
| `importer.rs` | `ImportService` — top-level orchestrator |

### Data Flow

```
Bruno directory
  → bru::parse_file / parse_env_file
  → BruDocument (unified AST)
  → converter::{request, environment, collection}
  → domain types (Request, Environment, CollectionVariable)
  → FsCollectionRepo::save_request / FsEnvironmentRepo::save
  → ~/.rocket-api/workspaces/<workspace>/collections/<name>/
```

### Public API

```rust
// In src-tauri/src/commands/import.rs
ImportService::new()                              // uses ROCKET_WORKSPACE_PATH env var
ImportService::new_with_workspace_path(&path)     // explicit path (used in tests)

service.import_collection(path, workspace_id) -> ImportResult<ImportReport>
service.import_workspace(path, create_new, target_id) -> ImportResult<ImportReport>
```

### Key Design Rules

- **No raw YAML writes.** All disk I/O goes through `FsCollectionRepo` and `FsEnvironmentRepo` from `rocket-infra`. This ensures the written files conform to the OpenCollection format.
- **Unified AST.** Both `.bru` (DSL) and `.yml` (YAML) Bruno formats produce a `BruDocument`. Converters only see `BruDocument`, never the raw source format.
- **Non-fatal skips.** Unsupported auth types (e.g. OAuth2) produce a `SkipReason::UnsupportedAuthType` entry in the report but still import the request with `auth: None`. Unsupported request types (GraphQL, gRPC, WebSocket) produce `SkipReason::UnsupportedRequestType` and skip the entire request.
- **Name conflict resolution.** If a collection with the target name already exists, the importer appends `-1`, `-2`, etc. until a free name is found. Checked by directory existence, not by querying the repo.
- **`environments/` is skipped during request walk.** `import_environments` handles it separately via `FsEnvironmentRepo`.

### BruDocument Fields

| Field | Source | Usage |
|---|---|---|
| `meta` | `meta {}` block / YAML `meta:` | `name`, `request_type`, `seq` |
| `method` | method block name (`get {`, `post {`) / YAML `http.method` | `HttpMethod` |
| `url` | `url:` in method block / YAML `http.url` | request URL |
| `headers` | `headers {}` block / YAML `http.headers` | `Vec<BruKeyValue>` — `~` prefix is **stripped** from the key and `disabled` is set to `true` |
| `body` | `body:json {}`, `body:text {}`, etc. | `BruBody` enum variant |
| `auth` | `auth:bearer {}`, `auth:basic {}`, etc. | `BruAuth` enum variant |
| `vars` | `vars {}` block (env files) | plain environment variables |
| `secret_vars` | `vars:secret []` block (env files) | secret variable names only — this block uses `[...]` list syntax (not `{}`); the lexer captures it as `RawText` and the parser splits on lines |
| `pre_request_script` | `script:pre-request {}` / YAML `http.script.req` | JS string |
| `post_response_script` | `script:post-response {}` / YAML `http.script.res` | JS string |
| `unknown_blocks` | unrecognised or non-HTTP request types | feeds `ImportReport.skipped` |

### Converter Mappings

**request.rs** — `BruDocument` → `rocket_collection::Request`:
- `BruMethod::*` → `rocket_shared::types::HttpMethod::*` (1:1)
- `BruBody::Json/Text/Xml` → `Body { mode, content: Some(s), .. }`
- `BruBody::FormUrlEncoded(kvs)` → `Body { mode: FormUrlEncoded, content: "k=v&k2=v2" }` (simple join)
- `BruBody::Multipart(kvs)` → `Body { mode: FormData, form_data: Vec<FormDataEntry> }`
- `BruAuth::Bearer/Basic/Digest/ApiKey/AwsV4` → matching `Auth::*` variants
- `BruAuth::Unknown` / `unknown_blocks[auth]` → `Auth::None` + `SkipReason::UnsupportedAuthType` (note: `BruAuth::Unknown` is never constructed by the parser — unsupported auth modes go to `unknown_blocks` instead)
- `meta.seq` → `req.seq`; `disabled` headers use `Header::disabled()` not `Header::new()`
- `~` stripping also applies to `vars {}` keys and `body:form-urlencoded` / `body:multipart-form` keys — same rule everywhere

**environment.rs** — `BruDocument` → `rocket_environment::Environment`:
- `doc.vars` → `Variable::new(key, value)`, `enabled = !kv.disabled`
- `doc.secret_vars` → `Variable::secret(name, "")` (empty value, `secret: true`)

**collection.rs** — `&[BruKeyValue]` → `Vec<CollectionVariable>`:
- `disabled` → `!enabled`; `initial_value` and `secret` are always empty/false (Bruno collection vars have no secret flag in the vars block)

### Fixture Files

Integration tests live in `tests/integration_test.rs` and use `tests/fixtures/my-api/`:

```
my-api/
  bruno.json               ← collection root marker
  get-users.bru            ← GET request (seq 1)
  create-user.yml          ← POST request in Bruno YAML format (seq 2)
  environments/
    local.yml              ← env with baseUrl variable
  auth/
    bruno.json             ← folder marker
    login.bru              ← POST request with JSON body + basic auth (seq 1)
```

### Tauri Commands

Registered in `src-tauri/src/commands/import.rs` (following project convention — commands live in `src-tauri`, not `rocket-app`):

```rust
import_bruno_collection(path: String, target_workspace_id: String) -> Result<ImportReport, String>
import_bruno_workspace(path: String, create_new_workspace: bool, target_workspace_id: Option<String>) -> Result<ImportReport, String>
```

### Dependencies

- `rocket-shared` — `DomainError`, `DomainResult`, HTTP types (`HttpMethod`, `Auth`, `Body`, `Header`)
- `rocket-collection` — `Request`, `CollectionRepository`, `CollectionVariable`
- `rocket-environment` — `Environment`, `Variable`, `EnvironmentRepository`
- `rocket-workspace` — workspace types
- `rocket-infra` — `FsCollectionRepo`, `FsEnvironmentRepo` (concrete I/O)
- `serde` / `serde_yaml` — YAML deserialization for `.yml` format
- `thiserror` — `ImportError` derive
- `tempfile` (dev) — integration test fixtures

### Non-Obvious Gotchas

- **`_workspace_id` is currently ignored.** `import_collection` accepts a `_workspace_id` parameter for future use but does not route writes by it — all writes go to `self.workspace_path`. The parameter exists for API compatibility with the planned workspace-aware path.
- **Environment name comes from file stem, not YAML.** `BruYmlEnv.name` is parsed but never used. The environment name is derived from the file stem (`local.yml` → `"local"`). This mirrors how `FsEnvironmentRepo` stores environments.
- **`~` prefix is stripped by the parser.** In `.bru` files, a leading `~` marks a disabled header/var. The parser strips the `~` from the key and sets `disabled = true`. The stored `BruKeyValue.key` never contains `~`.
- **Do not add `#[derive(Default)]` to `ImportService`.** `PathBuf::default()` is an empty path (`""`), which would cause silent failures when the service tries to write files. Always construct via `new()` or `new_with_workspace_path()`.

### Known Deferred Work

- `converter/collection.rs::convert_variables` is implemented but not yet called from `ImportService`. Bruno's `bruno.json` at the workspace level may carry collection-level variables; wiring this up is deferred.
