//! Table tests for contract check matching, verdicts and redaction.

use std::path::PathBuf;

use chrono::Utc;
use rocket_collection::contract::snapshot::KeyValueEntry;
use rocket_collection::contract::RequestSignatureSnapshot;
use rocket_network::{ContractChange, ContractChangeType, ContractPolicyWire, ContractVerdict};

use super::contract_check_match::{
    normalize_path, paths_match, redact_change, verdicts, LocalRequest,
};

fn kv(key: &str, value: &str) -> KeyValueEntry {
    KeyValueEntry {
        key: key.into(),
        value: value.into(),
    }
}

fn sig(method: &str, url: &str) -> RequestSignatureSnapshot {
    RequestSignatureSnapshot {
        request_path: PathBuf::from("r.yml"),
        method: method.into(),
        url_pattern: url.into(),
        headers: Vec::new(),
        query_params: Vec::new(),
        body_content: None,
        form_fields: Vec::new(),
        auth_type: "none".into(),
        auth_detail: String::new(),
        captured_at: Utc::now(),
        query_param_keys: Vec::new(),
        header_keys: Vec::new(),
        body_field_keys: Vec::new(),
    }
}

fn local(signature: RequestSignatureSnapshot) -> LocalRequest {
    LocalRequest {
        collection: "Shop".into(),
        name: "Get user".into(),
        signature,
    }
}

fn change(
    field: &str,
    change_type: ContractChangeType,
    old: Option<&str>,
    new: Option<&str>,
    breaking: bool,
) -> ContractChange {
    ContractChange {
        method: "GET".into(),
        path_template: "/users/{id}".into(),
        field: field.into(),
        change_type,
        old_value: old.map(Into::into),
        new_value: new.map(Into::into),
        breaking,
    }
}

fn verdict_of(c: ContractChange, s: RequestSignatureSnapshot) -> ContractVerdict {
    let out = verdicts(&[c], ContractPolicyWire::Strict, &[local(s)]);
    assert_eq!(out.len(), 1);
    out[0].0
}

#[test]
fn normalize_and_match_table() {
    assert_eq!(
        normalize_path("https://h.io/users/42?x=1"),
        vec!["users".to_string(), "42".to_string()]
    );
    assert_eq!(
        normalize_path("{{base}}/users/{id}/:x"),
        vec!["users".to_string(), "*".to_string(), "*".to_string()]
    );
    assert_eq!(normalize_path("/a/{{v}}/#frag"), vec!["a", "*"]);
    let table = [
        ("/users/:id", "https://h.io/users/42?x=1", true),
        ("{{base}}/users/{id}", "/users/7", true),
        ("/users/{id}", "{{baseUrl}}/users/{{userId}}", true),
        ("/users", "/users/7", false),
        ("/Users", "/users", false),
        ("/users/{id}/orders", "/users/7/items", false),
        ("/", "https://h.io", true),
    ];
    for (template, url, want) in table {
        assert_eq!(paths_match(template, url), want, "{template} vs {url}");
    }
    // The method must be equal ignoring case.
    let c = change(
        "header:X-New",
        ContractChangeType::Added,
        None,
        Some("1"),
        true,
    );
    assert_eq!(
        verdict_of(c.clone(), sig("get", "/users/7")),
        ContractVerdict::Break
    );
    assert_eq!(
        verdict_of(c, sig("POST", "/users/7")),
        ContractVerdict::NotUsed
    );
}

