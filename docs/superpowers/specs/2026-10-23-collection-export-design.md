# Export a collection to Bruno and Postman

## Goal

Let a user export a Rocket collection as a Bruno collection ZIP, a Postman Collection v2.1 file and Postman or Bruno environment files that import into those apps, with credentials blanked and every dropped feature listed in an export report.

## Overview

Rocket can import Bruno and Postman collections, but it cannot export to them. A user who wants to share a collection with a colleague on Bruno or Postman, or move away, has to rebuild it by hand.

Research (`docs/features/collection-export-report.md`, 2026-10-23) found:

- Bruno 3 and Rocket use the same OpenCollection YAML layout on disk, so a Bruno export is mostly a scrubbed copy of the collection folder.
- Postman needs a new Collection v2.1 serializer.
- Bruno's own exporter leaks credentials in clear text, exports disabled params as enabled, loses collection variable values and drops items without telling the user. Rocket should avoid each of those.

The user asked to "implement the same as Bruno has implemented". This spec copies Bruno's three-output Share dialog (folder ZIP, Postman, environments) and fixes its known defects.

All decisions in `## Decisions` are the assistant's calls, made while the user was away. They need the user's review before the spec is final.

## Current state

| Piece | Today |
|---|---|
| On-disk layout | OpenCollection YAML: `opencollection.yml`, `folder.yml`, one `.yml` per request, `environments/<env>.yml` (`docs/superpowers/specs/opencollection-spec-reference.md` section 5, lines 589-620) |
| Bruno modern import | Copies files one by one; skips `node_modules`, `workspace.yml`, `_order.yml`; merges the root `opencollection.yml` (`crates/rocket-import/src/importer.rs:820-870`) |
| ZIP | Only read: `zip = "2"` is a `rocket-import` dependency, nothing writes a ZIP |
| Auth model | `Auth` enum in `crates/rocket-shared/src/types.rs:229-273`: None, Basic, Bearer, ApiKey, OAuth2, AwsSigV4, Inherit, Wsse, Digest, Ntlm, OAuth1 |
| Collection settings | `CollectionSettings` in `crates/rocket-collection/src/settings.rs:37-77`: docs, auth, headers, variables (with `secret`), plus Rocket-only trust-gate requests `sandbox_mode`, `script_context_roots`, `agent_autonomy_enabled`. It has no collection script field. |
| Export pattern | `ExportService::export` returns bytes plus an extension (`crates/rocket-app/src/export_service.rs:15-37`); `export_load_test` returns base64 (`src-tauri/src/commands/load_test.rs:54-64`) |
| Save helper | `saveTextFile` is text-only and uses `writeFile`, because only `fs:allow-write-file` is granted (`src/lib/save-file.ts`) |
| Layering | Domain crates hold logic and traits, `rocket-infra` does I/O, `rocket-app` is trait-first, commands are thin (`crates/rocket-app/CLAUDE.md`, `.claude/rules/rust-ddd-boundaries.md`) |
| Postman importer | Reads bearer, basic, apikey, raw/urlencoded/formdata bodies, variables and environments. It ignores `event` scripts, folder auth and variables, oauth2, file and graphql bodies (survey of `crates/rocket-import/src/converter/postman.rs`) |

Gaps: no exporter, no ZIP writer, no binary save helper, no secret scrub, no export report, no UI entry.

## Non-goals

