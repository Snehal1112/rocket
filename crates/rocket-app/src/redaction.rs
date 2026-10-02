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

/// Every form of a secret value that must be masked.
///
/// The whole value, its trimmed form, and, for a multi-line value such as a PEM, each line on its
/// own, so one printed line is masked too. `-----BEGIN` and `-----END` armor lines are not secret
/// and are skipped. Forms shorter than `MIN_REDACTION_LEN` are left out, like in
/// `redact_secrets`.
pub(crate) fn redaction_forms(value: &str) -> Vec<String> {
    let mut forms: Vec<String> = Vec::new();
    let mut add = |s: &str| {
        if s.len() >= MIN_REDACTION_LEN && !forms.iter().any(|f| f == s) {
            forms.push(s.to_string());
        }
    };
    add(value);
    add(value.trim());
    if value.contains('\n') || value.contains('\r') {
        for line in value.lines() {
            let line = line.trim();
            if !line.starts_with("-----") {
                add(line);
            }
        }
    }
    forms
}

/// Collects the `redaction_forms` of every secret value, without duplicates.
pub(crate) fn secret_forms<'a>(values: impl IntoIterator<Item = &'a String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for value in values {
        for form in redaction_forms(value) {
            if !out.contains(&form) {
                out.push(form);
            }
        }
    }
    out
}

/// Whether `text` equals or contains any of the secret `forms`.
///
/// `forms` come from `secret_forms`, so they are already at least `MIN_REDACTION_LEN` long.
pub(crate) fn contains_secret(text: &str, forms: &[String]) -> bool {
    forms.iter().any(|form| text.contains(form.as_str()))
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
    fn contains_secret_matches_equal_and_embedded_values() {
        let forms = secret_forms(&["tok-abcdef".to_string()]);
        assert!(contains_secret("tok-abcdef", &forms));
        assert!(contains_secret("Bearer tok-abcdef!", &forms));
        assert!(!contains_secret("tok-abcde", &forms));
    }

    #[test]
    fn contains_secret_matches_a_single_pem_line() {
        let pem = "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkq\nhkiG9w0BAQEFAASC\n-----END PRIVATE KEY-----\n";
        let forms = secret_forms(&[pem.to_string()]);
        assert!(contains_secret("MIIEvQIBADANBgkq", &forms));
        assert!(contains_secret(pem, &forms));
        assert!(!contains_secret("-----BEGIN PRIVATE KEY-----", &forms));
    }

    #[test]
    fn contains_secret_ignores_values_below_the_floor() {
        let forms = secret_forms(&["12345".to_string()]);
        assert!(forms.is_empty());
        assert!(!contains_secret("12345", &forms));
        assert!(!contains_secret("anything", &[]));
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

    const PEM: &str = "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkqhkiG9w0B\nAQEFAASCBKcwggSjAgEAAoIB\n-----END PRIVATE KEY-----\n";

    fn forms(value: &str) -> HashSet<String> {
        redaction_forms(value).into_iter().collect()
    }

    #[test]
    fn a_single_line_value_has_one_form() {
        assert_eq!(redaction_forms("sk-live-abcdef123"), vec!["sk-live-abcdef123"]);
        assert!(redaction_forms("abc").is_empty(), "below the floor");
    }

    #[test]
    fn a_multi_line_value_is_masked_whole_and_line_by_line() {
        let set = forms(PEM);
        // Whole: nothing is left behind.
        assert_eq!(redact_secrets(PEM, &set), REDACTED);
        // One line on its own, for example from a script or an echoing server.
        assert_eq!(
            redact_secrets("leaked: AQEFAASCBKcwggSjAgEAAoIB", &set),
            format!("leaked: {REDACTED}")
        );
        // The trimmed form, without the final newline.
        assert_eq!(redact_secrets(PEM.trim(), &set), REDACTED);
    }

    #[test]
    fn crlf_values_are_masked_line_by_line_too() {
        let crlf = PEM.replace('\n', "\r\n");
        let set = forms(&crlf);
        assert_eq!(redact_secrets(&crlf, &set), REDACTED);
        assert_eq!(
            redact_secrets("x MIIEvQIBADANBgkqhkiG9w0B x", &set),
            format!("x {REDACTED} x")
        );
        // Also the same PEM printed with LF only.
        assert_eq!(
            redact_secrets("a\nMIIEvQIBADANBgkqhkiG9w0B\nb", &set),
            format!("a\n{REDACTED}\nb")
        );
    }

    #[test]
    fn pem_armor_lines_and_short_lines_are_not_secret() {
        let set = forms("-----BEGIN CERTIFICATE-----\nabc\n1234567890\n-----END CERTIFICATE-----\n");
        // The whole value starts with armor on purpose. No single line of armor is a form.
        assert!(!set
            .iter()
            .any(|f| !f.contains('\n') && f.starts_with("-----")));
        assert!(!set.contains("abc"));
        assert!(set.contains("1234567890"));
        assert_eq!(
            redact_secrets("-----BEGIN CERTIFICATE-----", &set),
            "-----BEGIN CERTIFICATE-----"
        );
    }
}
