# rocket-export

Pure logic that turns a Rocket collection into a Bruno ZIP or a Postman Collection v2.1 document, plus a report of what changed on the way. No filesystem access: file reads sit behind the `CollectionFileSource` port, and `rocket-infra` implements it. Background and decisions: `.claude/collection-export.md`.

## Module map

| Module | Role |
|---|---|
| `bruno.rs` | `bruno_zip(files, name, include_environments)`: scrubs each YAML file as a `serde_yaml::Value` and zips it under one root folder |
| `sensitive.rs` | Name rule `is_sensitive_name` (last word of a name, with an allow-list) |
| `scrub.rs` | Secret rules for every format: `scrub_auth`, `scrub_header_values`, `scrub_typed_rows`, `scrub_form_entries`, `scrub_path_params`, `scrub_url`, `scrub_yaml`, `strip_rocket_namespace`, `is_variable_reference` |
| `ports.rs` | `CollectionFileSource` and `SourceFile` |
| `tree.rs` | `CollectionExportTree`, `ExportFolder`, `ExportItem` (typed input of the Postman serializer) |
| `postman.rs` | `postman_collection(tree, opts)`: items, URLs, headers, bodies, auth, variables, events |
| `postman_auth.rs` | Auth type to Postman key table |
| `scripts.rs` | `translate_script`, `assertion_line`, `action_line`: Rocket JavaScript, assertions and actions to `pm.*` |
| `events.rs` | `request_events`, `folder_events`: build the `prerequest` and `test` event blocks |
| `postman_env.rs` | `postman_environment`, `environment_notes`, `postman_bundle` (collection plus environments ZIP) |
| `names.rs` | `slug` for file names |
| `report.rs` | `ExportReport`, `ExportNote`, `ExportAction` (`Translated`, `Blanked`, `Skipped`, `Preserved`) |

## Layering

- Depends on `rocket-shared`, `rocket-collection`, `rocket-environment`, `serde_json`, `serde_yaml`, `uuid` and `zip` (in memory only).
- `rocket-app` (`collection_export_service.rs`) builds the tree from repositories and calls this crate. The Tauri commands are in `src-tauri/src/commands/export.rs`.
- The report types serialize as camelCase because they cross IPC unchanged. Nothing here is persisted.

## Script translation

`translate_script` is one pass over the text. It skips string literals, template literals and comments, so text inside them never changes. It rewrites a chain only when the whole member path is in the table:

| Rocket | Postman |
|---|---|
| `rok.getVar` / `rok.setVar` | `pm.variables.get` / `pm.variables.set` |
| `rok.getEnvVar` / `rok.setEnvVar` | `pm.environment.get` / `pm.environment.set` |
| `rok.getCollectionVar` / `rok.setCollectionVar` | `pm.collectionVariables.get` / `pm.collectionVariables.set` |
| `rok.getGlobalEnvVar` / `rok.setGlobalEnvVar` | `pm.globals.get` / `pm.globals.set` |
| `test(` / `expect(` | `pm.test(` / `pm.expect(` |
| `res.getStatus()` and `res.status` | `pm.response.code` |
| `res.getBody()` | `pm.response.json()` |
| `res.getHeader(x)` | `pm.response.headers.get(x)` |
| `req.getUrl()` | `pm.request.url.toString()` |
| `req.getMethod()` | `pm.request.method` |

