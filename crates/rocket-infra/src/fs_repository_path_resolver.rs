use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use rocket_shared::error::{DomainError, DomainResult};
use rocket_workspace::{
    CollectionRefType, RepositoryId, RepositoryKind, RepositoryPathResolver, RepositorySelector,
    ResolvedRepository, Workspace, WorkspaceConfig,
};
use serde::Deserialize;

const COLLECTIONS_DIR: &str = "collections";
const COLLECTION_MARKER: &str = "opencollection.yml";

/// Filesystem-backed resolver for repository IDs that have already been
/// authorized against the current workspace registry.
#[derive(Debug, Default, Clone, Copy)]
pub struct FsRepositoryPathResolver;

impl FsRepositoryPathResolver {
    pub const fn new() -> Self {
        Self
    }

    fn canonical_workspace(workspace: &Workspace) -> DomainResult<PathBuf> {
        let canonical = canonicalize(&workspace.path, "Workspace")?;
        require_directory(&canonical, "Workspace")?;
        Ok(canonical)
    }

    fn resolve_collection(
        id: &RepositoryId,
        workspace: &Workspace,
        config: &WorkspaceConfig,
        collection_uid: &str,
    ) -> DomainResult<ResolvedRepository> {
        let workspace_path = Self::canonical_workspace(workspace)?;
        let collections_path = workspace_path.join(COLLECTIONS_DIR);
        let mut matches = Vec::new();

        match fs::symlink_metadata(&collections_path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err(DomainError::InvalidInput(format!(
                        "Embedded collections root '{}' must not be a symlink",
                        collections_path.display()
                    )));
                }
                if !metadata.is_dir() {
                    return Err(DomainError::InvalidInput(format!(
                        "Embedded collections root '{}' is not a directory",
                        collections_path.display()
                    )));
                }

                let canonical_collections =
                    canonicalize(&collections_path, "Embedded collections root")?;
                if !canonical_collections.starts_with(&workspace_path) {
                    return Err(DomainError::InvalidInput(
                        "Embedded collections root resolves outside the workspace".into(),
                    ));
                }
                Self::find_embedded_matches(&canonical_collections, collection_uid, &mut matches)?;
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(map_io_error(
                    error,
                    format!(
                        "Failed to inspect embedded collections root '{}'",
                        collections_path.display()
                    ),
                ));
            }
        }

        Self::find_external_matches(config, collection_uid, &mut matches)?;

        match matches.len() {
            0 => Err(DomainError::NotFound(format!(
                "Collection repository '{collection_uid}'"
            ))),
            1 => {
                let (kind, path) = matches.remove(0);
                Ok(ResolvedRepository {
                    id: id.clone(),
                    kind,
                    path,
                })
            }
            _ => Err(DomainError::Conflict(format!(
                "Multiple collection roots have UID '{collection_uid}'"
            ))),
        }
    }

    fn find_embedded_matches(
        canonical_collections: &Path,
        collection_uid: &str,
        matches: &mut Vec<(RepositoryKind, PathBuf)>,
    ) -> DomainResult<()> {
        let entries = fs::read_dir(canonical_collections).map_err(|error| {
            map_io_error(
                error,
                format!(
                    "Failed to read embedded collections root '{}'",
                    canonical_collections.display()
                ),
            )
        })?;

        for entry in entries {
            let entry = entry.map_err(|error| {
                DomainError::Io(format!("Failed to read embedded collection entry: {error}"))
            })?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(|error| {
                map_io_error(
                    error,
                    format!("Failed to inspect embedded collection '{}'", path.display()),
                )
            })?;

            // Embedded roots are direct, real child directories. Never follow an
            // entry symlink, even if its target would canonicalize inside the root.
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                continue;
            }

            let canonical = canonicalize(&path, "Embedded collection")?;
            if !canonical.starts_with(canonical_collections) {
                return Err(DomainError::InvalidInput(format!(
                    "Embedded collection '{}' resolves outside the collections root",
                    path.display()
                )));
            }

            if read_collection_uid(&canonical)?.as_deref() == Some(collection_uid) {
                matches.push((RepositoryKind::EmbeddedCollection, canonical));
            }
        }

        Ok(())
    }

    fn find_external_matches(
        config: &WorkspaceConfig,
        collection_uid: &str,
        matches: &mut Vec<(RepositoryKind, PathBuf)>,
    ) -> DomainResult<()> {
        for reference in &config.collections {
            if reference.ref_type != CollectionRefType::External {
                continue;
            }

            let path = reference.path.as_deref().ok_or_else(|| {
                DomainError::InvalidInput(format!(
                    "External collection reference '{}' has no path",
                    reference.name
                ))
            })?;

            reject_symlink(path, "External collection")?;
            require_directory(path, "External collection")?;
            let canonical = canonicalize(path, "External collection")?;
            require_directory(&canonical, "External collection")?;

            if read_collection_uid(&canonical)?.as_deref() == Some(collection_uid) {
                matches.push((RepositoryKind::ExternalCollection, canonical));
            }
        }

        Ok(())
    }
}

