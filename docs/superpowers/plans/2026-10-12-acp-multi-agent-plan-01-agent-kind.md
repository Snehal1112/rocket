# ACP Multi-Agent Support, Plan 1: AgentKind Plumbing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an `AgentKind` to every agent config and route each assistant session through one pure `session_plan` function, with no change to how Claude sessions behave today.

**Architecture:** `AgentKind` (`Claude`, `Generic`) lives in `rocket-acp` and persists on `AgentConfig`. A pure `session_plan(kind, ...)` in `rocket-app` returns the per-session args, env, `_meta`, files and a first-prompt prefix. `AcpSessionService::start_workspace_session` consumes the plan instead of `SessionIsolation`. `src-tauri` writes the planned files into the scratch directory before the agent spawns. Plan 2 adds the Gemini arm.

**Tech Stack:** Rust (rocket-acp, rocket-app, rocket-infra, src-tauri), React + TypeScript, Vitest, Biome.

**Spec:** `.claude/acp-multi-agent-support.md`. Index: `docs/superpowers/plans/2026-10-12-acp-multi-agent-index.md`.

## Global Constraints

- Run cargo with `-j4`. Never `cargo test --workspace` or `--all`. Use `cargo test -p <crate> -j4 <filter>`.
- Never `unwrap()` in production paths. `expect("...")` is fine in tests.
- `#[serde(rename_all = "camelCase")]` only on IPC DTOs, never on `AgentConfig` (it persists as snake_case in `agent_configs.yml`).
- Persistence stays backward compatible: every new `AgentConfig` field has `#[serde(default)]`.
- UI uses shadcn/ui primitives only (`RadioGroup`, `Label`, `Input`), lucide-react icons only, no raw `<button>`/`<input>`.
- Never `git add -A`, `git add .` or `--all`. Add explicit paths. Commit through the `dev-workflow-skills:1-git-commit` skill (conventional commit format, for example `feat(acp): ...`). The harness runs `yarn tsc --noEmit` and `yarn check` for staged TS and `cargo check -j4` for staged Rust before a commit.
- Claude behavior must not change: the existing `agent_isolation.rs` tests and the Claude assertions in the integration test stay green without edits to their expectations.
- No `OpenCollection` read needed: this plan does not touch collection or environment YAML.

## Review Focus

- An old `agent_configs.yml` with no `kind` key must load as Claude and keep Claude's `_meta` and `CLAUDE_CONFIG_DIR` (Task 1 and Task 3 tests).
- A Generic agent must never receive Claude's `_meta` or `CLAUDE_CONFIG_DIR` (Task 2 test).
- A planned file path with `..`, an absolute path or an empty path must be refused, and no file may be written outside the scratch root (Task 3 tests).
- A failed session start after files were written must still remove the whole scratch root (Task 3 keeps the existing cleanup test green).
- The Rocket instructions must reach a Generic agent exactly once, in the first prompt, and a failed first prompt must keep them for the next one (Task 2 test).

---

### Task 1: AgentKind on the config, the IPC DTO and the dialog

**Files:**
- Modify: `crates/rocket-acp/src/agent_config.rs` (struct at line 47-72, tests module from line 176)
- Modify: `crates/rocket-acp/src/lib.rs` (the `pub use agent_config::{...}` block at line 9-12)
- Modify: `src-tauri/src/commands/agent_configs.rs` (DTO at line 24-47, both `From` impls, tests from line 150)
- Modify (compiler-guided, add `kind: AgentKind::default(),`): every `AgentConfig { ... }` literal: `crates/rocket-infra/src/fs_agent_config_repo.rs:87`, `crates/rocket-app/src/agent_config_service.rs:395`, `:499`, `:509`, `crates/rocket-app/src/acp_session_service.rs:740`, `src-tauri/tests/acp_mcp_start_workspace_assistant.rs:140`, `src-tauri/src/commands/agent_configs.rs:164`
- Modify: `src/lib/tauri-api.ts` (the `AgentConfig` interface at line 446)
- Modify: `src/components/settings/AgentConfigsDialog.tsx`
- Test: `crates/rocket-acp/src/agent_config.rs`, `src-tauri/src/commands/agent_configs.rs`, `src/components/settings/__tests__/AgentConfigsDialog.test.tsx`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces:
  - `rocket_acp::AgentKind { Claude, Generic }` (`Default` = `Claude`, serde lowercase `"claude"` / `"generic"`, `Copy`).
  - `AgentConfig.kind: AgentKind` with `#[serde(default)]`.
  - `AgentConfigDto.kind: AgentKind` with `#[serde(default)]`.
  - TypeScript `type AgentKind = 'claude' | 'generic'` and `AgentConfig.kind?: AgentKind` (a missing value means `'claude'`).

- [ ] **Step 1: Write the failing Rust tests in `crates/rocket-acp/src/agent_config.rs`**

Add inside the `tests` module (after `a_config_without_the_new_fields_deserializes_as_vault`):

```rust
    #[test]
    fn agent_kind_defaults_to_claude_and_uses_lowercase_wire_names() {
        assert_eq!(AgentKind::default(), AgentKind::Claude);
        assert_eq!(
            serde_json::to_string(&AgentKind::Generic).expect("json"),
            "\"generic\""
        );
        assert_eq!(
            serde_json::from_str::<AgentKind>("\"claude\"").expect("parse"),
            AgentKind::Claude
        );
    }

    #[test]
    fn a_config_without_kind_loads_as_claude() {
        let json = serde_json::json!({
            "id": "a", "label": "A", "command": "c", "credential_env_var": "ANTHROPIC_API_KEY",
            "vault_connection_id": "v", "vault_name": "n", "vault_secret_id": "s",
            "vault_secret_name": "sn"
        });
        let config: AgentConfig = serde_json::from_value(json).expect("old shape");
        assert_eq!(config.kind, AgentKind::Claude);
    }

    #[test]
    fn kind_roundtrips_in_snake_case_config_json() {
        let mut c = sample();
        c.kind = AgentKind::Generic;
        let json = serde_json::to_string(&c).expect("serialize AgentConfig");
        assert!(json.contains("\"kind\":\"generic\""), "got: {json}");
        let back: AgentConfig = serde_json::from_str(&json).expect("deserialize AgentConfig");
        assert_eq!(back.kind, AgentKind::Generic);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rocket-acp -j4 agent_kind`
