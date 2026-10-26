# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

`rocket-import` orchestrates importing Bruno API client collections, Postman collections and WSDL 1.1 files into RocketAPI. It parses both `.bru` (DSL) and `.yml` (YAML) Bruno formats, converts the parsed AST into domain types, and delegates all disk writes to `FsCollectionRepo` and `FsEnvironmentRepo` from `rocket-infra`. No raw YAML is written by this crate.

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
| `converter/request.rs` | `convert(doc) → (Option<Request>, Vec<SkipReason>)`; `convert_item` routes gRPC documents to `converter/grpc.rs`, GraphQL documents to `convert_graphql` and WebSocket documents to `convert_websocket`, and returns a `Converted` (`Http`, `GraphQl`, `WebSocket` or `Grpc`) |
| `converter/grpc.rs` | `convert(doc) → (Option<GrpcRequest>, Vec<SkipReason>)`, `is_grpc(doc)` |
| `converter/environment.rs` | `convert(name, doc) → Environment` |
| `converter/collection.rs` | `convert_variables(kvs) → Vec<CollectionVariable>` |
| `wsdl/{ast,schema,sampler,parser}.rs` | WSDL 1.1 reader, owned XSD model, sample XML generation |
| `converter/wsdl.rs` | `convert_operation`, `build_envelope`, `soap_headers` |
| `openapi/{model,reader,layout}.rs` | OpenAPI 3.0, 3.1 and Swagger 2.0 reader into a neutral model (`OpenApiDoc`, `Operation`), and `GroupBy` folder layout |
| `converter/openapi.rs` | `convert_operation`, `request_name`, `security_variable_names`, `sample_json`, `baseline_hash` |
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
- **Non-fatal skips.** Unsupported auth types (e.g. OAuth2) produce a `SkipReason::UnsupportedAuthType` entry in the report but still import the request with `auth: None`. Unsupported request types produce `SkipReason::UnsupportedRequestType` and skip the entire request. A `.bru` file whose `meta.type` is neither `http` nor a type that imports is skipped too, instead of becoming an empty GET. WebSocket requests (`.bru` `meta.type: ws` with `ws {}` and `body:ws {}`, or YAML `ws:`/`websocket:`) import as `Converted::WebSocket` and are saved with `save_websocket_request`. gRPC requests (`.bru` `meta.type: grpc` with `grpc {}`, `metadata {}` and `body:grpc {}`, or the YAML `grpc:` block) import as `Converted::Grpc` and are saved with `save_grpc_request`. `.proto` files are copied next to the requests in both import paths, and a gRPC proto path is rebased to the collection root when the file can be found. Shared `.js` files are copied verbatim to the same relative path in both import paths (via `copy_support_file`), so scripts' `require('./shared-scripts/x')` keeps resolving; they are not counted as requests, symlinks are skipped and reported, and `node_modules` directories are never walked.
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
| `graphql` | `body:graphql {}` and `body:graphql:vars {}` blocks, or the OpenCollection `graphql:` block | `BruGraphQl { query, variables }` |
| `vars` | `vars {}` block (env files) | plain environment variables |
| `secret_vars` | `vars:secret []` block (env files) | secret variable names only — this block uses `[...]` list syntax (not `{}`); the lexer captures it as `RawText` and the parser splits on lines |
| `pre_request_script` | `script:pre-request {}` / YAML `http.script.req` | JS string |
| `post_response_script` | `script:post-response {}` / YAML `http.script.res` | JS string |
| `grpc` / `grpc_metadata` / `grpc_messages` | the `grpc {}`, `metadata {}` and `body:grpc {}` blocks, or the YAML `grpc:` block | `BruGrpc { url, method, method_type, proto_path, auth_mode }`, metadata entries and saved messages |
| `ws_messages` / `ws_auth_mode` | `body:ws {}` entries and the `auth:` mode inside `ws {}` / YAML `ws.messages` and `ws.auth.mode` | WebSocket messages (first one becomes selected) and `inherit`/`none` auth |
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

