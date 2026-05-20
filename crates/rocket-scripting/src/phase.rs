use serde::{Deserialize, Serialize};

/// Identifies which lifecycle hook a script belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScriptPhase {
    /// Runs before the HTTP request is sent. `req` is writable, `res` is unavailable.
    BeforeRequest,
    /// Runs after the HTTP response is received. `res` is read-only, `req` mutations are rejected.
    AfterResponse,
    /// Runs after the after-response phase. `res` is read-only. `test()` + `expect()` are available.
    Tests,
}

impl ScriptPhase {
    /// Returns the canonical string name used in `OcScript.script_type`.
    pub fn as_str(&self) -> &'static str {
        match self {
            ScriptPhase::BeforeRequest => "before-request",
            ScriptPhase::AfterResponse => "after-response",
            ScriptPhase::Tests => "tests",
        }
    }
}

impl std::fmt::Display for ScriptPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_as_str() {
        assert_eq!(ScriptPhase::BeforeRequest.as_str(), "before-request");
        assert_eq!(ScriptPhase::AfterResponse.as_str(), "after-response");
        assert_eq!(ScriptPhase::Tests.as_str(), "tests");
    }

    #[test]
    fn phase_display() {
        assert_eq!(ScriptPhase::BeforeRequest.to_string(), "before-request");
        assert_eq!(ScriptPhase::AfterResponse.to_string(), "after-response");
        assert_eq!(ScriptPhase::Tests.to_string(), "tests");
    }
}
