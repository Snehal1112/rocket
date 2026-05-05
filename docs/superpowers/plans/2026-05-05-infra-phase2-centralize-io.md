# rocket-infra Phase 2: Centralize I/O Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate ~250 lines of duplicated YAML I/O boilerplate across the four list-repos (`FsEnvironmentRepo`, `FsTemplateRepo`, `FsHistoryRepo`, `FsCookieRepo`) by extracting shared helpers into a new `yaml_io.rs` module, and remove redundant `create_dir_all` calls that `atomic_write` already handles.

**Architecture:** A new private module `crates/rocket-infra/src/yaml_io.rs` provides two helpers: `read_dir_yaml<T>` (read all `.yml` files in a directory, parse each with serde_yaml, return a `Vec<(PathBuf, T)>`) and `delete_if_exists` (remove a file, return `DomainError::NotFound` if absent). Each of the four repos is then rewritten to use these helpers, removing their own inline `read_dir` loops. The `create_dir_all` call in each repo's `save()` method is dropped because `atomic_write` already creates parent directories.

**Tech Stack:** Rust, `std::fs`, `serde::de::DeserializeOwned`, `serde_yaml`, `tempfile` (tests), `rocket-infra` internal only

---

## File Map

| File | Change |
|------|--------|
| `crates/rocket-infra/src/yaml_io.rs` | **Create** — `read_dir_yaml<T>`, `delete_if_exists` |
| `crates/rocket-infra/src/lib.rs` | Add `mod yaml_io;` (private) |
| `crates/rocket-infra/src/fs_environment_repo.rs` | Use `yaml_io::read_dir_yaml`, `yaml_io::delete_if_exists`; drop redundant `create_dir_all` |
| `crates/rocket-infra/src/fs_template_repo.rs` | Same as above |
| `crates/rocket-infra/src/fs_history_repo.rs` | Use `yaml_io::read_dir_yaml` for the mtime-sorted path, drop redundant `create_dir_all` |
| `crates/rocket-infra/src/fs_cookie_repo.rs` | Use `yaml_io::read_dir_yaml`, `yaml_io::delete_if_exists`; drop redundant `create_dir_all` |

---

### Task 1: Create `yaml_io.rs` with `read_dir_yaml` and `delete_if_exists`

**Files:**
- Create: `crates/rocket-infra/src/yaml_io.rs`
- Modify: `crates/rocket-infra/src/lib.rs` (add `mod yaml_io;`)

**Background:** Every one of the four list-repos contains a copy of the same loop:
```rust
for entry in fs::read_dir(&self.dir)? {
    let path = entry?.path();
    if path.extension().is_some_and(|e| e == "yml") {
        let content = fs::read_to_string(&path)?;
        if let Ok(item) = serde_yaml::from_str::<T>(&content) { ... }
    }
}
```
And each `delete` method has:
```rust
if !path.exists() { return Err(DomainError::NotFound(...)); }
fs::remove_file(&path)?;
```

`read_dir_yaml<T>` returns `Vec<(PathBuf, T)>` — each successfully-parsed item paired with its path. The path is needed by `FsHistoryRepo` (which sorts by mtime) and `FsCookieRepo` (which clears all). Items that fail to parse are silently skipped (matching the existing behavior). Returns an empty `Vec` if the directory does not exist.

`delete_if_exists` takes a `&Path` and a `&str` label for the `NotFound` message. Returns `Ok(())` on success, `DomainError::NotFound` if the file is absent.

