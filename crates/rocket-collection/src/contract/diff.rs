use crate::contract::changelog::{ChangeType, ChangelogEntry};
use crate::contract::snapshot::RequestSignatureSnapshot;
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

    diff_key_list(&path, "query_param", &old.query_param_keys, &new.query_param_keys, now, &mut entries);
    diff_key_list(&path, "header", &old.header_keys, &new.header_keys, now, &mut entries);
    diff_key_list(&path, "body_field", &old.body_field_keys, &new.body_field_keys, now, &mut entries);

    entries
}

fn diff_key_list(
    path: &std::path::Path,
    prefix: &str,
    old_keys: &[String],
    new_keys: &[String],
    now: chrono::DateTime<chrono::Utc>,
    out: &mut Vec<ChangelogEntry>,
) {
    for key in old_keys {
        if !new_keys.contains(key) {
            out.push(ChangelogEntry {
                timestamp: now,
                request_path: path.to_path_buf(),
                field: format!("{}.{}", prefix, key),
                change_type: ChangeType::Removed,
                old_value: Some(key.clone()),
                new_value: None,
            });
        }
    }
    for key in new_keys {
        if !old_keys.contains(key) {
            out.push(ChangelogEntry {
                timestamp: now,
                request_path: path.to_path_buf(),
                field: format!("{}.{}", prefix, key),
                change_type: ChangeType::Added,
                old_value: None,
                new_value: Some(key.clone()),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn base_snap() -> RequestSignatureSnapshot {
        RequestSignatureSnapshot {
            request_path: PathBuf::from("requests/payment.yml"),
            method: "POST".into(),
            url_pattern: "/payments".into(),
            query_param_keys: vec!["currency".into()],
            header_keys: vec!["Authorization".into()],
            body_field_keys: vec!["amount".into(), "currency".into()],
            auth_type: "bearer".into(),
            captured_at: Utc::now(),
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
        let changes = diff_signature(&old, &new);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "body_field.amount");
        assert_eq!(changes[0].change_type, ChangeType::Removed);
    }

    #[test]
    fn added_query_param_detected() {
        let old = base_snap();
        let mut new = base_snap();
        new.query_param_keys.push("locale".into());
        let changes = diff_signature(&old, &new);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "query_param.locale");
        assert_eq!(changes[0].change_type, ChangeType::Added);
    }

    #[test]
    fn multiple_changes_all_detected() {
        let old = base_snap();
        let mut new = base_snap();
        new.method = "PUT".into();
        new.body_field_keys = vec!["total".into()]; // "amount"+"currency" removed, "total" added
        let changes = diff_signature(&old, &new);
        // method + amount removed + currency removed + total added = 4
        assert_eq!(changes.len(), 4);
    }
}
