//! Pure, per-agent-kind setup for one assistant session. It decides the
//! extra args, the isolation env, the `session/new` `_meta`, the files to
//! write into the scratch directory and the text to put in front of the first
//! prompt. It does no I/O: `src-tauri` writes the files.

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
}
