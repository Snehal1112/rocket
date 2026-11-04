//! A path relative to a collection root that is safe inside a file transaction.

use std::fmt;

use rocket_shared::error::{DomainError, DomainResult};

use crate::file_tx::OWNED_STATE_FILES;

/// Names that Windows treats as devices, with or without an extension.
const RESERVED_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// A relative path with `/` separators that a transaction may touch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelPath(String);

impl RelPath {
    /// Fails with `InvalidInput` for an empty, absolute or escaping path, a Windows device
    /// name, a colon, or a file in `.rocket` that this feature does not own.
    pub fn parse(rel: &str) -> DomainResult<Self> {
        let bad = |why: &str| Err(DomainError::InvalidInput(format!("path \"{rel}\" {why}")));
        if rel.is_empty() {
            return bad("is empty");
        }
        if rel.contains('\\') || rel.contains('\0') {
            return bad("has a backslash or a NUL byte");
        }
        if rel.starts_with('/') {
            return bad("is absolute");
        }
        let parts: Vec<&str> = rel.split('/').collect();
        if parts
            .iter()
            .any(|p| p.is_empty() || *p == "." || *p == "..")
        {
            return bad("has an empty, \".\" or \"..\" part");
        }
        for part in &parts {
            if part.contains(':') {
                return bad("has a part with a colon");
            }
            let stem = part.split('.').next().unwrap_or("").trim_end();
            if RESERVED_NAMES
                .iter()
                .any(|name| name.eq_ignore_ascii_case(stem))
            {
                return bad("has a part that Windows reserves as a device name");
            }
        }
        if parts[0].eq_ignore_ascii_case(".rocket")
            && !(parts[0] == ".rocket" && parts.len() == 2 && OWNED_STATE_FILES.contains(&parts[1]))
        {
            return bad("is inside .rocket and is not a file this feature owns");
        }
        Ok(Self(rel.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for RelPath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_tx::OWNED_STATE_FILES;

    fn text(rel: &str) -> String {
        RelPath::parse(rel).expect_err("rejected").to_string()
    }

    #[test]
    fn parse_accepts_nested_paths() {
        assert!(RelPath::parse("requests/login.yml").is_ok());
        for file in OWNED_STATE_FILES {
            assert!(RelPath::parse(&format!(".rocket/{file}")).is_ok(), "{file}");
        }
    }

    #[test]
    fn parse_rejects_empty() {
        assert!(RelPath::parse("").is_err());
    }

    #[test]
    fn parse_rejects_backslash_and_nul() {
        assert!(RelPath::parse("a\\b").is_err());
        assert!(RelPath::parse("a\0b").is_err());
    }

    #[test]
    fn parse_rejects_absolute() {
        assert!(RelPath::parse("/etc/passwd").is_err());
    }

    #[test]
    fn parse_rejects_dot_parts() {
        for rel in ["a//b", "./a", "a/../b", "a/", "a/."] {
            assert!(RelPath::parse(rel).is_err(), "{rel}");
        }
    }

    #[test]
    fn parse_rejects_a_colon() {
        assert!(RelPath::parse("a/C:x").is_err());
    }

    #[test]
    fn parse_rejects_windows_device_names() {
        assert!(RelPath::parse("CON").is_err());
        assert!(RelPath::parse("a/nul.txt").is_err());
        assert!(RelPath::parse("a/Com1 .yml").is_err());
    }

    #[test]
    fn parse_rejects_foreign_rocket_files() {
        assert!(RelPath::parse(".rocket/other.txt").is_err());
        assert!(RelPath::parse(".ROCKET/x").is_err());
        assert!(RelPath::parse(".rocket/a/openapi-source.yml").is_err());
    }

    #[test]
    fn parse_keeps_the_error_text() {
        assert!(text("a/../b").contains("path \"a/../b\" has an empty, \".\" or \"..\" part"));
    }

    #[test]
    fn parse_keeps_the_text() {
        let rel = RelPath::parse("a/b.yml").expect("valid");
        assert_eq!(rel.as_str(), "a/b.yml");
        assert_eq!(rel.to_string(), "a/b.yml");
    }
}