### WSDL import

- SOAP is an HTTP POST with `BodyMode::Xml`.
- Headers are set per binding version. SOAP 1.1 uses `text/xml` and a quoted `SOAPAction`. SOAP 1.2 uses `application/soap+xml; action="..."` and no `SOAPAction`.
- Layout is `<service>/<port>/<operation>`.
- Only local imports are followed. Remote locations produce `UnsupportedRequestType` report items.
- DTDs are rejected.
- XSD attributes, wildcards and `group` are ignored. `choice` uses its first alternative.
- The depth limit is `sampler::MAX_DEPTH`.
- rpc/encoded is generated as literal.
- WSDL 2.0 is rejected.

### OpenAPI import

Spec: `docs/superpowers/specs/2026-10-25-openapi-import-and-sync-design.md`.

```rust
service.import_openapi_text(text, source_label, GroupBy::Tags, workspace_id) -> ImportResult<OpenApiImportOutcome>
```

- Input is text (JSON or YAML) of OpenAPI 3.0, 3.1 or Swagger 2.0. There is no network code here; the caller fetches the text. Swagger 1.x and unknown versions give `ImportError::NotAnOpenApiSpec`.
- The reader walks the document as `serde_yaml::Value` so paths and operations keep their document order. Only local `#/...` `$ref` values are followed (depth cap 32, a looping reference becomes `{}`); a remote `$ref` becomes a report warning.
- `Operation.key` is the `operationId`, else `METHOD /path`; duplicates get `-2`, `-3`.
- Layout: `GroupBy::Tags` (default) uses the first tag, with `untagged` for none. `GroupBy::Paths` nests the literal path segments before the first `{param}`. Folder and file names go through `unique_segment`, so they are safe, unique and not reserved.
- A new collection is created each time (`Orders`, then `Orders-1`). Its variables are `baseUrl` (from `servers[0]`) and one empty secret variable per auth placeholder (`<scheme>_token`, `_username`, `_password`, `_value`). No secret value from a spec is ever imported.
- Request: name from `summary`, else `operationId`, else `METHOD /path`, cut to 100 characters. URL is `{{baseUrl}}/path/:param`. Optional query and header parameters are imported disabled. Cookie parameters, OAuth2 and unknown schemes are reported, not imported. JSON, form, multipart and XML bodies use the spec example, else a sample built from the schema (depth cap 8).
- At most 5,000 operations per import (`MAX_OPENAPI_OPERATIONS`); the rest are reported.
- The outcome lists `ImportedOperation { key, request_path, uid, baseline_hash }` for each written request. `baseline_hash` is a SHA-256 over method, URL, query and path params, headers, body and auth. It ignores name, order, scripts, tests, docs and examples, so a later sync can tell whether the user edited a request.

### Fetching and the source file

Plan 2 of the OpenAPI series (`docs/superpowers/plans/2026-10-25-openapi-import-and-sync/plan-02-fetch-and-source.md`). The import itself still takes text; these pieces get the text and remember where it came from.

