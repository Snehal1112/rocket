# Collection export: scripts, environments and docs Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** Postman exports carry translated scripts, assertions and actions, an optional `Preserve scripts` mode, and the collection's environments (secrets blanked) in one ZIP; the feature is documented and has a manual test checklist.

**Architecture:** `rocket-export` gains a tokenizer-based script translator, event builders and a Postman environment serializer plus ZIP bundler. The export service gains environment access through `EnvironmentRepositoryFactory` and an options struct. The command and dialog expose both options.

**Tech Stack:** Rust (serde_json, zip 2), Tauri 2, React, TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-23-collection-export-design.md`

## Global Constraints

- Script translation rewrites only an exact member path from the table below, and only in code: string, template-literal and comment text is never changed. Aliases (`const r = rok.response`, `const g = rok.getVar`) and computed access are not followed; a script containing them keeps its original text and gets a `Preserved` note.
- Unmapped `rok.` calls stay in the script with a `// rok: not translated` line above them and a `Skipped` note.
- Pre-request script becomes a `prerequest` event; post-response script, tests, assertion lines and after-response action lines become one `test` event with a `// Tests` line between script and tests. Before-request actions are skipped and reported.
- Assertion operators: `eq` `.to.eql`, `neq` `.to.not.eql`, `gt` `.to.be.above`, `gte` `.to.be.at.least`, `lt` `.to.be.below`, `lte` `.to.be.at.most`, `contains` `.to.include`. Any other operator, a disabled assertion aside (silently dropped), gives a `Skipped` note.
- `Preserve scripts` keeps script text as written; assertions and actions are still translated.
- Postman environment: `{id, name, values:[{key,value,type,enabled}], _postman_variable_scope:"environment", _postman_exported_at, _postman_exported_using:"Rocket/<version>"}`; secret variables get `type:"secret"` and `value:""`; `extends` is flattened first; workspace-level environments are not exported.
- With environments included and at least one present, the Postman result is one ZIP `<slug>-postman.zip` holding `<slug>.postman_collection.json` and `environments/<env-slug>.postman_environment.json`; otherwise it is the plain JSON file.
- Environments are read with `list_strict`; a read or keychain error fails the export. Secret values are dropped immediately after the read.
- Same layering, serde and cargo rules as plans 1 and 2.

Translation table (left side checked against `src/components/editor/rok-types.ts` in Task 1):

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

## Review Focus

1. The text `"rok.setVar('a')"` inside a string literal or comment is never rewritten: pinned in Task 1 by `ignores_rok_text_inside_strings_and_comments`.
2. A script that aliases `const r = rok.getVar` keeps its text and is reported rather than half-translated: pinned in Task 1 by `aliased_script_is_kept_and_reported`.
3. An assertion with operator `matches` is skipped and reported, and a disabled assertion produces no output and no note: pinned in Task 1 by `unknown_operator_is_reported_and_disabled_is_silent`.
4. An environment with `extends` exports the parent's variables and the child's overrides, and a secret variable never carries a value: pinned in Task 2 by `environment_extends_is_flattened` and `secret_variable_value_is_empty_even_when_hydrated`.
5. A collection with no environments exports a plain JSON file, not an empty ZIP: pinned in Task 2 by `no_environments_gives_plain_json`.

## File Structure

- Create `crates/rocket-export/src/{scripts.rs,events.rs,postman_env.rs}` (`slug` already exists from plan 2)
- Modify `crates/rocket-export/src/{postman.rs,lib.rs}`, `crates/rocket-app/src/collection_export_service.rs`
- Modify `src-tauri/src/commands/export.rs`, `src/lib/tauri-api.ts`, `src/components/export/ExportCollectionDialog.tsx` and its test
- Create `crates/rocket-export/CLAUDE.md`, `.claude/collection-export.md`, `.claude/manual-tests/collection-export.md`
- Modify root `CLAUDE.md` (one pointer line), `.claude/roadmap.md` (one row)

---

### Task 1: Script translation and events

**Files:**
- Create: `crates/rocket-export/src/scripts.rs`, `crates/rocket-export/src/events.rs`
- Modify: `crates/rocket-export/src/{postman.rs,lib.rs}`

