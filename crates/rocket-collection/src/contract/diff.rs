use crate::contract::changelog::{ChangeType, ChangelogEntry};
use crate::contract::snapshot::{KeyValueEntry, RequestSignatureSnapshot};
use crate::contract::types::BreakingChangePolicy;
use chrono::Utc;

/// Pure function — no I/O, no side effects.
/// Returns one `ChangelogEntry` per detected change, each tagged with `is_breaking`
/// according to the supplied `BreakingChangePolicy`.
///
/// `author` is the OS username of the person who triggered the save; pass `None`
/// when the caller cannot determine the user (e.g. in recompute_drift).
pub fn diff_signature(
    old: &RequestSignatureSnapshot,
    new: &RequestSignatureSnapshot,
    policy: &BreakingChangePolicy,
    author: Option<String>,
) -> Vec<ChangelogEntry> {
    let mut entries = Vec::new();
    let now = Utc::now();
    let path = new.request_path.clone();

    // Method change — always breaking
    if old.method != new.method {
        entries.push(ChangelogEntry {
            timestamp: now,
            request_path: path.clone(),
            field: "method".into(),
            change_type: ChangeType::Changed,
            old_value: Some(old.method.clone()),
            new_value: Some(new.method.clone()),
            is_breaking: true,
            request_method: Some(new.method.clone()),
            http_path: Some(new.url_pattern.clone()),
            author: author.clone(),
        });
    }

    // URL pattern change — always breaking
    if old.url_pattern != new.url_pattern {
        entries.push(ChangelogEntry {
            timestamp: now,
            request_path: path.clone(),
            field: "url_pattern".into(),
            change_type: ChangeType::Changed,
            old_value: Some(old.url_pattern.clone()),
            new_value: Some(new.url_pattern.clone()),
            is_breaking: true,
            request_method: Some(new.method.clone()),
            http_path: Some(new.url_pattern.clone()),
            author: author.clone(),
        });
    }

    // Auth type change — always breaking
    if old.auth_type != new.auth_type {
        entries.push(ChangelogEntry {
            timestamp: now,
            request_path: path.clone(),
            field: "auth_type".into(),
            change_type: ChangeType::Changed,
            old_value: Some(old.auth_type.clone()),
            new_value: Some(new.auth_type.clone()),
            is_breaking: true,
            request_method: Some(new.method.clone()),
            http_path: Some(new.url_pattern.clone()),
            author: author.clone(),
        });
    }

    // Auth detail change — always breaking (credential change)
    if old.auth_detail != new.auth_detail {
        entries.push(ChangelogEntry {
            timestamp: now,
            request_path: path.clone(),
            field: "auth_detail".into(),
            change_type: ChangeType::Changed,
            old_value: if old.auth_detail.is_empty() { None } else { Some(old.auth_detail.clone()) },
            new_value: if new.auth_detail.is_empty() { None } else { Some(new.auth_detail.clone()) },
            is_breaking: true,
            request_method: Some(new.method.clone()),
            http_path: Some(new.url_pattern.clone()),
            author: author.clone(),
        });
    }

    // Key-value list diffs (new snapshot format)
    diff_kv_list(&path, "header", &old.headers, &new.headers, policy, now, &new.method, &new.url_pattern, author.as_deref(), &mut entries);
    diff_kv_list(&path, "query_param", &old.query_params, &new.query_params, policy, now, &new.method, &new.url_pattern, author.as_deref(), &mut entries);
    diff_kv_list(&path, "form_field", &old.form_fields, &new.form_fields, policy, now, &new.method, &new.url_pattern, author.as_deref(), &mut entries);

    // Legacy key-only list diffs (backward compat for old snapshots)
    diff_key_only_list(&path, "query_param", &old.query_param_keys, &new.query_param_keys, policy, now, &new.method, &new.url_pattern, author.as_deref(), &mut entries);
    diff_key_only_list(&path, "header", &old.header_keys, &new.header_keys, policy, now, &new.method, &new.url_pattern, author.as_deref(), &mut entries);
    diff_key_only_list(&path, "body_field", &old.body_field_keys, &new.body_field_keys, policy, now, &new.method, &new.url_pattern, author.as_deref(), &mut entries);

    // Body content diff
    match (&old.body_content, &new.body_content) {
        (Some(o), Some(n)) if o != n => {
            entries.push(ChangelogEntry {
                timestamp: now,
                request_path: path.clone(),
                field: "body".into(),
                change_type: ChangeType::Changed,
                old_value: Some(o.clone()),
                new_value: Some(n.clone()),
                is_breaking: true,
                request_method: Some(new.method.clone()),
                http_path: Some(new.url_pattern.clone()),
                author: author.clone(),
            });
        }
        (Some(o), None) => {
            entries.push(ChangelogEntry {
                timestamp: now,
                request_path: path.clone(),
                field: "body".into(),
                change_type: ChangeType::Removed,
                old_value: Some(o.clone()),
                new_value: None,
                is_breaking: true,
                request_method: Some(new.method.clone()),
                http_path: Some(new.url_pattern.clone()),
                author: author.clone(),
            });
        }
        (None, Some(n)) => {
            entries.push(ChangelogEntry {
                timestamp: now,
                request_path: path.clone(),
                field: "body".into(),
                change_type: ChangeType::Added,
                old_value: None,
                new_value: Some(n.clone()),
                is_breaking: matches!(policy, BreakingChangePolicy::Strict),
                request_method: Some(new.method.clone()),
                http_path: Some(new.url_pattern.clone()),
                author: author.clone(),
            });
        }
        _ => {}
    }

    entries
}

