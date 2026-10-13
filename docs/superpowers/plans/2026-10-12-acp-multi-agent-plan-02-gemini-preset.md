# ACP Multi-Agent Support, Plan 2: Gemini Preset Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the workspace assistant work with the Gemini CLI: Gemini sees only the Rocket tools, runs in a private home, gets the Rocket instructions and calls `propose_changes` without a permission prompt.

**Architecture:** Add `AgentKind::Gemini` and a Gemini arm to the pure `session_plan` from Plan 1. The plan writes a lockdown `settings.json` into a private `GEMINI_CLI_HOME` (the scratch config directory) with an exact-name `tools.core` allow-list, a `GEMINI.md` into the scratch cwd, and adds the Gemini env and args. The dialog gets a Gemini type with sensible defaults. A manual run with a real key closes the plan.

**Tech Stack:** Rust (rocket-acp, rocket-app, src-tauri), React + TypeScript, Vitest, Biome. Gemini CLI 0.63.0.

**Spec:** `.claude/acp-multi-agent-support.md` (section "Verified Gemini facts"). Depends on Plan 1 being merged: `docs/superpowers/plans/2026-10-12-acp-multi-agent-plan-01-agent-kind.md`.

## Global Constraints

- Run cargo with `-j4`. Never `cargo test --workspace` or `--all`.
- Never call unwrap in production paths (a project hook blocks it in Rust edits). `expect("...")` is fine in tests.
- `#[serde(rename_all = "camelCase")]` only on IPC DTOs, never on `AgentConfig`.
- UI uses shadcn/ui primitives only, lucide-react icons only.
- Never `git add -A`, `git add .` or `--all`. Explicit paths. Commit through the `dev-workflow-skills:1-git-commit` skill (conventional commits).
- Gemini facts that the code must respect (verified on Gemini CLI 0.63.0 against a fake model API):
  - The system settings file (`GEMINI_CLI_SYSTEM_SETTINGS_PATH`) is ignored unless root-owned. Use the user settings file at `$GEMINI_CLI_HOME/.gemini/settings.json`.
  - A wildcard in `tools.core` (`mcp_rocket_*`) hides the built-ins but every MCP call is denied by policy. Use exact tool names. Exact names also skip the permission prompt.
  - The MCP server is not started in an untrusted folder. Set `GEMINI_CLI_TRUST_WORKSPACE=true`.
  - MCP tools are named `mcp_<server>_<tool>`, for example `mcp_rocket_propose_changes`.
  - `_meta` is ignored by Gemini. `GEMINI.md` in the cwd lands in the first user message, not the system prompt.
- No OAuth in this plan: Gemini uses `GEMINI_API_KEY`. A Google login needs `oauth_creds.json` copied into the private home and is a separate, security-reviewed follow-up.
- No `OpenCollection` read needed: this plan does not touch collection or environment YAML.

## Review Focus

- The Gemini tool allow-list must list exactly the Rocket tools by name and never contain a wildcard (Task 4 tests, plus a parity test against the registered tools so a new tool cannot be silently denied).
- A Gemini config must not be able to claim `GEMINI_CLI_HOME`, `GEMINI_CLI_TRUST_WORKSPACE` or `GEMINI_CLI_IDE_WORKSPACE_PATH` in its extra env or as the credential name, even in a hand-edited `agent_configs.yml` (Task 4 tests).
- A user who already has `--acp` or `-e` in their args must not get them twice (Task 4 test).
- A Gemini session must not touch the user's real `~/.gemini` (Task 6 manual check).
- A schema rejected by Google's API would break the whole turn, not one tool (Task 6 gate).

---

### Task 4: The Gemini arm of the session plan

**Files:**
- Modify: `crates/rocket-acp/src/agent_config.rs` (enum, new `reserved_env_vars` and `validate_kind_env`)
- Modify: `crates/rocket-acp/src/lib.rs` (re-export `validate_kind_env`)
- Modify: `crates/rocket-app/src/session_plan.rs` (names constant, Gemini arm, tests)
- Modify: `crates/rocket-app/src/lib.rs` (re-export `ROCKET_MCP_TOOL_NAMES`, `GEMINI_FIRST_PROMPT_REMINDER`)
- Modify: `crates/rocket-app/src/agent_config_service.rs` (`validate_config`)
- Modify: `crates/rocket-app/src/acp_session_service.rs` (`start_workspace_session`)
- Modify: `src-tauri/src/mcp/tool_server.rs` (one parity test in its `tests` module)
- Modify: `src/lib/tauri-api.ts` (`AgentKind` union)
- Test: `crates/rocket-acp/src/agent_config.rs`, `crates/rocket-app/src/session_plan.rs`, `crates/rocket-app/src/agent_config_service.rs`, `crates/rocket-app/src/acp_session_service.rs`, `src-tauri/src/mcp/tool_server.rs`

