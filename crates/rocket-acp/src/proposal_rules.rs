//! Structural path rules for the assistant's proposals: locating a
//! collection-relative path in the summaries tree, normalizing and validating
//! paths and names, and refusing reserved names. Pure functions, no I/O.

use rocket_collection::{CollectionItem, Folder};
use rocket_shared::error::{DomainError, DomainResult};

use crate::proposal::ProposedChange;

/// File names the collection layout reserves, lowercase. A request must never
/// be saved over one of them. Mirrors `rocket-infra` `is_request_file`.
const RESERVED_FILE_NAMES: &[&str] = &[
    "opencollection.yml",
    "opencollection.yaml",
    "folder.yml",
    "folder.yaml",
    "workspace.yml",
    "workspace.yaml",
    "collection.json",
    "_order.json",
    "_order.yml",
    "_order.yaml",
];

/// Directory names the tree never shows, lowercase. Mirrors `rocket-infra`
/// `is_hidden_entry`. `flows` is hidden at the collection root only.
const RESERVED_DIR_NAMES: &[&str] = &["environments", "node_modules"];

/// What a collection-relative path points at in the summaries tree.
pub enum Target {
    /// A folder, with its shape text for the fingerprint.
    Folder(String),
    HttpRequest,
    /// A GraphQL, WebSocket or gRPC request, or a script file.
    Other,
    Missing,
}

/// A folder's direct children, sorted, one per line.
pub fn folder_shape(folder: &Folder) -> String {
    let mut lines: Vec<String> = folder
        .items
        .iter()
        .filter_map(|item| match item {
            CollectionItem::Folder(sub) => Some(format!("folder:{}", sub.dir_or_name())),
            CollectionItem::Summary(summary) => Some(format!(
                "request:{}:{}:{}:{}",
                summary.file_name.as_deref().unwrap_or_default(),
                summary.name,
                summary.method,
                summary.url
            )),
            CollectionItem::ScriptFile(file) => Some(format!("script:{}", file.file_name)),
            _ => None,
        })
        .collect();
    lines.sort();
    lines.join("\n")
}

/// Compares two names. With `fold`, case is ignored, so a case variant
/// counts as the same name on every filesystem.
pub fn same_name(fold: bool, a: &str, b: &str) -> bool {
    if fold {
        rocket_collection::fold_name(a) == rocket_collection::fold_name(b)
    } else {
        a == b
    }
}

pub fn child_folder<'a>(folder: &'a Folder, name: &str, fold: bool) -> Option<&'a Folder> {
    folder.items.iter().find_map(|item| match item {
        CollectionItem::Folder(sub) if same_name(fold, sub.dir_or_name(), name) => Some(sub),
        _ => None,
    })
}

/// Finds what `path` points at. `""` is the collection root.
pub fn locate(root: &Folder, path: &str) -> Target {
    locate_with(root, path, false)
}

pub fn locate_with(root: &Folder, path: &str, fold: bool) -> Target {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let Some((last, parents)) = segments.split_last() else {
        return Target::Folder(folder_shape(root));
    };
    let mut folder = root;
    for segment in parents {
        match child_folder(folder, segment, fold) {
            Some(next) => folder = next,
            None => return Target::Missing,
        }
    }
    if let Some(found) = child_folder(folder, last, fold) {
        return Target::Folder(folder_shape(found));
    }
    for item in &folder.items {
        match item {
            CollectionItem::Summary(summary)
                if summary
                    .file_name
                    .as_deref()
                    .is_some_and(|n| same_name(fold, n, last)) =>
            {
                return if summary.kind.is_http() {
                    Target::HttpRequest
                } else {
                    Target::Other
                };
            }
            CollectionItem::ScriptFile(file) if same_name(fold, &file.file_name, last) => {
                return Target::Other;
            }
            _ => {}
        }
    }
    Target::Missing
}

pub fn is_folder(root: &Folder, path: &str) -> bool {
    matches!(locate(root, path), Target::Folder(_))
}

/// Free in the tree, with names compared case-folded.
pub fn is_free(root: &Folder, path: &str) -> bool {
    matches!(locate_with(root, path, true), Target::Missing)
}

