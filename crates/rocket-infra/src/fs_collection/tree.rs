use std::collections::HashMap;
use std::fs;
use std::path::Path;

use rocket_collection::{CollectionItem, Folder, RequestKind, RequestSummary, ScriptFileItem};
use rocket_shared::error::{DomainError, DomainResult};

use crate::conversions::{oc_http_request_to_request, oc_item_to_collection_item};
use crate::oc::{OcHttpRequest, OcItem};

use super::folder_file::parse_folder_yml;
use super::paths::{is_request_file, read_uid_from_yaml};

/// With `strict`, a request file that cannot be parsed is an error and not a skipped item.
pub(super) fn build_folder_tree(current: &Path, strict: bool) -> DomainResult<Folder> {
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
            Ok(Some(CollectionItem::Grpc(mut grpc))) => {
                grpc.file_name = Some(entry_name.to_string());
                if grpc.uid.is_empty() {
                    grpc.uid = crate::conversions::derived_grpc_uid(entry_name);
                }
                Ok(Some(CollectionItem::Grpc(grpc)))
            }
            Ok(other) => Ok(other),
            Err(e) => {
                if strict {
                    return Err(DomainError::Internal(format!(
                        "request file '{entry_name}' cannot be parsed"
                    )));
                }
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
pub(super) fn build_tree<F>(current: &Path, load_item: &mut F) -> DomainResult<Folder>
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

    // Read folder.yml once for both the uid and the display name. The collection root has no
    // folder.yml, and a missing, unreadable, unparseable or uid-less file all fall back to
    // read_uid_from_yaml, which reads opencollection.yml and the legacy .uid file.
    if let Ok(content) = fs::read_to_string(current.join("folder.yml")) {
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
    folder.dir_name = Some(dir_name);

    let read_dir = match fs::read_dir(current) {
        Ok(read_dir) => read_dir,
        // A folder that is gone is the empty folder. Any other listing failure is still an error.
        Err(_) if !current.exists() => return Ok(folder),
        Err(e) => return Err(e.into()),
    };
    let mut entries: Vec<_> = read_dir.filter_map(|e| e.ok()).collect();
    // Apply explicit order from _order.yml (or _order.json for backward compat).
    sort_entries(&mut entries, read_order_list(current));

    // Only the collection root holds Rocket's flow files, so a nested `flows` folder stays visible.
    // The probe is a real stat, so a case-insensitive filesystem behaves as before, and its result
    // matters only for an entry named `flows`, so it runs only then.
    let is_collection_root = || current.join("opencollection.yml").exists();
    for entry in entries {
        let path = entry.path();
        let entry_name = entry.file_name().to_string_lossy().to_string();
        if is_hidden_entry(&entry_name, entry_name == "flows" && is_collection_root()) {
            continue;
        }
        // `file_type()` does not follow links and costs no stat on Linux and macOS. A symlink or a
        // failed lookup falls back to `path.is_dir()`, so a link to a directory is still found
        // whatever its name.
        let file_type = entry.file_type().ok();
        let is_symlink = file_type.map(|t| t.is_symlink()).unwrap_or(false);
        let is_dir = match file_type {
            Some(t) if !t.is_symlink() => t.is_dir(),
            _ => path.is_dir(),
        };
        if is_dir {
            // Skip symlinked directories to prevent exfiltration.
            if is_symlink {
                tracing::warn!(path = %path.display(), "skipping symlinked directory in folder tree");
                continue;
            }
            folder.add_subfolder(build_tree(&path, load_item)?);
        } else if is_script_file(&path) {
            // Symlinked scripts are skipped, as symlinked directories are above.
            // A failed lookup counts as a symlink, as before.
            if file_type.map(|t| t.is_symlink()).unwrap_or(true) {
                tracing::warn!(path = %path.display(), "skipping symlinked script file");
                continue;
            }
            folder
                .items
                .push(CollectionItem::ScriptFile(ScriptFileItem::new(&entry_name)));
        } else if is_request_file(&path) {
            if let Some(item) = load_item(&path, &entry_name)? {
                folder.items.push(item);
            }
        }
    }

    Ok(folder)
}

/// Reads the explicit order from `_order.yml`, or from `_order.json` when the YAML file is absent.
/// The content is read as a YAML list first and then as a JSON list. `None` means no usable order:
/// an unreadable `_order.yml`, or one that is a directory, does not fall through to the JSON file.
pub(super) fn read_order_list(dir: &Path) -> Option<Vec<String>> {
    let content = match fs::read_to_string(dir.join("_order.yml")) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fs::read_to_string(dir.join("_order.json")).ok()?
        }
        Err(_) => return None,
    };
    serde_yaml::from_str::<Vec<String>>(&content)
        .ok()
        .or_else(|| serde_json::from_str::<Vec<String>>(&content).ok())
}

