# Assistant Folder Tools Implementation Plan 02: Tool Server and Frontend

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The agent can call `get_folder_settings` and propose `update_folder_settings`, and the user sees, accepts or rejects that proposal in the assistant panel.

**Architecture:** The MCP tool server (`src-tauri/src/mcp/tool_server.rs`) wires the read tool and a new `propose_changes` op onto Plan 01's services. The frontend adds the TS type, a diff against the stored folder settings, and a small zustand store that tells the Folder Settings tab to reload after an accept and tells the card when that tab has unsaved edits.

**Tech Stack:** Rust (rmcp tool server, schemars), React + TypeScript, zustand, Vitest.

**Spec:** [../../specs/2026-10-11-assistant-folder-tools-design.md](../../specs/2026-10-11-assistant-folder-tools-design.md). Index and rulings: [00-index.md](00-index.md). Plan 01 must be complete.

## Global Constraints

- The `propose_changes` schema has no `auth` field, and `deny_unknown_fields` makes the server refuse one instead of dropping it.
- Caps reuse the existing constants: `MAX_PAIRS_PER_CHANGE` (100) for headers and variables, `MAX_FIELD_BYTES` for keys and values, `MAX_TEXT_BYTES` for scripts and docs, and the existing total-bytes cap.
- The tool list is the same in every mode and stays sorted by name. Only `propose_changes` checks the mode (Edit).
- Frontend: shadcn/ui primitives and lucide-react icons only. Zustand: narrow selectors, never destructure the store at the component top level. Biome style as in neighboring files.
- Comments are short full sentences ending with a period.
- Cargo: always `-j4`. Never `cargo test --workspace` or `--all`.
- Commit through the `dev-workflow-skills:1-git-commit` skill with explicit paths.

## Review Focus

- An agent that sends `auth` in the folder patch gets a parse error, not a silent drop (T1).
- Oversized header and variable lists, keys and values, and script bodies are refused before any filesystem read (T1).
- Accept is blocked while the folder's Folder Settings tab has unsaved edits, and nothing is written (T3).
- A clean open Folder Settings tab shows the accepted values without a reopen, and a dirty one keeps its edits (T3).
- The card shows a before and after of only the changed sections, with no fingerprint (T2).

---