- [ ] **Step 1: Write tests for `read_dir_yaml` and `delete_if_exists`**

  Create `crates/rocket-infra/src/yaml_io.rs` with this content (tests first, implementations as stubs):

  ```rust
  use std::fs;
  use std::path::{Path, PathBuf};

  use serde::de::DeserializeOwned;

  use rocket_shared::error::{DomainError, DomainResult};

  /// Read all `.yml` files in `dir` and parse each with serde_yaml.
  /// Files that fail to parse are silently skipped.
  /// Returns an empty Vec if `dir` does not exist.
  pub(crate) fn read_dir_yaml<T: DeserializeOwned>(dir: &Path) -> DomainResult<Vec<(PathBuf, T)>> {
      todo!()
  }

  /// Remove a file at `path`, returning `DomainError::NotFound(label)` if it does not exist.
  pub(crate) fn delete_if_exists(path: &Path, label: &str) -> DomainResult<()> {
      todo!()
  }

  #[cfg(test)]
  mod tests {
      use super::*;
      use serde::{Deserialize, Serialize};
      use tempfile::TempDir;

      #[derive(Debug, Serialize, Deserialize, PartialEq)]
      struct Item {
          name: String,
          value: u32,
      }

      #[test]
      fn read_dir_yaml_returns_empty_when_dir_missing() {
          let dir = TempDir::new().unwrap();
          let absent = dir.path().join("nonexistent");
          let result: Vec<(PathBuf, Item)> = read_dir_yaml(&absent).unwrap();
          assert!(result.is_empty());
      }

      #[test]
      fn read_dir_yaml_parses_yml_files() {
          let dir = TempDir::new().unwrap();
          fs::write(dir.path().join("a.yml"), b"name: alpha\nvalue: 1\n").unwrap();
          fs::write(dir.path().join("b.yml"), b"name: beta\nvalue: 2\n").unwrap();

          let mut items: Vec<(PathBuf, Item)> = read_dir_yaml(dir.path()).unwrap();
          items.sort_by(|(_, a), (_, b)| a.name.cmp(&b.name));

          assert_eq!(items.len(), 2);
          assert_eq!(items[0].1, Item { name: "alpha".into(), value: 1 });
          assert_eq!(items[1].1, Item { name: "beta".into(), value: 2 });
      }

      #[test]
      fn read_dir_yaml_skips_non_yml_files() {
          let dir = TempDir::new().unwrap();
          fs::write(dir.path().join("a.yml"), b"name: alpha\nvalue: 1\n").unwrap();
          fs::write(dir.path().join("b.json"), b"{\"name\":\"beta\",\"value\":2}").unwrap();
          fs::write(dir.path().join("c.txt"), b"plain text").unwrap();

          let items: Vec<(PathBuf, Item)> = read_dir_yaml(dir.path()).unwrap();
          assert_eq!(items.len(), 1);
          assert_eq!(items[0].1.name, "alpha");
      }

      #[test]
      fn read_dir_yaml_skips_unparseable_yml_files() {
          let dir = TempDir::new().unwrap();
          fs::write(dir.path().join("good.yml"), b"name: good\nvalue: 42\n").unwrap();
          fs::write(dir.path().join("bad.yml"), b"not: valid: yaml: {{").unwrap();

          let items: Vec<(PathBuf, Item)> = read_dir_yaml(dir.path()).unwrap();
          assert_eq!(items.len(), 1);
          assert_eq!(items[0].1.name, "good");
      }

      #[test]
      fn read_dir_yaml_returns_path_alongside_item() {
          let dir = TempDir::new().unwrap();
          fs::write(dir.path().join("thing.yml"), b"name: thing\nvalue: 7\n").unwrap();

          let items: Vec<(PathBuf, Item)> = read_dir_yaml(dir.path()).unwrap();
          assert_eq!(items.len(), 1);
          assert_eq!(items[0].0, dir.path().join("thing.yml"));
      }

      #[test]
      fn delete_if_exists_removes_file() {
          let dir = TempDir::new().unwrap();
          let path = dir.path().join("target.yml");
          fs::write(&path, b"data").unwrap();

          delete_if_exists(&path, "thing 'x'").unwrap();
          assert!(!path.exists());
      }

      #[test]
      fn delete_if_exists_returns_not_found_when_absent() {
          let dir = TempDir::new().unwrap();
          let path = dir.path().join("missing.yml");

          let err = delete_if_exists(&path, "item 'missing'").unwrap_err();
          assert!(matches!(err, DomainError::NotFound(_)), "expected NotFound, got {:?}", err);
      }
  }
  ```

- [ ] **Step 2: Run the tests to confirm they fail (stubs)**

  ```bash
  cargo test -p rocket-infra yaml_io 2>&1 | tail -15
  ```

  Expected: compile error on `todo!()` or test failures — functions not implemented yet.

- [ ] **Step 3: Implement `read_dir_yaml` and `delete_if_exists`**

  Replace the `todo!()` stubs with real implementations:

  ```rust
  pub(crate) fn read_dir_yaml<T: DeserializeOwned>(dir: &Path) -> DomainResult<Vec<(PathBuf, T)>> {
      if !dir.exists() {
          return Ok(Vec::new());
      }
      let mut out = Vec::new();
      for entry in fs::read_dir(dir)? {
          let path = entry?.path();
          if path.extension().is_some_and(|e| e == "yml") {
              let content = fs::read_to_string(&path)?;
              if let Ok(item) = serde_yaml::from_str::<T>(&content) {
                  out.push((path, item));
              }
          }
      }
      Ok(out)
  }

  pub(crate) fn delete_if_exists(path: &Path, label: &str) -> DomainResult<()> {
      if !path.exists() {
          return Err(DomainError::NotFound(label.to_string()));
      }
      fs::remove_file(path)?;
      Ok(())
  }
  ```

- [ ] **Step 4: Register the module in `lib.rs`**

  In `crates/rocket-infra/src/lib.rs`, add after line 1 (`mod atomic_write;`):

  ```rust
  mod yaml_io;
  ```

  The full top of `lib.rs` becomes:

  ```rust
  mod atomic_write;
  mod yaml_io;
  pub mod file_watcher;
  // ... rest unchanged
  ```

- [ ] **Step 5: Run the tests to confirm they pass**

  ```bash
  cargo test -p rocket-infra yaml_io 2>&1 | tail -15
  ```

  Expected: all 8 yaml_io tests PASS.

- [ ] **Step 6: Compile check**

  ```bash
  cargo check -p rocket-infra 2>&1 | grep "^error" | head -10
  ```

  Expected: zero errors.

- [ ] **Step 7: Commit**

  ```bash
  git add crates/rocket-infra/src/yaml_io.rs crates/rocket-infra/src/lib.rs
  git commit -m "feat(infra): add yaml_io module with read_dir_yaml and delete_if_exists helpers"
  ```

---

### Task 2: Migrate `FsEnvironmentRepo` to use `yaml_io` helpers

**Files:**
- Modify: `crates/rocket-infra/src/fs_environment_repo.rs`

