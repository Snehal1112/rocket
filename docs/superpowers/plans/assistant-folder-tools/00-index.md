# Assistant Folder Tools: Plan Index

**Spec:** [../../specs/2026-10-11-assistant-folder-tools-design.md](../../specs/2026-10-11-assistant-folder-tools-design.md).

**Context:** builds on the workspace AI assistant series, which lives on the unmerged branch `worktree-acp-mcp-tool-server`. Work on that branch, never on `main`.

## Plans

| # | Plan | Areas | Depends on |
|---|---|---|---|
| 01 | [Backend](2026-10-11-assistant-folder-tools-plan-01-backend.md): `FolderSettingsPatch` and the `UpdateFolderSettings` change (with its IPC DTO), `get_folder_settings` in `McpToolService`, `ProposalService` prepare, still-applies and apply | `rocket-acp`, `rocket-app`, `src-tauri` (DTO) | — |
| 02 | [Tool server and frontend](2026-10-11-assistant-folder-tools-plan-02-tool-server-frontend.md): MCP tool and params, TS types, card preview and diff, dirty and reload handling, docs | `src-tauri`, `src/lib`, `src/stores`, `src/hooks`, `src/components/assistant` | 01 |

Build in order 01, 02, one plan at a time. The next plan starts without asking once the current one is complete. Never push; merging is the user's decision.

## Model hints (one line per task)

| Task | Implementer | Reviewer |
|---|---|---|
| P01 T1 Types in `rocket-acp` and the IPC DTO | sonnet | haiku |
| P01 T2 `get_folder_settings` in `McpToolService` | sonnet | sonnet |
| P01 T3 `ProposalService` prepare, still-applies, apply | sonnet | **opus** (fingerprint on the saved copy, masked-value scan, auth survival) |
| P02 T1 Tool server: read tool, propose params, caps | sonnet | sonnet |
| P02 T2 TS types, view, diff, card | sonnet | sonnet |
| P02 T3 Folder edit store, dirty block, reload, docs | sonnet | sonnet |

The final whole-branch review after Plan 02 runs on opus.

## Execution notes

- **Ledger:** one per plan at `.superpowers/sdd/2026-10-11-assistant-folder-tools-plan-0N/progress.md`. "Task N: complete" means done. Log decisions as `Ruling:` lines. After `/clear` or compaction, use the `resume-plan` skill.
- **One implementer per task, one at a time per worktree.** Reviewers read a diff file, not the live tree. Re-reviews cover only the fixes since the last review.
- **Cross-plan compile rule:** `cargo check --workspace --all-targets -j4`, `yarn tsc --noEmit` and `yarn check` are green at the end of every task.
- **Tests:** each task writes its tests and runs only the focused commands named in it, plus the compile and lint checks. The user runs the full suites.
- **Cargo:** always `-j4`; never `cargo test --workspace` or `--all`.
- **Commits:** every commit through the `dev-workflow-skills:1-git-commit` skill, conventional format, explicit `git add <paths>` and `git commit --only -- <paths>`; never `git add -A`, `--all` or `.`; no bare `git stash`. Dispatched implementers get this rule in their prompt.
- **OpenCollection:** every backend task starts with "📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`." `folder.yml` stays strictly OpenCollection; the write goes through `CollectionService::save_folder_settings` only.
- **Docs:** tasks update `crates/rocket-acp/CLAUDE.md`, `crates/rocket-app/CLAUDE.md`, `.claude/assistant-followups.md`, `.claude/folder-settings.md` and one line in `.claude/assistant-composer.md` if it lists tools. Tasks do not edit the roadmap.

## Rulings (controller, 2026-10-11)

- Ruling: the patch variables use a new `ProposedVariable { key, value, enabled }` in `rocket-acp`, not `CollectionVariable`, because `rocket-acp` must not depend on `rocket-collection` (its CLAUDE.md rule). `rocket-app` converts to `CollectionVariable { secret: false, initial_value: value }` — cost if wrong: none, the spec type was a sketch. An equal `initial_value` and `value` is how folder variables are loaded and means no `initial` key is written.
- Ruling: spec section 8 says a dirty folder tab is found through the tab. It cannot be: a folder tab keeps its edits in `useFolderSettings` hook state and `FolderTab.isDirty` is always false (`.claude/folder-settings.md`, "Dirty folder tabs"). Plan 02 T3 adds a small zustand store `folder-edit-store` that the hook writes its dirty flag to and reads reload requests from. It does not change tab close behavior — cost if wrong: one small store to delete.
- Ruling: the fingerprint hashes headers, variables, the three scripts, docs and auth, in that order, as canonical JSON text. A missing `folder.yml` hashes the same as an empty `FolderSettings`.
- Ruling: `get_folder_settings` refuses `""` (the root) with a pointer to `get_collection_settings`, and checks that the folder exists in the summaries tree, so a typo is a clear `NotFound` and not an empty view.
- Ruling: the per-call caps are the existing `MAX_PAIRS_PER_CHANGE` (100) for headers and variables, `MAX_FIELD_BYTES` for keys and values, `MAX_TEXT_BYTES` for scripts and docs. No new constants.

## Locked contracts

```rust
// rocket_acp::proposal
pub struct ProposedVariable { pub key: String, pub value: String, pub enabled: bool }
pub struct FolderSettingsPatch {
    pub headers: Option<Vec<Header>>,
    pub variables: Option<Vec<ProposedVariable>>,
    pub pre_request_script: Option<String>,
    pub post_response_script: Option<String>,
    pub tests_script: Option<String>,
    pub docs: Option<String>,
}
impl FolderSettingsPatch { pub fn changed_fields(&self) -> Vec<&'static str>; pub fn is_empty(&self) -> bool }
ProposedChange::UpdateFolderSettings { collection: String, folder_path: String, patch: FolderSettingsPatch, base_fingerprint: String }

// rocket_app::mcp_read_views
pub struct FolderChildView { pub name: String, pub kind: &'static str }   // kind: "folder" | "request"
pub struct FolderView { pub collection: String, pub path: String, pub settings: MaskedFolderSettings, pub children: Vec<FolderChildView> }
// McpToolService
pub fn get_folder_settings(&self, session_id: &str, collection: &str, folder_path: &str) -> DomainResult<FolderView>
```

```text
// MCP tool: get_folder_settings { collection, folder_path }
// propose_changes op: "update_folder_settings" { collection, folder_path, patch: { headers?, variables?, pre_request_script?, post_response_script?, tests_script?, docs? } }  (deny_unknown_fields, so "auth" is refused)
// IPC DTO tag: "updateFolderSettings" { collection, folderPath, patch: { headers?, variables?, preRequestScript?, postResponseScript?, testsScript?, docs? } }
```

- Ruling: the IPC DTO moves to Plan 01 T1 because the exhaustive `From<ProposedChange>` match in `src-tauri` would not compile without it; Plan 01 T1 also adds temporary refusing arms in `ProposalService` that T3 replaces.
