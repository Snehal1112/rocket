pub use rocket_shared::acp::{ConfigChoice, ConfigOption};

/// What the agent accepts inside a prompt, from its `initialize` answer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PromptCapabilities {
    /// The agent accepts embedded text resources.
    pub embedded_context: bool,
    pub image: bool,
}

/// Everything Rocket keeps from the `initialize` and `session/new` handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    /// The ACP session id, used as-is for every later call.
    pub session_id: String,
    /// Select-style options such as the model and the effort level.
    pub config_options: Vec<ConfigOption>,
    pub prompt_capabilities: PromptCapabilities,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_capabilities_default_to_nothing_supported() {
        let caps = PromptCapabilities::default();
        assert!(!caps.embedded_context);
        assert!(!caps.image);
    }

    #[test]
    fn session_info_holds_the_reported_options() {
        let info = SessionInfo {
            session_id: "s-1".to_string(),
            config_options: vec![ConfigOption {
                id: "model".to_string(),
                name: "Model".to_string(),
                category: None,
                current_value: "default".to_string(),
                choices: Vec::new(),
            }],
            prompt_capabilities: PromptCapabilities {
                embedded_context: true,
                image: false,
            },
        };
        assert_eq!(info.clone(), info);
        assert_eq!(info.config_options[0].id, "model");
    }
}
