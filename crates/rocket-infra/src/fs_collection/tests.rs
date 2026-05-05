use super::*;
use std::fs;
use std::sync::{Arc, Mutex};
use dashmap::DashMap;
use rocket_collection::{CollectionRepository, CollectionSettings, CollectionVariable};
use rocket_shared::types::HttpMethod;
use tempfile::TempDir;

fn setup() -> (TempDir, FsCollectionRepo) {
    let dir = TempDir::new().unwrap();
    let repo = FsCollectionRepo::new(dir.path().to_path_buf(), Arc::new(DashMap::new()));
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
    let req = rocket_collection::Request::new("Get Users", HttpMethod::Get, "https://api.example.com/users");
    repo.save_request("my-api", "get-users.yml", &req).unwrap();
    let loaded = repo.get_request("my-api", "get-users.yml").unwrap();
    assert_eq!(loaded.name, "Get Users");
}

#[test]
fn save_request_in_subfolder() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    let req = rocket_collection::Request::new("Login", HttpMethod::Post, "/login");
    repo.save_request("my-api", "auth/login.yml", &req).unwrap();
    let loaded = repo.get_request("my-api", "auth/login.yml").unwrap();
    assert_eq!(loaded.name, "Login");
}

#[test]
fn delete_request() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    let req = rocket_collection::Request::new("Test", HttpMethod::Get, "/test");
    repo.save_request("my-api", "test.yml", &req).unwrap();
    repo.delete_request("my-api", "test.yml").unwrap();
    assert!(repo.get_request("my-api", "test.yml").is_err());
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
    let req = rocket_collection::Request::new("Test", HttpMethod::Get, "/test");
    repo.save_request("my-api", "old/test.yml", &req).unwrap();
    repo.move_item("my-api", "old/test.yml", "my-api", "new/test.yml").unwrap();
    assert!(repo.get_request("my-api", "old/test.yml").is_err());
    assert!(repo.get_request("my-api", "new/test.yml").is_ok());
}

#[test]
fn settings_default_when_no_file() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    let settings = repo.get_settings("my-api").unwrap();
    assert_eq!(settings, rocket_collection::CollectionSettings::default());
    assert!(settings.auth.is_none());
    assert!(settings.headers.is_empty());
}

#[test]
fn settings_roundtrip() {
    use rocket_shared::types::{Auth, Header};

    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();

    let original = rocket_collection::CollectionSettings {
        docs: None,
        auth: Some(Auth::Bearer { token: "tok_abc".into() }),
        headers: vec![Header::new("X-Tenant", "acme")],
        variables: vec![],
    };
    repo.save_settings("my-api", &original).unwrap();
    let loaded = repo.get_settings("my-api").unwrap();
    assert_eq!(loaded, original);
}

#[test]
fn settings_file_not_counted_as_request() {
    use rocket_shared::types::Auth;

    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();

    // Save settings, then verify the request count stays zero.
    let settings = rocket_collection::CollectionSettings {
        docs: None,
        auth: Some(Auth::None),
        headers: vec![],
        variables: vec![],
    };
    repo.save_settings("my-api", &settings).unwrap();

    let list = repo.list().unwrap();
    assert_eq!(list[0].request_count, 0);
}

#[test]
fn settings_stored_in_opencollection_yml() {
    use rocket_shared::types::{Auth, Header};

    let (dir, repo) = setup();
    repo.create("my-api").unwrap();

    let settings = CollectionSettings {
        docs: Some("My API docs".into()),
        auth: Some(Auth::Bearer { token: "tok".into() }),
        headers: vec![Header::new("X-Tenant", "acme")],
        variables: vec![],
    };
    repo.save_settings("my-api", &settings).unwrap();

    // Should NOT have collection.json.
    assert!(!dir.path().join("my-api/collection.json").exists());

    // opencollection.yml should contain the settings.
    let content = fs::read_to_string(dir.path().join("my-api/opencollection.yml")).unwrap();
    assert!(content.contains("X-Tenant"));

    // Round-trip.
    let loaded = repo.get_settings("my-api").unwrap();
    assert_eq!(loaded.auth, settings.auth);
    assert_eq!(loaded.headers.len(), 1);
    assert_eq!(loaded.docs, Some("My API docs".into()));
}