**Interfaces:**
- Consumes: `AgentKind`, `PlanInput`, `SessionPlan`, `PlannedFile`, `session_plan`, `NON_CLAUDE_TOOL_NOTICE`, `ROCKET_MCP_SERVER_NAME` (Plan 1 and `agent_isolation.rs`).
- Produces:
  - `AgentKind::Gemini` (wire name `"gemini"`).
  - `AgentKind::reserved_env_vars(self) -> &'static [&'static str]`.
  - `rocket_acp::validate_kind_env(kind: AgentKind, credential_env_var: &str, extra_env: &[EnvPair]) -> Result<(), String>`.
  - `rocket_app::ROCKET_MCP_TOOL_NAMES: [&str; 11]` (sorted, the registered tool names).
  - `rocket_app::GEMINI_FIRST_PROMPT_REMINDER: &str`.

- [ ] **Step 1: Write the failing reserved-env tests in `crates/rocket-acp/src/agent_config.rs`**

Add inside the `tests` module:

```rust
    #[test]
    fn gemini_kind_uses_its_lowercase_wire_name() {
        assert_eq!(
            serde_json::to_string(&AgentKind::Gemini).expect("json"),
            "\"gemini\""
        );
    }

    #[test]
    fn kind_env_check_refuses_the_reserved_names_of_that_kind_only() {
        let pair = |name: &str| EnvPair {
            name: name.to_string(),
            value: "v".to_string(),
        };
        let err = validate_kind_env(AgentKind::Gemini, "KEY", &[pair("GEMINI_CLI_HOME")])
            .expect_err("reserved extra env");
        assert!(err.contains("extra_env #1") && err.contains("reserved"), "{err}");
        assert!(
            validate_kind_env(AgentKind::Gemini, "gemini_cli_trust_workspace", &[]).is_err(),
            "the credential name is checked too, in any case"
        );
        assert!(
            validate_kind_env(AgentKind::Gemini, "KEY", &[pair("GEMINI_CLI_IDE_WORKSPACE_PATH")])
                .is_err()
        );
        assert!(validate_kind_env(AgentKind::Gemini, "GEMINI_API_KEY", &[pair("HTTPS_PROXY")]).is_ok());
        assert!(validate_kind_env(AgentKind::Claude, "KEY", &[pair("GEMINI_CLI_HOME")]).is_ok());
        assert!(validate_kind_env(AgentKind::Generic, "KEY", &[pair("GEMINI_CLI_HOME")]).is_ok());
        let err = validate_kind_env(AgentKind::Gemini, "GEMINI_CLI_HOME", &[pair("X")])
            .expect_err("reserved credential name");
        assert!(!err.contains('/'), "never echoes a value: {err}");
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p rocket-acp -j4 kind_env_check`
Expected: FAIL to compile (`no variant Gemini`, `cannot find function validate_kind_env`).

- [ ] **Step 3: Implement the variant, the reserved list and the check**

In `agent_config.rs` add the variant after `Claude` and before `Generic`:

```rust
    /// Gemini CLI in ACP mode. Isolation travels in files and env: a private
    /// home with a lockdown settings file, and a `GEMINI.md` in the cwd.
    Gemini,
```

Add below the enum:

```rust
impl AgentKind {
    /// Environment variable names Rocket sets for this kind. A config of this
    /// kind may not claim them, as a credential name or as extra env.
    pub fn reserved_env_vars(self) -> &'static [&'static str] {
        match self {
            AgentKind::Claude | AgentKind::Generic => &[],
            AgentKind::Gemini => &[
                "GEMINI_CLI_HOME",
                "GEMINI_CLI_TRUST_WORKSPACE",
                "GEMINI_CLI_IDE_WORKSPACE_PATH",
            ],
        }
    }
}

/// Checks the credential variable name and every extra variable name against
/// the names Rocket sets for `kind`. `CLAUDE_CONFIG_DIR` is checked for every
/// kind by `validate_env_var_name`. The error is a short reason and never
/// contains a value.
pub fn validate_kind_env(
    kind: AgentKind,
    credential_env_var: &str,
    extra_env: &[EnvPair],
) -> Result<(), String> {
    let reserved = kind.reserved_env_vars();
    let is_reserved = |name: &str| reserved.iter().any(|r| name.eq_ignore_ascii_case(r));
    if is_reserved(credential_env_var) {
        return Err("credential_env_var is reserved by Rocket".to_string());
    }
    for (i, pair) in extra_env.iter().enumerate() {
        if is_reserved(&pair.name) {
            return Err(format!("extra_env #{} name is reserved by Rocket", i + 1));
        }
    }
    Ok(())
}
```

In `crates/rocket-acp/src/lib.rs` add `validate_kind_env,` to the `pub use agent_config::{...}` list.

- [ ] **Step 4: Run the rocket-acp tests**

Run: `cargo test -p rocket-acp -j4 agent_config`
Expected: PASS.

- [ ] **Step 5: Write the failing Gemini plan tests in `crates/rocket-app/src/session_plan.rs`**

Add inside its `tests` module:

```rust
    fn gemini_input(config_args: &[String]) -> PlanInput<'_> {
        PlanInput {
            kind: AgentKind::Gemini,
            config_args,
            cwd: "/scratch/cwd",
            config_dir: "/scratch/config",
            instructions: "Be the workspace assistant.",
        }
    }

    fn file<'a>(plan: &'a SessionPlan, rel_path: &str) -> &'a PlannedFile {
        plan.files
            .iter()
            .find(|f| f.rel_path == rel_path)
            .unwrap_or_else(|| panic!("no planned file {rel_path}"))
    }

    #[test]
    fn the_gemini_plan_gives_gemini_a_private_home_and_trusts_the_scratch_cwd() {
        let plan = session_plan(&gemini_input(&[]));
        assert_eq!(plan.meta, None, "Gemini ignores _meta");
        let env: Vec<(&str, &str)> = plan
            .env
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        assert!(env.contains(&("GEMINI_CLI_HOME", "/scratch/config")));
        assert!(env.contains(&("GEMINI_CLI_TRUST_WORKSPACE", "true")));
        assert!(env.contains(&("GEMINI_CLI_IDE_WORKSPACE_PATH", "")));
        assert!(!env.iter().any(|(k, _)| *k == "CLAUDE_CONFIG_DIR"));
    }

    #[test]
    fn the_gemini_settings_allow_exactly_the_rocket_tools_by_name() {
        let plan = session_plan(&gemini_input(&[]));
        let settings: serde_json::Value = serde_json::from_str(
            &file(&plan, "config/.gemini/settings.json").contents,
        )
        .expect("settings are JSON");
        let core: Vec<String> = settings["tools"]["core"]
            .as_array()
            .expect("tools.core is a list")
            .iter()
            .map(|v| v.as_str().expect("string").to_string())
            .collect();
        let expected: Vec<String> = ROCKET_MCP_TOOL_NAMES
            .iter()
            .map(|name| format!("mcp_rocket_{name}"))
            .collect();
        assert_eq!(core, expected);
        assert!(core.iter().all(|name| !name.contains('*')), "no wildcard: {core:?}");
        assert_eq!(settings["skills"]["enabled"], false);
        assert_eq!(settings["hooksConfig"]["enabled"], false);
        assert_eq!(settings["mcp"]["allowed"], serde_json::json!(["rocket"]));
    }

    #[test]
    fn the_gemini_plan_adds_acp_and_no_extensions_only_when_missing() {
        let bare = session_plan(&gemini_input(&[]));
        assert_eq!(bare.args, vec!["--acp", "-e", "none"]);

        let own = vec!["--acp".to_string(), "-e".to_string(), "none".to_string()];
        assert!(session_plan(&gemini_input(&own)).args.is_empty());

        let old_flag = vec!["--experimental-acp".to_string()];
        assert_eq!(
            session_plan(&gemini_input(&old_flag)).args,
            vec!["-e", "none"],
            "the deprecated flag still counts as ACP mode"
        );

        let long = vec!["--extensions".to_string(), "none".to_string()];
        assert_eq!(session_plan(&gemini_input(&long)).args, vec!["--acp"]);
    }

    #[test]
    fn the_gemini_plan_writes_instructions_to_gemini_md_and_a_short_reminder_to_the_prompt() {
        let plan = session_plan(&gemini_input(&[]));
        let md = &file(&plan, "cwd/GEMINI.md").contents;
        assert!(md.starts_with("Be the workspace assistant."), "{md}");
        assert!(md.contains("propose_changes"), "{md}");
        assert!(md.contains("mcp_rocket_propose_changes"), "{md}");
        assert_eq!(
            plan.first_prompt_prefix.as_deref(),
            Some(GEMINI_FIRST_PROMPT_REMINDER)
        );
        assert!(
            GEMINI_FIRST_PROMPT_REMINDER.len() < 300,
            "the reminder stays short"
        );
        assert_eq!(plan.files.len(), 2);
    }

    #[test]
    fn the_tool_name_list_is_sorted_and_has_no_duplicates() {
        let mut sorted = ROCKET_MCP_TOOL_NAMES;
        sorted.sort_unstable();
        assert_eq!(sorted, ROCKET_MCP_TOOL_NAMES);
        let unique: std::collections::BTreeSet<_> = ROCKET_MCP_TOOL_NAMES.iter().collect();
        assert_eq!(unique.len(), ROCKET_MCP_TOOL_NAMES.len());
    }
```

