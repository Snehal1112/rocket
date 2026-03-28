use std::fs;
use std::path::{Path, PathBuf};

use rocket_collection::{Collection, CollectionRepository, CollectionSettings, CollectionSummary, Folder};
use rocket_shared::error::{DomainError, DomainResult};

use crate::oc_conversions::{oc_http_request_to_request, request_to_oc_http_request};
use crate::opencollection::{OcCollection, OcFolderInfo, OcHttpRequest, OcInfo};

/// Read UID from YAML metadata (opencollection.yml or folder.yml).
/// Falls back to legacy .uid file, migrating the value into YAML.
fn read_uid_from_yaml(dir: &Path) -> String {
    // Try opencollection.yml first (collection root).
    let oc_path = dir.join("opencollection.yml");
    if oc_path.exists() {
        if let Ok(content) = fs::read_to_string(&oc_path) {
            if let Ok(mut oc) = serde_yaml::from_str::<OcCollection>(&content) {
                if let Some(ref uid) = oc.uid {
                    if !uid.is_empty() {
                        return uid.clone();
                    }
                }
                // No UID in YAML — check legacy .uid file.
                let uid = read_legacy_uid(dir);
                oc.uid = Some(uid.clone());
                if let Ok(yaml) = serde_yaml::to_string(&oc) {
                    let _ = fs::write(&oc_path, yaml);
                }
                cleanup_legacy_uid(dir);
                return uid;
            }
        }
    }

    // Try folder.yml (subfolder).
    let folder_path = dir.join("folder.yml");
    if folder_path.exists() {
        if let Ok(content) = fs::read_to_string(&folder_path) {
            if let Ok(mut info) = serde_yaml::from_str::<OcFolderInfo>(&content) {
                if let Some(ref uid) = info.uid {
                    if !uid.is_empty() {
                        return uid.clone();
                    }
                }
                let uid = read_legacy_uid(dir);
                info.uid = Some(uid.clone());
                if let Ok(yaml) = serde_yaml::to_string(&info) {
                    let _ = fs::write(&folder_path, yaml);
                }
                cleanup_legacy_uid(dir);
                return uid;
            }
        }
    }

    // No YAML metadata at all — use legacy .uid.
    read_legacy_uid(dir)
}

/// Read UID from legacy .uid file, or generate a new one.
fn read_legacy_uid(dir: &Path) -> String {
    let uid_path = dir.join(".uid");
    if let Ok(uid) = fs::read_to_string(&uid_path) {
        let trimmed = uid.trim().to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }
    uuid::Uuid::new_v4().to_string()
}

/// Delete the legacy .uid file if it exists.
fn cleanup_legacy_uid(dir: &Path) {
    let uid_path = dir.join(".uid");
    if uid_path.exists() {
        let _ = fs::remove_file(&uid_path);
    }
}

pub struct FsCollectionRepo {
    base_dir: PathBuf,
}

impl FsCollectionRepo {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    fn collection_path(&self, name: &str) -> PathBuf {
        self.base_dir.join(name)
    }

    fn settings_path(&self, name: &str) -> PathBuf {
        self.collection_path(name).join("collection.json")
    }

    /// Resolves `path` under `base` and verifies it stays inside `base`.
    /// Works for paths that do not exist yet by canonicalizing the nearest
    /// existing ancestor and then appending the remaining components.
    fn validate_path(&self, base: &Path, path: &Path) -> Result<PathBuf, DomainError> {
        let full = base.join(path);

        let canonical_base = base
            .canonicalize()
            .map_err(|_| DomainError::NotFound("Base dir not found".into()))?;

        // Walk up to find the deepest ancestor that already exists on disk.
        let mut existing = full.as_path();
        while !existing.exists() {
            match existing.parent() {
                Some(p) => existing = p,
                None => {
                    return Err(DomainError::NotFound("Path not found".into()));
                }
            }
        }

        let canonical_existing = existing
            .canonicalize()
            .map_err(|_| DomainError::NotFound("Path not found".into()))?;

        // Reconstruct the full canonical path by appending any not-yet-existing suffix.
        let suffix = full.strip_prefix(existing).unwrap_or(Path::new(""));
        let canonical_full = if suffix == Path::new("") {
            canonical_existing
        } else {
            canonical_existing.join(suffix)
        };

        if !canonical_full.starts_with(&canonical_base) {
            return Err(DomainError::InvalidInput("Path traversal detected".into()));
        }

        Ok(canonical_full)
    }
}

