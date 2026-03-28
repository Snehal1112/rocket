# OC-P13: Migrate .uid Files Into YAML Metadata

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove standalone `.uid` files by storing the `uid` field inside `opencollection.yml` (for collections) and `folder.yml` (for folders). Migrate existing `.uid` values on first read.

**Architecture:** Add `uid: Option<String>` to `OcCollection` and `OcFolderInfo`. Replace `read_or_create_uid()` with YAML-based UID read/write. On first read of an old collection, migrate the `.uid` value into the YAML file and delete the `.uid` file.

**Tech Stack:** Rust, serde_yaml

**Prerequisite:** OC-P12 complete.

---

## Task 1: Add `uid` field to OcCollection and OcFolderInfo

**Files:**
- Modify: `crates/rocket-infra/src/opencollection.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn oc_collection_with_uid_yaml() {
    let yaml = "opencollection: \"0.1\"\nuid: \"550e8400-e29b-41d4-a716-446655440000\"\ninfo:\n  name: My API";
    let col: OcCollection = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(col.uid, Some("550e8400-e29b-41d4-a716-446655440000".into()));
}

#[test]
fn oc_folder_info_with_uid_yaml() {
    let yaml = "name: auth\nuid: \"abcd-1234\"\ntype: folder";
    let info: OcFolderInfo = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(info.uid, Some("abcd-1234".into()));
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p rocket-infra -- opencollection::tests::oc_collection_with_uid
cargo test -p rocket-infra -- opencollection::tests::oc_folder_info_with_uid
```
Expected: FAIL.

- [ ] **Step 3: Add `uid` field to both structs**

In `OcCollection`, add after the `opencollection` field:
```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
```

In `OcFolderInfo`, add after the `name` field:
```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
```

- [ ] **Step 4: Fix all code that constructs OcCollection or OcFolderInfo** — add `uid: None` or `uid: Some(...)` to every construction site.

Search for construction sites:
```bash
grep -rn "OcCollection {" crates/rocket-infra/src/
grep -rn "OcFolderInfo {" crates/rocket-infra/src/
```

Known sites:
- `fs_collection_repo.rs` `create()` — constructs `OcCollection` → set `uid: Some(uuid::Uuid::new_v4().to_string())`
- `fs_collection_repo.rs` `create_folder()` — constructs `OcFolderInfo` → set `uid: Some(uuid::Uuid::new_v4().to_string())`
- `oc_conversions.rs` `folder_to_oc_folder()` — constructs `OcFolderInfo` → set `uid: None`
- `oc_conversions.rs` `collection_to_oc_collection()` — constructs `OcCollection` → set `uid: None`
- Any test that constructs these types

- [ ] **Step 5: Run tests**

```bash
cargo test -p rocket-infra -- opencollection::tests
```
Expected: ALL PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-infra/src/
git commit -m "feat(infra): add uid field to OcCollection and OcFolderInfo"
```

---

## Task 2: Replace `read_or_create_uid()` with YAML-based UID management

**Files:**
- Modify: `crates/rocket-infra/src/fs_collection_repo.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Write a new helper `read_uid_from_yaml()` that reads uid from opencollection.yml or folder.yml**

