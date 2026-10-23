//! YAML scrub: blanks credentials and strips the Rocket extension namespace.
//!
//! The walk goes through `serde_yaml::Value` by key, never through typed
//! structs, so unknown keys survive.

use crate::report::{ExportAction, ExportNote};
use crate::sensitive::is_sensitive_name;
use rocket_shared::oauth2::OAuth2Flow;
use rocket_shared::types::{Auth, FormDataEntry, FormDataType, Header};
use serde_yaml::{Mapping, Value};

/// Whether a header's value is a credential by its name (see `is_sensitive_name`).
pub fn is_sensitive_header(name: &str) -> bool {
    is_sensitive_name(name)
}

/// True when the text is only a `{{variable}}` reference, optionally after
/// `Bearer `, `Basic ` or `Token `.
pub fn is_variable_reference(text: &str) -> bool {
    let mut rest = text.trim();
    for prefix in ["Bearer ", "Basic ", "Token "] {
        if let Some(stripped) = rest.strip_prefix(prefix) {
            rest = stripped.trim_start();
            break;
        }
    }
    match rest.strip_prefix("{{").and_then(|r| r.strip_suffix("}}")) {
        Some(inner) => !inner.trim().is_empty() && !inner.contains("{{") && !inner.contains("}}"),
        None => false,
    }
}

/// Removes `extensions.rocketapi`, and `extensions` itself when it ends up empty.
/// Returns true when something was removed.
pub fn strip_rocket_namespace(value: &mut Value) -> bool {
    let Some(root) = value.as_mapping_mut() else {
        return false;
    };
    let Some(Value::Mapping(ext)) = root.get_mut("extensions") else {
        return false;
    };
    let removed = ext.remove("rocketapi").is_some();
    if removed && ext.is_empty() {
        root.remove("extensions");
    }
    removed
}

/// Blanks credentials in a parsed YAML document and records one note per blank.
pub fn scrub_yaml(rel_path: &str, value: &mut Value, notes: &mut Vec<ExportNote>) {
    walk(rel_path, value, notes);
}

fn blank_note(rel_path: &str, feature: &str, detail: &str) -> ExportNote {
    ExportNote {
        path: rel_path.to_string(),
        feature: feature.to_string(),
        action: ExportAction::Blanked,
        detail: Some(detail.to_string()),
    }
}

/// Blanks a credential slot unless it is empty or a variable reference. A number or a
/// boolean is a literal credential too (an unquoted `token: 123456`), so it is blanked.
fn blank_string(slot: &mut Value) -> bool {
    match slot {
        Value::String(s) if !s.is_empty() && !is_variable_reference(s) => {
            s.clear();
            true
        }
        Value::Number(_) | Value::Bool(_) => {
            *slot = Value::String(String::new());
            true
        }
        _ => false,
    }
}

/// Clears a variable value whatever its form: text, number, boolean, or the typed form
/// `{type, data}`, where only `data` is cleared. A lone `{{ref}}` is kept (design decision 4).
fn clear_secret_value(slot: &mut Value) -> bool {
    match slot {
        Value::Mapping(typed) => typed.get_mut("data").is_some_and(clear_secret_value),
        _ => blank_string(slot),
    }
}

fn walk(rel_path: &str, value: &mut Value, notes: &mut Vec<ExportNote>) {
    match value {
        Value::Mapping(map) => {
            let is_form_body = matches!(
                map.get("type").and_then(Value::as_str),
                Some("form-urlencoded" | "multipart-form")
            );
            for (key, child) in map.iter_mut() {
                match key.as_str() {
                    Some("auth") => {
                        if let Value::Mapping(auth) = child {
                            yaml_scrub_auth(rel_path, auth, notes);
                        }
                    }
                    // gRPC metadata rows have the same name and value shape as headers.
                    Some("headers" | "metadata") => scrub_headers(rel_path, child, notes),
                    Some("params") => scrub_rows(rel_path, child, "Query parameter", notes),
                    Some("data") if is_form_body => {
                        scrub_rows(rel_path, child, "Form field", notes);
                    }
                    Some("additionalParameters") => {
                        if let Value::Mapping(groups) = child {
                            for (_, rows) in groups.iter_mut() {
                                scrub_rows(rel_path, rows, "OAuth2 parameter", notes);
                            }
                        }
                    }
                    Some("clientCertificates") => scrub_certificates(rel_path, child, notes),
                    Some("variables") => scrub_variables(rel_path, child, notes),
                    Some("url") => yaml_scrub_url(rel_path, child, notes),
                    _ => {}
                }
                walk(rel_path, child, notes);
            }
        }
        Value::Sequence(items) => {
            for item in items {
                walk(rel_path, item, notes);
            }
        }
        _ => {}
    }
}

