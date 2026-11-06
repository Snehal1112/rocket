//! What a stranger sees of a node: a display name and initials.

use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

use crate::limits::DISPLAY_NAME_MAX_CHARS;

const FALLBACK_NAME: &str = "Rocket user";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeProfile {
    pub display_name: String,
    pub initials: String,
}

impl NodeProfile {
    /// Trims the name, checks it and builds the initials.
    pub fn new(display_name: &str) -> DomainResult<NodeProfile> {
        let name = display_name.trim();
        let count = name.chars().count();
        if count == 0 || count > DISPLAY_NAME_MAX_CHARS {
            return Err(DomainError::InvalidInput(format!(
                "display name must be 1 to {DISPLAY_NAME_MAX_CHARS} characters"
            )));
        }
        if name.chars().any(char::is_control) {
            return Err(DomainError::InvalidInput(
                "display name has control characters".to_string(),
            ));
        }
        Ok(NodeProfile {
            display_name: name.to_string(),
            initials: initials_for(name),
        })
    }

    /// Re-checks a profile that came from a peer. The initials are rebuilt from
    /// the name, so a peer cannot choose them.
    pub fn validated(&self) -> DomainResult<NodeProfile> {
        NodeProfile::new(&self.display_name)
    }
}

/// The first character of the first two words, uppercased.
pub fn initials_for(display_name: &str) -> String {
    display_name
        .split_whitespace()
        .take(2)
        .filter_map(|word| word.chars().next())
        .flat_map(|c| c.to_uppercase())
        .collect()
}

/// The OS user name, or "Rocket user" when it is missing or unusable.
pub fn default_display_name(user: Option<&str>) -> String {
    let candidate = user.map(str::trim).unwrap_or("");
    if candidate.is_empty() || candidate.chars().any(char::is_control) {
        return FALLBACK_NAME.to_string();
    }
    let cut: String = candidate.chars().take(DISPLAY_NAME_MAX_CHARS).collect();
    cut.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::error::DomainError;

    #[test]
    fn new_trims_and_builds_initials() {
        let profile = NodeProfile::new("  Ada Lovelace  ").expect("valid");
        assert_eq!(profile.display_name, "Ada Lovelace");
        assert_eq!(profile.initials, "AL");
    }

    #[test]
    fn new_rejects_empty_long_and_control_names() {
        for bad in ["", "   ", "Ada\u{0}", "Ada\nLovelace", "tab\there"] {
            let err = NodeProfile::new(bad).expect_err("rejected");
            assert!(matches!(err, DomainError::InvalidInput(_)), "input {bad:?}");
        }
        let too_long = "a".repeat(DISPLAY_NAME_MAX_CHARS + 1);
        assert!(NodeProfile::new(&too_long).is_err());
        let at_limit = "a".repeat(DISPLAY_NAME_MAX_CHARS);
        assert!(NodeProfile::new(&at_limit).is_ok());
    }

    #[test]
    fn length_counts_characters_not_bytes() {
        let name = "é".repeat(DISPLAY_NAME_MAX_CHARS);
        assert!(NodeProfile::new(&name).is_ok());
    }

    #[test]
    fn initials_use_the_first_two_words() {
        assert_eq!(initials_for("ada lovelace"), "AL");
        assert_eq!(initials_for("Ada"), "A");
        assert_eq!(initials_for("Ada Byron King"), "AB");
        assert_eq!(initials_for("   "), "");
        assert_eq!(initials_for("éva  östen"), "ÉÖ");
    }

    #[test]
    fn default_name_uses_the_os_user_or_falls_back() {
        assert_eq!(default_display_name(Some(" sneha ")), "sneha");
        assert_eq!(default_display_name(None), "Rocket user");
        assert_eq!(default_display_name(Some("")), "Rocket user");
        assert_eq!(default_display_name(Some("  ")), "Rocket user");
        assert_eq!(default_display_name(Some("bad\u{7}name")), "Rocket user");
    }

    #[test]
    fn default_name_is_cut_to_the_limit() {
        let long = "b".repeat(60);
        let name = default_display_name(Some(&long));
        assert_eq!(name.chars().count(), DISPLAY_NAME_MAX_CHARS);
        assert!(NodeProfile::new(&name).is_ok());
    }

    #[test]
    fn validated_rebuilds_initials_and_rejects_bad_names() {
        let forged = NodeProfile {
            display_name: "Ada Lovelace".to_string(),
            initials: "ZZZZ".to_string(),
        };
        let clean = forged.validated().expect("valid name");
        assert_eq!(clean.initials, "AL");
        let bad = NodeProfile {
            display_name: "x\u{1b}[31m".to_string(),
            initials: "X".to_string(),
        };
        assert!(bad.validated().is_err());
    }

    #[test]
    fn profile_json_is_snake_case() {
        let profile = NodeProfile::new("Ada Lovelace").expect("valid");
        let value = serde_json::to_value(&profile).expect("serializes");
        assert_eq!(
            value,
            serde_json::json!({"display_name": "Ada Lovelace", "initials": "AL"})
        );
    }
}
