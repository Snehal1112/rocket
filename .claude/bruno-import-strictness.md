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
- **Other required fields Bruno could omit when empty** (audit of `oc/`, risk HIGH): WebSocket message `data` (`websocket.rs`), GraphQL `query`, `url` on blank GraphQL, gRPC and WebSocket requests, gRPC message variant `message`, and `value` on headers, params, form fields and gRPC metadata. MEDIUM: auth credential strings (`username`, `password`, `token`, `key`), OAuth2 `clientId`, script `code`, example response `type`, variant `title` fields.
- **No check at import time.** The importer cannot tell the user that a copied file will not open. A parse check after the copy, reported in `ImportReport.skipped`, would show it.
- **The `protoFiles` shape** in the spec reference (`{type, path}`) differs from Rocket's `filePath`. Not checked.

## How to check a collection

Run Rocket's readers over every file of the collection with a throwaway ignored test (`get_summaries`, then `get_request` and its GraphQL, WebSocket and gRPC variants for each summary, `get_folder_settings`, `get_settings`, `get_strict`, and `FsEnvironmentRepo::list_strict`). On 2026-10-23 this found the two body files and the `docs` problem in "Sage Network Desktop2", and nothing in "api-4". After the fixes both collections load with `get_strict` and `get_settings`.