fn yaml_scrub_auth(rel_path: &str, auth: &mut Mapping, notes: &mut Vec<ExportNote>) {
    let is_apikey = auth.get("type").and_then(Value::as_str) == Some("apikey");
    let mut direct = vec![
        "password",
        "token",
        "secretAccessKey",
        "sessionToken",
        "consumerSecret",
        "accessToken",
        "accessTokenSecret",
    ];
    if is_apikey {
        direct.push("value");
    }
    for key in direct {
        if let Some(slot) = auth.get_mut(key) {
            if blank_string(slot) {
                notes.push(blank_note(rel_path, "Credential", &format!("auth.{key}")));
            }
        }
    }
    for key in ["accessTokenUrl", "refreshTokenUrl", "authorizationUrl", "callbackUrl"] {
        if let Some(slot) = auth.get_mut(key) {
            yaml_scrub_url(rel_path, slot, notes);
        }
    }
    for (parent, child) in [
        ("privateKey", "value"),
        ("credentials", "clientSecret"),
        ("resourceOwner", "password"),
    ] {
        if let Some(Value::Mapping(inner)) = auth.get_mut(parent) {
            if let Some(slot) = inner.get_mut(child) {
                if blank_string(slot) {
                    notes.push(blank_note(
                        rel_path,
                        "Credential",
                        &format!("auth.{parent}.{child}"),
                    ));
                }
            }
        }
    }
}

fn scrub_headers(rel_path: &str, headers: &mut Value, notes: &mut Vec<ExportNote>) {
    let Value::Sequence(items) = headers else {
        return;
    };
    for item in items {
        let Value::Mapping(row) = item else { continue };
        let name = match row.get("name").and_then(Value::as_str) {
            Some(n) if is_sensitive_header(n) => n.to_string(),
            _ => continue,
        };
        if let Some(slot) = row.get_mut("value") {
            if blank_string(slot) {
                notes.push(blank_note(rel_path, "Sensitive header", &name));
            }
        }
    }
}

/// True for a multipart file part, whose value is a path and not a secret.
fn is_file_part(row: &Mapping) -> bool {
    row.get("type").and_then(Value::as_str) == Some("file") || row.contains_key("filePath")
}

/// Blanks the `value` of each row whose `name` is secret-like. Disabled rows are blanked too.
/// A multipart value may be a list of texts, and each one is blanked.
fn scrub_rows(rel_path: &str, rows: &mut Value, feature: &str, notes: &mut Vec<ExportNote>) {
    let Value::Sequence(items) = rows else {
        return;
    };
    for item in items {
        let Value::Mapping(row) = item else { continue };
        if is_file_part(row) {
            continue;
        }
        let name = match row.get("name").and_then(Value::as_str) {
            Some(n) if is_sensitive_name(n) => n.to_string(),
            _ => continue,
        };
        let changed = match row.get_mut("value") {
            Some(Value::Sequence(parts)) => parts.iter_mut().fold(false, |c, p| blank_string(p) | c),
            Some(slot) => blank_string(slot),
            None => false,
        };
        if changed {
            notes.push(blank_note(rel_path, feature, &format!("{name} (by name)")));
        }
    }
}

/// Blanks the passphrase of each client certificate. A `{{variable}}` reference is kept.
fn scrub_certificates(rel_path: &str, certs: &mut Value, notes: &mut Vec<ExportNote>) {
    let Value::Sequence(items) = certs else {
        return;
    };
    for item in items {
        let Value::Mapping(cert) = item else { continue };
        if let Some(slot) = cert.get_mut("passphrase") {
            if blank_string(slot) {
                notes.push(blank_note(
                    rel_path,
                    "Certificate passphrase",
                    "clientCertificates.passphrase",
                ));
            }
        }
    }
}