Expected: FAIL to compile with "cannot find type `AgentKind`".

- [ ] **Step 3: Implement the enum and the field**

In `crates/rocket-acp/src/agent_config.rs`, add above the `AgentConfig` struct:

```rust
/// Which ACP agent a config runs. It decides how Rocket isolates and
/// instructs the assistant session. Old files without it load as `Claude`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    /// Claude Code through its ACP adapter. Isolation travels in `_meta`.
    #[default]
    Claude,
    /// Any other ACP agent. Rocket sets no isolation: the agent keeps its own
    /// tools and settings, and gets the Rocket instructions in its first prompt.
    Generic,
}
```

Add the field to `AgentConfig`, directly after `label`:

```rust
    /// The agent family. Old files without this field load as `claude`.
    #[serde(default)]
    pub kind: AgentKind,
```

In `crates/rocket-acp/src/lib.rs` change the re-export to:

```rust
pub use agent_config::{
    validate_credential_source, validate_env_config, validate_env_var_name, AgentConfig,
    AgentConfigRepository, AgentKind, CredentialSource, EnvPair, MAX_ENV_VAR_NAME_LEN,
    RESERVED_ENV_VAR,
};
```

In the `sample()` helper of the same tests module add `kind: AgentKind::default(),` after `label`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p rocket-acp -j4 agent_config`
Expected: PASS (all tests in the `agent_config` module).

- [ ] **Step 5: Fix the other `AgentConfig` literals**

Run: `cargo check -j4 -p rocket-infra -p rocket-app -p rocket --tests`
Expected: one `E0063 missing field kind` per literal. In each file listed under **Files** add `kind: rocket_acp::AgentKind::default(),` (use the plain `AgentKind::default()` where `AgentKind` is already imported). Run the command again until it is clean.

- [ ] **Step 6: Write the failing DTO tests in `src-tauri/src/commands/agent_configs.rs`**

Add inside its `tests` module:

```rust
    #[test]
    fn the_dto_carries_kind_in_camel_case_json_and_defaults_to_claude() {
        let mut domain = sample_domain();
        domain.kind = rocket_acp::AgentKind::Generic;
        let dto = AgentConfigDto::from(domain);
        let json = serde_json::to_value(&dto).expect("json");
        assert_eq!(json["kind"], "generic");
        let back: AgentConfig = serde_json::from_value::<AgentConfigDto>(json)
            .expect("dto")
            .into();
        assert_eq!(back.kind, rocket_acp::AgentKind::Generic);

        let old = serde_json::json!({
            "id": "a", "label": "A", "command": "c", "args": [],
            "credentialEnvVar": "K", "vaultConnectionId": "", "vaultName": "",
            "vaultSecretId": "", "vaultSecretName": ""
        });
        let dto: AgentConfigDto = serde_json::from_value(old).expect("old caller");
        assert_eq!(AgentConfig::from(dto).kind, rocket_acp::AgentKind::Claude);
    }
```

- [ ] **Step 7: Run it to verify it fails**

Run: `cargo test -p rocket -j4 --lib the_dto_carries_kind`
Expected: FAIL to compile (`no field kind on AgentConfigDto`).

- [ ] **Step 8: Implement the DTO field**

In `AgentConfigDto` add after `label`:

```rust
    // Old callers may omit it.
    #[serde(default)]
    pub kind: rocket_acp::AgentKind,
```

In `impl From<AgentConfig> for AgentConfigDto` add `kind: c.kind,` after `label: c.label,`. In `impl From<AgentConfigDto> for AgentConfig` add `kind: dto.kind,` after `label: dto.label,`.

- [ ] **Step 9: Run the DTO tests**

Run: `cargo test -p rocket -j4 --lib agent_configs`
Expected: PASS.

- [ ] **Step 10: Write the failing dialog tests**

Append inside the `describe('AgentConfigsDialog', ...)` block of `src/components/settings/__tests__/AgentConfigsDialog.test.tsx`:

```tsx
  it('shows a stored Claude agent as Claude Code and saves no kind override', async () => {
    const user = userEvent.setup();
    renderDialog();
    await user.click(await screen.findByRole('button', { name: 'Edit agent' }));
    await screen.findByLabelText('Label');
    expect(screen.getByRole('radio', { name: /^claude code/i })).toBeChecked();
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
    expect(vi.mocked(api.saveAgentConfig).mock.calls[0][0].kind).toBeUndefined();
  });

  it('saves the chosen agent type', async () => {
    const user = userEvent.setup();
    renderDialog();
    await user.click(await screen.findByRole('button', { name: 'Edit agent' }));
    await screen.findByLabelText('Label');
    await user.click(screen.getByRole('radio', { name: /^generic/i }));
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
    expect(vi.mocked(api.saveAgentConfig).mock.calls[0][0]).toMatchObject({ kind: 'generic' });
  });

  it('opens a stored generic agent as generic', async () => {
    vi.mocked(api.listAgentConfigs).mockResolvedValue([{ ...claude, kind: 'generic' }]);
    const user = userEvent.setup();
    renderDialog();
    await user.click(await screen.findByRole('button', { name: 'Edit agent' }));
    await screen.findByLabelText('Label');
    expect(screen.getByRole('radio', { name: /^generic/i })).toBeChecked();
  });
