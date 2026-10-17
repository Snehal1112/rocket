use crate::error::{DomainError, DomainResult};

/// Fails with `message` as `InvalidInput` when the name is empty or only whitespace.
pub fn non_empty_name(name: &str, message: &str) -> DomainResult<()> {
    if name.trim().is_empty() {
        return Err(DomainError::InvalidInput(message.to_string()));
    }
    Ok(())
}

/// True for ASCII letters, digits, `_` and `-`.
pub fn is_slug_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::DomainError;

    #[test]
    fn non_empty_name_rejects_blank_and_whitespace() {
        for name in ["", "   ", "\t\n"] {
            match non_empty_name(name, "Name cannot be empty") {
                Err(DomainError::InvalidInput(m)) => assert_eq!(m, "Name cannot be empty"),
                other => panic!("expected InvalidInput, got {other:?}"),
            }
        }
    }

    #[test]
    fn non_empty_name_accepts_text() {
        assert!(non_empty_name("a b/c", "msg").is_ok());
    }

    #[test]
    fn is_slug_char_accepts_alnum_underscore_dash() {
        assert!("azAZ09_-".chars().all(is_slug_char));
    }

    #[test]
    fn is_slug_char_rejects_dot_space_slash_and_non_ascii() {
        for c in ['.', ' ', '/', 'é'] {
            assert!(!is_slug_char(c), "{c:?}");
        }
    }
}
