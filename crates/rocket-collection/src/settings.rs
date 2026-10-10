use std::collections::HashMap;

use rocket_shared::types::{Auth, Header};
use serde::{Deserialize, Serialize};

use crate::folder_settings::ScriptFlow;

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

/// JS sandbox capability level for scripts in a collection. Defaults to `Safe`
/// (no filesystem/process access). A collection file only requests a mode. The
/// trust store decides what is granted on this computer (see `trust.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SandboxMode {
    #[default]
    Safe,
    Developer,
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

    /// JS sandbox capability level for scripts in this collection.
    #[serde(default)]
    pub sandbox_mode: SandboxMode,

    /// Requested value only. The trust store decides which roots are approved.
    /// Extra directories scripts may `require()` from, in Developer sandbox mode only.
    /// Relative entries resolve against the collection directory.
    /// Persisted at `extensions.rocketapi.scripts.additionalContextRoots`.
    #[serde(default)]
    pub script_context_roots: Vec<String>,

    /// Script run order for this collection. Absent means sandwich.
    /// Persisted at `extensions.bruno.scripts.flow` (Plan 03).
    #[serde(default)]
    pub script_flow: ScriptFlow,

    /// Whether the ACP AI-assist agent may run requests, edit scripts, and
    /// write non-secret env vars against this collection without further
    /// per-action confirmation. Defaults to `false`. This field is
    /// git-shared like `sandbox_mode`, so it is only a request: a collection
    /// cloned or pulled with it set to `true` gets no access until the user
    /// allows it on this computer (see `trust.rs`).
    #[serde(default)]
    pub agent_autonomy_enabled: bool,
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
        let result = merge_folder_chain_variables(vec![
            vec![var("k", "outer", true)],
            vec![var("k", "inner", true)],
        ]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].value, "inner");
    }

    #[test]
    fn merge_disabled_does_not_shadow() {
        let result = merge_folder_chain_variables(vec![
            vec![var("k", "outer", true)],
            vec![var("k", "inner_value", false)],
        ]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].value, "outer");
    }

    #[test]
    fn merge_disabled_outer_not_present() {
        let result = merge_folder_chain_variables(vec![vec![var("x", "val", false)]]);
        assert_eq!(result, vec![]);
    }

    #[test]
    fn merge_three_levels_inner_wins() {
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

    #[test]
    fn sandbox_mode_defaults_to_safe_when_absent_from_json() {
        let json = r#"{"headers":[],"variables":[]}"#;
        let settings: CollectionSettings = serde_json::from_str(json).expect("deserialize");
        assert_eq!(settings.sandbox_mode, SandboxMode::Safe);
    }

    #[test]
    fn sandbox_mode_developer_roundtrips_as_camel_case() {
        let settings = CollectionSettings {
            sandbox_mode: SandboxMode::Developer,
            ..Default::default()
        };
        let json = serde_json::to_string(&settings).expect("serialize");
        assert!(
            json.contains(r#""sandboxMode":"developer""#),
            "expected camelCase sandboxMode field, got {json}"
        );
        let round: CollectionSettings = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(round.sandbox_mode, SandboxMode::Developer);
    }

    #[test]
    fn script_flow_defaults_to_sandwich_when_absent_from_json() {
        let json = r#"{"headers":[],"variables":[]}"#;
        let settings: CollectionSettings = serde_json::from_str(json).expect("deserialize");
        assert_eq!(settings.script_flow, ScriptFlow::Sandwich);
    }

    #[test]
    fn script_flow_sequential_roundtrips_as_camel_case() {
        let settings = CollectionSettings {
            script_flow: ScriptFlow::Sequential,
            ..Default::default()
        };
        let json = serde_json::to_string(&settings).expect("serialize");
        assert!(
            json.contains(r#""scriptFlow":"sequential""#),
            "expected camelCase scriptFlow field, got {json}"
        );
        let round: CollectionSettings = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(round.script_flow, ScriptFlow::Sequential);
    }

    #[test]
    fn ipc_json_carries_script_flow_and_defaults_when_absent() {
        let settings = CollectionSettings {
            script_flow: ScriptFlow::Sequential,
            ..Default::default()
        };
        let json = serde_json::to_value(&settings).expect("serialize");
        assert_eq!(json["scriptFlow"], "sequential");

        let round: CollectionSettings = serde_json::from_value(json).expect("deserialize");
        assert_eq!(round.script_flow, ScriptFlow::Sequential);

        // A payload from a frontend that does not know the field yet.
        let old: CollectionSettings =
            serde_json::from_str(r#"{"headers":[],"variables":[],"sandboxMode":"safe"}"#)
                .expect("old payload deserializes");
        assert_eq!(old.script_flow, ScriptFlow::Sandwich);
    }

    #[test]
    fn agent_autonomy_enabled_defaults_to_false_when_absent_from_json() {
        let json = r#"{"headers":[],"variables":[]}"#;
        let settings: CollectionSettings = serde_json::from_str(json).expect("deserialize");
        assert!(!settings.agent_autonomy_enabled);
    }

    #[test]
    fn agent_autonomy_enabled_true_roundtrips_as_camel_case() {
        let settings = CollectionSettings {
            agent_autonomy_enabled: true,
            ..Default::default()
        };
        let json = serde_json::to_string(&settings).expect("serialize");
        assert!(
            json.contains(r#""agentAutonomyEnabled":true"#),
            "expected camelCase agentAutonomyEnabled field, got {json}"
        );
        let round: CollectionSettings = serde_json::from_str(&json).expect("deserialize");
        assert!(round.agent_autonomy_enabled);
    }
}
