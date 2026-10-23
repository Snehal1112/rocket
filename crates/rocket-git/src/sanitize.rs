//! Masks secrets inside saved request examples before they reach a commit.
//!
//! Only the `examples:` block of a request file is touched. The rest of the file,
//! and the working tree, keep their real values.

use serde_yaml::Value;

const MASK: &str = "[masked]";
const MASKED_JSON: &str = "\"[masked]\"";

// Matched against a lowercase name with every non-alphanumeric character removed.
const SECRET_WORDS: &[&str] = &[
    "authorization",
    "cookie",
    "token",
    "secret",
    "password",
    "passwd",
    "apikey",
    "privatekey",
    "credential",
    "jwt",
];

fn is_secret_name(name: &str) -> bool {
    let normalized: String = name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    SECRET_WORDS.iter().any(|word| normalized.contains(word))
}

fn looks_like_jwt(text: &str) -> bool {
    text.starts_with("eyJ") && text.matches('.').count() == 2
}

/// Masks secrets in the `examples:` block of a request file. Returns the new
/// file text and the number of masked values, or `None` when nothing changed.
pub fn mask_examples(yaml: &str) -> Option<(String, usize)> {
    if !yaml.contains("examples:") {
        return None;
    }
    let mut doc: Value = serde_yaml::from_str(yaml).ok()?;
    let examples = doc.get_mut("examples")?.as_sequence_mut()?;

    let mut count = 0;
    for example in examples {
        for side in ["request", "response"] {
            if let Some(side) = example.get_mut(side) {
                count += mask_side(side);
            }
        }
    }
    if count == 0 {
        return None;
    }
    let masked = serde_yaml::to_string(&doc).ok()?;
    Some((masked, count))
}

fn mask_side(side: &mut Value) -> usize {
    let mut count = 0;
    for list in ["headers", "params"] {
        if let Some(entries) = side.get_mut(list).and_then(Value::as_sequence_mut) {
            for entry in entries {
                count += mask_named_entry(entry);
            }
        }
    }
    if let Some(Value::String(url)) = side.get_mut("url") {
        if let Some(masked) = mask_url(url) {
            *url = masked;
            count += 1;
        }
    }
    if let Some(body) = side.get_mut("body") {
        let is_json = body.get("type").and_then(Value::as_str) == Some("json");
        if let (true, Some(Value::String(data))) = (is_json, body.get_mut("data")) {
            let (masked, masked_count) = mask_json_text(data);
            if masked_count > 0 {
                *data = masked;
                count += masked_count;
            }
        }
    }
    count
}

// A header or param is a `name`/`value` pair. The value is masked when the name looks secret.
fn mask_named_entry(entry: &mut Value) -> usize {
    let secret = entry
        .get("name")
        .and_then(Value::as_str)
        .is_some_and(is_secret_name);
    if !secret {
        return 0;
    }
    match entry.get_mut("value") {
        Some(value) if value.as_str() != Some(MASK) => {
            *value = Value::String(MASK.to_string());
            1
        }
        _ => 0,
    }
}

fn mask_url(url: &str) -> Option<String> {
    let (base, query) = url.split_once('?')?;
    let mut changed = false;
    let pairs: Vec<String> = query
        .split('&')
        .map(|pair| match pair.split_once('=') {
            Some((key, value)) if is_secret_name(key) && !value.is_empty() && value != MASK => {
                changed = true;
                format!("{key}={MASK}")
            }
            _ => pair.to_string(),
        })
        .collect();
    changed.then(|| format!("{base}?{}", pairs.join("&")))
}

/// Masks the values of secret-looking keys, and any JWT-shaped string, in JSON text.
/// It edits the text in place, so key order and formatting stay as they were.
fn mask_json_text(text: &str) -> (String, usize) {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    let mut count = 0;
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] != b'"' {
            i += 1;
            continue;
        }
        let end = string_end(bytes, i);
        let inner = text.get(i + 1..end.saturating_sub(1)).unwrap_or("");

        let mut colon = end;
        while colon < bytes.len() && bytes[colon].is_ascii_whitespace() {
            colon += 1;
        }
        let is_key = colon < bytes.len() && bytes[colon] == b':';

        if is_key && is_secret_name(inner) {
            let mut start = colon + 1;
            while start < bytes.len() && bytes[start].is_ascii_whitespace() {
                start += 1;
            }
            let value_end = value_end(bytes, start);
            if text[start..value_end] != *MASKED_JSON {
                out.push_str(&text[last..start]);
                out.push_str(MASKED_JSON);
                last = value_end;
                count += 1;
            }
            i = value_end;
        } else if !is_key && looks_like_jwt(inner) {
            out.push_str(&text[last..i]);
            out.push_str(MASKED_JSON);
            last = end;
            count += 1;
            i = end;
        } else {
            i = end;
        }
    }
    out.push_str(&text[last..]);
    (out, count)
}

// Index just past the closing quote of the string that starts at `start`.
fn string_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