**Interfaces:**
- Consumes: `PostmanOptions` (plan 2), `Request`, `FolderSettings`, `Assertion`, `ActionSetVariable`.
- Produces:
  - `struct ScriptTranslation { text: String, untranslated: Vec<String>, aliased: bool }`
  - `fn translate_script(src: &str) -> ScriptTranslation`
  - `fn assertion_line(a: &Assertion) -> AssertionOutcome` where `enum AssertionOutcome { Line(String), Disabled, Unsupported(String) }`
  - `fn action_line(a: &ActionSetVariable) -> Option<String>` (after-response set-variable only)
  - `fn request_events(req: &Request, opts: &PostmanOptions, path: &str, notes: &mut Vec<ExportNote>) -> Vec<serde_json::Value>`
  - `fn folder_events(settings: &FolderSettings, opts: &PostmanOptions, path: &str, notes: &mut Vec<ExportNote>) -> Vec<serde_json::Value>`

- [ ] **Step 1: Read `src/components/editor/rok-types.ts` (variable, `res`, `req` and bare `test`/`expect` declarations), `.claude/rok-api-parity-notes.md`, `crates/rocket-shared/src/assertion.rs`, `crates/rocket-shared/src/action.rs` and `crates/rocket-app/src/assertion_evaluator.rs:134-236` (operators and expression forms).** Drop any table row whose Rocket name does not exist in `rok-types.ts`.

- [ ] **Step 2: Write the failing tests in `scripts.rs` and `events.rs`.** Example:

```rust
#[test]
fn translates_variable_and_response_calls() {
    let src = "const id = rok.getVar('id');\nrok.setEnvVar('t', res.getBody().token);\ntest('ok', () => { expect(res.getStatus()).to.equal(200); });\n";
    let out = translate_script(src);
    assert_eq!(
        out.text,
        "const id = pm.variables.get('id');\npm.environment.set('t', pm.response.json().token);\npm.test('ok', () => { pm.expect(pm.response.code).to.equal(200); });\n"
    );
    assert!(out.untranslated.is_empty());
}
```

| Test name | Input | Expected |
|---|---|---|
| `ignores_rok_text_inside_strings_and_comments` | `"// rok.setVar('a')\nconst s = \"rok.getVar('b')\";\nconst t = `rok.getEnvVar`;"` | `text` equals the input |
| `unmapped_call_gets_marker_and_is_listed` | `rok.sendRequest({})` | text has `// rok: not translated` on the line above; `untranslated == ["rok.sendRequest"]` |
| `aliased_script_is_kept_and_reported` | `const g = rok.getVar;\ng('a');` | `aliased == true`, `text` unchanged |
| `res_status_property_and_header_calls` | `res.status`, `res.getHeader('x')`, `req.getUrl()`, `req.getMethod()` | `pm.response.code`, `pm.response.headers.get('x')`, `pm.request.url.toString()`, `pm.request.method` |
| `assertion_lines_for_each_operator` | `res.status eq 200`, `neq`, `gt`, `gte`, `lt`, `lte`, `res.body.name contains "a"` | `pm.test(...)` lines using the operator suffixes from Global Constraints; numeric values unquoted, text quoted |
| `assertion_expression_paths` | `res.body.a.0.b` | `pm.response.json().a[0].b` |
| `unknown_operator_is_reported_and_disabled_is_silent` | operator `matches`; `disabled: Some(true)` | `Unsupported("matches")`; `Disabled` |
| `after_response_action_becomes_set_line` | scope `collection`, `environment`, `runtime`, expression `res.body.id` | `pm.collectionVariables.set("n", pm.response.json().id);` and the environment and variables forms |
| `before_request_action_is_skipped` | phase `before-request` | `action_line` is `None` and `request_events` adds a `Skipped` note |
| `request_events_split_prerequest_and_test` | pre script, post script, tests, one assertion | two events: `prerequest` and `test`; `test.script.exec` has the post script, a `// Tests` line, the tests, then the assertion line |
| `preserve_scripts_keeps_text` | `preserve_scripts: true`, script `rok.getVar('a')` | exec unchanged; assertions still translated |
| `folder_events_use_folder_scripts` | `FolderSettings` with pre and tests scripts | `prerequest` and `test` events |

- [ ] **Step 3: Run `cargo test -j4 -p rocket-export scripts events`.** Expected: compile errors `cannot find function 'translate_script' in this scope`, zero passing.

