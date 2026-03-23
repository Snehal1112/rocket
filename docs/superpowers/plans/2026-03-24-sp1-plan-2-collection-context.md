# SP1 Plan 2: Collection Bounded Context

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Collection bounded context — aggregate root, value objects (Request, Folder, CollectionItem), and the CollectionRepository trait. Pure domain logic, no I/O.

**Architecture:** Collection is the aggregate root owning a recursive tree of folders and requests. All mutations go through the aggregate. Repository trait defines persistence contract — implemented later in rocket-infra.

**Tech Stack:** Rust, serde, rocket-shared

---

## File Structure

```
crates/rocket-collection/src/
  lib.rs                  # Module exports
  collection.rs           # Collection aggregate root
  request.rs              # Request value object
  folder.rs               # Folder value object + CollectionItem enum
  repository.rs           # CollectionRepository trait
  summary.rs              # CollectionSummary (lightweight list item)
```

---

## Chunk 1: Collection domain model

### Task 1: Request value object

**Files:**
- Create: `crates/rocket-collection/src/request.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write the failing test**

Add to `crates/rocket-collection/src/request.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::types::{Auth, Body, BodyMode, Header, HttpMethod};

    #[test]
    fn new_request_has_defaults() {
        let req = Request::new("Get Users", HttpMethod::Get, "https://api.example.com/users");
        assert_eq!(req.name, "Get Users");
        assert_eq!(req.method, HttpMethod::Get);
        assert_eq!(req.url, "https://api.example.com/users");
        assert!(req.headers.is_empty());
        assert!(req.body.is_none());
        assert_eq!(req.auth, Auth::None);
    }

    #[test]
    fn request_with_headers() {
        let req = Request::new("Test", HttpMethod::Post, "https://api.example.com")
            .with_header("Content-Type", "application/json")
            .with_header("Authorization", "Bearer token");
        assert_eq!(req.headers.len(), 2);
        assert!(req.headers[0].enabled);
    }

    #[test]
    fn request_serialization_roundtrip() {
        let req = Request::new("Test", HttpMethod::Post, "https://api.example.com")
            .with_body(Body {
                mode: BodyMode::Json,
                content: Some("{\"key\":\"val\"}".into()),
                form_data: None,
            });
        let json = serde_json::to_string_pretty(&req).unwrap();
        let deserialized: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(req, deserialized);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p rocket-collection -- request::tests
```
Expected: FAIL — `Request` not defined.

- [ ] **Step 3: Implement Request value object**

`crates/rocket-collection/src/request.rs`:
```rust
use rocket_shared::types::{Auth, Body, Header, HttpMethod};
use serde::{Deserialize, Serialize};

/// A saved API request definition.
/// Value object — immutable identity, compared by value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub name: String,
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<Header>,
    pub body: Option<Body>,
    #[serde(default)]
    pub auth: Auth,
}

impl Request {
    pub fn new(
        name: impl Into<String>,
        method: HttpMethod,
        url: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            method,
            url: url.into(),
            headers: Vec::new(),
            body: None,
            auth: Auth::None,
        }
    }

    /// Builder method: add an enabled header.
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push(Header::new(key, value));
        self
    }

    /// Builder method: set body.
    pub fn with_body(mut self, body: Body) -> Self {
        self.body = Some(body);
        self
    }

    /// Builder method: set auth.
    pub fn with_auth(mut self, auth: Auth) -> Self {
        self.auth = auth;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::types::{Body, BodyMode};

    #[test]
    fn new_request_has_defaults() {
        let req = Request::new("Get Users", HttpMethod::Get, "https://api.example.com/users");
        assert_eq!(req.name, "Get Users");
        assert_eq!(req.method, HttpMethod::Get);
        assert_eq!(req.url, "https://api.example.com/users");
        assert!(req.headers.is_empty());
        assert!(req.body.is_none());
        assert_eq!(req.auth, Auth::None);
    }

    #[test]
    fn request_with_headers() {
        let req = Request::new("Test", HttpMethod::Post, "https://api.example.com")
            .with_header("Content-Type", "application/json")
            .with_header("Authorization", "Bearer token");
        assert_eq!(req.headers.len(), 2);
        assert!(req.headers[0].enabled);
    }

    #[test]
    fn request_serialization_roundtrip() {
        let req = Request::new("Test", HttpMethod::Post, "https://api.example.com")
            .with_body(Body {
                mode: BodyMode::Json,
                content: Some("{\"key\":\"val\"}".into()),
                form_data: None,
            });
        let json = serde_json::to_string_pretty(&req).unwrap();
        let deserialized: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(req, deserialized);
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p rocket-collection -- request::tests
```
Expected: PASS — 3 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-collection/src/request.rs
git commit -m "feat(collection): Request value object"
```

---

### Task 2: Folder value object and CollectionItem enum

**Files:**
- Create: `crates/rocket-collection/src/folder.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write the failing test**

```rust
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
        folder.add_request(req.clone());
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
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p rocket-collection -- folder::tests
```
Expected: FAIL — `Folder`, `CollectionItem` not defined.

- [ ] **Step 3: Implement Folder and CollectionItem**

`crates/rocket-collection/src/folder.rs`:
```rust
use crate::request::Request;
use serde::{Deserialize, Serialize};

/// A recursive tree node: either a Request or a nested Folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "itemType", rename_all = "camelCase")]
pub enum CollectionItem {
    #[serde(rename = "request")]
    Request(Request),
    #[serde(rename = "folder")]
    Folder(Folder),
}

/// A folder containing requests and sub-folders.
/// Value object — identity is its path within the collection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub name: String,
    pub items: Vec<CollectionItem>,
}

impl Folder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
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
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p rocket-collection -- folder::tests
```
Expected: PASS — 4 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-collection/src/folder.rs
git commit -m "feat(collection): Folder value object + CollectionItem enum"
```

---

### Task 3: Collection aggregate root

**Files:**
- Create: `crates/rocket-collection/src/collection.rs`
- Create: `crates/rocket-collection/src/summary.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write the failing test for CollectionSummary**

`crates/rocket-collection/src/summary.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_creation() {
        let s = CollectionSummary::new("my-api", "/path/to/my-api", 5);
        assert_eq!(s.name, "my-api");
        assert_eq!(s.request_count, 5);
    }
}
```

- [ ] **Step 2: Implement CollectionSummary**

`crates/rocket-collection/src/summary.rs`:
```rust
use serde::{Deserialize, Serialize};