impl RepositoryPathResolver for FsRepositoryPathResolver {
    fn resolve(
        &self,
        id: &RepositoryId,
        selector: &RepositorySelector,
        workspace: &Workspace,
        config: Option<&WorkspaceConfig>,
    ) -> DomainResult<ResolvedRepository> {
        let id_selector = id.selector()?;
        if &id_selector != selector {
            return Err(DomainError::InvalidInput(
                "Repository ID does not agree with its selector".into(),
            ));
        }

        match selector {
            RepositorySelector::Workspace { workspace_id } => {
                require_workspace_id(workspace_id, workspace)?;
                Ok(ResolvedRepository {
                    id: id.clone(),
                    kind: RepositoryKind::Workspace,
                    path: Self::canonical_workspace(workspace)?,
                })
            }
            RepositorySelector::Collection {
                workspace_id,
                collection_uid,
            } => {
                require_workspace_id(workspace_id, workspace)?;
                let config = config.ok_or_else(|| {
                    DomainError::InvalidInput(
                        "Workspace configuration is required to resolve a collection".into(),
                    )
                })?;
                Self::resolve_collection(id, workspace, config, collection_uid)
            }
        }
    }
}

#[derive(Deserialize)]
struct CollectionMarker {
    uid: Option<String>,
}

fn read_collection_uid(collection_root: &Path) -> DomainResult<Option<String>> {
    let marker = collection_root.join(COLLECTION_MARKER);
    let metadata = match fs::symlink_metadata(&marker) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(map_io_error(
                error,
                format!("Failed to inspect collection marker '{}'", marker.display()),
            ));
        }
    };

    if metadata.file_type().is_symlink() {
        return Err(DomainError::InvalidInput(format!(
            "Collection marker '{}' must not be a symlink",
            marker.display()
        )));
    }
    if !metadata.is_file() {
        return Err(DomainError::InvalidInput(format!(
            "Collection marker '{}' is not a file",
            marker.display()
        )));
    }

    let contents = fs::read_to_string(&marker).map_err(|error| {
        map_io_error(
            error,
            format!("Failed to read collection marker '{}'", marker.display()),
        )
    })?;
    let parsed: CollectionMarker = serde_yaml::from_str(&contents).map_err(|error| {
        DomainError::Serialization(format!(
            "Failed to parse collection marker '{}': {error}",
            marker.display()
        ))
    })?;

    Ok(parsed.uid.filter(|uid| !uid.is_empty()))
}

fn require_workspace_id(workspace_id: &str, workspace: &Workspace) -> DomainResult<()> {
    if workspace_id != workspace.id {
        return Err(DomainError::InvalidInput(format!(
            "Repository workspace ID '{workspace_id}' does not match workspace '{}'",
            workspace.id
        )));
    }
    Ok(())
}

fn reject_symlink(path: &Path, label: &str) -> DomainResult<()> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        map_io_error(
            error,
            format!("{label} '{}' cannot be inspected", path.display()),
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Err(DomainError::InvalidInput(format!(
            "{label} '{}' must not be a symlink",
            path.display()
        )));
    }
    Ok(())
}