```rust
/// Read UID from a YAML metadata file (opencollection.yml or folder.yml).
/// If no UID exists in the file, generate one and write it back.
/// Falls back to reading legacy .uid file, migrating the value into the YAML.
fn read_uid_from_yaml(dir: &Path) -> String {
    // Try opencollection.yml first (collection root).
    let oc_path = dir.join("opencollection.yml");
    if oc_path.exists() {
        if let Ok(content) = fs::read_to_string(&oc_path) {
            if let Ok(mut oc) = serde_yaml::from_str::<OcCollection>(&content) {
                if let Some(uid) = &oc.uid {
                    if !uid.is_empty() {
                        return uid.clone();
                    }
                }
                // No UID — check for legacy .uid file.
                let uid = read_legacy_uid(dir);
                oc.uid = Some(uid.clone());
                if let Ok(yaml) = serde_yaml::to_string(&oc) {
                    let _ = fs::write(&oc_path, yaml);
                }
                cleanup_legacy_uid(dir);
                return uid;
            }
        }
    }

    // Try folder.yml (subfolder).
    let folder_path = dir.join("folder.yml");
    if folder_path.exists() {
        if let Ok(content) = fs::read_to_string(&folder_path) {
            if let Ok(mut info) = serde_yaml::from_str::<OcFolderInfo>(&content) {
                if let Some(uid) = &info.uid {
                    if !uid.is_empty() {
                        return uid.clone();
                    }
                }
                let uid = read_legacy_uid(dir);
                info.uid = Some(uid.clone());
                if let Ok(yaml) = serde_yaml::to_string(&info) {
                    let _ = fs::write(&folder_path, yaml);
                }
                cleanup_legacy_uid(dir);
                return uid;
            }
        }
    }

    // No YAML metadata file — fall back to legacy .uid.
    read_legacy_uid(dir)
}

/// Read UID from legacy .uid file, or generate a new one.
fn read_legacy_uid(dir: &Path) -> String {
    let uid_path = dir.join(".uid");
    if let Ok(uid) = fs::read_to_string(&uid_path) {
        let trimmed = uid.trim().to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }
    uuid::Uuid::new_v4().to_string()
}

/// Delete the legacy .uid file if it exists.
fn cleanup_legacy_uid(dir: &Path) {
    let uid_path = dir.join(".uid");
    if uid_path.exists() {
        let _ = fs::remove_file(&uid_path);
    }
}
```

- [ ] **Step 2: Replace all `read_or_create_uid()` calls with `read_uid_from_yaml()`**

Three call sites in `fs_collection_repo.rs`:
1. `list()` line ~101: `let uid = read_or_create_uid(&path);` → `let uid = read_uid_from_yaml(&path);`
2. `create_folder()` line ~296: `read_or_create_uid(&dir_path);` → remove this line (UID is now written inside folder.yml in the `OcFolderInfo` constructor below it)
3. `build_folder_tree()` line ~437: `folder.uid = read_or_create_uid(current);` → `folder.uid = read_uid_from_yaml(current);`

- [ ] **Step 3: Update `create()` to include UID in opencollection.yml**

The `create()` function already writes `opencollection.yml`. Update it to include a UID:

```rust
let uid = uuid::Uuid::new_v4().to_string();
let oc = OcCollection {
    opencollection: Some("0.1".into()),
    uid: Some(uid),
    info: Some(OcInfo { name: name.into(), summary: None, version: None, authors: None }),
    // ... rest unchanged ...
};
```

- [ ] **Step 4: Update `create_folder()` to include UID in folder.yml**

```rust
let uid = uuid::Uuid::new_v4().to_string();
let info = OcFolderInfo {
    name: folder_name,
    uid: Some(uid),
    description: None,
    folder_type: Some("folder".into()),
    seq: None,
    tags: Vec::new(),
};
```

Remove the `read_or_create_uid(&dir_path);` call since UID is now in folder.yml.

- [ ] **Step 5: Delete the old `read_or_create_uid()` function**

Remove the function definition (lines 10-22). It is fully replaced by `read_uid_from_yaml()`.

- [ ] **Step 6: Add `.uid` to `is_request_file()` exclusion list** (so any leftover .uid files are not treated as requests)

The `.uid` files start with `.` so they are already skipped by the `entry_name.starts_with('.')` check in `build_folder_tree()`. No change needed here.

- [ ] **Step 7: Run all tests**