fn diff_kv_list(
    path: &std::path::Path,
    prefix: &str,
    old_kvs: &[KeyValueEntry],
    new_kvs: &[KeyValueEntry],
    policy: &BreakingChangePolicy,
    now: chrono::DateTime<Utc>,
    method: &str,
    http_path: &str,
    author: Option<&str>,
    out: &mut Vec<ChangelogEntry>,
) {
    let path_buf = path.to_path_buf();

    for old_entry in old_kvs {
        match new_kvs.iter().find(|e| e.key == old_entry.key) {
            None => {
                let is_breaking = match (prefix, policy) {
                    ("header", BreakingChangePolicy::AdditiveOk) => false,
                    _ => true,
                };
                out.push(ChangelogEntry {
                    timestamp: now,
                    request_path: path_buf.clone(),
                    field: format!("{prefix}.{}", old_entry.key),
                    change_type: ChangeType::Removed,
                    old_value: Some(old_entry.value.clone()),
                    new_value: None,
                    is_breaking,
                    request_method: Some(method.to_owned()),
                    http_path: Some(http_path.to_owned()),
                    author: author.map(ToOwned::to_owned),
                });
            }
            Some(new_entry) if new_entry.value != old_entry.value => {
                out.push(ChangelogEntry {
                    timestamp: now,
                    request_path: path_buf.clone(),
                    field: format!("{prefix}.{}", old_entry.key),
                    change_type: ChangeType::Changed,
                    old_value: Some(old_entry.value.clone()),
                    new_value: Some(new_entry.value.clone()),
                    is_breaking: true,
                    request_method: Some(method.to_owned()),
                    http_path: Some(http_path.to_owned()),
                    author: author.map(ToOwned::to_owned),
                });
            }
            _ => {}
        }
    }

    for new_entry in new_kvs {
        if !old_kvs.iter().any(|e| e.key == new_entry.key) {
            out.push(ChangelogEntry {
                timestamp: now,
                request_path: path_buf.clone(),
                field: format!("{prefix}.{}", new_entry.key),
                change_type: ChangeType::Added,
                old_value: None,
                new_value: Some(new_entry.value.clone()),
                is_breaking: matches!(policy, BreakingChangePolicy::Strict),
                request_method: Some(method.to_owned()),
                http_path: Some(http_path.to_owned()),
                author: author.map(ToOwned::to_owned),
            });
        }
    }
}

