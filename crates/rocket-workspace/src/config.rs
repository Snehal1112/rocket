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

/// Opt-in security policy for BeforeRequest script URL mutations. When both
/// flags are false (the default), behavior is unchanged from today: a script
/// may redirect a request to any host, exactly like a user typing the URL
/// manually — see docs/superpowers/specs/2026-09-16-request-mutation-host-guard-spec.md.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct RequestGuardPolicy {
    /// When true, a BeforeRequest script's `req.setUrl` mutation is checked
    /// against the blocked ranges below before the request is dispatched.
    #[serde(default)]
    pub block_script_redirects_to_internal_hosts: bool,
    /// Additionally block RFC1918 private ranges (10/8, 172.16/12, 192.168/16),
    /// not just loopback/link-local. Off by default even when the guard itself
    /// is enabled, since many legitimate internal APIs live on private ranges.
    #[serde(default)]
    pub also_block_private_ranges: bool,
}

/// Represents the per-workspace `workspace.yml` that lives inside
/// each workspace directory. This file makes the workspace portable
/// and Git-friendly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceConfig {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collections: Vec<CollectionReference>,
    #[serde(default)]
    pub environments: WorkspaceEnvironmentsConfig,
    /// Name of the selected global environment (workspace/environments/<n>.yml).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global_environment: Option<String>,
    /// Opt-in security policy for BeforeRequest script URL mutations. Defaults
    /// to fully permissive (today's behavior) for every existing workspace.
    #[serde(default)]
    pub request_guard_policy: RequestGuardPolicy,
}