/// Sorts by position in `order`, then by file name. A duplicate keeps its last position, and unknown
/// or non-UTF-8 names come last. Without an order the entries sort by file name.
fn sort_entries(entries: &mut [fs::DirEntry], order: Option<Vec<String>>) {
    let Some(ordered) = order else {
        entries.sort_by_key(|e| e.file_name());
        return;
    };
    let pos: HashMap<String, usize> = ordered
        .into_iter()
        .enumerate()
        .map(|(i, name)| (name, i))
        .collect();
    let rank = |e: &fs::DirEntry| {
        e.file_name()
            .to_str()
            .and_then(|n| pos.get(n))
            .copied()
            .unwrap_or(usize::MAX)
    };
    entries.sort_by(|a, b| {
        rank(a)
            .cmp(&rank(b))
            .then_with(|| a.file_name().cmp(&b.file_name()))
    });
}

/// True for names the tree never shows. Script file operations use it too, so a
/// script is never created or opened where the tree would not list it.
pub(super) fn is_hidden_entry(name: &str, at_collection_root: bool) -> bool {
    name.starts_with('.')
        || name == "environments"
        || name == "node_modules"
        || (at_collection_root && name == "flows")
}

/// True for a `.js` file. Dot-files never reach here because the skip above drops them.
fn is_script_file(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "js")
}

/// Parse only the uid/name/method/url fields from a request file for sidebar display.
/// GraphQL, WebSocket and gRPC files return a summary with their `kind`. ScriptFile
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
            // Read leniently: a broken `examples` block must never hide the request.
            #[serde(default)]
            examples: Option<serde_yaml::Value>,
        }
        /// Takes the string `name` of each mapping in a sequence. Any other shape gives none.
        fn example_names(value: Option<serde_yaml::Value>) -> Vec<String> {
            let Some(serde_yaml::Value::Sequence(items)) = value else {
                return Vec::new();
            };
            items
                .iter()
                .filter_map(|item| item.get("name")?.as_str().map(str::to_string))
                .collect()
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
                    example_names: example_names(min.examples),
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
                example_names: Vec::new(),
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
                example_names: Vec::new(),
            })),
            // A gRPC file is small, so the summary reads the few fields the sidebar needs.
            Ok(OcItem::Grpc(grpc)) => Ok(Some(RequestSummary {
                uid: grpc
                    .uid
                    .filter(|u| !u.is_empty())
                    .unwrap_or_else(|| crate::conversions::derived_grpc_uid(entry_name)),
                name: grpc.info.name,
                method: "GRPC".to_string(),
                url: grpc.grpc.url,
                file_name: Some(entry_name.to_string()),
                kind: RequestKind::Grpc,
                example_names: Vec::new(),
            })),
            Ok(OcItem::Http(_)) | Ok(OcItem::Folder(_)) | Err(_) => Err(DomainError::Internal(
                format!("Failed to parse request summary: {min_err}"),
            )),
            Ok(OcItem::ScriptFile(_)) => Ok(None),
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
            example_names: Vec::new(),
        }))
    }
}