```

- [ ] **Step 11: Run them to verify they fail**

Run: `yarn test AgentConfigsDialog`
Expected: FAIL (no radio named "Claude Code").

- [ ] **Step 12: Implement the TS type and the dialog control**

In `src/lib/tauri-api.ts` add above `export interface AgentConfig`:

```ts
// Which ACP agent a config runs. A missing value means 'claude'.
export type AgentKind = 'claude' | 'generic';
```

and inside `AgentConfig` add after `label: string;`:

```ts
  kind?: AgentKind;
```

In `src/components/settings/AgentConfigsDialog.tsx`:

1. Import `type AgentKind` from `@/lib/tauri-api` (next to `type AgentCredentialSource`).
2. In `emptyForm` add `kind: 'claude' as AgentKind,` after `label: '',`.
3. In `startEdit`'s `setEditing({...})` add `kind: c.kind ?? 'claude',` after `label: c.label,`.
4. In `handleSave`'s `config` object add `kind: editing.kind === 'claude' ? undefined : editing.kind,` after `label: editing.label.trim(),`.
5. In the form, directly after the Label `<div>` (the one with `id='ac-label'`), add:

```tsx
            <div>
              <Label className='text-sm'>Agent type</Label>
              <RadioGroup
                value={editing.kind}
                onValueChange={(value) => setEditing({ ...editing, kind: value as AgentKind })}
                className='mt-1 gap-1'
              >
                <div className='flex items-center gap-2'>
                  <RadioGroupItem value='claude' id='ac-kind-claude' />
                  <Label htmlFor='ac-kind-claude' className='text-sm font-normal'>
                    Claude Code
                  </Label>
                </div>
                <div className='flex items-center gap-2'>
                  <RadioGroupItem value='generic' id='ac-kind-generic' />
                  <Label htmlFor='ac-kind-generic' className='text-sm font-normal'>
                    Generic ACP agent
                  </Label>
                </div>
              </RadioGroup>
              {editing.kind === 'generic' && (
                <p className='mt-1 text-xs text-muted-foreground'>
                  Rocket sets no isolation for a generic agent. It keeps its own tools and
                  settings, so configure it to use only the Rocket tools.
                </p>
              )}
            </div>
```

- [ ] **Step 13: Run all checks for this task**

Run: `yarn test AgentConfigsDialog && yarn tsc --noEmit && yarn check && cargo test -p rocket-acp -j4 && cargo check -j4 -p rocket-infra -p rocket-app -p rocket --tests`
Expected: all PASS, no Biome or tsc errors.

- [ ] **Step 14: Commit**

```bash
git add crates/rocket-acp/src/agent_config.rs crates/rocket-acp/src/lib.rs \
  crates/rocket-infra/src/fs_agent_config_repo.rs crates/rocket-app/src/agent_config_service.rs \
  crates/rocket-app/src/acp_session_service.rs src-tauri/tests/acp_mcp_start_workspace_assistant.rs \
  src-tauri/src/commands/agent_configs.rs src/lib/tauri-api.ts \
  src/components/settings/AgentConfigsDialog.tsx \
  src/components/settings/__tests__/AgentConfigsDialog.test.tsx
```

Then invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(acp): add agent kind to agent configs`.

---

### Task 2: The pure session plan and the service that consumes it

**Files:**
- Create: `crates/rocket-app/src/session_plan.rs`
- Modify: `crates/rocket-app/src/lib.rs` (add `mod session_plan;` near `pub mod agent_isolation;` at line 5, and the `pub use` at line 64-66)
- Modify: `crates/rocket-app/src/acp_session_service.rs` (import at line 13, `start_workspace_session` at line 148-181, `session_env` at line 465-482, tests)
- Modify: `crates/rocket-app/src/mcp_tool_service.rs` (`PendingOutline` at line 127, `begin_assistant_session` at line 400, `peek_outline_preamble` at line 440, tests near line 2441)
- Test: the same files

**Interfaces:**
- Consumes: `rocket_acp::AgentKind`, `rocket_acp::AgentConfig` (Task 1); `SessionIsolation` and `isolation_meta` from `agent_isolation.rs` (unchanged).
- Produces:
  - `rocket_app::PlannedFile { rel_path: String, contents: String }` (`rel_path` is relative to the scratch root).
  - `rocket_app::SessionPlan { args: Vec<String>, env: Vec<(String, String)>, meta: Option<serde_json::Value>, files: Vec<PlannedFile>, first_prompt_prefix: Option<String> }`.
  - `rocket_app::PlanInput<'a> { kind: AgentKind, config_args: &'a [String], cwd: &'a str, config_dir: &'a str, instructions: &'a str }`.
  - `rocket_app::session_plan(&PlanInput) -> SessionPlan`.
  - `AcpSessionService::start_workspace_session(&self, agent_config_id: &str, cwd: &str, mcp_http: McpHttpServerCredentials, plan: SessionPlan)` (the fourth parameter changes from `SessionIsolation` to `SessionPlan`).
  - `AcpSessionService::agent_config(&self, agent_config_id: &str) -> DomainResult<AgentConfig>`.
  - `McpToolService::begin_assistant_session_with(&self, session_id: &str, mode: AssistantMode, embedded_context: bool, instructions: Option<String>)`.

- [ ] **Step 1: Create `session_plan.rs` with only its tests (they fail to compile)**

