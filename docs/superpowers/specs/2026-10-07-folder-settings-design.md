# Folder Settings Tab

Date: 2026-10-07. Status: draft for review.

## Goal

Clicking a folder opens a Folder Settings tab with the sub-tabs Headers, Script, Test, Vars, Auth and Docs, like Bruno. The settings are stored in `folder.yml` and are applied to every request below the folder when it runs. A Rocket collection exported to Bruno must keep working, so the on-disk shape stays strictly OpenCollection.

## Decisions already made

- Scope is full inheritance: the tab edits and persists, and execution applies the folder chain for headers, auth, scripts, tests and vars.
- Bruno compatibility is a hard constraint. The schema is `additionalProperties: false`, so `folder.yml` carries no Rocket-only fields.
- Folder Vars is pre-request only. The spec's `RequestDefaults` has no post-response variable slot, so a Post Response section is out of scope. Scripts can still set variables after a response.
- Script order is sandwich by default. `extensions.bruno.scripts.flow` (`sandwich` or `sequential`) in `opencollection.yml` is read, honored and preserved on write.
- Importing Bruno `.bru`-format `folder.bru` files is a separate follow-up.
- Scripts that use `bru.*` stay non-portable (existing no-`bru`-alias decision).

## Approach

A dedicated `FolderSettings` value object, mirroring `CollectionSettings`. It is not placed on the `Folder` tree struct, so the sidebar load and summaries path stay light.

### On-disk shape (`folder.yml`)

```yaml
info: { name, type: folder, seq, tags }   # unchanged
request:
  headers:   [ { name, value, description, disabled } ]
  auth:      <Auth | "inherit">
  variables: [ <Variable> ]
  scripts:
    - { type: before-request, code }
    - { type: after-response, code }
    - { type: tests, code }
docs: string | { content, type }
```

Rules:
- A section left empty is omitted, not written as an empty list.
- Reads keep the current legacy-shape fallback in `folder_file.rs`.
- Unknown keys under `request` (for example `metadata`, `settings`) are kept on save, as `save_folder_variables` does today.
- `request.scripts` entries of type `hooks` are kept untouched.

## Components

### Backend

| Layer | Change |
|---|---|
| `rocket-collection` | `FolderSettings { headers, auth, variables, pre_request_script, post_response_script, tests_script, docs }`. A pure `merge_folder_chain` helper for ordering rules. Script flow enum. |
| `CollectionRepository` | `get_folder_settings`, `save_folder_settings`, and `get_folder_chain_settings` (ancestors outermost first). Existing `get_folder_variables`, `save_folder_variables` and `get_folder_chain_variables` delegate to them. |
| `rocket-infra` | `fs_collection/folders.rs` and `folder_file.rs` read and write the `request` and `docs` blocks. Unknown-key preservation. Script flow read from `opencollection.yml` extensions. |
| `rocket-app` | `ExecutionService` loads the folder chain once per request and applies it (see Runtime rules). A `FolderSettingsSaved` domain event on save. |
| `src-tauri` | Thin commands `get_folder_settings` and `save_folder_settings`, with camelCase IPC DTOs separate from the persistence structs. |

### Frontend

- New tab type `folder`, keyed by collection name and folder path. `openFolderTab` in `pane-store`, with an active sub-section field like the collection tab.
- `FolderSettingsTab` with six sub-tabs. It reuses `HeadersEditor`, `AuthEditor`, `VarsTab` and the Markdown docs editor. `ScriptsTab` is split by phase: Script shows pre-request and post-response, Test shows tests.
- `FolderNode` opens the tab on click, and "Settings" is added to the folder menu. The expand and collapse chevron keeps its current behavior. `FolderVariablesPopover` is replaced by the Vars sub-tab.
- One Save button and a dirty flag, as in `CollectionOverviewTab`. Each save reads current settings first, so it never overwrites fields the tab does not edit.

## Runtime rules

Chain order is outermost folder to innermost, with the collection above all folders.

| Concern | Rule |
|---|---|
| Vars | Existing behavior, unchanged: request beats folder beats environment beats collection. Inner folder beats outer. Disabled entries are skipped. |
| Headers | Collection, then folders, then request. A more specific level replaces a duplicate header name. Disabled headers do not shadow. |
| Auth | A request set to `inherit` takes the nearest folder with auth other than `inherit`, then the collection, then none. All auth types are allowed on a folder. |
| Scripts, sandwich | Pre-request: collection, folders, request. Post-response and tests: request, folders (innermost first), collection. |
| Scripts, sequential | Every phase runs collection, folders, request. |
| Variable access in scripts | A folder-scope getter in the `rok` namespace, matching how other scopes are exposed. |

The sandbox mode, script-file `require()` rules and the secret-handling rules in `execution_service.rs` apply to folder scripts exactly as to request scripts. The existing check that blocks `getSecretVar` use applies to folder scripts too.

## Error handling

- A `folder.yml` that fails to parse during execution is reported as a request error naming the folder. It does not silently drop its settings.
- A save to a missing folder is an `InvalidInput` error.
- Folder script errors surface as `script_error` in phase order, like request scripts.

## Testing

- Unit: merge ordering for headers, auth and scripts in both flows. Disabled-entry handling.
- Infra: folder.yml roundtrip with every section populated, legacy-shape read, unknown-key preservation, omission of empty sections.
- Schema: extend `schema_shape_tests.rs` so a fully populated `folder.yml` passes the schema guard.
- Execution (wiremock): a request in a nested folder receives merged headers, inherited auth and chained scripts.
- Frontend: Vitest for the tab, the store action, and dirty and save behavior.
- Verification: `cargo check -j4`, targeted crate tests, `yarn tsc --noEmit`, `yarn check`.

## Plan split (at most 3 tasks each)

1. Domain `FolderSettings`, repo methods, infra read and write, `folder.yml` roundtrip and schema tests.
2. IPC commands and DTOs, `folder` tab type and store action, sidebar click, tab shell with Vars and Docs.
3. Headers, Auth, Script and Test sub-tabs.
4. Runtime header and auth inheritance.
5. Runtime script and test chain, flow setting, folder-scope getter.

Each plan starts with: read `docs/superpowers/specs/opencollection-spec-reference.md`.

## Out of scope

- Folder Post Response vars.
- `folder.bru` import.
- A `bru` global alias.
- Per-folder script flow (Bruno sets it per collection).
