# SP1 Plan 4: Infrastructure Layer

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement all repository traits and the HTTP executor using real I/O — filesystem for storage, reqwest for HTTP execution, notify for file watching. This is where the DDD meets the real world.

**Architecture:** Each domain trait gets a concrete `Fs*` implementation in rocket-infra. All I/O is isolated here — domain crates remain pure.

**Tech Stack:** Rust, std::fs, serde_json, reqwest, notify, dirs, tempfile (for tests)

---

## File Structure

```
crates/rocket-infra/src/
  lib.rs
  fs_collection_repo.rs       # implements CollectionRepository
  fs_environment_repo.rs       # implements EnvironmentRepository
  fs_history_repo.rs           # implements HistoryRepository
  fs_template_repo.rs          # implements TemplateRepository
  fs_cookie_repo.rs            # implements CookieRepository
  reqwest_executor.rs          # implements HttpExecutor
  file_watcher.rs              # NotifyFileWatcher (publishes events)
  tauri_event_bus.rs           # implements EventPublisher → Tauri emit
```

---

## Chunk 1: Filesystem collection repository

### Task 1: FsCollectionRepo

**Files:**
- Create: `crates/rocket-infra/src/fs_collection_repo.rs`
- Test: integration tests with tempdir

- [ ] **Step 1: Add tempfile dev-dependency**

In `crates/rocket-infra/Cargo.toml`, add:
```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::{Request, CollectionRepository};
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
        let result = repo.list().unwrap();
        assert!(result.is_empty());
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
        let result = repo.create("my-api");
        assert!(result.is_err());
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
        let req = Request::new("Get Users", HttpMethod::Get, "https://api.example.com/users");
        repo.save_request("my-api", "get-users.json", &req).unwrap();
        let loaded = repo.get_request("my-api", "get-users.json").unwrap();
        assert_eq!(loaded.name, "Get Users");
        assert_eq!(loaded.method, HttpMethod::Get);
    }

    #[test]
    fn save_request_in_subfolder() {
        let (_dir, repo) = setup();
        repo.create("my-api").unwrap();
        let req = Request::new("Login", HttpMethod::Post, "/login");
        repo.save_request("my-api", "auth/login.json", &req).unwrap();
        let loaded = repo.get_request("my-api", "auth/login.json").unwrap();
        assert_eq!(loaded.name, "Login");
    }

    #[test]
    fn delete_request() {
        let (_dir, repo) = setup();
        repo.create("my-api").unwrap();
        let req = Request::new("Test", HttpMethod::Get, "/test");
        repo.save_request("my-api", "test.json", &req).unwrap();
        repo.delete_request("my-api", "test.json").unwrap();
        assert!(repo.get_request("my-api", "test.json").is_err());
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
        let req = Request::new("Test", HttpMethod::Get, "/test");
        repo.save_request("my-api", "old/test.json", &req).unwrap();
        repo.move_item("my-api", "old/test.json", "my-api", "new/test.json").unwrap();
        assert!(repo.get_request("my-api", "old/test.json").is_err());
        assert!(repo.get_request("my-api", "new/test.json").is_ok());
    }
}
```

- [ ] **Step 3: Implement FsCollectionRepo**