```rust
//! Pure, per-agent-kind setup for one assistant session. It decides the
//! extra args, the isolation env, the `session/new` `_meta`, the files to
//! write into the scratch directory and the text to put in front of the first
//! prompt. It does no I/O: `src-tauri` writes the files.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_isolation::isolation_meta;

    fn input(kind: AgentKind) -> PlanInput<'static> {
        PlanInput {
            kind,
            config_args: &[],
            cwd: "/scratch/cwd",
            config_dir: "/scratch/config",
            instructions: "Be the workspace assistant.",
        }
    }

    #[test]
    fn the_claude_plan_matches_the_isolation_builders_exactly() {
        let plan = session_plan(&input(AgentKind::Claude));
        assert_eq!(plan.meta, Some(isolation_meta("Be the workspace assistant.")));
        assert_eq!(
            plan.env,
            vec![("CLAUDE_CONFIG_DIR".to_string(), "/scratch/config".to_string())]
        );
        assert!(plan.args.is_empty());
        assert!(plan.files.is_empty());
        assert_eq!(plan.first_prompt_prefix, None);
    }

    #[test]
    fn the_generic_plan_sets_no_claude_isolation_and_prefixes_the_first_prompt() {
        let plan = session_plan(&input(AgentKind::Generic));
        assert_eq!(plan.meta, None);
        assert!(plan.env.is_empty());
        assert!(plan.args.is_empty());
        assert!(plan.files.is_empty());
        let prefix = plan.first_prompt_prefix.expect("a generic agent needs the prefix");
        assert!(prefix.starts_with("Be the workspace assistant."), "{prefix}");
        assert!(prefix.contains("propose_changes"), "{prefix}");
        assert!(prefix.contains("Do not try to read, write or list files"), "{prefix}");
        assert!(!prefix.contains("CLAUDE_CONFIG_DIR"));
    }
}
```

Add `mod session_plan;` to `crates/rocket-app/src/lib.rs` (after `pub mod agent_isolation;`) so the file is compiled.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rocket-app -j4 session_plan`
Expected: FAIL to compile (`cannot find ... PlanInput`).

- [ ] **Step 3: Implement `session_plan.rs`**

Put this above the `#[cfg(test)]` block:

```rust
use rocket_acp::AgentKind;

use crate::agent_isolation::SessionIsolation;

/// Tells an agent that has no Rocket-specific lockdown how to behave. It names
/// the tool that edits requests, because such an agent may reach for files.
pub const NON_CLAUDE_TOOL_NOTICE: &str = "Use only the tools of the rocket MCP server. \
Do not try to read, write or list files, and do not run shell commands. To change a request, \
a folder's settings, a script or an environment variable, call propose_changes, which needs \
Edit mode. A request's documentation is the docs field of an update_request change.";

/// One file to create before the agent starts. The path is relative to the
/// session's scratch root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFile {
    pub rel_path: String,
    pub contents: String,
}

/// Everything that differs per agent kind for one session.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionPlan {
    /// Appended after the config's own args.
    pub args: Vec<String>,
    /// Isolation env. These win over extra env and the credential.
    pub env: Vec<(String, String)>,
    /// The `session/new` `_meta`, or `None` for an agent that ignores it.
    pub meta: Option<serde_json::Value>,
    /// Files written into the scratch root before the agent starts.
    pub files: Vec<PlannedFile>,
    /// Text put in front of the session's first prompt.
    pub first_prompt_prefix: Option<String>,
}

/// The inputs of `session_plan`.
#[derive(Debug, Clone, Copy)]
pub struct PlanInput<'a> {
    pub kind: AgentKind,
    /// The args of the agent config, so a kind can add a flag only when missing.
    pub config_args: &'a [String],
    pub cwd: &'a str,
    /// The scratch config directory. Claude uses it as `CLAUDE_CONFIG_DIR`.
    pub config_dir: &'a str,
    /// The Rocket assistant instructions.
    pub instructions: &'a str,
}

/// Builds the plan for one session.
pub fn session_plan(input: &PlanInput<'_>) -> SessionPlan {
    match input.kind {
        AgentKind::Claude => claude_plan(input),
        AgentKind::Generic => generic_plan(input),
    }
}

fn claude_plan(input: &PlanInput<'_>) -> SessionPlan {
    let isolation = SessionIsolation {
        config_dir: input.config_dir.to_string(),
        system_prompt_append: input.instructions.to_string(),
    };
    SessionPlan {
        args: Vec::new(),
        env: vec![isolation.env_entry()],
        meta: Some(isolation.meta()),
        files: Vec::new(),
        first_prompt_prefix: None,
    }
}

fn generic_plan(input: &PlanInput<'_>) -> SessionPlan {
    SessionPlan {
        args: Vec::new(),
        env: Vec::new(),
        meta: None,
        files: Vec::new(),
        first_prompt_prefix: Some(format!("{}\n\n{NON_CLAUDE_TOOL_NOTICE}", input.instructions)),
    }
}
```

In `crates/rocket-app/src/lib.rs` add next to the `agent_isolation` re-export:

```rust
pub use session_plan::{
    session_plan, PlanInput, PlannedFile, SessionPlan, NON_CLAUDE_TOOL_NOTICE,
};
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p rocket-app -j4 session_plan`
Expected: PASS (2 tests).

- [ ] **Step 5: Write the failing outline-prefix tests in `mcp_tool_service.rs`**

Add inside its `tests` module next to the other outline tests (they use `outline_ready_service()`):

