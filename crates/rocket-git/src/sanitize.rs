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

const TOKEN_PREFIXES: &[&str] = &[
    "sk_live_",
    "sk_test_",
    "ghp_",
    "gho_",
    "ghs_",
    "github_pat_",
    "xoxb-",
    "xoxp-",
    "xoxa-",
    "xoxr-",
    "xoxs-",
];

// True for values with a well-known secret shape, whatever name they sit under.
fn looks_like_secret_value(text: &str) -> bool {
    if looks_like_jwt(text) {
        return true;
    }
    for scheme in ["Bearer ", "Basic "] {
        if let Some(rest) = text.strip_prefix(scheme) {
            if rest.chars().any(|c| !c.is_whitespace()) {
                return true;
            }
        }
    }
    if TOKEN_PREFIXES
        .iter()
        .any(|prefix| text.len() > prefix.len() && text.starts_with(prefix))
    {
        return true;
    }
    if let Some(rest) = text.strip_prefix("AKIA") {
        if rest.len() == 16
            && rest
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        {
            return true;
        }
    }
    text.starts_with("-----BEGIN ") && text.contains("PRIVATE KEY")
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
        count += mask_body(body);
    }
    count
}

fn mask_body(body: &mut Value) -> usize {
    let kind = body.get("type").and_then(Value::as_str).unwrap_or("");
    if matches!(kind, "binary" | "file") {
        return 0;
    }
    let is_json = kind == "json";
    match body.get_mut("data") {
        Some(Value::Sequence(parts)) => parts
            .iter_mut()
            .filter(|part| part.get("value").is_some_and(Value::is_string))
            .map(mask_named_entry)
            .sum(),
        Some(Value::String(data)) => {
            let (masked, count) = if is_json {
                mask_json_text(data)
            } else {
                mask_text_body(data)
            };
            if count > 0 {
                *data = masked;
            }
            count
        }
        _ => 0,
    }
}

// Masks a text, XML, HTML or SPARQL body. A mislabelled JSON body is masked as JSON.
fn mask_text_body(text: &str) -> (String, usize) {
    if text.trim_start().starts_with(['{', '[']) {
        let (masked, count) = mask_json_text(text);
        // Bracketed text such as a log line is not JSON, so it also gets the word pass.
        if serde_json::from_str::<serde_json::Value>(&masked).is_ok() {
            return (masked, count);
        }
        let (masked, words) = mask_words(&masked);
        return (masked, count + words);
    }
    let (shaped, count) = match mask_form_text(text) {
        Some(result) => result,
        None => mask_xml_text(text),
    };
    let (out, words) = mask_words(&shaped);
    (out, count + words)
}

// Masks every secret-shaped word. Words end at whitespace, quotes, `=`, `<` and `>`.
fn mask_words(text: &str) -> (String, usize) {
    let mut out = String::with_capacity(text.len());
    let mut count = 0;
    let mut word_start = 0;
    for (i, c) in text.char_indices() {
        if c.is_whitespace() || matches!(c, '"' | '\'' | '=' | '<' | '>') {
            count += push_word(&mut out, &text[word_start..i]);
            out.push(c);
            word_start = i + c.len_utf8();
        }
    }
    count += push_word(&mut out, &text[word_start..]);
    (out, count)
}

// Pushes the word, or the mask when it is secret-shaped. Returns 1 when it masked.
fn push_word(out: &mut String, word: &str) -> usize {
    if looks_like_secret_value(word) {
        out.push_str(MASK);
        1
    } else {
        out.push_str(word);
        0
    }
}

// Masks secret-named values in `a=b&c=d` text. Returns `None` when the text is not form-shaped.
fn mask_form_text(text: &str) -> Option<(String, usize)> {
    let trimmed = text.trim();
    if trimmed.is_empty()
        || trimmed.chars().any(char::is_whitespace)
        || !trimmed.split('&').all(|part| part.contains('='))
    {
        return None;
    }
    let lead = &text[..text.len() - text.trim_start().len()];
    let trail = &text[text.trim_end().len()..];
    let text = trimmed;
    let mut count = 0;
    let parts: Vec<String> = text
        .split('&')
        .map(|part| match part.split_once('=') {
            Some((key, value)) if is_secret_name(key) && !value.is_empty() && value != MASK => {
                count += 1;
                format!("{key}={MASK}")
            }
            _ => part.to_string(),
        })
        .collect();
    Some((format!("{lead}{}{trail}", parts.join("&")), count))
}

