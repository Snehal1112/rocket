use std::fs;
use std::path::Path;

use rocket_collection::{CollectionItem, Folder, RequestKind, RequestSummary};
use rocket_shared::error::{DomainError, DomainResult};

use crate::conversions::{oc_http_request_to_request, oc_item_to_collection_item};
use crate::oc::{OcHttpRequest, OcItem};

use super::folder_file::parse_folder_yml;
use super::paths::{is_request_file, read_uid_from_yaml};

pub(super) fn build_folder_tree(current: &Path) -> DomainResult<Folder> {
    build_tree(current, &mut |path, entry_name| {
        let content = fs::read_to_string(path)?;
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let loaded = match ext {
            "yml" | "yaml" => load_yaml_item(&content),
            _ => serde_json::from_str::<rocket_collection::Request>(&content)
                .map(|r| Some(CollectionItem::Request(Box::new(r))))
                .map_err(|e| e.to_string()),
        };
        match loaded {
            Ok(Some(CollectionItem::Request(mut request))) => {
                request.file_name = Some(entry_name.to_string());
                Ok(Some(CollectionItem::Request(request)))
            }
            Ok(Some(CollectionItem::GraphQl(mut gql))) => {
                gql.file_name = Some(entry_name.to_string());
                Ok(Some(CollectionItem::GraphQl(gql)))
            }
            Ok(Some(CollectionItem::WebSocket(mut ws))) => {
                crate::conversions::with_file_identity(&mut ws, entry_name);
                Ok(Some(CollectionItem::WebSocket(ws)))
            }
            Ok(other) => Ok(other),
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "skipping corrupt request file");
                Ok(None)
            }
        }
    })
}

/// Parses one `.yml` item file. HTTP is tried first so a broken HTTP file keeps
/// its precise parse error. Other protocols are recognised through the untagged
/// `OcItem` enum. A file that matches only `OcItem::Folder` is a broken request,
/// since a folder is a directory and never a single file, so it is reported with
/// the HTTP parse error.
fn load_yaml_item(content: &str) -> Result<Option<CollectionItem>, String> {
    let http_err = match serde_yaml::from_str::<OcHttpRequest>(content) {
        Ok(req) => {
            return Ok(Some(CollectionItem::Request(Box::new(
                oc_http_request_to_request(req),
            ))))
        }
        Err(e) => e,
    };
    match serde_yaml::from_str::<OcItem>(content) {
        Ok(OcItem::Folder(_)) | Err(_) => Err(http_err.to_string()),
        Ok(OcItem::ScriptFile(_)) => {
            tracing::debug!("skipping script file; scripts are not collection tree items");
            Ok(None)
        }
        Ok(item) => Ok(oc_item_to_collection_item(item)),
    }
}