```bash
cargo test -p rocket-infra -- fs_collection_repo::tests
```
Expected: ALL PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/rocket-infra/src/fs_collection_repo.rs
git commit -m "feat(infra): migrate .uid into opencollection.yml and folder.yml"
```

---

## Task 3: Add migration tests + cleanup

**Files:**
- Modify: `crates/rocket-infra/src/fs_collection_repo.rs` (tests section)

- [ ] **Step 1: Write migration test — legacy .uid migrated into opencollection.yml**

```rust
#[test]
fn legacy_uid_migrated_into_opencollection_yml() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    let col_dir = dir.path().join("my-api");

    // Simulate a legacy collection: write .uid file and remove uid from opencollection.yml.
    let legacy_uid = "legacy-uid-12345";
    fs::write(col_dir.join(".uid"), legacy_uid).unwrap();

    // Re-read the opencollection.yml, strip uid, rewrite.
    let content = fs::read_to_string(col_dir.join("opencollection.yml")).unwrap();
    let mut oc: OcCollection = serde_yaml::from_str(&content).unwrap();
    oc.uid = None;
    let yaml = serde_yaml::to_string(&oc).unwrap();
    fs::write(col_dir.join("opencollection.yml"), yaml).unwrap();

    // Now list — should migrate .uid into opencollection.yml.
    let list = repo.list().unwrap();
    assert_eq!(list[0].uid, legacy_uid);

    // Verify .uid file was deleted.
    assert!(!col_dir.join(".uid").exists());

    // Verify opencollection.yml now contains the uid.
    let content = fs::read_to_string(col_dir.join("opencollection.yml")).unwrap();
    assert!(content.contains(legacy_uid));
}
```

- [ ] **Step 2: Write migration test — legacy .uid migrated into folder.yml**

```rust
#[test]
fn legacy_uid_migrated_into_folder_yml() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "auth").unwrap();
    let folder_dir = dir.path().join("my-api/auth");

    // Simulate legacy: write .uid, strip uid from folder.yml.
    let legacy_uid = "folder-uid-67890";
    fs::write(folder_dir.join(".uid"), legacy_uid).unwrap();

    let content = fs::read_to_string(folder_dir.join("folder.yml")).unwrap();
    let mut info: OcFolderInfo = serde_yaml::from_str(&content).unwrap();
    info.uid = None;
    let yaml = serde_yaml::to_string(&info).unwrap();
    fs::write(folder_dir.join("folder.yml"), yaml).unwrap();

    // Load the collection — build_folder_tree should migrate.
    let col = repo.get("my-api").unwrap();
    let auth_folder = col.root.find_folder("auth").unwrap();
    assert_eq!(auth_folder.uid, legacy_uid);

    // Verify .uid file was deleted.
    assert!(!folder_dir.join(".uid").exists());

    // Verify folder.yml now contains the uid.
    let content = fs::read_to_string(folder_dir.join("folder.yml")).unwrap();
    assert!(content.contains(legacy_uid));
}
```

- [ ] **Step 3: Write test — no .uid file created on new collection**

```rust
#[test]
fn no_uid_file_created_on_new_collection() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    assert!(!dir.path().join("my-api/.uid").exists());
    // UID should be in opencollection.yml instead.
    let content = fs::read_to_string(dir.path().join("my-api/opencollection.yml")).unwrap();
    assert!(content.contains("uid:"));
}
```

- [ ] **Step 4: Write test — no .uid file created on new folder**

```rust
#[test]
fn no_uid_file_created_on_new_folder() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "auth").unwrap();
    assert!(!dir.path().join("my-api/auth/.uid").exists());
    let content = fs::read_to_string(dir.path().join("my-api/auth/folder.yml")).unwrap();
    assert!(content.contains("uid:"));
}
```

- [ ] **Step 5: Run full workspace tests**

```bash
cargo test --workspace
```
Expected: ALL PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-infra/src/fs_collection_repo.rs
git commit -m "test(infra): migration tests for .uid → YAML, verify no .uid files created"
```

---

## Milestone Checklist — OC-P13

- [ ] `OcCollection` has `uid: Option<String>` field
- [ ] `OcFolderInfo` has `uid: Option<String>` field
- [ ] `create()` writes UID inside `opencollection.yml`
- [ ] `create_folder()` writes UID inside `folder.yml`
- [ ] No `.uid` files created for new collections/folders
- [ ] Legacy `.uid` files migrated into YAML on first read
- [ ] Legacy `.uid` files deleted after migration
- [ ] `read_or_create_uid()` function removed
- [ ] `cargo test --workspace` — all pass