// Masks the text of secret-named elements such as `<token>abc</token>`.
fn mask_xml_text(text: &str) -> (String, usize) {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut count = 0;
    while let Some(open) = rest.find('<') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('>') else { break };
        let name = &after[..close];
        // A long name or one holding `<` is never a plain tag, so skip it without allocating.
        if name.len() > 128 || name.contains('<') {
            out.push_str(&rest[..open + 1]);
            rest = after;
            continue;
        }
        let plain = !name.is_empty()
            && !name.starts_with(['/', '?', '!'])
            && !name.ends_with('/')
            && !name.contains(|c: char| c.is_whitespace() || c == '<');
        let body = &after[close + 1..];
        let value_len = body.find('<').unwrap_or(body.len());
        let value = &body[..value_len];
        let closing = format!("</{name}>");
        if plain
            && is_secret_name(name)
            && !value.is_empty()
            && value != MASK
            && body[value_len..].starts_with(&closing)
        {
            out.push_str(&rest[..open + 1 + close + 1]);
            out.push_str(MASK);
            rest = &body[value_len..];
            count += 1;
        } else {
            out.push_str(&rest[..open + 1]);
            rest = after;
        }
    }
    out.push_str(rest);
    (out, count)
}

// A header or param is a `name`/`value` pair. The value is masked when the name looks secret.
fn mask_named_entry(entry: &mut Value) -> usize {
    let secret = entry
        .get("name")
        .and_then(Value::as_str)
        .is_some_and(is_secret_name)
        || entry
            .get("value")
            .and_then(Value::as_str)
            .is_some_and(looks_like_secret_value);
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

// Masks `user:pass@` in the authority and secret query values. Host and path stay.
fn mask_url(url: &str) -> Option<String> {
    let (base, query) = match url.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (url, None),
    };
    let mut changed = false;
    let mut base = base.to_string();
    if let Some(scheme_end) = base.find("://") {
        let authority_start = scheme_end + 3;
        let authority_len = base[authority_start..]
            .find(['/', '#'])
            .unwrap_or(base.len() - authority_start);
        let authority = &base[authority_start..authority_start + authority_len];
        if let Some((userinfo, host)) = authority.rsplit_once('@') {
            if !userinfo.is_empty() && userinfo != MASK {
                base = format!(
                    "{}{MASK}@{host}{}",
                    &base[..authority_start],
                    &base[authority_start + authority_len..]
                );
                changed = true;
            }
        }
    }
    let Some(query) = query else {
        return changed.then_some(base);
    };
    let pairs: Vec<String> = query
        .split('&')
        .map(|pair| match pair.split_once('=') {
            Some((key, value))
                if (is_secret_name(key) || looks_like_secret_value(value))
                    && !value.is_empty()
                    && value != MASK =>
            {
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
        } else if !is_key && looks_like_secret_value(inner) {
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
    fn xml_scan_survives_a_body_of_open_brackets() {
        let text = format!("{}>", "<".repeat(200_000));
        let (out, count) = mask_xml_text(&text);
        assert_eq!(count, 0);
        assert_eq!(out, text);
    }

    #[test]
    fn xml_masks_a_plain_secret_tag() {
        let (out, count) = mask_xml_text("<a><token>abc</token></a>");
        assert_eq!(count, 1);
        assert_eq!(out, "<a><token>[masked]</token></a>");
    }

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

    // Builds a one-example file with a body on the given side.
    fn file_with_body(side: &str, kind: &str, data: &str) -> String {
        let other = if side == "request" {
            "response"
        } else {
            "request"
        };
        format!(
            "examples:\n  - name: e\n    {other}:\n      url: https://x.io\n    {side}:\n      body:\n        type: {kind}\n        data: {data:?}\n"
        )
    }

    #[test]
    fn form_urlencoded_list_masks_secret_named_fields() {
        let file = "examples:\n  - name: e\n    request:\n      body:\n        type: form-urlencoded\n        data:\n          - name: username\n            value: ann\n          - name: client_secret\n            value: s3\n";
        let (masked, count) = mask_examples(file).expect("masked");
        assert_eq!(count, 1);
        assert!(masked.contains("value: ann"));
        assert!(masked.contains("[masked]"));
        assert!(!masked.contains("s3"));
    }

    #[test]
    fn text_body_that_holds_json_is_masked_as_json() {
        let file = file_with_body(
            "response",
            "text",
            "{\"access_token\": \"abc\", \"ok\": true}",
        );
        let (masked, _) = mask_examples(&file).expect("masked");
        assert!(
            masked.contains("\"access_token\": \"[masked]\""),
            "{masked}"
        );
        assert!(masked.contains("\"ok\": true"));
    }

    #[test]
    fn form_shaped_text_body_masks_secret_keys() {
        let file = file_with_body("request", "text", "user=ann&password=hunter2&page=2");
        let (masked, _) = mask_examples(&file).expect("masked");
        assert!(masked.contains("password=[masked]"), "{masked}");
        assert!(masked.contains("user=ann"));
        assert!(masked.contains("page=2"));
    }

    #[test]
    fn form_text_with_trailing_newline_is_masked() {
        let (masked, count) = mask_text_body("user=ann&password=hunter2\n");
        assert_eq!(count, 1);
        assert_eq!(masked, "user=ann&password=[masked]\n");
        assert_eq!(mask_text_body(&masked).1, 0);
    }

    #[test]
    fn form_text_with_leading_newline_is_masked() {
        let (masked, count) = mask_text_body("\nuser=ann&password=hunter2");
        assert_eq!(count, 1);
        assert_eq!(masked, "\nuser=ann&password=[masked]");
    }

    #[test]
    fn masked_form_body_in_a_file_is_stable() {
        let file = file_with_body("request", "text", "user=ann&password=hunter2\n");
        let (once, _) = mask_examples(&file).expect("masked");
        assert!(!once.contains("hunter2"));
        assert_eq!(mask_examples(&once), None);
    }

    #[test]
    fn xml_body_masks_secret_named_elements() {
        let file = file_with_body(
            "response",
            "xml",
            "<r><token>abc</token><name>Ann</name></r>",
        );
        let (masked, _) = mask_examples(&file).expect("masked");
        assert!(masked.contains("<token>[masked]</token>"), "{masked}");
        assert!(masked.contains("<name>Ann</name>"));
    }

    #[test]
    fn jwt_inside_plain_text_is_masked() {
        let file = file_with_body("response", "text", &format!("got {JWT} ok"));
        let (masked, _) = mask_examples(&file).expect("masked");
        assert!(!masked.contains(JWT));
        assert!(masked.contains("got [masked] ok"), "{masked}");
    }

    #[test]
    fn prose_mentioning_a_secret_word_is_not_masked() {
        let file = file_with_body("response", "text", "the token expired, please log in");
        assert_eq!(mask_examples(&file), None);
    }

    #[test]
    fn binary_and_empty_bodies_are_left_alone() {
        let file = file_with_body("response", "binary", "AAAA");
        assert_eq!(mask_examples(&file), None);
        let file = file_with_body("response", "text", "");
        assert_eq!(mask_examples(&file), None);
    }

    #[test]
    fn set_cookie_value_is_masked_whatever_the_cookie_name() {
        let file = "examples:\n  - name: e\n    response:\n      headers:\n        - name: Set-Cookie\n          value: theme=dark; Path=/\n";
        let (masked, count) = mask_examples(file).expect("masked");
        assert_eq!(count, 1);
        assert!(!masked.contains("theme=dark"));
    }

    #[test]
    fn secret_shaped_values_are_detected() {
        for value in [
            JWT,
            "Bearer abc123",
            "Basic dXNlcjpwYXNz",
            "sk_live_abcdefghij",
            "ghp_abcdefghijklmnop",
            "github_pat_abc123",
            "xoxb-123-abc",
            "AKIAIOSFODNN7EXAMPLE",
            "-----BEGIN RSA PRIVATE KEY-----",
        ] {
            assert!(looks_like_secret_value(value), "{value}");
        }
        for value in [
            "application/json",
            "Ann",
            "550e8400-e29b-41d4-a716-446655440000",
            "da39a3ee5e6b4b0d3255bfef95601890afd80709",
            "Bearer",
            "Bearer ",
        ] {
            assert!(!looks_like_secret_value(value), "{value}");
        }
    }

    #[test]
    fn harmless_header_name_with_secret_value_is_masked() {
        let file = "examples:\n  - name: e\n    request:\n      headers:\n        - name: X-Tenant\n          value: Bearer abc123\n        - name: X-Trace\n          value: 550e8400-e29b-41d4-a716-446655440000\n";
        let (masked, count) = mask_examples(file).expect("masked");
        assert_eq!(count, 1);
        assert!(!masked.contains("abc123"));
        assert!(masked.contains("550e8400-e29b-41d4-a716-446655440000"));
    }

    #[test]
    fn harmless_json_key_with_secret_value_is_masked() {
        let text = "{\"data\": \"ghp_abcdefghijklmnop\", \"id\": \"550e8400-e29b-41d4-a716-446655440000\"}";
        let (masked, count) = mask_json_text(text);
        assert_eq!(count, 1);
        assert_eq!(
            masked,
            "{\"data\": \"[masked]\", \"id\": \"550e8400-e29b-41d4-a716-446655440000\"}"
        );
    }

    #[test]
    fn url_credentials_are_masked_and_host_kept() {
        assert_eq!(
            mask_url("https://ann:pw@api.io/v1?x=1").as_deref(),
            Some("https://[masked]@api.io/v1?x=1")
        );
        assert_eq!(
            mask_url("https://ann:pw@api.io/v1").as_deref(),
            Some("https://[masked]@api.io/v1")
        );
        assert_eq!(mask_url("https://[masked]@api.io/v1"), None);
        assert_eq!(mask_url("https://api.io/v1/a@b"), None);
    }

    #[test]
    fn url_query_value_with_secret_shape_is_masked() {
        assert_eq!(
            mask_url("https://x.io/a?ref=ghp_abcdefghijklmnop&page=1").as_deref(),
            Some("https://x.io/a?ref=[masked]&page=1")
        );
    }

    #[test]
    fn masking_stays_idempotent_with_new_rules() {
        let file = format!(
            "examples:\n  - name: e\n    request:\n      url: https://ann:pw@api.io/v1?ref=ghp_abcdefghijklmnop\n      headers:\n        - name: X-Tenant\n          value: Bearer abc123\n      body:\n        type: json\n        data: '{{\"data\": \"{JWT}\"}}'\n    response:\n      body:\n        type: text\n        data: \"[ERROR] {JWT} a=Bearer\"\n"
        );
        let (once, count) = mask_examples(&file).expect("masked");
        assert_eq!(count, 4);
        assert!(!once.contains("abc123") && !once.contains(JWT) && !once.contains("pw@"));
        assert_eq!(mask_examples(&once), None);
    }

    #[test]
    fn words_after_equals_and_angle_brackets_are_masked() {
        let (masked, count) = mask_text_body(&format!("foo={JWT} bar"));
        assert_eq!((masked.as_str(), count), ("foo=[masked] bar", 1));
        let (masked, count) = mask_text_body(&format!("<note>{JWT}</note>"));
        assert_eq!((masked.as_str(), count), ("<note>[masked]</note>", 1));
        assert_eq!(mask_text_body(&masked).1, 0);
    }

    #[test]
    fn bracketed_log_text_is_not_treated_as_json_only() {
        let (masked, count) = mask_text_body(&format!("[ERROR] {JWT}"));
        assert_eq!((masked.as_str(), count), ("[ERROR] [masked]", 1));
        assert_eq!(mask_text_body(&masked).1, 0);
        let (masked, count) = mask_text_body("{\"token\": \"abc\"}");
        assert_eq!((masked.as_str(), count), ("{\"token\": \"[masked]\"}", 1));
    }
}