impl CollectionRepository for FsCollectionRepo {
    fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
        let mut result = Vec::new();
        if !self.base_dir.exists() {
            return Ok(result);
        }
        for entry in fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                // Only recognize directories that contain opencollection.yml.
                if !path.join("opencollection.yml").exists() {
                    continue;
                }
                let count = count_request_files(&path);
                let uid = read_uid_from_yaml(&path);
                let modified_at = fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs().to_string());
                result.push(CollectionSummary::new(
                    uid,
                    &name,
                    path.to_string_lossy().to_string(),
                    count,
                    modified_at,
                ));
            }
        }
        result.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(result)
    }

    fn get(&self, name: &str) -> DomainResult<Collection> {
        let path = self.collection_path(name);
        if !path.exists() {
            return Err(DomainError::NotFound(format!("Collection '{}'", name)));
        }
        let root = build_folder_tree(&path)?;
        let settings = self.get_settings(name).unwrap_or_default();
        Ok(Collection { name: name.to_string(), root, settings })
    }

    fn create(&self, name: &str) -> DomainResult<Collection> {
        Collection::validate_name(name)?;
        let path = self.collection_path(name);
        if path.exists() {
            return Err(DomainError::AlreadyExists(format!("Collection '{}'", name)));
        }
        fs::create_dir_all(&path)?;

        // Write opencollection.yml with basic info.
        let oc = OcCollection {
            opencollection: Some("0.1".into()),
            uid: Some(uuid::Uuid::new_v4().to_string()),
            info: Some(OcInfo {
                name: name.into(),
                summary: None,
                version: None,
                authors: None,
            }),
            config: None,
            items: None,
            request: None,
            docs: None,
            bundled: None,
            extensions: None,
        };
        let yaml = serde_yaml::to_string(&oc)
            .map_err(|e| DomainError::Internal(format!("Failed to serialize opencollection.yml: {e}")))?;
        fs::write(path.join("opencollection.yml"), yaml)?;

        Ok(Collection::new(name))
    }

    fn delete(&self, name: &str) -> DomainResult<()> {
        let path = self.collection_path(name);
        if !path.exists() {
            return Err(DomainError::NotFound(format!("Collection '{}'", name)));
        }
        fs::remove_dir_all(&path)?;
        Ok(())
    }

    fn rename(&self, old_name: &str, new_name: &str) -> DomainResult<()> {
        Collection::validate_name(new_name)?;
        let old_path = self.collection_path(old_name);
        let new_path = self.collection_path(new_name);
        if !old_path.exists() {
            return Err(DomainError::NotFound(format!("Collection '{}'", old_name)));
        }
        if new_path.exists() {
            return Err(DomainError::AlreadyExists(format!("Collection '{}'", new_name)));
        }
        fs::rename(&old_path, &new_path)?;
        Ok(())
    }

    fn get_request(&self, collection: &str, path: &str) -> DomainResult<rocket_collection::Request> {
        let collection_dir = self.collection_path(collection);

        // Try .yml first, then .json for backward compatibility.
        let yml_path = if path.ends_with(".yml") || path.ends_with(".yaml") {
            path.to_string()
        } else {
            format!("{}.yml", path.strip_suffix(".json").unwrap_or(path))
        };

        // Try .yml first.
        if let Ok(file_path) = self.validate_path(&collection_dir, Path::new(&yml_path)) {
            if file_path.exists() && file_path.extension().map_or(false, |e| e == "yml" || e == "yaml") {
                let content = fs::read_to_string(&file_path)?;
                let oc: OcHttpRequest = serde_yaml::from_str(&content)
                    .map_err(|e| DomainError::Internal(format!("Failed to parse YAML request: {e}")))?;
                let mut req = oc_http_request_to_request(oc);
                req.file_name = file_path.file_name().map(|n| n.to_string_lossy().to_string());
                return Ok(req);
            }
        }

        // Fall back to .json for legacy files.
        let json_path = if path.ends_with(".json") { path.to_string() } else { format!("{}.json", path) };
        let file_path = self.validate_path(&collection_dir, Path::new(&json_path))
            .or_else(|_| self.validate_path(&collection_dir, Path::new(path)))?;
        if !file_path.exists() {
            return Err(DomainError::NotFound(format!("{}/{}", collection, path)));
        }
        let content = fs::read_to_string(&file_path)?;
        Ok(serde_json::from_str(&content)?)
    }

    fn save_request(&self, collection: &str, path: &str, request: &rocket_collection::Request) -> DomainResult<String> {
        let collection_dir = self.collection_path(collection);
        // Use .yml extension instead of .json.
        let base = if path.ends_with(".yml") || path.ends_with(".yaml") {
            path.to_string()
        } else if path.ends_with(".json") {
            // Strip .json and add .yml for migration.
            format!("{}.yml", &path[..path.len() - 5])
        } else {
            format!("{}.yml", path)
        };
        let mut file_path = self.validate_path(&collection_dir, Path::new(&base))?;

        // Same duplicate-avoidance logic, but with .yml.
        if request.uid.is_empty() && file_path.exists() {
            let stem = Path::new(&base).file_stem().unwrap_or_default().to_string_lossy().to_string();
            let parent_rel = Path::new(&base).parent().unwrap_or(Path::new(""));
            let mut counter = 1u32;
            loop {
                let candidate = if parent_rel.as_os_str().is_empty() {
                    format!("{} {}.yml", stem, counter)
                } else {
                    format!("{}/{} {}.yml", parent_rel.display(), stem, counter)
                };
                let candidate_path = self.validate_path(&collection_dir, Path::new(&candidate))?;
                if !candidate_path.exists() {
                    file_path = candidate_path;
                    break;
                }
                counter += 1;
            }
        }

        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Convert domain Request to OcHttpRequest, then serialize as YAML.
        let oc = request_to_oc_http_request(request.clone());
        let yaml = serde_yaml::to_string(&oc)
            .map_err(|e| DomainError::Internal(format!("Failed to serialize request YAML: {e}")))?;
        fs::write(&file_path, yaml)?;

        // Return the actual filename relative to the collection directory.
        let actual = file_path
            .strip_prefix(&collection_dir)
            .unwrap_or(&file_path)
            .to_string_lossy()
            .to_string();
        Ok(actual)
    }

    fn rename_request(&self, collection: &str, old_path: &str, new_path: &str) -> DomainResult<()> {
        let collection_dir = self.collection_path(collection);
        let old_file = resolve_request_path(self, &collection_dir, old_path)?;
        let new_ext = if new_path.ends_with(".yml") || new_path.ends_with(".yaml") || new_path.ends_with(".json") {
            new_path.to_string()
        } else {
            format!("{}.yml", new_path)
        };
        let new_file = self.validate_path(&collection_dir, Path::new(&new_ext))?;
        fs::rename(&old_file, &new_file)?;
        Ok(())
    }

    fn delete_request(&self, collection: &str, path: &str) -> DomainResult<()> {
        let collection_dir = self.collection_path(collection);
        let file_path = resolve_request_path(self, &collection_dir, path)?;
        if !file_path.exists() {
            return Err(DomainError::NotFound(format!("{}/{}", collection, path)));
        }
        fs::remove_file(&file_path)?;
        Ok(())
    }

    fn create_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
        let collection_dir = self.collection_path(collection);
        let dir_path = self.validate_path(&collection_dir, Path::new(path))?;
        fs::create_dir_all(&dir_path)?;

        // Write folder.yml with folder metadata.
        let folder_name = Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string());
        let info = OcFolderInfo {
            name: folder_name,
            uid: Some(uuid::Uuid::new_v4().to_string()),
            description: None,
            folder_type: Some("folder".into()),
            seq: None,
            tags: Vec::new(),
        };
        let yaml = serde_yaml::to_string(&info)
            .map_err(|e| DomainError::Internal(format!("Failed to serialize folder.yml: {e}")))?;
        fs::write(dir_path.join("folder.yml"), yaml)?;

        Ok(())
    }

    fn delete_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
        let collection_dir = self.collection_path(collection);
        let dir_path = self.validate_path(&collection_dir, Path::new(path))?;
        if !dir_path.exists() {
            return Err(DomainError::NotFound(format!("{}/{}", collection, path)));
        }
        fs::remove_dir_all(&dir_path)?;
        Ok(())
    }

    fn move_item(
        &self,
        src_collection: &str,
        src_path: &str,
        dst_collection: &str,
        dst_path: &str,
    ) -> DomainResult<()> {
        let src_collection_dir = self.collection_path(src_collection);
        let dst_collection_dir = self.collection_path(dst_collection);
        let src = self.validate_path(&src_collection_dir, Path::new(src_path))?;
        let dst = self.validate_path(&dst_collection_dir, Path::new(dst_path))?;
        if !src.exists() {
            return Err(DomainError::NotFound(format!("{}/{}", src_collection, src_path)));
        }
        if dst.starts_with(&src) {
            return Err(DomainError::InvalidInput("Cannot move into itself".into()));
        }
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&src, &dst)?;
        Ok(())
    }

    fn reorder_items(&self, collection: &str, folder_path: &str, ordered_names: &[String]) -> DomainResult<()> {
        let collection_dir = self.collection_path(collection);
        let dir = if folder_path.is_empty() {
            collection_dir.clone()
        } else {
            self.validate_path(&collection_dir, Path::new(folder_path))?
        };
        if !dir.is_dir() {
            return Err(DomainError::NotFound(format!("{}/{}", collection, folder_path)));
        }
        let yaml = serde_yaml::to_string(ordered_names)
            .map_err(|e| DomainError::Internal(format!("Failed to serialize order: {e}")))?;
        fs::write(dir.join("_order.yml"), yaml)?;
        Ok(())
    }

    fn get_settings(&self, name: &str) -> DomainResult<CollectionSettings> {
        let path = self.settings_path(name);
        if !path.exists() {
            return Ok(CollectionSettings::default());
        }
        let content = fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&content)?)
    }

    fn save_settings(&self, name: &str, settings: &CollectionSettings) -> DomainResult<()> {
        let path = self.settings_path(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(settings)?;
        fs::write(&path, json)?;
        Ok(())
    }
}

