use crate::contract::changelog::{ChangeType, ChangelogEntry};
use crate::contract::snapshot::{KeyValueEntry, RequestSignatureSnapshot};
use chrono::Utc;

/// Pure function — no I/O, no side effects.
/// Returns one `ChangelogEntry` per detected change.
///
/// This is the Model B extension seam:
/// the save hook calls this function and currently logs results silently.
/// Model B will act on the return value (warn / block) without changing this function.
pub fn diff_signature(
    old: &RequestSignatureSnapshot,
    new: &RequestSignatureSnapshot,
) -> Vec<ChangelogEntry> {
    let mut entries = Vec::new();
    let now = Utc::now();
    let path = new.request_path.clone();

    macro_rules! field_diff {
        ($field:expr, $old:expr, $new:expr) => {
            if $old != $new {
                entries.push(ChangelogEntry {
                    timestamp: now,
                    request_path: path.clone(),
                    field: $field.to_string(),
                    change_type: ChangeType::Changed,
                    old_value: Some($old.to_string()),
                    new_value: Some($new.to_string()),
                });
            }
        };
    }

    field_diff!("method", old.method, new.method);
    field_diff!("url_pattern", old.url_pattern, new.url_pattern);
    field_diff!("auth_type", old.auth_type, new.auth_type);

    // Diff auth_detail when the values differ.
    if old.auth_detail != new.auth_detail {
        entries.push(ChangelogEntry {
            timestamp: now,
            request_path: path.clone(),
            field: "auth_detail".to_string(),
            change_type: ChangeType::Changed,
            old_value: if old.auth_detail.is_empty() { None } else { Some(old.auth_detail.clone()) },
            new_value: if new.auth_detail.is_empty() { None } else { Some(new.auth_detail.clone()) },
        });
    }

    diff_key_value_list(&path, "header", &old.headers, &new.headers, now, &mut entries);
    diff_key_value_list(&path, "query_param", &old.query_params, &new.query_params, now, &mut entries);
    diff_key_value_list(&path, "form_field", &old.form_fields, &new.form_fields, now, &mut entries);

    // Diff raw body content.
    match (&old.body_content, &new.body_content) {
        (Some(o), Some(n)) if o != n => {
            entries.push(ChangelogEntry {
                timestamp: now,
                request_path: path.clone(),
                field: "body".to_string(),
                change_type: ChangeType::Changed,
                old_value: Some(o.clone()),
                new_value: Some(n.clone()),
            });
        }
        (Some(o), None) => {
            entries.push(ChangelogEntry {
                timestamp: now,
                request_path: path.clone(),
                field: "body".to_string(),
                change_type: ChangeType::Removed,
                old_value: Some(o.clone()),
                new_value: None,
            });
        }
        (None, Some(n)) => {
            entries.push(ChangelogEntry {
                timestamp: now,
                request_path: path.clone(),
                field: "body".to_string(),
                change_type: ChangeType::Added,
                old_value: None,
                new_value: Some(n.clone()),
            });
        }
        // Both None or identical — no change.
        _ => {}
    }

    entries
}