#[test]
fn folder_uid_and_name_are_loaded_from_single_parse() {
    use rocket_collection::CollectionItem;

    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "auth").unwrap();
    // get() must load the folder's UID and name without error.
    let col = repo.get("my-api").unwrap();
    let auth_folder = col.root.items.iter().find_map(|item| {
        if let CollectionItem::Folder(f) = item {
            if f.dir_name.as_deref() == Some("auth") {
                return Some(f);
            }
        }
        None
    });
    assert!(auth_folder.is_some(), "auth folder not found in tree");
    let auth = auth_folder.unwrap();
    // UID must be a non-empty string (generated on create).
    assert!(!auth.uid.is_empty(), "folder uid must not be empty");
}

#[test]
fn path_traversal_in_get_request_is_rejected() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    let result = repo.get_request("my-api", "../../etc/passwd");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, DomainError::InvalidInput(_) | DomainError::NotFound(_)),
        "expected traversal to be blocked, got {:?}",
        err
    );
}

#[test]
fn path_traversal_in_save_request_is_rejected() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    let req = rocket_collection::Request::new("Bad", rocket_shared::types::HttpMethod::Get, "/bad");
    let result = repo.save_request("my-api", "../../evil.yml", &req);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, DomainError::InvalidInput(_) | DomainError::NotFound(_)),
        "expected traversal to be blocked, got {:?}",
        err
    );
}

#[test]
fn path_traversal_in_delete_request_is_rejected() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    let result = repo.delete_request("my-api", "../../etc/passwd");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, DomainError::InvalidInput(_) | DomainError::NotFound(_)),
        "expected traversal to be blocked, got {:?}",
        err
    );
}

#[test]
fn path_traversal_in_create_folder_is_rejected() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    let result = repo.create_folder("my-api", "../../evil-dir");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, DomainError::InvalidInput(_) | DomainError::NotFound(_)),
        "expected traversal to be blocked, got {:?}",
        err
    );
}

#[test]
fn path_traversal_in_delete_folder_is_rejected() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    let result = repo.delete_folder("my-api", "../../etc");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, DomainError::InvalidInput(_) | DomainError::NotFound(_)),
        "expected traversal to be blocked, got {:?}",
        err
    );
}

#[test]
fn path_traversal_in_move_item_src_is_rejected() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    let result = repo.move_item("my-api", "../../etc/passwd", "my-api", "dest.yml");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, DomainError::InvalidInput(_) | DomainError::NotFound(_)),
        "expected traversal to be blocked, got {:?}",
        err
    );
}

#[test]
fn reorder_items_writes_order_file_and_get_respects_it() {
    use rocket_collection::CollectionItem;

    fn item_name(item: &CollectionItem) -> &str {
        match item {
            CollectionItem::Request(r) => r.name.as_str(),
            CollectionItem::Folder(f) => f.name.as_str(),
            CollectionItem::OpaqueItem(o) => o.name.as_str(),
        }
    }

    let (_dir, repo) = setup();
    repo.create("test-col").unwrap();

    // Create two requests. Alphabetically "aaa" comes before "bbb".
    let req_a = rocket_collection::Request::new("AAA", HttpMethod::Get, "/a");
    let req_b = rocket_collection::Request::new("BBB", HttpMethod::Get, "/b");
    repo.save_request("test-col", "aaa.yml", &req_a).unwrap();
    repo.save_request("test-col", "bbb.yml", &req_b).unwrap();

    // Confirm default (alphabetical) order: aaa first.
    let col = repo.get("test-col").unwrap();
    assert_eq!(item_name(&col.root.items[0]), "AAA");
    assert_eq!(item_name(&col.root.items[1]), "BBB");

    // Reorder so bbb comes first.
    repo.reorder_items(
        "test-col",
        "",
        &["bbb.yml".to_string(), "aaa.yml".to_string()],
    )
    .unwrap();

    // After reorder, bbb should appear first.
    let col = repo.get("test-col").unwrap();
    assert_eq!(item_name(&col.root.items[0]), "BBB");
    assert_eq!(item_name(&col.root.items[1]), "AAA");
}

#[test]
fn path_traversal_in_reorder_items_is_rejected() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    let result = repo.reorder_items("my-api", "../../evil", &["x.yml".to_string()]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, DomainError::InvalidInput(_) | DomainError::NotFound(_)),
        "expected traversal to be blocked, got {:?}",
        err
    );
}