- `parse_spec_url(text) -> SpecLocation` reads a pasted link. GitHub `blob`, `raw`, `raw.githubusercontent.com` (also `refs/heads/` and `refs/tags/`) and `api.github.com/repos/.../contents/...?ref=` links become `SpecLocation::Github { owner, repo, git_ref, path }`. The ref is the first segment after `blob`, so a ref with a `/` needs the contents-API form. Any other `https` link is `SpecLocation::Https { url }` without its fragment. `http`, `file`, links with `user:pass@`, `tree` (folder) links and text that is not a link are rejected with `ImportError::NotAnOpenApiSpec`.
- `fetch_target(&location, with_token) -> FetchTarget { url, accept, origin }`: GitHub without a token uses `raw.githubusercontent.com`; with a token it uses the contents API with `Accept: application/vnd.github.raw+json`. A token may only be sent to `origin`.
- `SpecFetcher` (port in `rocket-http`, `ReqwestSpecFetcher` in `rocket-infra`): https only, at most 5 redirects followed by hand, 30 s timeout, 25 MB cap enforced while reading, `If-None-Match` and `If-Modified-Since` supported. A redirect to `http` or to a loopback, private or link-local IP literal is refused (host names are not resolved, so DNS rebinding is not covered). The token is dropped on a redirect to another origin. Failures are `SpecFetchFailure` states (`NotFound`, `Unauthorized`, `RateLimited`, `TooLarge`, `NotHttps`, `TooManyRedirects`, `BlockedRedirect`, `Status`, `Network`). Used by `import_openapi_url` (see "Importing from a file or a link").
- `source_record(...) -> OpenApiSource` builds the record a collection keeps. `FsOpenApiSourceRepo` stores it at `<collection>/.rocket/openapi-source.yml` (snake_case keys, `atomic_write`). A dot folder is hidden from the collection loader and from the Bruno export. A corrupt file, or one with a higher `version`, is an error. The file holds the location, layout, spec hash, ETag, Last-Modified and one entry per request (key, uid, path, baseline hash). It never holds a token.

Sync state (Plan 4, task 1) lives in the same source file, all keys optional and skipped when empty: `source_id` (a v4 UUID, set at import; empty in older files), `sync_interval`, `last_checked_at` and `last_status` (legacy, read only since Plan 7: see "Sync timing"), and `pending_sha256`. A newer spec is kept as `<collection>/.rocket/openapi-pending.spec` through `save_pending`, `load_pending` and `clear_pending` on `OpenApiSourceRepository` (atomic write, a missing file is not an error). The file never holds a token.

### Sync timing

Since Plan 7 the interval and the check timing live in the local state that Git ignores (`.rocket/openapi-local.json`, `OpenApiLocalState`): `sync_interval` (`off`, `hourly`, `daily`, `weekly`, `monthly`; default `off`), `last_checked_at` (RFC 3339 UTC), `last_status` (`unchanged`, `changed` or `failed` with a machine `code`), `consecutive_failures`, `not_before` and `opt_in_root`. `set_interval` stores the canonical collection folder in `opt_in_root` (and clears it for `off`); the scheduler and `status()` treat a missing or different folder as `off`, so a local file that arrives with a copied, committed or moved folder never starts background checks until the interval is set again. Fields at their default are not written. The legacy `sync_interval`, `last_checked_at` and `last_status` of the source file stay readable but are never written. The old interval is ignored (a committed hourly source never turns checks on), and the old last check and status are only a fallback in `status()`.

`OpenApiSyncService` needs a local repository (`with_local`, or the one in the apply deps). Without one, `check`, `set_interval` and `status` return an error. `check` and `set_interval` write the local state under the collection lock. A failed check adds one to `consecutive_failures` and sets `not_before` from `Retry-After`; a finished one resets both. `apply` keeps the interval and timing it loaded. The pure rule `rocket_collection::next_check_at` (`openapi_schedule.rs`) gives the next due time: the interval plus a stable jitter (up to 10%, at most 5 minutes, from the source id), or after a failure the backoff ladder (15 min, doubling, capped at 6 h and at the interval), never before `not_before`. A missing, unreadable or future last check is due at once.

The check service (`OpenApiSyncService` in `rocket-app`, Plan 4 task 2) keeps the GitHub token in the keychain under the source id (`com.rocketapi.openapi-sync`, key `token`). A copied collection folder carries the same `source_id`, so it shares the token with the original. A keychain that cannot be read does not fail a check: the fetch goes out tokenless, and a 404 or 401 then says the keychain could not be read.