- [ ] **Step 4: Implement the signatures.** `translate_script` is a single-pass scanner with states code, single-quote, double-quote, template literal (opaque), line comment, block comment; in code state it reads identifier chains (`[A-Za-z_$][A-Za-z0-9_$]*(\.[A-Za-z_$][A-Za-z0-9_$]*)*`) and looks the chain up in the table; calls with arguments keep their argument text. Alias detection: an assignment whose right side is exactly a table path or `rok.<name>` with no call parentheses sets `aliased`. `exec` is the text split on `\n`. Wire `request_events` and `folder_events` into `postman_collection` (the `event` key is omitted when empty).

- [ ] **Step 5: Run `cargo test -j4 -p rocket-export` and `cargo check -j4 -p rocket-export`.** Expected: all tests pass including the plan 2 golden file (update the golden fixture only for the new `event` keys, using the paste-the-actual-output step), no warnings.

### Task 2: Environments and packaging

**Files:**
- Create: `crates/rocket-export/src/postman_env.rs`
- Modify: `crates/rocket-export/src/lib.rs`, `crates/rocket-app/src/collection_export_service.rs`

**Interfaces:**
- Consumes: `Environment`, `Variable`, `flatten_with_repo`, `EnvironmentRepository`, `EnvironmentRepositoryFactory::for_collection`, `slug` (plan 2).
- Produces:
  - `fn postman_environment(name: &str, vars: &[Variable], exported_at: &str, version: &str) -> serde_json::Value`
  - `fn postman_bundle(collection_file: (&str, &[u8]), envs: Vec<(String, Vec<u8>)>) -> DomainResult<Vec<u8>>`
  - `struct PostmanExportOptions { preserve_scripts: bool, include_environments: bool }` (`Default` both false)
  - `CollectionExportService::with_environments(self, factory: Box<dyn EnvironmentRepositoryFactory>) -> Self`
  - `fn export_postman(&self, collection: &str, opts: &PostmanExportOptions) -> DomainResult<ExportOutput>` (replaces the plan 2 signature; update its callers and tests)

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (environments and variable scopes).** Then read `crates/rocket-environment/src/{repository.rs,environment.rs,variable.rs,flatten.rs}` and the in-memory repository doubles used in `crates/rocket-app` tests.

- [ ] **Step 2: Write the failing tests.** In `postman_env.rs`:

```rust
#[test]
fn secret_variable_value_is_empty_even_when_hydrated() {
    let mut secret = Variable::new("API_KEY", "hydrated-value");
    secret.secret = true;
    let doc = postman_environment("prod", &[secret, Variable::new("HOST", "x.test")], "2026-10-23T00:00:00Z", "2.1.1");
    assert_eq!(doc["values"][0]["type"], "secret");
    assert_eq!(doc["values"][0]["value"], "");
    assert_eq!(doc["values"][1]["value"], "x.test");
    assert_eq!(doc["_postman_variable_scope"], "environment");
    assert_eq!(doc["_postman_exported_using"], "Rocket/2.1.1");
}
```

| Test name | Input | Expected |
|---|---|---|
| `disabled_variable_has_enabled_false` | `Variable{enabled:false}` | `values[0].enabled == false` |
| `bundle_contains_collection_and_environment_files` | one env | zip names `my-api.postman_collection.json`, `environments/prod.postman_environment.json` |
| `no_environments_gives_plain_json` (service) | include on, repo lists none | `file_name` ends `.postman_collection.json`, bytes parse as JSON |
| `environment_extends_is_flattened` (service) | `child` extends `base`, child overrides one key | exported `child` has both keys with the child's value |
| `include_environments_off_gives_plain_json` | option off, two environments exist | plain JSON, no repository call (`list_strict` hook not hit) |
| `environment_read_error_fails_export` | `list_strict` returns `Err` | `export_postman` returns that error |
| `environments_report_blanked_secrets` | two secret variables | report has two `Blanked` notes naming `environments/<name>` |

- [ ] **Step 3: Run `cargo test -j4 -p rocket-export postman_env` and `cargo test -j4 -p rocket-app collection_export`.** Expected: compile errors for the missing functions and options.

- [ ] **Step 4: Implement.** `export_postman` builds the tree, runs `postman_collection` with `PostmanOptions{preserve_scripts}`, and when `include_environments` is set lists environments with `factory.for_collection(collection).list_strict()`, flattens each with `flatten_with_repo`, blanks secret values, and bundles. File names use `slug`. Update the plan 2 service tests to the new signature.

