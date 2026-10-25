# Collection export (Bruno and Postman): research report

Status: research only. Nothing is built and no spec is written. Prepared 2026-10-23 while the user was away. The brainstorming interview has not happened yet; the open questions at the end need the user's answers first.

Request: export a Rocket collection so it imports into Bruno or Postman and works as expected, "same as Bruno has implemented".

## Recommendation in short

Mirror Bruno's Share dialog with three outputs, built in this order:

1. **Bruno collection ZIP.** Rocket's on-disk format is already OpenCollection YAML, which Bruno 3 reads natively. Export is a scrubbed copy of the folder, zipped. Smallest job, highest value.
2. **Postman Collection v2.1 JSON.** A new serializer. This is the real work.
3. **Postman environment JSON** (plus the Bruno environment shape), exported next to the collection.

Single-file OpenCollection YAML (Bruno's second option) is optional and cheap once the ZIP exists.

Always return an export report listing what was translated, dropped or blanked. Bruno silently drops items; Rocket should not.

Size: a project with a written design, three plans (see "Suggested split").

## 1. What Bruno does (read from usebruno/bruno `main`)

Trigger: collection `...` menu, **Share**. Source: `packages/bruno-app/src/components/ShareCollection/index.js`.

| Output | How | Notes |
|---|---|---|
| Bruno Collection (ZIP), default | IPC `renderer:export-collection-zip` in `packages/bruno-electron/src/ipc/collection.js` | Zips the collection folder as-is. Skips `node_modules` and `.git`. No conversion, no secret scrub. Import detects `bruno.json` or `opencollection.yml` at the root or one level down. |
| Single File (YAML) | `utils/exporters/opencollection.js` | `brunoToOpenCollection(...)` plus `extensions.bruno.exportedAt` and `exportedUsing`. Dumped with js-yaml. |
| Postman (under "Other Format") | `exportPostmanCollection` in `utils/exporters/postman-collection.js`, converter `packages/bruno-converters/src/postman/bruno-to-postman.js` | One option, "Preserve scripts" (export without translating). Warns that gRPC and WebSocket requests are not exported. Output path escape check and an overwrite prompt. |
| Environments | IPC `renderer:export-environment`, separate dialog | Per-environment JSON or one file, with `info.type`, `exportedAt`, `exportedUsing`. Postman output contains no environments. |
| OpenAPI | `utils/exporters/openapi-spec.js` | Separate feature, out of scope here (Rocket already exports OpenAPI for contracts). |

Bruno's Postman output (verified in code): top level `info`, `item`, `variable`, `auth`, `event`. Schema URL `https://schema.getpostman.com/json/collection/v2.1.0/collection.json`. Folders nest with their own auth and events. Test and post-response scripts merge into one `test` event. Auth mappings cover bearer, basic, apikey, digest, ntlm, awsv4, edgegrid, oauth1, oauth2. Scripts translate `bru.*`, `req.*`, `res.*` to `pm.*` through an AST pass (jscodeshift).

### Bruno limitations Rocket should not copy

- Disabled query and path params are exported as enabled.
- `variable` holds empty placeholders for every `{{token}}` found; real collection variable values are lost.
- Auth credentials are written to the file in clear text, with no secrets option. The ZIP export also copies everything on disk.
- Folder headers, folder variables and the binary `file` body are dropped.
- gRPC, WebSocket and the file body are dropped silently (only a UI note).
- Item names are not de-duplicated, and items without `seq` sort unpredictably.
- URL split: `host` is split on dots and `path` on slashes, which can mangle protocol-less URLs with a query.

## 2. What Rocket has today (fact sheet, paths from the survey)

- **Model.** `Collection`, `Folder`, `CollectionItem` (Request, Folder, GraphQl, WebSocket, Grpc, OpaqueItem, Summary, ScriptFile) in `crates/rocket-collection/src/{collection.rs,folder.rs}`. HTTP `Request` in `request.rs:11-58`. Shared types (`Auth` variants, `Body`, `FormDataEntry`, `Header`) in `crates/rocket-shared/src/types.rs`. Folder settings in `folder_settings.rs`, collection settings in `settings.rs:37-77`. Exporters must read through `CollectionRepository::get` or `get_strict`, not `get_summaries`.
- **Environments.** `crates/rocket-environment/src/{environment.rs,variable.rs,flatten.rs}`. Secret values live in the OS keychain, not the YAML. `FsEnvironmentRepo::with_secret_store` hydrates real values, so an exporter must blank every `secret == true` variable.
- **Importers.** `crates/rocket-import`. A modern Bruno (`opencollection.yml`) collection is copied file by file (`importer.rs:516`, `copy_collection_files` at `:827`). So Rocket and Bruno 3 share a disk format. Legacy Bruno goes through a `.bru` parser (no writer exists). The Postman importer (`converter/postman.rs`) maps bearer, basic, apikey, raw/urlencoded/formdata bodies, variables, and environments.
- **Export plumbing that exists.** None for collections. No ZIP writer anywhere (`zip = "2"` is a rocket-import dependency and only reads). Reusable patterns: `ExportService::export` returns bytes plus an extension (`crates/rocket-app/src/export_service.rs`); `export_load_test` returns base64 (`src-tauri/src/commands/load_test.rs:54`); `src/lib/save-file.ts` `saveTextFile` is text-only (a binary variant is needed; only `fs:allow-write-file` is granted); redaction helpers in `src/lib/flow-export.ts` and `src/lib/sensitive-headers.ts`.
- **UI.** Import lives in `src/components/import/ImportCollectionDialog.tsx`. The natural Export entry is the collection context menu in `src/components/collections/CollectionNode.tsx` (shadcn `ContextMenu`, items near lines 532-585).
- **Roadmap.** No export item exists in `docs/roadmaps/roadmap.md`. R-31 (real PDF export) is unrelated.

### Gaps that matter

1. No exporter module, DTOs, IPC command, TS wrapper, ZIP writer, or binary save helper.
2. Collection-level scripts are not in the domain model (`CollectionSettings` has no script field), so they cannot be exported. Folder scripts exist.
3. The Rocket Postman importer ignores `event` scripts, folder auth and variables, oauth2, file bodies and graphql bodies. A Postman, Rocket, Postman round trip is lossy today. This limits how strong an automated round-trip test can be.
4. Legacy `.bru` output has no writer. Not recommended (see open questions).
5. Layering to respect: domain crates hold logic and traits only, `rocket-infra` does I/O, `rocket-app` is trait-first, commands are thin, camelCase serde on IPC DTOs only, no panicking unwrap calls in production paths. Read `docs/superpowers/specs/opencollection-spec-reference.md` first when planning.

## 3. What Postman and Bruno need to import cleanly

Minimal valid document: `info` (with `name` and `schema`) and `item`. Details from the Postman v2.1 schema and Bruno's importer (`postman-to-bruno.js`):

- **Schema URL.** Bruno rejects any `info.schema` outside four URLs (v2.0.0 and v2.1.0 on `schema.getpostman.com` and `schema.postman.com`). Use the `schema.getpostman.com` v2.1.0 URL.
- **URL.** Keep `raw`, `query[]` and `variable[]` consistent. Bruno uses `raw` and also imports `query[]`, so a query can appear twice. Safest: put only enabled pairs in `raw`, list all params in `query[]` with `disabled:true` on disabled ones. Path params as `:id` plus `url.variable`.
- **Disabled flag.** `disabled:true` on headers, query, urlencoded and formdata entries. Bruno inverts it to `enabled`. Disabled whole items are not supported by either app: skip and report.
- **Auth.** Per-type key arrays of `{key, value, type}`. Use `{"type":"noauth"}` to force none, and omit `auth` to inherit. Key names: bearer `token`; basic and digest `username`, `password`; ntlm adds `domain`; apikey `key`, `value`, `in` (Bruno honours only `query`, else header); awsv4 `accessKey`, `secretKey`, `sessionToken`, `service`, `region`; oauth2 keys as in Bruno's `bruno-to-postman.js` (grant type renames: `authorization_code_with_pkce`, `password_credentials`).
- **Body.** Always set `options.raw.language` (json, xml, text) and a matching Content-Type. Formdata files use `type:"file"` and `src`. Graphql body is `{query, variables:"<json string>"}`. Binary body is `{mode:"file", file:{src}}`.
- **Scripts.** `event[]` with `listen` `prerequest` or `test` and `script.exec` as an array of lines. Bruno translates a limited set of `pm.*` calls to `bru.*`; unmapped calls pass through with only a console warning.
- **Folders.** Bruno imports folder auth and scripts and docs, but drops folder variables and folder headers. Copy folder (and collection) headers onto each request so behaviour survives.
- **Environments.** Postman shape: `{id, name, values:[{key,value,type,enabled}], _postman_variable_scope:"environment", _postman_exported_at, _postman_exported_using}`. `type` is `default` or `secret`. Bruno reads `key`, `value`, `enabled`, and `secret` when `type === "secret"`.

### Rocket concepts with no clean mapping, and the suggested action

| Rocket concept | Suggested export behaviour |
|---|---|
| `{{process.env.X}}` | No host-env slot in either app. Keep the text, add it to the report. Never emit the host value. |
| Secret and vault references | Env variable with `type:"secret"` and empty value. Never plaintext. Report each one. |
| Collection, folder, request secret variables (`CollectionVariable.secret`) | Blank the value, keep the key. |
| Literal auth credentials (basic password, bearer token, API key, AWS secret, OAuth client secret and so on) | Default: blank literals, keep `{{variable}}` references. Report counts. (Bruno exports them in clear text; this is the main safety difference.) |
| `rok.*` scripts | Not valid in either app. Translate the common subset to `pm.*` (variables, tests, expect); otherwise emit as commented lines with a marker and report. See `docs/features/rok-api-parity-notes.md` for the Rocket API surface (not read in this research). |
| Assertions, set-variable actions | Translate to `pm.test` or `pm.collectionVariables.set` lines in the `test` event, or skip and report if a mapping is unclear. Operator mapping is unverified. |
| Dynamic variables (`{{$guid}}` and so on) | Postman resolves its own set; Bruno passes them through as literal text. Keep names Postman knows, report the rest. |
| Flows | No equivalent. Skip, report. |
| gRPC, WebSocket requests | Not in Postman v2.1. Skip, report. |
| GraphQL requests | Emit as `body.mode:"graphql"`. Both apps read it. |
| Folder and collection headers | Copy onto each child request. |
| Client certificates, proxy, request settings | No slot. Skip, report. |
| Trust-gate fields (`sandbox_mode`, `script_context_roots`, `agent_autonomy_enabled`) | Rocket-only requests for access. Strip from every export. |
| Examples (saved responses) | Emit as `response[]` (Bruno does). Examples are opaque YAML in Rocket, so map best effort. |

## 4. Proposed design shape (a starting point for the interview, not a decision)

- New pure-logic crate `rocket-export` (or a module beside the importer): `to_postman(collection, options) -> (serde_json::Value, ExportReport)`, `postman_environment(env) -> Value`, `scrub_for_export(...)`. No I/O, so it is easy to test with fixtures.
- `rocket-app`: `CollectionExportService` reads through `CollectionRepository` and `EnvironmentRepository`, calls the converters, and returns bytes plus a suggested file name plus the report.
- `rocket-infra`: ZIP writing for the Bruno bundle (copy files, skip `.git` and `node_modules`, apply the scrub to YAML, strip trust-gate fields).
- `src-tauri`: thin commands `export_collection_bruno_zip` and `export_collection_postman` returning base64 bytes and a report (same pattern as `export_load_test`), with camelCase IPC DTOs.
- Frontend: "Export" item in the collection context menu opens a dialog (shadcn) with format choice, "Preserve scripts" toggle, an environments checkbox, and a report view. A `saveBinaryFile` helper next to `saveTextFile`.
- Output rules learned from Bruno's bugs: stable ordering by `seq` then name, de-duplicate sibling names, keep disabled params disabled, real collection variable values (secrets blanked), a skipped-items report instead of silent drops.

## 5. Suggested split (each plan at most 3 tasks, per factory rules)

1. **Bruno ZIP export + plumbing.** ZIP writer, scrub, command, binary save helper, context-menu dialog with the Bruno option, report type.
2. **Postman collection serializer.** Items, URL, headers, bodies, auth mappings, folder inheritance flattening, report. No scripts yet.
3. **Scripts, environments, polish.** `rok.*` to `pm.*` translation, assertions and actions to tests, Postman and Bruno environment export, "Preserve scripts" toggle, golden fixtures.

## 6. How to prove it "works as expected"

Automated, per Bruno's own test approach and the Postman docs:

1. Golden files: an export of a fixture collection that uses every body, auth and variable type, diffed in CI (Rust tests with `tempfile`).
2. Schema validation against `https://schema.postman.com/collection/json/v2.1.0/draft-07/collection.json` (`ajv-cli`, draft-07; exact flags unverified).
3. `newman run export.json -e env.json` against a local mock (wiremock or a tiny server) asserting the received method, URL, headers, body, and auth header.
4. Convert with `@usebruno/converters` (postman-to-bruno), read the returned `issues` list, then `bru run` against the same mock. Also import once through the Bruno GUI (Import Collection, then Postman Collection).
5. Open the Bruno ZIP in Bruno 3 (Open Collection) and run a request.
6. Round trip through Rocket's own importer using the fixtures in `crates/rocket-import/tests/fixtures/postman` for the supported field set.
7. Manual: import into the Postman app and send each request.

Bruno test files worth copying as fixtures: `packages/bruno-converters/tests/postman/*` (including `round-trip/`), `.../bruno/bruno-to-postman-translations/*.test.js`, and `tests/export/postman/` e2e.

## 7. Unverified items (check before relying on them)

- That Bruno 3 opens a Rocket-exported OpenCollection folder or ZIP without changes. The importer evidence says the formats match, but no real Bruno run was done. Files such as `_order.yml`, `uid` fields and Rocket-only keys need a check.
- Postman's behaviour when `url.raw` and the parsed parts disagree, and the exact oauth2 key list Postman requires (copy a real Postman export of an OAuth2 request).
- Whether Bruno's environment export blanks secret values (look at `tests/environments/fixtures/environment-exports/`).
- Operator mapping from Rocket assertions to `pm.expect`.
- The `rok.*` API surface that must be translated (not read here).
- Bruno's `sanitizeName` rules for file names.
- Bruno's exporter URL handling for protocol-less URLs with a query (the code suggests it can mangle them; not run).
- The `ajv-cli` validation command and `@usebruno/converters` script were not executed.

## 8. Open questions for the user (answers decide the spec)

1. **Scope of formats.** All three (Bruno ZIP, Postman collection, environments), or Postman only first? Recommended: Bruno ZIP first, then Postman, then environments and scripts.
2. **Secrets.** Blank literal credentials and secret values by default (recommended), or add an "include credentials" option with a warning dialog? Bruno has no option and exports in clear.
3. **Scripts.** Is a `rok.*` to `pm.*` translation for the common subset worth it in v1 (recommended: yes, in the third plan), or should scripts only be preserved untranslated with a toggle?
4. **Environments.** Export alongside the collection, always optional, or separate? Recommended: an optional checkbox in the same dialog.
5. **Legacy Bruno.** Skip the legacy `.bru` format entirely (recommended; Bruno 3 reads OpenCollection and a `.bru` writer does not exist)?
6. **Round trip.** Should the Rocket Postman importer also be extended to read `event` scripts, folder auth and variables, oauth2, file and graphql bodies, so a round trip is lossless? Recommended: a separate follow-up job, not part of export.
7. **Where in the UI.** Collection context menu "Export..." (recommended), plus the collection overview tab?

## Next steps

1. Answer the open questions above (or reply "your calls").
2. Continue the brainstorming interview to a written spec in `docs/superpowers/specs/`.
3. Run `/factory` with the feature text to write the plans and run the jobs.
