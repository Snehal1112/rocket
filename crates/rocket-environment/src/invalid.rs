//! The one way this crate builds an invalid input error.

use rocket_shared::error::DomainError;

/// Builds `DomainError::InvalidInput` from any text.
pub(crate) fn invalid(message: impl Into<String>) -> DomainError {
    DomainError::InvalidInput(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_wraps_str_and_string_in_invalid_input() {
        assert!(matches!(invalid("a"), DomainError::InvalidInput(m) if m == "a"));
        assert!(matches!(
            invalid(format!("b{}", 1)),
            DomainError::InvalidInput(m) if m == "b1"
        ));
    }
}