/// Build the folder tree loading only the minimal fields needed for the sidebar.
/// Skips full request body parsing for a significant speedup on large collections.
pub(super) fn build_folder_tree_summaries(current: &Path) -> DomainResult<Folder> {
    build_tree(
        current,
        &mut |path, entry_name| match load_request_summary(path, entry_name) {
            Ok(Some(summary)) => Ok(Some(CollectionItem::Summary(summary))),
            Ok(None) => {
                tracing::debug!(path = %path.display(), "skipping non-HTTP item in summary load");
                Ok(None)
            }
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "skipping corrupt request file in summary load");
                Ok(None)
            }
        },
    )
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
            if let Ok(oc_folder) = parse_folder_yml(&content) {
                if let Some(ref uid) = oc_folder.info.uid {
                    if !uid.is_empty() {
                        folder.uid = uid.clone();
                    }
                }
                folder.name = oc_folder.info.name;
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
    let order_path = if order_path.exists() {
        order_path
    } else {
        current.join("_order.json")
    };
    if let Ok(content) = fs::read_to_string(&order_path) {
        if let Ok(ordered) = serde_yaml::from_str::<Vec<String>>(&content) {
            let pos: std::collections::HashMap<String, usize> = ordered
                .into_iter()
                .enumerate()
                .map(|(i, name)| (name, i))
                .collect();
            entries.sort_by(|a, b| {
                let ai = a
                    .file_name()
                    .to_str()
                    .and_then(|n| pos.get(n))
                    .copied()
                    .unwrap_or(usize::MAX);
                let bi = b
                    .file_name()
                    .to_str()
                    .and_then(|n| pos.get(n))
                    .copied()
                    .unwrap_or(usize::MAX);
                ai.cmp(&bi).then_with(|| a.file_name().cmp(&b.file_name()))
            });
        } else if let Ok(ordered) = serde_json::from_str::<Vec<String>>(&content) {
            let pos: std::collections::HashMap<String, usize> = ordered
                .into_iter()
                .enumerate()
                .map(|(i, name)| (name, i))
                .collect();
            entries.sort_by(|a, b| {
                let ai = a
                    .file_name()
                    .to_str()
                    .and_then(|n| pos.get(n))
                    .copied()
                    .unwrap_or(usize::MAX);
                let bi = b
                    .file_name()
                    .to_str()
                    .and_then(|n| pos.get(n))
                    .copied()
                    .unwrap_or(usize::MAX);
                ai.cmp(&bi).then_with(|| a.file_name().cmp(&b.file_name()))
            });
        } else {
            entries.sort_by_key(|e| e.file_name());
        }
    } else {
        entries.sort_by_key(|e| e.file_name());
    }

    // Only the collection root holds Rocket's flow files, so a nested `flows` folder stays visible.
    let is_collection_root = current.join("opencollection.yml").exists();
    for entry in entries {
        let path = entry.path();
        let entry_name = entry.file_name().to_string_lossy().to_string();
        if entry_name.starts_with('.') || entry_name == "environments" {
            continue;
        }
        if is_collection_root && entry_name == "flows" {
            continue;
        }
        if path.is_dir() {
            // Skip symlinked directories to prevent exfiltration.
            if std::fs::symlink_metadata(&path)
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false)
            {
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
/// GraphQL and WebSocket files return a summary with their `kind`. gRPC and ScriptFile
/// .yml files that pass `is_request_file` are recognised via the untagged `OcItem` probe,
/// just like `load_yaml_item`, and return `Ok(None)` so the caller can skip them silently
/// (at debug level) instead of reporting them as corrupt. A file that matches only
/// `OcItem::Folder` is a broken request, not a recognised non-HTTP item — a folder is a
/// directory and never a single file — so, like `load_yaml_item`, it is reported as
/// genuine corruption alongside anything that matches no `OcItem` variant at all.
fn load_request_summary(path: &Path, entry_name: &str) -> DomainResult<Option<RequestSummary>> {
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
        let min_err = match serde_yaml::from_str::<MinReq>(&content) {
            Ok(min) => {
                return Ok(Some(RequestSummary {
                    uid: min.uid.unwrap_or_default(),
                    name: min.info.name,
                    method: min.http.method,
                    url: min.http.url,
                    file_name: Some(entry_name.to_string()),
                    kind: RequestKind::Http,
                }))
            }
            Err(e) => e,
        };
        match serde_yaml::from_str::<OcItem>(&content) {
            Ok(OcItem::GraphQL(gql)) => Ok(Some(RequestSummary {
                uid: gql.uid.unwrap_or_default(),
                name: gql.info.name,
                method: gql.graphql.method.unwrap_or_else(|| "POST".to_string()),
                url: gql.graphql.url,
                file_name: Some(entry_name.to_string()),
                kind: RequestKind::GraphQl,
            })),
            Ok(OcItem::WebSocket(ws)) => Ok(Some(RequestSummary {
                uid: ws
                    .uid
                    .filter(|u| !u.is_empty())
                    .unwrap_or_else(|| crate::conversions::derived_websocket_uid(entry_name)),
                name: ws.info.name,
                // A WebSocket handshake is a GET. The sidebar badge comes from `kind`, not this.
                method: "GET".to_string(),
                url: ws.websocket.url,
                file_name: Some(entry_name.to_string()),
                kind: RequestKind::WebSocket,
            })),
            Ok(OcItem::Http(_)) | Ok(OcItem::Folder(_)) | Err(_) => Err(DomainError::Internal(
                format!("Failed to parse request summary: {min_err}"),
            )),
            Ok(OcItem::Grpc(_) | OcItem::ScriptFile(_)) => Ok(None),
        }
    } else {
        // Legacy JSON: full Request deserialization then extract fields.
        let req: rocket_collection::Request = serde_json::from_str(&content)
            .map_err(|e| DomainError::Internal(format!("Failed to parse legacy request: {e}")))?;
        Ok(Some(RequestSummary {
            uid: req.uid,
            name: req.name,
            method: req.method.to_string(),
            url: req.url,
            file_name: Some(entry_name.to_string()),
            kind: RequestKind::Http,
        }))
    }
}
