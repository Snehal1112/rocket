# Bruno 3 imports and Rocket's strict readers

Written 2026-10-23, after three bugs on one imported collection ("Sage Network Desktop2").

## Why this happens

`import_modern_collection` (`crates/rocket-import/src/importer.rs`) copies the request, `folder.yml` and environment files of a Bruno 3 collection byte for byte. Nothing is parsed, normalized or checked, and every copied file counts as imported. Rocket's own readers (`crates/rocket-infra/src/oc/`) were written against Rocket's own output and require some fields that Bruno omits when a thing is empty. The legacy `.bru` import goes through `FsCollectionRepo::save_request`, which always writes the fields, so it does not have this problem.

Because `OcItem` and `OcAuth` are untagged enums, one missing field inside a request gives the opaque "did not match any variant" error, and the real field name is hidden.

## Fixed

| Symptom | Cause | Fix |
|---|---|---|
| "skipping corrupt environment YAML file" for every environment | Bruno names an environment by its file name, so the file has no `name:` | `OcEnvironment.name` defaults to empty and the repo uses the file stem (`7ed55d2c`) |
| Every uid-less request looked selected in the sidebar | Bruno request files have no `uid:`, the summary uid was empty and the tab id was the uid | a uid derived from the relative path, in the summaries and in `get_request` (`a447f459`) |
| "Could not open ...: http: missing field `data`" | Bruno writes `body: {type: form-urlencoded}` with no `data` for an empty body | `data` defaults to empty on all body variants and the example response body |
| Collection settings fail to load ("docs: invalid type: map, expected a string") | Bruno writes `docs: {content, type}` in `opencollection.yml`, and `OcCollection.docs` was a string | `docs` of the collection and of HTTP, GraphQL, gRPC and WebSocket requests is read from a string, an object or null through `oc::docs_text`; only the text is kept, and Rocket writes a plain string (folders already did this) |

## Known and not fixed

- **Saving collection settings rewrites an object `docs` as a plain string** (the `type` is dropped; the text is kept). Folder docs keep the object form when the text did not change (`docs_to_oc`); the collection and requests do not yet.
- **Fixed on 2026-10-24:** `value` on headers, params, form fields, response headers and gRPC metadata, WebSocket message `data`, GraphQL `query`, gRPC message variant `message`, and `url` on GraphQL, gRPC and WebSocket requests now default to empty (`#[serde(default)]` in `oc/`). A header without a `name` still fails.
- **Still strict (MEDIUM):** auth credential strings (`username`, `password`, `token`, `key`), OAuth2 `clientId`, script `code`, example response `type`, variant `title` fields.
- **Import-time check (added 2026-10-23).** After the copy, `import_modern_collection` reads every copied file back with Rocket's readers (`report_files_that_cannot_open`: collection settings, environments, `folder.yml`, and each request by kind). A failure is added to `ImportReport.skipped` as `ParseError("copied, but Rocket cannot open it: ...")`. The file stays on disk and still counts in `imported`. The UI shows these under the skipped list.
- **The `protoFiles` shape** in the spec reference (`{type, path}`) differs from Rocket's `filePath`. Not checked.

## Round-trip checks (2026-10-24)

`crates/rocket-import/tests/roundtrip_test.rs` exports a collection as a Bruno ZIP, imports it again and checks that no import-time "cannot open" entry appears, that the file count matches (Rocket-only `flows/` and `.rocket/` are expected to be left out) and that every HTTP request keeps its name, method, URL, header and query names, non-form body, scripts, assertion count and order. `crates/rocket-app/tests/postman_round_trip.rs` does the Postman export and import and compares request counts. Both have fixture tests and an ignored test for a real workspace:

```bash
ROCKET_ROUNDTRIP_WORKSPACE=~/.rocket-api/<workspace> cargo test -j4 -p rocket-import --test roundtrip_test -- --ignored --nocapture
ROCKET_ROUNDTRIP_WORKSPACE=~/.rocket-api/<workspace> cargo test -j4 -p rocket-app --test postman_round_trip -- --ignored --nocapture
```

They only read the workspace. On 2026-10-24 RocketVault, CollectionTest and Lockstep-Inbox all passed (Bruno: 128, 56, 58, 67 and 585 request files; Postman: 109, 43, 45, 56 and 453 requests). Not covered: content of gRPC, WebSocket and GraphQL requests, form bodies and credential values (the export blanks them on purpose), environments through Postman, and Postman requests that Rocket cannot represent.

## How to check a collection

Run Rocket's readers over every file of the collection with a throwaway ignored test (`get_summaries`, then `get_request` and its GraphQL, WebSocket and gRPC variants for each summary, `get_folder_settings`, `get_settings`, `get_strict`, and `FsEnvironmentRepo::list_strict`). On 2026-10-23 this found the two body files and the `docs` problem in "Sage Network Desktop2", and nothing in "api-4". After the fixes both collections load with `get_strict` and `get_settings`.
