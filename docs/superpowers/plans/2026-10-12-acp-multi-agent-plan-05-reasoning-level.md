# ACP Multi-Agent Support, Plan 5: Reasoning Level and Gemini Model Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the user control over how hard the agent reasons, the same way for Claude and Gemini, and let a Gemini agent pin its model.

**Architecture:** `AgentTuning` (made in Plan 4) gains `reasoning` (`low`, `medium`, `high`, default `high`) and `model` (an optional pinned model name). At session start Rocket applies the level to Claude through the adapter's `effort` config option, and to Gemini by writing `modelConfigs` thinking overrides and `model.name` into the private settings file from Plan 2. The composer's Effort picker (it already exists) is remembered per agent, like the model. A manual run confirms the setting reaches each agent.

**Tech Stack:** Rust (rocket-acp, rocket-app, src-tauri), React + TypeScript, Vitest, Biome. Claude Code adapter `claude-agent-acp`, Gemini CLI 0.63.0.

**Spec:** `.claude/acp-multi-agent-support.md`; findings in `docs/superpowers/plans/2026-10-12-acp-multi-agent-index.md` (section "Reasoning findings"). Depends on Plans 1 to 4 being merged.

## Global Constraints

- Run cargo with `-j4`. Never `cargo test --workspace` or `--all`.
- Never call unwrap in production paths (a project hook blocks it in Rust edits). `expect("...")` is fine in tests.
- `#[serde(rename_all = "camelCase")]` only on IPC DTOs, never on `AgentConfig` or `AgentTuning`.
- Persistence stays backward compatible: every new field has `#[serde(default)]`.
- UI uses shadcn/ui primitives only, lucide-react icons only.
- Never `git add -A`, `git add .` or `--all`. Explicit paths. Commit through the `dev-workflow-skills:1-git-commit` skill (conventional commits).
- Facts the code must respect:
  - The Claude adapter exposes an `effort` config option (category `thought_level`) only when the current model supports effort. Its values include `low`, `medium`, `high`, and for some models `xhigh` and `max` and `default`. Rocket applies only values the option offers.
  - A model change can add or remove the `effort` option, so effort is applied after the model.
  - Gemini returns no config options to ACP clients. Its thinking is set only through `modelConfigs` in the settings file. Gemini 3 models take `thinkingLevel` (`LOW` or `HIGH`), Gemini 2.5 models take `thinkingBudget` (a token count). `includeThoughts: true` must stay on, or Rocket's Reasoning block (Plan 4) stays empty.
  - The Gemini overrides match the concrete model name, not the `auto` alias.
- Ruling: the default level is `high`. This changes Claude sessions that used the adapter's own default (medium on the newest models). It costs more tokens and time per turn. The dialog shows the level, and a user can set `low` or `medium`.
- No `OpenCollection` read needed: this plan does not touch collection or environment YAML.

## Review Focus

- An agent whose `effort` option offers no `high` value (a model without effort support) must start normally and send no effort change (Task 13 tests).
- The user's remembered effort must beat the config default, and a remembered value the option no longer offers must fall back to the default (Task 13 tests).
- A hand-edited `agent_configs.yml` with a model name containing a quote, a newline or a space must be refused, so it can never reach the Gemini settings file (Task 12 tests).
- The Gemini overrides must never switch `includeThoughts` off (Task 13 test).
- Existing configs (no `reasoning`, no `model`) must keep working: Claude gets `high`, Gemini gets its high level, and no model is pinned (Task 12 test).

---

### Task 12: Reasoning level and model on the agent tuning

**Files:**
- Modify: `crates/rocket-acp/src/agent_config.rs` (`ReasoningLevel`, `AgentTuning` fields and `validate`, tests)
- Modify: `crates/rocket-acp/src/lib.rs` (re-export)
- Modify: `src-tauri/src/commands/agent_configs.rs` (`AgentTuningDto` and both conversions)
- Modify: `src/lib/tauri-api.ts` (`AgentTuning`, `ReasoningLevel`)
- Test: `crates/rocket-acp/src/agent_config.rs`, `src-tauri/src/commands/agent_configs.rs`

**Interfaces:**
- Consumes: `AgentTuning { idle_timeout_secs }` (Plan 4 Task 11).
- Produces:
  - `rocket_acp::ReasoningLevel { Low, Medium, High }` (`Copy`, `Default` = `High`, serde lowercase).
  - `AgentTuning { idle_timeout_secs: Option<u32>, reasoning: ReasoningLevel, model: Option<String> }`.
  - `rocket_acp::MAX_MODEL_NAME_LEN` (128).
  - `AgentTuningDto { idle_timeout_secs, reasoning, model }` (camelCase).
  - TypeScript `type ReasoningLevel = 'low' | 'medium' | 'high'` and `AgentTuning { idleTimeoutSecs?, reasoning?, model? }`.

- [ ] **Step 1: Write the failing tests**

Add inside `tests` of `crates/rocket-acp/src/agent_config.rs`:

