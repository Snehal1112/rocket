use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Deserializer, Serialize};

use crate::{Workspace, WorkspaceConfig};

const WORKSPACE_PREFIX: &str = "workspace:";
const COLLECTION_PREFIX: &str = "collection:";

/// An opaque, path-free identifier accepted at the renderer/backend boundary.
///
/// Repository IDs identify entries in the current workspace registry. They do
/// not authorize a filesystem path by themselves; every use must be resolved
/// against the current registry and workspace configuration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct RepositoryId(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositorySelector {
    Workspace {
        workspace_id: String,
    },
    Collection {
        workspace_id: String,
        collection_uid: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryKind {
    Workspace,
    EmbeddedCollection,
    ExternalCollection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRepository {
    pub id: RepositoryId,
    pub kind: RepositoryKind,
    pub path: PathBuf,
}

/// Resolves an already-authorized workspace selector to a canonical path.
///
/// The application layer is responsible for loading the workspace named by the
/// selector from the current registry and, for collection selectors, loading
/// that workspace's current configuration before invoking this trait.
pub trait RepositoryPathResolver: Send + Sync {
    fn resolve(
        &self,
        id: &RepositoryId,
        selector: &RepositorySelector,
        workspace: &Workspace,
        config: Option<&WorkspaceConfig>,
    ) -> DomainResult<ResolvedRepository>;
}

impl RepositoryId {
    pub fn workspace(workspace_id: &str) -> DomainResult<Self> {
        validate_component("workspace ID", workspace_id)?;
        Ok(Self(format!("{WORKSPACE_PREFIX}{workspace_id}")))
    }

    pub fn collection(workspace_id: &str, collection_uid: &str) -> DomainResult<Self> {
        validate_component("workspace ID", workspace_id)?;
        validate_component("collection UID", collection_uid)?;
        Ok(Self(format!(
            "{COLLECTION_PREFIX}{workspace_id}:{collection_uid}"
        )))
    }

    pub fn parse(value: &str) -> DomainResult<Self> {
        let id = Self(value.to_string());
        id.selector()?;
        Ok(id)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn selector(&self) -> DomainResult<RepositorySelector> {
        if let Some(workspace_id) = self.0.strip_prefix(WORKSPACE_PREFIX) {
            validate_component("workspace ID", workspace_id)?;
            return Ok(RepositorySelector::Workspace {
                workspace_id: workspace_id.to_string(),
            });
        }

        if let Some(value) = self.0.strip_prefix(COLLECTION_PREFIX) {
            let mut components = value.split(':');
            let workspace_id = components.next().unwrap_or_default();
            let collection_uid = components.next().unwrap_or_default();
            if components.next().is_some() {
                return Err(invalid_repository_id());
            }
            validate_component("workspace ID", workspace_id)?;
            validate_component("collection UID", collection_uid)?;
            return Ok(RepositorySelector::Collection {
                workspace_id: workspace_id.to_string(),
                collection_uid: collection_uid.to_string(),
            });
        }

        Err(invalid_repository_id())
    }
}

impl fmt::Display for RepositoryId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for RepositoryId {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl<'de> Deserialize<'de> for RepositoryId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

fn validate_component(label: &str, value: &str) -> DomainResult<()> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(DomainError::InvalidInput(format!(
            "Repository {label} contains unsupported characters"
        )));
    }
    Ok(())
}

fn invalid_repository_id() -> DomainError {
    DomainError::InvalidInput(
        "Repository ID must use 'workspace:<workspace-id>' or \
         'collection:<workspace-id>:<collection-uid>'"
            .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructs_and_parses_workspace_id() {
        let id = RepositoryId::workspace("default").expect("valid workspace ID");

        assert_eq!(id.as_str(), "workspace:default");
        assert_eq!(
            id.selector().expect("valid selector"),
            RepositorySelector::Workspace {
                workspace_id: "default".into()
            }
        );
    }

    #[test]
    fn constructs_and_parses_collection_id() {
        let id =
            RepositoryId::collection("workspace-1", "collection-2").expect("valid collection ID");

        assert_eq!(id.as_str(), "collection:workspace-1:collection-2");
        assert_eq!(
            id.selector().expect("valid selector"),
            RepositorySelector::Collection {
                workspace_id: "workspace-1".into(),
                collection_uid: "collection-2".into(),
            }
        );
    }

    #[test]
    fn rejects_unknown_or_ambiguous_formats() {
        for value in [
            "",
            "repo:default",
            "workspace:",
            "workspace:one:two",
            "collection:workspace",
            "collection:workspace:",
            "collection:workspace:collection:extra",
        ] {
            assert!(RepositoryId::parse(value).is_err(), "accepted {value:?}");
        }
    }

    #[test]
    fn rejects_path_and_control_characters() {
        for value in [
            "workspace:../outside",
            "workspace:with space",
            "workspace:with/slash",
            "collection:workspace:../outside",
            "collection:workspace:with\\slash",
            "collection:workspace:uid\0suffix",
        ] {
            assert!(RepositoryId::parse(value).is_err(), "accepted {value:?}");
        }
    }

    #[test]
    fn serde_uses_an_opaque_validated_string() {
        let id = RepositoryId::workspace("default").expect("valid workspace ID");
        let yaml = serde_yaml::to_string(&id).expect("serialize repository ID");
        let decoded: RepositoryId = serde_yaml::from_str(&yaml).expect("deserialize repository ID");
        assert_eq!(decoded, id);
        assert!(serde_yaml::from_str::<RepositoryId>("workspace:../outside").is_err());
    }

    #[test]
    fn resolver_trait_is_object_safe() {
        fn _assert_object_safe(_: Box<dyn RepositoryPathResolver>) {}
    }
}