fn diff_key_only_list(
    path: &std::path::Path,
    prefix: &str,
    old_keys: &[String],
    new_keys: &[String],
    policy: &BreakingChangePolicy,
    now: chrono::DateTime<Utc>,
    method: &str,
    http_path: &str,
    author: Option<&str>,
    out: &mut Vec<ChangelogEntry>,
) {
    let path_buf = path.to_path_buf();

    for key in old_keys {
        if !new_keys.contains(key) {
            let is_breaking = match (prefix, policy) {
                ("header", BreakingChangePolicy::AdditiveOk) => false,
                _ => true,
            };
            out.push(ChangelogEntry {
                timestamp: now,
                request_path: path_buf.clone(),
                field: format!("{prefix}.{key}"),
                change_type: ChangeType::Removed,
                old_value: Some(key.clone()),
                new_value: None,
                is_breaking,
                request_method: Some(method.to_owned()),
                http_path: Some(http_path.to_owned()),
                author: author.map(ToOwned::to_owned),
            });
        }
    }

    for key in new_keys {
        if !old_keys.contains(key) {
            out.push(ChangelogEntry {
                timestamp: now,
                request_path: path_buf.clone(),
                field: format!("{prefix}.{key}"),
                change_type: ChangeType::Added,
                old_value: None,
                new_value: Some(key.clone()),
                is_breaking: matches!(policy, BreakingChangePolicy::Strict),
                request_method: Some(method.to_owned()),
                http_path: Some(http_path.to_owned()),
                author: author.map(ToOwned::to_owned),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::snapshot::KeyValueEntry;
    use crate::contract::types::BreakingChangePolicy;
    use std::path::PathBuf;

    fn make_kv(key: &str, value: &str) -> KeyValueEntry {
        KeyValueEntry { key: key.into(), value: value.into() }
    }

    fn base_snap() -> RequestSignatureSnapshot {
        RequestSignatureSnapshot {
            request_path: PathBuf::from("requests/payment.yml"),
            method: "POST".into(),
            url_pattern: "/payments".into(),
            headers: vec![],
            query_params: vec![],
            body_content: None,
            form_fields: vec![],
            auth_type: "bearer".into(),
            auth_detail: String::new(),
            captured_at: Utc::now(),
            // Legacy key-list fields for old YAML compat.
            query_param_keys: vec!["currency".into()],
            header_keys: vec!["Authorization".into()],
            body_field_keys: vec!["amount".into(), "currency".into()],
        }
    }

    fn base_snap_v2() -> RequestSignatureSnapshot {
        RequestSignatureSnapshot {
            request_path: PathBuf::from("requests/payment.yml"),
            method: "POST".into(),
            url_pattern: "/payments".into(),
            headers: vec![make_kv("Authorization", "Bearer old"), make_kv("Content-Type", "application/json")],
            query_params: vec![make_kv("currency", "USD")],
            body_content: Some(r#"{"amount":100}"#.into()),
            form_fields: vec![],
            auth_type: "bearer".into(),
            auth_detail: "oldtoken…".into(),
            captured_at: Utc::now(),
            query_param_keys: vec![],
            header_keys: vec![],
            body_field_keys: vec![],
        }
    }

    fn lenient() -> BreakingChangePolicy { BreakingChangePolicy::Lenient }

    #[test]
    fn no_changes_returns_empty() {
        let snap = base_snap();
        assert!(diff_signature(&snap, &snap, &lenient(), None).is_empty());
    }

    #[test]
    fn method_change_detected() {
        let old = base_snap();
        let mut new = base_snap();
        new.method = "PUT".into();
        let changes = diff_signature(&old, &new, &lenient(), None);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "method");
        assert_eq!(changes[0].change_type, ChangeType::Changed);
        assert!(changes[0].is_breaking);
    }

    #[test]
    fn removed_body_field_detected() {
        let old = base_snap();
        let mut new = base_snap();
        new.body_field_keys = vec!["currency".into()]; // "amount" removed
        let changes = diff_signature(&old, &new, &lenient(), None);
        // body_field removal is a breaking change under Lenient
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "body_field.amount");
        assert_eq!(changes[0].change_type, ChangeType::Removed);
        assert!(changes[0].is_breaking);
    }

    #[test]
    fn added_query_param_detected() {
        let old = base_snap();
        let mut new = base_snap();
        new.query_param_keys.push("locale".into());
        let changes = diff_signature(&old, &new, &lenient(), None);
        // Added param — not breaking under Lenient
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "query_param.locale");
        assert_eq!(changes[0].change_type, ChangeType::Added);
        assert!(!changes[0].is_breaking);
    }

    #[test]
    fn multiple_changes_all_detected() {
        let old = base_snap();
        let mut new = base_snap();
        new.method = "PUT".into();
        new.body_field_keys = vec!["total".into()];
        // Method + "amount" removed + "currency" removed + "total" added = 4 changes
        let changes = diff_signature(&old, &new, &lenient(), None);
        assert_eq!(changes.len(), 4);
        assert!(changes.iter().any(|e| e.field == "method"));
    }

    // --- v2 tests: key-value list, body content, auth_detail ---

    #[test]
    fn header_value_change_detected() {
        let old = base_snap_v2();
        let mut new = base_snap_v2();
        new.headers[0].value = "Bearer new".into();
        let changes = diff_signature(&old, &new, &lenient(), None);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "header.Authorization");
        assert_eq!(changes[0].change_type, ChangeType::Changed);
        assert_eq!(changes[0].old_value.as_deref(), Some("Bearer old"));
        assert_eq!(changes[0].new_value.as_deref(), Some("Bearer new"));
    }

    #[test]
    fn header_removed_detected() {
        let old = base_snap_v2();
        let mut new = base_snap_v2();
        new.headers.retain(|h| h.key != "Content-Type");
        let changes = diff_signature(&old, &new, &lenient(), None);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "header.Content-Type");
        assert_eq!(changes[0].change_type, ChangeType::Removed);
    }

    #[test]
    fn header_added_detected() {
        let old = base_snap_v2();
        let mut new = base_snap_v2();
        new.headers.push(make_kv("X-New", "yes"));
        let changes = diff_signature(&old, &new, &lenient(), None);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "header.X-New");
        assert_eq!(changes[0].change_type, ChangeType::Added);
        assert_eq!(changes[0].new_value.as_deref(), Some("yes"));
    }

    #[test]
    fn query_param_value_change_detected() {
        let old = base_snap_v2();
        let mut new = base_snap_v2();
        new.query_params[0].value = "EUR".into();
        let changes = diff_signature(&old, &new, &lenient(), None);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "query_param.currency");
        assert_eq!(changes[0].change_type, ChangeType::Changed);
        assert_eq!(changes[0].old_value.as_deref(), Some("USD"));
        assert_eq!(changes[0].new_value.as_deref(), Some("EUR"));
    }

    #[test]
    fn body_content_change_detected() {
        let old = base_snap_v2();
        let mut new = base_snap_v2();
        new.body_content = Some(r#"{"amount":200}"#.into());
        let changes = diff_signature(&old, &new, &lenient(), None);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "body");
        assert_eq!(changes[0].change_type, ChangeType::Changed);
        assert_eq!(changes[0].old_value.as_deref(), Some(r#"{"amount":100}"#));
        assert_eq!(changes[0].new_value.as_deref(), Some(r#"{"amount":200}"#));
    }

    #[test]
    fn body_content_removed_detected() {
        let old = base_snap_v2();
        let mut new = base_snap_v2();
        new.body_content = None;
        let changes = diff_signature(&old, &new, &lenient(), None);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "body");
        assert_eq!(changes[0].change_type, ChangeType::Removed);
        assert!(changes[0].new_value.is_none());
    }

    #[test]
    fn body_content_added_detected() {
        let mut old = base_snap_v2();
        let mut new = base_snap_v2();
        old.body_content = None;
        new.body_content = Some(r#"{"amount":100}"#.into());
        let changes = diff_signature(&old, &new, &lenient(), None);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "body");
        assert_eq!(changes[0].change_type, ChangeType::Added);
        assert_eq!(changes[0].old_value, None);
        assert_eq!(changes[0].new_value.as_deref(), Some(r#"{"amount":100}"#));
    }

    #[test]
    fn form_field_value_change_detected() {
        let mut old = base_snap_v2();
        old.form_fields = vec![make_kv("name", "Ada")];
        let mut new = base_snap_v2();
        new.form_fields = vec![make_kv("name", "Bob")];
        let changes = diff_signature(&old, &new, &lenient(), None);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "form_field.name");
        assert_eq!(changes[0].change_type, ChangeType::Changed);
    }

    #[test]
    fn auth_detail_change_detected() {
        let old = base_snap_v2();
        let mut new = base_snap_v2();
        new.auth_detail = "newtoken…".into();
        let changes = diff_signature(&old, &new, &lenient(), None);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "auth_detail");
        assert_eq!(changes[0].change_type, ChangeType::Changed);
    }

    #[test]
    fn both_auth_detail_empty_no_entry() {
        let mut old = base_snap_v2();
        let mut new = base_snap_v2();
        old.auth_detail = String::new();
        new.auth_detail = String::new();
        assert!(diff_signature(&old, &new, &lenient(), None).is_empty());
    }

    #[test]
    fn no_changes_v2_returns_empty() {
        let snap = base_snap_v2();
        assert!(diff_signature(&snap, &snap, &lenient(), None).is_empty());
    }

    #[test]
    fn auth_type_and_detail_both_emitted_when_auth_scheme_changes() {
        let old = base_snap_v2(); // auth_type: "bearer", auth_detail: "oldtoken…"
        let mut new = base_snap_v2();
        new.auth_type = "none".into();
        new.auth_detail = String::new();

        let changes = diff_signature(&old, &new, &lenient(), None);

        assert_eq!(changes.len(), 2);
        assert!(changes.iter().any(|e| e.field == "auth_type" && e.change_type == ChangeType::Changed));
        assert!(changes.iter().any(|e| e.field == "auth_detail" && e.change_type == ChangeType::Changed));
    }

    fn make_policy_snap(method: &str, params: Vec<&str>) -> RequestSignatureSnapshot {
        RequestSignatureSnapshot {
            request_path: std::path::PathBuf::from("req.yml"),
            method: method.into(),
            url_pattern: "/test".into(),
            headers: vec![],
            query_params: params.iter().map(|k| KeyValueEntry { key: k.to_string(), value: String::new() }).collect(),
            body_content: None,
            form_fields: vec![],
            auth_type: "none".into(),
            auth_detail: String::new(),
            captured_at: chrono::Utc::now(),
            query_param_keys: vec![],
            header_keys: vec![],
            body_field_keys: vec![],
        }
    }

    #[test]
    fn method_change_always_breaking_all_policies() {
        let old = make_policy_snap("GET", vec![]);
        let new = make_policy_snap("POST", vec![]);
        for policy in [BreakingChangePolicy::Strict, BreakingChangePolicy::Lenient, BreakingChangePolicy::AdditiveOk] {
            let entries = diff_signature(&old, &new, &policy, None);
            assert!(entries.iter().any(|e| e.field == "method" && e.is_breaking), "method change must be breaking for {:?}", policy);
        }
    }

    #[test]
    fn new_param_strict_is_breaking_lenient_is_not() {
        let old = make_policy_snap("GET", vec![]);
        let new = make_policy_snap("GET", vec!["page"]);

        let strict = diff_signature(&old, &new, &BreakingChangePolicy::Strict, None);
        assert!(strict.iter().any(|e| e.is_breaking));

        let lenient = diff_signature(&old, &new, &BreakingChangePolicy::Lenient, None);
        assert!(lenient.iter().all(|e| !e.is_breaking));
    }

    #[test]
    fn required_param_removed_always_breaking() {
        let mut old = make_policy_snap("GET", vec![]);
        old.query_param_keys = vec!["required_param".into()];
        let new = make_policy_snap("GET", vec![]);

        for policy in [BreakingChangePolicy::Strict, BreakingChangePolicy::Lenient, BreakingChangePolicy::AdditiveOk] {
            let entries = diff_signature(&old, &new, &policy, None);
            assert!(!entries.is_empty(), "removed param must be detected for {:?}", policy);
        }
    }

    #[test]
    fn additive_ok_new_param_not_breaking() {
        let old = make_policy_snap("GET", vec![]);
        let new = make_policy_snap("GET", vec!["page"]);
        let entries = diff_signature(&old, &new, &BreakingChangePolicy::AdditiveOk, None);
        for e in &entries {
            assert!(!e.is_breaking, "additive change must not be breaking under AdditiveOk");
        }
    }
}
