# Collection export: Bruno ZIP and plumbing Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** A user can right-click a collection, choose `Export...`, and save a Bruno-readable ZIP of the collection with credentials blanked and an export report.

**Architecture:** A new pure crate `rocket-export` holds the report types, the file-source port, the YAML scrub and the ZIP builder. `rocket-infra` implements the port. `CollectionExportService` in `rocket-app` ties them together. A thin Tauri command returns base64 bytes, and a shadcn dialog saves them.

**Tech Stack:** Rust (serde_yaml, zip 2), Tauri 2 commands, React, TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-23-collection-export-design.md`

## Global Constraints

- Domain logic only in `rocket-export`; I/O only in `rocket-infra`; `rocket-app` holds trait objects only; commands stay thin.
- camelCase serde on IPC DTOs only. No panicking unwrap calls in production paths; tests use `.expect("...")`.
- Cargo commands per crate with `-j4`: `cargo check -j4 -p <crate>`, `cargo test -j4 -p <crate> <filter>`. Never a workspace-wide run.
- Bruno ZIP: one root folder named after the collection; excludes `.git`, `node_modules`, `workspace.yml` and any file that is not `.yml`, `.yaml`, `.proto` or `.js`; removes the whole `extensions.rocketapi` subtree.
- Scrub: literal credentials, `secret: true` variable values (`value` and `initial`), sensitive header values (`authorization`, `proxy-authorization`, `cookie`, `set-cookie`, `x-api-key`, case-insensitive) and URL passwords become empty strings; a value that is only a `{{variable}}` reference (optionally after `Bearer `, `Basic ` or `Token `) is kept.
- shadcn primitives and lucide icons only; narrow Zustand selectors; `yarn tsc --noEmit` and `yarn check` clean.

## Review Focus

1. A request file with `Authorization: Bearer {{token}}` keeps its value, while `Authorization: Bearer abc123` is blanked: pinned in Task 1 by `keeps_bearer_prefix_with_reference` and `blanks_literal_authorization_header`.
2. A URL such as `https://user:pass@host/x` loses only the password (`https://user:@host/x`), and `{{pass}}` in that position is kept: pinned in Task 1 by `blanks_url_password`.
3. A collection name like `../x` or `a/b` is refused by the file source and never reads outside the workspace: pinned in Task 2 by `file_source_rejects_unsafe_collection_name`.
4. Unrelated `extensions` keys (for example `extensions.bruno`) survive the strip: pinned in Task 1 by `strip_removes_rocketapi_and_keeps_other_extensions`.
5. Cancelling the save dialog writes nothing and shows no error: pinned in Task 3 by `saveBinaryFile returns false when the dialog is cancelled`.

## File Structure

- Create `crates/rocket-export/{Cargo.toml,src/lib.rs,src/report.rs,src/ports.rs,src/scrub.rs,src/bruno.rs}`
- Modify root `Cargo.toml` (members and `[workspace.dependencies]` path entry), `crates/rocket-app/Cargo.toml`, `crates/rocket-app/src/lib.rs`, `crates/rocket-infra/src/lib.rs`
- Create `crates/rocket-app/src/collection_export_service.rs`, `crates/rocket-infra/src/fs_collection_file_source.rs`
- Create `src-tauri/src/commands/export.rs`; modify `src-tauri/src/commands/mod.rs` and `src-tauri/src/lib.rs` (handler list)
- Modify `src/lib/tauri-api.ts`, `src/lib/save-file.ts`, `src/components/collections/CollectionNode.tsx`
- Create `src/components/export/ExportCollectionDialog.tsx` and its test, `src/lib/__tests__/save-file.test.ts`

---

### Task 1: Crate skeleton, report types and YAML scrub

**Files:**
- Create: `crates/rocket-export/Cargo.toml`, `src/lib.rs`, `src/report.rs`, `src/ports.rs`, `src/scrub.rs`
- Modify: root `Cargo.toml`

