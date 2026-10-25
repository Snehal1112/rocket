//! Where an operation goes in the collection tree.

use crate::openapi::Operation;

/// How an import groups requests into folders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupBy {
    /// One folder per tag, using the operation's first tag. Untagged operations go in `untagged`.
    #[default]
    Tags,
    /// Folders follow the URL path up to its first `{param}` segment.
    Paths,
}

impl GroupBy {
    /// The name stored in the source file.
    pub fn as_str(&self) -> &'static str {
        match self {
            GroupBy::Tags => "tags",
            GroupBy::Paths => "paths",
        }
    }

    /// Reads the stored name back. Only the exact names `tags` and `paths` are accepted.
    pub fn parse(text: &str) -> Option<GroupBy> {
        match text {
            "tags" => Some(GroupBy::Tags),
            "paths" => Some(GroupBy::Paths),
            _ => None,
        }
    }
}

/// Folder name used for operations without a tag.
pub(crate) const UNTAGGED_FOLDER: &str = "untagged";

/// The raw folder names, outermost first, an operation belongs under. Names are not yet
/// safe for the file system; the importer turns each one into a unique, safe segment.
/// An empty list means the collection root.
pub(crate) fn folder_names(op: &Operation, group_by: GroupBy) -> Vec<String> {
    match group_by {
        GroupBy::Tags => vec![op
            .tags
            .first()
            .filter(|t| !t.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| UNTAGGED_FOLDER.to_string())],
        GroupBy::Paths => op
            .path
            .split('/')
            .filter(|s| !s.is_empty())
            .take_while(|s| !s.contains('{'))
            .map(str::to_string)
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::types::HttpMethod;

    fn op(path: &str, tags: &[&str]) -> Operation {
        Operation {
            key: path.to_string(),
            method: HttpMethod::Get,
            path: path.to_string(),
            summary: None,
            operation_id: None,
            description: None,
            tags: tags.iter().map(|t| t.to_string()).collect(),
            parameters: Vec::new(),
            request_body: None,
            security: Vec::new(),
            deprecated: false,
        }
    }

    #[test]
    fn tags_use_the_first_tag_or_untagged() {
        assert_eq!(
            folder_names(&op("/a", &["Orders", "Admin"]), GroupBy::Tags),
            vec!["Orders"]
        );
        assert_eq!(
            folder_names(&op("/a", &[]), GroupBy::Tags),
            vec!["untagged"]
        );
        assert_eq!(
            folder_names(&op("/a", &["  "]), GroupBy::Tags),
            vec!["untagged"]
        );
    }

    #[test]
    fn paths_stop_at_the_first_param_segment() {
        assert_eq!(
            folder_names(&op("/api/v1/users/{id}/orders", &[]), GroupBy::Paths),
            vec!["api", "v1", "users"]
        );
        assert_eq!(
            folder_names(&op("/api/v1/users", &[]), GroupBy::Paths),
            vec!["api", "v1", "users"]
        );
        assert!(folder_names(&op("/{id}", &[]), GroupBy::Paths).is_empty());
        assert!(folder_names(&op("/", &[]), GroupBy::Paths).is_empty());
    }
}