```rust
    #[test]
    fn reasoning_defaults_to_high_and_uses_lowercase_wire_names() {
        assert_eq!(ReasoningLevel::default(), ReasoningLevel::High);
        assert_eq!(
            serde_json::to_string(&ReasoningLevel::Medium).expect("json"),
            "\"medium\""
        );
        assert_eq!(
            serde_json::from_str::<ReasoningLevel>("\"low\"").expect("parse"),
            ReasoningLevel::Low
        );
    }

    #[test]
    fn a_tuning_saved_before_reasoning_existed_loads_as_high_with_no_model() {
        let tuning: AgentTuning =
            serde_json::from_value(serde_json::json!({ "idle_timeout_secs": 300 })).expect("old");
        assert_eq!(tuning.reasoning, ReasoningLevel::High);
        assert_eq!(tuning.model, None);
        assert_eq!(tuning.idle_timeout_secs, Some(300));
    }

    #[test]
    fn a_pinned_model_must_be_a_plain_model_name() {
        let with_model = |m: &str| AgentTuning {
            model: Some(m.to_string()),
            ..AgentTuning::default()
        };
        for good in ["gemini-2.5-pro", "gemini-3-pro-preview", "models/gemini-2.5-flash", "a_b:c.d"] {
            assert!(with_model(good).validate().is_ok(), "{good}");
        }
        for bad in ["", "   ", "has space", "quote\"d", "new\nline", "back\\slash", "{x}"] {
            let err = with_model(bad).validate().expect_err(bad);
            assert!(err.contains("tuning model"), "{err}");
            assert!(!err.contains(bad.trim()) || bad.trim().is_empty(), "never echoes the value: {err}");
        }
        let too_long = "a".repeat(MAX_MODEL_NAME_LEN + 1);
        assert!(with_model(&too_long).validate().is_err());
        assert!(with_model(&"a".repeat(MAX_MODEL_NAME_LEN)).validate().is_ok());
    }
```

In the existing Plan 4 test `the_idle_timeout_must_stay_between_30_and_1800_seconds` change the closure to build the struct with `..AgentTuning::default()`:

```rust
        let tuning = |secs: Option<u32>| AgentTuning {
            idle_timeout_secs: secs,
            ..AgentTuning::default()
        };
```

In `src-tauri/src/commands/agent_configs.rs` add inside `tests`:

```rust
    #[test]
    fn the_dto_carries_reasoning_and_model_and_old_callers_get_high() {
        let mut domain = sample_domain();
        domain.tuning.reasoning = rocket_acp::ReasoningLevel::Low;
        domain.tuning.model = Some("gemini-2.5-pro".to_string());
        let json = serde_json::to_value(AgentConfigDto::from(domain)).expect("json");
        assert_eq!(json["tuning"]["reasoning"], "low");
        assert_eq!(json["tuning"]["model"], "gemini-2.5-pro");
        let back: AgentConfig = serde_json::from_value::<AgentConfigDto>(json)
            .expect("dto")
            .into();
        assert_eq!(back.tuning.reasoning, rocket_acp::ReasoningLevel::Low);
        assert_eq!(back.tuning.model.as_deref(), Some("gemini-2.5-pro"));

        let old = serde_json::json!({
            "id": "a", "label": "A", "command": "c", "args": [],
            "credentialEnvVar": "K", "vaultConnectionId": "", "vaultName": "",
            "vaultSecretId": "", "vaultSecretName": ""
        });
        let dto: AgentConfigDto = serde_json::from_value(old).expect("old caller");
        let config = AgentConfig::from(dto);
        assert_eq!(config.tuning.reasoning, rocket_acp::ReasoningLevel::High);
        assert_eq!(config.tuning.model, None);
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p rocket-acp -j4 reasoning`
Expected: FAIL to compile (`cannot find type ReasoningLevel`).

- [ ] **Step 3: Implement**

In `agent_config.rs` add above `AgentTuning`:

```rust
/// Longest accepted pinned model name, in bytes.
pub const MAX_MODEL_NAME_LEN: usize = 128;

/// How hard the agent should reason. Rocket maps it to each agent's own
/// control: Claude's `effort` option, Gemini's thinking settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningLevel {
    Low,
    Medium,
    #[default]
    High,
}
```

Add two fields to `AgentTuning` after `idle_timeout_secs`:

```rust
    /// How hard the agent should reason. Old files without it load as `high`.
    #[serde(default)]
    pub reasoning: ReasoningLevel,
    /// A model name to pin, written into a Gemini agent's settings. `None`
    /// keeps the agent's own choice.
    #[serde(default)]
    pub model: Option<String>,
```

Extend `AgentTuning::validate` with, before `Ok(())`:

```rust
        if let Some(model) = &self.model {
            let plain = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':' | '/');
            if model.is_empty() || model.len() > MAX_MODEL_NAME_LEN || !model.chars().all(plain) {
                return Err(format!(
                    "tuning model must be 1 to {MAX_MODEL_NAME_LEN} characters: letters, digits and . _ - : /"
                ));
            }
        }
```

Add `AgentTuning` struct-literal fixes: any place that builds `AgentTuning { idle_timeout_secs: ... }` without the new fields needs `..AgentTuning::default()` or the new fields (the Plan 4 DTO conversion is replaced below).

In `crates/rocket-acp/src/lib.rs` add `ReasoningLevel, MAX_MODEL_NAME_LEN,` to the `agent_config` re-export.

In `src-tauri/src/commands/agent_configs.rs` replace `AgentTuningDto` and the two tuning conversions:

```rust
/// Per-agent tuning on the wire.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTuningDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_timeout_secs: Option<u32>,
    #[serde(default)]
    pub reasoning: rocket_acp::ReasoningLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}
```

In `From<AgentConfig> for AgentConfigDto` use:

```rust
            tuning: AgentTuningDto {
                idle_timeout_secs: c.tuning.idle_timeout_secs,
                reasoning: c.tuning.reasoning,
                model: c.tuning.model,
            },
```

In `From<AgentConfigDto> for AgentConfig` use:

```rust
            tuning: rocket_acp::AgentTuning {
                idle_timeout_secs: dto.tuning.idle_timeout_secs,
                reasoning: dto.tuning.reasoning,
                model: dto.tuning.model,
            },
```

In `src/lib/tauri-api.ts` replace the `AgentTuning` interface with:

```ts
// How hard the agent should reason. Claude's effort and Gemini's thinking follow it.
export type ReasoningLevel = 'low' | 'medium' | 'high';

// Per-agent session tuning. A missing value means the default.
export interface AgentTuning {
  // Seconds a prompt turn may stay silent before the session ends. 30 to 1800.
  idleTimeoutSecs?: number;
  // Defaults to 'high'.
  reasoning?: ReasoningLevel;
  // A model name to pin. Used by Gemini agents.
  model?: string;
}
```

