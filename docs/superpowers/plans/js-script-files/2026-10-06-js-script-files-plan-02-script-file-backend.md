# JS Script Files Plan 02: Script File Backend

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `.js` files in a collection show up in the collection tree as `ScriptFile` items, and five IPC commands create, read, save, rename and delete them safely.

**Architecture:** `rocket-collection` gets a `ScriptFileItem` value type, name validation and a new `CollectionItem::ScriptFile` variant. `rocket-infra` gets a scanner branch and a `script_files.rs` module implementing new `CollectionRepository` methods. `rocket-app`'s `CollectionService` delegates, and `src-tauri` exposes thin commands. The file watcher already emits `collection-changed` for any file under the collections directory, so these commands publish no extra events.

**Tech Stack:** Rust, `tempfile` fixtures, Tauri 2 commands, TypeScript wrappers in `src/lib/tauri-api.ts`.

**Spec:** `docs/superpowers/specs/2026-10-06-js-script-files-design.md` ("Domain and scanner", "Commands"). Prerequisite: Plan 01 is merged (it adds `CollectionRepository::collection_root_path`; this plan follows the same default-method pattern).

## Global Constraints

- `-j4` on every cargo invocation. No `cargo test --workspace`.
- Production paths return `DomainResult` with explicit error mapping and never panic. The command layer stays thin: validate, call the service, map the error.
- Every file path from the frontend goes through `FsCollectionRepo::validate_path()`.
- Serde: `rename_all = "camelCase"` on IPC DTOs only. `ScriptFileItem` is an IPC DTO and is never persisted.
- Code comments: short full sentences ending in a period.
- Commits through `dev-workflow-skills:1-git-commit`, conventional subjects, staged by path.
- Each task starts with: read `docs/superpowers/specs/opencollection-spec-reference.md`.

## Review Focus

- Script name `../evil`, `a/b`, `.hidden`, empty, `..`, or a name with a NUL byte: rejected (Task 1, Task 2).
- Creating a script that already exists: error, existing file untouched (Task 2).
- Saving to a path that is a symlink, a directory, a non-`.js` file such as `opencollection.yml`, or a missing file: error, nothing written (Task 2).
- A `node_modules` folder full of `.js` files: not listed (Task 1).
- Renaming a script onto an existing name: error, both files intact (Task 2).
- Request counting, the Collection Runner and contract code that `match` on `CollectionItem`: ignore script files (Task 1).

---

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `crates/rocket-collection/src/script_file.rs` | create | `ScriptFileItem`, `normalize_script_name`, `SCRIPT_TEMPLATE` |
| `crates/rocket-collection/src/folder.rs` | modify | `CollectionItem::ScriptFile` |
| `crates/rocket-collection/src/lib.rs` | modify | module and re-exports |
| `crates/rocket-collection/src/repository.rs` | modify | five script-file trait methods with default bodies |
| `crates/rocket-infra/src/fs_collection/tree.rs` | modify | list `.js` files, skip `node_modules` |
| `crates/rocket-infra/src/fs_collection/script_files.rs` | create | create/read/save/rename/delete |
| `crates/rocket-infra/src/fs_collection/mod.rs` | modify | `mod script_files;` and trait impls |
| `crates/rocket-infra/src/shared_path_collection_repo.rs` | modify | delegate five methods |
| `crates/rocket-app/src/collection_service.rs` | modify | five delegating methods |
| `src-tauri/src/commands/collections.rs` | modify | five commands |
| `src-tauri/src/lib.rs` | modify | register commands |
| `src/lib/tauri-api.ts` | modify | type and wrappers |

---

### Task 1: Domain type, name rules and tree scanner

**Files:**
- Create: `crates/rocket-collection/src/script_file.rs`
- Modify: `crates/rocket-collection/src/folder.rs`, `crates/rocket-collection/src/lib.rs`, `crates/rocket-infra/src/fs_collection/tree.rs`
- Modify (compile fixes): every `match` on `CollectionItem` that the compiler flags.
- Test: inline in `script_file.rs`; `crates/rocket-infra/src/fs_collection/tests.rs`

**Interfaces:**
- Produces:
  - `rocket_collection::ScriptFileItem { pub file_name: String, pub name: String }` (`Debug, Clone, PartialEq, Serialize, Deserialize`, camelCase) with `ScriptFileItem::new(file_name: &str) -> Self`.
  - `rocket_collection::normalize_script_name(raw: &str) -> DomainResult<String>` returns the safe file name with `.js` appended when missing.
  - `rocket_collection::SCRIPT_TEMPLATE: &str`.
  - `CollectionItem::ScriptFile(ScriptFileItem)`, serialised as `{"type":"scriptFile","fileName":"utils.js","name":"utils.js"}`.