impl WorkspaceConfig {
    /// Create a new workspace config with just a name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            collections: Vec::new(),
            environments: WorkspaceEnvironmentsConfig::default(),
            global_environment: None,
            request_guard_policy: RequestGuardPolicy::default(),
        }
    }

    /// Add an embedded collection reference.
    pub fn add_embedded_collection(&mut self, name: impl Into<String>) {
        self.collections.push(CollectionReference {
            name: name.into(),
            ref_type: CollectionRefType::Embedded,
            path: None,
        });
    }

    /// Add an external collection reference.
    pub fn add_external_collection(&mut self, name: impl Into<String>, path: PathBuf) {
        self.collections.push(CollectionReference {
            name: name.into(),
            ref_type: CollectionRefType::External,
            path: Some(path),
        });
    }

    /// Remove a collection reference by name.
    pub fn remove_collection(&mut self, name: &str) {
        self.collections.retain(|c| c.name != name);
    }

    /// Check if a collection name already exists in this config.
    pub fn has_collection(&self, name: &str) -> bool {
        self.collections.iter().any(|c| c.name == name)
    }
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

    #[test]
    fn workspace_config_new_is_empty() {
        let cfg = WorkspaceConfig::new("Test");
        assert_eq!(cfg.name, "Test");
        assert!(cfg.collections.is_empty());
        assert_eq!(cfg.description, None);
        assert_eq!(cfg.environments.active_environment, None);
    }

    #[test]
    fn workspace_config_add_embedded_collection() {
        let mut cfg = WorkspaceConfig::new("Test");
        cfg.add_embedded_collection("Users API");
        assert_eq!(cfg.collections.len(), 1);
        assert_eq!(cfg.collections[0].name, "Users API");
        assert_eq!(cfg.collections[0].ref_type, CollectionRefType::Embedded);
        assert_eq!(cfg.collections[0].path, None);
    }

    #[test]
    fn workspace_config_add_external_collection() {
        let mut cfg = WorkspaceConfig::new("Test");
        cfg.add_external_collection("Shared Auth", PathBuf::from("/home/user/shared"));
        assert_eq!(cfg.collections.len(), 1);
        assert_eq!(cfg.collections[0].ref_type, CollectionRefType::External);
        assert_eq!(cfg.collections[0].path, Some(PathBuf::from("/home/user/shared")));
    }

    #[test]
    fn workspace_config_remove_collection() {
        let mut cfg = WorkspaceConfig::new("Test");
        cfg.add_embedded_collection("A");
        cfg.add_embedded_collection("B");
        cfg.remove_collection("A");
        assert_eq!(cfg.collections.len(), 1);
        assert_eq!(cfg.collections[0].name, "B");
    }

    #[test]
    fn workspace_config_has_collection() {
        let mut cfg = WorkspaceConfig::new("Test");
        cfg.add_embedded_collection("Users API");
        assert!(cfg.has_collection("Users API"));
        assert!(!cfg.has_collection("Other"));
    }

    #[test]
    fn workspace_config_full_serde_roundtrip() {
        let mut cfg = WorkspaceConfig::new("My Project");
        cfg.description = Some("Backend APIs".to_string());
        cfg.add_embedded_collection("Users API");
        cfg.add_external_collection("Shared Auth", PathBuf::from("/tmp/shared"));
        cfg.environments.active_environment = Some("staging".to_string());

        let yaml = serde_yaml::to_string(&cfg).unwrap();
        let back: WorkspaceConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn workspace_config_deserialize_minimal_yaml() {
        let yaml = "name: Minimal\n";
        let cfg: WorkspaceConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.name, "Minimal");
        assert!(cfg.collections.is_empty());
        assert_eq!(cfg.description, None);
    }

    #[test]
    fn workspace_global_environment_roundtrip() {
        let yaml = "name: test\nglobalEnvironment: staging";
        let ws: WorkspaceConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(ws.global_environment, Some("staging".into()));
    }

    #[test]
    fn workspace_global_environment_defaults_none() {
        let ws: WorkspaceConfig = serde_yaml::from_str("name: test").unwrap();
        assert!(ws.global_environment.is_none());
    }

    #[test]
    fn request_guard_policy_default_is_fully_permissive() {
        let policy = RequestGuardPolicy::default();
        assert!(!policy.block_script_redirects_to_internal_hosts);
        assert!(!policy.also_block_private_ranges);
    }

    #[test]
    fn request_guard_policy_serde_roundtrip() {
        let policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: true,
        };
        let yaml = serde_yaml::to_string(&policy).expect("serialize");
        assert!(yaml.contains("blockScriptRedirectsToInternalHosts: true"));
        assert!(yaml.contains("alsoBlockPrivateRanges: true"));
        let back: RequestGuardPolicy = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(policy, back);
    }

    #[test]
    fn request_guard_policy_deserializes_from_empty_yaml_as_permissive() {
        // Backward compatibility: a workspace.yml predating this field must not
        // fail to load, and must not implicitly enable the guard.
        let policy: RequestGuardPolicy = serde_yaml::from_str("{}").expect("deserialize");
        assert_eq!(policy, RequestGuardPolicy::default());
    }

    #[test]
    fn workspace_config_new_has_permissive_request_guard_policy() {
        let cfg = WorkspaceConfig::new("Test");
        assert_eq!(cfg.request_guard_policy, RequestGuardPolicy::default());
    }

    #[test]
    fn workspace_config_request_guard_policy_serde_roundtrip() {
        let mut cfg = WorkspaceConfig::new("My Project");
        cfg.request_guard_policy = RequestGuardPolicy {
            block_script_redirects_to_internal_hosts: true,
            also_block_private_ranges: false,
        };
        let yaml = serde_yaml::to_string(&cfg).expect("serialize");
        let back: WorkspaceConfig = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(cfg, back);
    }

    #[test]
    fn workspace_config_deserialize_minimal_yaml_defaults_request_guard_policy() {
        // A workspace.yml written before this feature existed has no
        // requestGuardPolicy key at all — it must still load, permissively.
        let yaml = "name: Minimal\n";
        let cfg: WorkspaceConfig = serde_yaml::from_str(yaml).expect("deserialize");
        assert_eq!(cfg.request_guard_policy, RequestGuardPolicy::default());
    }
}