### Task 1: Tool server: read tool, propose op, caps

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src-tauri/src/mcp/tool_server.rs`
- Modify: `crates/rocket-app/src/mcp_tool_service.rs` (`WORKSPACE_ASSISTANT_INSTRUCTIONS` only)

**Interfaces:**
- Consumes: `McpToolService::get_folder_settings` (plan 01 T2), `ProposedChange::UpdateFolderSettings`, `FolderSettingsPatch`, `ProposedVariable` (plan 01 T1), existing `to_headers`, `check_len`, `check_opt`, `check_pairs`, `to_tool_result`.
- Produces: tool `get_folder_settings`, `propose_changes` op `update_folder_settings`, params `FolderParams`, `FolderSettingsPatchParams`, `VariableParams`.

- [ ] **Step 1: Write the failing tests** in `mod tests` of `tool_server.rs`.

Update `the_tool_list_is_the_same_in_every_mode`: insert `"get_folder_settings",` between `"get_environment",` and `"get_history",`.

Add:

```rust
    #[test]
    fn a_folder_patch_with_an_auth_field_is_refused() {
        let parsed = serde_json::from_value::<ProposeChangesParams>(serde_json::json!({
            "changes": [{
                "op": "update_folder_settings", "collection": "demo", "folder_path": "auth",
                "patch": { "docs": "x", "auth": { "authType": "bearer", "token": "t" } }
            }]
        }));
        assert!(parsed.is_err(), "auth must not be dropped silently");
    }

    #[test]
    fn folder_patch_params_convert_to_the_domain_change() {
        let change = to_domain_change(parse_one(serde_json::json!({
            "op": "update_folder_settings", "collection": "demo", "folder_path": "auth",
            "patch": {
                "headers": [{ "key": "X-Key", "value": "v" }],
                "variables": [{ "key": "HOST", "value": "h", "enabled": false }],
                "pre_request_script": "a();",
                "tests_script": ""
            }
        })))
        .expect("convert");
        let ProposedChange::UpdateFolderSettings {
            collection,
            folder_path,
            patch,
            base_fingerprint,
        } = change
        else {
            panic!("expected UpdateFolderSettings");
        };
        assert_eq!(collection, "demo");
        assert_eq!(folder_path, "auth");
        assert!(base_fingerprint.is_empty(), "the service fills the fingerprint");
        assert_eq!(patch.headers.expect("headers")[0].key, "X-Key");
        let variables = patch.variables.expect("variables");
        assert_eq!(variables[0].key, "HOST");
        assert!(!variables[0].enabled);
        assert_eq!(patch.pre_request_script.as_deref(), Some("a();"));
        assert_eq!(patch.tests_script.as_deref(), Some(""));
        assert_eq!(patch.docs, None);
    }

    #[test]
    fn oversized_folder_patch_fields_are_refused() {
        let too_many: Vec<serde_json::Value> = (0..=MAX_PAIRS_PER_CHANGE)
            .map(|i| serde_json::json!({ "key": format!("K{i}"), "value": "v" }))
            .collect();
        let cases = [
            serde_json::json!({ "headers": too_many }),
            serde_json::json!({ "variables": too_many }),
            serde_json::json!({ "pre_request_script": "x".repeat(MAX_TEXT_BYTES + 1) }),
            serde_json::json!({ "docs": "x".repeat(MAX_TEXT_BYTES + 1) }),
            serde_json::json!({ "variables": [{ "key": "k", "value": "x".repeat(MAX_FIELD_BYTES + 1) }] }),
        ];
        for patch in cases {
            let change = parse_one(serde_json::json!({
                "op": "update_folder_settings", "collection": "demo",
                "folder_path": "auth", "patch": patch
            }));
            assert!(check_change_sizes(&change).is_err());
        }
    }

    #[tokio::test]
    async fn a_folder_proposal_is_refused_in_ask_mode() {
        let fixture = TestFixture::new(true);
        let params: ProposeChangesParams = serde_json::from_value(serde_json::json!({
            "changes": [{
                "op": "update_folder_settings", "collection": "demo",
                "folder_path": "auth", "patch": { "docs": "x" }
            }]
        }))
        .expect("parse");
        let result = fixture
            .server_in(AssistantMode::Ask)
            .propose_changes(Parameters(params))
            .await
            .expect("tool call");
        assert!(tool_is_error(&result));
        assert!(tool_text(&result).contains("Not available in Ask mode"));
    }

    #[tokio::test]
    async fn get_folder_settings_answers_in_ask_mode_and_refuses_the_root() {
        let fixture = TestFixture::new(true);
        let server = fixture.server_in(AssistantMode::Ask);
        let root = server
            .get_folder_settings(Parameters(FolderParams {
                collection: "demo".into(),
                folder_path: String::new(),
            }))
            .await
            .expect("tool call");
        assert!(tool_is_error(&root));
        assert!(tool_text(&root).contains("get_collection_settings"));
    }
```

If the fixture's collection has no folder, the last test only pins the root refusal and that the tool is callable in Ask mode; that is intended.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p <src-tauri package> -j4 folder`
Expected: compile errors (`FolderParams`, the op and the tool do not exist).

- [ ] **Step 3: Implement.**

Params, next to `CollectionParams`:

```rust
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FolderParams {
    pub collection: String,
    /// The folder path inside the collection, for example "auth/v2". Not "".
    pub folder_path: String,
}
```

Patch params, next to `RequestPatchParams`:

```rust
#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VariableParams {
    pub key: String,
    pub value: String,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
}

/// Sections to change in a folder. A given section replaces the whole section,
/// an omitted one stays as it is, and an empty script string clears the
/// script. Unknown fields, such as `auth`, are refused rather than dropped.
#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FolderSettingsPatchParams {
    #[serde(default)]
    pub headers: Option<Vec<KeyValueParams>>,
    #[serde(default)]
    pub variables: Option<Vec<VariableParams>>,
    #[serde(default)]
    pub pre_request_script: Option<String>,
    #[serde(default)]
    pub post_response_script: Option<String>,
    #[serde(default)]
    pub tests_script: Option<String>,
    #[serde(default)]
    pub docs: Option<String>,
}
```

Variant in `ProposedChangeParams` (after `SetEnvVar`):