- [ ] **Step 1: Read the spec reference**

Read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing domain tests**

Create `crates/rocket-collection/src/script_file.rs` with the test module only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_js_when_missing() {
        assert_eq!(normalize_script_name("utils").expect("ok"), "utils.js");
        assert_eq!(normalize_script_name("utils.js").expect("ok"), "utils.js");
        assert_eq!(normalize_script_name("  my lib  ").expect("ok"), "my lib.js");
    }

    #[test]
    fn rejects_unsafe_names() {
        for bad in [
            "", "   ", ".js", "..", "../evil", "a/b", "a\\b", ".hidden", "bad\0name", "..js",
        ] {
            assert!(
                normalize_script_name(bad).is_err(),
                "should reject {bad:?}"
            );
        }
    }

    #[test]
    fn rejects_overlong_names() {
        let long = "a".repeat(200);
        assert!(normalize_script_name(&long).is_err());
    }

    #[test]
    fn script_file_item_wire_shape() {
        let item = ScriptFileItem::new("utils.js");
        let json = serde_json::to_string(&crate::CollectionItem::ScriptFile(item)).expect("ser");
        assert_eq!(
            json,
            r#"{"type":"scriptFile","fileName":"utils.js","name":"utils.js"}"#
        );
    }

    #[test]
    fn template_exports_something() {
        assert!(SCRIPT_TEMPLATE.contains("module.exports"));
    }
}
```

Add `mod script_file;` and `pub use script_file::{normalize_script_name, ScriptFileItem, SCRIPT_TEMPLATE};` to `crates/rocket-collection/src/lib.rs`.

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -j4 -p rocket-collection script_file`
Expected: FAIL to compile.

- [ ] **Step 4: Implement the domain type**

Put above the test module in `script_file.rs`:

```rust
use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

/// Maximum length of a script file name, including `.js`.
const MAX_NAME_LEN: usize = 100;

/// Starter content written into a new script file.
pub const SCRIPT_TEMPLATE: &str = "// Shared helpers. Load them from any script tab with:\n\
// const { greet } = require('./THIS_FILE.js');\n\
\n\
const greet = (name) => `Hello, ${name}`;\n\
\n\
module.exports = {\n  greet,\n};\n";

/// A `.js` file in a collection folder. The file itself is the source of truth,
/// so there is no uid and nothing is persisted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptFileItem {
    /// On-disk file name, for example `utils.js`.
    pub file_name: String,
    /// Display name shown in the sidebar.
    pub name: String,
}

impl ScriptFileItem {
    pub fn new(file_name: &str) -> Self {
        Self {
            file_name: file_name.to_string(),
            name: file_name.to_string(),
        }
    }
}

/// Validates a user-typed script name and returns the safe file name.
///
/// Appends `.js` when missing. Rejects empty names, path separators, a leading
/// dot, `..`, NUL bytes and names over 100 characters.
pub fn normalize_script_name(raw: &str) -> DomainResult<String> {
    let trimmed = raw.trim();
    let bad = |msg: &str| DomainError::InvalidInput(format!("Invalid script name: {msg}"));
    if trimmed.is_empty() {
        return Err(bad("name is empty"));
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains('\0') {
        return Err(bad("name must not contain path separators"));
    }
    if trimmed.starts_with('.') || trimmed.contains("..") {
        return Err(bad("name must not start with a dot or contain '..'"));
    }
    let file_name = if trimmed.ends_with(".js") {
        trimmed.to_string()
    } else {
        format!("{trimmed}.js")
    };
    if file_name.len() > MAX_NAME_LEN {
        return Err(bad("name is too long"));
    }
    Ok(file_name)
}
```

Add the variant to `CollectionItem` in `folder.rs`, after `Summary`:

```rust
    /// A `.js` file that scripts load with `require()`. Not a request.
    #[serde(rename = "scriptFile")]
    ScriptFile(crate::ScriptFileItem),
```

- [ ] **Step 5: Run the domain tests**

Run: `cargo test -j4 -p rocket-collection script_file`
Expected: PASS.

- [ ] **Step 6: Fix every exhaustive match**

