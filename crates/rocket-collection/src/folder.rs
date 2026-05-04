use crate::request::Request;
use serde::{Deserialize, Serialize};

/// An opaque protocol item stored as raw YAML for lossless roundtrip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpaqueProtocolItem {
    /// The protocol type: "graphql", "grpc", "websocket".
    pub protocol: String,
    /// The display name (from info.name).
    pub name: String,
    /// The raw YAML value, preserved for lossless roundtrip.
    pub raw: serde_yaml::Value,
}

/// A recursive tree node: either a Request, a nested Folder, or an opaque protocol item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CollectionItem {
    #[serde(rename = "request")]
    Request(Request),
    #[serde(rename = "folder")]
    Folder(Folder),
    /// Raw YAML for non-HTTP protocols (GraphQL, gRPC, WebSocket).
    #[serde(rename = "opaque")]
    OpaqueItem(OpaqueProtocolItem),
}

/// A folder containing requests and sub-folders.
/// Value object — identity is its path within the collection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    #[serde(default = "crate::generate_uid")]
    pub uid: String,
    pub name: String,
    /// Actual on-disk directory name. May differ from `name` when `folder.yml`
    /// contains a display name that doesn't match the directory. The frontend
    /// uses this to build paths for `move_item` calls. Not persisted to disk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir_name: Option<String>,
    pub items: Vec<CollectionItem>,
}

impl Folder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            uid: crate::generate_uid(),
            name: name.into(),
            dir_name: None,
            items: Vec::new(),
        }
    }

    pub fn add_request(&mut self, request: Request) {
        self.items.push(CollectionItem::Request(request));
    }

    pub fn add_subfolder(&mut self, folder: Folder) {
        self.items.push(CollectionItem::Folder(folder));
    }

    /// Find a request by name (non-recursive, current level only).
    pub fn find_request(&self, name: &str) -> Option<&Request> {
        self.items.iter().find_map(|item| match item {
            CollectionItem::Request(r) if r.name == name => Some(r),
            _ => None,
        })
    }

    /// Find a subfolder by name (non-recursive, current level only).
    pub fn find_folder(&self, name: &str) -> Option<&Folder> {
        self.items.iter().find_map(|item| match item {
            CollectionItem::Folder(f) if f.name == name => Some(f),
            _ => None,
        })
    }

    /// Count all requests recursively.
    pub fn request_count(&self) -> usize {
        self.items.iter().map(|item| match item {
            CollectionItem::Request(_) => 1,
            CollectionItem::Folder(f) => f.request_count(),
            CollectionItem::OpaqueItem(_) => 0,
        }).sum()
    }

    /// List all folder names at current level.
    pub fn subfolder_names(&self) -> Vec<&str> {
        self.items.iter().filter_map(|item| match item {
            CollectionItem::Folder(f) => Some(f.name.as_str()),
            _ => None,
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::Request;
    use rocket_shared::types::HttpMethod;

    #[test]
    fn empty_folder() {
        let folder = Folder::new("auth");
        assert_eq!(folder.name, "auth");
        assert!(folder.items.is_empty());
    }

    #[test]
    fn folder_with_mixed_items() {
        let req = Request::new("Login", HttpMethod::Post, "/login");
        let subfolder = Folder::new("admin");

        let mut folder = Folder::new("api");
        folder.add_request(req);
        folder.add_subfolder(subfolder);

        assert_eq!(folder.items.len(), 2);
        assert!(matches!(&folder.items[0], CollectionItem::Request(_)));
        assert!(matches!(&folder.items[1], CollectionItem::Folder(_)));
    }

    #[test]
    fn folder_find_request_by_name() {
        let req = Request::new("Get Users", HttpMethod::Get, "/users");
        let mut folder = Folder::new("root");
        folder.add_request(req);

        let found = folder.find_request("Get Users");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Get Users");

        assert!(folder.find_request("nonexistent").is_none());
    }

    #[test]
    fn folder_count_requests_recursive() {
        let mut inner = Folder::new("inner");
        inner.add_request(Request::new("R1", HttpMethod::Get, "/r1"));
        inner.add_request(Request::new("R2", HttpMethod::Get, "/r2"));

        let mut root = Folder::new("root");
        root.add_request(Request::new("R0", HttpMethod::Get, "/r0"));
        root.add_subfolder(inner);

        assert_eq!(root.request_count(), 3);
    }
}