```rust
    #[test]
    fn instructions_go_in_front_of_the_outline_in_the_first_preamble_only() {
        let svc = outline_ready_service();
        svc.begin_assistant_session_with(
            "a1",
            AssistantMode::Edit,
            false,
            Some("Use only the rocket tools.".to_string()),
        );

        match svc.take_outline_preamble("a1") {
            Some(rocket_acp::PromptPart::Text(text)) => {
                assert!(text.starts_with("Use only the rocket tools."), "{text}");
                assert!(text.contains("Assistant mode: Edit."), "{text}");
                assert!(text.contains("POST login.yml"), "{text}");
            }
            _ => panic!("expected a text part"),
        }
        assert!(svc.take_outline_preamble("a1").is_none());
    }

    #[test]
    fn without_instructions_the_preamble_is_unchanged() {
        let svc = outline_ready_service();
        svc.begin_assistant_session("a1", AssistantMode::Edit, true);
        match svc.take_outline_preamble("a1") {
            Some(rocket_acp::PromptPart::Resource { text, .. }) => {
                assert!(text.starts_with("Assistant mode: Edit."), "{text}");
            }
            _ => panic!("expected an embedded resource part"),
        }
    }

    #[test]
    fn peeked_instructions_stay_until_the_outline_is_discarded() {
        let svc = outline_ready_service();
        svc.begin_assistant_session_with(
            "a1",
            AssistantMode::Ask,
            false,
            Some("Rocket rules.".to_string()),
        );
        for _ in 0..2 {
            match svc.peek_outline_preamble("a1") {
                Some(rocket_acp::PromptPart::Text(text)) => {
                    assert!(text.starts_with("Rocket rules."), "{text}");
                }
                _ => panic!("expected a text part"),
            }
        }
    }
```

- [ ] **Step 6: Run them to verify they fail**

Run: `cargo test -p rocket-app -j4 instructions_go_in_front`
Expected: FAIL to compile (`no method begin_assistant_session_with`).

- [ ] **Step 7: Implement the instructions prefix in `mcp_tool_service.rs`**

Change `PendingOutline` (line 127) to:

```rust
struct PendingOutline {
    text: String,
    /// Whether the agent accepts embedded resources (ACP `embeddedContext`).
    embedded_context: bool,
    /// Rocket instructions to put in front of the outline, for an agent that
    /// cannot take a system prompt addition.
    instructions: Option<String>,
}
```

Replace `begin_assistant_session` with these two methods:

```rust
    pub fn begin_assistant_session(
        &self,
        session_id: &str,
        mode: AssistantMode,
        embedded_context: bool,
    ) {
        self.begin_assistant_session_with(session_id, mode, embedded_context, None);
    }

    /// Like `begin_assistant_session`, and also stores `instructions` to go in
    /// front of the outline in the session's first prompt.
    pub fn begin_assistant_session_with(
        &self,
        session_id: &str,
        mode: AssistantMode,
        embedded_context: bool,
        instructions: Option<String>,
    ) {
        self.open_session(session_id, mode);
        // Built without a tool event: the agent has not called the tool.
        let text = self
            .build_workspace_outline(None, None)
            .map(|(text, _)| text)
            .unwrap_or_else(|_| OUTLINE_UNAVAILABLE.to_string());
        self.pending_outlines
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                session_id.to_string(),
                PendingOutline {
                    text,
                    embedded_context,
                    instructions,
                },
            );
    }
```

(Keep the existing doc comment of `begin_assistant_session` above the first method.) In `peek_outline_preamble` replace the `let text = format!(...)` with:

```rust
        let head = pending
            .instructions
            .as_deref()
            .map(|instructions| format!("{instructions}\n\n"))
            .unwrap_or_default();
        let text = format!(
            "{head}Assistant mode: {}. {}\n\n{}",
            mode.label(),
            mode.summary(),
            pending.text
        );
```

- [ ] **Step 8: Run the outline tests**

Run: `cargo test -p rocket-app -j4 outline`
Expected: PASS (the three new tests and every existing outline test).

- [ ] **Step 9: Write the failing service tests in `acp_session_service.rs`**

First extend the capturing client. In `struct CapturingSessionClient` add `captured_args: Arc<Mutex<Vec<String>>>,` and in its `start_session` replace `_args: &[String],` with `args: &[String],` and add `*self.captured_args.lock().expect("lock") = args.to_vec();`.

Add the new tests next to `start_workspace_session_builds_http_and_stdio_specs_with_token_only_in_env`:

```rust
    #[tokio::test]
    async fn a_plan_without_meta_or_env_reaches_the_agent_as_given_and_its_args_follow_the_config() {
        let captured_args: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let captured_env: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let captured_meta: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));
        let client = CapturingSessionClient {
            captured_args: Arc::clone(&captured_args),
            captured_env: Arc::clone(&captured_env),
            captured_meta: Arc::clone(&captured_meta),
            ..Default::default()
        };
        let service = AcpSessionService::new(
            Box::new(client),
            Box::new(SharedEventPublisher(Arc::new(FakeEventPublisher::new()))),
            noop_cleanup(),
            agent_config_service(),
        );
        let plan = SessionPlan {
            args: vec!["-e".to_string(), "none".to_string()],
            env: vec![("X_HOME".to_string(), "/home/x".to_string())],
            meta: None,
            files: Vec::new(),
            first_prompt_prefix: None,
        };

        service
            .start_workspace_session("agent-1", "/tmp", test_creds(), plan)
            .await
            .expect("start_workspace_session");

        assert_eq!(*captured_args.lock().expect("lock"), vec!["-e", "none"]);
        assert_eq!(*captured_meta.lock().expect("lock"), None);
        let env = captured_env.lock().expect("lock").clone();
        assert!(env.iter().any(|(k, v)| k == "X_HOME" && v == "/home/x"));
        assert!(!env.iter().any(|(k, _)| k == "CLAUDE_CONFIG_DIR"));
    }

    #[tokio::test]
    async fn agent_config_returns_the_stored_config_and_refuses_an_unknown_id() {
        let service = AcpSessionService::new(
            Box::new(FakeSessionClient::default()),
            Box::new(SharedEventPublisher(Arc::new(FakeEventPublisher::new()))),
            noop_cleanup(),
            agent_config_service(),
        );
        let config = service.agent_config("agent-1").expect("known id");
        assert_eq!(config.kind, rocket_acp::AgentKind::Claude);
        assert!(service.agent_config("no-such-agent").is_err());
    }
```

- [ ] **Step 10: Update the existing tests that name `SessionIsolation`**