#[test]
fn path_traversal_in_move_item_dst_is_rejected() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    let req = rocket_collection::Request::new("T", rocket_shared::types::HttpMethod::Get, "/t");
    repo.save_request("my-api", "src.yml", &req).unwrap();
    let result = repo.move_item("my-api", "src.yml", "my-api", "../../evil.yml");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, DomainError::InvalidInput(_) | DomainError::NotFound(_)),
        "expected traversal to be blocked, got {:?}",
        err
    );
}

#[test]
fn folder_yml_exists_after_create_folder() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "auth").unwrap();
    assert!(dir.path().join("my-api/auth/folder.yml").exists());
}

#[test]
fn folder_yml_not_counted_as_request() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "auth").unwrap();
    let list = repo.list().unwrap();
    assert_eq!(list[0].request_count, 0);
}

#[test]
fn list_ignores_dirs_without_opencollection_yml() {
    let (dir, repo) = setup();
    // Create a plain directory (not via repo.create).
    fs::create_dir(dir.path().join("plain-dir")).unwrap();
    // Create a proper collection.
    repo.create("proper").unwrap();
    let list = repo.list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "proper");
}

#[test]
fn opencollection_yml_exists_after_create() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    assert!(dir.path().join("my-api/opencollection.yml").exists());
}

#[test]
fn environments_dir_not_shown_in_collection_tree() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    // Simulate the environments directory created by the env service.
    fs::create_dir_all(dir.path().join("my-api/environments")).unwrap();
    let col = repo.get("my-api").unwrap();
    assert!(
        !col.root.subfolder_names().contains(&"environments"),
        "environments/ should not appear in the collection tree"
    );
}

#[test]
fn opencollection_yml_not_counted_as_request() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    let list = repo.list().unwrap();
    assert_eq!(list[0].request_count, 0);
}

#[test]
fn legacy_uid_migrated_into_opencollection_yml() {
    use crate::oc::OcCollection;

    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    let col_dir = dir.path().join("my-api");

    // Simulate a legacy collection: write .uid file and remove uid from opencollection.yml.
    let legacy_uid = "legacy-uid-12345";
    fs::write(col_dir.join(".uid"), legacy_uid).unwrap();

    // Re-read opencollection.yml, strip uid, rewrite.
    let content = fs::read_to_string(col_dir.join("opencollection.yml")).unwrap();
    let mut oc: OcCollection = serde_yaml::from_str(&content).unwrap();
    oc.uid = None;
    let yaml = serde_yaml::to_string(&oc).unwrap();
    fs::write(col_dir.join("opencollection.yml"), yaml).unwrap();

    // List should trigger migration.
    let list = repo.list().unwrap();
    assert_eq!(list[0].uid, legacy_uid);

    // .uid file should be deleted.
    assert!(!col_dir.join(".uid").exists());

    // opencollection.yml should now contain the uid.
    let content = fs::read_to_string(col_dir.join("opencollection.yml")).unwrap();
    assert!(content.contains(legacy_uid));
}

#[test]
fn legacy_uid_migrated_into_folder_yml() {
    use crate::oc::OcFolderInfo;

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

    // Load the collection — build_folder_tree should trigger migration.
    let col = repo.get("my-api").unwrap();
    let auth_folder = col.root.find_folder("auth").unwrap();
    assert_eq!(auth_folder.uid, legacy_uid);

    // .uid file should be deleted.
    assert!(!folder_dir.join(".uid").exists());

    // folder.yml should now contain the uid.
    let content = fs::read_to_string(folder_dir.join("folder.yml")).unwrap();
    assert!(content.contains(legacy_uid));
}

#[test]
fn no_uid_file_created_on_new_collection() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    // No .uid file should exist.
    assert!(!dir.path().join("my-api/.uid").exists());
    // UID should be in opencollection.yml.
    let content = fs::read_to_string(dir.path().join("my-api/opencollection.yml")).unwrap();
    assert!(content.contains("uid:"));
}

#[test]
fn no_uid_file_created_on_new_folder() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "auth").unwrap();
    // No .uid file should exist.
    assert!(!dir.path().join("my-api/auth/.uid").exists());
    // UID should be in folder.yml.
    let content = fs::read_to_string(dir.path().join("my-api/auth/folder.yml")).unwrap();
    assert!(content.contains("uid:"));
}