- [ ] **Step 5: Run `cargo test -j4 -p rocket-export`, `cargo test -j4 -p rocket-app collection_export`, `cargo check -j4 -p rocket-app`.** Expected: all pass, no warnings.

### Task 3: Command, dialog options and documentation

**Files:**
- Modify: `src-tauri/src/commands/export.rs`, `src-tauri/src/lib.rs`, `src/lib/tauri-api.ts`, `src/components/export/ExportCollectionDialog.tsx`, `src/components/export/ExportCollectionDialog.test.tsx`, root `CLAUDE.md`, `.claude/roadmap.md`
- Create: `crates/rocket-export/CLAUDE.md`, `.claude/collection-export.md`, `.claude/manual-tests/collection-export.md`

**Interfaces:**
- Consumes: `PostmanExportOptions`, `with_environments`.
- Produces:
  - Rust: `struct PostmanExportOptionsDto { preserve_scripts: bool, include_environments: bool }` (camelCase); `export_collection_postman(collection: String, options: PostmanExportOptionsDto, workspace_path: State<'_, Arc<Mutex<PathBuf>>>) -> Result<ExportResultDto, DomainError>` (replaces the plan 2 signature)
  - TS: `export interface PostmanExportOptions { preserveScripts: boolean; includeEnvironments: boolean }` and `exportCollectionPostman = (collection: string, options: PostmanExportOptions) => invoke<ExportResult>(...)`

- [ ] **Step 1: Read the current dialog and its test, `src-tauri/src/commands/export.rs`, `src-tauri/src/lib.rs` where `SharedCollectionEnvironmentRepo::with_secret_store` is built, `.claude/roadmap.md` (find the next free `R-` number) and one existing `.claude/manual-tests/*.md` file for style.**

- [ ] **Step 2: Write the failing dialog tests.**

```ts
it('passes Preserve scripts and Include environments to the Postman export', async () => {
  mockExportPostman.mockResolvedValue({ dataBase64: 'e30=', fileName: 'demo-postman.zip', report: { exported: 1, items: [] } });
  render(<ExportCollectionDialog open collectionName="demo" onClose={vi.fn()} />);
  await user.click(screen.getByRole('combobox', { name: /format/i }));
  await user.click(screen.getByRole('option', { name: /postman/i }));
  await user.click(screen.getByRole('checkbox', { name: /preserve scripts/i }));
  await user.click(screen.getByRole('button', { name: /export/i }));
  expect(mockExportPostman).toHaveBeenCalledWith('demo', { preserveScripts: true, includeEnvironments: true });
});
```

Further cases: `Preserve scripts is hidden for the Bruno ZIP format`; `the not-included-yet note from plan 2 is gone`; `saves a .zip file with an application zip filter when the result file name ends with .zip`.

- [ ] **Step 3: Run `yarn test ExportCollectionDialog`.** Expected: failures (`exportCollectionPostman` called with one argument, checkbox not found).

- [ ] **Step 4: Implement.** Update the command with the DTO and `with_environments(Box::new(SharedCollectionEnvironmentRepo::with_secret_store(Arc::clone(&*workspace_path), crate::env_secret_store())))`; update the wrapper and dialog; the dialog picks the save filter from the returned file name. Write the docs: `crates/rocket-export/CLAUDE.md` (module map, layering, the translation table, scrub rules, how to add a mapping), `.claude/collection-export.md` (decisions from the spec, known losses, unverified items), one pointer line in root `CLAUDE.md`, one roadmap row for the follow-ups (lossless importer round trip, single-file YAML, Insomnia), and `.claude/manual-tests/collection-export.md` with the checks: ajv validation against the v2.1 draft-07 schema, `newman run` against a local mock, `@usebruno/converters` plus `bru run`, opening the Bruno ZIP in Bruno 3, importing into Postman, and a secret leak grep over the outputs.

- [ ] **Step 5: Run `yarn test ExportCollectionDialog`, `yarn tsc --noEmit`, `yarn check`, `cargo check -j4 -p <src-tauri package>`, and `cargo test -j4 -p rocket-export`.** Expected: all pass, no type, lint or Rust errors.

Next plan: none
