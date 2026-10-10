//! Finds the collections that already exist on disk, for the one-time trust migration.

use std::path::{Path, PathBuf};

use rocket_collection::{CollectionRepository, LegacyCollection, RequestedElevation};
use rocket_workspace::{CollectionRefType, WorkspaceConfigRepository};

use crate::FsCollectionRepo;

/// Lists the embedded collections of every workspace and the external collections named
/// in each `workspace.yml`. A collection that cannot be read is skipped, so it ends up
/// untrusted. Duplicates (same canonical folder) are listed once.
pub fn discover_legacy_collections(
    workspace_paths: &[PathBuf],
    config_repo: &dyn WorkspaceConfigRepository,
) -> Vec<LegacyCollection> {
    let mut found: Vec<LegacyCollection> = Vec::new();
    for workspace in workspace_paths {
        let repo = FsCollectionRepo::new_standalone(workspace.join("collections"));
        if let Ok(summaries) = repo.list() {
            for summary in summaries {
                push_from_repo(&mut found, &repo, &summary.name);
            }
        }
        let Ok(config) = config_repo.load(workspace) else {
            continue;
        };
        for reference in config.collections {
            if reference.ref_type != CollectionRefType::External {
                continue;
            }
            let Some(path) = reference.path else { continue };
            push_external(&mut found, &path);
        }
    }
    found
}

fn push_external(found: &mut Vec<LegacyCollection>, dir: &Path) {
    let (Some(parent), Some(name)) = (dir.parent(), dir.file_name().and_then(|n| n.to_str()))
    else {
        return;
    };
    let repo = FsCollectionRepo::new_standalone(parent.to_path_buf());
    push_from_repo(found, &repo, name);
}

fn push_from_repo(found: &mut Vec<LegacyCollection>, repo: &FsCollectionRepo, name: &str) {
    let (Ok(identity), Ok(settings)) = (repo.collection_identity(name), repo.get_settings(name))
    else {
        return;
    };
    if found
        .iter()
        .any(|c| c.identity.canonical_root == identity.canonical_root)
    {
        return;
    }
    found.push(LegacyCollection {
        name: name.to_string(),
        identity,
        requested: RequestedElevation::from_settings(&settings),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FsWorkspaceConfigRepo;
    use rocket_collection::settings::SandboxMode;
    use rocket_collection::CollectionSettings;

    #[test]
    fn finds_embedded_collections_with_their_requests() {
        let ws = tempfile::tempdir().expect("tempdir");
        let repo = FsCollectionRepo::new_standalone(ws.path().join("collections"));
        std::fs::create_dir_all(ws.path().join("collections")).expect("mkdir");
        repo.create("plain").expect("create");
        repo.create("dev").expect("create");
        repo.save_settings(
            "dev",
            &CollectionSettings {
                sandbox_mode: SandboxMode::Developer,
                ..Default::default()
            },
        )
        .expect("save");
        let found = discover_legacy_collections(
            &[ws.path().to_path_buf()],
            &FsWorkspaceConfigRepo::new(),
        );
        assert_eq!(found.len(), 2);
        let dev = found.iter().find(|c| c.name == "dev").expect("dev");
        assert!(dev.requested.developer_mode);
    }

    #[test]
    fn a_missing_workspace_gives_nothing() {
        let found = discover_legacy_collections(
            &[PathBuf::from("/definitely/not/here")],
            &FsWorkspaceConfigRepo::new(),
        );
        assert!(found.is_empty());
    }
}