pub fn last_segment(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Refuses names the collection layout reserves or the tree never shows, so
/// an accepted change cannot overwrite settings or land where it is hidden.
pub fn check_reserved_name(parent: &str, name: &str, is_dir: bool) -> DomainResult<()> {
    let lower = rocket_collection::fold_name(name);
    // A directory must not take a reserved file name either: it would sit
    // where the layout expects settings or an order file.
    let reserved_file = RESERVED_FILE_NAMES.contains(&lower.as_str());
    let reserved = if is_dir {
        reserved_file
            || RESERVED_DIR_NAMES.contains(&lower.as_str())
            || (parent.is_empty() && lower == "flows")
    } else {
        reserved_file
    };
    if reserved {
        return Err(DomainError::InvalidInput(format!(
            "'{name}' is a reserved name; choose another"
        )));
    }
    Ok(())
}

/// `parent/name`, or `name` at the root. Both inputs are normalized.
pub fn join_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

/// The folder that holds `path`, `""` for the root.
pub fn parent_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(index) => &path[..index],
        None => "",
    }
}

/// Where `from_path` lands when moved into `to_folder`.
pub fn move_destination(from_path: &str, to_folder: &str) -> DomainResult<String> {
    if from_path.is_empty() {
        return Err(DomainError::InvalidInput(
            "the collection root cannot be moved".to_string(),
        ));
    }
    if to_folder == from_path || to_folder.starts_with(&format!("{from_path}/")) {
        return Err(DomainError::InvalidInput(
            "a folder cannot be moved into itself".to_string(),
        ));
    }
    if parent_of(from_path) == to_folder {
        return Err(DomainError::InvalidInput(
            "the item is already in that folder".to_string(),
        ));
    }
    let name = from_path.rsplit('/').next().unwrap_or(from_path);
    Ok(join_path(to_folder, name))
}

/// Drops empty segments, so `a//b`, `/a/b` and `a/b/` all become `a/b`.
pub fn normalize_path(path: &str) -> String {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

/// Normalizes every path in the change.
pub fn normalize_paths(change: &mut ProposedChange) {
    let trim = |path: &mut String| *path = normalize_path(path);
    match change {
        ProposedChange::CreateFolder { parent_path, .. } => trim(parent_path),
        ProposedChange::CreateRequest { folder_path, .. } => trim(folder_path),
        ProposedChange::UpdateRequest { request_path, .. }
        | ProposedChange::EditScript { request_path, .. } => trim(request_path),
        ProposedChange::MoveItem {
            from_path,
            to_folder,
            ..
        } => {
            trim(from_path);
            trim(to_folder);
        }
        ProposedChange::RenameItem { path, .. } => trim(path),
        ProposedChange::SetEnvVar { .. } => {}
        ProposedChange::UpdateFolderSettings { folder_path, .. } => trim(folder_path),
    }
}

pub fn paths_of(change: &ProposedChange) -> Vec<&str> {
    match change {
        ProposedChange::CreateFolder { parent_path, .. } => vec![parent_path.as_str()],
        ProposedChange::CreateRequest { folder_path, .. } => vec![folder_path.as_str()],
        ProposedChange::UpdateRequest { request_path, .. }
        | ProposedChange::EditScript { request_path, .. } => vec![request_path.as_str()],
        ProposedChange::MoveItem {
            from_path,
            to_folder,
            ..
        } => vec![from_path.as_str(), to_folder.as_str()],
        ProposedChange::RenameItem { path, .. } => vec![path.as_str()],
        ProposedChange::SetEnvVar { .. } => vec![],
        ProposedChange::UpdateFolderSettings { folder_path, .. } => vec![folder_path.as_str()],
    }
}

/// Refuses paths that could leave the collection. The repository checks
/// again, so this only gives the agent a clear message early.
pub fn validate_relative_path(path: &str) -> DomainResult<()> {
    if path.contains(['\\', '\0']) || path.split('/').any(|s| s == ".." || s == ".") {
        return Err(DomainError::InvalidInput(
            "paths must stay inside the collection".to_string(),
        ));
    }
    Ok(())
}

/// A folder or request name must be one path segment.
pub fn validate_item_name(name: &str) -> DomainResult<()> {
    // Windows drops a trailing dot or space and may read a `~` name as a short
    // alias of another entry, so those names can land on an existing item.
    if name.trim().is_empty()
        || name.contains(['/', '\\', '\0', '~'])
        || name.starts_with('.')
        || name.ends_with(['.', ' '])
    {
        return Err(DomainError::InvalidInput(format!(
            "'{name}' is not a valid name"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_that_differ_only_in_unicode_normalization_count_as_the_same() {
        assert!(same_name(true, "caf\u{e9}", "cafe\u{301}"));
        assert!(same_name(true, "CAF\u{c9}", "cafe\u{301}"));
        assert!(!same_name(false, "caf\u{e9}", "cafe\u{301}"));
    }

    #[test]
    fn paths_with_empty_segments_are_normalized() {
        assert_eq!(normalize_path("a//b"), "a/b");
        assert_eq!(normalize_path("/a/b/"), "a/b");
        assert_eq!(normalize_path("///"), "");
    }
}
