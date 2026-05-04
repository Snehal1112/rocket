use rocket_shared::types::{Auth, Header};
use serde::{Deserialize, Serialize};

/// A collection-scoped variable (like Postman/Bruno collection variables).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CollectionVariable {
    pub key: String,
    pub value: String,
    /// Initial/default value committed to Git; fallback when value is empty.
    #[serde(default)]
    pub initial_value: String,
    pub enabled: bool,
    /// Mark as secret to hide in the UI (like Bruno).
    #[serde(default)]
    pub secret: bool,
}

/// Per-collection default auth, headers, and variables, stored in opencollection.yml.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSettings {
    /// Markdown documentation for this collection (maps to `docs:` in opencollection.yml).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,

    /// Optional auth applied to all requests in this collection.
    #[serde(default)]
    pub auth: Option<Auth>,

    /// Default headers prepended to every request in this collection.
    #[serde(default)]
    pub headers: Vec<Header>,

    /// Collection-scoped variables, resolved alongside environment variables.
    #[serde(default)]
    pub variables: Vec<CollectionVariable>,
}

/// Merge a folder ancestor chain into a single deduplicated, sorted variable set.
///
/// `chain` is ordered outermost-first. For each folder, only enabled variables
/// participate; disabled entries are skipped entirely and do not shadow
/// enabled variables from outer folders. On key collision, the innermost
/// (later) folder wins. The returned vector is sorted by `key`.
pub fn merge_folder_chain_variables(
    chain: Vec<Vec<CollectionVariable>>,
) -> Vec<CollectionVariable> {
    use std::collections::HashMap;
    let mut merged: HashMap<String, CollectionVariable> = HashMap::new();
    for folder_vars in chain {
        for v in folder_vars {
            if v.enabled {
                merged.insert(v.key.clone(), v);
            }
        }
    }
    let mut out: Vec<CollectionVariable> = merged.into_values().collect();
    out.sort_by(|a, b| a.key.cmp(&b.key));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(key: &str, value: &str, enabled: bool) -> CollectionVariable {
        CollectionVariable {
            key: key.to_string(),
            value: value.to_string(),
            initial_value: String::new(),
            enabled,
            secret: false,
        }
    }

    #[test]
    fn merge_empty_chain_returns_empty() {
        assert_eq!(merge_folder_chain_variables(vec![]), vec![]);
    }

    #[test]
    fn merge_single_folder_returns_sorted_enabled() {
        // Two enabled vars in reverse alpha order; result must be sorted ascending.
        let result = merge_folder_chain_variables(vec![vec![
            var("z_key", "z_val", true),
            var("a_key", "a_val", true),
        ]]);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].key, "a_key");
        assert_eq!(result[1].key, "z_key");
    }

    #[test]
    fn merge_inner_wins_on_collision() {
        // Same key "k" in both folders; inner value must win.
        let result = merge_folder_chain_variables(vec![
            vec![var("k", "outer", true)],
            vec![var("k", "inner", true)],
        ]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].value, "inner");
    }

    #[test]
    fn merge_disabled_does_not_shadow() {
        // Outer has k=outer (enabled), inner has same key disabled. Outer value survives.
        let result = merge_folder_chain_variables(vec![
            vec![var("k", "outer", true)],
            vec![var("k", "inner_value", false)],
        ]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].value, "outer");
    }

    #[test]
    fn merge_disabled_outer_not_present() {
        // Single disabled var; nothing should appear in the result.
        let result = merge_folder_chain_variables(vec![vec![var("x", "val", false)]]);
        assert_eq!(result, vec![]);
    }

    #[test]
    fn merge_three_levels_inner_wins() {
        // Level 1: a=1, b=1. Level 2: a=2. Level 3: b=3. Expect a=2, b=3.
        let result = merge_folder_chain_variables(vec![
            vec![var("a", "1", true), var("b", "1", true)],
            vec![var("a", "2", true)],
            vec![var("b", "3", true)],
        ]);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].key, "a");
        assert_eq!(result[0].value, "2");
        assert_eq!(result[1].key, "b");
        assert_eq!(result[1].value, "3");
    }
}
