use std::fs;
use std::path::Path;

use rocket_collection::{CollectionItem, Folder, RequestSummary};
use rocket_shared::error::{DomainError, DomainResult};

use crate::conversions::oc_http_request_to_request;
use crate::oc::{OcFolderInfo, OcHttpRequest};

use super::paths::{is_request_file, read_uid_from_yaml};

pub(super) fn build_folder_tree(current: &Path) -> DomainResult<Folder> {
    build_tree(current, &mut |path, entry_name| {
        let content = fs::read_to_string(path)?;
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let request_result = match ext {
            "yml" | "yaml" => serde_yaml::from_str::<OcHttpRequest>(&content)
                .map(oc_http_request_to_request)
                .map_err(|e| DomainError::Internal(e.to_string())),
            _ => serde_json::from_str::<rocket_collection::Request>(&content)
                .map_err(|e| DomainError::Internal(e.to_string())),
        };
        match request_result {
            Ok(mut request) => {
                request.file_name = Some(entry_name.to_string());
                Ok(Some(CollectionItem::Request(request)))
            }
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "skipping corrupt request file");
                Ok(None)
            }
        }
    })
}

/// Build the folder tree loading only the minimal fields needed for the sidebar.
/// Skips full request body parsing for a significant speedup on large collections.
pub(super) fn build_folder_tree_summaries(current: &Path) -> DomainResult<Folder> {
    build_tree(current, &mut |path, entry_name| {
        match load_request_summary(path, entry_name) {
            Ok(summary) => Ok(Some(CollectionItem::Summary(summary))),
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "skipping corrupt request file in summary load");
                Ok(None)
            }
        }
    })
}

/// Shared folder-tree walker. Handles UID/name loading, ordering, symlink rejection, and
/// recursion. The `load_item` closure decides what to do with each request file — it returns
/// `Ok(Some(item))` to add an item, `Ok(None)` to skip it, or `Err` to propagate.
fn build_tree<F>(current: &Path, load_item: &mut F) -> DomainResult<Folder>
where
    F: FnMut(&Path, &str) -> DomainResult<Option<CollectionItem>>,
{
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
            folder.add_subfolder(build_tree(&path, load_item)?);
        } else if is_request_file(&path) {
            if let Some(item) = load_item(&path, &entry_name)? {
                folder.items.push(item);
            }
        }
    }

    Ok(folder)
}

/// Parse only the uid/name/method/url fields from a request file for sidebar display.
/// Non-HTTP protocol files (e.g. a GraphQL .yml that passes is_request_file) will fail
/// here and be skipped by the caller's warn path.
fn load_request_summary(path: &Path, entry_name: &str) -> DomainResult<RequestSummary> {
    let content = fs::read_to_string(path)?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    if ext == "yml" || ext == "yaml" {
        #[derive(serde::Deserialize)]
        struct MinReq {
            uid: Option<String>,
            info: MinInfo,
            http: MinHttp,
        }
        #[derive(serde::Deserialize)]
        struct MinInfo {
            name: String,
        }
        #[derive(serde::Deserialize)]
        struct MinHttp {
            method: String,
            url: String,
        }
        let min: MinReq = serde_yaml::from_str(&content)
            .map_err(|e| DomainError::Internal(format!("Failed to parse request summary: {e}")))?;
        Ok(RequestSummary {
            uid: min.uid.unwrap_or_default(),
            name: min.info.name,
            method: min.http.method,
            url: min.http.url,
            file_name: Some(entry_name.to_string()),
        })
    } else {
        // Legacy JSON: full Request deserialization then extract fields.
        let req: rocket_collection::Request = serde_json::from_str(&content)
            .map_err(|e| DomainError::Internal(format!("Failed to parse legacy request: {e}")))?;
        Ok(RequestSummary {
            uid: req.uid,
            name: req.name,
            method: req.method.to_string(),
            url: req.url,
            file_name: Some(entry_name.to_string()),
        })
    }
}
