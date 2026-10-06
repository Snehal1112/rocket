use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

/// Maximum length of a script file name, including `.js`.
const MAX_NAME_LEN: usize = 100;

/// Starter content written into a new script file.
pub const SCRIPT_TEMPLATE: &str = "// Shared helpers. Load them from any script tab with:\n\
// const { greet } = require('./THIS_FILE.js');\n\
\n\
const greet = (name) => `Hello, ${name}`;\n\
\n\
module.exports = {\n  greet,\n};\n";

/// A `.js` file in a collection folder. The file itself is the source of truth,
/// so there is no uid and nothing is persisted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptFileItem {
    /// On-disk file name, for example `utils.js`.
    pub file_name: String,
    /// Display name shown in the sidebar.
    pub name: String,
}

impl ScriptFileItem {
    pub fn new(file_name: &str) -> Self {
        Self {
            file_name: file_name.to_string(),
            name: file_name.to_string(),
        }
    }
}

/// Validates a user-typed script name and returns the safe file name.
///
/// Appends `.js` when missing. Rejects empty names, path separators, a leading
/// dot, `..`, NUL bytes and names over 100 characters.
pub fn normalize_script_name(raw: &str) -> DomainResult<String> {
    let trimmed = raw.trim();
    let bad = |msg: &str| DomainError::InvalidInput(format!("Invalid script name: {msg}"));
    if trimmed.is_empty() {
        return Err(bad("name is empty"));
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains('\0') {
        return Err(bad("name must not contain path separators"));
    }
    if trimmed.starts_with('.') || trimmed.contains("..") {
        return Err(bad("name must not start with a dot or contain '..'"));
    }
    let file_name = if trimmed.ends_with(".js") {
        trimmed.to_string()
    } else {
        format!("{trimmed}.js")
    };
    if file_name.len() > MAX_NAME_LEN {
        return Err(bad("name is too long"));
    }
    Ok(file_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_js_when_missing() {
        assert_eq!(normalize_script_name("utils").expect("ok"), "utils.js");
        assert_eq!(normalize_script_name("utils.js").expect("ok"), "utils.js");
        assert_eq!(
            normalize_script_name("  my lib  ").expect("ok"),
            "my lib.js"
        );
    }

    #[test]
    fn rejects_unsafe_names() {
        for bad in [
            "",
            "   ",
            ".js",
            "..",
            "../evil",
            "a/b",
            "a\\b",
            ".hidden",
            "bad\0name",
            "..js",
        ] {
            assert!(normalize_script_name(bad).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn rejects_overlong_names() {
        let long = "a".repeat(200);
        assert!(normalize_script_name(&long).is_err());
    }

    #[test]
    fn script_file_item_wire_shape() {
        let item = ScriptFileItem::new("utils.js");
        let json = serde_json::to_string(&crate::CollectionItem::ScriptFile(item)).expect("ser");
        assert_eq!(
            json,
            r#"{"type":"scriptFile","fileName":"utils.js","name":"utils.js"}"#
        );
    }

    #[test]
    fn template_exports_something() {
        assert!(SCRIPT_TEMPLATE.contains("module.exports"));
    }
}
