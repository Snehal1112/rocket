//! Masks secret values and sensitive headers in text shown to the user.

use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS, NON_ALPHANUMERIC};
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

// The sets below mirror the ones the `url` crate applies per component.
const QUERY_SET: &AsciiSet = &CONTROLS.add(b' ').add(b'"').add(b'#').add(b'<').add(b'>');
const PATH_SET: &AsciiSet = &QUERY_SET.add(b'?').add(b'`').add(b'{').add(b'}');
const USERINFO_SET: &AsciiSet = &PATH_SET
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'=')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'|');

/// Like `redact_secrets`, but also masks the percent-encoded forms of each secret.
///
/// A URL is re-serialized before it is sent, so a secret can appear in it
/// (or in an error that quotes it) in an encoded form.
pub(crate) fn redact_url_secrets(text: &str, secret_values: &HashSet<String>) -> String {
    let mut forms: HashSet<String> = HashSet::new();
    for secret in secret_values
        .iter()
        .filter(|s| s.len() >= MIN_REDACTION_LEN)
    {
        forms.insert(secret.clone());
        for set in [QUERY_SET, PATH_SET, USERINFO_SET, NON_ALPHANUMERIC] {
            forms.insert(utf8_percent_encode(secret, set).to_string());
        }
        forms.insert(url::form_urlencoded::byte_serialize(secret.as_bytes()).collect());
    }
    redact_secrets(text, &forms)
}

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
    fn url_masking_covers_percent_encoded_forms() {
        let secret = "p@ss word é1";
        let secrets = set(&[secret]);
        let raw = format!("https://h/{secret}?q={secret}");
        assert_eq!(
            redact_url_secrets(&raw, &secrets),
            "https://h/••••••?q=••••••"
        );
        let encoded = "https://h/p@ss%20word%20%C3%A91?q=p%40ss+word+%C3%A91";
        assert_eq!(
            redact_url_secrets(encoded, &secrets),
            "https://h/••••••?q=••••••"
        );
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
