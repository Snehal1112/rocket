//! Names of node exits (`FlowEdge::source_handle`) and data-less inputs
//! (`FlowEdge::target_field`). The validator, the executor and the tests all
//! use these constants, so there is one spelling for each name.

/// The single exit of Request and Input nodes. It is the default for every edge.
pub const RESULT: &str = "result";
/// The exit an If node takes when its condition is truthy.
pub const TRUE: &str = "true";
/// The exit an If node takes when its condition is falsy.
pub const FALSE: &str = "false";
/// The exit a Switch node takes when no case matches.
pub const DEFAULT: &str = "default";
/// The single data input of If and Switch nodes.
pub const INPUT: &str = "input";
/// The data-less "Run when" input of Request and Output nodes.
pub const TRIGGER: &str = "trigger";
/// The `target_field` of a wire from an Auth node into a Request node. The
/// wire sets the request's auth. It carries no expression.
pub const AUTH: &str = "auth";
/// Prefix of a Switch case exit. The rest of the handle is the case id.
pub const CASE_PREFIX: &str = "case:";

/// Builds the exit handle for the Switch case with id `case_id`.
pub fn case_handle(case_id: &str) -> String {
    format!("{CASE_PREFIX}{case_id}")
}

/// Returns the case id inside a `case:<id>` handle. Any other handle, and a
/// handle with an empty id, returns `None`.
pub fn case_id_from_handle(handle: &str) -> Option<&str> {
    handle
        .strip_prefix(CASE_PREFIX)
        .filter(|case_id| !case_id.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_handle_prefixes_the_case_id() {
        assert_eq!(case_handle("01J9CASE"), "case:01J9CASE");
    }

    #[test]
    fn case_id_from_handle_strips_the_prefix() {
        assert_eq!(case_id_from_handle("case:01J9CASE"), Some("01J9CASE"));
    }

    #[test]
    fn case_id_from_handle_rejects_non_case_handles() {
        assert_eq!(case_id_from_handle(RESULT), None);
        assert_eq!(case_id_from_handle(DEFAULT), None);
        assert_eq!(case_id_from_handle("cases:x"), None);
    }

    #[test]
    fn case_id_from_handle_rejects_an_empty_case_id() {
        assert_eq!(case_id_from_handle("case:"), None);
    }

    #[test]
    fn handle_names_match_the_spec() {
        assert_eq!(
            [RESULT, TRUE, FALSE, DEFAULT, INPUT, TRIGGER, CASE_PREFIX],
            ["result", "true", "false", "default", "input", "trigger", "case:"]
        );
    }

    #[test]
    fn auth_handle_name_matches_the_spec() {
        assert_eq!(AUTH, "auth");
    }
}