**Interfaces:**
- Produces (all `pub`, re-exported from `lib.rs`):
  - `enum ExportAction { Translated, Blanked, Skipped, Preserved }` (serde camelCase)
  - `struct ExportNote { path: String, feature: String, action: ExportAction, detail: Option<String> }`
  - `struct ExportReport { exported: usize, items: Vec<ExportNote> }` (both camelCase, `Default`)
  - `struct SourceFile { rel_path: String, bytes: Vec<u8> }`
  - `trait CollectionFileSource: Send + Sync { fn list_files(&self, collection: &str) -> DomainResult<Vec<SourceFile>>; }`
  - `fn scrub_yaml(rel_path: &str, value: &mut serde_yaml::Value, notes: &mut Vec<ExportNote>)`
  - `fn strip_rocket_namespace(value: &mut serde_yaml::Value) -> bool` (true when something was removed)
  - `fn is_variable_reference(text: &str) -> bool`
  - `fn is_sensitive_header(name: &str) -> bool`

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (section 5 layout, section 2 auth).** Then read `crates/rocket-import/Cargo.toml`, `crates/rocket-infra/src/oc/auth.rs` (auth YAML keys), `crates/rocket-infra/src/oc/variables.rs`, `crates/rocket-app/src/redaction.rs` (`is_sensitive_header`) and the `extensions` fixture at `crates/rocket-infra/src/fs_collection/tests.rs:3126`.

- [ ] **Step 2: Create the crate files with the types above and failing tests in `src/scrub.rs`.** Add the crate to root `Cargo.toml` members and `rocket-export = { path = "crates/rocket-export" }` to `[workspace.dependencies]`. Dependencies: `rocket-shared`, `serde`, `serde_json`, `serde_yaml` (workspace), `zip = "2"`. Test example (the others follow the table):

```rust
fn yaml(src: &str) -> serde_yaml::Value {
    serde_yaml::from_str(src).expect("fixture yaml parses")
}

#[test]
fn blanks_literal_bearer_token_and_keeps_reference() {
    let mut v = yaml("http:\n  auth:\n    type: bearer\n    token: abc123\n");
    let mut notes = Vec::new();
    scrub_yaml("users/get.yml", &mut v, &mut notes);
    assert_eq!(v["http"]["auth"]["token"], serde_yaml::Value::String(String::new()));
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].action, ExportAction::Blanked);
    assert_eq!(notes[0].path, "users/get.yml");

    let mut kept = yaml("http:\n  auth:\n    type: bearer\n    token: \"{{authToken}}\"\n");
    let mut none = Vec::new();
    scrub_yaml("users/get.yml", &mut kept, &mut none);
    assert_eq!(kept["http"]["auth"]["token"], serde_yaml::Value::String("{{authToken}}".into()));
    assert!(none.is_empty());
}
```

| Test name | Input fixture | Expected |
|---|---|---|
| `blanks_credentials_for_each_auth_type` | one request per type under `http.auth`: basic `password`, digest `password`, wsse `password`, ntlm `password`, apikey `value`, awsv4 `secretAccessKey` and `sessionToken`, oauth1 `consumerSecret`, `accessTokenSecret`, `privateKey.value`, oauth2 `credentials.clientSecret` and `resourceOwner.password` | each listed key is `""`; usernames, URLs and ids unchanged |
| `scrubs_collection_and_folder_auth` | `request.auth` in `opencollection.yml` and `folder.yml` shapes; `runtime.auth` in a request | credentials blanked in all three places |
| `blanks_secret_variable_values` | `runtime.variables` and `request.variables` with `secret: true` (`value`, `initial`) and a normal variable | secret entry values `""`, normal entry unchanged |
| `blanks_literal_authorization_header` | `http.headers: [{name: Authorization, value: "Bearer abc123"}]` | value `""`, one note |
| `keeps_bearer_prefix_with_reference` | `value: "Bearer {{token}}"` | unchanged, no note |
| `blanks_url_password` | `http.url: "https://user:pass@host/x"` and `"https://user:{{pw}}@host/x"` | first becomes `https://user:@host/x`, second unchanged |
| `strip_removes_rocketapi_and_keeps_other_extensions` | fixture from `fs_collection/tests.rs:3126` | `rocketapi` gone, `bruno` and `other` kept, function returns true |
| `is_variable_reference_cases` | `{{a}}`, `Bearer {{a}}`, `abc`, `{{a}}x`, `` | true, true, false, false, false |