- [ ] **Step 6: Run them to verify they fail**

Run: `cargo test -p rocket-app -j4 session_plan`
Expected: FAIL to compile (`non-exhaustive patterns: Gemini`, unknown constants).

- [ ] **Step 7: Implement the Gemini arm**

In `crates/rocket-app/src/session_plan.rs` change the imports to:

```rust
use rocket_acp::AgentKind;

use crate::agent_isolation::{SessionIsolation, ROCKET_MCP_SERVER_NAME};
```

Add after `NON_CLAUDE_TOOL_NOTICE`:

```rust
/// The names of every tool the Rocket MCP server registers. Gemini needs them
/// spelled out, because a wildcard in its allow-list denies the MCP calls.
/// A test in `src-tauri` (`the_exported_tool_names_match_the_registered_tools`)
/// fails when this list and the registered tools differ.
pub const ROCKET_MCP_TOOL_NAMES: [&str; 11] = [
    "get_collection_settings",
    "get_environment",
    "get_folder_settings",
    "get_history",
    "get_request",
    "get_test_results",
    "get_workspace_outline",
    "list_collections",
    "list_proposals",
    "propose_changes",
    "run_request",
];

/// Put in front of a Gemini session's first prompt. `GEMINI.md` carries the
/// full instructions, but it sits in the first message and may be summarised
/// in a long session, so this short reminder travels with the first prompt.
pub const GEMINI_FIRST_PROMPT_REMINDER: &str = "Reminder: use only the mcp_rocket_* tools. \
Do not read or write files. To change anything, call mcp_rocket_propose_changes.";
```

Add `AgentKind::Gemini => gemini_plan(input),` to the `match` in `session_plan` and add:

```rust
fn gemini_plan(input: &PlanInput<'_>) -> SessionPlan {
    let has_flag = |flags: &[&str]| {
        input
            .config_args
            .iter()
            .any(|arg| flags.contains(&arg.as_str()))
    };
    let mut args = Vec::new();
    if !has_flag(&["--acp", "--experimental-acp"]) {
        args.push("--acp".to_string());
    }
    if !has_flag(&["-e", "--extensions"]) {
        args.push("-e".to_string());
        args.push("none".to_string());
    }
    let tool_names: Vec<String> = ROCKET_MCP_TOOL_NAMES
        .iter()
        .map(|name| format!("mcp_{ROCKET_MCP_SERVER_NAME}_{name}"))
        .collect();
    // Written to the user settings file of the private home: Gemini ignores
    // a system settings file that is not root-owned.
    let settings = serde_json::json!({
        "tools": { "core": tool_names },
        "skills": { "enabled": false },
        "hooksConfig": { "enabled": false },
        "mcp": { "allowed": [ROCKET_MCP_SERVER_NAME] }
    });
    let guide = format!(
        "{}\n\n{NON_CLAUDE_TOOL_NOTICE}\n\nIn this session the Rocket tools are named \
mcp_{ROCKET_MCP_SERVER_NAME}_<tool>, for example mcp_{ROCKET_MCP_SERVER_NAME}_propose_changes.\n",
        input.instructions
    );
    SessionPlan {
        args,
        env: vec![
            ("GEMINI_CLI_HOME".to_string(), input.config_dir.to_string()),
            ("GEMINI_CLI_TRUST_WORKSPACE".to_string(), "true".to_string()),
            // Stops an inherited IDE path from adding workspace directories.
            ("GEMINI_CLI_IDE_WORKSPACE_PATH".to_string(), String::new()),
        ],
        meta: None,
        files: vec![
            PlannedFile {
                rel_path: "config/.gemini/settings.json".to_string(),
                contents: serde_json::to_string_pretty(&settings)
                    .unwrap_or_else(|_| settings.to_string()),
            },
            PlannedFile {
                rel_path: "cwd/GEMINI.md".to_string(),
                contents: guide,
            },
        ],
        first_prompt_prefix: Some(GEMINI_FIRST_PROMPT_REMINDER.to_string()),
    }
}
```

In `crates/rocket-app/src/lib.rs` extend the `session_plan` re-export with `GEMINI_FIRST_PROMPT_REMINDER, ROCKET_MCP_TOOL_NAMES,`.

- [ ] **Step 8: Run the plan tests**

Run: `cargo test -p rocket-app -j4 session_plan`
Expected: PASS (the two Plan 1 tests and the five new ones).

- [ ] **Step 9: Write the parity test in `src-tauri/src/mcp/tool_server.rs`**

Add inside its `tests` module next to `the_tool_list_has_the_propose_tools_and_no_direct_write_tools`:

```rust
    #[test]
    fn the_exported_tool_names_match_the_registered_tools() {
        let server: RocketMcpToolServer<tauri::test::MockRuntime> =
            RocketMcpToolServer::new(test_app_handle(), "session-1".to_string());
        let mut registered: Vec<String> = server
            .tool_router
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect();
        registered.sort();
        let exported: Vec<String> = rocket_app::ROCKET_MCP_TOOL_NAMES
            .iter()
            .map(|name| name.to_string())
            .collect();
        assert_eq!(
            registered, exported,
            "update ROCKET_MCP_TOOL_NAMES in crates/rocket-app/src/session_plan.rs"
        );
    }
```

- [ ] **Step 10: Run it**

Run: `cargo test -p rocket -j4 --lib the_exported_tool_names`
Expected: PASS (the constant was written from the current tool list). If it fails, the constant is out of date: fix the constant, not the test.

- [ ] **Step 11: Write the failing validation tests**

In `crates/rocket-app/src/agent_config_service.rs` add inside `tests`:

```rust
    #[test]
    fn validate_config_refuses_a_gemini_reserved_env_name_and_only_for_gemini() {
        let mut config = sample_config("a", "conn-1");
        config.extra_env = vec![rocket_acp::EnvPair {
            name: "GEMINI_CLI_HOME".to_string(),
            value: "/x".to_string(),
        }];
        assert!(validate_config(&config).is_ok(), "fine for a Claude config");
        config.kind = rocket_acp::AgentKind::Gemini;
        let err = validate_config(&config).expect_err("reserved for Gemini");
        assert!(err.to_string().contains("reserved"), "{err}");
        assert!(!err.to_string().contains("/x"));
    }
```

In `crates/rocket-app/src/acp_session_service.rs` add inside `tests`, after `start_workspace_session_refuses_an_invalid_persisted_config`:

```rust
    #[tokio::test]
    async fn start_workspace_session_refuses_a_persisted_gemini_config_that_claims_a_reserved_name() {
        let mut bad = sample_config();
        bad.kind = rocket_acp::AgentKind::Gemini;
        bad.extra_env = vec![env_pair("GEMINI_CLI_HOME", "/evil")];
        let sm_repo = FakeSecretManagerRepo(Mutex::new(vec![sample_connection()]));
        let secret_manager = Arc::new(crate::secret_manager_service::SecretManagerService::new(
            Box::new(sm_repo),
            Arc::new(FakeSecretStore),
            Arc::new(FakeVaultFetcher::default()),
        ));
        let cfg_service = Arc::new(AgentConfigService::new(
            Box::new(FakeAgentConfigRepo(Mutex::new(vec![bad]))),
            secret_manager,
            Arc::new(rocket_environment::secret_store::NullSecretStore),
            crate::process_env_reader(),
        ));
        let service = AcpSessionService::new(
            Box::new(FakeSessionClient::default()),
            Box::new(SharedEventPublisher(Arc::new(FakeEventPublisher::new()))),
            noop_cleanup(),
            cfg_service,
        );
        let err = service
            .start_workspace_session("agent-1", "/tmp", test_creds(), test_isolation())
            .await
            .expect_err("a reserved name must not start");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        let msg = err.to_string();
        assert!(msg.contains("extra_env #1") && msg.contains("reserved"), "got: {msg}");
        assert!(!msg.contains("/evil"));
    }
```

- [ ] **Step 12: Run them to verify they fail**

Run: `cargo test -p rocket-app -j4 reserved`
Expected: FAIL (`validate_config` accepts the name; the session starts).

- [ ] **Step 13: Implement the validation calls**

In `agent_config_service.rs`, in `validate_config`, after the `validate_env_config` call add:

```rust
    rocket_acp::validate_kind_env(config.kind, &config.credential_env_var, &config.extra_env)
        .map_err(|why| DomainError::InvalidInput(format!("agent config {why}")))?;
```

In `acp_session_service.rs`, in `start_workspace_session`, after the `validate_env_config` call add the same statement.

- [ ] **Step 14: Add the TypeScript union member**

In `src/lib/tauri-api.ts` change the type to `export type AgentKind = 'claude' | 'gemini' | 'generic';`.

- [ ] **Step 15: Run all checks for this task**

Run: `cargo test -p rocket-acp -j4 && cargo test -p rocket-app -j4 && cargo test -p rocket -j4 --lib the_exported_tool_names && yarn tsc --noEmit && yarn check`
Expected: all PASS.

- [ ] **Step 16: Commit**

```bash
git add crates/rocket-acp/src/agent_config.rs crates/rocket-acp/src/lib.rs \
  crates/rocket-app/src/session_plan.rs crates/rocket-app/src/lib.rs \
  crates/rocket-app/src/agent_config_service.rs crates/rocket-app/src/acp_session_service.rs \
  src-tauri/src/mcp/tool_server.rs src/lib/tauri-api.ts
```

Then invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(acp): add the Gemini session plan`.

---

### Task 5: Gemini in the agent dialog

**Files:**
- Modify: `src/components/settings/AgentConfigsDialog.tsx`
- Test: `src/components/settings/__tests__/AgentConfigsDialog.test.tsx`

**Interfaces:**
- Consumes: `AgentKind` including `'gemini'` (Task 4), the dialog's `editing` form state and the `Agent type` radio group (Plan 1 Task 1).
- Produces: a third radio `Gemini CLI`; `withKind(form, kind)` that fills empty command, args and credential variable name for a new Gemini agent; the saved payload carries `kind: 'gemini'`.

- [ ] **Step 1: Write the failing tests**

Add inside `describe('AgentConfigsDialog', ...)`:

```tsx
  it('prefills a new Gemini agent with the Gemini command, args and key name', async () => {
    vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
    const user = userEvent.setup();
    renderDialog();
    await user.click(await screen.findByRole('button', { name: /add agent/i }));
    await screen.findByLabelText('Label');
    await user.click(screen.getByRole('radio', { name: /^gemini cli/i }));
    expect(screen.getByLabelText('Command')).toHaveValue('gemini');
    expect(screen.getByLabelText(/Args/)).toHaveValue('--acp');
    expect(screen.getByLabelText('Credential Env Var')).toHaveValue('GEMINI_API_KEY');
  });

  it('keeps what the user already typed when Gemini is chosen', async () => {
    vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
    const user = userEvent.setup();
    renderDialog();
    await user.click(await screen.findByRole('button', { name: /add agent/i }));
    await user.type(await screen.findByLabelText('Command'), '/opt/gemini');
    await user.click(screen.getByRole('radio', { name: /^gemini cli/i }));
    expect(screen.getByLabelText('Command')).toHaveValue('/opt/gemini');
    expect(screen.getByLabelText(/Args/)).toHaveValue('--acp');
  });

  it('does not change the fields of a stored agent when its type is switched', async () => {
    const user = userEvent.setup();
    renderDialog();
    await user.click(await screen.findByRole('button', { name: 'Edit agent' }));
    await screen.findByLabelText('Label');
    await user.click(screen.getByRole('radio', { name: /^gemini cli/i }));
    expect(screen.getByLabelText('Command')).toHaveValue('claude-agent-acp');
    expect(screen.getByLabelText('Credential Env Var')).toHaveValue('ANTHROPIC_API_KEY');
  });

  it('saves a Gemini agent with kind gemini', async () => {
    vi.mocked(api.listAgentConfigs).mockResolvedValue([]);
    vi.mocked(api.listSecretManagerConnections).mockResolvedValue([]);
    const user = userEvent.setup();
    renderDialog();
    await user.click(await screen.findByRole('button', { name: /add agent/i }));
    await user.type(await screen.findByLabelText('Label'), 'Gemini');
    await user.click(screen.getByRole('radio', { name: /^gemini cli/i }));
    await user.click(screen.getByRole('radio', { name: /rocket environment variable/i }));
    await user.type(screen.getByLabelText('Environment variable name'), 'GEMINI_API_KEY');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
    expect(vi.mocked(api.saveAgentConfig).mock.calls[0][0]).toMatchObject({
      kind: 'gemini',
      command: 'gemini',
      args: ['--acp'],
      credentialEnvVar: 'GEMINI_API_KEY',
      credentialSource: 'env',
      sourceEnvVar: 'GEMINI_API_KEY',
    });
  });
