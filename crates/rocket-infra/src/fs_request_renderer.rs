//! Renders collection files the way `FsCollectionRepo` writes them, without touching the real
//! collection. The apply of an OpenAPI sync stages these bytes in one file transaction.

use std::path::PathBuf;

use rocket_collection::{
    request_filename_for, CollectionRepository, CollectionSettings, Request, RequestRenderer,
};
use rocket_shared::error::{DomainError, DomainResult};

use crate::FsCollectionRepo;

/// Name of the collection inside the throwaway folder.
const SCRATCH: &str = "render";

/// Saves into a temporary `FsCollectionRepo` and reads the bytes back, so the output is exactly
/// what a normal save writes. The temporary folder is removed after each call.
#[derive(Default)]
pub struct FsRequestRenderer {
    /// The real collection folder, used only to read files for the round-trip guard.
    root: Option<PathBuf>,
}

impl FsRequestRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    /// A renderer that can also judge the files of the collection at `root`. Without a root,
    /// `can_rewrite` says no.
    pub fn for_collection(root: PathBuf) -> Self {
        Self { root: Some(root) }
    }

    /// Runs `f` against a fresh scratch collection.
    fn scratch<T>(
        &self,
        f: impl FnOnce(&FsCollectionRepo, &std::path::Path) -> DomainResult<T>,
    ) -> DomainResult<T> {
        let dir = tempfile::tempdir().map_err(|e| {
            DomainError::Internal(format!("a scratch folder could not be made: {e}"))
        })?;
        let repo = FsCollectionRepo::new_standalone(dir.path().to_path_buf());
        repo.create(SCRATCH)?;
        let root = dir.path().join(SCRATCH);
        f(&repo, &root)
    }
}

fn read(root: &std::path::Path, rel: &str) -> DomainResult<Vec<u8>> {
    std::fs::read(root.join(rel))
        .map_err(|e| DomainError::Internal(format!("a rendered file could not be read: {e}")))
}

impl RequestRenderer for FsRequestRenderer {
    fn render(&self, request_path: &str, request: &Request) -> DomainResult<Vec<u8>> {
        self.scratch(|repo, root| {
            repo.save_request(SCRATCH, request_path, request)?;
            read(root, &request_filename_for(request_path))
        })
    }

    fn render_settings(&self, settings: &CollectionSettings) -> DomainResult<(String, Vec<u8>)> {
        // The scratch collection starts from the real settings file, so its name, uid, docs and
        // extensions survive. Without a root there is nothing to start from, so refuse.
        let Some(real) = &self.root else {
            return Err(DomainError::InvalidInput(
                "the collection settings cannot be rendered without the collection folder".into(),
            ));
        };
        let rel = "opencollection.yml";
        let existing = std::fs::read(real.join(rel))
            .map_err(|e| DomainError::Internal(format!("{rel} could not be read: {e}")))?;
        self.scratch(|repo, root| {
            std::fs::write(root.join(rel), &existing).map_err(|e| {
                DomainError::Internal(format!("a scratch file could not be written: {e}"))
            })?;
            repo.save_settings(SCRATCH, settings)?;
            Ok((rel.to_string(), read(root, rel)?))
        })
    }

    fn render_folder(&self, folder_path: &str) -> DomainResult<Vec<u8>> {
        // A folder file that already exists is kept as it is.
        if let Some(real) = &self.root {
            if let Ok(bytes) = std::fs::read(real.join(folder_path).join("folder.yml")) {
                return Ok(bytes);
            }
        }
        self.scratch(|repo, root| {
            repo.create_folder(SCRATCH, folder_path)?;
            read(root, &format!("{folder_path}/folder.yml"))
        })
    }

    fn can_rewrite(&self, request_path: &str, request: &Request) -> DomainResult<bool> {
        let Some(root) = &self.root else {
            return Ok(false);
        };
        let on_disk = match std::fs::read_to_string(root.join(request_filename_for(request_path))) {
            Ok(text) => text,
            Err(_) => return Ok(false),
        };
        let rendered = self.render(request_path, request)?;
        // Compare as YAML values, so key order and spacing do not count.
        let Ok(old) = serde_yaml::from_str::<serde_yaml::Value>(&on_disk) else {
            return Ok(false);
        };
        let Ok(new) = serde_yaml::from_slice::<serde_yaml::Value>(&rendered) else {
            return Ok(false);
        };
        Ok(old == new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::types::HttpMethod;
    use tempfile::tempdir;

    #[test]
    fn renderer_output_equals_a_normal_save() {
        let request = Request::new("List pets", HttpMethod::Get, "{{baseUrl}}/pets");
        let bytes = FsRequestRenderer::new()
            .render("Pets/List-pets", &request)
            .expect("renders");
        let dir = tempdir().expect("tempdir");
        let repo = FsCollectionRepo::new_standalone(dir.path().to_path_buf());
        repo.create("c").expect("create");
        repo.save_request("c", "Pets/List-pets", &request)
            .expect("save");
        let saved = std::fs::read(dir.path().join("c/Pets/List-pets.yml")).expect("read");
        assert_eq!(bytes, saved);
    }

    #[test]
    fn can_rewrite_is_true_for_a_file_rocket_wrote_and_false_for_extras() {
        let dir = tempdir().expect("tempdir");
        let repo = FsCollectionRepo::new_standalone(dir.path().to_path_buf());
        repo.create("c").expect("create");
        let request = Request::new("One", HttpMethod::Get, "http://x");
        repo.save_request("c", "One", &request).expect("save");
        let root = dir.path().join("c");
        let renderer = FsRequestRenderer::for_collection(root.clone());
        let loaded = repo.get_request("c", "One").expect("get");
        assert!(renderer.can_rewrite("One", &loaded).expect("guard"));
        let file = root.join("One.yml");
        let mut text = std::fs::read_to_string(&file).expect("read");
        text.push_str("unmodeled-key: 1\n");
        std::fs::write(&file, text).expect("write");
        assert!(!renderer.can_rewrite("One", &loaded).expect("guard"));
        assert!(!FsRequestRenderer::new()
            .can_rewrite("One", &loaded)
            .expect("guard"));
    }

    #[test]
    fn folder_and_settings_render() {
        let renderer = FsRequestRenderer::new();
        let folder = renderer.render_folder("A/B").expect("folder");
        assert!(!folder.is_empty());
        assert!(renderer
            .render_settings(&CollectionSettings::default())
            .is_err());
        let dir = tempdir().expect("tempdir");
        let repo = FsCollectionRepo::new_standalone(dir.path().to_path_buf());
        repo.create("c").expect("create");
        let real = FsRequestRenderer::for_collection(dir.path().join("c"));
        let (rel, bytes) = real
            .render_settings(&CollectionSettings::default())
            .expect("settings");
        assert_eq!(rel, "opencollection.yml");
        assert!(String::from_utf8(bytes).expect("utf8").contains("name: c"));
    }
}