**Background:** The `list()` method in `FsEnvironmentRepo` loops over `read_dir`, filters `.yml`, and parses. The `delete()` method checks existence then removes. Both duplicate logic now in `yaml_io`. The `save()` method calls `fs::create_dir_all(&self.dir)?` before `atomic_write`, but `atomic_write` already creates parent directories — the call is redundant.

**Note on `list()`:** `FsEnvironmentRepo::list()` has special fallback logic: it tries `serde_yaml::from_str::<OcEnvironment>` first, then falls back to `serde_yaml::from_str::<Environment>`. This two-format fallback cannot be expressed as a single `read_dir_yaml<T>` call. Keep the loop inline for `list()` but use `read_dir_yaml` for the path enumeration (just get the paths, then do the two-format parse). Actually, the cleanest approach: read items with `read_dir_yaml::<OcEnvironment>` and handle the fallback separately by also reading files that failed to parse as `Environment`. The simplest correct approach is: keep the existing `list()` body intact (it is not duplicated elsewhere) and only apply `yaml_io` to `delete()` and remove `create_dir_all` from `save()`.

- [ ] **Step 1: Confirm existing environment tests pass before touching anything**

  ```bash
  cargo test -p rocket-infra fs_environment_repo 2>&1 | tail -10
  ```

  Expected: all environment tests PASS. This is your regression baseline.

- [ ] **Step 2: Use `delete_if_exists` in `FsEnvironmentRepo::delete`**

  In `crates/rocket-infra/src/fs_environment_repo.rs`, add the import at the top:

  ```rust
  use crate::yaml_io::delete_if_exists;
  ```

  Replace the `delete` method (lines 68-75):

  ```rust
  // Before:
  fn delete(&self, name: &str) -> DomainResult<()> {
      let path = self.file_path(name);
      if !path.exists() {
          return Err(DomainError::NotFound(format!("Environment '{}'", name)));
      }
      fs::remove_file(&path)?;
      Ok(())
  }
  ```

  With:

  ```rust
  fn delete(&self, name: &str) -> DomainResult<()> {
      delete_if_exists(&self.file_path(name), &format!("Environment '{}'", name))
  }
  ```

- [ ] **Step 3: Drop the redundant `create_dir_all` from `FsEnvironmentRepo::save`**

  In `save()` (line 59), remove:

  ```rust
  fs::create_dir_all(&self.dir)?;
  ```

  The `save` method becomes:

  ```rust
  fn save(&self, env: &Environment) -> DomainResult<()> {
      let oc: OcEnvironment = env.clone().into();
      let yaml = serde_yaml::to_string(&oc)
          .map_err(|e| DomainError::Internal(format!("Failed to serialize environment: {e}")))?;
      atomic_write(&self.file_path(&env.name), yaml.as_bytes())?;
      Ok(())
  }
  ```

  Also remove the `use std::fs;` import if it is now unused (check whether `fs` is still referenced anywhere — in `list()` it is still used for `fs::read_dir`). Keep `use std::fs;` since `list()` still calls `fs::read_dir` and `fs::read_to_string`.

- [ ] **Step 4: Run all environment repo tests**

  ```bash
  cargo test -p rocket-infra fs_environment_repo 2>&1 | tail -10
  ```

  Expected: all tests PASS (same count as Step 1).

- [ ] **Step 5: Compile check**

  ```bash
  cargo check -p rocket-infra 2>&1 | grep "^error" | head -10
  ```

  Expected: zero errors.

- [ ] **Step 6: Commit**

  ```bash
  git add crates/rocket-infra/src/fs_environment_repo.rs
  git commit -m "refactor(infra): use delete_if_exists in FsEnvironmentRepo; drop redundant create_dir_all"
  ```

---

### Task 3: Migrate `FsTemplateRepo` to use `yaml_io` helpers

**Files:**
- Modify: `crates/rocket-infra/src/fs_template_repo.rs`

**Background:** `FsTemplateRepo::list()` loops over `.yml` files and parses each as `Template` — a single type with no format fallback. This is a direct fit for `read_dir_yaml::<Template>`. `delete()` is also a straightforward `delete_if_exists` replacement. `save()` has a redundant `create_dir_all`.

- [ ] **Step 1: Confirm existing template tests pass**

  ```bash
  cargo test -p rocket-infra fs_template_repo 2>&1 | tail -10
  ```

  Expected: all template tests PASS.