```

- [ ] **Step 2: Run them to verify they fail**

Run: `yarn test AgentConfigsDialog`
Expected: FAIL (no radio named "Gemini CLI").

- [ ] **Step 3: Implement the radio, the presets and the help text**

In `AgentConfigsDialog.tsx` add above `emptyForm`:

```tsx
// Defaults for a new agent of a kind. They fill only fields the user left empty.
const KIND_PRESETS: Partial<
  Record<AgentKind, { command: string; args: string; credentialEnvVar: string }>
> = {
  gemini: { command: 'gemini', args: '--acp', credentialEnvVar: 'GEMINI_API_KEY' },
};

function withKind<
  T extends {
    isNew: boolean;
    command: string;
    args: string;
    credentialEnvVar: string;
    kind: AgentKind;
  },
>(form: T, kind: AgentKind): T {
  const preset = form.isNew ? KIND_PRESETS[kind] : undefined;
  if (!preset) return { ...form, kind };
  return {
    ...form,
    kind,
    command: form.command.trim() === '' ? preset.command : form.command,
    args: form.args.trim() === '' ? preset.args : form.args,
    credentialEnvVar:
      form.credentialEnvVar.trim() === '' ? preset.credentialEnvVar : form.credentialEnvVar,
  };
}
```

Change the radio group's `onValueChange` to `onValueChange={(value) => setEditing(withKind(editing, value as AgentKind))}`. Add a third item between Claude Code and Generic:

```tsx
                <div className='flex items-center gap-2'>
                  <RadioGroupItem value='gemini' id='ac-kind-gemini' />
                  <Label htmlFor='ac-kind-gemini' className='text-sm font-normal'>
                    Gemini CLI
                  </Label>
                </div>
```

Add after the existing `generic` help paragraph:

```tsx
              {editing.kind === 'gemini' && (
                <p className='mt-1 text-xs text-muted-foreground'>
                  Rocket runs Gemini in a private home with only the Rocket tools. Use a Gemini API
                  key: a Google login is not available inside a Rocket session.
                </p>
              )}
```

Change the command input placeholder to `placeholder={editing.kind === 'gemini' ? 'gemini' : 'claude-agent-acp'}` and the credential variable placeholder to `placeholder={editing.kind === 'gemini' ? 'GEMINI_API_KEY' : 'ANTHROPIC_API_KEY'}`.

- [ ] **Step 4: Run all checks for this task**

Run: `yarn test AgentConfigsDialog && yarn tsc --noEmit && yarn check`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/components/settings/AgentConfigsDialog.tsx \
  src/components/settings/__tests__/AgentConfigsDialog.test.tsx
```