/// Lightweight summary for listing collections (no full tree).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSummary {
    pub name: String,
    pub path: String,
    pub request_count: usize,
}

impl CollectionSummary {
    pub fn new(name: impl Into<String>, path: impl Into<String>, request_count: usize) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            request_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_creation() {
        let s = CollectionSummary::new("my-api", "/path/to/my-api", 5);
        assert_eq!(s.name, "my-api");
        assert_eq!(s.request_count, 5);
    }
}
```

- [ ] **Step 3: Run test**

```bash
cargo test -p rocket-collection -- summary::tests
```
Expected: PASS.

- [ ] **Step 4: Write the failing tests for Collection aggregate**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::Request;
    use rocket_shared::types::HttpMethod;

    #[test]
    fn new_collection_is_empty() {
        let col = Collection::new("my-api");
        assert_eq!(col.name, "my-api");
        assert_eq!(col.root.request_count(), 0);
    }

    #[test]
    fn rename_collection() {
        let mut col = Collection::new("old-name");
        col.rename("new-name").unwrap();
        assert_eq!(col.name, "new-name");
    }

    #[test]
    fn rename_to_empty_fails() {
        let mut col = Collection::new("test");
        let result = col.rename("");
        assert!(result.is_err());
    }

    #[test]
    fn validate_name_rejects_invalid_chars() {
        assert!(Collection::validate_name("valid-name").is_ok());
        assert!(Collection::validate_name("also_valid.name").is_ok());
        assert!(Collection::validate_name("").is_err());
        assert!(Collection::validate_name("has/slash").is_err());
        assert!(Collection::validate_name("has\\backslash").is_err());
    }
}
```

- [ ] **Step 5: Implement Collection aggregate**

`crates/rocket-collection/src/collection.rs`:
```rust
use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

use crate::folder::Folder;

/// Collection aggregate root.
/// A collection is a named group of API requests organized in a folder tree.
/// Identity: the collection name (unique within the workspace).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub name: String,
    pub root: Folder,
}

impl Collection {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            root: Folder::new(&name),
            name,
        }
    }

    /// Rename the collection. Validates the new name.
    pub fn rename(&mut self, new_name: impl Into<String>) -> DomainResult<()> {
        let new_name = new_name.into();
        Self::validate_name(&new_name)?;
        self.name = new_name;
        Ok(())
    }

    /// Validate a collection name.
    pub fn validate_name(name: &str) -> DomainResult<()> {
        if name.trim().is_empty() {
            return Err(DomainError::InvalidInput(
                "Collection name cannot be empty".into(),
            ));
        }
        if name.contains('/') || name.contains('\\') {
            return Err(DomainError::InvalidInput(
                "Collection name cannot contain path separators".into(),
            ));
        }
        if name.starts_with('.') {
            return Err(DomainError::InvalidInput(
                "Collection name cannot start with a dot".into(),
            ));
        }
        Ok(())
    }

    /// Total request count across all folders.
    pub fn request_count(&self) -> usize {
        self.root.request_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_collection_is_empty() {
        let col = Collection::new("my-api");
        assert_eq!(col.name, "my-api");
        assert_eq!(col.root.request_count(), 0);
    }

    #[test]
    fn rename_collection() {
        let mut col = Collection::new("old-name");
        col.rename("new-name").unwrap();
        assert_eq!(col.name, "new-name");
    }

    #[test]
    fn rename_to_empty_fails() {
        let mut col = Collection::new("test");
        let result = col.rename("");
        assert!(result.is_err());
    }

    #[test]
    fn validate_name_rejects_invalid_chars() {
        assert!(Collection::validate_name("valid-name").is_ok());
        assert!(Collection::validate_name("also_valid.name").is_ok());
        assert!(Collection::validate_name("").is_err());
        assert!(Collection::validate_name("has/slash").is_err());
        assert!(Collection::validate_name("has\\backslash").is_err());
    }
}
```