- An unmapped `rok.`, `res.` or `req.` member (for example `rok.sendRequest`, `res.getResponseTime()`, `req.setHeader(...)`, `res.getBody({ raw: true })`) stays, gets a `// rok: not translated` line above it and a `Skipped` note. A script that declares its own `res` or `req` (`const res = ...`) is not treated as the response or request.
- The `${}` expressions of a template literal are code and are translated. Text inside the template and nested template text are never changed. An unmapped call inside `${}` gets a `Skipped` note but no marker line, because a marker would change the string.
- A script that copies a mapped name (`const g = rok.getVar;`, `const { getVar } = rok;`) is kept whole and gets a `Preserved` note. Aliases and computed access are never followed.
- Regex literals are not understood by the scanner. A `/` is treated as a plain character.
- Assertions: `eq` `.to.eql`, `neq` `.to.not.eql`, `gt` `.to.be.above`, `gte` `.to.be.at.least`, `lt` `.to.be.below`, `lte` `.to.be.at.most`, `contains` `.to.include`. Another operator is `Skipped`. A disabled assertion is dropped without a note. The expression `res.body` (the raw text) becomes `pm.response.text()`, while `res.body.a.0.b` becomes `pm.response.json().a[0].b`.
- After-response set-variable actions become `pm.collectionVariables.set`, `pm.environment.set` or `pm.variables.set` lines. Before-request actions have no Postman form and are `Skipped`.
- `PostmanOptions.preserve_scripts` keeps script text as written. Assertions and actions are still translated.

### Add a mapping

1. Check the Rocket name exists in `src/components/editor/rok-types.ts`.
2. Add a row to `TABLE` in `scripts.rs` with `Call`, `EmptyCall` or `Prop`.
3. Add a case to `translates_variable_and_response_calls` or a new test next to it.
4. Update the table above and in `.claude/collection-export.md`.

## Secret scrub

- Literal credentials in auth, `secret` variable values and a password in URL userinfo are blanked. A value that is exactly one `{{variable}}` reference is kept, also in a secret variable, in Bruno files and in Postman collection, folder and request variables (decision 4). Postman environments differ, see below. URL userinfo without a `:` is not blanked.
- `sensitive.rs` holds the name rule `is_sensitive_name`. It matches the last word of a name (token, secret, password, passwd, credential(s), authorization, cookie, signature, apikey, `api key`, `private key`, `secret key`, `client assertion`, single `pass` and `pwd`), never a substring. Allow-list: `token_type`, `page_token`, `next_token`, `prev_token`, `previous_token`, `max_tokens`. It blanks literal values of headers, query pairs, path parameters, form fields, OAuth URL fields and variables, in both formats. gRPC and WebSocket metadata and OAuth2 `additionalParameters` exist only in the Bruno ZIP, because Postman does not export them. The five old headers still match.
- Identifiers stay: basic, digest and NTLM usernames, awsv4 `accessKeyId`, OAuth2 `clientId`. OAuth1 `accessToken` is blanked.
- Postman `auth_value` adds scrub notes only for auths it exports. OAuth1 and WSSE become `noauth` and give only a `Skipped` note.
- The Bruno ZIP scrub walks the YAML by key. It also blanks gRPC `metadata` rows with a sensitive name, the `passphrase` of every `clientCertificates` entry (collection and environment files), OAuth1 `accessToken`, and the `data` of a typed secret variable (`value: {type, data}`). A number or boolean in a credential slot counts as a literal and is blanked.
- The Bruno ZIP drops saved request examples (captured responses may hold tokens) with one `Skipped` note per file, and leaves out `flows/` and hidden directories such as `.rocket/` (one `Skipped` note per directory), because Bruno cannot read them.
- The Bruno ZIP converts a legacy flat `folder.yml` (top-level `name`, `type`, `uid`) to the `info` shape, because Bruno reads the folder name only from `info.name` (the name falls back to the directory name). It removes `uid` keys (top level and `info.uid`) because they are not part of the OpenCollection schema. Both give report notes (`Legacy folder.yml` per folder, one `uid` summary).
- Bodies, scripts and descriptions are not scanned.
- Environments are written with `type: "secret"`. Secret values come from the keychain and the service clears them right after the read, so a secret environment value is always exported empty. The keep-a-ref rule in `postman_env.rs` only matters when the function is called with a ref directly. A non-secret variable with a sensitive name is blanked (note `Variable`, `<key> (by name)`).

## Tests

- `cargo test -j4 -p rocket-export`. The Postman golden file is `tests/fixtures/postman-golden.json`. Update it only for an intended output change, by pasting the actual output.