Then invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(acp): offer Gemini CLI in the agent dialog`.

---

### Task 6: Verify Gemini end to end with a real key

This task has no code. It is the gate before Plan 2 is merged. It needs a real `GEMINI_API_KEY` exported in the shell that starts Rocket. Everything before it ran against a fake model API, so Google's real schema validation and Gemini's real tool use are untested until now.

**Files:**
- Modify: `.claude/acp-multi-agent-support.md` (append a "Verification log" section)

**Interfaces:**
- Consumes: the finished Tasks 4 and 5.
- Produces: a recorded pass or fail for each check below, with the Gemini version and date.

- [ ] **Step 1: Prepare**

Run: `gemini --version` (expect 0.63.0 or newer; record it) and `ls -la --time=ctime ~/.gemini | head -5` (record the newest change time of the real `~/.gemini`). In the shell that will start Rocket run `export GEMINI_API_KEY=<your key>`. Then `yarn tauri dev`.

- [ ] **Step 2: Add the agent and start the assistant**

In Settings, AI Agents, Add agent. Choose `Gemini CLI`. Label `Gemini`. Command, args and the key name are prefilled (`gemini`, `--acp`, `GEMINI_API_KEY`). Pick the credential source `Rocket environment variable` with name `GEMINI_API_KEY`. Save, then use Test. Open the assistant, choose the Gemini agent, mode `Edit`.
Expected: the session starts without an error toast.

- [ ] **Step 3: Check the tool set**

Ask: `List every tool you can call, by exact name.`
Expected: only the 11 names `mcp_rocket_*` (see `ROCKET_MCP_TOOL_NAMES`). No `read_file`, `glob`, `run_shell_command`, `web_fetch`, `google_web_search`, `activate_skill`. Record the reply.
If any built-in appears, STOP: the lockdown failed. Record it and write a follow-up plan. Do not merge.

- [ ] **Step 4: Check the schema and the edit**

Ask: `Set the docs of the request <pick a real request in a collection> to "Updated by Gemini". Use the right Rocket tool.`
Expected: a proposal card appears (no permission card, no error), you accept it, and the request's docs show the new text.
If the turn fails with an API error that mentions the schema (`400`, `INVALID_ARGUMENT`, `unsupported`), STOP. Record the exact error text. The fix is a Gemini-only flattened `propose_changes` schema (inline the `$ref`s, use `enum` instead of `const`, drop `$schema`) and belongs in a new plan. Do not improvise it here.

- [ ] **Step 5: Check isolation and cleanup**

While the session is open run `ls "${XDG_RUNTIME_DIR:-$HOME/.rocket-api}/rocket-agent-sessions/"*/config/.gemini` (expect `settings.json` and Gemini's own state). End the session and run the same `ls` (expect the directory gone). Run `ls -la --time=ctime ~/.gemini | head -5` again.
Expected: the real `~/.gemini` has the same newest change time as in Step 1.

- [ ] **Step 6: Record the result**

Append to `.claude/acp-multi-agent-support.md`:

```markdown
## Verification log

- <date>, Gemini CLI <version>, Plan 2: tool set <pass|fail: names listed>, docs edit through propose_changes <pass|fail: error text>, real ~/.gemini untouched <yes|no>, scratch removed after the session <yes|no>.
```

Fill every angle bracket with the real result before committing.

- [ ] **Step 7: Commit**

```bash
git add .claude/acp-multi-agent-support.md
```

Then invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `docs(acp): record the Gemini verification run`.

---

## Next plan

**Next: Plan 3, hardening and docs** (`docs/superpowers/plans/2026-10-12-acp-multi-agent-plan-03-hardening.md`).

When Tasks 4 and 5 are committed with green checks and the Task 6 run passed, my Claude executes Plan 3 next, in the same worktree, without asking again. Tasks 4 and 5 are fully automatic. Task 6 needs a real `GEMINI_API_KEY` and a person to drive the app, so my Claude stops there, asks for the key and the manual run, and records the result. If Task 6 fails (a built-in tool shows up, or Google rejects the schema), do not start Plan 3: write a new fix plan first. Ledger: `.superpowers/sdd/2026-10-12-acp-multi-agent-plan-02/progress.md`.