// Index just past the JSON value that starts at `start`.
fn value_end(bytes: &[u8], start: usize) -> usize {
    if start >= bytes.len() {
        return bytes.len();
    }
    match bytes[start] {
        b'"' => string_end(bytes, start),
        b'{' | b'[' => {
            let mut depth = 0usize;
            let mut i = start;
            while i < bytes.len() {
                match bytes[i] {
                    b'"' => {
                        i = string_end(bytes, i);
                        continue;
                    }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return i + 1;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            bytes.len()
        }
        _ => {
            let mut i = start;
            while i < bytes.len() && !matches!(bytes[i], b',' | b'}' | b']') {
                if bytes[i].is_ascii_whitespace() {
                    break;
                }
                i += 1;
            }
            i
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JWT: &str = "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiIxIn0.c2ln";

    #[test]
    fn secret_names_are_matched_loosely() {
        for name in [
            "Authorization",
            "set-cookie",
            "X-Api-Key",
            "access_token",
            "clientSecret",
            "Password",
        ] {
            assert!(is_secret_name(name), "{name}");
        }
        for name in ["content-type", "date", "message", "server"] {
            assert!(!is_secret_name(name), "{name}");
        }
    }

    #[test]
    fn json_masks_secret_keys_and_keeps_layout() {
        let text = "{\n  \"id\": 7,\n  \"accessToken\": \"abc\",\n  \"user\": {\"password\": \"p\", \"name\": \"Ann\"}\n}";
        let (masked, count) = mask_json_text(text);
        assert_eq!(count, 2);
        assert_eq!(
            masked,
            "{\n  \"id\": 7,\n  \"accessToken\": \"[masked]\",\n  \"user\": {\"password\": \"[masked]\", \"name\": \"Ann\"}\n}"
        );
    }

    #[test]
    fn json_masks_an_object_value_under_a_secret_key() {
        let (masked, count) =
            mask_json_text("{\"credentials\": {\"a\": [1, \"}\"]}, \"ok\": true}");
        assert_eq!(count, 1);
        assert_eq!(masked, "{\"credentials\": \"[masked]\", \"ok\": true}");
    }

    #[test]
    fn json_masks_a_jwt_under_any_key() {
        let text = format!("{{\"data\": \"{JWT}\", \"n\": 1}}");
        let (masked, count) = mask_json_text(&text);
        assert_eq!(count, 1);
        assert_eq!(masked, "{\"data\": \"[masked]\", \"n\": 1}");
    }

    #[test]
    fn json_masking_is_idempotent_and_ignores_secret_looking_values() {
        let text = "{\"message\": \"token expired\", \"token\": \"[masked]\"}";
        let (masked, count) = mask_json_text(text);
        assert_eq!(count, 0);
        assert_eq!(masked, text);
    }

    #[test]
    fn json_handles_escapes_and_utf8() {
        let (masked, count) = mask_json_text("{\"note\": \"a \\\" b é\", \"secret\": \"s\"}");
        assert_eq!(count, 1);
        assert_eq!(
            masked,
            "{\"note\": \"a \\\" b é\", \"secret\": \"[masked]\"}"
        );
    }

    #[test]
    fn url_masks_secret_query_values_only() {
        assert_eq!(
            mask_url("https://x.io/a?page=1&api_key=abc&q=t").as_deref(),
            Some("https://x.io/a?page=1&api_key=[masked]&q=t")
        );
        assert_eq!(mask_url("https://x.io/a?page=1"), None);
        assert_eq!(mask_url("https://x.io/a"), None);
    }

    const FILE: &str = r#"info:
  name: Get Datasets
http:
  method: GET
  url: https://x.io/datasets
  auth:
    type: bearer
    token: live-token
examples:
  - name: example
    request:
      url: https://x.io/datasets?token=abc
      method: GET
      headers:
        - name: Authorization
          value: Bearer abc
    response:
      status: 403
      headers:
        - name: content-type
          value: application/json
        - name: set-cookie
          value: sid=1
      body:
        type: json
        data: |-
          {"message": "Forbidden", "refresh_token": "r"}
"#;

    #[test]
    fn masks_only_the_examples_block() {
        let (masked, count) = mask_examples(FILE).expect("masked");
        assert_eq!(count, 4);
        assert!(masked.contains("token: live-token"), "auth is left alone");
        assert!(masked.contains("value: application/json"));
        assert!(masked.contains("token=[masked]"));
        for leaked in ["Bearer abc", "sid=1", "\"r\""] {
            assert!(!masked.contains(leaked), "{leaked} leaked");
        }
        assert!(masked.contains("Forbidden"));
    }

    #[test]
    fn masking_twice_changes_nothing() {
        let (once, _) = mask_examples(FILE).expect("masked");
        assert_eq!(mask_examples(&once), None);
    }

    #[test]
    fn files_without_examples_are_not_touched() {
        assert_eq!(mask_examples("info:\n  name: A\nhttp:\n  url: x\n"), None);
        assert_eq!(mask_examples("examples: [not valid"), None);
    }
}