fn count_request_files(dir: &Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += count_request_files(&path);
            } else if is_request_file(&path) {
                count += 1;
            }
        }
    }
    count
}

fn is_request_file(path: &Path) -> bool {
    // Exclude reserved sidecar and config files.
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if matches!(name, "collection.json" | "_order.json" | "_order.yml" | "opencollection.yml" | "folder.yml") {
            return false;
        }
    }
    path.extension().is_some_and(|ext| ext == "json" || ext == "yml" || ext == "yaml" || ext == "bru")
}

/// Resolve a request file path, trying .yml first, then .json for backward compat.
fn resolve_request_path(repo: &FsCollectionRepo, collection_dir: &Path, path: &str) -> DomainResult<PathBuf> {
    // Try .yml first.
    let yml = if path.ends_with(".yml") || path.ends_with(".yaml") {
        path.to_string()
    } else {
        format!("{}.yml", path.strip_suffix(".json").unwrap_or(path))
    };
    if let Ok(p) = repo.validate_path(collection_dir, Path::new(&yml)) {
        if p.exists() {
            return Ok(p);
        }
    }
    // Fall back to .json.
    let json = if path.ends_with(".json") { path.to_string() } else { format!("{}.json", path) };
    repo.validate_path(collection_dir, Path::new(&json))
        .or_else(|_| repo.validate_path(collection_dir, Path::new(path)))
}

