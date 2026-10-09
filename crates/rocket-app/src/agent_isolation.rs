//! Pure builders for an isolated agent session. The adapter reads these
//! values from `session/new` `_meta` and from the process environment. This
//! module does no I/O; `src-tauri` creates the directories it names.

/// The environment variable that points Claude Code at its config directory.
pub const ISOLATION_ENV_CONFIG_DIR: &str = "CLAUDE_CONFIG_DIR";

/// The name Rocket's MCP server is registered under in `session/new`.
pub const ROCKET_MCP_SERVER_NAME: &str = "rocket";

/// Claude Code names MCP tools `mcp__<server>__<tool>`. This pattern allows
/// every Rocket tool in advance, so none of them asks for permission.
pub const ROCKET_MCP_TOOL_PATTERN: &str = "mcp__rocket__*";

/// Appended to the adapter's `claude_code` system prompt preset.
pub const ROCKET_ASSISTANT_SYSTEM_PROMPT: &str = "You are Rocket's API assistant. \
You run inside Rocket, a desktop API client, and you help the user understand, write and fix \
HTTP requests, scripts and tests in their Rocket workspace. \
You have only the Rocket tools, whose names start with mcp__rocket__. \
You have no shell, no file system and no web access. \
Do not assume that any files exist, and do not try to read, write or list files. \
When you need workspace data, call a Rocket tool. \
When no Rocket tool can answer, say what you could not check instead of guessing.";

/// Builds the `_meta` object for `session/new`. `settingSources: []` drops
/// user, project and local settings, `tools: []` drops every built-in tool,
/// and `allowDangerouslySkipPermissions` must be the boolean `false`, because
/// the adapter disables bypass mode only for that exact value.
pub fn isolation_meta(system_prompt_append: &str) -> serde_json::Value {
    serde_json::json!({
        "claudeCode": {
            "options": {
                "settingSources": [],
                "strictMcpConfig": true,
                "tools": [],
                "allowedTools": [ROCKET_MCP_TOOL_PATTERN],
                "allowDangerouslySkipPermissions": false
            }
        },
        "systemPrompt": { "append": system_prompt_append }
    })
}

/// The per-session isolation inputs the command layer hands to
/// `AcpSessionService::start_session`. `config_dir` is an empty directory
/// that becomes the agent's `CLAUDE_CONFIG_DIR`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionIsolation {
    pub config_dir: String,
    pub system_prompt_append: String,
}

impl SessionIsolation {
    /// Uses the default Rocket system prompt.
    pub fn new(config_dir: impl Into<String>) -> Self {
        Self {
            config_dir: config_dir.into(),
            system_prompt_append: ROCKET_ASSISTANT_SYSTEM_PROMPT.to_string(),
        }
    }

    /// The `_meta` value for `session/new`.
    pub fn meta(&self) -> serde_json::Value {
        isolation_meta(&self.system_prompt_append)
    }

    /// The `CLAUDE_CONFIG_DIR` entry for the agent's environment.
    pub fn env_entry(&self) -> (String, String) {
        (
            ISOLATION_ENV_CONFIG_DIR.to_string(),
            self.config_dir.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolation_meta_matches_the_locked_shape() {
        let meta = isolation_meta("Be brief.");
        assert_eq!(
            meta,
            serde_json::json!({
                "claudeCode": {
                    "options": {
                        "settingSources": [],
                        "strictMcpConfig": true,
                        "tools": [],
                        "allowedTools": ["mcp__rocket__*"],
                        "allowDangerouslySkipPermissions": false
                    }
                },
                "systemPrompt": { "append": "Be brief." }
            })
        );
    }

    #[test]
    fn system_prompt_is_an_append_object_never_a_bare_string() {
        // A bare string would replace the whole claude_code preset in the adapter.
        let meta = isolation_meta("x");
        assert!(meta["systemPrompt"].is_object());
        assert_eq!(meta["systemPrompt"]["append"], "x");
    }

    #[test]
    fn allowed_tools_pattern_follows_the_mcp_server_name() {
        assert_eq!(
            ROCKET_MCP_TOOL_PATTERN,
            format!("mcp__{ROCKET_MCP_SERVER_NAME}__*")
        );
    }

    #[test]
    fn default_prompt_names_rocket_and_its_tools_and_forbids_file_assumptions() {
        assert!(ROCKET_ASSISTANT_SYSTEM_PROMPT.contains("Rocket"));
        assert!(ROCKET_ASSISTANT_SYSTEM_PROMPT.contains("mcp__rocket__"));
        assert!(ROCKET_ASSISTANT_SYSTEM_PROMPT.contains("Do not assume that any files exist"));
    }

    #[test]
    fn session_isolation_builds_meta_and_the_config_dir_env_entry() {
        let isolation = SessionIsolation::new("/scratch/config");
        assert_eq!(
            isolation.system_prompt_append,
            ROCKET_ASSISTANT_SYSTEM_PROMPT
        );
        assert_eq!(
            isolation.meta(),
            isolation_meta(ROCKET_ASSISTANT_SYSTEM_PROMPT)
        );
        assert_eq!(
            isolation.env_entry(),
            (
                "CLAUDE_CONFIG_DIR".to_string(),
                "/scratch/config".to_string()
            )
        );
    }
}