- [ ] **Step 6: Run tests**

```bash
cargo test -p rocket-collection -- collection::tests
```
Expected: PASS — 4 tests.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-collection/src/collection.rs crates/rocket-collection/src/summary.rs
git commit -m "feat(collection): Collection aggregate root + CollectionSummary"
```

---

### Task 4: CollectionRepository trait

**Files:**
- Create: `crates/rocket-collection/src/repository.rs`

- [ ] **Step 1: Write the trait with object safety check**

`crates/rocket-collection/src/repository.rs`:
```rust
use rocket_shared::error::DomainResult;

use crate::collection::Collection;
use crate::folder::{CollectionItem, Folder};
use crate::request::Request;
use crate::summary::CollectionSummary;

/// Repository trait for Collection persistence.
/// Implemented by FsCollectionRepo in rocket-infra.
pub trait CollectionRepository: Send + Sync {
    /// List all collections (lightweight summaries).
    fn list(&self) -> DomainResult<Vec<CollectionSummary>>;

    /// Get full collection tree by name.
    fn get(&self, name: &str) -> DomainResult<Collection>;

    /// Create a new empty collection.
    fn create(&self, name: &str) -> DomainResult<Collection>;

    /// Delete a collection and all its contents.
    fn delete(&self, name: &str) -> DomainResult<()>;

    /// Rename a collection.
    fn rename(&self, old_name: &str, new_name: &str) -> DomainResult<()>;

    /// Read a single request file by collection name and relative path.
    fn get_request(&self, collection: &str, path: &str) -> DomainResult<Request>;

    /// Save a request to a specific path within a collection.
    fn save_request(&self, collection: &str, path: &str, request: &Request) -> DomainResult<()>;

    /// Delete a request file.
    fn delete_request(&self, collection: &str, path: &str) -> DomainResult<()>;

    /// Create a folder within a collection.
    fn create_folder(&self, collection: &str, path: &str) -> DomainResult<()>;

    /// Delete a folder and its contents.
    fn delete_folder(&self, collection: &str, path: &str) -> DomainResult<()>;

    /// Move a request or folder within or across collections.
    fn move_item(
        &self,
        src_collection: &str,
        src_path: &str,
        dst_collection: &str,
        dst_path: &str,
    ) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        // Compile-time check
        fn _assert_object_safe(_: Box<dyn CollectionRepository>) {}
    }
}
```

- [ ] **Step 2: Run test**

```bash
cargo test -p rocket-collection -- repository::tests
```
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-collection/src/repository.rs
git commit -m "feat(collection): CollectionRepository trait"
```

---

### Task 5: Wire up collection crate lib.rs

**Files:**
- Modify: `crates/rocket-collection/src/lib.rs`

- [ ] **Step 1: Update lib.rs to export all modules**

`crates/rocket-collection/src/lib.rs`:
```rust
pub mod collection;
pub mod folder;
pub mod repository;
pub mod request;
pub mod summary;

// Re-export key types at crate root for convenience
pub use collection::Collection;
pub use folder::{CollectionItem, Folder};
pub use repository::CollectionRepository;
pub use request::Request;
pub use summary::CollectionSummary;
```

- [ ] **Step 2: Run all collection tests**

```bash
cargo test -p rocket-collection
```
Expected: PASS — all 12 tests (3 request + 4 folder + 4 collection + 1 repository).

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-collection/src/lib.rs
git commit -m "feat(collection): wire up module exports"
```

---

## Milestone Checklist — Plan 2

- [ ] `rocket-collection` crate compiles cleanly
- [ ] Request VO: creation, builder pattern, serde roundtrip (3 tests)
- [ ] Folder VO: creation, mixed items, find, recursive count (4 tests)
- [ ] Collection aggregate: creation, rename, validation (4 tests)
- [ ] CollectionRepository trait: object safe (1 test)
- [ ] All 12 tests pass: `cargo test -p rocket-collection`
- [ ] No clippy warnings: `cargo clippy -p rocket-collection`