`crates/rocket-infra/src/fs_collection_repo.rs`:
```rust
use std::fs;
use std::path::{Path, PathBuf};

use rocket_collection::*;
use rocket_shared::error::{DomainError, DomainResult};

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
                if name.starts_with('.') { continue; }
                let count = count_request_files(&path);
                result.push(CollectionSummary::new(
                    &name,
                    path.to_string_lossy().to_string(),
                    count,
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
        let root = build_folder_tree(&path, &path)?;
        Ok(Collection { name: name.to_string(), root })
    }

    fn create(&self, name: &str) -> DomainResult<Collection> {
        Collection::validate_name(name)?;
        let path = self.collection_path(name);
        if path.exists() {
            return Err(DomainError::AlreadyExists(format!("Collection '{}'", name)));
        }
        fs::create_dir_all(&path)?;
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

    fn get_request(&self, collection: &str, path: &str) -> DomainResult<Request> {
        let file_path = self.collection_path(collection).join(path);
        if !file_path.exists() {
            return Err(DomainError::NotFound(format!("{}/{}", collection, path)));
        }
        let content = fs::read_to_string(&file_path)?;
        Ok(serde_json::from_str(&content)?)
    }

    fn save_request(&self, collection: &str, path: &str, request: &Request) -> DomainResult<()> {
        let file_path = self.collection_path(collection).join(path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(request)?;
        fs::write(&file_path, json)?;
        Ok(())
    }

    fn delete_request(&self, collection: &str, path: &str) -> DomainResult<()> {
        let file_path = self.collection_path(collection).join(path);
        if !file_path.exists() {
            return Err(DomainError::NotFound(format!("{}/{}", collection, path)));
        }
        fs::remove_file(&file_path)?;
        Ok(())
    }

    fn create_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
        let dir_path = self.collection_path(collection).join(path);
        fs::create_dir_all(&dir_path)?;
        Ok(())
    }

    fn delete_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
        let dir_path = self.collection_path(collection).join(path);
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
        let src = self.collection_path(src_collection).join(src_path);
        let dst = self.collection_path(dst_collection).join(dst_path);
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
    path.extension()
        .map_or(false, |ext| ext == "json" || ext == "bru")
}

fn build_folder_tree(base: &Path, current: &Path) -> DomainResult<Folder> {
    let name = current
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut folder = Folder::new(name);

    if !current.exists() {
        return Ok(folder);
    }

    let mut entries: Vec<_> = fs::read_dir(current)?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let entry_name = entry.file_name().to_string_lossy().to_string();

        if entry_name.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            let subfolder = build_folder_tree(base, &path)?;
            folder.add_subfolder(subfolder);
        } else if is_request_file(&path) {
            let content = fs::read_to_string(&path)?;
            if let Ok(request) = serde_json::from_str::<Request>(&content) {
                folder.add_request(request);
            }
        }
    }

    Ok(folder)
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let req = Request::new("Get Users", HttpMethod::Get, "https://api.example.com/users");
        repo.save_request("my-api", "get-users.json", &req).unwrap();
        let loaded = repo.get_request("my-api", "get-users.json").unwrap();
        assert_eq!(loaded.name, "Get Users");
    }

    #[test]
    fn save_request_in_subfolder() {
        let (_dir, repo) = setup();
        repo.create("my-api").unwrap();
        let req = Request::new("Login", HttpMethod::Post, "/login");
        repo.save_request("my-api", "auth/login.json", &req).unwrap();
        let loaded = repo.get_request("my-api", "auth/login.json").unwrap();
        assert_eq!(loaded.name, "Login");
    }

    #[test]
    fn delete_request() {
        let (_dir, repo) = setup();
        repo.create("my-api").unwrap();
        let req = Request::new("Test", HttpMethod::Get, "/test");
        repo.save_request("my-api", "test.json", &req).unwrap();
        repo.delete_request("my-api", "test.json").unwrap();
        assert!(repo.get_request("my-api", "test.json").is_err());
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
        let req = Request::new("Test", HttpMethod::Get, "/test");
        repo.save_request("my-api", "old/test.json", &req).unwrap();
        repo.move_item("my-api", "old/test.json", "my-api", "new/test.json").unwrap();
        assert!(repo.get_request("my-api", "old/test.json").is_err());
        assert!(repo.get_request("my-api", "new/test.json").is_ok());
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p rocket-infra -- fs_collection_repo::tests
```
Expected: PASS — 10 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-infra/
git commit -m "feat(infra): FsCollectionRepo — filesystem collection repository"
```

---

## Chunk 2: Remaining filesystem repositories

### Task 2: FsEnvironmentRepo + FsHistoryRepo + FsTemplateRepo + FsCookieRepo

These follow the exact same pattern as FsCollectionRepo — JSON files in directories. I'll give the structure; each is a straightforward read/write-JSON-to-disk implementation.

**Files:**
- Create: `crates/rocket-infra/src/fs_environment_repo.rs`
- Create: `crates/rocket-infra/src/fs_history_repo.rs`
- Create: `crates/rocket-infra/src/fs_template_repo.rs`
- Create: `crates/rocket-infra/src/fs_cookie_repo.rs`

- [ ] **Step 1: Implement FsEnvironmentRepo**

Pattern: `environments/{name}.json` → read/write `Environment` structs.

Each method: `list()` reads all `.json` files in the dir; `get(name)` reads one file; `save(env)` writes to `{name}.json`; `delete(name)` removes the file.

Tests needed (5): list_empty, save_and_list, save_and_get, update_existing, delete.

- [ ] **Step 2: Implement FsHistoryRepo**

Pattern: `history/{id}.json` → read/write `HistoryEntry` structs.

`list(limit)` reads all, sorts by timestamp desc, truncates. `clear()` removes all files.

Tests needed (4): save_and_list, list_with_limit, get_by_id, clear.

- [ ] **Step 3: Implement FsTemplateRepo**

Pattern: `templates/{name}.json` → read/write `Template` structs.

Tests needed (3): save_and_list, get_by_name, delete.

- [ ] **Step 4: Implement FsCookieRepo**

Pattern: `cookies/{sanitized_domain}.json` → read/write `CookieJar` structs.

Tests needed (3): save_and_get_all, get_by_domain, clear.

- [ ] **Step 5: Run all infra tests**

```bash
cargo test -p rocket-infra
```
Expected: PASS — ~25 tests total.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-infra/src/
git commit -m "feat(infra): filesystem repos for environments, history, templates, cookies"
```