In the `tests` module:

1. Replace the helper at line 956 with:

```rust
    fn test_isolation() -> crate::session_plan::SessionPlan {
        crate::session_plan::session_plan(&crate::session_plan::PlanInput {
            kind: rocket_acp::AgentKind::Claude,
            config_args: &[],
            cwd: "/tmp",
            config_dir: "/scratch/config",
            instructions: "rocket",
        })
    }
```

2. Remove the `use crate::agent_isolation::SessionIsolation;` line in the `tests` module (line 607).
3. In the three `session_env` tests (about lines 505-600), change the third argument from a tuple to a slice, for example `("CLAUDE_CONFIG_DIR".to_string(), "/scratch".to_string())` becomes `&[("CLAUDE_CONFIG_DIR".to_string(), "/scratch".to_string())]`.
4. In the workspace capture test (about line 2016-2043) replace the `SessionIsolation { ... }` literal with:

```rust
        let isolation = crate::session_plan::session_plan(&crate::session_plan::PlanInput {
            kind: rocket_acp::AgentKind::Claude,
            config_args: &[],
            cwd: "/tmp/scratch-cwd",
            config_dir: "/tmp/scratch-config",
            instructions: "rocket",
        });
```

   and change `assert_eq!(capture.meta, Some(isolation.meta()));` to `assert_eq!(capture.meta, isolation.meta);`.

- [ ] **Step 11: Run the service tests to verify they fail**

Run: `cargo test -p rocket-app -j4 acp_session_service`
Expected: FAIL to compile (the service still takes `SessionIsolation`).

- [ ] **Step 12: Implement the service changes**

In `acp_session_service.rs`:

1. Change the import at line 13 to `use crate::agent_isolation::ROCKET_MCP_SERVER_NAME;` and add `use crate::session_plan::SessionPlan;`.
2. Replace `start_workspace_session` (line 148-181) with:

```rust
    pub async fn start_workspace_session(
        &self,
        agent_config_id: &str,
        cwd: &str,
        mcp_http: McpHttpServerCredentials,
        plan: SessionPlan,
    ) -> DomainResult<SessionInfo> {
        let config = self.agent_config_service.get(agent_config_id)?;
        // A hand-edited agent_configs.yml skips the save-time check.
        rocket_acp::validate_env_config(&config.credential_env_var, &config.extra_env)
            .map_err(|why| DomainError::InvalidInput(format!("agent config {why}")))?;
        let credential = self
            .agent_config_service
            .resolve_credential(agent_config_id)
            .await?;
        let env = session_env(&config, credential, &plan.env);
        let mcp_servers = mcp_server_specs(mcp_http)?;
        let mut args = config.args.clone();
        args.extend(plan.args.iter().cloned());
        let info = self
            .session_client
            .start_session(&config.command, &args, cwd, &env, &mcp_servers, plan.meta)
            .await?;
        self.event_publisher
            .publish(DomainEvent::AcpSessionStarted {
                session_id: info.session_id.clone(),
            });
        Ok(info)
    }

    /// The stored config of an agent. The command layer reads its kind and
    /// args to build the session plan before the session starts.
    pub fn agent_config(&self, agent_config_id: &str) -> DomainResult<AgentConfig> {
        self.agent_config_service.get(agent_config_id)
    }
```

   Update the doc comment above it: "`plan` supplies the isolation env, the `_meta` and extra args."

3. Replace `session_env` (line 460-482) with:

```rust
/// Builds the agent process environment. Order matters because a later
/// entry overrides an earlier one: the extra variables come first, then the
/// credential, then Rocket's isolation entries. An extra variable that names
/// the credential or an isolation variable is dropped, so Rocket's settings
/// win even if the config did not pass validation.
fn session_env(
    config: &AgentConfig,
    credential: String,
    isolation_env: &[(String, String)],
) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = config
        .extra_env
        .iter()
        .filter(|p| {
            !p.name.eq_ignore_ascii_case(&config.credential_env_var)
                && !isolation_env
                    .iter()
                    .any(|(name, _)| p.name.eq_ignore_ascii_case(name))
        })
        .map(|p| (p.name.clone(), p.value.clone()))
        .collect();
    env.push((config.credential_env_var.clone(), credential));
    env.extend(isolation_env.iter().cloned());
    env
}
```

- [ ] **Step 13: Run the rocket-app tests**

Run: `cargo test -p rocket-app -j4 acp_session_service && cargo test -p rocket-app -j4 session_plan && cargo test -p rocket-app -j4 agent_isolation`
Expected: PASS. `agent_isolation` tests pass unchanged.

- [ ] **Step 14: Commit**

```bash
git add crates/rocket-app/src/session_plan.rs crates/rocket-app/src/lib.rs \
  crates/rocket-app/src/acp_session_service.rs crates/rocket-app/src/mcp_tool_service.rs
```