#[test]
fn verdict_table() {
    use ContractChangeType::{Added, Changed, Removed};
    use ContractVerdict::{Break, Pass};

    let url = "{{base}}/users/{{id}}";
    let plain = sig("GET", url);
    let mut with_tenant = sig("GET", url);
    with_tenant.headers = vec![kv("x-tenant", "acme")];
    let mut with_legacy = sig("GET", url);
    with_legacy.query_params = vec![kv("legacy", "1")];
    let mut page1 = sig("GET", url);
    page1.query_params = vec![kv("page", "1")];
    let mut page2 = sig("GET", url);
    page2.query_params = vec![kv("page", "2")];
    let mut page_var = sig("GET", url);
    page_var.query_params = vec![kv("page", "{{page}}")];
    let mut bearer = sig("GET", url);
    bearer.auth_type = "bearer".into();
    let mut basic = sig("GET", url);
    basic.auth_type = "basic".into();
    let mut inherit = sig("GET", url);
    inherit.auth_type = "inherit".into();
    let mut json_body = sig("GET", url);
    json_body.body_content = Some("{\"a\":1}".into());
    let mut form_body = sig("GET", url);
    form_body.form_fields = vec![kv("password", "x"), kv("name", "n")];
    let path_named = sig("GET", "/users/{{id}}");
    let mut with_token = sig("GET", url);
    with_token.headers = vec![kv("Authorization", "Bearer abc")];
    let literal_path = sig("GET", "/users/42");

    let table: Vec<(
        &str,
        ContractChange,
        RequestSignatureSnapshot,
        ContractVerdict,
    )> = vec![
        (
            "added breaking header, request lacks it",
            change("header:X-Tenant", Added, None, Some("t"), true),
            plain.clone(),
            Break,
        ),
        (
            "added breaking header, request sends it (any case)",
            change("header:X-Tenant", Added, None, Some("t"), true),
            with_tenant.clone(),
            Pass,
        ),
        (
            "added header, not breaking",
            change("header:X-Tenant", Added, None, Some("t"), false),
            plain.clone(),
            Pass,
        ),
        (
            "removed query, request still sends it",
            change("query:legacy", Removed, Some("1"), None, true),
            with_legacy.clone(),
            Break,
        ),
        (
            "removed query, request does not send it",
            change("query:legacy", Removed, Some("1"), None, true),
            plain.clone(),
            Pass,
        ),
        (
            "removed query, not breaking",
            change("query:legacy", Removed, Some("1"), None, false),
            with_legacy.clone(),
            Pass,
        ),
        (
            "changed query, request sends the old value",
            change("query:page", Changed, Some("1"), Some("2"), true),
            page1.clone(),
            Break,
        ),
        (
            "changed query, request sends the new value",
            change("query:page", Changed, Some("1"), Some("2"), true),
            page2.clone(),
            Pass,
        ),
        (
            "changed query, request does not send it",
            change("query:page", Changed, Some("1"), Some("2"), true),
            plain.clone(),
            Pass,
        ),
        (
            "changed query, request value is a variable",
            change("query:page", Changed, Some("1"), Some("2"), true),
            page_var.clone(),
            Pass,
        ),
        (
            "changed query, not breaking",
            change("query:page", Changed, Some("1"), Some("2"), false),
            page1.clone(),
            Pass,
        ),
        (
            "query names are case-sensitive",
            change("query:Legacy", Removed, Some("1"), None, true),
            with_legacy.clone(),
            Pass,
        ),
        (
            "auth changed to the request's type",
            change("auth", Changed, Some("basic"), Some("bearer"), true),
            bearer.clone(),
            Pass,
        ),
        (
            "auth changed to another type",
            change("auth", Changed, Some("basic"), Some("bearer"), true),
            basic.clone(),
            Break,
        ),
        (
            "auth changed, request inherits",
            change("auth", Changed, Some("basic"), Some("bearer"), true),
            inherit.clone(),
            Pass,
        ),
        (
            "auth removed, request still sends auth",
            change("auth", Removed, Some("basic"), None, true),
            basic.clone(),
            Break,
        ),
        (
            "body removed, request has a body",
            change("body", Removed, Some("{}"), None, true),
            json_body.clone(),
            Break,
        ),
        (
            "body changed, request has a form body",
            change("body", Changed, Some("{}"), Some("[]"), true),
            form_body.clone(),
            Break,
        ),
        (
            "body changed, request without a body",
            change("body", Changed, Some("{}"), Some("[]"), true),
            plain.clone(),
            Pass,
        ),
        (
            "body path removed, request has a body",
            change("body:$.a.b", Removed, Some("1"), None, true),
            json_body.clone(),
            Break,
        ),
        (
            "body path removed, not breaking",
            change("body:$.a.b", Removed, Some("1"), None, false),
            json_body.clone(),
            Pass,
        ),
        (
            "body path added, request has a body",
            change("body:$.a.b", Added, None, Some("1"), true),
            json_body.clone(),
            Pass,
        ),
        (
            "body added, request without a body",
            change("body", Added, None, Some("{}"), true),
            plain.clone(),
            Break,
        ),
        (
            "form field removed, request sends it",
            change("form:password", Removed, Some("x"), None, true),
            form_body.clone(),
            Break,
        ),
        (
            "form field added, request lacks it",
            change("form:email", Added, None, Some("e"), true),
            form_body.clone(),
            Break,
        ),
        (
            "path variable removed, request uses that name",
            change("path:id", Removed, None, None, true),
            path_named.clone(),
            Break,
        ),
        (
            "path variable removed, request uses a literal",
            change("path:id", Removed, None, None, true),
            literal_path.clone(),
            Pass,
        ),
        (
            "path variable added, request matched the template",
            change("path:id", Added, None, None, true),
            literal_path.clone(),
            Pass,
        ),
        (
            "unknown field is never a break",
            change("status", Changed, Some("200"), Some("204"), true),
            plain.clone(),
            Pass,
        ),
        (
            "changed header with a hidden new value is unknown",
            change(
                "header:Authorization",
                Changed,
                Some("[hidden]"),
                Some("[hidden]"),
                true,
            ),
            with_token.clone(),
            Pass,
        ),
        (
            "changed header with a real differing value",
            change(
                "header:Authorization",
                Changed,
                None,
                Some("Bearer x"),
                true,
            ),
            with_token.clone(),
            Break,
        ),
        (
            "auth changed to a hidden value is unknown",
            change("auth", Changed, Some("[hidden]"), Some("[hidden]"), true),
            bearer.clone(),
            Pass,
        ),
        (
            "auth added with a hidden value is unknown",
            change("auth", Added, None, Some("[hidden]"), true),
            bearer.clone(),
            Pass,
        ),
        (
            "auth changed to a real differing value",
            change("auth", Changed, None, Some("basic"), true),
            bearer.clone(),
            Break,
        ),
        (
            "auth added with a real differing value",
            change("auth", Added, None, Some("basic"), true),
            bearer.clone(),
            Break,
        ),
    ];
    for (name, c, s, want) in table {
        assert_eq!(verdict_of(c, s), want, "{name}");
    }
}

