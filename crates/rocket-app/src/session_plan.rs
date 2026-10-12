//! Pure, per-agent-kind setup for one assistant session. It decides the
//! extra args, the isolation env, the `session/new` `_meta`, the files to
//! write into the scratch directory and the text to put in front of the first
//! prompt. It does no I/O: `src-tauri` writes the files.

use rocket_acp::AgentKind;

use crate::agent_isolation::{SessionIsolation, ROCKET_MCP_SERVER_NAME};

/// Tells an agent that has no Rocket-specific lockdown how to behave. It names
/// the tool that edits requests, because such an agent may reach for files.
pub const NON_CLAUDE_TOOL_NOTICE: &str = "Use only the tools of the rocket MCP server. \
Do not try to read, write or list files, and do not run shell commands. To change a request, \
a folder's settings, a script or an environment variable, call propose_changes, which needs \
Edit mode. A request's documentation is the docs field of an update_request change.";

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
        AgentKind::Gemini => gemini_plan(input),
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

fn generic_plan(input: &PlanInput<'_>) -> SessionPlan {
    SessionPlan {
        args: Vec::new(),
        env: Vec::new(),
        meta: None,
        files: Vec::new(),
        first_prompt_prefix: Some(format!(
            "{}\n\n{NON_CLAUDE_TOOL_NOTICE}",
            input.instructions
        )),
    }
}

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
        assert_eq!(
            plan.meta,
            Some(isolation_meta("Be the workspace assistant."))
        );
        assert_eq!(
            plan.env,
            vec![(
                "CLAUDE_CONFIG_DIR".to_string(),
                "/scratch/config".to_string()
            )]
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
        let prefix = plan
            .first_prompt_prefix
            .expect("a generic agent needs the prefix");
        assert!(
            prefix.starts_with("Be the workspace assistant."),
            "{prefix}"
        );
        assert!(prefix.contains("propose_changes"), "{prefix}");
        assert!(
            prefix.contains("Do not try to read, write or list files"),
            "{prefix}"
        );
        assert!(!prefix.contains("CLAUDE_CONFIG_DIR"));
    }

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
        let settings: serde_json::Value =
            serde_json::from_str(&file(&plan, "config/.gemini/settings.json").contents)
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
        assert!(
            core.iter().all(|name| !name.contains('*')),
            "no wildcard: {core:?}"
        );
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
}