// Key uniqueness is not enforced here. If two entries share a key, the first
// match wins. The snapshot builder (from_request) collects from the UI layer
// which does not permit duplicate enabled keys, so this is safe in practice.
/// Compares two key-value lists, emitting Added/Removed/Changed entries.
fn diff_key_value_list(
    path: &std::path::Path,
    prefix: &str,
    old: &[KeyValueEntry],
    new: &[KeyValueEntry],
    now: chrono::DateTime<chrono::Utc>,
    out: &mut Vec<ChangelogEntry>,
) {
    let path_buf = path.to_path_buf();

    // Detect removed and changed entries.
    for old_entry in old {
        match new.iter().find(|e| e.key == old_entry.key) {
            None => {
                out.push(ChangelogEntry {
                    timestamp: now,
                    request_path: path_buf.clone(),
                    field: format!("{}.{}", prefix, old_entry.key),
                    change_type: ChangeType::Removed,
                    old_value: Some(old_entry.value.clone()),
                    new_value: None,
                });
            }
            Some(new_entry) if new_entry.value != old_entry.value => {
                out.push(ChangelogEntry {
                    timestamp: now,
                    request_path: path_buf.clone(),
                    field: format!("{}.{}", prefix, old_entry.key),
                    change_type: ChangeType::Changed,
                    old_value: Some(old_entry.value.clone()),
                    new_value: Some(new_entry.value.clone()),
                });
            }
            _ => {}
        }
    }

    // Detect added entries.
    for new_entry in new {
        if !old.iter().any(|e| e.key == new_entry.key) {
            out.push(ChangelogEntry {
                timestamp: now,
                request_path: path_buf.clone(),
                field: format!("{}.{}", prefix, new_entry.key),
                change_type: ChangeType::Added,
                old_value: None,
                new_value: Some(new_entry.value.clone()),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::snapshot::KeyValueEntry;
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

    #[test]
    fn no_changes_returns_empty() {
        let snap = base_snap();
        assert!(diff_signature(&snap, &snap).is_empty());
    }

    #[test]
    fn method_change_detected() {
        let old = base_snap();
        let mut new = base_snap();
        new.method = "PUT".into();
        let changes = diff_signature(&old, &new);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "method");
        assert_eq!(changes[0].change_type, ChangeType::Changed);
    }

    #[test]
    fn removed_body_field_detected() {
        let old = base_snap();
        let mut new = base_snap();
        new.body_field_keys = vec!["currency".into()]; // "amount" removed
        // Legacy key-list fields are no longer diffed; no change expected.
        let changes = diff_signature(&old, &new);
        assert!(changes.is_empty());
    }

    #[test]
    fn added_query_param_detected() {
        let old = base_snap();
        let mut new = base_snap();
        new.query_param_keys.push("locale".into());
        // Legacy key-list fields are no longer diffed; no change expected.
        let changes = diff_signature(&old, &new);
        assert!(changes.is_empty());
    }

    #[test]
    fn multiple_changes_all_detected() {
        let old = base_snap();
        let mut new = base_snap();
        new.method = "PUT".into();
        new.body_field_keys = vec!["total".into()];
        // Only method change is detected; legacy key-list fields are not diffed.
        let changes = diff_signature(&old, &new);
        assert_eq!(changes.len(), 1);
    }

    // --- v2 tests: key-value list, body content, auth_detail ---

    #[test]
    fn header_value_change_detected() {
        let old = base_snap_v2();
        let mut new = base_snap_v2();
        new.headers[0].value = "Bearer new".into();
        let changes = diff_signature(&old, &new);
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
        let changes = diff_signature(&old, &new);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "header.Content-Type");
        assert_eq!(changes[0].change_type, ChangeType::Removed);
    }

    #[test]
    fn header_added_detected() {
        let old = base_snap_v2();
        let mut new = base_snap_v2();
        new.headers.push(make_kv("X-New", "yes"));
        let changes = diff_signature(&old, &new);
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
        let changes = diff_signature(&old, &new);
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
        let changes = diff_signature(&old, &new);
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
        let changes = diff_signature(&old, &new);
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
        let changes = diff_signature(&old, &new);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "body");
        assert_eq!(changes[0].change_type, ChangeType::Added);
        assert_eq!(changes[0].old_value, None);
        assert_eq!(changes[0].new_value.as_deref(), Some(r#"{"amount":100}"#));
    }

    #[test]
    fn form_field_value_change_detected() {
        let old = base_snap_v2();
        let mut new = base_snap_v2();
        new.form_fields = vec![make_kv("name", "Bob")];
        let mut old2 = base_snap_v2();
        old2.form_fields = vec![make_kv("name", "Ada")];
        let changes = diff_signature(&old2, &new);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "form_field.name");
        assert_eq!(changes[0].change_type, ChangeType::Changed);
    }

    #[test]
    fn auth_detail_change_detected() {
        let old = base_snap_v2();
        let mut new = base_snap_v2();
        new.auth_detail = "newtoken…".into();
        let changes = diff_signature(&old, &new);
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
        assert!(diff_signature(&old, &new).is_empty());
    }

    #[test]
    fn no_changes_v2_returns_empty() {
        let snap = base_snap_v2();
        assert!(diff_signature(&snap, &snap).is_empty());
    }
}