Baseline snapshot (Plan 5, task 1): `<collection>/.rocket/openapi-baseline.json` (`FsOpenApiBaselineRepo`, JSON, snake_case struct keys, `atomic_write`) holds `version`, `converter_version` (`converter::openapi::CONVERTER_VERSION`, bump it on any change to `convert_operation` output), `base_url` (`servers[0]`) and one `{key, request}` per imported operation, with the request exactly as saved (name suffix included). `ImportedOperation.baseline` carries it. A corrupt file or one with a higher `version` is an error. A link import writes it after the source file; a failed write is a warning ("The review will be cautious until the next sync."), and a file import writes none. `rocket_infra::ensure_gitignore` keeps `<collection>/.rocket/.gitignore` listing `openapi-baseline.json` and `openapi-pending.spec`. It only appends missing lines, and the baseline `save` and `save_pending` both call it. The file never holds a token.

### Sync plan

Plan 5, task 3. Code is in `openapi/sync/{plan,rename}.rs`. `build_sync_plan(&SyncInputs) -> SyncPlan` is pure: it writes nothing and does no I/O.

- Inputs: the source record, the baseline (optional), the incoming `OpenApiDoc`, the collection's `CurrentRequest` list (`uid`, `request_path`, `request`), the current `baseUrl` value, the existing variable names, and `skips` (operation key to the fingerprint it was skipped at).
- Output: `SyncPlan { items, variable_changes, skipped_hidden, deleted_by_user, cautious, warnings }`. `SyncItem` is `New`, `Changed` (with a `MergeOutcome`) or `Removed` (with `edited: Option<bool>`). Order: New and Changed in the incoming document order, then Removed in source order.
- Identity is the stored operation key plus the request `uid`, never the path. A key whose request is gone goes to `deleted_by_user` and is not recreated, even when the spec removed it too.
- Normal mode needs a baseline with the running `CONVERTER_VERSION` that holds the key: `merge_request(baseline, current, incoming)`, and a Changed item only when `changes` is not empty. `Removed.edited = Some(request_is_edited(..))`.
- Cautious mode (no baseline, another converter version, or the key missing from the baseline) works per request. If `baseline_hash(current)` equals the stored `baseline_hash`, the request counts as untouched and the spec's differences are `TakeSpec`, except `name` and `description`, which the hash does not cover and so stay `KeepYours`. Otherwise `merged` is the current request and every change is `KeepYours`. `MergeOutcome.cautious` is true and `Removed.edited` is `None`; the apply step must treat `None` as edited. Plan 6 applies a user's choices with `merge_item(baseline, source_op, current, incoming, choices)`, which makes the same trust and cautious decisions as the plan and then applies only the chosen fields; never call `merge_request(current, current, ..)` for this, it would take every field from the spec. `merge_item` also carries a ` (n)` duplicate-name suffix of the baseline (or current) name onto the incoming request, so a repeated name is not a false change. `VariableChange.baseline` is the trusted baseline `baseUrl`; `None` means the user's value counts as edited.
- `fingerprint(request)` is a SHA-256 over the `baseline_hash` fields plus name and description. A Changed or New item carries the fingerprint of the spec's request; a Removed item uses the fixed text `removed`. A skip hides an item only when the fingerprints are equal.
- Rename hints are a hint only. A Removed and a New item pair when they are the single candidate on both sides by (method, URL) or by (method, name). Any ambiguity, by either rule, gives no hint. Hash-map grouping keeps it linear.
- Variables: `baseUrl` follows the scalar rule (listed when the spec differs from the baseline and from the user's value; with no baseline the user's value is the baseline), and `AddSecurity` lists scheme variables not in `existing_variables`.

### Apply

Plan 6, task 2. Code is in `openapi/sync/{apply,apply_types,apply_paths,spec}.rs`. `build_apply(&ApplyInputs) -> ImportResult<ApplyOutput>` is pure: it returns file changes and new state, and the caller stages them in one `CollectionFileTx`.

