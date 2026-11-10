use std::path::PathBuf;

use chrono::Utc;
use rocket_collection::contract::changelog::{ChangeType, ChangelogEntry};
use rocket_collection::contract::types::BreakingChangePolicy;
use rocket_collection::{Collection, Folder, Request};
use rocket_network::{ContractChange, ContractChangeType, ContractPolicyWire};
use rocket_shared::types::{Header, HttpMethod};

use super::contract_check_ports::{local_requests_of, policy_to_wire, wire_changes};

fn entry(field: &str, change_type: ChangeType) -> ChangelogEntry {
    ChangelogEntry {
        timestamp: Utc::now(),
        request_path: PathBuf::from("users/get.yml"),
        field: field.into(),
        change_type,
        old_value: Some("old".into()),
        new_value: Some("new".into()),
        is_breaking: true,
        request_method: Some("GET".into()),
        http_path: Some("/users/{id}".into()),
        author: Some("me".into()),
    }
}

#[test]
fn adapter_maps_entries_to_wire_changes() {
    let mut long = entry("header.X-Trace", ChangeType::Changed);
    long.old_value = Some("a".repeat(300));
    long.is_breaking = false;
    let mut no_method = entry("header.X-Trace", ChangeType::Added);
    no_method.request_method = None;
    let mut no_path = entry("header.X-Trace", ChangeType::Added);
    no_path.http_path = None;
    let mut secret = entry("header.Authorization", ChangeType::Changed);
    secret.new_value = Some("Bearer abc".into());

    let changes = wire_changes(&[long, no_method, no_path, secret]);

    assert_eq!(changes.len(), 2, "{changes:?}");
    let first = &changes[0];
    assert_eq!(first.method, "GET");
    assert_eq!(first.path_template, "/users/{id}");
    assert_eq!(first.field, "header:X-Trace");
    assert_eq!(first.change_type, ContractChangeType::Changed);
    assert_eq!(
        first.old_value.as_deref().map(|v| v.chars().count()),
        Some(256)
    );
    assert_eq!(first.new_value.as_deref(), Some("new"));
    assert!(!first.breaking);
    // Secrets are redacted later by the service, so the adapter keeps the value.
    assert_eq!(changes[1].field, "header:Authorization");
    assert_eq!(changes[1].new_value.as_deref(), Some("Bearer abc"));
}

#[test]
fn adapter_converts_field_names_to_the_wire_convention() {
    let cases = [
        ("header.Accept", "header:Accept"),
        ("query_param.page", "query:page"),
        ("form_field.email", "form:email"),
        ("auth_type", "auth"),
        ("auth_detail", "auth"),
        ("body", "body"),
    ];
    for (from, to) in cases {
        let changes = wire_changes(&[entry(from, ChangeType::Changed)]);
        let fields: Vec<&str> = changes.iter().map(|c| c.field.as_str()).collect();
        assert_eq!(fields, vec![to], "{from}");
    }
    let kinds = wire_changes(&[
        entry("header.A", ChangeType::Added),
        entry("header.B", ChangeType::Removed),
    ]);
    assert_eq!(kinds[0].change_type, ContractChangeType::Added);
    assert_eq!(kinds[1].change_type, ContractChangeType::Removed);
}

#[test]
fn adapter_skips_unmapped_and_empty_entries() {
    let mut empty_method = entry("header.A", ChangeType::Added);
    empty_method.request_method = Some(String::new());
    let mut empty_path = entry("header.A", ChangeType::Added);
    empty_path.http_path = Some("\u{7}".into());
    let mut long_path = entry("header.A", ChangeType::Added);
    long_path.http_path = Some(format!("/{}", "a".repeat(600)));
    let entries = [
        entry("method", ChangeType::Changed),
        entry("url_pattern", ChangeType::Changed),
        entry("request", ChangeType::Removed),
        entry("body_field.name", ChangeType::Removed),
        entry("header.", ChangeType::Added),
        entry("query_param.", ChangeType::Added),
        entry("", ChangeType::Added),
        empty_method,
        empty_path,
        long_path,
    ];

    let changes: Vec<ContractChange> = wire_changes(&entries);

    assert!(changes.is_empty(), "{changes:?}");
}

#[test]
fn adapter_strips_control_characters() {
    let mut e = entry("header.X-\u{0}Trace\n", ChangeType::Changed);
    e.request_method = Some("G\tET".into());
    e.http_path = Some("/users/\r{id}".into());
    e.old_value = Some("a\u{1b}b".into());
    e.new_value = Some("c\u{7f}d".into());

    let changes = wire_changes(&[e]);

    assert_eq!(changes.len(), 1);
    let c = &changes[0];
    assert_eq!(c.method, "GET");
    assert_eq!(c.path_template, "/users/{id}");
    assert_eq!(c.field, "header:X-Trace");
    assert_eq!(c.old_value.as_deref(), Some("ab"));
    assert_eq!(c.new_value.as_deref(), Some("cd"));
}

#[test]
fn policy_maps_to_the_wire_policy() {
    assert_eq!(
        policy_to_wire(&BreakingChangePolicy::Strict),
        ContractPolicyWire::Strict
    );
    assert_eq!(
        policy_to_wire(&BreakingChangePolicy::Lenient),
        ContractPolicyWire::Lenient
    );
    assert_eq!(
        policy_to_wire(&BreakingChangePolicy::AdditiveOk),
        ContractPolicyWire::AdditiveOk
    );
}

#[test]
fn local_requests_walk_every_http_request_of_a_collection() {
    let mut collection = Collection::new("orders");
    collection.root.dir_name = Some("orders".into());
    let mut list = Request::new("List orders", HttpMethod::Get, "{{base}}/orders");
    list.file_name = Some("list.yml".into());
    list.headers.push(Header::new("Accept", "application/json"));
    collection.root.add_request(list);
    let mut sub = Folder::new("admin");
    sub.dir_name = Some("admin".into());
    let mut delete = Request::new("Delete order", HttpMethod::Delete, "/orders/:id");
    delete.file_name = Some("delete.yml".into());
    sub.add_request(delete);
    collection.root.add_subfolder(sub);

    let locals = local_requests_of(&collection);

    assert_eq!(locals.len(), 2);
    assert_eq!(locals[0].collection, "orders");
    assert_eq!(locals[0].name, "List orders");
    assert_eq!(locals[0].signature.method, "GET");
    assert_eq!(locals[0].signature.headers.len(), 1);
    assert_eq!(locals[1].name, "Delete order");
    assert_eq!(
        locals[1].signature.request_path,
        PathBuf::from("admin/delete.yml")
    );
}
