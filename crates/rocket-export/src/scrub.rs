//! YAML scrub: blanks credentials and strips the Rocket extension namespace.
//!
//! The walk goes through `serde_yaml::Value` by key, never through typed
//! structs, so unknown keys survive.

use crate::report::{ExportAction, ExportNote};
use crate::sensitive::is_sensitive_name;
use rocket_shared::oauth2::OAuth2Flow;
use rocket_shared::types::{Auth, Header};
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

/// Clears a secret variable value whatever its form: text, number, boolean, or the typed
/// form `{type, data}`, where only `data` is cleared. A reference is blanked too.
fn clear_secret_value(slot: &mut Value) -> bool {
    match slot {
        Value::String(s) if !s.is_empty() => {
            s.clear();
            true
        }
        Value::Number(_) | Value::Bool(_) => {
            *slot = Value::String(String::new());
            true
        }
        Value::Mapping(typed) => typed.get_mut("data").is_some_and(clear_secret_value),
        _ => false,
    }
}

fn walk(rel_path: &str, value: &mut Value, notes: &mut Vec<ExportNote>) {
    match value {
        Value::Mapping(map) => {
            for (key, child) in map.iter_mut() {
                match key.as_str() {
                    Some("auth") => {
                        if let Value::Mapping(auth) = child {
                            yaml_scrub_auth(rel_path, auth, notes);
                        }
                    }
                    // gRPC metadata rows have the same name and value shape as headers.
                    Some("headers" | "metadata") => scrub_headers(rel_path, child, notes),
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
        if row.get("secret").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let name = row
            .get("name")
            .or_else(|| row.get("key"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let mut changed = false;
        for field in ["value", "initial"] {
            if let Some(slot) = row.get_mut(field) {
                // Secret values are always blanked, even a reference, per the spec.
                changed |= clear_secret_value(slot);
            }
        }
        if changed {
            notes.push(blank_note(rel_path, "Secret variable", &name));
        }
    }
}

fn yaml_scrub_url(rel_path: &str, url: &mut Value, notes: &mut Vec<ExportNote>) {
    let Value::String(text) = url else { return };
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

/// Typed scrub of an `Auth` value: blanks literal credentials, keeps `{{refs}}`.
pub fn scrub_auth(auth: &Auth, path: &str, notes: &mut Vec<ExportNote>) -> Auth {
    let mut out = auth.clone();
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

/// Blanks a literal password in the URL userinfo.
pub fn scrub_url(url: &str, path: &str, notes: &mut Vec<ExportNote>) -> String {
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
}