- [ ] **Step 4: Run all checks for this task**

Run: `cargo test -p rocket-acp -j4 && cargo test -p rocket -j4 --lib agent_configs && cargo check -j4 && yarn tsc --noEmit && yarn check`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-acp/src/agent_config.rs crates/rocket-acp/src/lib.rs \
  src-tauri/src/commands/agent_configs.rs src/lib/tauri-api.ts
```

Then invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(acp): add reasoning level and model to the agent tuning`.

---

### Task 13: Apply the reasoning level to Claude and Gemini

**Files:**
- Modify: `src-tauri/src/commands/acp_sessions.rs` (`effort_to_apply`, `start_workspace_assistant` and `_inner` signatures, tests)
- Modify: `src-tauri/tests/acp_mcp_start_workspace_assistant.rs` (the `start` helper passes the new argument)
- Modify: `crates/rocket-app/src/session_plan.rs` (`PlanInput` fields, Gemini overrides, tests)
- Modify: `crates/rocket-app/src/acp_session_service.rs` (tests that build a `PlanInput`)
- Modify: `src/lib/tauri-api.ts` (`startWorkspaceAssistant`)
- Modify: `src/lib/assistant/assistant-session.ts`, `src/lib/assistant/model-memory.ts`
- Test: `src-tauri/src/commands/acp_sessions.rs`, `crates/rocket-app/src/session_plan.rs`, `src/lib/assistant/__tests__/model-memory.test.ts`, `src/lib/assistant/__tests__/assistant-session.test.ts`

**Interfaces:**
- Consumes: `AgentTuning.reasoning`, `AgentTuning.model` (Task 12), `PlanInput`, `session_plan`, the Gemini arm (Plan 2), `MODEL_CONFIG_ID` and `model_to_apply` (existing).
- Produces:
  - `fn effort_to_apply(options: &[ConfigOption], requested: Option<&str>, level: ReasoningLevel) -> Option<String>` and `const EFFORT_CONFIG_ID: &str = "effort"` in `acp_sessions.rs`.
  - `start_workspace_assistant(agent_config_id, mode, model, effort, ...)`: a new `effort: Option<String>` parameter after `model` (on the command and on `start_workspace_assistant_inner`).
  - `PlanInput.reasoning: ReasoningLevel` and `PlanInput.model: Option<&str>`.
  - TypeScript `startWorkspaceAssistant(agentConfigId, mode, model?, effort?)`, `loadRememberedEffort(agentConfigId)` and `rememberEffort(agentConfigId, effort)`.

- [ ] **Step 1: Write the failing `effort_to_apply` tests**

Add inside `tests` of `src-tauri/src/commands/acp_sessions.rs` (the module already has `model_option(current, choices)`):

```rust
    fn effort_option(current: &str, choices: &[&str]) -> ConfigOption {
        ConfigOption {
            id: EFFORT_CONFIG_ID.to_string(),
            name: "Effort".to_string(),
            category: Some("thought_level".to_string()),
            current_value: current.to_string(),
            choices: choices
                .iter()
                .map(|value| ConfigChoice {
                    value: value.to_string(),
                    name: value.to_string(),
                    description: None,
                })
                .collect(),
        }
    }

    #[test]
    fn effort_follows_the_configured_level_when_the_agent_offers_it() {
        let options = vec![effort_option("medium", &["low", "medium", "high", "max"])];
        assert_eq!(
            effort_to_apply(&options, None, rocket_acp::ReasoningLevel::High).as_deref(),
            Some("high")
        );
        assert_eq!(
            effort_to_apply(&options, None, rocket_acp::ReasoningLevel::Low).as_deref(),
            Some("low")
        );
        assert_eq!(
            effort_to_apply(&options, None, rocket_acp::ReasoningLevel::Medium),
            None,
            "already current"
        );
    }

    #[test]
    fn a_remembered_effort_beats_the_default_and_falls_back_when_not_offered() {
        let options = vec![effort_option("medium", &["low", "medium", "high", "max"])];
        assert_eq!(
            effort_to_apply(&options, Some("max"), rocket_acp::ReasoningLevel::High).as_deref(),
            Some("max")
        );
        assert_eq!(
            effort_to_apply(&options, Some("xhigh"), rocket_acp::ReasoningLevel::High).as_deref(),
            Some("high"),
            "a remembered value the option no longer offers falls back to the level"
        );
    }

    #[test]
    fn no_effort_is_sent_when_the_agent_has_no_effort_option_or_no_matching_value() {
        assert_eq!(
            effort_to_apply(&[], None, rocket_acp::ReasoningLevel::High),
            None
        );
        let only_model = vec![model_option("default", &["default"])];
        assert_eq!(
            effort_to_apply(&only_model, None, rocket_acp::ReasoningLevel::High),
            None
        );
        let no_high = vec![effort_option("low", &["low", "default"])];
        assert_eq!(
            effort_to_apply(&no_high, None, rocket_acp::ReasoningLevel::High),
            None
        );
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p rocket -j4 --lib effort`
Expected: FAIL to compile (`cannot find function effort_to_apply`).

- [ ] **Step 3: Implement `effort_to_apply` and apply it at session start**

In `acp_sessions.rs` add after `model_to_apply`:

```rust
/// The ACP config option id the adapter uses for the effort (thinking) level.
const EFFORT_CONFIG_ID: &str = "effort";

/// The effort value that matches a reasoning level.
fn effort_value(level: rocket_acp::ReasoningLevel) -> &'static str {
    match level {
        rocket_acp::ReasoningLevel::Low => "low",
        rocket_acp::ReasoningLevel::Medium => "medium",
        rocket_acp::ReasoningLevel::High => "high",
    }
}

/// The effort value to send right after start, or `None` when nothing should
/// be sent: the agent has no `effort` option (a model without effort
/// support, or an agent that does not offer one), the wanted value is not
/// among its choices, or it is already current. The user's remembered choice
/// wins when the option still offers it. Otherwise the configured level
/// decides.
fn effort_to_apply(
    options: &[ConfigOption],
    requested: Option<&str>,
    level: rocket_acp::ReasoningLevel,
) -> Option<String> {
    let option = options.iter().find(|o| o.id == EFFORT_CONFIG_ID)?;
    let offered = |value: &str| option.choices.iter().any(|choice| choice.value == value);
    let wanted = requested
        .filter(|value| offered(value))
        .unwrap_or_else(|| effort_value(level));
    (offered(wanted) && option.current_value != wanted).then(|| wanted.to_string())
}
```

Add `effort: Option<String>,` after `model: Option<String>,` in both `start_workspace_assistant` and `start_workspace_assistant_inner` (and pass it through in the former). In `start_workspace_assistant_inner`, right after the block that applies the model (the `if let Some(value) = model_to_apply(...)`), add:

```rust
    // Effort comes after the model, because a model change can add or remove
    // the effort option. A failed change keeps the agent's default.
    if let Some(value) =
        effort_to_apply(&config_options, effort.as_deref(), config.tuning.reasoning)
    {
        if let Ok(updated) = svc
            .set_config_option(&info.session_id, EFFORT_CONFIG_ID, &value)
            .await
        {
            config_options = updated;
        }
    }
```

In `src-tauri/tests/acp_mcp_start_workspace_assistant.rs` the helper `start` calls `start_workspace_assistant_inner("agent-1".to_string(), AssistantMode::Edit, None, ...)`: add one more `None,` right after the existing model `None,`.

- [ ] **Step 4: Run the focused tests**

Run: `cargo test -p rocket -j4 --lib acp_sessions && cargo test -p rocket -j4 --test acp_mcp_start_workspace_assistant`
Expected: PASS.

- [ ] **Step 5: Write the failing Gemini thinking tests**