Run: `cargo check -j4 --workspace --tests`
Expected: FAIL with "non-exhaustive patterns: `ScriptFile(_)`". Known sites: `crates/rocket-collection/src/folder.rs:95` (request counting, return `0`), `crates/rocket-app/src/runner_sequence.rs:90` (add `| CollectionItem::ScriptFile(_)` to the no-op arm), `crates/rocket-app/src/contract_service.rs:992` (no-op), and the two arms in `crates/rocket-infra/src/conversions/folder.rs` (lines ~81 and ~191: return `None`, a script file is never serialised as a collection item). Fix each so a script file is ignored. Re-run until the workspace checks clean.

- [ ] **Step 7: Write the failing scanner tests**

Append to `crates/rocket-infra/src/fs_collection/tests.rs`:

```rust
fn script_names(items: &[rocket_collection::CollectionItem]) -> Vec<String> {
    items
        .iter()
        .filter_map(|i| match i {
            rocket_collection::CollectionItem::ScriptFile(s) => Some(s.file_name.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn tree_lists_js_files_at_root_and_in_folders() {
    let (dir, repo) = setup();
    repo.create("col").expect("create");
    let root = dir.path().join("col");
    fs::write(root.join("utils.js"), "module.exports = 1;").expect("write");
    repo.create_folder("col", "lib").expect("folder");
    fs::write(root.join("lib/helper.js"), "module.exports = 2;").expect("write");
    fs::write(root.join("notes.txt"), "not a script").expect("write");

    for collection in [
        repo.get("col").expect("get"),
        repo.get_summaries("col").expect("summaries"),
    ] {
        assert_eq!(script_names(&collection.root.items), vec!["utils.js"]);
        let lib = collection
            .root
            .items
            .iter()
            .find_map(|i| match i {
                rocket_collection::CollectionItem::Folder(f) if f.name == "lib" => Some(f),
                _ => None,
            })
            .expect("lib folder");
        assert_eq!(script_names(&lib.items), vec!["helper.js"]);
    }
}

#[test]
fn tree_follows_order_file_for_scripts() {
    let (dir, repo) = setup();
    repo.create("col").expect("create");
    let root = dir.path().join("col");
    fs::write(root.join("a.js"), "").expect("write");
    fs::write(root.join("b.js"), "").expect("write");
    fs::write(root.join("_order.yml"), "- b.js\n- a.js\n").expect("write order");
    let collection = repo.get("col").expect("get");
    assert_eq!(script_names(&collection.root.items), vec!["b.js", "a.js"]);
}

#[test]
fn tree_skips_node_modules_and_dot_files() {
    let (dir, repo) = setup();
    repo.create("col").expect("create");
    let root = dir.path().join("col");
    fs::create_dir_all(root.join("node_modules/pkg")).expect("mkdir");
    fs::write(root.join("node_modules/pkg/index.js"), "").expect("write");
    fs::write(root.join(".hidden.js"), "").expect("write");
    let collection = repo.get("col").expect("get");
    assert!(script_names(&collection.root.items).is_empty());
    assert!(collection
        .root
        .items
        .iter()
        .all(|i| !matches!(i, rocket_collection::CollectionItem::Folder(f) if f.name == "node_modules")));
}

#[cfg(unix)]
#[test]
fn tree_skips_symlinked_js_files() {
    let (dir, repo) = setup();
    repo.create("col").expect("create");
    let root = dir.path().join("col");
    let outside = dir.path().join("outside.js");
    fs::write(&outside, "module.exports = 1;").expect("write");
    std::os::unix::fs::symlink(&outside, root.join("link.js")).expect("symlink");
    let collection = repo.get("col").expect("get");
    assert!(script_names(&collection.root.items).is_empty());
}
```

Run: `cargo test -j4 -p rocket-infra tree_`
Expected: FAIL (no script items listed).

- [ ] **Step 8: Implement the scanner branch**

In `crates/rocket-infra/src/fs_collection/tree.rs`, change the skip condition near the top of the `for entry in entries` loop:

```rust
        if entry_name.starts_with('.')
            || entry_name == "environments"
            || entry_name == "node_modules"
        {
            continue;
        }
```

Replace the final `else if is_request_file(&path) { ... }` with:

```rust
        } else if is_script_file(&path) {
            // Symlinked scripts are skipped, as symlinked directories are above.
            let is_symlink = std::fs::symlink_metadata(&path)
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(true);
            if is_symlink {
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
```

Add this helper at the bottom of the non-test code in `tree.rs`:

```rust
/// True for a `.js` file. Dot-files never reach here because the skip above drops them.
fn is_script_file(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "js")
}
```