fn build_folder_tree(current: &Path) -> DomainResult<Folder> {
    let name = current
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut folder = Folder::new(name);
    folder.uid = read_uid_from_yaml(current);

    // Read folder.yml for metadata if present.
    let folder_yml = current.join("folder.yml");
    if folder_yml.exists() {
        if let Ok(content) = fs::read_to_string(&folder_yml) {
            if let Ok(info) = serde_yaml::from_str::<OcFolderInfo>(&content) {
                folder.name = info.name;
            }
        }
    }

    if !current.exists() {
        return Ok(folder);
    }

    let mut entries: Vec<_> = fs::read_dir(current)?.filter_map(|e| e.ok()).collect();
    // Apply explicit order from _order.yml (or _order.json for backward compat).
    let order_path = current.join("_order.yml");
    let order_path = if order_path.exists() { order_path } else { current.join("_order.json") };
    if let Ok(content) = fs::read_to_string(&order_path) {
        if let Ok(ordered) = serde_yaml::from_str::<Vec<String>>(&content) {
            let pos: std::collections::HashMap<String, usize> = ordered
                .into_iter().enumerate().map(|(i, name)| (name, i)).collect();
            entries.sort_by(|a, b| {
                let ai = pos.get(&a.file_name().to_string_lossy().into_owned()).copied().unwrap_or(usize::MAX);
                let bi = pos.get(&b.file_name().to_string_lossy().into_owned()).copied().unwrap_or(usize::MAX);
                ai.cmp(&bi).then_with(|| a.file_name().cmp(&b.file_name()))
            });
        } else if let Ok(ordered) = serde_json::from_str::<Vec<String>>(&content) {
            let pos: std::collections::HashMap<String, usize> = ordered
                .into_iter().enumerate().map(|(i, name)| (name, i)).collect();
            entries.sort_by(|a, b| {
                let ai = pos.get(&a.file_name().to_string_lossy().into_owned()).copied().unwrap_or(usize::MAX);
                let bi = pos.get(&b.file_name().to_string_lossy().into_owned()).copied().unwrap_or(usize::MAX);
                ai.cmp(&bi).then_with(|| a.file_name().cmp(&b.file_name()))
            });
        } else {
            entries.sort_by_key(|e| e.file_name());
        }
    } else {
        entries.sort_by_key(|e| e.file_name());
    }

    for entry in entries {
        let path = entry.path();
        let entry_name = entry.file_name().to_string_lossy().to_string();
        if entry_name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            folder.add_subfolder(build_folder_tree(&path)?);
        } else if is_request_file(&path) {
            let content = fs::read_to_string(&path)?;
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let request_result = match ext {
                "yml" | "yaml" => {
                    serde_yaml::from_str::<OcHttpRequest>(&content)
                        .map(oc_http_request_to_request)
                        .map_err(|e| DomainError::Internal(e.to_string()))
                }
                _ => {
                    serde_json::from_str::<rocket_collection::Request>(&content)
                        .map_err(|e| DomainError::Internal(e.to_string()))
                }
            };
            if let Ok(mut request) = request_result {
                request.file_name = Some(entry_name.clone());
                folder.add_request(request);
            }
        }
    }

    Ok(folder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::CollectionRepository;
    use rocket_shared::types::HttpMethod;
    use tempfile::TempDir;

    fn setup() -> (TempDir, FsCollectionRepo) {
        let dir = TempDir::new().unwrap();
        let repo = FsCollectionRepo::new(dir.path().to_path_buf());
        (dir, repo)
    }

    #[test]
    fn list_empty() {
        let (_dir, repo) = setup();
        assert!(repo.list().unwrap().is_empty());
    }

    #[test]
    fn create_and_list() {
        let (_dir, repo) = setup();
        repo.create("my-api").unwrap();
        let list = repo.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "my-api");
    }

    #[test]
    fn create_duplicate_fails() {
        let (_dir, repo) = setup();
        repo.create("my-api").unwrap();
        assert!(repo.create("my-api").is_err());
    }

    #[test]
    fn delete_collection() {
        let (_dir, repo) = setup();
        repo.create("my-api").unwrap();
        repo.delete("my-api").unwrap();
        assert!(repo.list().unwrap().is_empty());
    }

    #[test]
    fn rename_collection() {
        let (_dir, repo) = setup();
        repo.create("old").unwrap();
        repo.rename("old", "new").unwrap();
        let list = repo.list().unwrap();
        assert_eq!(list[0].name, "new");
    }

    #[test]
    fn save_and_read_request() {
        let (_dir, repo) = setup();
        repo.create("my-api").unwrap();
        let req = rocket_collection::Request::new("Get Users", HttpMethod::Get, "https://api.example.com/users");
        repo.save_request("my-api", "get-users.yml", &req).unwrap();
        let loaded = repo.get_request("my-api", "get-users.yml").unwrap();
        assert_eq!(loaded.name, "Get Users");
    }

    #[test]
    fn save_request_in_subfolder() {
        let (_dir, repo) = setup();
        repo.create("my-api").unwrap();
        let req = rocket_collection::Request::new("Login", HttpMethod::Post, "/login");
        repo.save_request("my-api", "auth/login.yml", &req).unwrap();
        let loaded = repo.get_request("my-api", "auth/login.yml").unwrap();
        assert_eq!(loaded.name, "Login");
    }

    #[test]
    fn delete_request() {
        let (_dir, repo) = setup();
        repo.create("my-api").unwrap();
        let req = rocket_collection::Request::new("Test", HttpMethod::Get, "/test");
        repo.save_request("my-api", "test.yml", &req).unwrap();
        repo.delete_request("my-api", "test.yml").unwrap();
        assert!(repo.get_request("my-api", "test.yml").is_err());
    }

    #[test]
    fn create_and_delete_folder() {
        let (_dir, repo) = setup();
        repo.create("my-api").unwrap();
        repo.create_folder("my-api", "auth").unwrap();
        repo.delete_folder("my-api", "auth").unwrap();
    }

    #[test]
    fn move_request_across_folders() {
        let (_dir, repo) = setup();
        repo.create("my-api").unwrap();
        let req = rocket_collection::Request::new("Test", HttpMethod::Get, "/test");
        repo.save_request("my-api", "old/test.yml", &req).unwrap();
        repo.move_item("my-api", "old/test.yml", "my-api", "new/test.yml").unwrap();
        assert!(repo.get_request("my-api", "old/test.yml").is_err());
        assert!(repo.get_request("my-api", "new/test.yml").is_ok());
    }

    #[test]
    fn settings_default_when_no_file() {
        let (_dir, repo) = setup();
        repo.create("my-api").unwrap();
        let settings = repo.get_settings("my-api").unwrap();
        assert_eq!(settings, rocket_collection::CollectionSettings::default());
        assert!(settings.auth.is_none());
        assert!(settings.headers.is_empty());
    }

    #[test]
    fn settings_roundtrip() {
        use rocket_shared::types::{Auth, Header};

        let (_dir, repo) = setup();
        repo.create("my-api").unwrap();

        let original = rocket_collection::CollectionSettings {
            description: None,
            auth: Some(Auth::Bearer { token: "tok_abc".into() }),
            headers: vec![Header::new("X-Tenant", "acme")],
            variables: vec![],
        };
        repo.save_settings("my-api", &original).unwrap();
        let loaded = repo.get_settings("my-api").unwrap();
        assert_eq!(loaded, original);
    }

    #[test]
    fn settings_file_not_counted_as_request() {
        use rocket_shared::types::Auth;

        let (_dir, repo) = setup();
        repo.create("my-api").unwrap();

        // Save settings, then verify the request count stays zero.
        let settings = rocket_collection::CollectionSettings {
            description: None,
            auth: Some(Auth::None),
            headers: vec![],
            variables: vec![],
        };
        repo.save_settings("my-api", &settings).unwrap();

        let list = repo.list().unwrap();
        assert_eq!(list[0].request_count, 0);
    }

    #[test]
    fn path_traversal_in_get_request_is_rejected() {
        let (_dir, repo) = setup();
        repo.create("my-api").unwrap();
        let result = repo.get_request("my-api", "../../etc/passwd");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, DomainError::InvalidInput(_) | DomainError::NotFound(_)),
            "expected traversal to be blocked, got {:?}",
            err
        );
    }

    #[test]
    fn path_traversal_in_save_request_is_rejected() {
        let (_dir, repo) = setup();
        repo.create("my-api").unwrap();
        let req = rocket_collection::Request::new("Bad", rocket_shared::types::HttpMethod::Get, "/bad");
        let result = repo.save_request("my-api", "../../evil.yml", &req);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, DomainError::InvalidInput(_) | DomainError::NotFound(_)),
            "expected traversal to be blocked, got {:?}",
            err
        );
    }

    #[test]
    fn path_traversal_in_delete_request_is_rejected() {
        let (_dir, repo) = setup();
        repo.create("my-api").unwrap();
        let result = repo.delete_request("my-api", "../../etc/passwd");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, DomainError::InvalidInput(_) | DomainError::NotFound(_)),
            "expected traversal to be blocked, got {:?}",
            err
        );
    }

    #[test]
    fn path_traversal_in_create_folder_is_rejected() {
        let (_dir, repo) = setup();
        repo.create("my-api").unwrap();
        let result = repo.create_folder("my-api", "../../evil-dir");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, DomainError::InvalidInput(_) | DomainError::NotFound(_)),
            "expected traversal to be blocked, got {:?}",
            err
        );
    }

    #[test]
    fn path_traversal_in_delete_folder_is_rejected() {
        let (_dir, repo) = setup();
        repo.create("my-api").unwrap();
        let result = repo.delete_folder("my-api", "../../etc");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, DomainError::InvalidInput(_) | DomainError::NotFound(_)),
            "expected traversal to be blocked, got {:?}",
            err
        );
    }

    #[test]
    fn path_traversal_in_move_item_src_is_rejected() {
        let (_dir, repo) = setup();
        repo.create("my-api").unwrap();
        let result = repo.move_item("my-api", "../../etc/passwd", "my-api", "dest.yml");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, DomainError::InvalidInput(_) | DomainError::NotFound(_)),
            "expected traversal to be blocked, got {:?}",
            err
        );
    }

    #[test]
    fn reorder_items_writes_order_file_and_get_respects_it() {
        use rocket_collection::CollectionItem;

        fn item_name(item: &CollectionItem) -> &str {
            match item {
                CollectionItem::Request(r) => r.name.as_str(),
                CollectionItem::Folder(f) => f.name.as_str(),
            }
        }

        let (_dir, repo) = setup();
        repo.create("test-col").unwrap();

        // Create two requests. Alphabetically "aaa" comes before "bbb".
        let req_a = rocket_collection::Request::new("AAA", HttpMethod::Get, "/a");
        let req_b = rocket_collection::Request::new("BBB", HttpMethod::Get, "/b");
        repo.save_request("test-col", "aaa.yml", &req_a).unwrap();
        repo.save_request("test-col", "bbb.yml", &req_b).unwrap();

        // Confirm default (alphabetical) order: aaa first.
        let col = repo.get("test-col").unwrap();
        assert_eq!(item_name(&col.root.items[0]), "AAA");
        assert_eq!(item_name(&col.root.items[1]), "BBB");

        // Reorder so bbb comes first.
        repo.reorder_items(
            "test-col",
            "",
            &["bbb.yml".to_string(), "aaa.yml".to_string()],
        )
        .unwrap();

        // After reorder, bbb should appear first.
        let col = repo.get("test-col").unwrap();
        assert_eq!(item_name(&col.root.items[0]), "BBB");
        assert_eq!(item_name(&col.root.items[1]), "AAA");
    }

    #[test]
    fn path_traversal_in_reorder_items_is_rejected() {
        let (_dir, repo) = setup();
        repo.create("my-api").unwrap();
        let result = repo.reorder_items("my-api", "../../evil", &["x.yml".to_string()]);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, DomainError::InvalidInput(_) | DomainError::NotFound(_)),
            "expected traversal to be blocked, got {:?}",
            err
        );
    }

    #[test]
    fn path_traversal_in_move_item_dst_is_rejected() {
        let (_dir, repo) = setup();
        repo.create("my-api").unwrap();
        let req = rocket_collection::Request::new("T", rocket_shared::types::HttpMethod::Get, "/t");
        repo.save_request("my-api", "src.yml", &req).unwrap();
        let result = repo.move_item("my-api", "src.yml", "my-api", "../../evil.yml");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, DomainError::InvalidInput(_) | DomainError::NotFound(_)),
            "expected traversal to be blocked, got {:?}",
            err
        );
    }

    #[test]
    fn folder_yml_exists_after_create_folder() {
        let (dir, repo) = setup();
        repo.create("my-api").unwrap();
        repo.create_folder("my-api", "auth").unwrap();
        assert!(dir.path().join("my-api/auth/folder.yml").exists());
    }

    #[test]
    fn folder_yml_not_counted_as_request() {
        let (_dir, repo) = setup();
        repo.create("my-api").unwrap();
        repo.create_folder("my-api", "auth").unwrap();
        let list = repo.list().unwrap();
        assert_eq!(list[0].request_count, 0);
    }

    #[test]
    fn list_ignores_dirs_without_opencollection_yml() {
        let (dir, repo) = setup();
        // Create a plain directory (not via repo.create).
        fs::create_dir(dir.path().join("plain-dir")).unwrap();
        // Create a proper collection.
        repo.create("proper").unwrap();
        let list = repo.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "proper");
    }

    #[test]
    fn opencollection_yml_exists_after_create() {
        let (dir, repo) = setup();
        repo.create("my-api").unwrap();
        assert!(dir.path().join("my-api/opencollection.yml").exists());
    }

    #[test]
    fn opencollection_yml_not_counted_as_request() {
        let (_dir, repo) = setup();
        repo.create("my-api").unwrap();
        let list = repo.list().unwrap();
        assert_eq!(list[0].request_count, 0);
    }

    #[test]
    fn legacy_uid_migrated_into_opencollection_yml() {
        let (dir, repo) = setup();
        repo.create("my-api").unwrap();
        let col_dir = dir.path().join("my-api");

        // Simulate a legacy collection: write .uid file and remove uid from opencollection.yml.
        let legacy_uid = "legacy-uid-12345";
        fs::write(col_dir.join(".uid"), legacy_uid).unwrap();

        // Re-read opencollection.yml, strip uid, rewrite.
        let content = fs::read_to_string(col_dir.join("opencollection.yml")).unwrap();
        let mut oc: OcCollection = serde_yaml::from_str(&content).unwrap();
        oc.uid = None;
        let yaml = serde_yaml::to_string(&oc).unwrap();
        fs::write(col_dir.join("opencollection.yml"), yaml).unwrap();

        // List should trigger migration.
        let list = repo.list().unwrap();
        assert_eq!(list[0].uid, legacy_uid);

        // .uid file should be deleted.
        assert!(!col_dir.join(".uid").exists());

        // opencollection.yml should now contain the uid.
        let content = fs::read_to_string(col_dir.join("opencollection.yml")).unwrap();
        assert!(content.contains(legacy_uid));
    }

    #[test]
    fn legacy_uid_migrated_into_folder_yml() {
        let (dir, repo) = setup();
        repo.create("my-api").unwrap();
        repo.create_folder("my-api", "auth").unwrap();
        let folder_dir = dir.path().join("my-api/auth");

        // Simulate legacy: write .uid, strip uid from folder.yml.
        let legacy_uid = "folder-uid-67890";
        fs::write(folder_dir.join(".uid"), legacy_uid).unwrap();

        let content = fs::read_to_string(folder_dir.join("folder.yml")).unwrap();
        let mut info: OcFolderInfo = serde_yaml::from_str(&content).unwrap();
        info.uid = None;
        let yaml = serde_yaml::to_string(&info).unwrap();
        fs::write(folder_dir.join("folder.yml"), yaml).unwrap();

        // Load the collection — build_folder_tree should trigger migration.
        let col = repo.get("my-api").unwrap();
        let auth_folder = col.root.find_folder("auth").unwrap();
        assert_eq!(auth_folder.uid, legacy_uid);

        // .uid file should be deleted.
        assert!(!folder_dir.join(".uid").exists());

        // folder.yml should now contain the uid.
        let content = fs::read_to_string(folder_dir.join("folder.yml")).unwrap();
        assert!(content.contains(legacy_uid));
    }

    #[test]
    fn no_uid_file_created_on_new_collection() {
        let (dir, repo) = setup();
        repo.create("my-api").unwrap();
        // No .uid file should exist.
        assert!(!dir.path().join("my-api/.uid").exists());
        // UID should be in opencollection.yml.
        let content = fs::read_to_string(dir.path().join("my-api/opencollection.yml")).unwrap();
        assert!(content.contains("uid:"));
    }

    #[test]
    fn no_uid_file_created_on_new_folder() {
        let (dir, repo) = setup();
        repo.create("my-api").unwrap();
        repo.create_folder("my-api", "auth").unwrap();
        // No .uid file should exist.
        assert!(!dir.path().join("my-api/auth/.uid").exists());
        // UID should be in folder.yml.
        let content = fs::read_to_string(dir.path().join("my-api/auth/folder.yml")).unwrap();
        assert!(content.contains("uid:"));
    }
}