fn scrub_variables(rel_path: &str, variables: &mut Value, notes: &mut Vec<ExportNote>) {
    let Value::Sequence(items) = variables else {
        return;
    };
    for item in items {
        let Value::Mapping(row) = item else { continue };
        let name = row
            .get("name")
            .or_else(|| row.get("key"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let secret = row.get("secret").and_then(Value::as_bool) == Some(true);
        if !secret && !is_sensitive_name(&name) {
            continue;
        }
        let mut changed = false;
        for field in ["value", "initial"] {
            if let Some(slot) = row.get_mut(field) {
                changed |= clear_secret_value(slot);
            }
        }
        if changed {
            notes.push(if secret {
                blank_note(rel_path, "Secret variable", &name)
            } else {
                blank_note(rel_path, "Variable", &format!("{name} (by name)"))
            });
        }
    }
}

fn yaml_scrub_url(rel_path: &str, url: &mut Value, notes: &mut Vec<ExportNote>) {
    let Value::String(text) = url else { return };
    scrub_url_password(rel_path, text, notes);
    scrub_url_query(rel_path, text, notes);
}

fn scrub_url_password(rel_path: &str, text: &mut String, notes: &mut Vec<ExportNote>) {
    let Some(scheme_end) = text.find("://") else {
        return;
    };
    let auth_start = scheme_end + 3;
    let auth_len = text[auth_start..]
        .find(['/', '?', '#'])
        .unwrap_or(text.len() - auth_start);
    let authority = &text[auth_start..auth_start + auth_len];
    let Some(at) = authority.rfind('@') else {
        return;
    };
    let userinfo = &authority[..at];
    let Some(colon) = userinfo.find(':') else {
        return;
    };
    let password = &userinfo[colon + 1..];
    if password.is_empty() || is_variable_reference(password) {
        return;
    }
    let pw_start = auth_start + colon + 1;
    let pw_end = pw_start + password.len();
    text.replace_range(pw_start..pw_end, "");
    notes.push(blank_note(rel_path, "URL password", "url"));
}

/// Blanks the value of each `name=value` query pair whose name is secret-like.
/// The fragment and pairs without `=` stay as they are. Returns the new text and the
/// blanked names, or `None` when nothing changed. Both the YAML and the typed scrub use it.
fn blank_query_values(text: &str) -> Option<(String, Vec<String>)> {
    let q = text.find('?')?;
    let query_start = q + 1;
    let query_end = text[query_start..]
        .find('#')
        .map_or(text.len(), |h| query_start + h);
    let mut blanked = Vec::new();
    let pairs: Vec<String> = text[query_start..query_end]
        .split('&')
        .map(|pair| match pair.split_once('=') {
            Some((name, value))
                if !value.is_empty() && !is_variable_reference(value) && is_sensitive_name(name) =>
            {
                blanked.push(name.to_string());
                format!("{name}=")
            }
            _ => pair.to_string(),
        })
        .collect();
    if blanked.is_empty() {
        return None;
    }
    let mut out = text.to_string();
    out.replace_range(query_start..query_end, &pairs.join("&"));
    Some((out, blanked))
}

fn scrub_url_query(rel_path: &str, text: &mut String, notes: &mut Vec<ExportNote>) {
    let Some((clean, blanked)) = blank_query_values(text) else {
        return;
    };
    *text = clean;
    for name in blanked {
        notes.push(blank_note(rel_path, "URL query", &format!("{name} (by name)")));
    }
}

/// Typed scrub of an `Auth` value: blanks literal credentials, keeps `{{refs}}`.
pub fn scrub_auth(auth: &Auth, path: &str, notes: &mut Vec<ExportNote>) -> Auth {
    let mut out = auth.clone();
    scrub_auth_urls(&mut out, path, notes);
    let mut blank = |slot: &mut String, field: &str| {
        if !slot.is_empty() && !is_variable_reference(slot) {
            slot.clear();
            notes.push(blank_note(path, "Credential", &format!("auth.{field}")));
        }
    };
    match &mut out {
        Auth::Basic { password, .. }
        | Auth::Digest { password, .. }
        | Auth::Wsse { password, .. }
        | Auth::Ntlm { password, .. } => blank(password, "password"),
        Auth::Bearer { token } => blank(token, "token"),
        Auth::ApiKey { value, .. } => blank(value, "value"),
        Auth::AwsSigV4 {
            secret_key,
            session_token,
            ..
        } => {
            blank(secret_key, "secretKey");
            if let Some(t) = session_token {
                blank(t, "sessionToken");
            }
        }
        Auth::OAuth2(flow) => match flow.as_mut() {
            OAuth2Flow::ClientCredentials { credentials, .. }
            | OAuth2Flow::AuthorizationCode { credentials, .. } => {
                blank(&mut credentials.client_secret, "clientSecret")
            }
            OAuth2Flow::ResourceOwnerPassword {
                credentials,
                resource_owner,
                ..
            } => {
                blank(&mut credentials.client_secret, "clientSecret");
                if let Some(ro) = resource_owner {
                    blank(&mut ro.password, "password");
                }
            }
            OAuth2Flow::Implicit { .. } => {}
        },
        Auth::OAuth1(o) => {
            for (slot, name) in [
                (&mut o.consumer_secret, "consumerSecret"),
                (&mut o.access_token, "accessToken"),
                (&mut o.access_token_secret, "accessTokenSecret"),
            ] {
                if let Some(v) = slot {
                    blank(v, name);
                }
            }
            if let Some(k) = &mut o.private_key {
                blank(&mut k.value, "privateKey");
            }
        }
        Auth::None | Auth::Inherit => {}
    }
    out
}

/// Scrubs the URL fields of OAuth2 flows and the OAuth1 callback like a request URL.
fn scrub_auth_urls(auth: &mut Auth, path: &str, notes: &mut Vec<ExportNote>) {
    let mut fix = |slot: &mut String| {
        if !slot.is_empty() {
            *slot = scrub_url(slot, path, notes);
        }
    };
    match auth {
        Auth::OAuth2(flow) => match flow.as_mut() {
            OAuth2Flow::ClientCredentials {
                access_token_url,
                refresh_token_url,
                ..
            }
            | OAuth2Flow::ResourceOwnerPassword {
                access_token_url,
                refresh_token_url,
                ..
            } => {
                fix(access_token_url);
                refresh_token_url.iter_mut().for_each(&mut fix);
            }
            OAuth2Flow::AuthorizationCode {
                authorization_url,
                access_token_url,
                refresh_token_url,
                callback_url,
                ..
            } => {
                fix(authorization_url);
                fix(access_token_url);
                refresh_token_url.iter_mut().for_each(&mut fix);
                callback_url.iter_mut().for_each(&mut fix);
            }
            OAuth2Flow::Implicit {
                authorization_url,
                callback_url,
                ..
            } => {
                fix(authorization_url);
                callback_url.iter_mut().for_each(&mut fix);
            }
        },
        Auth::OAuth1(o) => o.callback_url.iter_mut().for_each(&mut fix),
        _ => {}
    }
}

/// Blanks literal values of sensitive headers. References are kept.
pub fn scrub_header_values(
    headers: Vec<Header>,
    path: &str,
    notes: &mut Vec<ExportNote>,
) -> Vec<Header> {
    headers
        .into_iter()
        .map(|mut h| {
            if is_sensitive_header(&h.key) && !h.value.is_empty() && !is_variable_reference(&h.value)
            {
                h.value.clear();
                notes.push(blank_note(path, "Credential", &format!("header {}", h.key)));
            }
            h
        })
        .collect()
}

/// Blanks a literal password in the URL userinfo and the value of each secret-like query pair.
pub fn scrub_url(url: &str, path: &str, notes: &mut Vec<ExportNote>) -> String {
    let url = scrub_userinfo_password(url, path, notes);
    match blank_query_values(&url) {
        Some((clean, blanked)) => {
            for name in blanked {
                notes.push(blank_note(path, "URL query", &format!("{name} (by name)")));
            }
            clean
        }
        None => url,
    }
}

fn scrub_userinfo_password(url: &str, path: &str, notes: &mut Vec<ExportNote>) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    let Some((userinfo, host)) = authority.rsplit_once('@') else {
        return url.to_string();
    };
    let Some((user, password)) = userinfo.split_once(':') else {
        return url.to_string();
    };
    if password.is_empty() || is_variable_reference(password) {
        return url.to_string();
    }
    notes.push(blank_note(path, "Credential", "url password"));
    format!("{scheme}://{user}:@{host}{tail}")
}

/// Blanks the value of each row whose key is secret-like, disabled rows too.
/// A value that is only a `{{variable}}` reference is kept.
pub fn scrub_typed_rows(
    rows: Vec<Header>,
    feature: &str,
    path: &str,
    notes: &mut Vec<ExportNote>,
) -> Vec<Header> {
    rows.into_iter()
        .map(|mut h| {
            if is_sensitive_name(&h.key) && !h.value.is_empty() && !is_variable_reference(&h.value)
            {
                h.value.clear();
                notes.push(blank_note(path, feature, &format!("{} (by name)", h.key)));
            }
            h
        })
        .collect()
}

/// Blanks text entries of a form body by key. File entries hold a path and stay as they are.
pub fn scrub_form_entries(
    entries: Vec<FormDataEntry>,
    path: &str,
    notes: &mut Vec<ExportNote>,
) -> Vec<FormDataEntry> {
    entries
        .into_iter()
        .map(|mut e| {
            if e.entry_type == FormDataType::Text
                && is_sensitive_name(&e.key)
                && !e.value.is_empty()
                && !is_variable_reference(&e.value)
            {
                e.value.clear();
                notes.push(blank_note(path, "Form field", &format!("{} (by name)", e.key)));
            }
            e
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml(src: &str) -> Value {
        serde_yaml::from_str(src).expect("fixture yaml parses")
    }

    fn empty() -> Value {
        Value::String(String::new())
    }

    #[test]
    fn oauth1_access_token_is_blanked() {
        let auth = Auth::OAuth1(Box::new(rocket_shared::types::OAuth1Auth {
            access_token: Some("abc".into()),
            ..Default::default()
        }));
        let mut notes = Vec::new();
        let clean = scrub_auth(&auth, "r.yml", &mut notes);
        let Auth::OAuth1(o) = clean else {
            panic!("expected oauth1");
        };
        assert!(o.access_token.as_deref().unwrap_or("").is_empty());
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].action, ExportAction::Blanked);
    }

    #[test]
    fn is_sensitive_header_covers_the_five_names() {
        for name in [
            "authorization",
            "Proxy-Authorization",
            "COOKIE",
            "Set-Cookie",
            "X-API-KEY",
        ] {
            assert!(is_sensitive_header(name), "{name} should be sensitive");
        }
    }

    #[test]
    fn blanks_literal_bearer_token_and_keeps_reference() {
        let mut v = yaml("http:\n  auth:\n    type: bearer\n    token: abc123\n");
        let mut notes = Vec::new();
        scrub_yaml("users/get.yml", &mut v, &mut notes);
        assert_eq!(v["http"]["auth"]["token"], empty());
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].action, ExportAction::Blanked);
        assert_eq!(notes[0].path, "users/get.yml");

        let mut kept = yaml("http:\n  auth:\n    type: bearer\n    token: \"{{authToken}}\"\n");
        let mut none = Vec::new();
        scrub_yaml("users/get.yml", &mut kept, &mut none);
        assert_eq!(
            kept["http"]["auth"]["token"],
            Value::String("{{authToken}}".into())
        );
        assert!(none.is_empty());
    }

    #[test]
    fn blanks_credentials_for_each_auth_type() {
        let src = r#"
items:
  - http:
      auth: {type: basic, username: u, password: p}
  - http:
      auth: {type: digest, username: u, password: p}
  - http:
      auth: {type: wsse, username: u, password: p}
  - http:
      auth: {type: ntlm, username: u, password: p, domain: d}
  - http:
      auth: {type: apikey, key: X-Key, value: secret, placement: header}
  - http:
      auth: {type: awsv4, accessKeyId: AKIA, secretAccessKey: s, sessionToken: t, region: r}
  - http:
      auth:
        type: oauth1
        consumerKey: ck
        consumerSecret: cs
        accessTokenSecret: ats
        privateKey: {type: text, value: pem}
  - http:
      auth:
        type: oauth2
        flow: password
        accessTokenUrl: https://t
        credentials: {clientId: cid, clientSecret: csec}
        resourceOwner: {username: ro, password: rop}
"#;
        let mut v = yaml(src);
        let mut notes = Vec::new();
        scrub_yaml("a.yml", &mut v, &mut notes);
        let auth = |i: usize| &v["items"][i]["http"]["auth"];
        assert_eq!(auth(0)["password"], empty());
        assert_eq!(auth(0)["username"], Value::String("u".into()));
        assert_eq!(auth(1)["password"], empty());
        assert_eq!(auth(2)["password"], empty());
        assert_eq!(auth(3)["password"], empty());
        assert_eq!(auth(3)["domain"], Value::String("d".into()));
        assert_eq!(auth(4)["value"], empty());
        assert_eq!(auth(4)["key"], Value::String("X-Key".into()));
        assert_eq!(auth(5)["secretAccessKey"], empty());
        assert_eq!(auth(5)["sessionToken"], empty());
        assert_eq!(auth(5)["accessKeyId"], Value::String("AKIA".into()));
        assert_eq!(auth(6)["consumerSecret"], empty());
        assert_eq!(auth(6)["accessTokenSecret"], empty());
        assert_eq!(auth(6)["privateKey"]["value"], empty());
        assert_eq!(auth(6)["consumerKey"], Value::String("ck".into()));
        assert_eq!(auth(7)["credentials"]["clientSecret"], empty());
        assert_eq!(auth(7)["resourceOwner"]["password"], empty());
        assert_eq!(auth(7)["resourceOwner"]["username"], Value::String("ro".into()));
        assert_eq!(auth(7)["credentials"]["clientId"], Value::String("cid".into()));
        assert_eq!(auth(7)["accessTokenUrl"], Value::String("https://t".into()));
    }

    #[test]
    fn scrubs_collection_and_folder_auth() {
        let mut collection = yaml("request:\n  auth:\n    type: basic\n    username: u\n    password: p\n");
        let mut folder = yaml("request:\n  auth:\n    type: bearer\n    token: t\n");
        let mut req = yaml("runtime:\n  auth:\n    type: bearer\n    token: t2\n");
        let mut notes = Vec::new();
        scrub_yaml("opencollection.yml", &mut collection, &mut notes);
        scrub_yaml("f/folder.yml", &mut folder, &mut notes);
        scrub_yaml("f/r.yml", &mut req, &mut notes);
        assert_eq!(collection["request"]["auth"]["password"], empty());
        assert_eq!(folder["request"]["auth"]["token"], empty());
        assert_eq!(req["runtime"]["auth"]["token"], empty());
        assert_eq!(notes.len(), 3);
    }

    #[test]
    fn blanks_secret_variable_values() {
        let src = r#"
runtime:
  variables:
    - {name: a, value: s1, initial: i1, secret: true}
    - {name: b, value: plain}
request:
  variables:
    - {name: c, value: s2, secret: true}
"#;
        let mut v = yaml(src);
        let mut notes = Vec::new();
        scrub_yaml("r.yml", &mut v, &mut notes);
        assert_eq!(v["runtime"]["variables"][0]["value"], empty());
        assert_eq!(v["runtime"]["variables"][0]["initial"], empty());
        assert_eq!(v["runtime"]["variables"][1]["value"], Value::String("plain".into()));
        assert_eq!(v["request"]["variables"][0]["value"], empty());
        assert_eq!(notes.len(), 2);
    }

    #[test]
    fn blanks_literal_authorization_header() {
        let mut v = yaml("http:\n  headers:\n    - name: Authorization\n      value: Bearer abc123\n");
        let mut notes = Vec::new();
        scrub_yaml("r.yml", &mut v, &mut notes);
        assert_eq!(v["http"]["headers"][0]["value"], empty());
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn keeps_bearer_prefix_with_reference() {
        let mut v = yaml("http:\n  headers:\n    - name: authorization\n      value: \"Bearer {{token}}\"\n");
        let mut notes = Vec::new();
        scrub_yaml("r.yml", &mut v, &mut notes);
        assert_eq!(
            v["http"]["headers"][0]["value"],
            Value::String("Bearer {{token}}".into())
        );
        assert!(notes.is_empty());
    }

    #[test]
    fn blanks_url_password() {
        let mut a = yaml("http:\n  url: \"https://user:pass@host/x\"\n");
        let mut b = yaml("http:\n  url: \"https://user:{{pw}}@host/x\"\n");
        let mut notes = Vec::new();
        scrub_yaml("r.yml", &mut a, &mut notes);
        scrub_yaml("r.yml", &mut b, &mut notes);
        assert_eq!(a["http"]["url"], Value::String("https://user:@host/x".into()));
        assert_eq!(b["http"]["url"], Value::String("https://user:{{pw}}@host/x".into()));
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn strip_removes_rocketapi_and_keeps_other_extensions() {
        let mut v = yaml(
            "name: c\nextensions:\n  rocketapi:\n    sandboxMode: developer\n    keep: me\n  bruno:\n    other: 1\n  other:\n    x: 1\n",
        );
        assert!(strip_rocket_namespace(&mut v));
        assert!(v["extensions"].get("rocketapi").is_none());
        assert_eq!(v["extensions"]["bruno"]["other"], Value::Number(1.into()));
        assert_eq!(v["extensions"]["other"]["x"], Value::Number(1.into()));
        assert!(!strip_rocket_namespace(&mut v));

        let mut only = yaml("extensions:\n  rocketapi:\n    a: 1\n");
        assert!(strip_rocket_namespace(&mut only));
        assert!(only.get("extensions").is_none());
    }

    #[test]
    fn is_variable_reference_cases() {
        assert!(is_variable_reference("{{a}}"));
        assert!(is_variable_reference("Bearer {{a}}"));
        assert!(!is_variable_reference("abc"));
        assert!(!is_variable_reference("{{a}}x"));
        assert!(!is_variable_reference(""));
    }

    #[test]
    fn sensitive_header_names_are_case_insensitive() {
        assert!(is_sensitive_header("X-API-Key"));
        assert!(is_sensitive_header("Set-Cookie"));
        assert!(!is_sensitive_header("Accept"));
    }

    #[test]
    fn blanks_sensitive_grpc_metadata_and_keeps_others() {
        let mut v = yaml(
            "grpc:\n  metadata:\n    - name: authorization\n      value: Bearer LEAK1\n    - name: x-trace\n      value: keep\n    - name: Cookie\n      value: \"{{c}}\"\n",
        );
        let mut notes = Vec::new();
        scrub_yaml("rpc/get.yml", &mut v, &mut notes);
        assert_eq!(v["grpc"]["metadata"][0]["value"], empty());
        assert_eq!(v["grpc"]["metadata"][1]["value"], Value::String("keep".into()));
        assert_eq!(v["grpc"]["metadata"][2]["value"], Value::String("{{c}}".into()));
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].action, ExportAction::Blanked);
    }

    #[test]
    fn blanks_client_certificate_passphrase_in_collection_and_environment() {
        let mut collection = yaml(
            "config:\n  clientCertificates:\n    - type: pkcs12\n      domain: a.test\n      passphrase: LEAK3\n    - type: pem\n      domain: b.test\n      passphrase: \"{{vault.p}}\"\n",
        );
        let mut notes = Vec::new();
        scrub_yaml("opencollection.yml", &mut collection, &mut notes);
        let certs = &collection["config"]["clientCertificates"];
        assert_eq!(certs[0]["passphrase"], empty());
        assert_eq!(certs[0]["domain"], Value::String("a.test".into()));
        assert_eq!(certs[1]["passphrase"], Value::String("{{vault.p}}".into()));
        assert_eq!(notes.len(), 1);

        let mut env = yaml(
            "name: prod\nclientCertificates:\n  - type: pkcs12\n    domain: a.test\n    passphrase: LEAK4\n",
        );
        let mut env_notes = Vec::new();
        scrub_yaml("environments/prod.yml", &mut env, &mut env_notes);
        assert_eq!(env["clientCertificates"][0]["passphrase"], empty());
        assert_eq!(env_notes.len(), 1);
    }

    #[test]
    fn blanks_typed_secret_variable_data_and_keeps_its_type() {
        let mut v = yaml(
            "request:\n  variables:\n    - name: k\n      secret: true\n      value:\n        type: string\n        data: LEAK2\n    - name: plain\n      value:\n        type: string\n        data: keep\n",
        );
        let mut notes = Vec::new();
        scrub_yaml("a.yml", &mut v, &mut notes);
        let vars = &v["request"]["variables"];
        assert_eq!(vars[0]["value"]["data"], empty());
        assert_eq!(vars[0]["value"]["type"], Value::String("string".into()));
        assert_eq!(vars[1]["value"]["data"], Value::String("keep".into()));
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn blanks_non_string_scalars_in_credential_slots() {
        let mut v = yaml("http:\n  auth:\n    type: bearer\n    token: 123456789\n");
        let mut notes = Vec::new();
        scrub_yaml("a.yml", &mut v, &mut notes);
        assert_eq!(v["http"]["auth"]["token"], empty());
        assert_eq!(notes.len(), 1);

        let mut b = yaml("http:\n  auth:\n    type: basic\n    username: u\n    password: true\n");
        let mut b_notes = Vec::new();
        scrub_yaml("b.yml", &mut b, &mut b_notes);
        assert_eq!(b["http"]["auth"]["password"], empty());
    }

    #[test]
    fn blanks_oauth1_access_token() {
        let mut v = yaml(
            "http:\n  auth:\n    type: oauth1\n    consumerKey: ck\n    accessToken: LEAK7\n    accessTokenSecret: LEAK9\n",
        );
        let mut notes = Vec::new();
        scrub_yaml("a.yml", &mut v, &mut notes);
        assert_eq!(v["http"]["auth"]["accessToken"], empty());
        assert_eq!(v["http"]["auth"]["accessTokenSecret"], empty());
        assert_eq!(v["http"]["auth"]["consumerKey"], Value::String("ck".into()));
        assert_eq!(notes.len(), 2);
    }

    #[test]
    fn params_are_scrubbed_by_name() {
        let mut v = yaml(
            "http:\n  params:\n    - {name: access_token, value: abc, type: query}\n    - {name: page_token, value: p1, type: query}\n    - {name: author, value: me, type: query}\n",
        );
        let mut notes = Vec::new();
        scrub_yaml("r.yml", &mut v, &mut notes);
        let p = &v["http"]["params"];
        assert_eq!(p[0]["value"], empty());
        assert_eq!(p[1]["value"], Value::String("p1".into()));
        assert_eq!(p[2]["value"], Value::String("me".into()));
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].action, ExportAction::Blanked);
        assert_eq!(notes[0].feature, "Query parameter");
        assert_eq!(notes[0].detail.as_deref(), Some("access_token (by name)"));
    }

    #[test]
    fn references_in_sensitive_rows_are_kept() {
        let mut v = yaml(
            "http:\n  url: \"https://x.test/p?access_token={{t}}\"\n  params:\n    - {name: api_key, value: \"{{k}}\", type: query}\n  headers:\n    - {name: X-Auth-Token, value: \"{{t}}\"}\n",
        );
        let before = v.clone();
        let mut notes = Vec::new();
        scrub_yaml("r.yml", &mut v, &mut notes);
        assert_eq!(v, before);
        assert!(notes.is_empty());
    }

    #[test]
    fn headers_are_blanked_by_name() {
        let mut v = yaml(
            "http:\n  headers:\n    - {name: X-Auth-Token, value: abc}\n    - {name: X-Request-Id, value: \"1\"}\n    - {name: Authorization, value: Bearer abc}\n",
        );
        let mut notes = Vec::new();
        scrub_yaml("r.yml", &mut v, &mut notes);
        let h = &v["http"]["headers"];
        assert_eq!(h[0]["value"], empty());
        assert_eq!(h[1]["value"], Value::String("1".into()));
        assert_eq!(h[2]["value"], empty());
        assert_eq!(notes.len(), 2);
    }

    #[test]
    fn grpc_and_websocket_rows_use_the_name_rule() {
        let mut v = yaml(
            "grpc:\n  metadata:\n    - {name: x-auth-token, value: abc}\nwebsocket:\n  headers:\n    - {name: Cookie, value: a=b}\n",
        );
        let mut notes = Vec::new();
        scrub_yaml("r.yml", &mut v, &mut notes);
        assert_eq!(v["grpc"]["metadata"][0]["value"], empty());
        assert_eq!(v["websocket"]["headers"][0]["value"], empty());
        assert_eq!(notes.len(), 2);
    }

    #[test]
    fn form_rows_are_scrubbed_by_name() {
        let mut url = yaml(
            "http:\n  body:\n    type: form-urlencoded\n    data:\n      - {name: client_secret, value: s}\n      - {name: grant_type, value: password}\n",
        );
        let mut multi = yaml(
            "http:\n  body:\n    type: multipart-form\n    data:\n      - {name: password, type: text, value: p}\n      - {name: secret_file, type: file, value: [a.bin]}\n",
        );
        let mut notes = Vec::new();
        scrub_yaml("a.yml", &mut url, &mut notes);
        scrub_yaml("b.yml", &mut multi, &mut notes);
        let u = &url["http"]["body"]["data"];
        assert_eq!(u[0]["value"], empty());
        assert_eq!(u[1]["value"], Value::String("password".into()));
        let m = &multi["http"]["body"]["data"];
        assert_eq!(m[0]["value"], empty());
        assert_eq!(m[1]["value"][0], Value::String("a.bin".into()));
        assert_eq!(notes.len(), 2);
        assert!(notes.iter().all(|n| n.feature == "Form field"));
    }

    #[test]
    fn additional_parameters_are_scrubbed_by_name() {
        let mut v = yaml(
            "http:\n  auth:\n    type: oauth2\n    flow: client_credentials\n    additionalParameters:\n      accessTokenRequest:\n        - {name: client_assertion, value: jwt, placement: body, enabled: true}\n        - {name: audience, value: api}\n",
        );
        let mut notes = Vec::new();
        scrub_yaml("r.yml", &mut v, &mut notes);
        let rows = &v["http"]["auth"]["additionalParameters"]["accessTokenRequest"];
        assert_eq!(rows[0]["value"], empty());
        assert_eq!(rows[1]["value"], Value::String("api".into()));
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].feature, "OAuth2 parameter");
    }

    #[test]
    fn url_query_pairs_are_scrubbed_by_name() {
        let mut a = yaml("http:\n  url: \"https://x.test/p?a=1&access_token=abc&token_type=Bearer&sig=ok\"\n");
        let mut b = yaml("http:\n  url: \"https://x.test/p\"\n");
        let mut c = yaml("http:\n  url: \"https://u:pw@x.test/p?access_token=abc#frag\"\n");
        let mut notes = Vec::new();
        scrub_yaml("r.yml", &mut a, &mut notes);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].feature, "URL query");
        scrub_yaml("r.yml", &mut b, &mut notes);
        scrub_yaml("r.yml", &mut c, &mut notes);
        assert_eq!(
            a["http"]["url"],
            Value::String("https://x.test/p?a=1&access_token=&token_type=Bearer&sig=ok".into())
        );
        assert_eq!(b["http"]["url"], Value::String("https://x.test/p".into()));
        assert_eq!(
            c["http"]["url"],
            Value::String("https://u:@x.test/p?access_token=#frag".into())
        );
    }

    #[test]
    fn oauth_url_fields_are_scrubbed() {
        let mut v = yaml(
            "http:\n  auth:\n    type: oauth2\n    accessTokenUrl: \"https://u:pw@idp.test/token?client_secret=x&a=1\"\n    authorizationUrl: \"https://idp.test/auth?state=s\"\n    refreshTokenUrl: \"{{base}}/r?token={{t}}\"\n",
        );
        let mut o1 = yaml("http:\n  auth:\n    type: oauth1\n    callbackUrl: \"https://u:pw@app.test/cb\"\n");
        let mut notes = Vec::new();
        scrub_yaml("r.yml", &mut v, &mut notes);
        scrub_yaml("r.yml", &mut o1, &mut notes);
        let a = &v["http"]["auth"];
        assert_eq!(
            a["accessTokenUrl"],
            Value::String("https://u:@idp.test/token?client_secret=&a=1".into())
        );
        assert_eq!(a["authorizationUrl"], Value::String("https://idp.test/auth?state=s".into()));
        assert_eq!(a["refreshTokenUrl"], Value::String("{{base}}/r?token={{t}}".into()));
        assert_eq!(
            o1["http"]["auth"]["callbackUrl"],
            Value::String("https://u:@app.test/cb".into())
        );
    }

    #[test]
    fn secret_variable_keeps_reference_and_blanks_literal() {
        let mut v = yaml(
            "request:\n  variables:\n    - {name: a, secret: true, value: \"{{ref}}\"}\n    - {name: b, secret: true, value: top, initial: top}\n    - name: c\n      secret: true\n      value: {type: secret, data: lit}\n",
        );
        let mut notes = Vec::new();
        scrub_yaml("r.yml", &mut v, &mut notes);
        let vars = &v["request"]["variables"];
        assert_eq!(vars[0]["value"], Value::String("{{ref}}".into()));
        assert_eq!(vars[1]["value"], empty());
        assert_eq!(vars[1]["initial"], empty());
        assert_eq!(vars[2]["value"]["data"], empty());
        assert_eq!(notes.len(), 2);
        assert!(notes.iter().all(|n| n.feature == "Secret variable"));
    }

    #[test]
    fn variables_are_blanked_by_name() {
        let rows = "    - {name: api_token, value: abc}\n    - {name: plain_name, value: abc}\n    - {name: db_password, value: \"{{ref}}\"}\n";
        let mut coll = yaml(&format!("request:\n  variables:\n{rows}"));
        let mut env = yaml(&format!("name: dev\nvariables:\n{}", rows.replace("    -", "  -")));
        let mut notes = Vec::new();
        scrub_yaml("opencollection.yml", &mut coll, &mut notes);
        scrub_yaml("environments/dev.yml", &mut env, &mut notes);
        for vars in [&coll["request"]["variables"], &env["variables"]] {
            assert_eq!(vars[0]["value"], empty());
            assert_eq!(vars[1]["value"], Value::String("abc".into()));
            assert_eq!(vars[2]["value"], Value::String("{{ref}}".into()));
        }
        assert_eq!(notes.len(), 2);
        assert!(notes.iter().all(|n| n.feature == "Variable"));
        assert_eq!(notes[0].detail.as_deref(), Some("api_token (by name)"));
    }
}