- [ ] **Step 2: Rewrite `FsTemplateRepo` using `yaml_io`**

  Replace the full content of `crates/rocket-infra/src/fs_template_repo.rs` with:

  ```rust
  use std::path::PathBuf;

  use crate::atomic_write;
  use crate::yaml_io::{delete_if_exists, read_dir_yaml};
  use rocket_history::{Template, TemplateRepository};
  use rocket_shared::error::DomainResult;

  pub struct FsTemplateRepo {
      dir: PathBuf,
  }

  impl FsTemplateRepo {
      pub fn new(dir: PathBuf) -> Self {
          Self { dir }
      }

      fn file_path(&self, name: &str) -> PathBuf {
          self.dir.join(format!("{}.yml", name))
      }
  }

  impl TemplateRepository for FsTemplateRepo {
      fn list(&self) -> DomainResult<Vec<Template>> {
          let mut items: Vec<Template> = read_dir_yaml::<Template>(&self.dir)?
              .into_iter()
              .map(|(_, t)| t)
              .collect();
          items.sort_by(|a, b| a.name.cmp(&b.name));
          Ok(items)
      }

      fn get(&self, name: &str) -> DomainResult<Template> {
          let path = self.file_path(name);
          if !path.exists() {
              return Err(rocket_shared::error::DomainError::NotFound(format!("Template '{}'", name)));
          }
          let content = std::fs::read_to_string(&path)?;
          serde_yaml::from_str(&content)
              .map_err(|e| rocket_shared::error::DomainError::Internal(format!("Failed to parse YAML: {e}")))
      }

      fn save(&self, template: &Template) -> DomainResult<()> {
          let yaml = serde_yaml::to_string(template)
              .map_err(|e| rocket_shared::error::DomainError::Internal(format!("Failed to serialize YAML: {e}")))?;
          atomic_write(&self.file_path(&template.name), yaml.as_bytes())?;
          Ok(())
      }

      fn delete(&self, name: &str) -> DomainResult<()> {
          delete_if_exists(&self.file_path(name), &format!("Template '{}'", name))
      }
  }

  #[cfg(test)]
  mod tests {
      use super::*;
      use rocket_shared::types::HttpMethod;
      use tempfile::TempDir;

      fn setup() -> (TempDir, FsTemplateRepo) {
          let dir = TempDir::new().unwrap();
          let repo = FsTemplateRepo::new(dir.path().to_path_buf());
          (dir, repo)
      }

      #[test]
      fn save_and_list() {
          let (_dir, repo) = setup();
          let t = Template::new("JSON POST", HttpMethod::Post, "https://api.example.com");
          repo.save(&t).unwrap();
          let list = repo.list().unwrap();
          assert_eq!(list.len(), 1);
          assert_eq!(list[0].name, "JSON POST");
      }

      #[test]
      fn get_by_name() {
          let (_dir, repo) = setup();
          let t = Template::new("GET Users", HttpMethod::Get, "https://api.example.com/users");
          repo.save(&t).unwrap();
          let loaded = repo.get("GET Users").unwrap();
          assert_eq!(loaded.url, "https://api.example.com/users");
      }

      #[test]
      fn delete_template() {
          let (_dir, repo) = setup();
          let t = Template::new("temp", HttpMethod::Delete, "/resource");
          repo.save(&t).unwrap();
          repo.delete("temp").unwrap();
          assert!(repo.list().unwrap().is_empty());
      }

      #[test]
      fn delete_nonexistent_returns_not_found() {
          let (_dir, repo) = setup();
          let err = repo.delete("ghost").unwrap_err();
          assert!(matches!(err, rocket_shared::error::DomainError::NotFound(_)));
      }
  }
  ```

- [ ] **Step 3: Run the template tests**

  ```bash
  cargo test -p rocket-infra fs_template_repo 2>&1 | tail -10
  ```

  Expected: all tests PASS (including the new `delete_nonexistent_returns_not_found`).

- [ ] **Step 4: Compile check**

  ```bash
  cargo check -p rocket-infra 2>&1 | grep "^error" | head -10
  ```

  Expected: zero errors.

- [ ] **Step 5: Commit**

  ```bash
  git add crates/rocket-infra/src/fs_template_repo.rs
  git commit -m "refactor(infra): migrate FsTemplateRepo to yaml_io helpers; drop redundant create_dir_all"
  ```

---

### Task 4: Migrate `FsCookieRepo` to use `yaml_io` helpers

**Files:**
- Modify: `crates/rocket-infra/src/fs_cookie_repo.rs`

**Background:** `FsCookieRepo::get_all()` loops `.yml` files and parses as `CookieJar` — a single type, fits `read_dir_yaml`. `clear()` also iterates `.yml` files and removes each — it can use the `(PathBuf, _)` pairs from `read_dir_yaml` to get paths, or keep its own simpler loop. The `save()` has the redundant `create_dir_all`.

`get_by_domain` uses `Ok(None)` instead of `NotFound`, so it does not use `delete_if_exists`. There is no `delete()` method — cookie deletion is via `clear()` only.

- [ ] **Step 1: Confirm existing cookie tests pass**

  ```bash
  cargo test -p rocket-infra fs_cookie_repo 2>&1 | tail -10
  ```

  Expected: all cookie tests PASS.

