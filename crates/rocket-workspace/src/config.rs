use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// Whether a collection is embedded (inside workspace dir) or external.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CollectionRefType {
    Embedded,
    External,
}

/// A reference to a collection within a workspace.
/// Embedded collections live inside `workspace/collections/`.
/// External collections are referenced by absolute path.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CollectionReference {
    pub name: String,
    #[serde(rename = "type")]
    pub ref_type: CollectionRefType,
    /// Only present for external collections.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
}

/// Configuration for workspace-level environments.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEnvironmentsConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_environment: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_ref_serializes_correctly() {
        let r = CollectionReference {
            name: "Users API".to_string(),
            ref_type: CollectionRefType::Embedded,
            path: None,
        };
        let yaml = serde_yaml::to_string(&r).unwrap();
        assert!(yaml.contains("type: embedded"));
        assert!(!yaml.contains("path:"));
    }

    #[test]
    fn external_ref_serializes_with_path() {
        let r = CollectionReference {
            name: "Shared Auth".to_string(),
            ref_type: CollectionRefType::External,
            path: Some(PathBuf::from("/home/user/shared-auth")),
        };
        let yaml = serde_yaml::to_string(&r).unwrap();
        assert!(yaml.contains("type: external"));
        assert!(yaml.contains("/home/user/shared-auth"));
    }

    #[test]
    fn collection_ref_serde_roundtrip() {
        let r = CollectionReference {
            name: "Test".to_string(),
            ref_type: CollectionRefType::External,
            path: Some(PathBuf::from("/tmp/ext")),
        };
        let yaml = serde_yaml::to_string(&r).unwrap();
        let back: CollectionReference = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn environments_config_defaults_to_none() {
        let cfg = WorkspaceEnvironmentsConfig::default();
        assert_eq!(cfg.active_environment, None);
    }

    #[test]
    fn environments_config_serde_roundtrip() {
        let cfg = WorkspaceEnvironmentsConfig {
            active_environment: Some("staging".to_string()),
        };
        let yaml = serde_yaml::to_string(&cfg).unwrap();
        let back: WorkspaceEnvironmentsConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(cfg, back);
    }
}
