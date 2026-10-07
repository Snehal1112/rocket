# Folder Settings Tab, Plan Index

**Goal:** clicking a folder opens a Folder Settings tab (Headers, Script, Test, Vars, Auth, Docs) like Bruno. Settings persist in an OpenCollection-shaped `folder.yml` and apply to every request below the folder at run time.

**Spec:** [2026-10-07-folder-settings-design.md](../../specs/2026-10-07-folder-settings-design.md). Read it before any plan.

**Scope:** 12 plans, max 3 tasks each. Each plan ends with a **Next Plan** section. Run one plan at a time and chain to the next when the current one finishes.

## Plan breakdown

| # | Plan | Area | Depends on |
|---|---|---|---|
| 01 | Domain model and resolution helpers | rocket-collection | none |
| 02 | folder.yml persistence | rocket-infra | 01 |
| 03 | Script flow setting (`extensions.bruno.scripts.flow`) | rocket-collection, rocket-infra | 01, 02 |
| 04 | IPC commands, DTOs, event, TS bindings | src-tauri, frontend api | 02 |
| 05 | Runtime header and auth inheritance (backend and frontend send path) | rocket-app, frontend | 01, 02, 04 |
| 06 | Runtime script and test chain | rocket-app | 01, 02, 03, 05 |
| 07 | Folder variable API in scripts | rocket-infra scripting, rocket-app, frontend | 02, 06 |
| 08 | Folder tab shell, store action, sidebar click | frontend | 04 |
| 09 | Settings hook, Vars and Docs sub-tabs | frontend | 08 |
| 10 | Headers and Auth sub-tabs | frontend | 09, 05 |
| 11 | Script and Test sub-tabs | frontend | 09 |
| 12 | Bruno compatibility verification and docs | infra tests, docs | all |

Recommended order: 01 to 12. Plans 08, 09 and 11 only need plan 04, so they can run before 05 to 07 if wanted. Plan 10 must run after plan 05, because both rewrite the auth block in `resolveRequestFieldsForPath`.

## Locked contract

Every plan is written against these names. If an implementer must deviate, update this index and every plan that mentions the name.

### Rust domain (`rocket-collection`, new module `folder_settings`, re-exported at the crate root)

```rust
pub struct FolderSettings {            // Debug, Clone, PartialEq, Default
    pub headers: Vec<rocket_shared::types::Header>,
    pub auth: Option<rocket_shared::types::Auth>,   // None or Some(Auth::Inherit) both mean "no folder auth"
    pub variables: Vec<CollectionVariable>,         // pre-request only
    pub pre_request_script: Option<String>,         // OC scripts[] type "before-request"
    pub post_response_script: Option<String>,       // OC scripts[] type "after-response"
    pub tests_script: Option<String>,               // OC scripts[] type "tests"
    pub docs: Option<String>,                       // OC top-level docs, content only
}
pub enum ScriptFlow { Sandwich /* default */, Sequential }   // serde lowercase; also in CollectionSettings.script_flow
pub enum ScriptPhase { PreRequest, PostResponse, Tests }

/// Collection plus folders (outermost first) headers. Inner replaces outer by key. Disabled entries never shadow.
pub fn inherited_headers(collection: &[Header], folders: &[FolderSettings]) -> Vec<Header>;
/// Innermost folder auth that is not None or Inherit.
pub fn resolve_folder_auth(folders: &[FolderSettings]) -> Option<Auth>;
/// Scripts for one phase in execution order, including the request's own script.
/// Sandwich: PreRequest = outer folders to inner, then request. PostResponse and Tests = request, then inner folders to outer.
/// Sequential: every phase = outer folders to inner, then request. Blank scripts are skipped.
pub fn chain_scripts(phase: ScriptPhase, flow: ScriptFlow, folders: &[FolderSettings], request_script: Option<&str>) -> Vec<String>;
```

### Repository (`CollectionRepository`, all three methods defaulted so existing test doubles keep compiling)

```rust
fn get_folder_settings(&self, collection: &str, folder_path: &str) -> DomainResult<FolderSettings>;   // default: Err(Internal("folder settings not supported"))
fn save_folder_settings(&self, collection: &str, folder_path: &str, settings: &FolderSettings) -> DomainResult<()>; // same default
fn get_folder_chain_settings(&self, collection: &str, request_path: &str) -> DomainResult<Vec<FolderSettings>>;    // default: Ok(vec![]); outermost folder first
```

`folder_path` is relative to the collection root, `""` for the root. `get_folder_variables`, `save_folder_variables` and `get_folder_chain_variables` keep their signatures and read or write the same `request.variables`.

### On-disk shape (`folder.yml`, strictly OpenCollection)

`request.headers`, `request.auth`, `request.variables`, `request.scripts[]` (types `before-request`, `after-response`, `tests`; `hooks` entries are preserved untouched), and top-level `docs`. Empty sections are omitted. The existing `metadata` and `settings` under `request` are preserved on save. No Rocket-only fields.

### Script flow

`CollectionSettings.script_flow: ScriptFlow`, persisted at `extensions.bruno.scripts.flow` in `opencollection.yml` (`sandwich` or `sequential`, absent means sandwich). Other keys under `extensions` are preserved on save.

### IPC (plan 04)

Tauri commands `get_folder_settings(collection: String, folder_path: String) -> FolderSettingsDto` and `save_folder_settings(collection: String, folder_path: String, settings: FolderSettingsDto) -> ()`. `FolderSettingsDto` is a camelCase IPC DTO in `src-tauri/src/commands/`, separate from the domain struct. Domain event `FolderSettingsSaved { collection, folder_path }`. Frontend wrappers in `src/lib/tauri-api.ts`: `getFolderSettings(collection, folderPath)` and `saveFolderSettings(collection, folderPath, settings)`, with the TS type `FolderSettings` mirroring the DTO.