Add inside `tests` of `crates/rocket-app/src/session_plan.rs` (add `use rocket_acp::ReasoningLevel;` to the module's test imports):

```rust
    fn gemini_with(reasoning: ReasoningLevel, model: Option<&str>) -> SessionPlan {
        session_plan(&PlanInput {
            kind: AgentKind::Gemini,
            config_args: &[],
            cwd: "/scratch/cwd",
            config_dir: "/scratch/config",
            instructions: "Be the workspace assistant.",
            reasoning,
            model,
        })
    }

    fn gemini_settings(plan: &SessionPlan) -> serde_json::Value {
        serde_json::from_str(&file(plan, "config/.gemini/settings.json").contents)
            .expect("settings are JSON")
    }

    fn thinking_of(settings: &serde_json::Value, model: &str) -> serde_json::Value {
        settings["modelConfigs"]["overrides"]
            .as_array()
            .expect("overrides list")
            .iter()
            .find(|o| o["match"]["model"] == model)
            .unwrap_or_else(|| panic!("no override for {model}"))["modelConfig"]
            ["generateContentConfig"]["thinkingConfig"]
            .clone()
    }

    #[test]
    fn gemini_thinking_follows_the_reasoning_level_per_model_family() {
        let high = gemini_settings(&gemini_with(ReasoningLevel::High, None));
        assert_eq!(thinking_of(&high, "gemini-3-pro-preview")["thinkingLevel"], "HIGH");
        assert_eq!(thinking_of(&high, "gemini-2.5-pro")["thinkingBudget"], 24576);

        let medium = gemini_settings(&gemini_with(ReasoningLevel::Medium, None));
        assert_eq!(thinking_of(&medium, "gemini-3-flash-preview")["thinkingLevel"], "HIGH");
        assert_eq!(thinking_of(&medium, "gemini-2.5-flash")["thinkingBudget"], 8192);

        let low = gemini_settings(&gemini_with(ReasoningLevel::Low, None));
        assert_eq!(thinking_of(&low, "gemini-3-pro-preview")["thinkingLevel"], "LOW");
        assert_eq!(thinking_of(&low, "gemini-2.5-pro")["thinkingBudget"], 1024);
    }

    #[test]
    fn every_gemini_override_keeps_thoughts_on() {
        for level in [ReasoningLevel::Low, ReasoningLevel::Medium, ReasoningLevel::High] {
            let settings = gemini_settings(&gemini_with(level, None));
            let overrides = settings["modelConfigs"]["overrides"]
                .as_array()
                .expect("overrides");
            assert!(!overrides.is_empty());
            for entry in overrides {
                assert_eq!(
                    entry["modelConfig"]["generateContentConfig"]["thinkingConfig"]["includeThoughts"],
                    true,
                    "{entry}"
                );
            }
        }
    }

    #[test]
    fn a_pinned_gemini_model_is_written_and_no_model_means_no_key() {
        let pinned = gemini_settings(&gemini_with(ReasoningLevel::High, Some("gemini-2.5-pro")));
        assert_eq!(pinned["model"]["name"], "gemini-2.5-pro");
        let free = gemini_settings(&gemini_with(ReasoningLevel::High, None));
        assert!(free.get("model").is_none(), "{free}");
    }

    #[test]
    fn the_reasoning_level_does_not_change_the_claude_plan() {
        let claude = |reasoning| {
            session_plan(&PlanInput {
                kind: AgentKind::Claude,
                config_args: &[],
                cwd: "/scratch/cwd",
                config_dir: "/scratch/config",
                instructions: "Be the workspace assistant.",
                reasoning,
                model: None,
            })
        };
        assert_eq!(claude(ReasoningLevel::Low), claude(ReasoningLevel::High));
    }
```

- [ ] **Step 6: Run them to verify they fail**

Run: `cargo test -p rocket-app -j4 session_plan`
Expected: FAIL to compile (`struct PlanInput has no field reasoning`).

- [ ] **Step 7: Implement the Gemini thinking settings**

In `session_plan.rs` add to `PlanInput` after `instructions`:

```rust
    /// How hard the agent should reason. Only Gemini reads it from the plan.
    pub reasoning: rocket_acp::ReasoningLevel,
    /// A model name to pin. Only Gemini reads it from the plan.
    pub model: Option<&'a str>,
```

Add near the other constants:

```rust
/// Gemini 3 and later models take a thinking level.
const GEMINI_LEVEL_MODELS: [&str; 5] = [
    "gemini-3-pro-preview",
    "gemini-3.1-pro-preview",
    "gemini-3-flash-preview",
    "gemini-3.5-flash",
    "gemini-3.8-flash",
];

/// Gemini 2.5 models take a thinking budget in tokens.
const GEMINI_BUDGET_MODELS: [&str; 2] = ["gemini-2.5-pro", "gemini-2.5-flash"];

fn gemini_thinking_level(level: rocket_acp::ReasoningLevel) -> &'static str {
    match level {
        rocket_acp::ReasoningLevel::Low => "LOW",
        // Gemini 3 models offer two levels, so medium and high both ask for HIGH.
        rocket_acp::ReasoningLevel::Medium | rocket_acp::ReasoningLevel::High => "HIGH",
    }
}

fn gemini_thinking_budget(level: rocket_acp::ReasoningLevel) -> u32 {
    match level {
        rocket_acp::ReasoningLevel::Low => 1024,
        rocket_acp::ReasoningLevel::Medium => 8192,
        rocket_acp::ReasoningLevel::High => 24576,
    }
}

/// One `modelConfigs` override that sets the thinking of one concrete model.
/// `includeThoughts` stays on, or the Reasoning block would stay empty.
fn thinking_override(model: &str, mut thinking: serde_json::Value) -> serde_json::Value {
    thinking["includeThoughts"] = serde_json::Value::Bool(true);
    serde_json::json!({
        "match": { "model": model },
        "modelConfig": { "generateContentConfig": { "thinkingConfig": thinking } }
    })
}
```

In `gemini_plan`, change `let settings = serde_json::json!({...});` to `let mut settings = ...` and, directly after it, add:

```rust
    let level = gemini_thinking_level(input.reasoning);
    let budget = gemini_thinking_budget(input.reasoning);
    let overrides: Vec<serde_json::Value> = GEMINI_LEVEL_MODELS
        .iter()
        .map(|model| thinking_override(model, serde_json::json!({ "thinkingLevel": level })))
        .chain(GEMINI_BUDGET_MODELS.iter().map(|model| {
            thinking_override(model, serde_json::json!({ "thinkingBudget": budget }))
        }))
        .collect();
    settings["modelConfigs"] = serde_json::json!({ "overrides": overrides });
    if let Some(model) = input.model {
        settings["model"] = serde_json::json!({ "name": model });
    }
```

Fix every other `PlanInput { ... }` literal (compiler-guided): in `session_plan.rs` tests (`input`, `gemini_input`), in `acp_session_service.rs` tests (`test_isolation()` and the workspace capture test), and in `src-tauri/src/commands/acp_sessions.rs`. For tests add `reasoning: rocket_acp::ReasoningLevel::High, model: None,`. In `acp_sessions.rs` add `reasoning: config.tuning.reasoning, model: config.tuning.model.as_deref(),`.

- [ ] **Step 8: Run the plan tests**

Run: `cargo test -p rocket-app -j4 session_plan && cargo test -p rocket-app -j4 acp_session_service && cargo check -j4`
Expected: PASS.

- [ ] **Step 9: Write the failing frontend tests**

Add inside the `describe('model memory', ...)` of `src/lib/assistant/__tests__/model-memory.test.ts` (and import `loadRememberedEffort, rememberEffort`):

```ts
  it('remembers one effort per agent config, apart from the model', () => {
    rememberModel('a1', 'opus');
    rememberEffort('a1', 'max');
    rememberEffort('a2', 'low');
    expect(loadRememberedEffort('a1')).toBe('max');
    expect(loadRememberedEffort('a2')).toBe('low');
    expect(loadRememberedEffort('a3')).toBeUndefined();
    expect(loadRememberedModel('a1')).toBe('opus');
  });

  it('survives storage that throws for the effort too', () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('denied');
    });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('denied');
    });
    expect(() => rememberEffort('a1', 'high')).not.toThrow();
    expect(loadRememberedEffort('a1')).toBeUndefined();
  });
```

In `src/lib/assistant/__tests__/assistant-session.test.ts` change the expectation at line 69 to `expect(api.startWorkspaceAssistant).toHaveBeenCalledWith('agent-1', 'ask', undefined, undefined);` and add this test next to it:

```ts
  it('passes the remembered model and effort of the agent to the start', async () => {
    vi.mocked(api.startWorkspaceAssistant).mockResolvedValue({ sessionId: 's2', configOptions: [] });
    localStorage.setItem('rocket-api:assistant-model:agent-1', 'opus');
    localStorage.setItem('rocket-api:assistant-effort:agent-1', 'max');
    await startAssistant('agent-1', 'edit');
    expect(api.startWorkspaceAssistant).toHaveBeenCalledWith('agent-1', 'edit', 'opus', 'max');
  });
```

(Use the same `startAssistant` import and store reset the neighbouring tests in that file already use.)

- [ ] **Step 10: Run them to verify they fail**

Run: `yarn test model-memory assistant-session`
Expected: FAIL (`loadRememberedEffort` is not exported; the start call has three arguments).

- [ ] **Step 11: Implement the frontend wiring**

In `src/lib/assistant/model-memory.ts` append:

```ts
const effortKeyFor = (agentConfigId: string) => `rocket-api:assistant-effort:${agentConfigId}`;

/** The effort level last chosen for this agent config, used when the next session starts. */
export function loadRememberedEffort(agentConfigId: string): string | undefined {
  try {
    return localStorage.getItem(effortKeyFor(agentConfigId)) ?? undefined;
  } catch {
    // Blocked storage means no remembered effort. The configured level is used.
    return undefined;
  }
}

export function rememberEffort(agentConfigId: string, effort: string): void {
  try {
    localStorage.setItem(effortKeyFor(agentConfigId), effort);
  } catch {
    // Blocked storage only loses the remembered choice.
  }
}
```

In `src/lib/tauri-api.ts` change `startWorkspaceAssistant` to:

```ts
export const startWorkspaceAssistant = (
  agentConfigId: string,
  mode: AssistantMode,
  model?: string,
  effort?: string,
) =>
  invoke<AgentSessionStarted>('start_workspace_assistant', {
    agentConfigId,
    mode,
    model: model ?? null,
    effort: effort ?? null,
  });
```

In `src/lib/assistant/assistant-session.ts` import `loadRememberedEffort` with `loadRememberedModel` and pass it as the fourth argument: `startWorkspaceAssistant(agentConfigId, mode, loadRememberedModel(agentConfigId), loadRememberedEffort(agentConfigId))`.

If a test in `src/lib/queries/__tests__` asserts the exact `invoke` payload of `start_workspace_assistant`, add `effort: null` to it.

- [ ] **Step 12: Run all checks for this task**

Run: `yarn test model-memory assistant-session agent-session-api && yarn tsc --noEmit && yarn check && cargo test -p rocket-app -j4 && cargo test -p rocket -j4 --lib acp_sessions && cargo check -j4`
Expected: PASS.

- [ ] **Step 13: Commit**

```bash
git add src-tauri/src/commands/acp_sessions.rs src-tauri/tests/acp_mcp_start_workspace_assistant.rs \
  crates/rocket-app/src/session_plan.rs crates/rocket-app/src/acp_session_service.rs \
  src/lib/tauri-api.ts src/lib/assistant/assistant-session.ts src/lib/assistant/model-memory.ts \
  src/lib/assistant/__tests__/model-memory.test.ts src/lib/assistant/__tests__/assistant-session.test.ts
```

Then invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(acp): apply the reasoning level to Claude effort and Gemini thinking`.

---

### Task 14: Reasoning controls in the UI, and a check that they reach the agents

**Files:**
- Modify: `src/components/settings/AgentConfigsDialog.tsx`
- Modify: `src/components/assistant/composer/Composer.tsx` (remember the effort)
- Modify: `.claude/acp-multi-agent-support.md` (append the verification log)
- Test: `src/components/settings/__tests__/AgentConfigsDialog.test.tsx`, `src/components/assistant/composer/__tests__/Composer.test.tsx`

**Interfaces:**
- Consumes: `AgentTuning.reasoning`, `AgentTuning.model` (Task 12), `rememberEffort` (Task 13), `EFFORT_OPTION_ID` from `ComposerToolbar.tsx`, the `idleTimeoutSecs` form field (Plan 4).
- Produces: a Reasoning radio group (Low, Medium, High) and a Gemini model field in the dialog; the saved `tuning` carries only non-default values.

- [ ] **Step 1: Write the failing dialog tests**

Add inside `describe('AgentConfigsDialog', ...)`:

```tsx
  it('shows High as the reasoning level of an agent that has none stored', async () => {
    const user = userEvent.setup();
    renderDialog();
    await user.click(await screen.findByRole('button', { name: 'Edit agent' }));
    await screen.findByLabelText('Label');
    expect(screen.getByRole('radio', { name: /^high/i })).toBeChecked();
  });

  it('saves a lower reasoning level under tuning and omits the default', async () => {
    const user = userEvent.setup();
    renderDialog();
    await user.click(await screen.findByRole('button', { name: 'Edit agent' }));
    await screen.findByLabelText('Label');
    await user.click(screen.getByRole('radio', { name: /^medium/i }));
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
    expect(vi.mocked(api.saveAgentConfig).mock.calls[0][0]).toMatchObject({
      tuning: { reasoning: 'medium' },
    });
  });

  it('shows a stored reasoning level', async () => {
    vi.mocked(api.listAgentConfigs).mockResolvedValue([
      { ...claude, tuning: { reasoning: 'low' } },
    ]);
    const user = userEvent.setup();
    renderDialog();
    await user.click(await screen.findByRole('button', { name: 'Edit agent' }));
    await screen.findByLabelText('Label');
    expect(screen.getByRole('radio', { name: /^low/i })).toBeChecked();
  });

  it('offers a model field for Gemini agents only and saves it under tuning', async () => {
    vi.mocked(api.listAgentConfigs).mockResolvedValue([{ ...claude, kind: 'gemini' }]);
    const user = userEvent.setup();
    renderDialog();
    await user.click(await screen.findByRole('button', { name: 'Edit agent' }));
    await screen.findByLabelText('Label');
    await user.type(screen.getByLabelText(/Gemini model/), 'gemini-2.5-pro');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
    expect(vi.mocked(api.saveAgentConfig).mock.calls[0][0]).toMatchObject({
      kind: 'gemini',
      tuning: { model: 'gemini-2.5-pro' },
    });
  });

  it('hides the model field for a Claude agent and refuses a model name with spaces', async () => {
    const user = userEvent.setup();
    renderDialog();
    await user.click(await screen.findByRole('button', { name: 'Edit agent' }));
    await screen.findByLabelText('Label');
    expect(screen.queryByLabelText(/Gemini model/)).not.toBeInTheDocument();
    await user.click(screen.getByRole('radio', { name: /^gemini cli/i }));
    await user.type(screen.getByLabelText(/Gemini model/), 'gemini 2.5');
    expect(screen.getByRole('alert')).toHaveTextContent(/letters, digits/i);
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
  });
```

- [ ] **Step 2: Run them to verify they fail**

Run: `yarn test AgentConfigsDialog`
Expected: FAIL (no radio named "High").

- [ ] **Step 3: Implement the dialog controls**

In `AgentConfigsDialog.tsx`:

1. Import `type ReasoningLevel` from `@/lib/tauri-api`.
2. Add above the component:

```tsx
const MODEL_NAME_PATTERN = /^[A-Za-z0-9._:/-]{1,128}$/;

// An empty field means no pinned model. The backend stays the authority.
function validateModelName(text: string): string | null {
  const trimmed = text.trim();
  if (trimmed === '') return null;
  return MODEL_NAME_PATTERN.test(trimmed)
    ? null
    : 'Use letters, digits and . _ - : / only, at most 128 characters.';
}

const REASONING_LEVELS: Array<{ value: ReasoningLevel; label: string; hint: string }> = [
  { value: 'low', label: 'Low', hint: 'Fast answers, little deliberation.' },
  { value: 'medium', label: 'Medium', hint: 'Balanced.' },
  { value: 'high', label: 'High', hint: 'Deepest reasoning. Best for guiding and correcting.' },
];
```

3. Add `reasoning: 'high' as ReasoningLevel,` and `model: '',` to `emptyForm`; in `startEdit` add `reasoning: c.tuning?.reasoning ?? 'high',` and `model: c.tuning?.model ?? '',`.
4. After `idleError` add `const modelError = editing && editing.kind === 'gemini' ? validateModelName(editing.model) : null;` and include `|| modelError !== null` in the `envInvalid` expression.
5. Replace the `tuning` line in `handleSave`'s `config` object with:

```tsx
      tuning: (() => {
        const idleTimeoutSecs =
          editing.idleTimeoutSecs.trim() === '' ? undefined : Number(editing.idleTimeoutSecs.trim());
        const reasoning = editing.reasoning === 'high' ? undefined : editing.reasoning;
        const model =
          editing.kind === 'gemini' && editing.model.trim() !== ''
            ? editing.model.trim()
            : undefined;
        return idleTimeoutSecs === undefined && reasoning === undefined && model === undefined
          ? undefined
          : { idleTimeoutSecs, reasoning, model };
      })(),
```

6. After the idle timeout field `<div>` add:

```tsx
            <div>
              <Label className='text-sm'>Reasoning</Label>
              <RadioGroup
                value={editing.reasoning}
                onValueChange={(value) =>
                  setEditing({ ...editing, reasoning: value as ReasoningLevel })
                }
                className='mt-1 gap-1'
              >
                {REASONING_LEVELS.map((level) => (
                  <div key={level.value} className='flex items-center gap-2'>
                    <RadioGroupItem value={level.value} id={`ac-reasoning-${level.value}`} />
                    <Label
                      htmlFor={`ac-reasoning-${level.value}`}
                      className='text-sm font-normal'
                    >
                      {level.label}{' '}
                      <span className='text-muted-foreground'>({level.hint})</span>
                    </Label>
                  </div>
                ))}
              </RadioGroup>
              <p className='mt-1 text-xs text-muted-foreground'>
                Claude uses its effort setting, Gemini its thinking budget. You can still change
                Claude's effort in the chat toolbar.
              </p>
            </div>
            {editing.kind === 'gemini' && (
              <div>
                <Label htmlFor='ac-gemini-model' className='text-sm'>
                  Gemini model <span className='text-muted-foreground'>(optional)</span>
                </Label>
                <Input
                  id='ac-gemini-model'
                  value={editing.model}
                  onChange={(e) => setEditing({ ...editing, model: e.target.value })}
                  placeholder='auto'
                  className='h-8 text-sm'
                  aria-invalid={modelError !== null}
                  autoComplete='off'
                  spellCheck={false}
                />
                {modelError ? (
                  <p role='alert' className='mt-1 text-xs text-destructive'>
                    {modelError}
                  </p>
                ) : (
                  <p className='mt-1 text-xs text-muted-foreground'>
                    Leave empty to let Gemini choose. Example: gemini-2.5-pro.
                  </p>
                )}
              </div>
            )}
```

- [ ] **Step 4: Run the dialog tests**

Run: `yarn test AgentConfigsDialog`
Expected: PASS (the new tests and every existing one, because the default level adds no `tuning` to the payload).

- [ ] **Step 5: Write the failing composer test**

In `src/components/assistant/composer/__tests__/Composer.test.tsx` change the import on line 6 to `import { loadRememberedEffort, loadRememberedModel } from '@/lib/assistant/model-memory';` and add this test after `changes the model, stores the new options and remembers the choice`:

```tsx
  it('remembers the effort the user picks, apart from the model', async () => {
    const effort = {
      id: 'effort',
      name: 'Effort',
      currentValue: 'medium',
      choices: [
        { value: 'medium', name: 'Medium' },
        { value: 'max', name: 'Max' },
      ],
    } as ConfigOption;
    useAssistantStore.setState({
      session: {
        sessionId: 's1',
        agentConfigId: 'a1',
        status: 'active',
        configOptions: [MODEL, effort],
        mode: 'ask',
      },
    });
    vi.mocked(setAgentConfigOption).mockResolvedValue([
      MODEL,
      { ...effort, currentValue: 'max' },
    ]);
    const { user } = renderComposer();
    await user.click(screen.getByRole('button', { name: 'Effort: Medium' }));
    await user.click(screen.getByRole('menuitemradio', { name: /Max/ }));
    expect(setAgentConfigOption).toHaveBeenCalledWith('s1', 'effort', 'max');
    expect(await screen.findByRole('button', { name: 'Effort: Max' })).toBeInTheDocument();
    expect(loadRememberedEffort('a1')).toBe('max');
    expect(loadRememberedModel('a1')).toBeUndefined();
  });
```

- [ ] **Step 6: Run it to verify it fails**

Run: `yarn test Composer`
Expected: FAIL (the effort key is `null`).

- [ ] **Step 7: Remember the effort**

In `Composer.tsx` change the import to `import { rememberEffort, rememberModel } from '@/lib/assistant/model-memory';`, import `EFFORT_OPTION_ID` from `./ComposerToolbar` next to `MODEL_OPTION_ID`, and in `handleConfigChange` after the `rememberModel` line add:

```ts
      if (configId === EFFORT_OPTION_ID && agentConfigId) rememberEffort(agentConfigId, value);
```

- [ ] **Step 8: Run all automated checks for the plan**

Run: `yarn test AgentConfigsDialog Composer model-memory assistant-session && yarn tsc --noEmit && yarn check && cargo test -p rocket-acp -j4 && cargo test -p rocket-app -j4 && cargo test -p rocket -j4 --lib acp_sessions && cargo check -j4`
Expected: all PASS.

- [ ] **Step 9: Commit the UI**

```bash
git add src/components/settings/AgentConfigsDialog.tsx src/components/assistant/composer/Composer.tsx \
  src/components/settings/__tests__/AgentConfigsDialog.test.tsx \
  src/components/assistant/composer/__tests__/Composer.test.tsx
```

Then invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(assistant): reasoning level and Gemini model controls`.

- [ ] **Step 10: Confirm the Gemini thinking setting reaches the API (no key needed)**

Write this script to `/tmp/claude-1000/fake-gemini-api.py` (it only logs what Gemini would send to Google and answers with an error):

```python
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

LOG = sys.argv[2]


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_POST(self):
        length = int(self.headers.get("Content-Length") or 0)
        raw = self.rfile.read(length).decode("utf-8", "replace")
        try:
            body = json.loads(raw)
        except ValueError:
            body = {}
        config = body.get("generationConfig") or {}
        model = self.path.rsplit("/", 1)[-1].split(":")[0]
        with open(LOG, "a") as out:
            out.write(json.dumps({"model": model, "thinkingConfig": config.get("thinkingConfig")}) + "\n")
        self.send_response(500)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(b'{"error":{"code":500,"message":"fake endpoint, request logged","status":"INTERNAL"}}')


ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), Handler).serve_forever()
```

Run it: `python3 -I /tmp/claude-1000/fake-gemini-api.py 8765 /tmp/claude-1000/fake-gemini.log &`. In Rocket (`yarn tauri dev`) add a Gemini agent: credential source `Rocket environment variable`, variable `GEMINI_API_KEY` exported as `dummy` in the shell that starts Rocket, an extra environment variable `GOOGLE_GEMINI_BASE_URL` = `http://127.0.0.1:8765`, Gemini model `gemini-2.5-pro`, Reasoning `High`. Start the assistant and send `hello`.
Expected: `/tmp/claude-1000/fake-gemini.log` shows a line with `"model": "gemini-2.5-pro"` and `"thinkingConfig": {"includeThoughts": true, "thinkingBudget": 24576}`. Change the agent to Reasoning `Low`, start a new session, send `hello`: the line shows `"thinkingBudget": 1024`. Change the model to `gemini-3-pro-preview`: the line shows `"thinkingLevel": "HIGH"` for High and `"LOW"` for Low.
If the thinking config is missing or wrong, STOP: the override shape in `thinking_override` does not match Gemini 0.63. Read `$(npm root -g)/@google/gemini-cli/bundle/docs/cli/generation-settings.md` (the override examples near the end) and fix the JSON shape and its tests. Do not record a pass.
Stop the fake server when done: `kill %1`.

- [ ] **Step 11: Check Claude and the thinking stream with real keys (manual)**

With a real Claude agent: start the assistant, open the toolbar's Effort picker. Expected: it shows `high` right after start (before any change), unless the model has no effort option. Pick `max`, end the session, start a new one. Expected: the picker shows `max` again (remembered). Ask a hard question, for example `Review the login request and tell me what could be wrong with its auth`. Record whether the Reasoning block appears and has text. Recent Claude models may send an empty thinking text by default, so an empty block is a known limit, not a failure: record it.
With a real Gemini agent (`GEMINI_API_KEY`): ask the same question. Expected: the Reasoning block fills with Gemini's thought summaries while it works.

- [ ] **Step 12: Record the results**

Append to `.claude/acp-multi-agent-support.md` under the "Verification log" from Plan 2:

```markdown
- <date>, Plan 5: Gemini thinkingConfig reaches the API <pass|fail: log lines>, Claude effort picker starts at high and is remembered <pass|fail>, Reasoning block text <Claude: shown|empty|n/a> <Gemini: shown|empty|n/a>.
```

Fill every angle bracket with the real result, then commit with `git add .claude/acp-multi-agent-support.md` and the `dev-workflow-skills:1-git-commit` skill (suggested subject: `docs(acp): record the reasoning verification run`).

---

## Next plan

**Next: Plan 6, guidance quality** (`docs/superpowers/plans/2026-10-12-acp-multi-agent-plan-06-guidance-quality.md`).

When Tasks 12 and 13 are committed with green checks and the Task 14 steps 1 to 10 pass, my Claude executes Plan 6 next, in the same worktree, without asking again. Steps 11 and 12 of Task 14 need real keys and a person at the app: my Claude asks for them and records the result, but does not wait on them to start Plan 6 unless step 10 failed. If step 10 failed, fix the Gemini override shape first. Ledger: `.superpowers/sdd/2026-10-12-acp-multi-agent-plan-05/progress.md`.