```rust
    /// Change some sections of a folder's own settings (default headers,
    /// variables, scripts, docs). Folder auth cannot be changed.
    UpdateFolderSettings {
        collection: String,
        folder_path: String,
        patch: FolderSettingsPatchParams,
    },
```

Arm in `check_change_sizes`:

```rust
        ProposedChangeParams::UpdateFolderSettings {
            collection,
            folder_path,
            patch,
        } => {
            check_len("collection", collection, MAX_FIELD_BYTES)?;
            check_len("folder_path", folder_path, MAX_FIELD_BYTES)?;
            if let Some(headers) = &patch.headers {
                check_pairs("header", headers)?;
            }
            if let Some(variables) = &patch.variables {
                if variables.len() > MAX_PAIRS_PER_CHANGE {
                    return Err(format!(
                        "too many variable entries: {}, at most {MAX_PAIRS_PER_CHANGE} are allowed",
                        variables.len()
                    ));
                }
                for variable in variables {
                    check_len("variable key", &variable.key, MAX_FIELD_BYTES)?;
                    check_len("variable value", &variable.value, MAX_FIELD_BYTES)?;
                }
            }
            check_opt("pre_request_script", &patch.pre_request_script, MAX_TEXT_BYTES)?;
            check_opt(
                "post_response_script",
                &patch.post_response_script,
                MAX_TEXT_BYTES,
            )?;
            check_opt("tests_script", &patch.tests_script, MAX_TEXT_BYTES)?;
            check_opt("docs", &patch.docs, MAX_TEXT_BYTES)
        }
```

Arm in `to_domain_change`:

```rust
        ProposedChangeParams::UpdateFolderSettings {
            collection,
            folder_path,
            patch,
        } => ProposedChange::UpdateFolderSettings {
            collection,
            folder_path,
            patch: FolderSettingsPatch {
                headers: patch.headers.map(to_headers),
                variables: patch.variables.map(|list| {
                    list.into_iter()
                        .map(|v| ProposedVariable {
                            key: v.key,
                            value: v.value,
                            enabled: v.enabled,
                        })
                        .collect()
                }),
                pre_request_script: patch.pre_request_script,
                post_response_script: patch.post_response_script,
                tests_script: patch.tests_script,
                docs: patch.docs,
            },
            base_fingerprint: String::new(),
        },
```

Import `FolderSettingsPatch, ProposedVariable` next to the other `rocket_acp::proposal` imports.

The read tool, after `get_collection_settings`:

```rust
    #[tool(
        description = "Read one folder's own settings: auth type (credentials masked), default headers, variables, the three scripts and docs, plus the names of its direct subfolders and requests. Pass the folder path inside the collection, for example \"auth/v2\". Folder settings apply to every request below the folder."
    )]
    async fn get_folder_settings(
        &self,
        Parameters(params): Parameters<FolderParams>,
    ) -> Result<CallToolResult, McpError> {
        let svc = mcp_tool_service(&self.app_handle)?;
        let result = svc.get_folder_settings(
            self.binding.session_id(),
            &params.collection,
            &params.folder_path,
        );
        Ok(to_tool_result(result))
    }
```