- [ ] **Step 3: Run `cargo test -j4 -p rocket-export`.** Expected: compile errors `cannot find function 'scrub_yaml' in this scope` (or the tests fail), zero passing.

- [ ] **Step 4: Implement the functions in `scrub.rs`.** Walk `serde_yaml::Value` by key, never through typed structs, so unknown keys survive. Credential key paths per auth `type` come from `oc/auth.rs` as read in Step 1. `strip_rocket_namespace` removes `extensions.rocketapi` and drops `extensions` entirely only if it becomes empty. `is_sensitive_header` uses the five names above.

- [ ] **Step 5: Run `cargo test -j4 -p rocket-export` and `cargo check -j4 -p rocket-export`.** Expected: all Task 1 tests pass (14 or more), no warnings.

### Task 2: Bruno ZIP, file source and export service

**Files:**
- Create: `crates/rocket-export/src/bruno.rs`, `crates/rocket-infra/src/fs_collection_file_source.rs`, `crates/rocket-app/src/collection_export_service.rs`
- Modify: `crates/rocket-export/src/lib.rs`, `crates/rocket-infra/src/lib.rs`, `crates/rocket-app/Cargo.toml` (add `rocket-export`), `crates/rocket-app/src/lib.rs`

**Interfaces:**
- Consumes: Task 1 types.
- Produces:
  - `fn bruno_zip(files: Vec<SourceFile>, collection_name: &str, include_environments: bool) -> DomainResult<(Vec<u8>, ExportReport)>`
  - `struct FsCollectionFileSource` with `fn new(workspace_path: Arc<Mutex<PathBuf>>) -> Self`, implementing `CollectionFileSource`
  - `struct ExportOutput { bytes: Vec<u8>, file_name: String, report: ExportReport }`
  - `struct CollectionExportService` with `fn new(files: Box<dyn CollectionFileSource>) -> Self` and `fn export_bruno(&self, collection: &str, include_environments: bool) -> DomainResult<ExportOutput>`

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.** Then read `crates/rocket-import/src/importer.rs:820-870` (what the importer skips), `crates/rocket-infra/src/fs_collection/mod.rs:40-70` (`collection_path`, `new_standalone`) and the `tempfile` fixture pattern in `crates/rocket-infra/src/fs_collection/tests.rs`.

- [ ] **Step 2: Write the failing tests.** In `bruno.rs`, a helper `file(path: &str, text: &str) -> SourceFile` and a helper `names(zip_bytes: &[u8]) -> Vec<String>` using `zip::ZipArchive`. Example:

```rust
#[test]
fn bruno_zip_has_single_root_folder_and_skips_noise() {
    let files = vec![
        file("opencollection.yml", "opencollection: \"1.0.0\"\ninfo:\n  name: my-api\n"),
        file("users/folder.yml", "info:\n  name: users\n"),
        file("users/get.yml", "info:\n  name: get\nhttp:\n  method: GET\n  url: https://x\n"),
        file("environments/local.yml", "name: local\nvariables: []\n"),
        file(".git/config", "x"),
        file("node_modules/a/b.js", "x"),
        file("workspace.yml", "name: ws\n"),
        file("notes.txt", "x"),
    ];
    let (bytes, report) = bruno_zip(files, "my-api", true).expect("zip builds");
    assert_eq!(
        names(&bytes),
        vec!["my-api/environments/local.yml", "my-api/opencollection.yml", "my-api/users/folder.yml", "my-api/users/get.yml"]
    );
    assert_eq!(report.exported, 3);
}
```