---

## Chunk 3: HTTP executor + file watcher

### Task 3: ReqwestExecutor

**Files:**
- Create: `crates/rocket-infra/src/reqwest_executor.rs`

- [ ] **Step 1: Implement ReqwestExecutor**

This implements `HttpExecutor` trait using the `reqwest` crate. Converts domain `HttpRequest` → reqwest builder → execute → domain `HttpResponse`.

Key logic:
- Map `HttpMethod` enum → `reqwest::Method`
- Add enabled headers to `HeaderMap`
- Apply auth (basic_auth, bearer_auth, or header/query for api-key)
- Apply body based on mode (json, xml, text, formdata)
- Apply options (timeout, SSL verify, redirect policy)
- Measure duration with `Instant::now()`
- Collect response headers, body bytes, status

Note: Integration tests for this require network access. Add a unit test that validates the request building logic, and mark the network test with `#[ignore]`.

- [ ] **Step 2: Commit**

```bash
git add crates/rocket-infra/src/reqwest_executor.rs
git commit -m "feat(infra): ReqwestExecutor — HTTP execution via reqwest"
```

---

### Task 4: File watcher

**Files:**
- Create: `crates/rocket-infra/src/file_watcher.rs`

- [ ] **Step 1: Implement NotifyFileWatcher**

Uses `notify` crate to watch the collections directory recursively. On file change events, publishes `DomainEvent::FileChanged` via an `EventPublisher`.

```rust
pub struct NotifyFileWatcher {
    watcher: Mutex<Option<RecommendedWatcher>>,
}

impl NotifyFileWatcher {
    pub fn new() -> Self { ... }

    pub fn start(
        &self,
        collections_dir: PathBuf,
        publisher: Arc<dyn EventPublisher>,
    ) -> Result<(), String> { ... }

    pub fn stop(&self) { ... }
}
```

- [ ] **Step 2: Commit**

```bash
git add crates/rocket-infra/src/file_watcher.rs
git commit -m "feat(infra): NotifyFileWatcher — filesystem change detection"
```

---

### Task 5: Wire up infra lib.rs

**Files:**
- Modify: `crates/rocket-infra/src/lib.rs`

- [ ] **Step 1: Export all modules**

```rust
pub mod fs_collection_repo;
pub mod fs_environment_repo;
pub mod fs_history_repo;
pub mod fs_template_repo;
pub mod fs_cookie_repo;
pub mod reqwest_executor;
pub mod file_watcher;

pub use fs_collection_repo::FsCollectionRepo;
pub use fs_environment_repo::FsEnvironmentRepo;
pub use fs_history_repo::FsHistoryRepo;
pub use fs_template_repo::FsTemplateRepo;
pub use fs_cookie_repo::FsCookieRepo;
pub use reqwest_executor::ReqwestExecutor;
pub use file_watcher::NotifyFileWatcher;
```

- [ ] **Step 2: Full workspace test**

```bash
cargo test --workspace
cargo clippy --workspace
```

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-infra/src/lib.rs
git commit -m "feat(infra): wire up all module exports"
```

---

## Milestone Checklist — Plan 4

- [ ] FsCollectionRepo: 10 integration tests with tempdir
- [ ] FsEnvironmentRepo: 5 tests
- [ ] FsHistoryRepo: 4 tests
- [ ] FsTemplateRepo: 3 tests
- [ ] FsCookieRepo: 3 tests
- [ ] ReqwestExecutor: compiles, unit test for request building
- [ ] NotifyFileWatcher: compiles, publishes events
- [ ] Full workspace: `cargo test --workspace` — all pass
- [ ] Full workspace: `cargo clippy --workspace` — no warnings