Update the `propose_changes` description to list `update_folder_settings` (folder headers, variables, scripts and docs; never auth). Update `WORKSPACE_ASSISTANT_INSTRUCTIONS` with one sentence: "Folder settings (default headers, variables, scripts, docs and auth) apply to every request below a folder, so read them with get_folder_settings before advising on how a request behaves." Keep the constant a single string literal.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p <src-tauri package> -j4 folder` and `cargo test -p <src-tauri package> -j4 the_tool_list`
Expected: PASS.

- [ ] **Step 5: Checks.** `cargo check --workspace --all-targets -j4` and `cargo clippy -p <src-tauri package> -j4 -- -D warnings`. Run `cargo test -p rocket-app -j4 instructions` if a test pins the instructions text and fix its expectation only if it pins this sentence.

- [ ] **Step 6: Commit** via the commit skill. Paths: `src-tauri/src/mcp/tool_server.rs`, `crates/rocket-app/src/mcp_tool_service.rs`. Message: `feat(assistant): add folder tools to the MCP server`.

---

### Task 2: TS types, preview, diff and card

**Files:**
- Modify: `src/lib/tauri-api.ts`
- Modify: `src/lib/assistant/proposal-view.ts`
- Modify: `src/lib/assistant/proposal-actions.ts` (only `affectedAmong`)
- Modify: `src/lib/assistant/__tests__/proposal-view.test.ts`
- Modify: `src/components/assistant/__tests__/AssistantProposalCard.test.tsx`

**Interfaces:**
- Consumes: the DTO shape from plan 01 T1: `{ op: 'updateFolderSettings', collection, folderPath, patch: { headers?, variables?: {key,value,enabled}[], preRequestScript?, postResponseScript?, testsScript?, docs? } }`; `getFolderSettings(collection, folderPath): Promise<FolderSettings>`.
- Produces: the `AgentProposedChange` variant, `proposalTarget`/`proposalPreview`/`loadProposalDiff` cases.

- [ ] **Step 1: Write the failing tests.** In `proposal-view.test.ts` extend the `vi.mock('@/lib/tauri-api', ...)` factory with `getFolderSettings: vi.fn(),` and add:

```ts
  describe('folder settings update', () => {
    const change = {
      op: 'updateFolderSettings' as const,
      collection: 'orders',
      folderPath: 'auth/v2',
      patch: {
        preRequestScript: 'new();',
        variables: [{ key: 'HOST', value: 'api.test', enabled: true }],
      },
    };

    it('targets the folder and previews a diff', () => {
      expect(proposalTarget(change)).toEqual({ collection: 'orders', path: 'auth/v2' });
      expect(proposalPreview(change)).toEqual({ kind: 'diff' });
    });

    it('diffs only the changed sections against the stored folder', async () => {
      vi.mocked(api.getFolderSettings).mockResolvedValue({
        headers: [{ key: 'X-Old', value: 'old', enabled: true }],
        variables: [
          { key: 'HOST', value: 'old.test', initialValue: '', enabled: true, secret: false },
        ],
        preRequestScript: 'old();',
        docs: 'untouched',
      });
      const diff = await loadProposalDiff(makeProposal({ change }));
      expect(api.getFolderSettings).toHaveBeenCalledWith('orders', 'auth/v2');
      expect(diff?.language).toBe('json');
      const before = JSON.parse(diff?.before ?? '{}');
      const after = JSON.parse(diff?.after ?? '{}');
      expect(Object.keys(after).sort()).toEqual(['preRequestScript', 'variables']);
      expect(before.preRequestScript).toBe('old();');
      expect(before.variables).toEqual([{ key: 'HOST', value: 'old.test', enabled: true }]);
      expect(after.variables).toEqual([{ key: 'HOST', value: 'api.test', enabled: true }]);
      expect(diff?.before).not.toContain('untouched');
    });
  });
```

In `AssistantProposalCard.test.tsx`, copy the existing "expands the diff of an editScript" test (search the file for `proposal-diff`) and adapt it to a folder proposal: mock `api.getFolderSettings` (add it to the file's `vi.mock` factory), render `makeProposal({ change: <the change above>, summary: 'Update pre-request script, variables of folder \'auth/v2\' in orders' })`, expand, and expect `data-language='json'` and that `data-modified` contains `new();`.

- [ ] **Step 2: Run to verify it fails**

Run: `yarn vitest run src/lib/assistant/__tests__/proposal-view.test.ts src/components/assistant/__tests__/AssistantProposalCard.test.tsx`
Expected: FAIL (type or switch errors, no diff).

- [ ] **Step 3: Implement.**

`tauri-api.ts`, next to the other change types:

```ts
export interface AgentFolderSettingsPatch {
  headers?: Header[];
  variables?: { key: string; value: string; enabled: boolean }[];
  preRequestScript?: string;
  postResponseScript?: string;
  testsScript?: string;
  docs?: string;
}
```

and add to `AgentProposedChange`:

```ts
  | {
      op: 'updateFolderSettings';
      collection: string;
      folderPath: string;
      patch: AgentFolderSettingsPatch;
    }