| Test name | Input | Expected |
|---|---|---|
| `bruno_zip_without_environments_leaves_folder_out` | same files, `include_environments=false` | no `my-api/environments/` entries |
| `bruno_zip_strips_rocketapi_namespace` | `opencollection.yml` with `extensions.rocketapi.sandboxMode: developer` and `extensions.bruno.other: 1` | unzipped file has no `rocketapi`, keeps `bruno` |
| `bruno_zip_blanks_credentials_and_reports` | request with `auth: {type: bearer, token: abc}` | unzipped token empty; report has a `Blanked` note naming the file |
| `bruno_zip_notes_rok_scripts` | request with `runtime.scripts: [{type: before-request, code: "rok.setVar('a', 1)"}]` | one `Preserved` note whose detail mentions `bru.*` |
| `bruno_zip_keeps_proto_and_js_files` | `protos/a.proto`, `shared/util.js` | both present |
| `bruno_zip_rejects_unparsable_yaml` | `users/bad.yml` containing `: : :` | `Err`, no partial bytes |

In `fs_collection_file_source.rs`: `file_source_lists_collection_files_and_skips_git` (tempfile workspace with `collections/my-api/{opencollection.yml,users/get.yml,.git/config}` lists the first two, relative paths with `/`) and `file_source_rejects_unsafe_collection_name` (`../x`, `a/b`, `a\\b`, empty give `Err(DomainError::InvalidInput(..))`). In `collection_export_service.rs`: `export_bruno_names_the_file` using an in-memory source returning one file; expects `file_name == "my-api.zip"` and bytes non-empty.

- [ ] **Step 3: Run `cargo test -j4 -p rocket-export bruno`, then `cargo test -j4 -p rocket-infra file_source`, then `cargo test -j4 -p rocket-app export_bruno`.** Expected: compile errors for the missing functions and types.