- The legacy `.bru` format. Bruno 3 reads OpenCollection, and no `.bru` writer exists.
- Single-file OpenCollection YAML (Bruno's second option). Possible later.
- OpenAPI export of a collection. Contract export already exists separately.
- Extending the Postman importer so a Postman to Rocket to Postman round trip is lossless. A separate job.
- Exporting flows, gRPC requests or WebSocket requests to Postman. They are reported as skipped.
- Exporting vault values or host environment values.

## Design

Flow: collection context menu `Export...` -> dialog (format, options) -> Tauri command -> `rocket-app` export service -> pure converter in `rocket-export` -> bytes plus `ExportReport` -> frontend saves the file with a native dialog and shows the report.

### 1. Crate and ports

- New crate `crates/rocket-export`. Pure logic, no filesystem access. It depends on `rocket-shared`, `rocket-collection`, `rocket-environment`, `serde_json`, `serde_yaml` and `zip` (in-memory bytes only).
- `ExportReport { exported: usize, items: Vec<ExportNote> }` and `ExportNote { path: String, feature: String, action: ExportAction, detail: Option<String> }` with `ExportAction` = `Translated | Blanked | Skipped | Preserved`. IPC DTOs use camelCase; persistence structs do not exist (nothing is persisted).
- Port `CollectionFileSource` (trait in `rocket-export`): `fn list_files(&self, collection: &str) -> DomainResult<Vec<SourceFile>>` where `SourceFile { rel_path: String, bytes: Vec<u8> }`. `rocket-infra` implements it by walking the collection folder. The walk skips `.git` and `node_modules`.
- `rocket-app` gets `CollectionExportService` that takes `Box<dyn CollectionRepository>`, `Box<dyn EnvironmentRepositoryFactory>` and `Box<dyn CollectionFileSource>`.
- The two outputs read different data on purpose:
  - Bruno ZIP: raw files from `CollectionFileSource`, scrubbed as `serde_yaml::Value` (never round-tripped through typed structs, so unknown keys survive). Environment files in the ZIP are the on-disk `environments/*.yml`, which hold secret variable names only (values live in the keychain), so no keychain read is needed.
  - Postman: a typed `CollectionExportTree { name, docs, settings: CollectionSettings, root: ExportFolder }` with `ExportFolder { name, settings: FolderSettings, items: Vec<ExportItem> }` and `ExportItem` = `Request(Request) | Folder(ExportFolder) | Skipped { path, kind }`. The service builds it from `CollectionRepository::get_strict`, `get_settings` and `get_folder_settings` (a read error fails the export; the lenient `get_summaries` is never used). Environments come from `EnvironmentRepositoryFactory::for_collection(name).list_strict()`, and secret values are dropped by the converter immediately after the read.

### 2. Bruno collection ZIP

- `fn bruno_zip(files: Vec<SourceFile>, collection_name: &str) -> DomainResult<(Vec<u8>, ExportReport)>`.
- Includes `opencollection.yml`, every `folder.yml`, request `.yml`, `.proto` and shared `.js` files, and `environments/*.yml`. Excludes `.git`, `node_modules`, `workspace.yml`, and any file outside those types.
- Scrub applied to each YAML file after parsing it as `serde_yaml::Value`:
  - Remove the whole `extensions.rocketapi` subtree (Rocket's namespace, which holds script context roots), and the keys that hold `sandbox_mode` and agent autonomy. This is a blacklist of Rocket namespaces, so other `extensions.*` keys (for example `extensions.bruno`) stay. The builder locates the exact YAML keys in `crates/rocket-infra/src/oc/` before coding.
  - Blank `secret: true` variable values (collection, folder, request, environment).
  - Blank literal auth credentials, sensitive header values and URL passwords (see section 5). The builder reads the auth YAML shape in `crates/rocket-infra/src/oc/` and walks the `Value` by its `type` field at collection, folder and request level.
  - Keep `{{variable}}` references untouched.
  - Rocket scripts are copied unchanged. When any script uses the `rok.` API, add one `Preserved` note saying Bruno uses `bru.*`, so those scripts need editing there.
- The ZIP root is a single folder named after the collection, so Bruno's zip detection (`opencollection.yml` at the root or one level down) works.
- Example entry list: `my-api/opencollection.yml`, `my-api/users/folder.yml`, `my-api/users/get-user.yml`, `my-api/environments/local.yml`.

### 3. Postman Collection v2.1

- `fn postman_collection(tree: &CollectionExportTree, opts: &PostmanOptions) -> DomainResult<(serde_json::Value, ExportReport)>` where `PostmanOptions { preserve_scripts: bool }` (default false).
- `info.schema` is exactly `https://schema.getpostman.com/json/collection/v2.1.0/collection.json` (Bruno rejects other URLs). `info._postman_id` is a new UUID, `info.name` the collection name, `info.description` the collection docs.
- Items: folders first in `seq` then name order, requests by `seq` then name; sibling names that collide get ` (2)`, ` (3)`. A folder is `{name, item, auth?, event?}`.
- URL: `raw` holds only enabled query pairs; `query[]` lists all pairs with `disabled: true` on disabled ones; path params are `:id` in `raw` and `path` plus `variable[]`. `protocol`, `host` and `path` are built from the parsed URL, and when the URL has no scheme only `raw` is trusted.
- Headers: `{key, value, disabled, description}`. Collection and folder headers are copied onto each request (outermost first, inner key wins, disabled never shadows), because Bruno drops folder headers.
- Bodies: json, xml, text, sparql become `raw` with `options.raw.language` (json, xml, text) and a matching Content-Type header when none exists. formurlencoded becomes `urlencoded`, formdata becomes `formdata` (file entries `{type:"file", src}`), binary becomes `{mode:"file", file:{src}}`, graphql requests become `{mode:"graphql", graphql:{query, variables}}` with `variables` a JSON string.
- Auth per type: `type` plus a `[{key, value, type:"string"}]` array under a property named after the type.

  | Rocket `Auth` | Postman `type` | Keys |
  |---|---|---|
  | Bearer | `bearer` | `token` |
  | Basic | `basic` | `username`, `password` |
  | Digest | `digest` | `username`, `password` |
  | Ntlm | `ntlm` | `username`, `password`, `domain` |
  | ApiKey | `apikey` | `key`, `value`, `in` (`header` or `query`) |
  | AwsSigV4 | `awsv4` | `accessKey`, `secretKey`, `sessionToken`, `service`, `region` |
  | OAuth2 | `oauth2` | `grant_type` (`authorization_code`, `authorization_code_with_pkce`, `password_credentials`, `client_credentials`, `implicit`), `accessTokenUrl`, `refreshTokenUrl`, `authUrl`, `clientId`, `clientSecret`, `scope`, `state`, `redirect_uri`, `username`, `password`, `tokenName`, `addTokenTo` (`queryParams` or `header`), `headerPrefix`, `client_authentication` (`body` or `header`) |
  | Inherit | (no `auth` key) | |
  | None | `noauth` | |
  | OAuth1, Wsse | skipped, `noauth` emitted | reported |

  The OAuth2 key names follow Bruno's exporter and are unverified against a real Postman export (see Testing, manual checks).
- Collection `variable[]` holds the real collection variable values (secrets blanked), not empty placeholders.
- Request `examples` (saved responses) are not exported in this version: they hold captured response data that may contain tokens. Skipped and reported once per request that has them.
- Not exported, always reported: flows, gRPC and WebSocket requests, client certificates, proxy, per-request settings, the trust-gate keys.

### 4. Scripts and tests

- `preserve_scripts = true`: scripts go into `event[]` untouched.
- Default: translate a fixed table of exact call paths to `pm.*`. A small tokenizer skips string literals, template literals and comments, and rewrites a call only when its full member path matches a row, so text inside strings or comments is never changed. Aliases (`const r = rok.response`) and computed access are not followed; a script that uses them keeps the original text and is reported.

  | Rocket call | Postman call |
  |---|---|
  | `rok.getVar`, `rok.setVar` | `pm.variables.get`, `pm.variables.set` |
  | `rok.getEnvVar`, `rok.setEnvVar` | `pm.environment.get`, `pm.environment.set` |
  | `rok.getCollectionVar`, `rok.setCollectionVar` | `pm.collectionVariables.get`, `pm.collectionVariables.set` |
  | `rok.getGlobalEnvVar`, `rok.setGlobalEnvVar` | `pm.globals.get`, `pm.globals.set` |
  | `test(`, `expect(` | `pm.test(`, `pm.expect(` |
  | `res.getStatus()`, `res.status` | `pm.response.code` |
  | `res.getBody()` | `pm.response.json()` |
  | `res.getHeader(x)` | `pm.response.headers.get(x)` |
  | `req.getUrl()` | `pm.request.url.toString()` |
  | `req.getMethod()` | `pm.request.method` |

  The builder checks each left-hand name against `src/components/editor/rok-types.ts` and `docs/features/rok-api-parity-notes.md`, and drops a row whose Rocket name does not exist. A call that matches no row is left as it is with a `// rok: not translated` comment line above it, and reported.
- Pre-request script becomes a `prerequest` event; post-response script plus tests become one `test` event (a `// Tests` marker between them), as Bruno does. Assertions become `pm.test` lines for the operators `eq` (`.to.eql`), `neq` (`.to.not.eql`), `gt` (`.to.be.above`), `gte` (`.to.be.at.least`), `lt` (`.to.be.below`), `lte` (`.to.be.at.most`) and `contains` (`.to.include`); any other operator is skipped and reported. The builder reads `crates/rocket-shared/src/assertion.rs` for the real operator names and expression form. Set-variable actions become `pm.collectionVariables.set` lines (scope `collection`), `pm.environment.set` (scope `environment`) or `pm.variables.set` (other scopes).
- Folder scripts become folder `event[]`. Collection-level scripts do not exist in the Rocket model and are not exported.

### 5. Secret scrub (one policy for all formats)

Scope: the scrub covers auth fields, `secret` variables, header values for `Authorization`, `Proxy-Authorization`, `Cookie`, `Set-Cookie` and `X-Api-Key` (matched case-insensitively, same list as `src/lib/sensitive-headers.ts`), and a password inside URL userinfo (`https://user:pass@host`). Request bodies, scripts and descriptions are the user's own content and are not scanned; the export dialog says so. A literal that is only a `{{variable}}` reference is kept in every place.

`fn scrub_auth(auth: &Auth) -> (Auth, Vec<ExportNote>)`: for every credential field of the `Auth` variants (basic, digest, wsse, ntlm password; bearer token; api key value; AWS secret key and session token; OAuth2 client secret and password; OAuth1 consumer and token secrets and private key), keep the text when it is exactly one `{{variable}}` reference, otherwise replace it with an empty string and add a `Blanked` note naming the request path and field. Variables with `secret == true` keep their key and get an empty value. Environment variables with `secret` export as Postman `type: "secret"` with an empty value. Environments are read with `list_strict`, which may hydrate secret values from the keychain; the converter drops every `secret` value immediately after the read and never stores it.

### 6. Environments

- Collection environments only. Workspace-level (global) environments are out of scope for this version.
- Postman: `{id, name, values:[{key, value, type, enabled}], _postman_variable_scope:"environment", _postman_exported_at, _postman_exported_using:"Rocket/<version>"}`. `type` is `secret` for secret variables. File name `<env-slug>.postman_environment.json`.
- Output packaging for Postman: with `Include environments` off, the result is the single file `<collection-slug>.postman_collection.json`. With it on, the result is one ZIP `<collection-slug>-postman.zip` holding that file and `environments/<env-slug>.postman_environment.json`, so the dialog needs one save.
- Bruno: the existing environment YAML inside the ZIP (section 2). Unchecking `Include environments` leaves `environments/` out of the ZIP. No separate Bruno environment JSON in this version.
- `extends` is flattened with `rocket_environment` flatten before export so the file stands alone.
- `{{process.env.X}}` text is kept and reported. No host value is ever written.

### 7. IPC and frontend

- Commands in `src-tauri/src/commands/export.rs`: `export_collection_bruno(collection: String, include_environments: bool) -> ExportResultDto` and `export_collection_postman(collection: String, options: PostmanExportOptionsDto) -> ExportResultDto` where `PostmanExportOptionsDto { preserveScripts: bool, includeEnvironments: bool }`. `ExportResultDto { dataBase64: String, fileName: String, report: ExportReportDto }`. Commands only validate, call the service and map errors, as `export_load_test` does.
- Frontend: wrappers in `src/lib/tauri-api.ts`; `saveBinaryFile(defaultName, bytes, filters)` next to `saveTextFile` in `src/lib/save-file.ts`; `ExportCollectionDialog` (shadcn Dialog, Select, Checkbox, Button; lucide icons) in `src/components/export/`, opened by a new `Export...` item in the collection context menu in `src/components/collections/CollectionNode.tsx`. The dialog offers format (Bruno ZIP, Postman collection), `Preserve scripts` (Postman only), `Include environments` (default on), and after export shows the report grouped by action.

## Error handling and edge cases

| Case | Behaviour |
|---|---|
| Collection has no requests | Export succeeds with an empty `item[]` and a note |
| Duplicate sibling names | Suffix ` (2)`, ` (3)`; a `Translated` note |
| Request without `seq` (`Request.seq` is `Option<u32>`) | Order by name |
| Request with saved examples | Examples skipped, reported once for the request |
| Literal `Authorization` header or URL password | Blanked, reported |
| OAuth1 or Wsse auth | `noauth` emitted, reported |
| Workspace-level environments | Not exported |
| gRPC, WebSocket, flow, opaque item | Skipped, reported with the item path |
| Literal credential | Blanked, reported; `{{var}}` kept |
| Unknown or unmapped auth type | Skipped for that request (`noauth` emitted), reported |
| URL without scheme | `raw` only, no `host` or `path` split |
| `rok.*` call with no `pm.*` mapping | Comment marker plus original line, reported |
| Keychain locked while reading environments | Export fails with the repository error; no partial file |
| Save dialog cancelled | No file written, no error |
| File name | Collection name sanitised with the existing slug rules; unsafe characters replaced |

## Testing

- `rocket-export` unit tests: `bruno_zip_has_single_root_folder`, `bruno_zip_skips_git_node_modules_and_workspace_yml`, `bruno_zip_strips_trust_gate_keys`, `scrub_keeps_variable_reference_and_blanks_literal`, `scrub_blanks_secret_variable_values`, `postman_schema_url_is_v2_1_getpostman`, `postman_disabled_query_param_stays_disabled`, `postman_path_param_uses_colon_and_variable`, `postman_folder_headers_copied_onto_requests`, `postman_raw_body_sets_language`, `postman_formdata_file_entry_uses_src`, `postman_graphql_body_variables_is_json_string`, `postman_auth_table` (one case per auth type), `postman_noauth_and_inherit`, `postman_duplicate_names_get_suffix`, `postman_collection_variables_keep_values`, `postman_skips_grpc_websocket_flow_with_report`, `scripts_translate_common_calls`, `scripts_leave_comment_for_unmapped_call`, `preserve_scripts_keeps_text`, `postman_environment_marks_secret_type`, `environment_extends_is_flattened`, `scrub_blanks_sensitive_header_and_url_password`, `postman_skips_examples_with_report`, `bruno_zip_notes_rok_scripts`, `bruno_zip_without_environments_leaves_folder_out`, `postman_with_environments_is_one_zip`, `scripts_ignore_rok_text_inside_strings_and_comments`, `assertion_operators_map_and_unknown_is_reported`.
- `rocket-app`: `export_service_reads_via_repositories` using the in-memory doubles pattern in `crates/rocket-app`.
- `rocket-infra`: `file_source_lists_collection_files_and_skips_git` with `tempfile`.
- Golden fixture: a collection using every body, auth and variable type, exported to Postman JSON and compared with a checked-in file under `crates/rocket-export/tests/fixtures/`.
- Round trip: the Postman golden file imported by `rocket-import` for the supported fields (name, method, url, headers, bearer/basic/apikey, raw/urlencoded body).
- Frontend: `saveBinaryFile` test with mocked dialog and fs; `ExportCollectionDialog` test for format, options and report rendering.
- Manual checks, recorded in `.claude/manual-tests/`: schema validation with `ajv-cli` against the v2.1 draft-07 schema; `newman run` against a local mock; convert with `@usebruno/converters` and `bru run`; open the ZIP in Bruno 3; import into Postman.
- Commands: `cargo test -j4 -p rocket-export`, `cargo test -j4 -p rocket-app export`, `cargo test -j4 -p rocket-infra file_source`, `yarn test export`, `yarn tsc --noEmit`, `yarn check`.

## Files changed

- New: `crates/rocket-export/` (lib, `bruno.rs`, `postman.rs`, `scripts.rs`, `scrub.rs`, `report.rs`, `ports.rs`, tests, fixtures).
- New: `crates/rocket-app/src/collection_export_service.rs`.
- New: `crates/rocket-infra/src/fs_collection_file_source.rs`.
- New: `src-tauri/src/commands/export.rs`; wiring in `src-tauri/src/lib.rs`.
- Modified: `Cargo.toml` (workspace member), `src/lib/tauri-api.ts`, `src/lib/save-file.ts`, `src/components/collections/CollectionNode.tsx`.
- New: `src/components/export/ExportCollectionDialog.tsx` and tests.
- Docs: `crates/rocket-export/CLAUDE.md`, a line in the root `CLAUDE.md`, an item in `docs/roadmaps/roadmap.md`.

## Constraints

- Read `docs/superpowers/specs/opencollection-spec-reference.md` before planning any task that touches collection, environment or auth data.
- Domain logic stays in `rocket-export`; I/O only in `rocket-infra`; `rocket-app` holds trait objects only; commands stay thin.
- camelCase serde on IPC DTOs only. No panicking unwrap calls in production paths.
- cargo commands per crate with `-j4`; no workspace-wide test run.
- shadcn primitives and lucide icons only. Zustand state is read with narrow selectors.
- No secret variable value, vault value or host environment value may appear in any export or report. Literal credentials in auth, sensitive headers and URL passwords are blanked. Free text (bodies, scripts, descriptions) is not scanned.

## Acceptance criteria

- The collection context menu has an `Export...` item that opens a dialog with Bruno ZIP and Postman collection.
- A Bruno ZIP has one root folder and contains the collection's YAML, `.proto` and `.js` files and environments, and none of `.git`, `node_modules`, `workspace.yml` or trust-gate keys.
- A Postman export validates against the v2.1.0 draft-07 schema and uses the `schema.getpostman.com` v2.1.0 schema URL.
- Disabled query params, headers and form rows stay disabled; collection and folder headers appear on each request; collection variable values are kept.
- Every auth type in the mapping table exports with the Postman key names, `Inherit` omits `auth` and `None` emits `noauth`.
- Every literal credential and every secret variable value is empty in all outputs; `{{variable}}` references are kept.
- Skipped, blanked and translated items appear in the report and the dialog shows it.
- `rok.*` calls with a mapping become `pm.*`; unmapped calls leave a comment marker and a report entry; `Preserve scripts` keeps the text.
- The Postman environment file marks secrets as `type: "secret"` with empty values and flattens `extends`.
- Manual checks pass: ajv schema validation, `newman run`, Bruno converter plus `bru run`, Bruno 3 opening the ZIP, Postman importing the file.
- All listed tests pass; `yarn tsc --noEmit`, `yarn check` and `cargo check -j4 -p <crate>` for each touched crate are clean.

## Out of scope

- Legacy `.bru` output, single-file OpenCollection YAML, collection OpenAPI export.
- Importer changes for a lossless round trip.
- Collection-level scripts (not in the Rocket model).
- Exporting to Insomnia or other tools.

## Follow-ups

- Extend the Rocket Postman importer to read `event` scripts, folder auth and variables, oauth2, file and graphql bodies.
- Single-file OpenCollection YAML export.
- Postman Data Export and Insomnia targets.

## Decisions

- Which formats and in what order? Options: all three in one plan; Postman first; Bruno ZIP then Postman then environments and scripts. Chosen: Bruno ZIP, then Postman, then scripts and environments. Decided by: call
- Credentials: blank by default, or an include-credentials option? Options: blank always; blank with an opt-in override; export in clear like Bruno. Chosen: blank always, no override. Decided by: call
- Script translation in v1? Options: preserve only; translate the common `rok.*` subset; full translation. Chosen: translate the common subset with a Preserve scripts toggle. Decided by: call
- Environments: separate, always, or optional? Options: separate dialog; always included; optional checkbox. Chosen: optional checkbox, default on. Decided by: call
- Legacy `.bru` format? Options: write it; skip it. Chosen: skip. Decided by: call
- Lossless round trip through Rocket's importer? Options: include in this job; separate follow-up. Chosen: follow-up. Decided by: call
- UI entry point? Options: collection context menu; collection overview tab; both. Chosen: collection context menu. Decided by: call
- Crate placement: a new `rocket-export` crate rather than a module in `rocket-import`, because import and export have different dependencies and the importer's rule is to write only through repositories. Decided by: call
- Request examples are not exported, because captured responses may hold tokens. Decided by: call
- Strip policy for the Bruno ZIP: remove the `extensions.rocketapi` subtree plus the trust-gate keys, and keep other extension namespaces. Decided by: call
- Script translation uses a tokenizer and a fixed call table, not regex over raw text; aliases are not followed. Decided by: call
- Workspace-level environments are out of scope; Postman output with environments is one ZIP so the dialog needs a single save. Decided by: call
- The Bruno ZIP keeps Rocket scripts as they are and reports that they use the `rok.` API. Decided by: call