#[test]
fn legacy_json_collection_auto_migrated_on_list() {
    let (dir, repo) = setup();
    let col_dir = dir.path().join("legacy-api");
    fs::create_dir(&col_dir).unwrap();

    // Write a legacy JSON request (no opencollection.yml).
    let json = r#"{"uid":"999","name":"Old Request","method":"GET","url":"/old","headers":[],"body":null,"auth":{"authType":"none"}}"#;
    fs::write(col_dir.join("old-request.json"), json).unwrap();

    // list() should detect and migrate.
    let list = repo.list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "legacy-api");

    // Verify migration happened.
    assert!(col_dir.join("opencollection.yml").exists());
    assert!(col_dir.join("old-request.yml").exists());
    assert!(!col_dir.join("old-request.json").exists());
}

#[test]
fn legacy_json_collection_auto_migrated_on_get() {
    let (dir, repo) = setup();
    let col_dir = dir.path().join("legacy-api");
    fs::create_dir(&col_dir).unwrap();

    let json = r#"{"uid":"888","name":"Legacy Req","method":"POST","url":"/legacy","headers":[],"body":null,"auth":{"authType":"none"}}"#;
    fs::write(col_dir.join("test.json"), json).unwrap();

    // get() should auto-migrate.
    let col = repo.get("legacy-api").unwrap();
    assert_eq!(col.name, "legacy-api");
    assert_eq!(col.root.request_count(), 1);

    // Verify migration happened.
    assert!(col_dir.join("opencollection.yml").exists());
    assert!(col_dir.join("test.yml").exists());
    assert!(!col_dir.join("test.json").exists());
}

#[test]
fn folder_variables_roundtrip() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "auth").unwrap();

    let vars = vec![
        CollectionVariable { key: "BASE_URL".into(), value: "https://api.example.com".into(), initial_value: "".into(), enabled: true, secret: false },
        CollectionVariable { key: "TIMEOUT".into(), value: "30".into(), initial_value: "".into(), enabled: true, secret: false },
    ];
    repo.save_folder_variables("my-api", "auth", vars.clone()).unwrap();

    // save_folder_variables doesn't expose a direct getter; verify via get_folder_chain_variables.
    let req = rocket_collection::Request::new("Login", HttpMethod::Get, "/login");
    repo.save_request("my-api", "auth/login.yml", &req).unwrap();

    let chain = repo.get_folder_chain_variables("my-api", "auth/login.yml").unwrap();
    assert_eq!(chain.len(), 2);
    let keys: Vec<&str> = chain.iter().map(|v| v.key.as_str()).collect();
    assert!(keys.contains(&"BASE_URL"));
    assert!(keys.contains(&"TIMEOUT"));
}

#[test]
fn request_variables_roundtrip() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    let req = rocket_collection::Request::new("Get Users", HttpMethod::Get, "/users");
    repo.save_request("my-api", "get-users.yml", &req).unwrap();

    let vars = vec![
        CollectionVariable { key: "PAGE".into(), value: "2".into(), initial_value: "1".into(), enabled: true, secret: false },
    ];
    repo.save_request_variables("my-api", "get-users.yml", vars).unwrap();

    let loaded = repo.get_request_variables("my-api", "get-users.yml").unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].key, "PAGE");
    // Both initial and current values must survive the roundtrip independently.
    assert_eq!(loaded[0].initial_value, "1");
    assert_eq!(loaded[0].value, "2");
}

#[test]
fn folder_chain_walks_disk_and_merges() {
    // Proves that the disk walk feeds into the domain merge correctly.
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "outer").unwrap();
    repo.create_folder("my-api", "outer/inner").unwrap();

    let outer_vars = vec![
        CollectionVariable { key: "k".into(), value: "outer".into(), initial_value: "outer".into(), enabled: true, secret: false },
    ];
    let inner_vars = vec![
        CollectionVariable { key: "k".into(), value: "inner".into(), initial_value: "inner".into(), enabled: true, secret: false },
    ];
    repo.save_folder_variables("my-api", "outer", outer_vars).unwrap();
    repo.save_folder_variables("my-api", "outer/inner", inner_vars).unwrap();

    let req = rocket_collection::Request::new("Test", HttpMethod::Get, "/test");
    repo.save_request("my-api", "outer/inner/req.yml", &req).unwrap();

    let result = repo.get_folder_chain_variables("my-api", "outer/inner/req.yml").unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].key, "k");
    assert_eq!(result[0].value, "inner");
}