- `ParsedSpec::parse(text)` is the public entry; `SyncInputs.incoming` is a `&ParsedSpec`.
- Inputs: the plan inputs, `pending_sha256`, a `Selection` (approved items with `item_digest` and choices, skips, variable names, rename links), a `RequestRenderer`, `existing_paths` (every file of the collection relative to its root, requests and `folder.yml`) and the collection settings.
- `RequestRenderer` has four methods: `render`, `render_settings`, `render_folder` (bytes of a new `folder.yml`) and `can_rewrite` (the round-trip guard: false when the request file holds more than Rocket models). No YAML is written in this crate.
- `item_digest` covers key, kind, fingerprint, each change, the request uid and path, the removal state and the merged result. An approved item whose digest differs from the fresh plan is `stale` and nothing changes for it.
- New: folder and file names go through `unique_segment` with the importer's reserved lists, a taken stem gets `-2` and the name gets ` (n)`. A folder is reused in any letter case; a missing one is created with a `folder.yml`. Every path passes `rocket_collection::validate_rel_path` and no path repeats (case-folded).
- Changed: `merge_item_indexed` with the user's choices, source `baseline_hash` and `request_path` updated, the baseline entry replaced by the spec's request (carried name, uid of the current request). A request that fails `can_rewrite` goes to `not_rewritable` and stays.
- Removed: deleted only when `edited == Some(false)`; otherwise detached (source and baseline entries dropped, file kept).
- Variables: `baseUrl` changes only when `VariableChange.baseline` equals the current value; `AddSecurity` adds a secret empty variable once.
- `plan_with_links(&SyncInputs, links) -> ImportResult<SyncPlan>` is the plan `build_apply` works from once rename links are applied. Compute shown and approved digests on it, so both sides agree.
- `RequestRenderer` contract (four methods): `render(request_path, request)`, `render_settings(settings)`, `render_folder(folder_path)` and `can_rewrite(request_path, request)`. Request paths are extensionless; the file is `request_filename_for(path)`.
- `existing_paths` must list every file of the collection, `folder.yml` files included. A folder with no listed file counts as gone and is created again with a `folder.yml`.
- A new baseline (none existed) records the spec's first server URL, never the user's current `baseUrl`.
- A variable change the user did not select does not block finalize. It stays in the plan's `variable_changes` until it is selected when a baseline already exists (the baseline does not move for it). With no baseline, the new baseline is seeded from the spec, so an unselected change is not listed again.
- A generated name whose path fails the transaction rules falls back to `unnamed` (with the usual unique suffix); the apply never fails on a hostile name.
- Finalize when `remaining == 0` and nothing is stale: `spec_sha256 = pending_sha256`, pending cleared, status `Unchanged`. `converter_version` is never changed.

### Applying through the transaction (Plan 6, task 3)

- `OpenApiSyncService::apply(collection, &Selection)` lives in `rocket-app` (`openapi_sync_apply.rs`). It runs `recover`, checks `sha256(pending) == pending_sha256` (else `Conflict`), flattens the HTTP requests (uid and extensionless path), lists the files through `CollectionFileTxFactory::list_files`, calls `build_apply`, and stages every `FileChange` plus the source (YAML), baseline and local state (JSON) and, when `finalized`, a delete of the pending copy.
- After any `Err` from `commit`, `apply` calls `recover` for the collection and answers by its outcome: `Finished` is success (every change landed), a rollback or nothing is the error plus "nothing was changed", a failed recovery is an error saying the apply is finished or undone at the next launch. `check`, `set_interval` and `set_token` also settle a journal first when apply deps are set.
- Variables: `Selection.variables` is `Vec<VariableApproval { name, digest }>` with `variable_digest(&VariableChange)`; a mismatch with the fresh plan is `stale` (nothing changes for it, finalize is blocked). A Removed request is deleted only when unedited and `can_rewrite` holds; a request stored only as `.yaml` is detached, never deleted or rewritten.
- Locks: one process-wide mutex per collection root (`collection_lock`) is held by `apply`, `recover`, `set_interval` and `set_token`; `check` takes it only after its fetch, reloads the source and writes just its own fields. A pending hash without its file drops the validators (and a 304 then clears the hash), and a re-fetched body rewrites a missing pending copy. `apply` and `recover_all` use `pinned()` once.
- `RequestRenderer` lives in `rocket-collection` (re-exported here). `FsRequestRenderer::render_settings` starts from the real `opencollection.yml` so name, uid, docs and extensions survive, and refuses without a collection root.
- `FsRequestRenderer` (`rocket-infra`) implements all four `RequestRenderer` methods over a throwaway `FsCollectionRepo`. `can_rewrite` needs `FsRequestRenderer::for_collection(root)`; it re-renders the request and compares YAML values with the file, and fails closed without a root.
- `recover(collection)`, `recover_all()` and `local_state(collection)` are on the same service. `src-tauri` runs `recover_all` on a background task at startup and after `switch_workspace`.
- Wire the dependencies with `OpenApiSyncService::with_apply(ApplyDeps { .. })`; without it `apply` and `recover` refuse.