```

`proposal-view.ts`: import `getFolderSettings`. In `proposalTarget` add `case 'updateFolderSettings': return { collection: change.collection, path: change.folderPath };`. In `proposalPreview` add `case 'updateFolderSettings': return { kind: 'diff' };` (next to `updateRequest`). In `loadProposalDiff`, before the request branch:

```ts
  if (change.op === 'updateFolderSettings') {
    const current = await getFolderSettings(change.collection, change.folderPath);
    const patch = asRecord(change.patch);
    const keys = Object.keys(patch).filter((key) => patch[key] !== undefined);
    // Variables are shown as key, value and enabled only, like the proposal.
    const before = asRecord(current);
    before.variables = current.variables.map(({ key, value, enabled }) => ({
      key,
      value,
      enabled,
    }));
    return {
      before: stringifyPicked(before, keys),
      after: stringifyPicked(patch, keys),
      language: 'json',
    };
  }
```

(`asRecord(current)` returns the same object, so copy first: `const before = { ...asRecord(current) };`.) Adjust the early `if (change.op !== 'updateRequest' && change.op !== 'editScript') return null;` so the folder branch runs first.

`proposal-actions.ts` `affectedAmong`: add `if (change.op === 'updateFolderSettings') return [];` next to the `setEnvVar` line, with the comment `// A folder settings change rewrites no request tab.`

- [ ] **Step 4: Run to verify it passes**

Run: `yarn vitest run src/lib/assistant/__tests__/proposal-view.test.ts src/components/assistant/__tests__/AssistantProposalCard.test.tsx`
Expected: PASS.

- [ ] **Step 5: Checks.** `yarn tsc --noEmit` and `yarn check`.

- [ ] **Step 6: Commit** via the commit skill. Paths: the five files above. Message: `feat(assistant): preview folder settings proposals`.

---

### Task 3: Dirty block, reload after accept, docs

**Files:**
- Create: `src/stores/folder-edit-store.ts`
- Create: `src/stores/__tests__/folder-edit-store.test.ts`
- Modify: `src/hooks/useFolderSettings.ts`
- Modify: `src/hooks/__tests__/useFolderSettings.test.ts`
- Modify: `src/lib/assistant/proposal-actions.ts`
- Modify: `src/components/assistant/AssistantProposalCard.tsx`
- Modify: `src/components/assistant/__tests__/AssistantProposalCard.test.tsx`
- Modify: `docs/features/folder-settings.md`, `docs/features/assistant-followups.md`, `docs/features/assistant-composer.md` (only if it lists the tools)

**Interfaces:**
- Consumes: T2's `updateFolderSettings` change type; `useFolderSettings(collection, folderPath)` internals (`dirtyRef`, `loadedRef`, `baselineRef`, `settingsRef`, `keyRef`).
- Produces: `useFolderEditStore` with `dirty: Record<string, true>`, `reloads: Record<string, number>`, `setDirty(key, dirty)`, `requestReload(key)`; `folderEditKey(collection, folderPath)`.

- [ ] **Step 1: Write the failing tests.**

`src/stores/__tests__/folder-edit-store.test.ts`:

```ts
import { beforeEach, describe, expect, it } from 'vitest';
import { folderEditKey, useFolderEditStore } from '../folder-edit-store';

describe('folder-edit-store', () => {
  beforeEach(() => {
    useFolderEditStore.setState({ dirty: {}, reloads: {} });
  });

  it('keys a folder by collection and path', () => {
    expect(folderEditKey('c', 'a/b')).not.toBe(folderEditKey('c', 'a'));
    expect(folderEditKey('c', 'a/b')).toBe(folderEditKey('c', 'a/b'));
  });

  it('marks and clears a dirty folder', () => {
    const key = folderEditKey('c', 'a');
    useFolderEditStore.getState().setDirty(key, true);
    expect(useFolderEditStore.getState().dirty[key]).toBe(true);
    useFolderEditStore.getState().setDirty(key, false);
    expect(useFolderEditStore.getState().dirty[key]).toBeUndefined();
  });

  it('counts reload requests per folder', () => {
    const key = folderEditKey('c', 'a');
    useFolderEditStore.getState().requestReload(key);
    useFolderEditStore.getState().requestReload(key);
    expect(useFolderEditStore.getState().reloads[key]).toBe(2);
    expect(useFolderEditStore.getState().reloads[folderEditKey('c', 'b')]).toBeUndefined();
  });
});
```