#[test]
fn rename_folder_updates_folder_yml_name() {
    // Regression: move_item renamed the directory but left the stale name in
    // folder.yml, causing build_folder_tree to override with the old name.
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "old-name").unwrap();

    repo.move_item("my-api", "old-name", "my-api", "new-name").unwrap();

    let collection = repo.get("my-api").unwrap();
    let folder = collection.root.items.iter().find_map(|item| {
        if let rocket_collection::CollectionItem::Folder(f) = item { Some(f) } else { None }
    });
    assert!(folder.is_some(), "folder should still exist after rename");
    assert_eq!(
        folder.unwrap().name, "new-name",
        "folder name should reflect the new directory name, not the stale folder.yml value"
    );
}

#[test]
fn rename_nested_folder_updates_folder_yml_name() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "parent").unwrap();
    repo.create_folder("my-api", "parent/child").unwrap();

    repo.move_item("my-api", "parent/child", "my-api", "parent/renamed-child").unwrap();

    let collection = repo.get("my-api").unwrap();
    let parent = collection.root.items.iter().find_map(|item| {
        if let rocket_collection::CollectionItem::Folder(f) = item { Some(f) } else { None }
    }).unwrap();
    let child = parent.items.iter().find_map(|item| {
        if let rocket_collection::CollectionItem::Folder(f) = item { Some(f) } else { None }
    });
    assert!(child.is_some(), "child folder should exist after rename");
    assert_eq!(child.unwrap().name, "renamed-child");
}

#[test]
fn get_rejects_path_traversal_in_collection_name() {
    let (_dir, repo) = setup();
    let err = repo.get("../evil").unwrap_err();
    assert!(matches!(err, DomainError::InvalidInput(_)), "expected InvalidInput, got {:?}", err);
}

#[test]
fn delete_rejects_path_traversal_in_collection_name() {
    let (_dir, repo) = setup();
    let err = repo.delete("../evil").unwrap_err();
    assert!(matches!(err, DomainError::InvalidInput(_)), "expected InvalidInput, got {:?}", err);
}

#[test]
#[cfg(unix)]
fn delete_rejects_symlinked_collection() {
    use std::os::unix::fs::symlink;
    let dir = TempDir::new().unwrap();
    let repo = FsCollectionRepo::new(dir.path().to_path_buf(), Arc::new(DashMap::new()));
    let target = dir.path().parent().unwrap().join("outside");
    fs::create_dir_all(&target).unwrap();
    let link = dir.path().join("evil-collection");
    symlink(&target, &link).unwrap();
    let err = repo.delete("evil-collection").unwrap_err();
    assert!(matches!(err, DomainError::InvalidInput(_)), "expected InvalidInput, got {:?}", err);
    assert!(target.exists());
}

#[test]
#[cfg(unix)]
fn delete_folder_rejects_symlinked_folder() {
    use std::os::unix::fs::symlink;
    let dir = TempDir::new().unwrap();
    let repo = FsCollectionRepo::new(dir.path().to_path_buf(), Arc::new(DashMap::new()));
    repo.create("my-api").unwrap();
    let target = dir.path().parent().unwrap().join("important");
    fs::create_dir_all(&target).unwrap();
    let link = dir.path().join("my-api").join("evil-folder");
    symlink(&target, &link).unwrap();
    let err = repo.delete_folder("my-api", "evil-folder").unwrap_err();
    assert!(matches!(err, DomainError::InvalidInput(_)), "expected InvalidInput, got {:?}", err);
    assert!(target.exists());
}

#[test]
fn save_folder_variables_rejects_corrupt_folder_yml() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "auth").unwrap();
    let folder_yml = dir.path().join("my-api").join("auth").join("folder.yml");
    fs::write(&folder_yml, b"{{{{not valid yaml: [[[").unwrap();
    let result = repo.save_folder_variables("my-api", "auth", vec![]);
    assert!(result.is_err(), "expected error on corrupt folder.yml, got Ok");
    // File must NOT have been silently overwritten.
    let content = fs::read_to_string(&folder_yml).unwrap();
    assert!(content.contains("not valid yaml"), "file was silently overwritten");
}

