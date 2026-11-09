/// The workspace assistant's Rocket mode. Each mode allows everything the
/// one before it allows, so the derived order (Ask < Edit < Agent) is what
/// `check_mode` compares. Serialized as "ask", "edit" and "agent".
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum AssistantMode {
    /// Read tools only. The mode of any session with no recorded mode.
    #[default]
    Ask,
    /// Read tools and proposals.
    Edit,
    /// Everything, including `run_request` in collections whose run switch
    /// is on.
    Agent,
}

impl AssistantMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ask => "Ask",
            Self::Edit => "Edit",
            Self::Agent => "Agent",
        }
    }

    /// One sentence for the agent about what this mode allows.
    pub fn summary(self) -> &'static str {
        match self {
            Self::Ask => {
                "You can read the workspace. Proposing changes and running requests are not available."
            }
            Self::Edit => {
                "You can read the workspace and propose changes. Running requests is not available."
            }
            Self::Agent => {
                "You can read the workspace, propose changes, and run requests in collections whose run switch is on."
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assistant_mode_order_is_ask_edit_agent() {
        assert!(AssistantMode::Ask < AssistantMode::Edit);
        assert!(AssistantMode::Edit < AssistantMode::Agent);
    }

    #[test]
    fn assistant_mode_default_is_ask() {
        assert_eq!(AssistantMode::default(), AssistantMode::Ask);
    }

    #[test]
    fn assistant_mode_json_strings() {
        for (mode, text) in [
            (AssistantMode::Ask, "\"ask\""),
            (AssistantMode::Edit, "\"edit\""),
            (AssistantMode::Agent, "\"agent\""),
        ] {
            assert_eq!(serde_json::to_string(&mode).expect("serialize"), text);
            let back: AssistantMode = serde_json::from_str(text).expect("deserialize");
            assert_eq!(back, mode);
        }
    }

    #[test]
    fn assistant_mode_label_and_summary_are_not_empty() {
        for mode in [
            AssistantMode::Ask,
            AssistantMode::Edit,
            AssistantMode::Agent,
        ] {
            assert!(!mode.label().is_empty());
            assert!(!mode.summary().is_empty());
        }
    }
}