Then invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(acp): plan each assistant session per agent kind`. Note: `src-tauri` does not compile until Task 3, so `cargo check -j4 -p rocket-app --tests` is the gate for this commit; if the harness runs a full check, commit Tasks 2 and 3 together.

---

### Task 3: Write planned files into the scratch and wire the command

**Files:**
- Modify: `src-tauri/src/agent_session/scratch.rs` (add `paths` and `write_files`, tests)
- Modify: `src-tauri/src/commands/acp_sessions.rs` (imports at line 3-7, `start_workspace_assistant_inner` at line 146-244)
- Modify: `src-tauri/tests/acp_mcp_start_workspace_assistant.rs` (fixture builders and one new test)
- Test: `src-tauri/src/agent_session/scratch.rs`, `src-tauri/tests/acp_mcp_start_workspace_assistant.rs`

**Interfaces:**
- Consumes: `PlannedFile`, `SessionPlan`, `PlanInput`, `session_plan`, `AcpSessionService::agent_config`, `McpToolService::begin_assistant_session_with` (Task 2).
- Produces:
  - `SessionScratch::paths(&self) -> Result<(String, String), DomainError>` returning `(cwd, config_dir)`.
  - `SessionScratch::write_files(&self, files: &[PlannedFile]) -> std::io::Result<()>`.

- [ ] **Step 1: Write the failing scratch tests**

Add inside the `tests` module of `src-tauri/src/agent_session/scratch.rs`:

```rust
    #[test]
    fn paths_returns_the_cwd_and_the_config_dir() {
        let parent = private_tempdir();
        let scratch = SessionScratch::create_in(parent.path()).expect("create scratch");
        let (cwd, config_dir) = scratch.paths().expect("utf-8 paths");
        assert_eq!(Path::new(&cwd), scratch.cwd());
        assert_eq!(Path::new(&config_dir), scratch.config_dir());
    }

    fn planned(rel_path: &str, contents: &str) -> rocket_app::PlannedFile {
        rocket_app::PlannedFile {
            rel_path: rel_path.to_string(),
            contents: contents.to_string(),
        }
    }

    #[test]
    fn write_files_creates_nested_files_under_the_root() {
        let parent = private_tempdir();
        let scratch = SessionScratch::create_in(parent.path()).expect("create scratch");
        scratch
            .write_files(&[
                planned("config/.gemini/settings.json", "{}"),
                planned("cwd/GEMINI.md", "rules"),
            ])
            .expect("write files");
        assert_eq!(
            std::fs::read_to_string(scratch.root().join("config/.gemini/settings.json"))
                .expect("settings"),
            "{}"
        );
        assert_eq!(
            std::fs::read_to_string(scratch.cwd().join("GEMINI.md")).expect("md"),
            "rules"
        );
    }

    #[cfg(unix)]
    #[test]
    fn written_files_and_new_dirs_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let parent = private_tempdir();
        let scratch = SessionScratch::create_in(parent.path()).expect("create scratch");
        scratch
            .write_files(&[planned("config/.gemini/settings.json", "{}")])
            .expect("write files");
        let file = scratch.root().join("config/.gemini/settings.json");
        let dir = scratch.root().join("config/.gemini");
        let mode = |p: &Path| std::fs::metadata(p).expect("meta").permissions().mode() & 0o777;
        assert_eq!(mode(&file), 0o600);
        assert_eq!(mode(&dir), 0o700);
    }

    #[test]
    fn write_files_refuses_unsafe_paths_and_writes_nothing_outside_the_root() {
        let parent = private_tempdir();
        let scratch = SessionScratch::create_in(parent.path()).expect("create scratch");
        for bad in ["", "/abs/path", "../escape", "cwd/../../escape", "cwd/./x", "./x"] {
            let err = scratch
                .write_files(&[planned(bad, "x")])
                .expect_err(bad);
            assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput, "{bad:?}");
        }
        assert!(!parent.path().join("escape").exists());
        assert_eq!(std::fs::read_dir(scratch.cwd()).expect("read cwd").count(), 0);
    }

    #[test]
    fn write_files_never_overwrites_an_existing_file() {
        let parent = private_tempdir();
        let scratch = SessionScratch::create_in(parent.path()).expect("create scratch");
        scratch
            .write_files(&[planned("cwd/a.txt", "first")])
            .expect("first write");
        let err = scratch
            .write_files(&[planned("cwd/a.txt", "second")])
            .expect_err("second write");
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(
            std::fs::read_to_string(scratch.cwd().join("a.txt")).expect("read"),
            "first"
        );
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p rocket -j4 --lib scratch`
Expected: FAIL to compile (`no method paths`, `write_files`).

- [ ] **Step 3: Implement `paths` and `write_files`**

At the top of `scratch.rs` change the import to `use rocket_app::{PlannedFile, SessionIsolation};`. Add inside `impl SessionScratch`, after `isolation`:

```rust
    /// The cwd and the config directory as strings, for `session/new` and the
    /// session plan.
    pub fn paths(&self) -> Result<(String, String), DomainError> {
        Ok((
            path_to_string(&self.cwd)?,
            path_to_string(&self.config_dir)?,
        ))
    }

    /// Writes the planned files under the root, before the agent starts. A
    /// path must be non-empty and made only of plain names (no `..`, no `.`,
    /// no root or prefix), so nothing can leave the root. Missing directories
    /// are created with mode 0700 and each file with mode 0600. An existing
    /// file is never overwritten.
    pub fn write_files(&self, files: &[PlannedFile]) -> std::io::Result<()> {
        for file in files {
            let rel = Path::new(&file.rel_path);
            let safe = !file.rel_path.is_empty()
                && rel
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_)));
            if !safe {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "a planned scratch file path must be relative and contain only plain names",
                ));
            }
            let target = self.root.join(rel);
            if let Some(parent) = target.parent() {
                create_private_dir_all(parent)?;
            }
            write_new_private_file(&target, file.contents.as_bytes())?;
        }
        Ok(())
    }