- [ ] **Step 2: Rewrite `FsCookieRepo` using `yaml_io`**

  Replace the full content of `crates/rocket-infra/src/fs_cookie_repo.rs` with:

  ```rust
  use std::fs;
  use std::path::PathBuf;

  use rocket_http::{CookieJar, CookieRepository};
  use rocket_shared::error::DomainResult;

  use crate::atomic_write;
  use crate::yaml_io::read_dir_yaml;

  pub struct FsCookieRepo {
      dir: PathBuf,
  }

  impl FsCookieRepo {
      pub fn new(dir: PathBuf) -> Self {
          Self { dir }
      }

      /// Sanitize domain for use as a filename (replace dots and colons).
      fn file_path(&self, domain: &str) -> PathBuf {
          let sanitized = domain.replace(['.', ':'], "_");
          self.dir.join(format!("{}.yml", sanitized))
      }
  }

  impl CookieRepository for FsCookieRepo {
      fn get_all(&self) -> DomainResult<Vec<CookieJar>> {
          Ok(read_dir_yaml::<CookieJar>(&self.dir)?
              .into_iter()
              .map(|(_, jar)| jar)
              .collect())
      }

      fn get_by_domain(&self, domain: &str) -> DomainResult<Option<CookieJar>> {
          let path = self.file_path(domain);
          if !path.exists() {
              return Ok(None);
          }
          let content = fs::read_to_string(&path)?;
          let jar = serde_yaml::from_str(&content)
              .map_err(|e| rocket_shared::error::DomainError::Internal(format!("Failed to parse YAML: {e}")))?;
          Ok(Some(jar))
      }

      fn save(&self, jar: &CookieJar) -> DomainResult<()> {
          let yaml = serde_yaml::to_string(jar)
              .map_err(|e| rocket_shared::error::DomainError::Internal(format!("Failed to serialize YAML: {e}")))?;
          atomic_write(&self.file_path(&jar.domain), yaml.as_bytes())?;
          Ok(())
      }

      fn clear(&self) -> DomainResult<()> {
          for (path, _) in read_dir_yaml::<CookieJar>(&self.dir)? {
              fs::remove_file(&path)?;
          }
          Ok(())
      }
  }

  #[cfg(test)]
  mod tests {
      use super::*;
      use rocket_http::Cookie;
      use tempfile::TempDir;

      fn setup() -> (TempDir, FsCookieRepo) {
          let dir = TempDir::new().unwrap();
          let repo = FsCookieRepo::new(dir.path().to_path_buf());
          (dir, repo)
      }

      fn sample_jar(domain: &str) -> CookieJar {
          let mut jar = CookieJar::new(domain);
          jar.add(Cookie {
              name: "session".into(),
              value: "abc123".into(),
              domain: domain.into(),
              path: "/".into(),
              secure: true,
              http_only: true,
              expires: None,
          });
          jar
      }

      #[test]
      fn save_and_get_all() {
          let (_dir, repo) = setup();
          repo.save(&sample_jar("example.com")).unwrap();
          repo.save(&sample_jar("api.example.com")).unwrap();
          let all = repo.get_all().unwrap();
          assert_eq!(all.len(), 2);
      }

      #[test]
      fn get_by_domain() {
          let (_dir, repo) = setup();
          repo.save(&sample_jar("example.com")).unwrap();
          let jar = repo.get_by_domain("example.com").unwrap();
          assert!(jar.is_some());
          assert_eq!(jar.unwrap().get("session").unwrap().value, "abc123");
      }

      #[test]
      fn clear_all() {
          let (_dir, repo) = setup();
          repo.save(&sample_jar("a.com")).unwrap();
          repo.save(&sample_jar("b.com")).unwrap();
          repo.clear().unwrap();
          assert!(repo.get_all().unwrap().is_empty());
      }

      #[test]
      fn get_all_returns_empty_when_dir_missing() {
          let dir = TempDir::new().unwrap();
          let repo = FsCookieRepo::new(dir.path().join("cookies"));
          assert!(repo.get_all().unwrap().is_empty());
      }
  }
  ```

- [ ] **Step 3: Run the cookie tests**

  ```bash
  cargo test -p rocket-infra fs_cookie_repo 2>&1 | tail -10
  ```

  Expected: all tests PASS.

- [ ] **Step 4: Compile check**

  ```bash
  cargo check -p rocket-infra 2>&1 | grep "^error" | head -10
  ```

  Expected: zero errors.

- [ ] **Step 5: Commit**

  ```bash
  git add crates/rocket-infra/src/fs_cookie_repo.rs
  git commit -m "refactor(infra): migrate FsCookieRepo to yaml_io helpers; drop redundant create_dir_all"
  ```

---

### Task 5: Migrate `FsHistoryRepo` to use `yaml_io` and drop redundant `create_dir_all`

**Files:**
- Modify: `crates/rocket-infra/src/fs_history_repo.rs`

**Background:** `FsHistoryRepo` is the most complex of the four repos because:
1. `list()` sorts entries by **mtime** before parsing — it cannot just call `read_dir_yaml<T>` and iterate the result, because mtime sorting requires the metadata, which is only available during `read_dir` iteration.
2. `search()` also sorts by mtime with a cap of 200.
3. `clear()` deletes all `.yml` files.
4. `save()` has the redundant `create_dir_all`.

For (1) and (2): `read_dir_yaml` does not capture mtime. We need a variant that includes mtime. Rather than over-generalizing `yaml_io`, add a second helper `read_dir_yaml_with_mtime<T>` that returns `Vec<(SystemTime, PathBuf, T)>` — sorted descending by mtime, up to an optional limit before parsing.