fn require_directory(path: &Path, label: &str) -> DomainResult<()> {
    let metadata = fs::metadata(path).map_err(|error| {
        map_io_error(
            error,
            format!("{label} '{}' cannot be inspected", path.display()),
        )
    })?;
    if !metadata.is_dir() {
        return Err(DomainError::InvalidInput(format!(
            "{label} '{}' is not a directory",
            path.display()
        )));
    }
    Ok(())
}

fn canonicalize(path: &Path, label: &str) -> DomainResult<PathBuf> {
    fs::canonicalize(path).map_err(|error| {
        map_io_error(
            error,
            format!("{label} '{}' cannot be resolved", path.display()),
        )
    })
}

fn map_io_error(error: std::io::Error, context: String) -> DomainError {
    if error.kind() == ErrorKind::NotFound {
        DomainError::NotFound(context)
    } else {
        DomainError::Io(format!("{context}: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn workspace(path: PathBuf) -> Workspace {
        Workspace {
            id: "workspace-1".into(),
            name: "Test Workspace".into(),
            path,
            description: None,
            pinned: false,
        }
    }

    fn create_workspace(temp: &TempDir) -> Workspace {
        let path = temp.path().join("workspace");
        fs::create_dir_all(path.join(COLLECTIONS_DIR)).expect("create workspace");
        workspace(path)
    }

    fn write_collection(path: &Path, uid: &str) {
        fs::create_dir_all(path).expect("create collection directory");
        fs::write(
            path.join(COLLECTION_MARKER),
            format!("opencollection: 1.0.0\nuid: {uid}\ninfo:\n  name: Test Collection\n"),
        )
        .expect("write collection marker");
    }

    fn workspace_selector() -> RepositorySelector {
        RepositorySelector::Workspace {
            workspace_id: "workspace-1".into(),
        }
    }

    fn collection_selector(uid: &str) -> RepositorySelector {
        RepositorySelector::Collection {
            workspace_id: "workspace-1".into(),
            collection_uid: uid.into(),
        }
    }

    #[test]
    fn resolves_workspace_to_its_canonical_directory() {
        let temp = TempDir::new().expect("tempdir");
        let mut workspace = create_workspace(&temp);
        workspace.path = workspace.path.join(".");
        let id = RepositoryId::workspace(&workspace.id).expect("workspace ID");

        let resolved = FsRepositoryPathResolver::new()
            .resolve(&id, &workspace_selector(), &workspace, None)
            .expect("resolve workspace");

        assert_eq!(resolved.id, id);
        assert_eq!(resolved.kind, RepositoryKind::Workspace);
        assert_eq!(
            resolved.path,
            fs::canonicalize(&workspace.path).expect("canonical workspace")
        );
    }

    #[test]
    fn rejects_missing_workspace_and_collections_root() {
        let temp = TempDir::new().expect("tempdir");
        let missing_workspace = workspace(temp.path().join("missing"));
        let workspace_id = RepositoryId::workspace(&missing_workspace.id).expect("workspace ID");
        assert!(matches!(
            FsRepositoryPathResolver.resolve(
                &workspace_id,
                &workspace_selector(),
                &missing_workspace,
                None,
            ),
            Err(DomainError::NotFound(_))
        ));

        let path = temp.path().join("workspace-without-collections");
        fs::create_dir(&path).expect("create workspace");
        let workspace = workspace(path);
        let collection_id =
            RepositoryId::collection(&workspace.id, "missing").expect("collection ID");
        assert!(matches!(
            FsRepositoryPathResolver.resolve(
                &collection_id,
                &collection_selector("missing"),
                &workspace,
                Some(&WorkspaceConfig::new("Test")),
            ),
            Err(DomainError::NotFound(_))
        ));
    }

    #[test]
    fn resolves_embedded_collection_by_persisted_uid() {
        let temp = TempDir::new().expect("tempdir");
        let workspace = create_workspace(&temp);
        let collection = workspace.path.join(COLLECTIONS_DIR).join("users-api");
        write_collection(&collection, "embedded-uid");
        let id = RepositoryId::collection(&workspace.id, "embedded-uid").expect("collection ID");

        let resolved = FsRepositoryPathResolver
            .resolve(
                &id,
                &collection_selector("embedded-uid"),
                &workspace,
                Some(&WorkspaceConfig::new("Test")),
            )
            .expect("resolve embedded collection");

        assert_eq!(resolved.kind, RepositoryKind::EmbeddedCollection);
        assert_eq!(
            resolved.path,
            fs::canonicalize(collection).expect("canonical collection")
        );
    }

    #[test]
    fn resolves_only_configured_external_collection() {
        let temp = TempDir::new().expect("tempdir");
        let workspace = create_workspace(&temp);
        let external = temp.path().join("external");
        write_collection(&external, "external-uid");
        let mut config = WorkspaceConfig::new("Test");
        config.add_external_collection("External", external.clone());
        let id = RepositoryId::collection(&workspace.id, "external-uid").expect("collection ID");

        let resolved = FsRepositoryPathResolver
            .resolve(
                &id,
                &collection_selector("external-uid"),
                &workspace,
                Some(&config),
            )
            .expect("resolve external collection");

        assert_eq!(resolved.kind, RepositoryKind::ExternalCollection);
        assert_eq!(
            resolved.path,
            fs::canonicalize(external).expect("canonical external collection")
        );
    }

    #[test]
    fn resolves_external_collection_without_embedded_collections_directory() {
        let temp = TempDir::new().expect("tempdir");
        let workspace_path = temp.path().join("external-only-workspace");
        fs::create_dir(&workspace_path).expect("create workspace");
        let workspace = workspace(workspace_path);
        let external = temp.path().join("external-only");
        write_collection(&external, "external-only-uid");
        let mut config = WorkspaceConfig::new("Test");
        config.add_external_collection("External", external.clone());
        let id =
            RepositoryId::collection(&workspace.id, "external-only-uid").expect("collection ID");

        let resolved = FsRepositoryPathResolver
            .resolve(
                &id,
                &collection_selector("external-only-uid"),
                &workspace,
                Some(&config),
            )
            .expect("resolve external collection");

        assert_eq!(resolved.kind, RepositoryKind::ExternalCollection);
        assert_eq!(
            resolved.path,
            fs::canonicalize(external).expect("canonical external collection")
        );
    }

    #[test]
    fn cannot_resolve_forged_or_unconfigured_external_path() {
        let temp = TempDir::new().expect("tempdir");
        let workspace = create_workspace(&temp);
        let external = temp.path().join("unconfigured-external");
        write_collection(&external, "forged-uid");
        let id = RepositoryId::collection(&workspace.id, "forged-uid").expect("collection ID");

        let result = FsRepositoryPathResolver.resolve(
            &id,
            &collection_selector("forged-uid"),
            &workspace,
            Some(&WorkspaceConfig::new("Test")),
        );

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    #[test]
    fn duplicate_collection_uid_fails_closed() {
        let temp = TempDir::new().expect("tempdir");
        let workspace = create_workspace(&temp);
        write_collection(
            &workspace.path.join(COLLECTIONS_DIR).join("first"),
            "duplicate-uid",
        );
        write_collection(
            &workspace.path.join(COLLECTIONS_DIR).join("second"),
            "duplicate-uid",
        );
        let id = RepositoryId::collection(&workspace.id, "duplicate-uid").expect("collection ID");

        let result = FsRepositoryPathResolver.resolve(
            &id,
            &collection_selector("duplicate-uid"),
            &workspace,
            Some(&WorkspaceConfig::new("Test")),
        );

        assert!(matches!(result, Err(DomainError::Conflict(_))));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_embedded_root_and_entry_symlinks() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().expect("tempdir");
        let outside = temp.path().join("outside");
        write_collection(&outside, "outside-uid");

        let entry_workspace = create_workspace(&temp);
        symlink(
            &outside,
            entry_workspace.path.join(COLLECTIONS_DIR).join("escaped"),
        )
        .expect("create collection symlink");
        let id =
            RepositoryId::collection(&entry_workspace.id, "outside-uid").expect("collection ID");
        assert!(matches!(
            FsRepositoryPathResolver.resolve(
                &id,
                &collection_selector("outside-uid"),
                &entry_workspace,
                Some(&WorkspaceConfig::new("Test")),
            ),
            Err(DomainError::NotFound(_))
        ));

        let root_workspace_path = temp.path().join("workspace-with-linked-root");
        fs::create_dir(&root_workspace_path).expect("create workspace");
        symlink(&outside, root_workspace_path.join(COLLECTIONS_DIR))
            .expect("create collections root symlink");
        let root_workspace = workspace(root_workspace_path);
        let id =
            RepositoryId::collection(&root_workspace.id, "outside-uid").expect("collection ID");
        assert!(matches!(
            FsRepositoryPathResolver.resolve(
                &id,
                &collection_selector("outside-uid"),
                &root_workspace,
                Some(&WorkspaceConfig::new("Test")),
            ),
            Err(DomainError::InvalidInput(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_external_root_symlink() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().expect("tempdir");
        let workspace = create_workspace(&temp);
        let external = temp.path().join("external");
        write_collection(&external, "external-uid");
        let linked_external = temp.path().join("linked-external");
        symlink(&external, &linked_external).expect("create external symlink");
        let mut config = WorkspaceConfig::new("Test");
        config.add_external_collection("Linked", linked_external);
        let id = RepositoryId::collection(&workspace.id, "external-uid").expect("collection ID");

        assert!(matches!(
            FsRepositoryPathResolver.resolve(
                &id,
                &collection_selector("external-uid"),
                &workspace,
                Some(&config),
            ),
            Err(DomainError::InvalidInput(_))
        ));
    }

    #[test]
    fn collection_root_without_marker_does_not_resolve() {
        let temp = TempDir::new().expect("tempdir");
        let workspace = create_workspace(&temp);
        fs::create_dir(workspace.path.join(COLLECTIONS_DIR).join("no-marker"))
            .expect("create markerless collection");
        let id = RepositoryId::collection(&workspace.id, "missing-marker").expect("collection ID");

        assert!(matches!(
            FsRepositoryPathResolver.resolve(
                &id,
                &collection_selector("missing-marker"),
                &workspace,
                Some(&WorkspaceConfig::new("Test")),
            ),
            Err(DomainError::NotFound(_))
        ));
    }

    #[test]
    fn rejects_mismatched_workspace_selector_and_id() {
        let temp = TempDir::new().expect("tempdir");
        let workspace = create_workspace(&temp);
        let workspace_id = RepositoryId::workspace(&workspace.id).expect("workspace ID");
        let wrong_workspace_selector = RepositorySelector::Workspace {
            workspace_id: "workspace-2".into(),
        };
        assert!(matches!(
            FsRepositoryPathResolver.resolve(
                &workspace_id,
                &wrong_workspace_selector,
                &workspace,
                None,
            ),
            Err(DomainError::InvalidInput(_))
        ));

        let other_id = RepositoryId::workspace("workspace-2").expect("workspace ID");
        let other_selector = RepositorySelector::Workspace {
            workspace_id: "workspace-2".into(),
        };
        assert!(matches!(
            FsRepositoryPathResolver.resolve(&other_id, &other_selector, &workspace, None),
            Err(DomainError::InvalidInput(_))
        ));

        let collection_id =
            RepositoryId::collection("workspace-2", "collection-uid").expect("collection ID");
        let collection_selector = RepositorySelector::Collection {
            workspace_id: "workspace-2".into(),
            collection_uid: "collection-uid".into(),
        };
        assert!(matches!(
            FsRepositoryPathResolver.resolve(
                &collection_id,
                &collection_selector,
                &workspace,
                Some(&WorkspaceConfig::new("Test")),
            ),
            Err(DomainError::InvalidInput(_))
        ));
    }
}