In `useFolderSettings.test.ts` add (the file's `mockGet`/`load` helpers and `waitFor` exist; add `import { folderEditKey, useFolderEditStore } from '@/stores/folder-edit-store';` and reset the store in `beforeEach` with `useFolderEditStore.setState({ dirty: {}, reloads: {} })`):

```ts
  it('reports its dirty flag to the folder edit store and clears it on unmount', async () => {
    const { result, unmount } = load('col', 'a/b');
    await waitFor(() => expect(result.current.isLoaded).toBe(true));
    const key = folderEditKey('col', 'a/b');
    expect(useFolderEditStore.getState().dirty[key]).toBeUndefined();
    act(() => result.current.setSettings((s) => ({ ...s, docs: 'edited' })));
    expect(useFolderEditStore.getState().dirty[key]).toBe(true);
    unmount();
    expect(useFolderEditStore.getState().dirty[key]).toBeUndefined();
  });

  it('reloads a clean tab when a reload is requested', async () => {
    const { result } = load('col', 'a/b');
    await waitFor(() => expect(result.current.isLoaded).toBe(true));
    mockGet.mockResolvedValue({ ...base, docs: 'from the assistant' });
    act(() => useFolderEditStore.getState().requestReload(folderEditKey('col', 'a/b')));
    await waitFor(() => expect(result.current.settings.docs).toBe('from the assistant'));
    expect(result.current.isDirty).toBe(false);
  });

  it('keeps the edits of a dirty tab when a reload is requested', async () => {
    const { result } = load('col', 'a/b');
    await waitFor(() => expect(result.current.isLoaded).toBe(true));
    act(() => result.current.setSettings((s) => ({ ...s, docs: 'my edit' })));
    mockGet.mockResolvedValue({ ...base, docs: 'from the assistant' });
    act(() => useFolderEditStore.getState().requestReload(folderEditKey('col', 'a/b')));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(result.current.settings.docs).toBe('my edit');
    expect(result.current.isDirty).toBe(true);
  });
```

In `AssistantProposalCard.test.tsx` add two tests, using the folder change from T2:

```ts
  it('blocks Accept while the folder has unsaved edits in its settings tab', () => {
    useFolderEditStore.getState().setDirty(folderEditKey('orders', 'auth/v2'), true);
    render(<AssistantProposalCard proposal={makeProposal({ change: folderChange })} />);
    expect(screen.getByRole('button', { name: /accept/i })).toBeDisabled();
    expect(screen.getByText(/unsaved edits in its Folder Settings tab/i)).toBeInTheDocument();
  });

  it('asks a clean folder tab to reload after an accepted folder update', async () => {
    const proposal = makeProposal({ change: folderChange });
    vi.mocked(api.acceptAgentProposal).mockResolvedValue({ ...proposal, status: 'accepted' });
    render(<AssistantProposalCard proposal={proposal} />);
    await userEvent.click(screen.getByRole('button', { name: /accept/i }));
    expect(useFolderEditStore.getState().reloads[folderEditKey('orders', 'auth/v2')]).toBe(1);
  });
```

where `folderChange` is the `updateFolderSettings` change from T2, defined at the top of the test file, and `beforeEach` resets `useFolderEditStore.setState({ dirty: {}, reloads: {} })`. If the existing accept tests need extra mocks (for example `getQueryClient`), copy what the neighboring accept test does.

- [ ] **Step 2: Run to verify it fails**

Run: `yarn vitest run src/stores/__tests__/folder-edit-store.test.ts src/hooks/__tests__/useFolderSettings.test.ts src/components/assistant/__tests__/AssistantProposalCard.test.tsx`
Expected: FAIL (no store module).

- [ ] **Step 3: Implement.**

`src/stores/folder-edit-store.ts`:

```ts
import { create } from 'zustand';

/** One key per folder of a collection. */
export function folderEditKey(collection: string, folderPath: string): string {
  return JSON.stringify([collection, folderPath]);
}

interface FolderEditState {
  /** Folders whose open Folder Settings tab has unsaved edits. */
  dirty: Record<string, true>;
  /** How many times a reload was requested, per folder. */
  reloads: Record<string, number>;
  setDirty: (key: string, dirty: boolean) => void;
  requestReload: (key: string) => void;
}

/**
 * Bridges the Folder Settings tab and the assistant panel. A folder tab keeps
 * its edits in hook state, so this store carries only its dirty flag and the
 * reload requests that an accepted assistant proposal raises.
 */
export const useFolderEditStore = create<FolderEditState>()((set) => ({
  dirty: {},
  reloads: {},
  setDirty: (key, dirty) =>
    set((state) => {
      if (dirty === (state.dirty[key] === true)) return state;
      const next = { ...state.dirty };
      if (dirty) next[key] = true;
      else delete next[key];
      return { dirty: next };
    }),
  requestReload: (key) =>
    set((state) => ({ reloads: { ...state.reloads, [key]: (state.reloads[key] ?? 0) + 1 } })),
}));
```

`useFolderSettings.ts`: import `useFolderEditStore`. After the `keyRef` setup add:

```ts
  // Tell the assistant panel whether this folder has unsaved edits.
  useEffect(() => {
    useFolderEditStore.getState().setDirty(key, isDirty);
  }, [key, isDirty]);
  useEffect(
    () => () => {
      useFolderEditStore.getState().setDirty(key, false);
    },
    [key],
  );

  // An accepted assistant proposal asks open clean tabs to read the file again.
  const reloadNonce = useFolderEditStore((s) => s.reloads[key] ?? 0);
  useEffect(() => {
    if (reloadNonce === 0 || !loadedRef.current || dirtyRef.current) return;
    const reloadKey = keyRef.current;
    getFolderSettings(collectionRef.current, folderPathRef.current)
      .then((loaded) => {
        // Drop the answer when the tab moved on or the user started editing.
        if (!mountedRef.current || keyRef.current !== reloadKey || dirtyRef.current) return;
        settingsRef.current = loaded;
        baselineRef.current = loaded;
        setSettingsState(loaded);
      })
      .catch((err) => console.error('[useFolderSettings] reload failed', err));
  }, [reloadNonce]);
```

`proposal-actions.ts`: import `folderEditKey, useFolderEditStore` from `@/stores/folder-edit-store`. In `syncTabsAfterAccept`'s switch add before `default`:

```ts
      case 'updateFolderSettings':
        // An open clean Folder Settings tab reads the file again. A dirty one
        // keeps its edits, and the card blocked Accept before that happened.
        useFolderEditStore
          .getState()
          .requestReload(folderEditKey(change.collection, change.folderPath));
        return undefined;
```

`AssistantProposalCard.tsx`: import the store. After `dirtyPlace`:

```ts
  const folderTabDirty = useFolderEditStore((s) =>
    proposal.status === 'pending' && proposal.change.op === 'updateFolderSettings'
      ? s.dirty[folderEditKey(proposal.change.collection, proposal.change.folderPath)] === true
      : false,
  );
  const blockedByEdits = dirtyPlace !== undefined || folderTabDirty;
```

(replace the existing `blockedByEdits` line). In the blocked-note paragraph add a first branch: `folderTabDirty ? 'This folder has unsaved edits in its Folder Settings tab. Save or discard them before accepting.' : <existing expression>`.

- [ ] **Step 4: Run to verify it passes**

Run: `yarn vitest run src/stores/__tests__/folder-edit-store.test.ts src/hooks/__tests__/useFolderSettings.test.ts src/components/assistant/__tests__/AssistantProposalCard.test.tsx src/lib/assistant`
Expected: PASS.

- [ ] **Step 5: Checks.** `yarn tsc --noEmit` and `yarn check`.

- [ ] **Step 6: Docs.**
  - `docs/features/folder-settings.md`: add a section "Assistant edits" under "Rules that are easy to break": the assistant proposes `UpdateFolderSettings` (no auth), the save is read-then-patch through `save_folder_settings`, `folder-edit-store` carries the tab's dirty flag and reload requests, only a mounted tab takes part (an unmounted tab already drops its edits), and a header replaced by a proposal loses its description.
  - `docs/features/assistant-followups.md`: add under open items: folder auth edits (needs a security decision), headers lose their description, a patch replaces whole sections (no per-item merge), gRPC requests do not inherit folder settings, folder tab close still does not warn.
  - `docs/features/assistant-composer.md`: add `get_folder_settings` only if the file lists the read tools.

- [ ] **Step 7: Commit** via the commit skill. Paths: the files above that changed. Message: `feat(assistant): block and refresh folder tabs on accept`.