### Frontend (plans 08 to 11)

- `FolderSection = 'headers' | 'script' | 'test' | 'vars' | 'auth' | 'docs'`.
- Tab: `FolderTab { tabType: 'folder'; collectionName: string; folderPath: string; activeSection: FolderSection }`.
- Store: `openFolderTab(collection: string, folderPath: string, section?: FolderSection): boolean` and `updateFolderSection(tabId: string, section: FolderSection): void` in `src/stores/pane-store.ts`.
- Component `src/components/collections/FolderSettingsTab.tsx`. Hook `src/hooks/useFolderSettings.ts` returning `{ settings, setSettings, isDirty, isLoaded, save, saveState }`.
- Hard rules from CLAUDE.md apply: shadcn/ui primitives only, lucide-react icons, `SingleLineEditor` for single-line fields, Monaco for multi-line, narrow Zustand selectors.

### Runtime

- Headers: `merge_headers(&inherited_headers(&settings.headers, &folders), &request.headers)`. Request wins by key, as today.
- Auth: `merge_auth(request_auth, resolve_folder_auth(&folders).or(settings.auth))`.
- Scripts: `chain_scripts(...)` per phase using `settings.script_flow`. The sandbox mode, `require()` rules and secret-use scan apply to folder scripts exactly as to request scripts.
- Chain source: one call to `get_folder_chain_settings(collection, request_path)` per execution. A `folder.yml` that fails to parse during execution is an error naming the folder, never silently dropped.

## Merge points between plans

Plans touch these files more than once, so rebase carefully: `crates/rocket-collection/src/repository.rs`, `crates/rocket-infra/src/fs_collection/folder_file.rs`, `fs_collection/variables.rs`, `fs_collection/schema_shape_tests.rs`, `crates/rocket-app/src/execution_service.rs`, `src/stores/pane-store.ts`, `src/lib/tauri-api.ts`, `src/components/collections/FolderNode.tsx`.

## Rules every plan follows

- Starts with the line: "Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`" for any task touching folder.yml, collections, variables, auth or related Tauri commands.
- Cargo commands always pass `-j4`, target one crate, never `--workspace`.
- Commits use the `dev-workflow-skills:1-git-commit` skill with a pathspec (`git add <paths>` then commit with those paths), conventional commit format, never `git add -A`.
- Frontend tasks end with `yarn tsc --noEmit` and `yarn check`.

## Additions found while writing the plans

These are part of the contract now. Each plan already uses them.

- **Frontend names (plan 08):** section files are `HeadersSection`, `ScriptSection`, `TestSection`, `VarsSection`, `AuthSection`, `DocsSection` under `src/components/collections/folder-settings/`. All take `FolderSectionProps { collectionName: string; folderPath: string; settings?: FolderSettings; onChange?: (patch: Partial<FolderSettings>) => void }` (plan 09 makes `settings` and `onChange` required in the sections it fills in). Extra store helpers: `renameFolderTabs(collection, oldPath, newPath)`, `findFolderTab`, `findFolderTabsWithin`. `openFolderTab` returns `true` when an existing tab was reused.
- **Hook (plan 09):** `useFolderSettings` also returns `error: string | null`, and `setSettings` accepts a value or an updater.
- **IPC (plan 04):** the TS `FolderSettings` has `auth: Auth | null`. The event is `FolderSettingsSaved` on the existing `collection-changed` channel, with the TS tag `folderSettingsSaved`. A save to a missing folder directory is `InvalidInput`.
- **Persistence (plan 02):** `save_folder_settings("")` returns `InvalidInput` (the collection root has no `folder.yml`). `save_folder_variables` keeps editing only `request.variables`. `get_folder_chain_variables` keeps skipping a corrupt `folder.yml`, while `get_folder_chain_settings` reports it by name.
- **Runtime (plan 06):** `ExecuteRequestInput` gains `skip_folder_scripts: bool` (`#[serde(default)]`). Each script in a phase runs as its own engine call. Folder script errors read `Folder "<path>" <phase> script: <msg>`.
- **Frontend auth (plans 05 and 10):** plan 05 owns `src/lib/folder-inheritance.ts` and the headers and non-OAuth2 auth path. Plan 10 owns `resolveInheritedFolderAuth` in `src/lib/inherited-auth.ts` and the folder OAuth2 token store, layered on plan 05's block.
- **Variable fidelity (plan 12):** `OcVariable::from(CollectionVariable)` drops variable descriptions and writes an `initial` key. Plan 12 task 1 adds the `folder_oc_variables` helper that fixes both. Until plan 12 runs, saving a folder's variables from the tab can drop descriptions in a Bruno-authored file.

## Decisions to confirm

- **Header name case:** matching is case-sensitive on the backend (as `merge_headers` is today) and case-insensitive on the frontend send path. Both are existing rules, kept as is.
- **Folder "No Auth":** a folder cannot switch auth off for its children. The UI offers Inherit and real auth types, never None.
- **Collection scripts:** Rocket has no collection-level scripts, so the sandwich order covers folders and the request only.
- **Known gaps outside this series:** gRPC does not inherit folder settings, Flow and the runner get no interactive-OAuth2 folder token, and importing a Bruno `sequential` collection resets the flow to sandwich (importer builds fresh settings).
