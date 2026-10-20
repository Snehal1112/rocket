//! Who caused an audited action, derived from where the action came from.

/// The origin of an audited action. Stored in the log as its `as_str` text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditActor {
    /// A person acting through the app UI (an IPC command).
    User,
    /// A user script, such as an environment write from `rok.setEnvVar`.
    Script,
    /// An ACP agent or an MCP tool call.
    Agent,
    /// Internal housekeeping, or an origin that could not be determined.
    System,
}

impl AuditActor {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Script => "script",
            Self::Agent => "agent",
            Self::System => "system",
        }
    }
}

impl From<AuditActor> for String {
    fn from(actor: AuditActor) -> Self {
        actor.as_str().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_text_is_stable() {
        assert_eq!(AuditActor::User.as_str(), "user");
        assert_eq!(AuditActor::Script.as_str(), "script");
        assert_eq!(AuditActor::Agent.as_str(), "agent");
        assert_eq!(AuditActor::System.as_str(), "system");
        assert_eq!(String::from(AuditActor::Agent), "agent");
    }
}
