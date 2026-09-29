//! Masks secret values and sensitive headers in text shown to the user.

use std::collections::HashSet;

/// The text that replaces a masked value.
pub(crate) const REDACTED: &str = "••••••";

/// Secrets shorter than this are left alone, matching the script engine.
pub(crate) const MIN_REDACTION_LEN: usize = 6;

/// Replaces every secret value in `text` with `REDACTED`.
///
/// Longer secrets are replaced first, so a secret that is a prefix of
/// another cannot leave part of the longer one visible.
pub(crate) fn redact_secrets(text: &str, secret_values: &HashSet<String>) -> String {
    let mut secrets: Vec<&String> = secret_values
        .iter()
        .filter(|s| s.len() >= MIN_REDACTION_LEN)
        .collect();
    secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
    let mut out = text.to_string();
    for secret in secrets {
        out = out.replace(secret.as_str(), REDACTED);
    }
    out
}

// Consumed by the debug logging added in a later task.
#[allow(dead_code)]
/// Whether a header's value is always masked, whatever it contains.
pub(crate) fn is_sensitive_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "authorization" | "proxy-authorization" | "cookie" | "set-cookie" | "x-api-key"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn set(values: &[&str]) -> HashSet<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn masks_every_occurrence_and_the_longest_secret_first() {
        let out = redact_secrets("a=abc123xyz&b=abc123", &set(&["abc123", "abc123xyz"]));
        assert_eq!(out, "a=••••••&b=••••••");
    }

    #[test]
    fn leaves_secrets_shorter_than_the_floor() {
        assert_eq!(redact_secrets("pin=1234", &set(&["1234"])), "pin=1234");
    }

    #[test]
    fn empty_set_returns_the_text_unchanged() {
        assert_eq!(redact_secrets("hello", &HashSet::new()), "hello");
    }

    #[test]
    fn sensitive_header_names_match_case_insensitively() {
        for name in [
            "Authorization",
            "proxy-authorization",
            "COOKIE",
            "Set-Cookie",
            "x-api-key",
        ] {
            assert!(is_sensitive_header(name), "{name}");
        }
        assert!(!is_sensitive_header("Content-Type"));
    }
}