```

Add these free functions next to `create_private_dir`:

```rust
fn create_private_dir_all(path: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

/// Creates `path` for writing and fails when it already exists, including as
/// a symlink. The mode is 0600 on Unix.
fn write_new_private_file(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(contents)
}
```

- [ ] **Step 4: Run the scratch tests**

Run: `cargo test -p rocket -j4 --lib scratch`
Expected: PASS (the new tests and every existing scratch test).

- [ ] **Step 5: Write the failing integration test**

In `src-tauri/tests/acp_mcp_start_workspace_assistant.rs`:

1. Rename `fn agent_config_service()` to `fn agent_config_service_for(kind: rocket_acp::AgentKind)`, replace the `kind: rocket_acp::AgentKind::default(),` line that Task 1 added to its `AgentConfig` literal with `kind,`, and add below it:

```rust
fn agent_config_service() -> Arc<rocket_app::AgentConfigService> {
    agent_config_service_for(rocket_acp::AgentKind::Claude)
}
```

2. Rename `fn build_fixture(client: FakeSessionClient) -> Fixture` to `fn build_fixture_for(client: FakeSessionClient, kind: rocket_acp::AgentKind) -> Fixture`, replace its one call of `agent_config_service()` with `agent_config_service_for(kind)`, and add below it:

```rust
fn build_fixture(client: FakeSessionClient) -> Fixture {
    build_fixture_for(client, rocket_acp::AgentKind::Claude)
}
```

3. Add the test:

```rust
#[tokio::test]
async fn a_generic_agent_gets_no_claude_isolation_and_the_instructions_in_its_first_prompt() {
    let (client, _, captured_start) = client(false, "acp-real-session-generic");
    let fx = build_fixture_for(client, rocket_acp::AgentKind::Generic);

    let session_id = start(&fx)
        .await
        .expect("start_workspace_assistant_inner should succeed")
        .session_id;

    let start = captured_start
        .lock()
        .expect("lock")
        .clone()
        .expect("the client's start_session must have been called");
    assert_eq!(start.meta, None, "a generic agent gets no Claude _meta");
    assert!(
        !start.env.iter().any(|(k, _)| k == ISOLATION_ENV_CONFIG_DIR),
        "a generic agent gets no CLAUDE_CONFIG_DIR"
    );
    match fx.mcp_tool_svc.peek_outline_preamble(&session_id) {
        Some(rocket_acp::PromptPart::Resource { text, .. })
        | Some(rocket_acp::PromptPart::Text(text)) => {
            assert!(text.starts_with(WORKSPACE_ASSISTANT_INSTRUCTIONS), "{text}");
            assert!(text.contains("propose_changes"), "{text}");
        }
        None => panic!("the first prompt must carry the instructions"),
    }
}
```

- [ ] **Step 6: Run it to verify it fails**

Run: `cargo test -p rocket -j4 --test acp_mcp_start_workspace_assistant a_generic_agent`
Expected: FAIL to compile (the command still calls the old `start_workspace_session`).

- [ ] **Step 7: Wire the command layer**

In `src-tauri/src/commands/acp_sessions.rs`:

1. Change the `rocket_app` import to:

```rust
use rocket_app::{
    session_plan, AcpSessionService, AssistantMode, ConversationService, McpHttpServerCredentials,
    McpToolService, PlanInput, WORKSPACE_ASSISTANT_INSTRUCTIONS,
};
```

2. In `start_workspace_assistant_inner`, replace the block from `let (cwd, isolation) = scratch.isolation()?;` through the `SessionIsolation { ... };` statement (line 162-168) with:

```rust
    let (cwd, config_dir) = scratch.paths()?;
    // The config and its kind are read before the tool server starts, so an
    // unknown agent leaves nothing bound. The workspace assistant's own
    // instructions replace the default Rocket prompt.
    let config = svc.agent_config(&agent_config_id)?;
    let plan = session_plan(&PlanInput {
        kind: config.kind,
        config_args: &config.args,
        cwd: &cwd,
        config_dir: &config_dir,
        instructions: WORKSPACE_ASSISTANT_INSTRUCTIONS,
    });
    scratch.write_files(&plan.files).map_err(|e| {
        DomainError::Internal(format!("failed to write the agent scratch files: {e}"))
    })?;
```

3. Change the start call to pass the plan, keeping a copy of the prefix:

```rust
    let first_prompt_prefix = plan.first_prompt_prefix.clone();
    let info = match svc
        .start_workspace_session(&agent_config_id, &cwd, credentials, plan)
        .await
```

4. Replace the `mcp_tool_svc.begin_assistant_session(...)` call with:

```rust
    mcp_tool_svc.begin_assistant_session_with(
        &info.session_id,
        mode,
        info.prompt_capabilities.embedded_context,
        first_prompt_prefix,
    );
```

5. Update the comment above `SessionScratch::create()` to "An empty working directory and an empty config directory (the agent's `CLAUDE_CONFIG_DIR` or private home), created before the MCP server, so a failure here leaves nothing bound." Remove `scratch.isolation()` usage; keep the method (its own test still passes).

- [ ] **Step 8: Run the integration tests and the focused suites**

Run: `cargo test -p rocket -j4 --test acp_mcp_start_workspace_assistant && cargo test -p rocket -j4 --lib agent_session && cargo test -p rocket -j4 --lib commands::acp_sessions`
Expected: PASS, including `session_start_isolates_the_agent_in_fresh_scratch_directories` (Claude, unchanged assertions) and `session_start_failure_removes_the_scratch_directories`.

- [ ] **Step 9: Whole-change verification**

Run: `cargo check -j4 && cargo test -p rocket-acp -j4 && cargo test -p rocket-app -j4 && yarn tsc --noEmit && yarn check`
Expected: all PASS.

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/agent_session/scratch.rs src-tauri/src/commands/acp_sessions.rs \
  src-tauri/tests/acp_mcp_start_workspace_assistant.rs
```

Then invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(acp): write planned session files and use the session plan`.

---

## Next plan

**Next: Plan 2, Gemini preset** (`docs/superpowers/plans/2026-10-12-acp-multi-agent-plan-02-gemini-preset.md`).

When every task above is committed and the checks are green, my Claude executes Plan 2 next, in the same worktree, without asking again. One plan at a time: do not start Plan 2 before Plan 1's final checks pass. Keep the ledger at `.superpowers/sdd/2026-10-12-acp-multi-agent-plan-01/progress.md` and mark "Task N: complete" for each task so the `resume-plan` skill can find the next action after a `/clear`.
