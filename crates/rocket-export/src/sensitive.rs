//! Name rule: decides whether a header, param, field or variable name holds a secret.

/// Names that end in a secret word but are not secrets (words joined with `_`).
const ALLOWED: [&str; 6] = [
    "token_type",
    "page_token",
    "next_token",
    "prev_token",
    "previous_token",
    "max_tokens",
];

/// A name whose last word is one of these is sensitive.
const SENSITIVE_LAST_WORDS: [&str; 10] = [
    "token",
    "secret",
    "password",
    "passwd",
    "credential",
    "credentials",
    "authorization",
    "cookie",
    "signature",
    "apikey",
];

/// A name that is exactly one of these words is sensitive.
const SENSITIVE_SINGLE_WORDS: [&str; 2] = ["pass", "pwd"];

/// A name whose last two words are one of these pairs is sensitive.
const SENSITIVE_ENDINGS: [(&str, &str); 4] = [
    ("api", "key"),
    ("private", "key"),
    ("secret", "key"),
    ("client", "assertion"),
];

/// Splits a name into lowercase words at separators, case changes and acronym ends.
fn words(name: &str) -> Vec<String> {
    let chars: Vec<char> = name.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if c.is_uppercase() && !cur.is_empty() {
            let prev = chars[i - 1];
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            if prev.is_lowercase() || prev.is_ascii_digit() || (prev.is_uppercase() && next_lower) {
                out.push(std::mem::take(&mut cur));
            }
        }
        cur.extend(c.to_lowercase());
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Whether a name looks like it holds a secret, for example `access_token`.
pub fn is_sensitive_name(name: &str) -> bool {
    let words = words(name);
    let Some(last) = words.last() else {
        return false;
    };
    if ALLOWED.contains(&words.join("_").as_str()) {
        return false;
    }
    if SENSITIVE_LAST_WORDS.contains(&last.as_str()) {
        return true;
    }
    if words.len() == 1 && SENSITIVE_SINGLE_WORDS.contains(&last.as_str()) {
        return true;
    }
    if let [.., prev, last] = words.as_slice() {
        return SENSITIVE_ENDINGS.contains(&(prev.as_str(), last.as_str()));
    }
    false
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_rule_table() {
        let sensitive = [
            "Authorization",
            "Proxy-Authorization",
            "Cookie",
            "Set-Cookie",
            "X-Api-Key",
            "api_key",
            "apiKey",
            "APIKey",
            "apikey",
            "access_token",
            "accessToken",
            "refresh_token",
            "sessionToken",
            "X-Auth-Token",
            "X-Amz-Security-Token",
            "client_secret",
            "clientSecret",
            "db_password",
            "password",
            "passwd",
            "pass",
            "pwd",
            "private_key",
            "privateKey",
            "secret_key",
            "client_assertion",
            "signature",
            "X-Hub-Signature",
        ];
        let plain = [
            "token_type",
            "tokenType",
            "page_token",
            "next_token",
            "prev_token",
            "max_tokens",
            "author",
            "keyword",
            "monkey",
            "sort_key",
            "Cache-Key",
            "key",
            "session_id",
            "auth_type",
            "grant_type",
            "bypass",
            "compass",
            "passport",
            "tokenizer",
            "X-Request-Id",
            "Content-Type",
            "scope",
            "state",
            "",
        ];
        for name in sensitive {
            assert!(is_sensitive_name(name), "{name:?} should be sensitive");
        }
        for name in plain {
            assert!(!is_sensitive_name(name), "{name:?} should not be sensitive");
        }
    }
}
