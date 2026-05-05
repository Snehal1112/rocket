use crate::oc::*;
use rocket_workspace::{CollectionReference, CollectionRefType, WorkspaceConfig, WorkspaceEnvironmentsConfig};

impl From<OcWorkspaceCollectionRef> for CollectionReference {
    fn from(r: OcWorkspaceCollectionRef) -> Self {
        match r.path {
            Some(p) if p.is_absolute() => CollectionReference {
                name: r.name,
                ref_type: CollectionRefType::External,
                path: Some(p),
            },
            // Any non-absolute path (relative or absent) is treated as Embedded.
            // External collections in the spec use absolute paths, matching Bruno's convention.
            _ => CollectionReference {
                name: r.name,
                ref_type: CollectionRefType::Embedded,
                path: None,
            },
        }
    }
}

impl From<CollectionReference> for OcWorkspaceCollectionRef {
    fn from(r: CollectionReference) -> Self {
        OcWorkspaceCollectionRef {
            path: match r.ref_type {
                CollectionRefType::Embedded => {
                    Some(std::path::PathBuf::from(format!("collections/{}", r.name)))
                }
                CollectionRefType::External => r.path,
            },
            name: r.name,
        }
    }
}

impl From<OcWorkspaceConfig> for WorkspaceConfig {
    fn from(oc: OcWorkspaceConfig) -> Self {
        WorkspaceConfig {
            name: oc.info.name,
            description: oc.docs,
            collections: oc.collections.into_iter().map(CollectionReference::from).collect(),
            environments: WorkspaceEnvironmentsConfig {
                active_environment: oc.environments.and_then(|e| e.active_environment),
            },
            global_environment: oc.global_environment,
        }
    }
}

impl From<WorkspaceConfig> for OcWorkspaceConfig {
    fn from(w: WorkspaceConfig) -> Self {
        let has_active_env = w.environments.active_environment.is_some();
        OcWorkspaceConfig {
            opencollection: Some("1.0.0".into()),
            info: OcWorkspaceInfo {
                name: w.name,
                workspace_type: Some("workspace".into()),
            },
            collections: w.collections.into_iter().map(OcWorkspaceCollectionRef::from).collect(),
            docs: w.description,
            environments: if has_active_env {
                Some(OcWorkspaceEnvironments {
                    active_environment: w.environments.active_environment,
                })
            } else {
                None
            },
            global_environment: w.global_environment,
        }
    }
}
