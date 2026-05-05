use std::fs;
use std::path::Path;

use rocket_collection::Folder;
use rocket_shared::error::{DomainError, DomainResult};

use crate::conversions::oc_http_request_to_request;
use crate::oc::{OcFolderInfo, OcHttpRequest};

use super::paths::{is_request_file, read_uid_from_yaml};

pub(super) fn build_folder_tree(current: &Path) -> DomainResult<Folder> {
    let dir_name = current
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut folder = Folder::new(&dir_name);
    // Clear the auto-generated UID; we'll load the actual one from disk or legacy sources.
    folder.uid = String::new();

    // Parse folder.yml once to extract both uid and display name.
    // For the collection root, folder.yml does not exist — fall back to read_uid_from_yaml
    // which reads opencollection.yml instead.
    let folder_yml = current.join("folder.yml");
    if folder_yml.exists() {
        if let Ok(content) = fs::read_to_string(&folder_yml) {
            if let Ok(info) = serde_yaml::from_str::<OcFolderInfo>(&content) {
                if let Some(ref uid) = info.uid {
                    if !uid.is_empty() {
                        folder.uid = uid.clone();
                    }
                }
                folder.name = info.name;
            }
        }
        if folder.uid.is_empty() {
            folder.uid = read_uid_from_yaml(current);
        }
    } else {
        folder.uid = read_uid_from_yaml(current);
    }
    folder.dir_name = Some(dir_name);

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
                let ai = a.file_name().to_str().and_then(|n| pos.get(n)).copied().unwrap_or(usize::MAX);
                let bi = b.file_name().to_str().and_then(|n| pos.get(n)).copied().unwrap_or(usize::MAX);
                ai.cmp(&bi).then_with(|| a.file_name().cmp(&b.file_name()))
            });
        } else if let Ok(ordered) = serde_json::from_str::<Vec<String>>(&content) {
            let pos: std::collections::HashMap<String, usize> = ordered
                .into_iter().enumerate().map(|(i, name)| (name, i)).collect();
            entries.sort_by(|a, b| {
                let ai = a.file_name().to_str().and_then(|n| pos.get(n)).copied().unwrap_or(usize::MAX);
                let bi = b.file_name().to_str().and_then(|n| pos.get(n)).copied().unwrap_or(usize::MAX);
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
        if entry_name.starts_with('.') || entry_name == "environments" {
            continue;
        }
        if path.is_dir() {
            // Skip symlinked directories to prevent exfiltration.
            if std::fs::symlink_metadata(&path).map(|m| m.file_type().is_symlink()).unwrap_or(false) {
                tracing::warn!(path = %path.display(), "skipping symlinked directory in folder tree");
                continue;
            }
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
            match request_result {
                Ok(mut request) => {
                    request.file_name = Some(entry_name.clone());
                    folder.add_request(request);
                }
                Err(e) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "skipping corrupt request file"
                    );
                }
            }
        }
    }

    Ok(folder)
}