and add `ScriptFileItem` to the `rocket_collection` import at the top of the file.

- [ ] **Step 9: Run the checks**

Run: `cargo test -j4 -p rocket-infra tree_ && cargo test -j4 -p rocket-collection && cargo check -j4 --workspace --tests`
Expected: PASS and a clean check.

- [ ] **Step 10: Commit**

Invoke `dev-workflow-skills:1-git-commit` for `crates/rocket-collection/src`, `crates/rocket-infra/src/fs_collection/tree.rs`, `crates/rocket-infra/src/fs_collection/tests.rs`, `crates/rocket-infra/src/conversions/folder.rs`, `crates/rocket-app/src/runner_sequence.rs`, `crates/rocket-app/src/contract_service.rs` (add by path, plus any other file the compiler fixes touched).
Suggested subject: `feat(collection): list .js files as script items`

---

### Task 2: Script file repository operations

**Files:**
- Modify: `crates/rocket-collection/src/repository.rs`
- Create: `crates/rocket-infra/src/fs_collection/script_files.rs`
- Modify: `crates/rocket-infra/src/fs_collection/mod.rs`, `crates/rocket-infra/src/shared_path_collection_repo.rs`
- Test: `crates/rocket-infra/src/fs_collection/tests.rs`

**Interfaces:**
- Consumes: `normalize_script_name`, `SCRIPT_TEMPLATE` from Task 1.
- Produces, on `CollectionRepository` (default bodies return `DomainError::Internal("script files are not supported")`):
  - `fn create_script_file(&self, collection: &str, folder_path: &str, name: &str) -> DomainResult<String>` returns the collection-relative path, for example `lib/utils.js`.
  - `fn read_script_file(&self, collection: &str, path: &str) -> DomainResult<String>`
  - `fn save_script_file(&self, collection: &str, path: &str, content: &str) -> DomainResult<()>`
  - `fn rename_script_file(&self, collection: &str, path: &str, new_name: &str) -> DomainResult<String>` returns the new relative path.
  - `fn delete_script_file(&self, collection: &str, path: &str) -> DomainResult<()>`

- [ ] **Step 1: Read the spec reference**

Read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing tests**

Append to `crates/rocket-infra/src/fs_collection/tests.rs`:

```rust
fn text_of(dir: &TempDir, rel: &str) -> String {
    fs::read_to_string(dir.path().join(rel)).expect("read file")
}

#[test]
fn script_create_writes_template_at_root_and_in_folder() {
    let (dir, repo) = setup();
    repo.create("col").expect("create");
    repo.create_folder("col", "lib").expect("folder");

    let root_path = repo.create_script_file("col", "", "utils").expect("root");
    assert_eq!(root_path, "utils.js");
    let nested = repo.create_script_file("col", "lib", "helper.js").expect("nested");
    assert_eq!(nested, "lib/helper.js");

    let text = text_of(&dir, "col/lib/helper.js");
    assert!(text.contains("module.exports"));
    assert!(text.contains("helper.js"), "template names the file: {text}");
    assert_eq!(
        repo.read_script_file("col", "utils.js").expect("read"),
        text_of(&dir, "col/utils.js")
    );
}

#[test]
fn script_create_rejects_duplicates_bad_names_and_bad_folders() {
    let (dir, repo) = setup();
    repo.create("col").expect("create");
    repo.create_script_file("col", "", "utils").expect("first");
    fs::write(dir.path().join("col/utils.js"), "keep me").expect("overwrite fixture");

    assert!(repo.create_script_file("col", "", "utils").is_err());
    assert_eq!(text_of(&dir, "col/utils.js"), "keep me", "existing file untouched");
    assert!(repo.create_script_file("col", "", "../evil").is_err());
    assert!(repo.create_script_file("col", "", "a/b").is_err());
    assert!(repo.create_script_file("col", "no-such-folder", "x").is_err());
    assert!(repo.create_script_file("col", "../..", "x").is_err());
}

#[test]
fn script_save_and_read_roundtrip_and_reject_non_scripts() {
    let (dir, repo) = setup();
    repo.create("col").expect("create");
    repo.create_script_file("col", "", "utils").expect("create script");

    repo.save_script_file("col", "utils.js", "module.exports = 42;")
        .expect("save");
    assert_eq!(
        repo.read_script_file("col", "utils.js").expect("read"),
        "module.exports = 42;"
    );

    let settings_before = text_of(&dir, "col/opencollection.yml");
    assert!(repo.save_script_file("col", "opencollection.yml", "x").is_err());
    assert_eq!(text_of(&dir, "col/opencollection.yml"), settings_before);
    assert!(repo.save_script_file("col", "missing.js", "x").is_err());
    assert!(
        !dir.path().join("col/missing.js").exists(),
        "save must not create files"
    );
    assert!(repo.save_script_file("col", "../outside.js", "x").is_err());
    assert!(repo.read_script_file("col", "opencollection.yml").is_err());
}

#[cfg(unix)]
#[test]
fn script_ops_reject_symlinks_and_directories() {
    let (dir, repo) = setup();
    repo.create("col").expect("create");
    let outside = dir.path().join("outside.js");
    fs::write(&outside, "secret").expect("write");
    std::os::unix::fs::symlink(&outside, dir.path().join("col/link.js")).expect("symlink");
    fs::create_dir_all(dir.path().join("col/dir.js")).expect("dir named .js");

    assert!(repo.read_script_file("col", "link.js").is_err());
    assert!(repo.save_script_file("col", "link.js", "x").is_err());
    assert!(repo.delete_script_file("col", "link.js").is_err());
    assert_eq!(fs::read_to_string(&outside).expect("read"), "secret");
    assert!(repo.read_script_file("col", "dir.js").is_err());
    assert!(repo.delete_script_file("col", "dir.js").is_err());
}

#[test]
fn script_rename_and_delete() {
    let (dir, repo) = setup();
    repo.create("col").expect("create");
    repo.create_folder("col", "lib").expect("folder");
    repo.create_script_file("col", "lib", "a").expect("a");
    repo.create_script_file("col", "lib", "b").expect("b");
    fs::write(dir.path().join("col/lib/b.js"), "b content").expect("fixture");

    let renamed = repo.rename_script_file("col", "lib/a.js", "c").expect("rename");
    assert_eq!(renamed, "lib/c.js");
    assert!(!dir.path().join("col/lib/a.js").exists());
    assert!(dir.path().join("col/lib/c.js").exists());

    assert!(repo.rename_script_file("col", "lib/c.js", "b").is_err());
    assert_eq!(text_of(&dir, "col/lib/b.js"), "b content", "target untouched");
    assert!(dir.path().join("col/lib/c.js").exists(), "source untouched");
    assert!(repo.rename_script_file("col", "lib/c.js", "../x").is_err());

    repo.delete_script_file("col", "lib/c.js").expect("delete");
    assert!(!dir.path().join("col/lib/c.js").exists());
    assert!(repo.delete_script_file("col", "lib/c.js").is_err());
    assert!(repo.delete_script_file("col", "opencollection.yml").is_err());
    assert!(dir.path().join("col/opencollection.yml").exists());
}
```

Run: `cargo test -j4 -p rocket-infra script_`
Expected: FAIL to compile (methods missing).

- [ ] **Step 3: Add the trait methods**

In `crates/rocket-collection/src/repository.rs`, inside the trait (after `collection_root_path`):

```rust
    /// Creates `name` (`.js` appended when missing) in `folder_path` with starter
    /// content. `folder_path` is relative to the collection root, `""` for the root.
    /// Returns the collection-relative path of the new file.
    fn create_script_file(
        &self,
        _collection: &str,
        _folder_path: &str,
        _name: &str,
    ) -> DomainResult<String> {
        Err(DomainError::Internal("script files are not supported".into()))
    }

    /// Reads a script file by collection-relative path.
    fn read_script_file(&self, _collection: &str, _path: &str) -> DomainResult<String> {
        Err(DomainError::Internal("script files are not supported".into()))
    }

    /// Overwrites an existing script file. Never creates a file.
    fn save_script_file(
        &self,
        _collection: &str,
        _path: &str,
        _content: &str,
    ) -> DomainResult<()> {
        Err(DomainError::Internal("script files are not supported".into()))
    }

    /// Renames a script file inside its folder. Returns the new relative path.
    fn rename_script_file(
        &self,
        _collection: &str,
        _path: &str,
        _new_name: &str,
    ) -> DomainResult<String> {
        Err(DomainError::Internal("script files are not supported".into()))
    }

    /// Deletes a script file.
    fn delete_script_file(&self, _collection: &str, _path: &str) -> DomainResult<()> {
        Err(DomainError::Internal("script files are not supported".into()))
    }
```

- [ ] **Step 4: Implement `script_files.rs`**

Create `crates/rocket-infra/src/fs_collection/script_files.rs`:

```rust
//! Create, read, save, rename and delete `.js` script files in a collection.
//!
//! Every path is validated against the collection directory first. Only regular
//! `.js` files are touched, never symlinks, directories or other file types.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use rocket_collection::{normalize_script_name, Collection, SCRIPT_TEMPLATE};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;

use super::FsCollectionRepo;

/// Resolves an existing script file and checks it is a regular, non-symlink `.js` file.
fn resolve_existing(
    repo: &FsCollectionRepo,
    collection: &str,
    path: &str,
) -> DomainResult<(PathBuf, PathBuf)> {
    Collection::validate_name(collection)?;
    let collection_dir = repo.collection_path(collection);
    let full = repo.validate_path(&collection_dir, Path::new(path))?;
    if full.extension().and_then(|e| e.to_str()) != Some("js") {
        return Err(DomainError::InvalidInput(
            "Only .js script files can be used here".into(),
        ));
    }
    // Check the unresolved path too, since `validate_path` follows symlinks.
    let unresolved = collection_dir.join(path);
    let meta = fs::symlink_metadata(&unresolved)
        .map_err(|_| DomainError::NotFound(format!("Script file '{path}' not found")))?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err(DomainError::InvalidInput(format!(
            "'{path}' is not a regular script file"
        )));
    }
    Ok((collection_dir, full))
}

fn relative_path(folder_path: &str, file_name: &str) -> String {
    let folder = folder_path.trim_matches('/');
    if folder.is_empty() {
        file_name.to_string()
    } else {
        format!("{folder}/{file_name}")
    }
}

pub(super) fn create_script_file(
    repo: &FsCollectionRepo,
    collection: &str,
    folder_path: &str,
    name: &str,
) -> DomainResult<String> {
    Collection::validate_name(collection)?;
    let file_name = normalize_script_name(name)?;
    let collection_dir = repo.collection_path(collection);
    let folder = repo.validate_path(&collection_dir, Path::new(folder_path))?;
    if !folder.is_dir() {
        return Err(DomainError::NotFound(format!(
            "Folder '{folder_path}' not found"
        )));
    }
    let mutex = repo.collection_mutex(collection);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    let target = folder.join(&file_name);
    let template = SCRIPT_TEMPLATE.replace("THIS_FILE.js", &file_name);
    // `create_new` fails when the file exists, so an existing script is never overwritten.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                DomainError::InvalidInput(format!("'{file_name}' already exists"))
            } else {
                DomainError::from(e)
            }
        })?;
    file.write_all(template.as_bytes())?;
    Ok(relative_path(folder_path, &file_name))
}

pub(super) fn read_script_file(
    repo: &FsCollectionRepo,
    collection: &str,
    path: &str,
) -> DomainResult<String> {
    let (_, full) = resolve_existing(repo, collection, path)?;
    fs::read_to_string(&full).map_err(|e| {
        if e.kind() == std::io::ErrorKind::InvalidData {
            DomainError::InvalidInput(format!("'{path}' is not valid UTF-8 text"))
        } else {
            DomainError::from(e)
        }
    })
}

pub(super) fn save_script_file(
    repo: &FsCollectionRepo,
    collection: &str,
    path: &str,
    content: &str,
) -> DomainResult<()> {
    let (_, full) = resolve_existing(repo, collection, path)?;
    let mutex = repo.collection_mutex(collection);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    atomic_write(&full, content.as_bytes())?;
    Ok(())
}

pub(super) fn rename_script_file(
    repo: &FsCollectionRepo,
    collection: &str,
    path: &str,
    new_name: &str,
) -> DomainResult<String> {
    let (_, full) = resolve_existing(repo, collection, path)?;
    let new_file_name = normalize_script_name(new_name)?;
    let parent = full
        .parent()
        .ok_or_else(|| DomainError::InvalidInput("Script has no parent folder".into()))?;
    let target = parent.join(&new_file_name);
    let mutex = repo.collection_mutex(collection);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    if target.exists() {
        return Err(DomainError::InvalidInput(format!(
            "'{new_file_name}' already exists"
        )));
    }
    fs::rename(&full, &target)?;
    let folder_part = Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    Ok(relative_path(&folder_part, &new_file_name))
}

pub(super) fn delete_script_file(
    repo: &FsCollectionRepo,
    collection: &str,
    path: &str,
) -> DomainResult<()> {
    let (_, full) = resolve_existing(repo, collection, path)?;
    let mutex = repo.collection_mutex(collection);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    fs::remove_file(&full)?;
    Ok(())
}
```