### Importing from a file or a link

Plan 3 of the OpenAPI series. Code is in `openapi/entry.rs`; both methods return `OpenApiImportResult { report, collection, source_url, source_id, warnings }` (`source_id` is `None` for a file import).

- `import_openapi_file(path, group_by, workspace_id)` opens the file once, rejects a non-regular file or a length over 25 MB (`ImportError::SpecUnavailable`), then reads at most the cap with `read_capped`. It writes no source file and `source_url` is `None`.
- `import_openapi_url(fetcher, sources, baselines, link, group_by, token, workspace_id)` (async) runs `parse_spec_url`, `fetch_target(.., token.is_some())`, the fetch, then `import_openapi_text`, then saves the source file and the baseline. A failed fetch or a `NotModified` answer is `SpecUnavailable` and creates nothing; a body that is not a spec is `NotAnOpenApiSpec`.
- A source file that cannot be saved is a warning ("Sync could not be set up"), never an error. A GitHub ref of 40 hex characters warns that the commit never changes; 7 to 39 hex characters get a hedged warning, because it may be a branch name.
- `NotFound` and `Unauthorized` share one message, because GitHub answers 404 for a private repository; without a token it says a private GitHub repository needs an access token. With a token (`fetch_failure_message(_, true)`) it says the token was rejected or lacks Contents read access.
- Token (Plan 4, task 3): a blank token counts as none. With a token and a GitHub link the fetch uses the contents API and a bearer; with a token and any other link it returns `SpecUnavailable` ("Tokens work with GitHub links only.") before any fetch. The import never stores the token. The Tauri command saves it with `OpenApiSyncService::set_token` after a successful import; if that fails the import still succeeds with a warning. The token never reaches a file, a warning or an error text.
- `GroupBy::parse` accepts exactly `tags` and `paths`.

### Remote references

Plan 5b, task 1. Code is in `openapi/bundle.rs`. The reader follows only `#/` pointers, so `bundle_spec(fetcher, text, location, token, previous)` (async) inlines the files a spec refers to by URL into one text before parsing.

