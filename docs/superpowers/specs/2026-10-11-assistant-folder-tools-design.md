# Assistant folder tools: design

Date: 2026-10-11. Status: design approved by the user in chat, spec ready for review.
Read first: `docs/superpowers/specs/opencollection-spec-reference.md` (sections 2.7, 5, 6), `.claude/folder-settings.md`, `docs/superpowers/specs/2026-10-09-workspace-ai-assistant-design.md`, `crates/rocket-acp/CLAUDE.md`.

## 1. Summary

The workspace assistant can create, move and rename folders, but it cannot read what a folder holds or change it. A folder's own settings (default headers, variables, the three scripts, docs and auth) change what every request below it sends, so an assistant that cannot see them gives wrong advice. Today it sees them only when the user attaches a `#` folder chip.

This design adds:
1. A read tool `get_folder_settings`.
2. A proposal change `UpdateFolderSettings` that the user accepts or rejects, like every other edit.

Folder auth is not editable by the agent. This matches the rule for requests.

## 2. Decisions made with the user

- Scope: read plus proposed edits (2026-10-11), not read-only and not full edits including auth.
- Auth stays out of the patch.
- Folder variables are plain text in `folder.yml`, so a proposal may set any variable value, as `SetEnvVar` does for non-secret values.
- A folder script gets the same validation as `EditScript`, with no extra scanner.

## 3. Goals and non-goals

Goals:
1. The agent can read a folder's settings in every mode, masked like the other read tools.
2. In Edit and Agent mode the agent can propose changes to a folder's headers, variables, scripts and docs.
3. Accepting a proposal writes through the same path as a manual Folder Settings save, with a fingerprint and stale check.

Non-goals:
- Folder auth edits (needs a separate security decision).
- Collection-level variants, deleting folders, a per-folder script flow.
- Secret folder variables. A folder variable is never secret at run time today, so a proposal cannot create one.

## 4. OpenCollection compatibility

- `folder.yml` stays strictly OpenCollection (`additionalProperties: false`). No Rocket-only key is added. The schema guard in `schema_shape_tests.rs` must keep passing.
- The write uses `CollectionService::save_folder_settings`, which reads the file first and replaces only `headers`, `variables`, the three typed scripts and `docs`. `auth`, `request.metadata`, `request.settings`, `info.seq` and hooks scripts survive. An empty section is omitted from the file.
- No transcript, proposal or session data is written into any collection file.

## 5. Read tool

`get_folder_settings(collection, folder_path)`, in `McpToolService` and `src-tauri/src/mcp/tool_server.rs`.

- Returns the existing `MaskedFolderSettings` (auth type and masked auth, headers, variables, scripts, docs) plus `children`: the names of the folder's direct subfolders and requests, so the agent can navigate.
- Allowed in every mode. The tool list does not change per mode.
- Checks the collection against the session's pinned workspace (`check_in_workspace`), validates the path with the same rules as the other tools, and returns a clear agent-visible error for a missing folder. `""` is refused with a message that points to `get_collection_settings` for the root.
- Publishes `DomainEvent::AcpToolInvoked` with a value-free line such as `read the settings of folder 'auth/v2' in 'my-api'`.
- Reads through `get_folder_settings` on the collection repository. A `folder.yml` that exists but cannot be parsed returns an agent-visible error naming the folder, never partial data.

## 6. Proposal change

In `rocket-acp` `proposal.rs`:

```rust
ProposedChange::UpdateFolderSettings {
    collection: String,
    folder_path: String,
    patch: FolderSettingsPatch,
    base_fingerprint: String,
}

pub struct FolderSettingsPatch {
    pub headers: Option<Vec<Header>>,
    pub variables: Option<Vec<CollectionVariable>>,
    pub pre_request_script: Option<String>,
    pub post_response_script: Option<String>,
    pub tests_script: Option<String>,
    pub docs: Option<String>,
}
```

- A given field replaces that whole section. An omitted field is untouched. A script given as an empty string clears that script. `is_empty()` and `changed_fields()` follow `RequestPatch`.
- There is no `auth` field. The tool parameter struct uses `#[serde(deny_unknown_fields)]`, so an `auth` field is refused with a message, not dropped.
- `collection()`, `base_fingerprint()`, `set_base_fingerprint()` and `summary()` handle the new variant. The summary lists section names only, for example `Update folder 'auth/v2' in my-api: headers, pre-request script`. It never holds a header value, variable value or script body.
- Edit mode is required to propose it, checked by the tool server like the other changes.
- `propose_changes` in the tool server gets a matching `UpdateFolderSettings` params variant with the size caps below.

## 7. Validation and safety