If `DomainError` has no `From<std::io::Error>`, replace `DomainError::from(e)` with the mapping used elsewhere in `fs_collection`. The existing `?` on `fs::create_dir_all` in `folders.rs` shows the conversion exists, so `From` should be there. If `atomic_write`'s signature differs from `(&Path, &[u8])`, follow how `settings.rs::save_settings` calls it.

In `crates/rocket-infra/src/fs_collection/mod.rs` add `mod script_files;` with the other `mod` lines and these methods inside `impl CollectionRepository for FsCollectionRepo`:

```rust
    fn create_script_file(
        &self,
        collection: &str,
        folder_path: &str,
        name: &str,
    ) -> DomainResult<String> {
        script_files::create_script_file(self, collection, folder_path, name)
    }

    fn read_script_file(&self, collection: &str, path: &str) -> DomainResult<String> {
        script_files::read_script_file(self, collection, path)
    }

    fn save_script_file(&self, collection: &str, path: &str, content: &str) -> DomainResult<()> {
        script_files::save_script_file(self, collection, path, content)
    }

    fn rename_script_file(
        &self,
        collection: &str,
        path: &str,
        new_name: &str,
    ) -> DomainResult<String> {
        script_files::rename_script_file(self, collection, path, new_name)
    }

    fn delete_script_file(&self, collection: &str, path: &str) -> DomainResult<()> {
        script_files::delete_script_file(self, collection, path)
    }
```

In `shared_path_collection_repo.rs` add the same five signatures delegating to `self.repo().<method>(...)`.

- [ ] **Step 5: Run the tests**

Run: `cargo test -j4 -p rocket-infra script_ && cargo test -j4 -p rocket-infra tree_`
Expected: PASS.

- [ ] **Step 6: Commit**

Invoke `dev-workflow-skills:1-git-commit` for `crates/rocket-collection/src/repository.rs`, `crates/rocket-infra/src/fs_collection/script_files.rs`, `crates/rocket-infra/src/fs_collection/mod.rs`, `crates/rocket-infra/src/fs_collection/tests.rs`, `crates/rocket-infra/src/shared_path_collection_repo.rs`.
Suggested subject: `feat(collection): add script file repository operations`

---

### Task 3: Service, Tauri commands and TypeScript bindings

**Files:**
- Modify: `crates/rocket-app/src/collection_service.rs`, `src-tauri/src/commands/collections.rs`, `src-tauri/src/lib.rs`, `src/lib/tauri-api.ts`
- Test: `yarn tsc --noEmit`, `cargo check -j4 --workspace --tests`

**Interfaces:**
- Consumes: the five trait methods from Task 2.
- Produces:
  - `CollectionService::{create_script_file, read_script_file, save_script_file, rename_script_file, delete_script_file}` with the same arguments as the trait.
  - Tauri commands `create_script_file(collection, folder_path, name) -> String`, `read_script_file(collection, path) -> String`, `save_script_file(collection, path, content)`, `rename_script_file(collection, path, new_name) -> String`, `delete_script_file(collection, path)`.
  - TypeScript: `ScriptFileItem`, `createScriptFile`, `readScriptFile`, `saveScriptFile`, `renameScriptFile`, `deleteScriptFile`, and the `scriptFile` member of `CollectionItem`.

- [ ] **Step 1: Read the spec reference**

Read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Add the service methods**

In `crates/rocket-app/src/collection_service.rs`, after `delete_folder`, add:

```rust
    /// Creates a starter `.js` file. The file watcher reports the change to the UI.
    pub fn create_script_file(
        &self,
        collection: &str,
        folder_path: &str,
        name: &str,
    ) -> DomainResult<String> {
        self.repo.create_script_file(collection, folder_path, name)
    }

    pub fn read_script_file(&self, collection: &str, path: &str) -> DomainResult<String> {
        self.repo.read_script_file(collection, path)
    }

    pub fn save_script_file(
        &self,
        collection: &str,
        path: &str,
        content: &str,
    ) -> DomainResult<()> {
        self.repo.save_script_file(collection, path, content)
    }

    pub fn rename_script_file(
        &self,
        collection: &str,
        path: &str,
        new_name: &str,
    ) -> DomainResult<String> {
        self.repo.rename_script_file(collection, path, new_name)
    }

    pub fn delete_script_file(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.repo.delete_script_file(collection, path)
    }
```

- [ ] **Step 3: Add the Tauri commands**

In `src-tauri/src/commands/collections.rs`, after `delete_folder`:

```rust
#[tauri::command]
pub fn create_script_file(
    collection: String,
    folder_path: String,
    name: String,
    svc: State<'_, CollectionService>,
) -> Result<String, DomainError> {
    svc.create_script_file(&collection, &folder_path, &name)
}

#[tauri::command]
pub fn read_script_file(
    collection: String,
    path: String,
    svc: State<'_, CollectionService>,
) -> Result<String, DomainError> {
    svc.read_script_file(&collection, &path)
}

#[tauri::command]
pub fn save_script_file(
    collection: String,
    path: String,
    content: String,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    svc.save_script_file(&collection, &path, &content)
}

#[tauri::command]
pub fn rename_script_file(
    collection: String,
    path: String,
    new_name: String,
    svc: State<'_, CollectionService>,
) -> Result<String, DomainError> {
    svc.rename_script_file(&collection, &path, &new_name)
}

#[tauri::command]
pub fn delete_script_file(
    collection: String,
    path: String,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    svc.delete_script_file(&collection, &path)
}
```

In `src-tauri/src/lib.rs`, after `commands::collections::delete_folder,` (line ~579) register:

```rust
            commands::collections::create_script_file,
            commands::collections::read_script_file,
            commands::collections::save_script_file,
            commands::collections::rename_script_file,
            commands::collections::delete_script_file,
```

- [ ] **Step 4: Add the TypeScript bindings**

In `src/lib/tauri-api.ts`, add above `export type CollectionItem`:

```ts
export interface ScriptFileItem {
  fileName: string;
  name: string;
}
```

and extend the union:

```ts
export type CollectionItem =
  | ({ type: 'request' } & Request)
  | ({ type: 'folder' } & Folder)
  | ({ type: 'summary' } & RequestSummary)
  | ({ type: 'opaque' } & OpaqueProtocolItem)
  | ({ type: 'scriptFile' } & ScriptFileItem);
```

Below `deleteFolder` add:

```ts
export const createScriptFile = (collection: string, folderPath: string, name: string) =>
  invoke<string>('create_script_file', { collection, folderPath, name });

export const readScriptFile = (collection: string, path: string) =>
  invoke<string>('read_script_file', { collection, path });

export const saveScriptFile = (collection: string, path: string, content: string) =>
  invoke<void>('save_script_file', { collection, path, content });

export const renameScriptFile = (collection: string, path: string, newName: string) =>
  invoke<string>('rename_script_file', { collection, path, newName });

export const deleteScriptFile = (collection: string, path: string) =>
  invoke<void>('delete_script_file', { collection, path });
```

- [ ] **Step 5: Fix TypeScript fallout**

Run: `yarn tsc --noEmit`
Expected: possibly FAIL where code narrows `CollectionItem` exhaustively (for example `src/lib/collection-utils.ts`, `src/components/collections/tree-utils.ts`, the runner entry flattening in `src/stores/pane-store.ts`). For each error, make script files be ignored (they are not requests or folders). Do not add UI yet; that is Plan 03. Re-run until clean.

- [ ] **Step 6: Run the checks**

Run: `cargo check -j4 --workspace --tests && yarn tsc --noEmit && yarn check`
Expected: all clean.

- [ ] **Step 7: Commit**

Invoke `dev-workflow-skills:1-git-commit` for `crates/rocket-app/src/collection_service.rs`, `src-tauri/src/commands/collections.rs`, `src-tauri/src/lib.rs`, `src/lib/tauri-api.ts`, plus any TypeScript file Step 5 touched.
Suggested subject: `feat(ipc): add script file commands and bindings`

---

## Plan 02 Self-Review

- Spec "Domain and scanner" covered in Task 1 (variant, `.js` branch, `_order.yml`, `node_modules`, symlinks, `.yml` ScriptFile still skipped).
- Spec "Commands" covered in Tasks 2 and 3 (all five commands, `validate_path`, template, name rules, atomic save).
- The service layer is a one-line delegation with no logic, so it has no unit test of its own. The behaviour is tested at the repository level in Task 2.
- Names used across tasks: `ScriptFileItem`, `normalize_script_name`, `SCRIPT_TEMPLATE`, `create_script_file`, `read_script_file`, `save_script_file`, `rename_script_file`, `delete_script_file`, `scriptFile`.
- Verification after the plan: `cargo check -j4 --workspace --tests`, `cargo test -j4 -p rocket-collection`, `cargo test -j4 -p rocket-infra script_ tree_`, `yarn tsc --noEmit`, `yarn check`.