- Each referenced file lands under the root key `x-rocket-remote` as `"0"`, `"1"` ... (first encounter, breadth first). `URL#/a/b` becomes `#/x-rocket-remote/<i>/a/b`; a `#/a/b` inside file `i` becomes the same prefix. `URL` or `URL#` points at the whole file. A spec with no followable reference comes back as the same text, byte for byte.
- Relative references resolve against the main spec's raw address (`fetch_target(location, false).url`) and each file's own address, using the `url` crate. `inline_refs` (pure) only knows absolute addresses; `bundle_spec` handles the main spec's relative ones.
- Not followed, with one note each (file name or host, never a query): non-https, a user name or password, a query string, an address `redirect_target_allowed` refuses, a fragment that is not a `/` pointer. No local path is ever read.
- Limits: 20 files, depth 3, 25 MB in total with the main text. One note per limit; `Bundle.incomplete` is true for a failed or unusable file and for a limit. A file is fetched once however often it is referenced.
- Token: only to a file whose `parse_spec_url` is a GitHub location in the same owner and repo as the main one (through `fetch_target(.., true)`); every other file is a plain request.
- YAML texts are counted before any value is built (`openapi/yaml_budget.rs`, `YamlBudget`): every node and every string or key byte, aliases counted each time they are replayed. The node limit scales with the input: `node_limit_for(bytes)` = 4 nodes per input byte, at least 1,000,000 and at most 20,000,000; string bytes are capped at 25 MB. A text without aliases has fewer nodes than bytes, so only an alias bomb (small input, huge expansion) hits it. The bundler uses ONE shared budget for the main text and all files; its node limit is `node_limit_for` of the main text plus every body accepted so far (a file is judged with itself counted). A file that passes it is not loaded (note, `incomplete`), the files after it are not fetched (one note), and an over-budget main text comes back unchanged. `parse_openapi` runs the same check with `node_limit_for(text.len())` and 25 MB, so a bundle the bundler made is never refused for node count, and answers `NotAnOpenApiSpec("the file is too large or too complex to read")`. Bodies starting with `{` (after a BOM) are read as JSON.
- `bundle_spec` always fetches fresh bodies and records ETag and Last-Modified in `Bundle.refs` (`RemoteRefState`, defined in `rocket-collection`). `refs_unchanged` sends those validators and is true only if every file answers 304.

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
import_wsdl(path: String, target_workspace_id: String) -> Result<ImportReport, String>
```

### Dependencies

- `rocket-shared` — `DomainError`, `DomainResult`, HTTP types (`HttpMethod`, `Auth`, `Body`, `Header`)
- `rocket-collection` — `Request`, `CollectionRepository`, `CollectionVariable`
- `rocket-environment` — `Environment`, `Variable`, `EnvironmentRepository`
- `rocket-workspace` — workspace types
- `rocket-infra` — `FsCollectionRepo`, `FsEnvironmentRepo` (concrete I/O)
- `serde` / `serde_yaml` — YAML deserialization for `.yml` format
- `roxmltree` — WSDL and XSD XML parsing
- `thiserror` — `ImportError` derive
- `tempfile` (dev) — integration test fixtures

### Non-Obvious Gotchas

- **`_workspace_id` is currently ignored.** `import_collection` accepts a `_workspace_id` parameter for future use but does not route writes by it — all writes go to `self.workspace_path`. The parameter exists for API compatibility with the planned workspace-aware path.
- **A Bruno 3 collection (`opencollection.yml` at the root) is copied, not converted.** `import_modern_collection` copies request, `folder.yml` and environment `.yml` files byte for byte, merges only the collection-level keys of the root `opencollection.yml`, and drops `_order.yml`. Nothing is parsed or validated, so every copied file counts as imported even if Rocket cannot open it. The "No raw YAML writes" rule above applies to the legacy path only. Rocket's readers must therefore tolerate what Bruno omits (see `crates/rocket-infra/CLAUDE.md`, "Bruno 3 files are read as they are").
- **Environment name comes from file stem, not YAML.** `BruYmlEnv.name` is parsed but never used. The environment name is derived from the file stem (`local.yml` → `"local"`). This mirrors how `FsEnvironmentRepo` stores environments.
- **`~` prefix is stripped by the parser.** In `.bru` files, a leading `~` marks a disabled header/var. The parser strips the `~` from the key and sets `disabled = true`. The stored `BruKeyValue.key` never contains `~`.
- **Do not add `#[derive(Default)]` to `ImportService`.** `PathBuf::default()` is an empty path (`""`), which would cause silent failures when the service tries to write files. Always construct via `new()` or `new_with_workspace_path()`.

### Known Deferred Work

- `converter/collection.rs::convert_variables` is implemented but not yet called from `ImportService`. Bruno's `bruno.json` at the workspace level may carry collection-level variables; wiring this up is deferred.