Actually, looking at the current `list()` implementation: it first collects `(mtime, path)` pairs (without parsing), sorts them, then parses in order stopping at `cap`. This two-phase approach is important for performance (avoids parsing files we'll never use). We should preserve this approach.

The cleanest fit: add a new helper `read_dir_by_mtime` to `yaml_io.rs` that returns `Vec<(SystemTime, PathBuf)>` sorted by mtime descending. Then `list()` and `search()` each call this and parse in their own loops. This keeps the performance optimization and eliminates the duplicated `read_dir` + filter + metadata + sort boilerplate between `list()` and `search()`.

- [ ] **Step 1: Add `read_dir_by_mtime` to `yaml_io.rs`**

  In `crates/rocket-infra/src/yaml_io.rs`, add this function and its test **before** the `#[cfg(test)]` block:

  ```rust
  /// Collect all `.yml` file paths in `dir` paired with their modification time,
  /// sorted most-recently-modified first. Returns an empty Vec if `dir` does not exist.
  /// Used by repos that sort by mtime before parsing (e.g., history, search).
  pub(crate) fn read_dir_by_mtime(dir: &Path) -> DomainResult<Vec<(std::time::SystemTime, PathBuf)>> {
      if !dir.exists() {
          return Ok(Vec::new());
      }
      let mut paths: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
      for entry in fs::read_dir(dir)? {
          let entry = entry?;
          let path = entry.path();
          if path.extension().is_some_and(|e| e == "yml") {
              let mtime = entry
                  .metadata()?
                  .modified()
                  .unwrap_or(std::time::UNIX_EPOCH);
              paths.push((mtime, path));
          }
      }
      paths.sort_by(|a, b| b.0.cmp(&a.0));
      Ok(paths)
  }
  ```

  And add this test inside the existing `#[cfg(test)] mod tests { ... }` block in `yaml_io.rs`:

  ```rust
  #[test]
  fn read_dir_by_mtime_returns_empty_when_dir_missing() {
      let dir = TempDir::new().unwrap();
      let absent = dir.path().join("nonexistent");
      let result = read_dir_by_mtime(&absent).unwrap();
      assert!(result.is_empty());
  }

  #[test]
  fn read_dir_by_mtime_returns_yml_paths_sorted_newest_first() {
      let dir = TempDir::new().unwrap();
      // Write files with a small delay so mtime differs.
      fs::write(dir.path().join("a.yml"), b"x").unwrap();
      std::thread::sleep(std::time::Duration::from_millis(5));
      fs::write(dir.path().join("b.yml"), b"x").unwrap();

      let paths = read_dir_by_mtime(dir.path()).unwrap();
      assert_eq!(paths.len(), 2);
      // b.yml was written last, so it must be first.
      assert_eq!(paths[0].1.file_name().unwrap(), "b.yml");
      assert_eq!(paths[1].1.file_name().unwrap(), "a.yml");
  }

  #[test]
  fn read_dir_by_mtime_skips_non_yml_files() {
      let dir = TempDir::new().unwrap();
      fs::write(dir.path().join("a.yml"), b"x").unwrap();
      fs::write(dir.path().join("b.json"), b"{}").unwrap();

      let paths = read_dir_by_mtime(dir.path()).unwrap();
      assert_eq!(paths.len(), 1);
      assert_eq!(paths[0].1.file_name().unwrap(), "a.yml");
  }
  ```

- [ ] **Step 2: Run the new yaml_io tests to confirm they pass**

  ```bash
  cargo test -p rocket-infra yaml_io 2>&1 | tail -15
  ```

  Expected: all yaml_io tests PASS (now 11 tests).

- [ ] **Step 3: Confirm existing history repo tests pass before touching the repo**

  ```bash
  cargo test -p rocket-infra fs_history_repo 2>&1 | tail -10
  ```

  Expected: all history tests PASS.

- [ ] **Step 4: Rewrite `FsHistoryRepo` using `yaml_io`**

  Replace the full content of `crates/rocket-infra/src/fs_history_repo.rs` with:

  ```rust
  use std::fs;
  use std::path::PathBuf;

  use rocket_history::{HistoryEntry, HistoryFilter, HistoryRepository};
  use rocket_shared::error::{DomainError, DomainResult};

  use crate::atomic_write;
  use crate::yaml_io::{read_dir_by_mtime, read_dir_yaml};

  pub struct FsHistoryRepo {
      dir: PathBuf,
  }

  impl FsHistoryRepo {
      pub fn new(dir: PathBuf) -> Self {
          Self { dir }
      }

      fn file_path(&self, id: &str) -> PathBuf {
          self.dir.join(format!("{}.yml", id))
      }
  }

  impl HistoryRepository for FsHistoryRepo {
      fn list(&self, limit: Option<usize>) -> DomainResult<Vec<HistoryEntry>> {
          let paths = read_dir_by_mtime(&self.dir)?;
          let cap = limit.unwrap_or(usize::MAX);
          let mut entries = Vec::with_capacity(cap.min(paths.len()));
          for (_, path) in paths {
              if entries.len() >= cap {
                  break;
              }
              let content = match fs::read_to_string(&path) {
                  Ok(c) => c,
                  Err(_) => continue,
              };
              if let Ok(h) = serde_yaml::from_str::<HistoryEntry>(&content) {
                  entries.push(h);
              }
          }
          // Secondary sort by timestamp in case mtime ties (same-second writes).
          entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
          Ok(entries)
      }

      fn get(&self, id: &str) -> DomainResult<HistoryEntry> {
          let path = self.file_path(id);
          if !path.exists() {
              return Err(DomainError::NotFound(format!("HistoryEntry '{}'", id)));
          }
          let content = fs::read_to_string(&path)?;
          serde_yaml::from_str(&content)
              .map_err(|e| DomainError::Internal(format!("Failed to parse YAML: {e}")))
      }

      fn save(&self, entry: &HistoryEntry) -> DomainResult<()> {
          let yaml = serde_yaml::to_string(entry)
              .map_err(|e| DomainError::Internal(format!("Failed to serialize YAML: {e}")))?;
          atomic_write(&self.file_path(&entry.id), yaml.as_bytes())?;
          Ok(())
      }

      fn clear(&self) -> DomainResult<()> {
          for (_, path) in read_dir_yaml::<HistoryEntry>(&self.dir)? {
              fs::remove_file(&path)?;
          }
          Ok(())
      }

      fn search(&self, filter: &HistoryFilter) -> DomainResult<Vec<HistoryEntry>> {
          const SEARCH_LIMIT: usize = 200;
          let paths = read_dir_by_mtime(&self.dir)?;
          let mut results = Vec::new();
          for (_, path) in paths {
              if results.len() >= SEARCH_LIMIT {
                  break;
              }
              let content = match fs::read_to_string(&path) {
                  Ok(c) => c,
                  Err(_) => continue,
              };
              let entry: HistoryEntry = match serde_yaml::from_str(&content) {
                  Ok(e) => e,
                  Err(_) => continue,
              };
              if let Some(method) = &filter.method {
                  if !entry.method.eq_ignore_ascii_case(method) {
                      continue;
                  }
              }
              if let Some(url_pattern) = &filter.url_contains {
                  if !entry.url.contains(url_pattern.as_str()) {
                      continue;
                  }
              }
              if let Some(min) = filter.status_min {
                  if entry.status < min {
                      continue;
                  }
              }
              if let Some(max) = filter.status_max {
                  if entry.status > max {
                      continue;
                  }
              }
              results.push(entry);
          }
          Ok(results)
      }
  }

  #[cfg(test)]
  mod tests {
      use super::*;
      use tempfile::TempDir;

      fn setup() -> (TempDir, FsHistoryRepo) {
          let dir = TempDir::new().unwrap();
          let repo = FsHistoryRepo::new(dir.path().to_path_buf());
          (dir, repo)
      }

      #[test]
      fn save_and_list() {
          let (_dir, repo) = setup();
          let entry = HistoryEntry::new("GET", "https://api.example.com", 200, 100, 512);
          repo.save(&entry).unwrap();
          let list = repo.list(None).unwrap();
          assert_eq!(list.len(), 1);
          assert_eq!(list[0].method, "GET");
      }

      #[test]
      fn list_with_limit() {
          let (_dir, repo) = setup();
          for i in 0..5 {
              let e = HistoryEntry::new("GET", format!("/path/{}", i), 200, 10, 0);
              repo.save(&e).unwrap();
          }
          let list = repo.list(Some(3)).unwrap();
          assert_eq!(list.len(), 3);
      }

      #[test]
      fn get_by_id() {
          let (_dir, repo) = setup();
          let entry = HistoryEntry::new("POST", "/api", 201, 50, 128);
          let id = entry.id.clone();
          repo.save(&entry).unwrap();
          let loaded = repo.get(&id).unwrap();
          assert_eq!(loaded.id, id);
      }

      #[test]
      fn clear_history() {
          let (_dir, repo) = setup();
          repo.save(&HistoryEntry::new("GET", "/a", 200, 10, 0)).unwrap();
          repo.save(&HistoryEntry::new("POST", "/b", 201, 20, 0)).unwrap();
          repo.clear().unwrap();
          assert!(repo.list(None).unwrap().is_empty());
      }

      #[test]
      fn search_empty_filter_returns_all() {
          let (_dir, repo) = setup();
          repo.save(&HistoryEntry::new("GET", "/a", 200, 10, 0)).unwrap();
          repo.save(&HistoryEntry::new("POST", "/b", 201, 20, 0)).unwrap();
          let results = repo.search(&HistoryFilter::default()).unwrap();
          assert_eq!(results.len(), 2);
      }

      #[test]
      fn search_by_method_returns_matching_entries() {
          let (_dir, repo) = setup();
          repo.save(&HistoryEntry::new("GET", "/a", 200, 10, 0)).unwrap();
          repo.save(&HistoryEntry::new("POST", "/b", 201, 20, 0)).unwrap();
          repo.save(&HistoryEntry::new("get", "/c", 204, 5, 0)).unwrap();
          let filter = HistoryFilter { method: Some("GET".to_string()), ..Default::default() };
          let results = repo.search(&filter).unwrap();
          assert_eq!(results.len(), 2);
          assert!(results.iter().all(|e| e.method.to_uppercase() == "GET"));
      }

      #[test]
      fn search_by_url_contains_returns_matching_entries() {
          let (_dir, repo) = setup();
          repo.save(&HistoryEntry::new("GET", "https://api.example.com/users", 200, 10, 0)).unwrap();
          repo.save(&HistoryEntry::new("GET", "https://api.example.com/items", 200, 10, 0)).unwrap();
          repo.save(&HistoryEntry::new("GET", "https://other.io/users", 200, 10, 0)).unwrap();
          let filter = HistoryFilter { url_contains: Some("example.com".to_string()), ..Default::default() };
          let results = repo.search(&filter).unwrap();
          assert_eq!(results.len(), 2);
          assert!(results.iter().all(|e| e.url.contains("example.com")));
      }

      #[test]
      fn search_by_status_range_returns_2xx_only() {
          let (_dir, repo) = setup();
          repo.save(&HistoryEntry::new("GET", "/ok", 200, 10, 0)).unwrap();
          repo.save(&HistoryEntry::new("GET", "/created", 201, 10, 0)).unwrap();
          repo.save(&HistoryEntry::new("GET", "/not-found", 404, 10, 0)).unwrap();
          repo.save(&HistoryEntry::new("GET", "/error", 500, 10, 0)).unwrap();
          let filter = HistoryFilter { status_min: Some(200), status_max: Some(299), ..Default::default() };
          let results = repo.search(&filter).unwrap();
          assert_eq!(results.len(), 2);
          assert!(results.iter().all(|e| e.status >= 200 && e.status <= 299));
      }

      #[test]
      fn search_combined_method_and_status_filters() {
          let (_dir, repo) = setup();
          repo.save(&HistoryEntry::new("GET", "/a", 200, 10, 0)).unwrap();
          repo.save(&HistoryEntry::new("GET", "/b", 404, 10, 0)).unwrap();
          repo.save(&HistoryEntry::new("POST", "/c", 200, 10, 0)).unwrap();
          let filter = HistoryFilter {
              method: Some("GET".to_string()),
              status_min: Some(200),
              status_max: Some(299),
              ..Default::default()
          };
          let results = repo.search(&filter).unwrap();
          assert_eq!(results.len(), 1);
          assert_eq!(results[0].method, "GET");
          assert_eq!(results[0].status, 200);
      }

      #[test]
      fn list_with_limit_reads_only_needed_files() {
          let (_dir, repo) = setup();
          let mut ids = Vec::new();
          for i in 0..10u64 {
              let e = HistoryEntry::new("GET", format!("/path/{}", i), 200, i, 0);
              ids.push(e.id.clone());
              repo.save(&e).unwrap();
              std::thread::sleep(std::time::Duration::from_millis(2));
          }
          let list = repo.list(Some(3)).unwrap();
          assert_eq!(list.len(), 3);
          let returned_ids: std::collections::HashSet<_> = list.iter().map(|e| e.id.as_str()).collect();
          for id in &ids[7..] {
              assert!(returned_ids.contains(id.as_str()), "expected id {} in results", id);
          }
      }
  }
  ```

- [ ] **Step 5: Run the history repo tests**

  ```bash
  cargo test -p rocket-infra fs_history_repo 2>&1 | tail -15
  ```

  Expected: all 10 history tests PASS.

- [ ] **Step 6: Run the full infra test suite**

  ```bash
  cargo test -p rocket-infra 2>&1 | tail -10
  ```

  Expected: all tests pass.

- [ ] **Step 7: Commit**

  ```bash
  git add crates/rocket-infra/src/yaml_io.rs crates/rocket-infra/src/fs_history_repo.rs
  git commit -m "refactor(infra): migrate FsHistoryRepo to yaml_io helpers; add read_dir_by_mtime; drop redundant create_dir_all"
  ```

---

## Self-Review

### Spec coverage (Phase 2 §7 checklist)

| Requirement | Task | Covered? |
|---|---|---|
| Create `crate::yaml_io` with `read<T>`, `write<T>`, `read_dir<T>`, `delete_if_exists`, `parse_with_ctx<T>` | Tasks 1–5 | `read_dir_yaml<T>`, `read_dir_by_mtime`, `delete_if_exists` — write/parse_with_ctx not needed (write is `atomic_write` which already exists; parse_with_ctx is implicit in the skip behavior). YAGNI — don't add unused helpers. |
| Migrate four list-repos (env/template/history/cookie) to use `yaml_io` | Tasks 2–5 | ✅ All four covered. |
| Drop redundant `fs::create_dir_all` before `atomic_write` | Tasks 2–5 | ✅ Removed in `save()` of all four repos. |

**Gap:** The review's Phase 2 spec also mentions `read<T>` (single-item read) and `write<T>` (serialize + write). These are not centralized here because:
- `get()` in each repo is trivially a 2-line `read_to_string` + `from_str` that varies by error message — a generic helper would need to accept the label as a parameter and the signature complexity would exceed the benefit.
- `write<T>` is already `serde_yaml::to_string(x)` + `atomic_write` — also 2 lines. Not worth abstracting.
YAGNI applies: only centralize patterns that appear 4+ times in identical form.

### Placeholder scan

No TBDs, TODOs, or incomplete steps found. All code blocks are complete.

### Type consistency

- `read_dir_yaml<T: DeserializeOwned>` returns `Vec<(PathBuf, T)>` — used as `read_dir_yaml::<Template>`, `read_dir_yaml::<CookieJar>`, `read_dir_yaml::<HistoryEntry>` consistently.
- `read_dir_by_mtime` returns `Vec<(SystemTime, PathBuf)>` — used in `FsHistoryRepo::list()` and `FsHistoryRepo::search()` identically.
- `delete_if_exists(path: &Path, label: &str) -> DomainResult<()>` — used in `FsEnvironmentRepo::delete()` and `FsTemplateRepo::delete()` with matching signatures.