- [ ] **Step 4: Implement the signatures above.** `bruno_zip` parses each `.yml`/`.yaml` file, runs `strip_rocket_namespace` then `scrub_yaml`, writes it back with `serde_yaml::to_string`, copies `.proto` and `.js` bytes unchanged, and writes entries sorted by path under `<collection_name>/`. The file source canonicalises nothing outside `<ws>/collections/<name>`, validates the name as the importer and `env_service_for` do (no `\0`, leading `/` or `\`, or `..` component), and skips `.git`, `node_modules` and symlinks. `export_bruno` sanitises the zip file name to `<collection>.zip`.

- [ ] **Step 5: Run the three test commands again plus `cargo check -j4 -p rocket-app` and `cargo check -j4 -p rocket-infra`.** Expected: all Task 2 tests pass (6 + 2 + 1), no warnings.

### Task 3: Tauri command, save helper, dialog and menu item

**Files:**
- Create: `src-tauri/src/commands/export.rs`, `src/components/export/ExportCollectionDialog.tsx`, `src/components/export/ExportCollectionDialog.test.tsx`, `src/lib/__tests__/save-file.test.ts`
- Modify: `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs` (handler list), `src/lib/tauri-api.ts`, `src/lib/save-file.ts`, `src/components/collections/CollectionNode.tsx`

**Interfaces:**
- Consumes: `CollectionExportService::export_bruno`, `FsCollectionFileSource::new`.
- Produces:
  - Rust: `#[tauri::command] pub async fn export_collection_bruno(collection: String, include_environments: bool, workspace_path: State<'_, Arc<Mutex<PathBuf>>>) -> Result<ExportResultDto, DomainError>`; `struct ExportResultDto { data_base64: String, file_name: String, report: ExportReport }` (camelCase)
  - TS: `export interface ExportNote { path: string; feature: string; action: 'translated' | 'blanked' | 'skipped' | 'preserved'; detail?: string }`, `export interface ExportReport { exported: number; items: ExportNote[] }`, `export interface ExportResult { dataBase64: string; fileName: string; report: ExportReport }`, `export const exportCollectionBruno = (collection: string, includeEnvironments: boolean) => invoke<ExportResult>(...)`
  - `export async function saveBinaryFile(defaultName: string, bytes: Uint8Array, filters: FileFilter[]): Promise<boolean>` and `export function base64ToBytes(b64: string): Uint8Array`
  - `export function ExportCollectionDialog(props: { open: boolean; collectionName: string; onClose: () => void })`

- [ ] **Step 1: Read `src-tauri/src/commands/import.rs` (service built per call), `src-tauri/src/commands/load_test.rs:50-70` (`export_load_test`), `src/lib/tauri-api.ts:1765-1790`, `src/lib/save-file.ts`, the context menu at `src/components/collections/CollectionNode.tsx` (`ContextMenuContent` and the `DropdownMenu` "more" button, both list the same actions) and `src/components/import/ImportCollectionDialog.test.tsx:1-60` for the mock style.**

- [ ] **Step 2: Write the failing frontend tests.**

```ts
// src/lib/__tests__/save-file.test.ts
const mockSave = vi.fn();
const mockWriteFile = vi.fn();
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: (...a: unknown[]) => mockSave(...a) }));
vi.mock('@tauri-apps/plugin-fs', () => ({ writeFile: (...a: unknown[]) => mockWriteFile(...a) }));

it('saveBinaryFile returns false when the dialog is cancelled', async () => {
  mockSave.mockResolvedValue(null);
  await expect(saveBinaryFile('a.zip', new Uint8Array([1]), [])).resolves.toBe(false);
  expect(mockWriteFile).not.toHaveBeenCalled();
});

it('saveBinaryFile writes the bytes to the chosen path', async () => {
  mockSave.mockResolvedValue('/tmp/a.zip');
  await expect(saveBinaryFile('a.zip', new Uint8Array([1, 2]), [])).resolves.toBe(true);
  expect(mockWriteFile).toHaveBeenCalledWith('/tmp/a.zip', new Uint8Array([1, 2]));
});

it('base64ToBytes decodes standard base64', () => {
  expect(Array.from(base64ToBytes('AQI='))).toEqual([1, 2]);
});
```

Dialog tests in `ExportCollectionDialog.test.tsx` (mock `@/lib/tauri-api` with `exportCollectionBruno`, mock `@/lib/save-file` with `saveBinaryFile` and the real `base64ToBytes`, mock `sonner`): `renders the format choice and an Include environments checkbox on by default`; `export calls exportCollectionBruno with the collection name and the checkbox value, then saves the decoded bytes under the returned file name`; `shows the report items grouped by action after a successful export`; `shows an error toast and no report when the export fails`; `does not show a report when the save dialog is cancelled`.

- [ ] **Step 3: Run `yarn test save-file ExportCollectionDialog`.** Expected: failures that the modules or exports do not exist.

- [ ] **Step 4: Implement.** (a) `save-file.ts`: add `saveBinaryFile` (same `save` plus `writeFile` pattern as `saveTextFile`) and `base64ToBytes`. (b) `tauri-api.ts`: the types and `exportCollectionBruno`. (c) `export.rs`: build `CollectionExportService::new(Box::new(FsCollectionFileSource::new(Arc::clone(&*workspace_path))))` per call (the same pattern as `make_import_service`), call `export_bruno`, base64 the bytes like `export_load_test`; register the module in `commands/mod.rs` and the command in `generate_handler!`. (d) Dialog with shadcn `Dialog`, `Select` (Bruno collection ZIP now; Postman added in plan 2), `Checkbox` for `Include environments` (default on), `Button`, lucide icons, report list below. (e) `CollectionNode.tsx`: local `useState` for the dialog, an `Export...` item (lucide `Download` icon) in both the `ContextMenuContent` and the `DropdownMenu`, between `New Script` and `Rename`, rendering the dialog with `collectionName={summary.name}`.

- [ ] **Step 5: Run `yarn test save-file ExportCollectionDialog`, `yarn tsc --noEmit`, `yarn check`, then `cargo check -j4 -p <package name in src-tauri/Cargo.toml>`.** Expected: all 8 new frontend tests pass, no type, lint or Rust errors.

Next plan: docs/superpowers/plans/2026-10-23-collection-export/plan-02-collection-export.md