#[test]
fn worst_verdict_wins() {
    let mut sends = sig("GET", "/users/1");
    sends.headers = vec![kv("X-Tenant", "a")];
    let lacks = sig("GET", "/users/2");
    let other = sig("GET", "/orders/2");
    let locals = [local(sends), local(other), local(lacks)];
    let c = change(
        "header:X-Tenant",
        ContractChangeType::Added,
        None,
        Some("t"),
        true,
    );
    let out = verdicts(&[c], ContractPolicyWire::Strict, &locals);
    assert_eq!(out, vec![(ContractVerdict::Break, vec![0, 2])]);
}

#[test]
fn no_match_is_not_used() {
    let locals = [local(sig("GET", "/orders/1"))];
    let changes = [
        change("query:a", ContractChangeType::Added, None, None, true),
        change("query:b", ContractChangeType::Removed, None, None, true),
    ];
    let out = verdicts(&changes, ContractPolicyWire::Lenient, &locals);
    assert_eq!(
        out,
        vec![
            (ContractVerdict::NotUsed, Vec::new()),
            (ContractVerdict::NotUsed, Vec::new()),
        ]
    );
    assert!(verdicts(&changes, ContractPolicyWire::Strict, &[])
        .iter()
        .all(|(v, idx)| *v == ContractVerdict::NotUsed && idx.is_empty()));
}

#[test]
fn redaction_table() {
    let hidden = Some("[hidden]".to_string());
    for field in [
        "header:Authorization",
        "query:api_key",
        "auth",
        "form:password",
        "form:pass",
        "body:$.user.password",
        "auth_type",
        "auth_detail",
        "Auth",
        "AUTH:x",
        "auth.x",
        "body",
        "body:$.password",
        "body:$.a.b",
    ] {
        let c = change(
            field,
            ContractChangeType::Changed,
            Some("a"),
            Some("b"),
            true,
        );
        let out = redact_change(c.clone());
        assert_eq!(out.old_value, hidden, "{field}");
        assert_eq!(out.new_value, hidden, "{field}");
        assert_eq!(out.field, c.field);
        assert_eq!(out.method, c.method);
        assert_eq!(out.path_template, c.path_template);

        let added = change(field, ContractChangeType::Added, None, Some("b"), true);
        let out = redact_change(added);
        assert_eq!(out.old_value, None, "{field}");
        assert_eq!(out.new_value, hidden, "{field}");
    }
    let accept = change(
        "header:Accept",
        ContractChangeType::Changed,
        Some("text/plain"),
        Some("application/json"),
        true,
    );
    assert_eq!(redact_change(accept.clone()), accept);
    let none = change("auth", ContractChangeType::Removed, None, None, true);
    assert_eq!(redact_change(none.clone()), none);
}