In `ProposalService::prepare`:
- `normalize_paths` and `validate_relative_path` run on `folder_path`. The folder must exist (`NotFound` otherwise). `""` (the root) is refused.
- An empty patch is refused with "the patch changes no field".
- `refuse_masked_values` also scans the patch: header keys and values, variable keys and values, scripts and docs. A value that contains the masked placeholder is refused with the existing message, so a masked read can never be saved back over a real value.
- The base fingerprint is the SHA-256 of the folder's stored settings form (all fields of `FolderSettings` that the patch can change, plus `auth`, so an auth change by the user in between also makes the proposal stale). A missing `folder.yml` hashes as an empty form.
- A folder variable whose name is empty is refused. Duplicate variable keys in one patch are refused.

In `ProposalService::still_applies` and `apply`:
- `still_applies` recomputes the fingerprint (a gone folder is `Stale`).
- `apply` re-reads the settings, compares the fingerprint on the very copy it then patches and saves (as `apply_env_var` does), applies only the given sections, and saves through `CollectionService::save_folder_settings`. A mismatch returns `false` (`Stale`) and writes nothing. A failed write is `Failed`.
- `FolderSettingsSaved { collection, folder_path }` is published by the existing save path.

Size caps (constants next to the existing ones in `tool_server.rs`): at most 50 headers and 100 variables per call, each key at most 256 bytes, each value at most 8 KB, each script and the docs at most 64 KB, counted into the existing total-bytes cap (`check_call_bytes`).

## 8. Frontend

- `AgentProposalDto` / `ProposedChangeDto` in `src-tauri/src/commands/agent_proposals.rs` get `UpdateFolderSettings` (tag `updateFolderSettings`), carrying `collection`, `folderPath` and the sections that changed. Header and variable values are included, because the card already shows request header values the same way. Values the user may not see are masked by the read views, not here, because the agent only ever proposes values it chose.
- `src/lib/tauri-api.ts` gets the matching type. `proposalTarget`, `proposalPreview` and `loadProposalDiff` handle it: scripts and docs preview as a diff against the stored version, headers and variables as a definition list of the changed sections.
- `dirtyAffectedTabPlace` also treats an open, dirty Folder Settings tab of that folder as affected, so Accept is blocked with the same wording. A clean open folder tab reloads after the accept (the tab does not listen to `folderSettingsSaved` today; the fix is to reload on the proposal-resolved event for that folder, not a general listener, see `.claude/folder-settings.md`).
- No new view. The History view shows the proposal as a plain line through its summary.

## 9. Errors and edge cases

- Folder renamed, moved or deleted between propose and accept: `Stale`.
- User edits the folder in the Folder Settings tab and saves between propose and accept: `Stale`.
- Another proposal for the same folder pending: both can be pending. The second to be accepted is `Stale`, because the first changed the fingerprint.
- A `folder.yml` that does not parse: propose is refused with an error naming the folder.
- gRPC requests do not inherit folder settings (existing gap). The tool description does not claim they do.
- Workspace switch during a session: the session pin decides the workspace, as for every tool.

## 10. Testing

- `rocket-acp`: patch `is_empty`, `changed_fields`, summary never holds a value, the variant in `collection()` and fingerprint accessors.
- `rocket-app` `mcp_tool_service`: read works in every mode, outside-workspace collection refused, missing folder and root refused, secrets and credentials masked, `AcpToolInvoked` published, `folder.yml` parse failure is an error.
- `rocket-app` `proposal_service`: propose then accept writes only the given sections, the other sections and `auth` survive, stale on a user edit and on a gone folder, masked placeholder refused, empty patch refused, duplicate variable refused, Ask mode refuses.
- `src-tauri` tool server: `deny_unknown_fields` refuses `auth`, size caps, the tool list contains `get_folder_settings` in every mode, DTO casing.
- `rocket-infra`: a save keeps `request.metadata`, `info.seq` and hooks (existing tests cover the repo; add one through the proposal path if no end to end test exists).
- Vitest: card preview for each section kind, dirty folder tab blocks Accept, clean folder tab reloads after accept.
- Manual: ask the assistant to read a folder, then to add a header and a pre-request script to it, accept, and check the Folder Settings tab and a request run.

## 11. Plans

Plans hold at most 3 tasks each.

- Plan 1 (backend): `FolderSettingsPatch` and the variant with fingerprint accessors and summary in `rocket-acp`; `get_folder_settings` in `McpToolService`; `prepare`, `still_applies`, `apply` and masked-value scanning in `ProposalService`.
- Plan 2 (tool server and frontend): tool schema and params with caps and the read tool wiring; DTO and `tauri-api` type; card preview, dirty-tab handling and reload; docs (`.claude/assistant-followups.md`, `crates/rocket-acp/CLAUDE.md`, the roadmap row).

Each task runs the usual loop: implementer, review, fix. The proposal safety task gets an Opus review.
