use crate::session_info::ConfigOption;

/// One typed update from the agent during a prompt turn. It carries no
/// `agent-client-protocol` types, so this crate stays protocol-free.
#[derive(Debug, Clone, PartialEq)]
pub enum AcpUpdate {
    /// A chunk of the agent's reply text.
    Text { text: String },
    /// The agent started a tool call. `kind` is the ACP tool kind in
    /// snake_case, for example `read`, `execute` or `other`.
    ToolCall {
        call_id: String,
        title: String,
        kind: String,
        status: ToolCallStatus,
    },
    /// A change to an earlier tool call. A field the agent left out is `None`.
    ToolCallUpdate {
        call_id: String,
        title: Option<String>,
        status: Option<ToolCallStatus>,
    },
    /// The agent's full, current list of session options.
    ConfigOptions { options: Vec<ConfigOption> },
    /// Context window use, and the session cost when the agent reports it in US dollars.
    Usage {
        used: u64,
        size: u64,
        cost_usd: Option<f64>,
    },
}

/// Progress of one tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCallStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

impl ToolCallStatus {
    /// The snake_case name used on events and in the frontend.
    pub fn as_str(self) -> &'static str {
        match self {
            ToolCallStatus::Pending => "pending",
            ToolCallStatus::InProgress => "in_progress",
            ToolCallStatus::Completed => "completed",
            ToolCallStatus::Failed => "failed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_call_status_has_snake_case_wire_names() {
        assert_eq!(ToolCallStatus::Pending.as_str(), "pending");
        assert_eq!(ToolCallStatus::InProgress.as_str(), "in_progress");
        assert_eq!(ToolCallStatus::Completed.as_str(), "completed");
        assert_eq!(ToolCallStatus::Failed.as_str(), "failed");
    }

    #[test]
    fn updates_compare_by_value() {
        let options: Vec<ConfigOption> = Vec::new();
        assert_eq!(
            AcpUpdate::ConfigOptions {
                options: options.clone()
            },
            AcpUpdate::ConfigOptions { options }
        );
        assert_ne!(
            AcpUpdate::Text {
                text: "a".to_string()
            },
            AcpUpdate::Text {
                text: "b".to_string()
            }
        );
    }
}