#[test]
fn build_folder_tree_skips_corrupt_request_file() {
    use rocket_shared::types::HttpMethod;
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    let req = rocket_collection::Request::new("Good", HttpMethod::Get, "https://example.com");
    repo.save_request("my-api", "good.yml", &req).unwrap();
    let bad_path = dir.path().join("my-api").join("bad.yml");
    fs::write(&bad_path, b"http:\n  method: [[[unclosed").unwrap();
    let collection = repo.get("my-api").unwrap();
    let names: Vec<&str> = collection.root.items.iter().filter_map(|item| {
        if let rocket_collection::CollectionItem::Request(r) = item {
            Some(r.name.as_str())
        } else {
            None
        }
    }).collect();
    assert!(names.contains(&"Good"), "good request missing: {:?}", names);
    assert!(!names.contains(&"bad"), "corrupt file should be skipped: {:?}", names);
}

#[test]
fn build_folder_tree_respects_order_yml() {
    let (_dir, repo) = setup();
    repo.create("ordered").unwrap();
    let req_a = rocket_collection::Request::new("Alpha", HttpMethod::Get, "https://a.test");
    let req_b = rocket_collection::Request::new("Beta",  HttpMethod::Get, "https://b.test");
    let req_c = rocket_collection::Request::new("Gamma", HttpMethod::Get, "https://c.test");
    repo.save_request("ordered", "c-gamma.yml", &req_c).unwrap();
    repo.save_request("ordered", "b-beta.yml",  &req_b).unwrap();
    repo.save_request("ordered", "a-alpha.yml", &req_a).unwrap();
    let order_path = _dir.path().join("ordered").join("_order.yml");
    std::fs::write(&order_path, "- c-gamma.yml\n- b-beta.yml\n- a-alpha.yml\n").unwrap();
    let col = repo.get("ordered").unwrap();
    let names: Vec<_> = col.root.items.iter().filter_map(|item| {
        if let rocket_collection::CollectionItem::Request(r) = item {
            Some(r.name.as_str())
        } else {
            None
        }
    }).collect();
    assert_eq!(names, vec!["Gamma", "Beta", "Alpha"]);
}

#[test]
fn get_folder_chain_variables_empty_for_root_request() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    let req = rocket_collection::Request::new("Root", HttpMethod::Get, "https://example.com");
    repo.save_request("my-api", "root.yml", &req).unwrap();
    // Root-level request has no ancestor folders, so chain variables must be empty.
    let vars = repo.get_folder_chain_variables("my-api", "root.yml").unwrap();
    assert!(vars.is_empty(), "expected no chain vars for root request, got {:?}", vars);
}

#[test]
fn concurrent_save_settings_does_not_corrupt_file() {
    use std::thread;

    let dir = TempDir::new().unwrap();
    let locks: Arc<DashMap<String, Arc<Mutex<()>>>> = Arc::new(DashMap::new());
    let repo = Arc::new(FsCollectionRepo::new(dir.path().to_path_buf(), Arc::clone(&locks)));
    repo.create("race-api").unwrap();

    let threads: Vec<_> = (0..8).map(|i| {
        let repo = Arc::clone(&repo);
        thread::spawn(move || {
            let _ = i;
            let settings = rocket_collection::CollectionSettings::default();
            repo.save_settings("race-api", &settings).unwrap();
            // Verify get_settings also works without panic.
            repo.get_settings("race-api").unwrap();
        })
    }).collect();

    for t in threads {
        t.join().unwrap();
    }
}

#[test]
fn get_folder_chain_variables_returns_folder_vars() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "auth").unwrap();
    // Save a variable on the auth folder.
    repo.save_folder_variables("my-api", "auth", vec![
        rocket_collection::CollectionVariable {
            key: "token".to_string(),
            value: "secret".to_string(),
            initial_value: String::new(),
            enabled: true,
            secret: false,
        },
    ]).unwrap();
    let req = rocket_collection::Request::new("Login", HttpMethod::Post, "https://example.com");
    repo.save_request("my-api", "auth/login.yml", &req).unwrap();
    let vars = repo.get_folder_chain_variables("my-api", "auth/login.yml").unwrap();
    assert_eq!(vars.len(), 1);
    assert_eq!(vars[0].key, "token");
}
