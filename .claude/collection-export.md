# Collection export

Export a collection from the collection context menu (`Export...`) as a Bruno ZIP or a Postman Collection v2.1 file. Spec: `docs/superpowers/specs/2026-10-23-collection-export-design.md`. Crate guide: `crates/rocket-export/CLAUDE.md`.

## What exists

| Output | Content |
|---|---|
| Bruno ZIP | Scrubbed copy of the collection folder under one root folder. `Include environments` adds `environments/*.yml` |
| Postman collection | One `<slug>.postman_collection.json` |
| Postman collection with environments | One `<slug>-postman.zip` holding the collection file and `environments/<env-slug>.postman_environment.json`. With no environments the result is the plain JSON file |

Dialog options: format, `Preserve scripts` (Postman only), `Include environments` (default on). After the save the dialog lists the report grouped by action.

Flow: dialog, then `export_collection_bruno` or `export_collection_postman` (`src-tauri/src/commands/export.rs`), then `CollectionExportService` (`crates/rocket-app/src/collection_export_service.rs`), then the pure converters in `rocket-export`.

## Decisions

- Literal credentials are always blanked. No option exports them. A secret variable whose value is a lone `{{variable}}` reference keeps the reference in Bruno files and in Postman collection, folder and request variables (decision 4 of the name-scrub spec). Postman environments are the exception: secret values come from the keychain and the service clears them right after the read, so a secret environment value is always exported empty.
- One name rule (`is_sensitive_name`) matches the last word of a name: token, secret, password, passwd, credential(s), authorization, cookie, signature, apikey, `api key`, `private key`, `secret key`, `client assertion`, and the single words `pass` and `pwd`. It is never a substring match. Allow-list: `token_type`, `page_token`, `next_token`, `prev_token`, `previous_token`, `max_tokens`.
- The rule blanks the literal values of headers, gRPC and WebSocket metadata, query parameters (rows and URL pairs), path parameters, form-urlencoded and multipart text fields, OAuth2 additional parameters, OAuth URL fields and variables (including non-secret variables and environments). The note says `<name> (by name)`.
- Identifiers stay: basic, digest and NTLM usernames, awsv4 `accessKeyId`, OAuth2 `clientId`.
- URL userinfo without a `:` (`https://TOKEN@host`) is not blanked, because that would also blank plain usernames.
- The common `rok.*` subset is translated to `pm.*`. `Preserve scripts` keeps the text.
- Environments are optional and on by default. Workspace-level environments are not exported.
- No legacy `.bru` output and no single-file OpenCollection YAML.
- Saved request examples are not exported in either format, because captured responses may hold tokens. The Bruno ZIP removes the `examples` key and reports it.
- The Bruno ZIP leaves out Rocket-only files (`flows/` and hidden directories such as `.rocket/`) and reports them.
- The Bruno ZIP converts a legacy flat `folder.yml` (top-level `name`, `type`, `uid`) to the `info` shape, because Bruno reads the folder name only from `info.name` (the name falls back to the directory name). It removes `uid` keys (top level and `info.uid`) because they are not part of the OpenCollection schema. Both give report notes (`Legacy folder.yml` per folder, one `uid` summary).
- The Bruno ZIP removes the `extensions.rocketapi` subtree and the trust-gate keys and keeps other `extensions.*` keys.
- A Postman export with environments is one ZIP, so the dialog needs one save.
- Environments are read with `list_strict`. A keychain or read error fails the export.

## Known losses

- Flows, gRPC and WebSocket requests, client certificates, proxy, per-request settings and the trust-gate keys are not exported. Each is reported.
- OAuth1 and WSSE auth become `noauth` in Postman and are reported.
- Collection-level scripts do not exist in the Rocket model.
- Scripts that use an alias, computed access or a `rok.` call without a mapping are not fully translated. They keep a marker or their original text and a report entry.
- Assertion operators other than `eq`, `neq`, `gt`, `gte`, `lt`, `lte` and `contains` are skipped. An assertion value is sent as a number when it is a JSON number and as text otherwise.
- Before-request set-variable actions have no Postman form.
- The script scanner does not understand regex literals.
- Bodies, scripts and descriptions are not scanned for secrets. The dialog says so.
- A credential with a harmless name (for example `session_id`) is exported as written. The name rule cannot see it.

## Unverified

These need the manual checks in `.claude/manual-tests/collection-export.md`:

- The v2.1 draft-07 schema validation of the output.
- The OAuth2 key names, which follow Bruno's exporter and not a real Postman export.
- That Postman and Bruno 3 open the files.
- The `// Tests` marker layout in Postman's test tab.

## Follow-ups

Roadmap row R-65 in `.claude/roadmap.md`: lossless Postman round trip in the importer, single-file OpenCollection YAML, Insomnia and Postman Data Export targets.
