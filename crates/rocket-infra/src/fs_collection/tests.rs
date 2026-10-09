use super::*;
use dashmap::DashMap;
use rocket_collection::settings::SandboxMode;
use rocket_collection::{CollectionRepository, CollectionSettings, CollectionVariable};
use rocket_shared::types::HttpMethod;
use std::fs;
use std::sync::{Arc, Mutex};
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
    let req = rocket_collection::Request::new(
        "Get Users",
        HttpMethod::Get,
        "https://api.example.com/users",
    );
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
fn get_request_errors_on_body_malformed_but_summary_parseable_file() {
    let (dir, repo) = setup();
    repo.create("my-api").expect("create collection");
    fs::write(
        dir.path().join("my-api/broken.yml"),
        "info:\n  name: Broken\n  type: http\nhttp:\n  method: GET\n  url: https://example.com\n  body:\n    type: bogus\n    data: x\n",
    )
    .expect("write broken.yml");

    // The lenient summary loader only reads uid/info.name/http.method/http.url,
    // so this file still appears in get_summaries()...
    let summaries = repo.get_summaries("my-api").expect("get_summaries");
    assert_eq!(summaries.root.items.len(), 1);

    // ...but the strict full loader used by get_request rejects the malformed
    // body.type discriminant instead of panicking. The frontend's on-demand
    // fetch (RequestNode.createTab) relies on this being a clean error.
    let result = repo.get_request("my-api", "broken.yml");
    assert!(
        result.is_err(),
        "expected malformed body to error, got {result:?}"
    );
}

#[test]
fn get_request_on_uid_less_file_does_not_rewrite_it() {
    let (dir, repo) = setup();
    repo.create("my-api").expect("create collection");
    let file_path = dir.path().join("my-api/no-uid.yml");
    let original =
        "info:\n  name: No Uid\n  type: http\nhttp:\n  method: GET\n  url: https://example.com\n";
    fs::write(&file_path, original).expect("write no-uid.yml");

    // Callers still get a non-empty in-memory uid.
    let req = repo
        .get_request("my-api", "no-uid.yml")
        .expect("get_request");
    assert!(!req.uid.is_empty(), "expected an in-memory uid");

    // A pure read must not silently rewrite the file on disk.
    let after = fs::read(&file_path).expect("re-read no-uid.yml");
    assert_eq!(
        after,
        original.as_bytes(),
        "get_request must not modify the file on disk"
    );
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
    repo.move_item("my-api", "old/test.yml", "my-api", "new/test.yml")
        .unwrap();
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
        auth: Some(Auth::Bearer {
            token: "tok_abc".into(),
        }),
        headers: vec![Header::new("X-Tenant", "acme")],
        variables: vec![],
        sandbox_mode: SandboxMode::Safe,
        ..Default::default()
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
        sandbox_mode: SandboxMode::Safe,
        ..Default::default()
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
        auth: Some(Auth::Bearer {
            token: "tok".into(),
        }),
        headers: vec![Header::new("X-Tenant", "acme")],
        variables: vec![],
        sandbox_mode: SandboxMode::Safe,
        ..Default::default()
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
fn settings_sandbox_mode_developer_roundtrips() {
    let (_dir, repo) = setup();
    repo.create("my-api").expect("create collection");

    let settings = CollectionSettings {
        sandbox_mode: SandboxMode::Developer,
        ..Default::default()
    };
    repo.save_settings("my-api", &settings)
        .expect("save settings");

    let loaded = repo.get_settings("my-api").expect("get settings");
    assert_eq!(loaded.sandbox_mode, SandboxMode::Developer);
}

#[test]
fn settings_sandbox_mode_defaults_to_safe_without_rocketapi_extension() {
    let (dir, repo) = setup();
    repo.create("my-api").expect("create collection");

    // Write an opencollection.yml with no `extensions` key at all.
    let path = dir.path().join("my-api/opencollection.yml");
    fs::write(&path, "opencollection: \"1.0.0\"\ninfo:\n  name: my-api\n").expect("write fixture");

    let loaded = repo.get_settings("my-api").expect("get settings");
    assert_eq!(loaded.sandbox_mode, SandboxMode::Safe);
}

#[test]
fn save_settings_preserves_unrelated_extensions_data() {
    let (dir, repo) = setup();
    repo.create("my-api").expect("create collection");

    // Write a fixture with unrelated data under `extensions`.
    let path = dir.path().join("my-api/opencollection.yml");
    fs::write(
        &path,
        "opencollection: \"1.0.0\"\ninfo:\n  name: my-api\nextensions:\n  someOtherTool:\n    foo: bar\n",
    )
    .expect("write fixture");

    let settings = CollectionSettings {
        sandbox_mode: SandboxMode::Developer,
        ..Default::default()
    };
    repo.save_settings("my-api", &settings)
        .expect("save settings");

    let content = fs::read_to_string(&path).expect("read back opencollection.yml");
    assert!(content.contains("someOtherTool"));
    assert!(content.contains("foo: bar"));
    assert!(content.contains("sandboxMode: developer"));
}

#[test]
fn settings_agent_autonomy_enabled_roundtrips() {
    let (_dir, repo) = setup();
    repo.create("my-api").expect("create collection");

    let settings = CollectionSettings {
        agent_autonomy_enabled: true,
        ..Default::default()
    };
    repo.save_settings("my-api", &settings)
        .expect("save settings");

    let loaded = repo.get_settings("my-api").expect("get settings");
    assert!(loaded.agent_autonomy_enabled);
}

#[test]
fn settings_agent_autonomy_enabled_defaults_to_false_without_rocketapi_extension() {
    let (dir, repo) = setup();
    repo.create("my-api").expect("create collection");

    let path = dir.path().join("my-api/opencollection.yml");
    fs::write(&path, "opencollection: \"1.0.0\"\ninfo:\n  name: my-api\n").expect("write fixture");

    let loaded = repo.get_settings("my-api").expect("get settings");
    assert!(!loaded.agent_autonomy_enabled);
}

#[test]
fn save_settings_sandbox_mode_and_agent_autonomy_enabled_persist_together() {
    let (_dir, repo) = setup();
    repo.create("my-api").expect("create collection");

    let settings = CollectionSettings {
        sandbox_mode: SandboxMode::Developer,
        agent_autonomy_enabled: true,
        ..Default::default()
    };
    repo.save_settings("my-api", &settings)
        .expect("save settings");

    let loaded = repo.get_settings("my-api").expect("get settings");
    assert_eq!(loaded.sandbox_mode, SandboxMode::Developer);
    assert!(loaded.agent_autonomy_enabled);
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
            CollectionItem::GraphQl(g) => g.name.as_str(),
            CollectionItem::WebSocket(w) => w.name.as_str(),
            CollectionItem::Grpc(g) => g.name.as_str(),
            CollectionItem::OpaqueItem(o) => o.name.as_str(),
            CollectionItem::Summary(s) => s.name.as_str(),
            CollectionItem::ScriptFile(s) => s.name.as_str(),
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
fn flows_dir_not_shown_in_collection_tree() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    // Flow files are a Rocket-only extension and are not requests.
    fs::create_dir_all(dir.path().join("my-api/flows")).unwrap();
    fs::write(
        dir.path().join("my-api/flows/my-flow.yml"),
        "name: my-flow\nnodes: []\n",
    )
    .unwrap();
    let col = repo.get("my-api").unwrap();
    assert!(
        !col.root.subfolder_names().contains(&"flows"),
        "flows/ should not appear in the collection tree"
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
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "auth").unwrap();
    let folder_dir = dir.path().join("my-api/auth");

    // Simulate legacy: write .uid, strip uid from folder.yml.
    let legacy_uid = "folder-uid-67890";
    fs::write(folder_dir.join(".uid"), legacy_uid).unwrap();

    let content = fs::read_to_string(folder_dir.join("folder.yml")).unwrap();
    let mut folder = crate::fs_collection::folder_file::parse_folder_yml(&content).unwrap();
    folder.info.uid = None;
    fs::write(
        folder_dir.join("folder.yml"),
        serde_yaml::to_string(&folder).unwrap(),
    )
    .unwrap();

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
        CollectionVariable {
            key: "BASE_URL".into(),
            value: "https://api.example.com".into(),
            initial_value: "".into(),
            enabled: true,
            secret: false,
        },
        CollectionVariable {
            key: "TIMEOUT".into(),
            value: "30".into(),
            initial_value: "".into(),
            enabled: true,
            secret: false,
        },
    ];
    repo.save_folder_variables("my-api", "auth", vars.clone())
        .unwrap();

    // save_folder_variables doesn't expose a direct getter; verify via get_folder_chain_variables.
    let req = rocket_collection::Request::new("Login", HttpMethod::Get, "/login");
    repo.save_request("my-api", "auth/login.yml", &req).unwrap();

    let chain = repo
        .get_folder_chain_variables("my-api", "auth/login.yml")
        .unwrap();
    assert_eq!(chain.len(), 2);
    let keys: Vec<&str> = chain.iter().map(|v| v.key.as_str()).collect();
    assert!(keys.contains(&"BASE_URL"));
    assert!(keys.contains(&"TIMEOUT"));
}

#[test]
fn save_request_preserves_variables_written_by_save_request_variables() {
    // Regression: save_request (used by auto-save on tab switch) must not erase
    // variables written by save_request_variables; the IPC payload never carries them.
    let (_dir, repo) = setup();
    repo.create("my-api").expect("create collection");
    let req = rocket_collection::Request::new("Get Users", HttpMethod::Get, "/users");
    repo.save_request("my-api", "get-users.yml", &req)
        .expect("initial save");

    let vars = vec![CollectionVariable {
        key: "TOKEN".into(),
        value: "abc".into(),
        initial_value: "".into(),
        enabled: true,
        secret: true,
    }];
    repo.save_request_variables("my-api", "get-users.yml", vars)
        .expect("save vars");

    // Simulate the frontend auto-save payload — variables field is intentionally empty.
    let mut req_without_vars = repo
        .get_request("my-api", "get-users.yml")
        .expect("load request");
    req_without_vars.variables = vec![];
    req_without_vars.pre_request_script = Some("console.log('pre');".into());
    repo.save_request("my-api", "get-users.yml", &req_without_vars)
        .expect("save request after scripts edit");

    let preserved = repo
        .get_request_variables("my-api", "get-users.yml")
        .expect("load vars");
    assert_eq!(
        preserved.len(),
        1,
        "variables must survive save_request when payload carries none"
    );
    assert_eq!(preserved[0].key, "TOKEN");
    assert_eq!(preserved[0].value, "abc");
}

#[test]
fn save_request_script_only_touches_the_targeted_phase() {
    let (_dir, repo) = setup();
    repo.create("my-api").expect("create collection");
    let mut req = rocket_collection::Request::new("Get Users", HttpMethod::Get, "/users");
    req.pre_request_script = Some("console.log('pre');".into());
    req.post_response_script = Some("console.log('post');".into());
    req.tests = Some("rok.test('ok', () => {});".into());
    repo.save_request("my-api", "get-users.yml", &req)
        .expect("initial save");

    repo.save_request_script(
        "my-api",
        "get-users.yml",
        rocket_collection::RequestScriptPhase::PostResponse,
        "console.log('post-updated');".into(),
    )
    .expect("save_request_script");

    let loaded = repo
        .get_request("my-api", "get-users.yml")
        .expect("load request");
    assert_eq!(
        loaded.pre_request_script,
        Some("console.log('pre');".to_string()),
        "an unrelated phase must not be touched"
    );
    assert_eq!(
        loaded.post_response_script,
        Some("console.log('post-updated');".to_string())
    );
    assert_eq!(
        loaded.tests,
        Some("rok.test('ok', () => {});".to_string()),
        "an unrelated phase must not be touched"
    );
}

#[test]
fn save_request_script_on_uid_less_file_persists_a_real_uid() {
    let (dir, repo) = setup();
    repo.create("my-api").expect("create collection");
    let file_path = dir.path().join("my-api/no-uid.yml");
    fs::write(
        &file_path,
        "info:\n  name: No Uid\n  type: http\nhttp:\n  method: GET\n  url: https://example.com\n",
    )
    .expect("write no-uid.yml");

    repo.save_request_script(
        "my-api",
        "no-uid.yml",
        rocket_collection::RequestScriptPhase::Tests,
        "rok.test('x', () => {});".into(),
    )
    .expect("save_request_script");

    // An empty uid must never be written to disk.
    let written: crate::oc::OcHttpRequest =
        serde_yaml::from_str(&fs::read_to_string(&file_path).expect("re-read no-uid.yml"))
            .expect("parse no-uid.yml");
    let uid = written.uid.expect("uid should be persisted");
    assert!(!uid.is_empty(), "persisted uid must not be empty");

    // The persisted uid is stable across reads.
    let first = repo.get_request("my-api", "no-uid.yml").expect("load 1");
    let second = repo.get_request("my-api", "no-uid.yml").expect("load 2");
    assert_eq!(first.uid, uid);
    assert_eq!(second.uid, uid);
}

#[test]
fn save_request_script_errors_for_a_missing_request() {
    let (_dir, repo) = setup();
    repo.create("my-api").expect("create collection");
    let err = repo
        .save_request_script(
            "my-api",
            "does-not-exist.yml",
            rocket_collection::RequestScriptPhase::Tests,
            "rok.test('x', () => {});".into(),
        )
        .expect_err("missing request file must error, not panic");
    // Matches save_request_variables's exact behavior for the identical
    // missing-file case: resolve_request_path returns a canonicalized path
    // under the (existing) collection directory even when the file itself
    // doesn't exist, so the failure surfaces from fs::read_to_string as an
    // io::Error, converted to DomainError::Io — not DomainError::NotFound.
    assert!(matches!(err, DomainError::Io(_)));
}

#[test]
fn request_variables_roundtrip() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    let req = rocket_collection::Request::new("Get Users", HttpMethod::Get, "/users");
    repo.save_request("my-api", "get-users.yml", &req).unwrap();

    let vars = vec![CollectionVariable {
        key: "PAGE".into(),
        value: "2".into(),
        initial_value: "1".into(),
        enabled: true,
        secret: false,
    }];
    repo.save_request_variables("my-api", "get-users.yml", vars)
        .unwrap();

    let loaded = repo
        .get_request_variables("my-api", "get-users.yml")
        .unwrap();
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

    let outer_vars = vec![CollectionVariable {
        key: "k".into(),
        value: "outer".into(),
        initial_value: "outer".into(),
        enabled: true,
        secret: false,
    }];
    let inner_vars = vec![CollectionVariable {
        key: "k".into(),
        value: "inner".into(),
        initial_value: "inner".into(),
        enabled: true,
        secret: false,
    }];
    repo.save_folder_variables("my-api", "outer", outer_vars)
        .unwrap();
    repo.save_folder_variables("my-api", "outer/inner", inner_vars)
        .unwrap();

    let req = rocket_collection::Request::new("Test", HttpMethod::Get, "/test");
    repo.save_request("my-api", "outer/inner/req.yml", &req)
        .unwrap();

    let result = repo
        .get_folder_chain_variables("my-api", "outer/inner/req.yml")
        .unwrap();
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

    repo.move_item("my-api", "old-name", "my-api", "new-name")
        .unwrap();

    let collection = repo.get("my-api").unwrap();
    let folder = collection.root.items.iter().find_map(|item| {
        if let rocket_collection::CollectionItem::Folder(f) = item {
            Some(f)
        } else {
            None
        }
    });
    assert!(folder.is_some(), "folder should still exist after rename");
    assert_eq!(
        folder.unwrap().name,
        "new-name",
        "folder name should reflect the new directory name, not the stale folder.yml value"
    );
}

#[test]
fn rename_nested_folder_updates_folder_yml_name() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "parent").unwrap();
    repo.create_folder("my-api", "parent/child").unwrap();

    repo.move_item("my-api", "parent/child", "my-api", "parent/renamed-child")
        .unwrap();

    let collection = repo.get("my-api").unwrap();
    let parent = collection
        .root
        .items
        .iter()
        .find_map(|item| {
            if let rocket_collection::CollectionItem::Folder(f) = item {
                Some(f)
            } else {
                None
            }
        })
        .unwrap();
    let child = parent.items.iter().find_map(|item| {
        if let rocket_collection::CollectionItem::Folder(f) = item {
            Some(f)
        } else {
            None
        }
    });
    assert!(child.is_some(), "child folder should exist after rename");
    assert_eq!(child.unwrap().name, "renamed-child");
}

#[test]
fn get_rejects_path_traversal_in_collection_name() {
    let (_dir, repo) = setup();
    let err = repo.get("../evil").unwrap_err();
    assert!(
        matches!(err, DomainError::InvalidInput(_)),
        "expected InvalidInput, got {:?}",
        err
    );
}

#[test]
fn delete_rejects_path_traversal_in_collection_name() {
    let (_dir, repo) = setup();
    let err = repo.delete("../evil").unwrap_err();
    assert!(
        matches!(err, DomainError::InvalidInput(_)),
        "expected InvalidInput, got {:?}",
        err
    );
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
    assert!(
        matches!(err, DomainError::InvalidInput(_)),
        "expected InvalidInput, got {:?}",
        err
    );
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
    assert!(
        matches!(err, DomainError::InvalidInput(_)),
        "expected InvalidInput, got {:?}",
        err
    );
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
    assert!(
        result.is_err(),
        "expected error on corrupt folder.yml, got Ok"
    );
    // File must NOT have been silently overwritten.
    let content = fs::read_to_string(&folder_yml).unwrap();
    assert!(
        content.contains("not valid yaml"),
        "file was silently overwritten"
    );
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
    let names: Vec<&str> = collection
        .root
        .items
        .iter()
        .filter_map(|item| {
            if let rocket_collection::CollectionItem::Request(r) = item {
                Some(r.name.as_str())
            } else {
                None
            }
        })
        .collect();
    assert!(names.contains(&"Good"), "good request missing: {:?}", names);
    assert!(
        !names.contains(&"bad"),
        "corrupt file should be skipped: {:?}",
        names
    );
}

#[test]
fn build_folder_tree_respects_order_yml() {
    let (_dir, repo) = setup();
    repo.create("ordered").unwrap();
    let req_a = rocket_collection::Request::new("Alpha", HttpMethod::Get, "https://a.test");
    let req_b = rocket_collection::Request::new("Beta", HttpMethod::Get, "https://b.test");
    let req_c = rocket_collection::Request::new("Gamma", HttpMethod::Get, "https://c.test");
    repo.save_request("ordered", "c-gamma.yml", &req_c).unwrap();
    repo.save_request("ordered", "b-beta.yml", &req_b).unwrap();
    repo.save_request("ordered", "a-alpha.yml", &req_a).unwrap();
    let order_path = _dir.path().join("ordered").join("_order.yml");
    std::fs::write(&order_path, "- c-gamma.yml\n- b-beta.yml\n- a-alpha.yml\n").unwrap();
    let col = repo.get("ordered").unwrap();
    let names: Vec<_> = col
        .root
        .items
        .iter()
        .filter_map(|item| {
            if let rocket_collection::CollectionItem::Request(r) = item {
                Some(r.name.as_str())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(names, vec!["Gamma", "Beta", "Alpha"]);
}

#[test]
fn get_folder_chain_variables_empty_for_root_request() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    let req = rocket_collection::Request::new("Root", HttpMethod::Get, "https://example.com");
    repo.save_request("my-api", "root.yml", &req).unwrap();
    // Root-level request has no ancestor folders, so chain variables must be empty.
    let vars = repo
        .get_folder_chain_variables("my-api", "root.yml")
        .unwrap();
    assert!(
        vars.is_empty(),
        "expected no chain vars for root request, got {:?}",
        vars
    );
}

#[test]
fn concurrent_save_settings_does_not_corrupt_file() {
    use std::thread;

    let dir = TempDir::new().unwrap();
    let locks: Arc<DashMap<String, Arc<Mutex<()>>>> = Arc::new(DashMap::new());
    let repo = Arc::new(FsCollectionRepo::new(
        dir.path().to_path_buf(),
        Arc::clone(&locks),
    ));
    repo.create("race-api").unwrap();

    let threads: Vec<_> = (0..8)
        .map(|i| {
            let repo = Arc::clone(&repo);
            thread::spawn(move || {
                let _ = i;
                let settings = rocket_collection::CollectionSettings::default();
                repo.save_settings("race-api", &settings).unwrap();
                // Verify get_settings also works without panic.
                repo.get_settings("race-api").unwrap();
            })
        })
        .collect();

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
    repo.save_folder_variables(
        "my-api",
        "auth",
        vec![rocket_collection::CollectionVariable {
            key: "token".to_string(),
            value: "secret".to_string(),
            initial_value: String::new(),
            enabled: true,
            secret: false,
        }],
    )
    .unwrap();
    let req = rocket_collection::Request::new("Login", HttpMethod::Post, "https://example.com");
    repo.save_request("my-api", "auth/login.yml", &req).unwrap();
    let vars = repo
        .get_folder_chain_variables("my-api", "auth/login.yml")
        .unwrap();
    assert_eq!(vars.len(), 1);
    assert_eq!(vars[0].key, "token");
}

#[test]
fn get_summaries_returns_collection_with_summary_items() {
    let (_dir, repo) = setup();
    repo.create("pets").unwrap();
    let req = rocket_collection::Request::new(
        "List Pets",
        HttpMethod::Get,
        "https://api.example.com/pets",
    );
    repo.save_request("pets", "list-pets.yml", &req).unwrap();

    let col = repo.get_summaries("pets").unwrap();
    assert_eq!(col.name, "pets");
    let summaries = col.root.request_summaries();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].name, "List Pets");
    assert_eq!(summaries[0].method, "GET");
    assert_eq!(summaries[0].url, "https://api.example.com/pets");
    assert!(!summaries[0].uid.is_empty());
}

#[test]
fn get_summaries_does_not_load_body_or_auth() {
    let (_dir, repo) = setup();
    repo.create("api").unwrap();
    let mut req = rocket_collection::Request::new(
        "Post Data",
        HttpMethod::Post,
        "https://api.example.com/data",
    );
    req.body = Some(rocket_shared::types::Body {
        mode: rocket_shared::types::BodyMode::Json,
        content: Some(r#"{"x":1}"#.to_string()),
        form_data: None,
        file_path: None,
    });
    repo.save_request("api", "post-data.yml", &req).unwrap();

    // get_summaries must succeed and return name/method/url — body is not loaded.
    let col = repo.get_summaries("api").unwrap();
    let summaries = col.root.request_summaries();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].name, "Post Data");
    assert_eq!(summaries[0].method, "POST");
}

#[test]
fn get_summaries_preserves_folder_structure() {
    let (_dir, repo) = setup();
    repo.create("api").unwrap();
    repo.create_folder("api", "auth").unwrap();
    let req =
        rocket_collection::Request::new("Login", HttpMethod::Post, "https://api.example.com/login");
    repo.save_request("api", "auth/login.yml", &req).unwrap();

    let col = repo.get_summaries("api").unwrap();
    let auth_folder = col
        .root
        .subfolders()
        .into_iter()
        .find(|f| f.dir_name.as_deref() == Some("auth"));
    assert!(
        auth_folder.is_some(),
        "auth folder missing from summaries tree"
    );
    let summaries = auth_folder.unwrap().request_summaries();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].name, "Login");
}

#[test]
fn rename_request_holds_collection_mutex() {
    // Structural invariant: rename_request must acquire the per-collection mutex
    // before doing any I/O, matching every other RMW method on FsCollectionRepo.
    // We verify this by holding the mutex on the test thread and showing that a
    // rename_request call from another thread blocks until we drop the guard.
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;
    use std::time::Duration;

    let (_dir, repo) = setup();
    let repo = Arc::new(repo);
    repo.create("col").unwrap();
    let req = rocket_collection::Request::new("R", HttpMethod::Get, "https://example.com");
    repo.save_request("col", "req.yml", &req).unwrap();

    let mutex = repo.collection_mutex("col");
    let guard = mutex.lock().unwrap();

    let started = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let repo_t = Arc::clone(&repo);
    let started_t = Arc::clone(&started);
    let finished_t = Arc::clone(&finished);
    let handle = thread::spawn(move || {
        started_t.store(true, Ordering::SeqCst);
        repo_t.rename_request("col", "req.yml", "req2.yml").unwrap();
        finished_t.store(true, Ordering::SeqCst);
    });

    // Wait for the worker to actually start.
    while !started.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(1));
    }
    // Give it ample time to attempt the rename. It must not finish while we hold the lock.
    thread::sleep(Duration::from_millis(50));
    assert!(
        !finished.load(Ordering::SeqCst),
        "rename_request finished while the per-collection mutex was held — mutex guard missing"
    );

    drop(guard);
    handle.join().unwrap();
    assert!(finished.load(Ordering::SeqCst));
    assert!(repo.collection_path("col").join("req2.yml").exists());
}

#[test]
fn script_roundtrip_matches_bruno_oc_spec() {
    let (_dir, repo) = setup();
    repo.create("api").expect("create collection");

    let mut req =
        rocket_collection::Request::new("Test Scripts", HttpMethod::Get, "https://example.com");
    req.pre_request_script = Some("req.setHeader('X-Trace', '1');".into());
    req.post_response_script = Some("res.status;".into());
    req.tests = Some("expect(res.status).to.equal(200);".into());

    repo.save_request("api", "test-scripts.yml", &req)
        .expect("save request");

    // Inspect the raw YAML to confirm it matches the Bruno OpenCollection spec format.
    let col_dir = repo.collection_path("api");
    let raw = std::fs::read_to_string(col_dir.join("test-scripts.yml")).expect("read yml");
    // Spec requires: runtime.scripts[].type and runtime.scripts[].code
    assert!(raw.contains("runtime:"), "missing runtime block:\n{raw}");
    assert!(raw.contains("scripts:"), "missing scripts key:\n{raw}");
    assert!(
        raw.contains("type: before-request"),
        "missing before-request type:\n{raw}"
    );
    assert!(
        raw.contains("type: after-response"),
        "missing after-response type:\n{raw}"
    );
    assert!(raw.contains("type: tests"), "missing tests type:\n{raw}");
    // Bruno always uses |- (strip chomping) — assert no bare '| ' block scalars remain.
    assert!(
        !raw.contains("code: |\n"),
        "tests script must use |- not |:\n{raw}"
    );

    // Confirm full roundtrip preserves all three scripts.
    let loaded = repo
        .get_request("api", "test-scripts.yml")
        .expect("load request");
    assert_eq!(
        loaded.pre_request_script.as_deref(),
        Some("req.setHeader('X-Trace', '1');")
    );
    assert_eq!(loaded.post_response_script.as_deref(), Some("res.status;"));
    assert_eq!(
        loaded.tests.as_deref(),
        Some("expect(res.status).to.equal(200);")
    );

    // Scripts entered with trailing newlines (as Monaco produces) must be normalized to |-
    // so the file matches the Bruno format regardless of editor behavior.
    let mut req2 =
        rocket_collection::Request::new("Trailing NL", HttpMethod::Get, "https://example.com");
    req2.pre_request_script = Some("console.log('pre');\n".into());
    req2.post_response_script = Some("console.log('post');\n".into());
    req2.tests = Some("expect(res.status).to.equal(200);\n".into());
    repo.save_request("api", "trailing-nl.yml", &req2)
        .expect("save trailing-nl");
    let raw2 = std::fs::read_to_string(col_dir.join("trailing-nl.yml")).expect("read trailing-nl");
    assert!(
        !raw2.contains("code: |\n"),
        "trailing-newline scripts must serialize as |-:\n{raw2}"
    );
    let loaded2 = repo
        .get_request("api", "trailing-nl.yml")
        .expect("load trailing-nl");
    // Trailing newline is stripped on write, so readback does not have it.
    assert_eq!(
        loaded2.pre_request_script.as_deref(),
        Some("console.log('pre');")
    );
    assert_eq!(
        loaded2.post_response_script.as_deref(),
        Some("console.log('post');")
    );
    assert_eq!(
        loaded2.tests.as_deref(),
        Some("expect(res.status).to.equal(200);")
    );
}

fn read_yaml_value(path: &std::path::Path) -> serde_yaml::Value {
    serde_yaml::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn create_folder_writes_spec_folder_shape() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "auth").unwrap();

    let raw = read_yaml_value(&dir.path().join("my-api/auth/folder.yml"));
    assert_eq!(raw["info"]["name"].as_str(), Some("auth"), "{raw:?}");
    assert_eq!(raw["info"]["type"].as_str(), Some("folder"), "{raw:?}");
    assert!(
        raw.get("name").is_none(),
        "folder.yml must not be a bare FolderInfo: {raw:?}"
    );
    assert!(
        raw.get("items").is_none(),
        "items must never be written: {raw:?}"
    );
}

#[test]
fn folder_uid_is_stable_across_reloads() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "auth").unwrap();

    let first = repo
        .get("my-api")
        .unwrap()
        .root
        .find_folder("auth")
        .unwrap()
        .uid
        .clone();
    let second = repo
        .get("my-api")
        .unwrap()
        .root
        .find_folder("auth")
        .unwrap()
        .uid
        .clone();
    assert!(!first.is_empty());
    assert_eq!(
        first, second,
        "folder uid must not regenerate on every load"
    );
}

#[test]
fn legacy_bare_folder_yml_still_loads_and_is_upgraded_on_write() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    let folder_dir = dir.path().join("my-api/auth");
    fs::create_dir_all(&folder_dir).unwrap();
    fs::write(
        folder_dir.join("folder.yml"),
        "name: Auth Flows\nuid: legacy-folder-uid\ntype: folder\nrequest:\n  variables:\n  - name: token\n    value: abc\n",
    )
    .unwrap();

    // Legacy file loads with its uid and display name.
    let col = repo.get("my-api").unwrap();
    let folder = col
        .root
        .items
        .iter()
        .find_map(|i| match i {
            rocket_collection::CollectionItem::Folder(f) => Some(f),
            _ => None,
        })
        .unwrap();
    assert_eq!(folder.uid, "legacy-folder-uid");
    assert_eq!(folder.name, "Auth Flows");

    // Legacy folder variables still feed the chain.
    let req = rocket_collection::Request::new("Login", HttpMethod::Post, "https://example.com");
    repo.save_request("my-api", "auth/login.yml", &req).unwrap();
    let chain = repo
        .get_folder_chain_variables("my-api", "auth/login.yml")
        .unwrap();
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].key, "token");

    // The next write upgrades the file to the spec shape and keeps the uid.
    repo.save_folder_variables(
        "my-api",
        "auth",
        vec![CollectionVariable {
            key: "token".into(),
            value: "xyz".into(),
            initial_value: String::new(),
            enabled: true,
            secret: false,
        }],
    )
    .unwrap();
    let raw = read_yaml_value(&folder_dir.join("folder.yml"));
    assert_eq!(
        raw["info"]["uid"].as_str(),
        Some("legacy-folder-uid"),
        "{raw:?}"
    );
    assert_eq!(raw["info"]["name"].as_str(), Some("Auth Flows"), "{raw:?}");
    assert!(
        raw["info"].get("request").is_none(),
        "request defaults must leave info: {raw:?}"
    );
    assert_eq!(
        raw["request"]["variables"][0]["name"].as_str(),
        Some("token"),
        "{raw:?}"
    );
    assert_eq!(
        repo.get_folder_variables("my-api", "auth").unwrap()[0].value,
        "xyz"
    );
}

#[test]
fn rename_folder_keeps_spec_shape_and_uid() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "old-name").unwrap();
    let before = read_yaml_value(&dir.path().join("my-api/old-name/folder.yml"));
    let uid = before["info"]["uid"].as_str().unwrap().to_string();

    repo.move_item("my-api", "old-name", "my-api", "new-name")
        .unwrap();

    let after = read_yaml_value(&dir.path().join("my-api/new-name/folder.yml"));
    assert_eq!(
        after["info"]["name"].as_str(),
        Some("new-name"),
        "{after:?}"
    );
    assert_eq!(
        after["info"]["uid"].as_str(),
        Some(uid.as_str()),
        "{after:?}"
    );
}

const GRAPHQL_ITEM_YML: &str = "info:\n  name: List Users\n  type: graphql\ngraphql:\n  url: https://api.example.com/graphql\n  body:\n    query: '{ users { id } }'\n";
const GRPC_ITEM_YML: &str = "info:\n  name: Get User\n  type: grpc\ngrpc:\n  url: grpc://api.example.com\n  method: users.UserService/GetUser\n  methodType: unary\n";
const WEBSOCKET_ITEM_YML: &str =
    "info:\n  name: Chat\n  type: websocket\nwebsocket:\n  url: wss://chat.example.com/ws\n";

fn opaque_items(
    folder: &rocket_collection::Folder,
) -> Vec<&rocket_collection::folder::OpaqueProtocolItem> {
    folder
        .items
        .iter()
        .filter_map(|i| match i {
            rocket_collection::CollectionItem::OpaqueItem(o) => Some(o),
            _ => None,
        })
        .collect()
}

#[test]
fn build_folder_tree_loads_non_http_items_as_opaque() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "realtime").unwrap();
    let col_dir = dir.path().join("my-api");
    fs::write(col_dir.join("list-users.yml"), GRAPHQL_ITEM_YML).unwrap();
    fs::write(col_dir.join("get-user.yml"), GRPC_ITEM_YML).unwrap();
    fs::write(col_dir.join("realtime/chat.yml"), WEBSOCKET_ITEM_YML).unwrap();

    let col = repo.get("my-api").unwrap();
    let mut root: Vec<(&str, &str)> = opaque_items(&col.root)
        .iter()
        .map(|o| (o.protocol.as_str(), o.name.as_str()))
        .collect();
    root.sort();
    // GraphQL and gRPC are typed now. Only WebSocket stays opaque.
    assert!(root.is_empty(), "no opaque item at the root: {root:?}");
    assert!(col.root.items.iter().any(
        |i| matches!(i, rocket_collection::CollectionItem::Grpc(g) if g.name == "Get User")
    ));
    assert!(col.root.items.iter().any(
        |i| matches!(i, rocket_collection::CollectionItem::GraphQl(g) if g.name == "List Users")
    ));

    let realtime = col.root.find_folder("realtime").unwrap();
    assert!(opaque_items(realtime).is_empty(), "websocket is typed now");
    let ws = realtime
        .items
        .iter()
        .find_map(|i| match i {
            rocket_collection::CollectionItem::WebSocket(w) => Some(w),
            _ => None,
        })
        .expect("typed websocket item");
    assert_eq!(ws.name, "Chat");
    assert_eq!(ws.url, "wss://chat.example.com/ws");
}

#[test]
fn build_folder_tree_skips_script_files_without_dropping_siblings() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(
        dir.path().join("my-api/setup.yml"),
        "type: script\nscript: ./scripts/setup.js\n",
    )
    .unwrap();
    let req = rocket_collection::Request::new("Good", HttpMethod::Get, "https://example.com");
    repo.save_request("my-api", "good.yml", &req).unwrap();

    let col = repo.get("my-api").unwrap();
    assert_eq!(col.root.items.len(), 1, "{:?}", col.root.items);
    assert!(matches!(
        &col.root.items[0],
        rocket_collection::CollectionItem::Request(r) if r.name == "Good"
    ));
}

#[test]
fn build_folder_tree_skips_http_file_missing_method_instead_of_misreading_it() {
    // OcItem is untagged and OcFolder needs only `info`, so a broken HTTP file
    // would match OcItem::Folder. It must be skipped as corrupt, never turned
    // into a phantom folder.
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(
        dir.path().join("my-api/broken.yml"),
        "info:\n  name: Broken\n  type: http\nhttp:\n  url: https://example.com\n",
    )
    .unwrap();

    let col = repo.get("my-api").unwrap();
    assert!(col.root.items.is_empty(), "{:?}", col.root.items);
}

#[test]
fn get_summaries_skips_http_file_missing_method_instead_of_misreading_it() {
    // Mirrors build_folder_tree_skips_http_file_missing_method_instead_of_misreading_it
    // for the lightweight summary loader. OcItem is untagged and OcFolder needs only
    // `info`, so a broken HTTP file (missing the required `http.method` field) matches
    // OcItem::Folder in the fallback probe. That must still be treated as genuine
    // corruption in load_request_summary, not as a recognised non-HTTP item, so it does
    // not surface as a phantom summary or folder in the sidebar tree.
    let (dir, repo) = setup();
    repo.create("my-api").expect("create collection");
    fs::write(
        dir.path().join("my-api/broken.yml"),
        "info:\n  name: Broken\n  type: http\nhttp:\n  url: https://example.com\n",
    )
    .expect("write broken.yml");
    let req = rocket_collection::Request::new("Good", HttpMethod::Get, "https://example.com");
    repo.save_request("my-api", "good.yml", &req)
        .expect("save request");

    let col = repo.get_summaries("my-api").expect("get_summaries");
    assert_eq!(col.root.items.len(), 1, "{:?}", col.root.items);
    assert!(matches!(
        &col.root.items[0],
        rocket_collection::CollectionItem::Summary(s) if s.name == "Good"
    ));
}

#[test]
fn websocket_settings_survive_the_typed_load() {
    use rocket_shared::types::RequestSettingValue;

    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(
        dir.path().join("my-api/chat.yml"),
        "info:\n  name: Chat\n  type: websocket\nwebsocket:\n  url: wss://chat.example.com/ws\nsettings:\n  timeout: 5000\n  keepAliveInterval: 30000\n",
    )
    .unwrap();

    let ws = repo.get_websocket_request("my-api", "chat.yml").unwrap();
    let settings = ws.settings.expect("settings");
    assert_eq!(settings.timeout, Some(RequestSettingValue::Value(5000.0)));
    assert_eq!(settings.keep_alive_interval, Some(RequestSettingValue::Value(30000.0)));
}

#[test]
fn save_settings_omits_none_auth_instead_of_writing_type_none() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    let settings = CollectionSettings {
        auth: Some(rocket_shared::types::Auth::None),
        ..Default::default()
    };
    repo.save_settings("my-api", &settings).unwrap();

    let content = fs::read_to_string(dir.path().join("my-api/opencollection.yml")).unwrap();
    assert!(!content.contains("type: none"), "{content}");
    let raw: serde_yaml::Value = serde_yaml::from_str(&content).unwrap();
    assert!(
        raw.get("request").is_none(),
        "no empty request block: {content}"
    );
    assert_eq!(repo.get_settings("my-api").unwrap().auth, None);
}

#[test]
fn save_settings_writes_inherit_as_spec_string() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    let settings = CollectionSettings {
        auth: Some(rocket_shared::types::Auth::Inherit),
        ..Default::default()
    };
    repo.save_settings("my-api", &settings).unwrap();

    let content = fs::read_to_string(dir.path().join("my-api/opencollection.yml")).unwrap();
    assert!(content.contains("auth: inherit"), "{content}");
    assert_eq!(
        repo.get_settings("my-api").unwrap().auth,
        Some(rocket_shared::types::Auth::Inherit)
    );
}

#[test]
fn legacy_type_none_collection_auth_still_loads() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(
        dir.path().join("my-api/opencollection.yml"),
        "opencollection: \"1.0.0\"\ninfo:\n  name: my-api\nrequest:\n  auth:\n    type: none\n",
    )
    .unwrap();
    assert_eq!(
        repo.get_settings("my-api").unwrap().auth,
        Some(rocket_shared::types::Auth::None)
    );
}

#[test]
fn save_request_omits_none_auth() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    let req = rocket_collection::Request::new("Ping", HttpMethod::Get, "https://example.com");
    repo.save_request("my-api", "ping.yml", &req).unwrap();
    let content = fs::read_to_string(dir.path().join("my-api/ping.yml")).unwrap();
    assert!(!content.contains("auth"), "{content}");
}

fn find_root_folder(col: &rocket_collection::Collection) -> &rocket_collection::Folder {
    col.root
        .items
        .iter()
        .find_map(|i| match i {
            rocket_collection::CollectionItem::Folder(f) => Some(f),
            _ => None,
        })
        .expect("root folder")
}

#[test]
fn spec_folder_yml_with_object_docs_loads_and_keeps_its_identity() {
    // The spec's Folder.docs is `Documentation`: a string, null, or {content, type}.
    let (dir, repo) = setup();
    repo.create("my-api").expect("create");
    let folder_dir = dir.path().join("my-api/auth");
    fs::create_dir_all(&folder_dir).expect("mkdir");
    fs::write(
        folder_dir.join("folder.yml"),
        "info:\n  name: Auth Flows\n  uid: spec-folder-uid\n  type: folder\nrequest:\n  variables:\n  - name: token\n    value: abc\ndocs:\n  content: '# Auth'\n  type: text/markdown\n",
    )
    .expect("write");

    let first = repo.get("my-api").expect("get");
    let folder = find_root_folder(&first);
    assert_eq!(folder.uid, "spec-folder-uid");
    assert_eq!(folder.name, "Auth Flows");
    let second = repo.get("my-api").expect("get");
    assert_eq!(find_root_folder(&second).uid, "spec-folder-uid");

    let vars = repo.get_folder_variables("my-api", "auth").expect("vars");
    assert_eq!(vars.len(), 1);
    assert_eq!(vars[0].key, "token");

    // Saving folder variables keeps the object-form docs intact.
    repo.save_folder_variables("my-api", "auth", Vec::new())
        .expect("save vars");
    let raw: serde_yaml::Value =
        serde_yaml::from_str(&fs::read_to_string(folder_dir.join("folder.yml")).expect("read"))
            .expect("yaml");
    assert_eq!(raw["docs"]["content"].as_str(), Some("# Auth"), "{raw:?}");
    assert_eq!(
        raw["docs"]["type"].as_str(),
        Some("text/markdown"),
        "{raw:?}"
    );
}

#[test]
fn save_folder_variables_without_folder_yml_names_folder_after_its_directory() {
    let (dir, repo) = setup();
    repo.create("my-api").expect("create");
    // A folder that exists on disk without folder.yml, e.g. created outside Rocket.
    fs::create_dir_all(dir.path().join("my-api/billing")).expect("mkdir");

    repo.save_folder_variables(
        "my-api",
        "billing",
        vec![CollectionVariable {
            key: "region".into(),
            value: "eu".into(),
            initial_value: String::new(),
            enabled: true,
            secret: false,
        }],
    )
    .expect("save vars");

    let raw: serde_yaml::Value = serde_yaml::from_str(
        &fs::read_to_string(dir.path().join("my-api/billing/folder.yml")).expect("read"),
    )
    .expect("yaml");
    assert_eq!(raw["info"]["name"].as_str(), Some("billing"), "{raw:?}");
    let col = repo.get("my-api").expect("get");
    assert_eq!(find_root_folder(&col).name, "billing");
}

fn gql_fixture(uid_line: &str) -> String {
    format!(
        "{uid_line}info:\n  name: List Users\n  type: graphql\ngraphql:\n  method: POST\n  url: https://api.example.com/graphql\n  body:\n    query: '{{ users {{ id }} }}'\n"
    )
}

#[test]
fn graphql_request_round_trips_through_the_repo() {
    let (dir, repo) = setup();
    repo.create("my-api").expect("create collection");
    let mut g = rocket_collection::GraphQlRequest::new(
        "List Users",
        "https://api.example.com/graphql",
    )
    .with_query("query Users { users { id } }");
    g.body.variables = Some("{\"first\": 5}".into());
    g.headers.push(rocket_shared::types::Header::new("X-Trace", "1"));

    let saved = repo
        .save_graphql_request("my-api", "list-users.yml", &g)
        .expect("save");
    assert_eq!(saved, "list-users.yml");

    let back = repo
        .get_graphql_request("my-api", "list-users.yml")
        .expect("get");
    assert_eq!(back.uid, g.uid);
    assert_eq!(back.body, g.body);
    assert_eq!(back.method, HttpMethod::Post);
    assert_eq!(back.headers.len(), 1);
    assert_eq!(back.file_name.as_deref(), Some("list-users.yml"));

    let yaml = fs::read_to_string(dir.path().join("my-api/list-users.yml")).expect("read yaml");
    assert!(yaml.contains("type: graphql"), "{yaml}");
    assert!(yaml.contains("graphql:"), "{yaml}");
    assert!(!yaml.contains("http:"), "{yaml}");
}

#[test]
fn get_graphql_request_gives_a_uid_less_file_an_in_memory_uid() {
    let (dir, repo) = setup();
    repo.create("my-api").expect("create collection");
    let path = dir.path().join("my-api/q.yml");
    fs::write(&path, gql_fixture("")).expect("write fixture");

    let g = repo.get_graphql_request("my-api", "q.yml").expect("get");
    assert!(!g.uid.is_empty());
    assert_eq!(
        fs::read_to_string(&path).expect("read back"),
        gql_fixture(""),
        "a read must not rewrite the file"
    );
}

#[test]
fn save_graphql_request_rejects_an_empty_uid() {
    let (_dir, repo) = setup();
    repo.create("my-api").expect("create collection");
    let mut g = rocket_collection::GraphQlRequest::new("A", "https://x/graphql");
    g.uid = String::new();
    assert!(repo.save_graphql_request("my-api", "a.yml", &g).is_err());
}

#[test]
fn save_graphql_request_keeps_unselected_variants_and_stored_variables() {
    let (dir, repo) = setup();
    repo.create("my-api").expect("create collection");
    fs::write(
        dir.path().join("my-api/multi.yml"),
        "uid: g1\ninfo:\n  name: Multi\n  type: graphql\ngraphql:\n  url: https://x/graphql\n  body:\n  - title: A\n    selected: true\n    body:\n      query: '{ a }'\n  - title: B\n    body:\n      query: '{ b }'\nruntime:\n  variables:\n  - name: tenant\n    value: acme\n",
    )
    .expect("write fixture");

    let mut g = repo
        .get_graphql_request("my-api", "multi.yml")
        .expect("get");
    // The IPC payload carries no request variables, so a save must not erase them.
    g.variables.clear();
    g.body.query = "{ a id }".into();
    repo.save_graphql_request("my-api", "multi.yml", &g)
        .expect("save");

    let yaml = fs::read_to_string(dir.path().join("my-api/multi.yml")).expect("read yaml");
    assert!(yaml.contains("title: B"), "{yaml}");
    assert!(yaml.contains("{ b }"), "{yaml}");
    assert!(yaml.contains("{ a id }"), "{yaml}");
    assert!(yaml.contains("name: tenant"), "{yaml}");
}

#[test]
fn request_kind_reads_the_protocol_key() {
    let (dir, repo) = setup();
    repo.create("my-api").expect("create collection");
    fs::write(dir.path().join("my-api/q.yml"), gql_fixture("")).expect("write gql");
    fs::write(dir.path().join("my-api/g.yml"), GRPC_ITEM_YML).expect("write grpc");
    let req = rocket_collection::Request::new("Good", HttpMethod::Get, "https://example.com");
    repo.save_request("my-api", "good.yml", &req)
        .expect("save http");

    use rocket_collection::RequestKind;
    assert_eq!(
        repo.request_kind("my-api", "q.yml").expect("gql kind"),
        RequestKind::GraphQl
    );
    assert_eq!(
        repo.request_kind("my-api", "g.yml").expect("grpc kind"),
        RequestKind::Grpc
    );
    assert_eq!(
        repo.request_kind("my-api", "good.yml").expect("http kind"),
        RequestKind::Http
    );
    assert!(repo.request_kind("my-api", "missing.yml").is_err());
}

#[test]
fn request_variables_work_for_a_graphql_file() {
    let (dir, repo) = setup();
    repo.create("my-api").expect("create collection");
    fs::write(dir.path().join("my-api/q.yml"), gql_fixture("uid: g1\n")).expect("write fixture");

    let vars = vec![CollectionVariable {
        key: "tenant".into(),
        value: "acme".into(),
        initial_value: String::new(),
        enabled: true,
        secret: false,
    }];
    repo.save_request_variables("my-api", "q.yml", vars)
        .expect("save vars");
    let back = repo
        .get_request_variables("my-api", "q.yml")
        .expect("get vars");
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].key, "tenant");

    // The save must not turn the file into an HTTP request.
    let g = repo.get_graphql_request("my-api", "q.yml").expect("get");
    assert_eq!(g.body.query, "{ users { id } }");
}

#[test]
fn full_tree_loads_graphql_as_a_typed_item_with_its_file_name() {
    let (dir, repo) = setup();
    repo.create("my-api").expect("create collection");
    fs::write(
        dir.path().join("my-api/list-users.yml"),
        gql_fixture("uid: g1\n"),
    )
    .expect("write fixture");

    let col = repo.get("my-api").expect("get collection");
    let found = col.root.items.iter().find_map(|i| match i {
        rocket_collection::CollectionItem::GraphQl(g) => Some(g),
        _ => None,
    });
    let g = found.expect("a typed GraphQl item");
    assert_eq!(g.name, "List Users");
    assert_eq!(g.uid, "g1");
    assert_eq!(g.file_name.as_deref(), Some("list-users.yml"));
}

#[test]
fn get_summaries_returns_a_graphql_summary_with_its_kind() {
    let (dir, repo) = setup();
    repo.create("my-api").expect("create collection");
    fs::write(
        dir.path().join("my-api/list-users.yml"),
        gql_fixture("uid: g1\n"),
    )
    .expect("write gql");
    fs::write(dir.path().join("my-api/get-user.yml"), GRPC_ITEM_YML).expect("write grpc");

    let col = repo.get_summaries("my-api").unwrap();
    assert_eq!(col.root.items.len(), 2, "GraphQL and gRPC both listed: {:?}", col.root.items);
    let s = col
        .root
        .items
        .iter()
        .find_map(|i| match i {
            rocket_collection::CollectionItem::Summary(s)
                if s.kind == rocket_collection::RequestKind::GraphQl =>
            {
                Some(s)
            }
            _ => None,
        })
        .expect("a GraphQL summary");
    assert_eq!(s.uid, "g1");
    assert_eq!(s.method, "POST");
    assert_eq!(s.url, "https://api.example.com/graphql");
    assert_eq!(s.file_name.as_deref(), Some("list-users.yml"));
}

#[test]
fn websocket_roundtrip_through_the_repo_preserves_every_field() {
    use rocket_collection::websocket::*;
    use rocket_collection::CollectionVariable;
    use rocket_shared::description::Description;
    use rocket_shared::types::{Auth, Header, RequestSettingValue};

    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();

    let mut ws = WebSocketRequest::new("Chat", "wss://chat.example.com/ws");
    ws.description = Some(Description::text("Team chat"));
    ws.seq = Some(4);
    ws.tags = vec!["realtime".into()];
    ws.headers = vec![Header::new("Origin", "https://example.com")];
    ws.messages = vec![
        WebSocketMessage { title: "hi".into(), selected: true, kind: WebSocketMessageKind::Json, data: "{}".into() },
        WebSocketMessage { title: "raw".into(), selected: false, kind: WebSocketMessageKind::Binary, data: "AQID".into() },
    ];
    ws.auth = Auth::Bearer { token: "t".into() };
    ws.runtime_auth = Some(Auth::Basic { username: "u".into(), password: "p".into() });
    ws.variables = vec![CollectionVariable {
        key: "room".into(),
        value: "general".into(),
        initial_value: "general".into(),
        enabled: true,
        secret: false,
    }];
    ws.scripts = vec![WebSocketScript { script_type: "before-request".into(), code: "// pre".into() }];
    ws.settings = Some(WebSocketSettings {
        timeout: Some(RequestSettingValue::Value(5000.0)),
        keep_alive_interval: Some(RequestSettingValue::Inherit("inherit".into())),
    });
    ws.docs = Some("# Chat".into());

    let written = repo.save_websocket_request("my-api", "chat", &ws).unwrap();
    assert_eq!(written, "chat.yml");

    let loaded = repo.get_websocket_request("my-api", "chat.yml").unwrap();
    let mut expected = ws.clone();
    expected.file_name = Some("chat.yml".into());
    assert_eq!(loaded, expected);
}

#[test]
fn single_untitled_message_is_written_in_the_single_form() {
    use rocket_collection::websocket::*;

    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    let mut ws = WebSocketRequest::new("Chat", "ws://x");
    ws.messages = vec![WebSocketMessage {
        title: String::new(),
        selected: true,
        kind: WebSocketMessageKind::Json,
        data: "{}".into(),
    }];
    repo.save_websocket_request("my-api", "chat", &ws).unwrap();

    let raw: serde_yaml::Value =
        serde_yaml::from_str(&fs::read_to_string(dir.path().join("my-api/chat.yml")).unwrap()).unwrap();
    assert!(raw["websocket"]["message"].is_mapping(), "{raw:?}");
    assert_eq!(raw["websocket"]["message"]["type"].as_str(), Some("json"));
    assert_eq!(raw["info"]["type"].as_str(), Some("websocket"));
}

#[test]
fn several_messages_are_written_as_variants() {
    use rocket_collection::websocket::*;

    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    let mut ws = WebSocketRequest::new("Chat", "ws://x");
    ws.messages = vec![
        WebSocketMessage { title: "a".into(), selected: true, kind: WebSocketMessageKind::Text, data: "1".into() },
        WebSocketMessage { title: "b".into(), selected: false, kind: WebSocketMessageKind::Text, data: "2".into() },
    ];
    repo.save_websocket_request("my-api", "chat", &ws).unwrap();

    let raw: serde_yaml::Value =
        serde_yaml::from_str(&fs::read_to_string(dir.path().join("my-api/chat.yml")).unwrap()).unwrap();
    assert!(raw["websocket"]["message"].is_sequence(), "{raw:?}");
    assert_eq!(raw["websocket"]["message"][1]["title"].as_str(), Some("b"));
}

#[test]
fn a_websocket_file_without_uid_loads_with_the_same_derived_uid_everywhere() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(dir.path().join("my-api/chat.yml"), WEBSOCKET_ITEM_YML).unwrap();

    let by_path = repo.get_websocket_request("my-api", "chat.yml").unwrap();
    assert_eq!(by_path.uid, "ws-chat.yml");

    let full = repo.get("my-api").unwrap();
    let in_tree = full
        .root
        .items
        .iter()
        .find_map(|i| match i {
            rocket_collection::CollectionItem::WebSocket(w) => Some(w),
            _ => None,
        })
        .expect("websocket item in the full tree");
    assert_eq!(in_tree.uid, by_path.uid);
    assert_eq!(in_tree.file_name.as_deref(), Some("chat.yml"));
}

#[test]
fn summary_loading_returns_a_websocket_summary_for_the_sidebar() {
    use rocket_collection::{CollectionItem, RequestKind};

    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(dir.path().join("my-api/chat.yml"), WEBSOCKET_ITEM_YML).unwrap();

    let col = repo.get_summaries("my-api").unwrap();

    assert_eq!(col.root.items.len(), 1, "{:?}", col.root.items);
    match &col.root.items[0] {
        CollectionItem::Summary(s) => {
            assert_eq!(s.kind, RequestKind::WebSocket);
            assert_eq!(s.name, "Chat");
            assert_eq!(s.method, "GET");
            assert_eq!(s.url, "wss://chat.example.com/ws");
            assert_eq!(s.file_name.as_deref(), Some("chat.yml"));
            // The same derived uid as a full load, so a tab opened from the sidebar keeps its id.
            assert_eq!(s.uid, "ws-chat.yml");
        }
        other => panic!("expected a summary, got {other:?}"),
    }
}

#[test]
fn request_kind_reports_websocket_files() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(dir.path().join("my-api/chat.yml"), WEBSOCKET_ITEM_YML).unwrap();
    assert_eq!(
        repo.request_kind("my-api", "chat.yml").unwrap(),
        rocket_collection::RequestKind::WebSocket
    );
}

#[test]
fn saving_a_websocket_request_keeps_runtime_variables_edited_on_their_own_path() {
    use rocket_collection::CollectionVariable;

    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    let ws = rocket_collection::WebSocketRequest::new("Chat", "ws://x");
    repo.save_websocket_request("my-api", "chat", &ws).unwrap();

    // Variables are edited through their own commands, not through the request payload.
    let var = CollectionVariable {
        key: "room".into(),
        value: "general".into(),
        initial_value: "general".into(),
        enabled: true,
        secret: false,
    };
    repo.save_request_variables("my-api", "chat.yml", vec![var.clone()]).unwrap();
    assert_eq!(repo.get_request_variables("my-api", "chat.yml").unwrap(), vec![var.clone()]);

    // A later save from the UI sends no variables and must not erase them.
    let mut again = repo.get_websocket_request("my-api", "chat.yml").unwrap();
    again.variables = Vec::new();
    again.name = "Chat v2".into();
    repo.save_websocket_request("my-api", "chat", &again).unwrap();

    assert_eq!(repo.get_request_variables("my-api", "chat.yml").unwrap(), vec![var]);
    let after = repo.get_websocket_request("my-api", "chat.yml").unwrap();
    assert_eq!(after.name, "Chat v2");
    assert_eq!(after.url, "ws://x", "the file is still a websocket file");
}

#[test]
fn get_websocket_on_an_http_file_is_an_error_not_a_panic() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    let req = rocket_collection::Request::new("Get", rocket_shared::types::HttpMethod::Get, "https://x");
    repo.save_request("my-api", "get", &req).unwrap();
    assert!(repo.get_websocket_request("my-api", "get.yml").is_err());
}

#[test]
fn save_websocket_rejects_an_empty_uid() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    let mut ws = rocket_collection::WebSocketRequest::new("Chat", "ws://x");
    ws.uid = String::new();
    assert!(repo.save_websocket_request("my-api", "chat", &ws).is_err());
}

fn grpc_item_yml(uid_line: &str) -> String {
    format!(
        "{uid_line}info:\n  name: Get User\n  type: grpc\ngrpc:\n  url: grpc://api.example.com\n  method: users.UserService/GetUser\n  methodType: unary\n"
    )
}

fn sample_grpc_request() -> rocket_collection::GrpcRequest {
    use rocket_collection::{GrpcMessage, GrpcMetadataEntry, GrpcMethodType};

    let mut g = rocket_collection::GrpcRequest::new("Say Hello", "localhost:50051");
    g.method = Some("demo.greeter.v1.Greeter/SayHello".into());
    g.method_type = GrpcMethodType::ServerStreaming;
    g.proto_file_path = Some("protos/greeter.proto".into());
    g.metadata = vec![GrpcMetadataEntry::new("x-trace", "abc")];
    g.messages = vec![GrpcMessage {
        title: String::new(),
        selected: true,
        content: "{\"name\": \"ada\"}".into(),
    }];
    g
}

#[test]
fn grpc_request_round_trips_through_the_repo() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    let g = sample_grpc_request();

    let saved = repo
        .save_grpc_request("my-api", "say-hello.yml", &g)
        .unwrap();
    assert_eq!(saved, "say-hello.yml");

    let back = repo.get_grpc_request("my-api", "say-hello.yml").unwrap();
    assert_eq!(back.uid, g.uid);
    assert_eq!(back.method, g.method);
    assert_eq!(back.method_type, g.method_type);
    assert_eq!(back.proto_file_path, g.proto_file_path);
    assert_eq!(back.metadata, g.metadata);
    assert_eq!(back.messages, g.messages);
    assert_eq!(back.file_name.as_deref(), Some("say-hello.yml"));

    let raw = read_yaml_value(&dir.path().join("my-api/say-hello.yml"));
    assert_eq!(raw["info"]["type"].as_str(), Some("grpc"), "{raw:?}");
    assert!(raw.get("grpc").is_some(), "{raw:?}");
    assert!(raw.get("http").is_none(), "{raw:?}");
    assert_eq!(
        raw["grpc"]["methodType"].as_str(),
        Some("server-streaming"),
        "{raw:?}"
    );
}

#[test]
fn a_single_untitled_message_is_saved_as_a_plain_string() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.save_grpc_request("my-api", "a.yml", &sample_grpc_request())
        .unwrap();
    let raw = read_yaml_value(&dir.path().join("my-api/a.yml"));
    assert_eq!(
        raw["grpc"]["message"].as_str(),
        Some("{\"name\": \"ada\"}"),
        "{raw:?}"
    );
}

#[test]
fn get_grpc_request_gives_a_uid_less_file_an_in_memory_uid() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    let path = dir.path().join("my-api/get-user.yml");
    fs::write(&path, grpc_item_yml("")).unwrap();

    let g = repo.get_grpc_request("my-api", "get-user.yml").unwrap();
    assert!(!g.uid.is_empty());
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        grpc_item_yml(""),
        "a read must not rewrite the file"
    );
}

#[test]
fn an_opaque_era_grpc_file_keeps_its_call_description_through_load_and_save() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(dir.path().join("my-api/get-user.yml"), grpc_item_yml("")).unwrap();

    let g = repo.get_grpc_request("my-api", "get-user.yml").unwrap();
    repo.save_grpc_request("my-api", "get-user.yml", &g)
        .unwrap();

    let raw = read_yaml_value(&dir.path().join("my-api/get-user.yml"));
    assert_eq!(raw["info"]["name"].as_str(), Some("Get User"), "{raw:?}");
    assert_eq!(
        raw["grpc"]["url"].as_str(),
        Some("grpc://api.example.com"),
        "{raw:?}"
    );
    assert_eq!(
        raw["grpc"]["method"].as_str(),
        Some("users.UserService/GetUser"),
        "{raw:?}"
    );
    assert_eq!(raw["grpc"]["methodType"].as_str(), Some("unary"), "{raw:?}");
    assert!(
        raw["uid"].as_str().is_some(),
        "the save persists the uid: {raw:?}"
    );
}

#[test]
fn save_grpc_request_rejects_an_empty_uid() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    let mut g = sample_grpc_request();
    g.uid = String::new();
    assert!(repo.save_grpc_request("my-api", "a.yml", &g).is_err());
}

#[test]
fn save_grpc_request_keeps_stored_variables_when_the_payload_has_none() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(
        dir.path().join("my-api/get-user.yml"),
        "uid: g1\ninfo:\n  name: Get User\n  type: grpc\ngrpc:\n  url: h:1\nruntime:\n  variables:\n  - name: tenant\n    value: acme\n",
    )
    .unwrap();

    let mut g = repo.get_grpc_request("my-api", "get-user.yml").unwrap();
    g.variables.clear();
    g.name = "Renamed".into();
    repo.save_grpc_request("my-api", "get-user.yml", &g)
        .unwrap();

    let yaml = fs::read_to_string(dir.path().join("my-api/get-user.yml")).unwrap();
    assert!(yaml.contains("name: tenant"), "{yaml}");
    assert!(yaml.contains("name: Renamed"), "{yaml}");
}

#[test]
fn request_variables_work_for_a_grpc_file() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(
        dir.path().join("my-api/get-user.yml"),
        grpc_item_yml("uid: g1\n"),
    )
    .unwrap();

    let vars = vec![CollectionVariable {
        key: "tenant".into(),
        value: "acme".into(),
        initial_value: String::new(),
        enabled: true,
        secret: false,
    }];
    repo.save_request_variables("my-api", "get-user.yml", vars)
        .unwrap();
    let back = repo
        .get_request_variables("my-api", "get-user.yml")
        .unwrap();
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].key, "tenant");

    // The save must not turn the file into an HTTP request.
    let g = repo.get_grpc_request("my-api", "get-user.yml").unwrap();
    assert_eq!(g.url, "grpc://api.example.com");
    assert_eq!(g.variables.len(), 1);
}

#[test]
fn request_kind_reports_grpc_for_a_grpc_file() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(dir.path().join("my-api/get-user.yml"), grpc_item_yml("")).unwrap();
    assert_eq!(
        repo.request_kind("my-api", "get-user.yml").unwrap(),
        rocket_collection::RequestKind::Grpc
    );
}

#[test]
fn full_tree_loads_grpc_as_a_typed_item_with_its_file_name() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "users").unwrap();
    fs::write(
        dir.path().join("my-api/users/get-user.yml"),
        grpc_item_yml("uid: g1\n"),
    )
    .unwrap();

    let col = repo.get("my-api").unwrap();
    let users = col.root.find_folder("users").unwrap();
    let found = users.items.iter().find_map(|i| match i {
        rocket_collection::CollectionItem::Grpc(g) => Some(g),
        _ => None,
    });
    let g = found.expect("a typed Grpc item");
    assert_eq!(g.name, "Get User");
    assert_eq!(g.uid, "g1");
    assert_eq!(g.url, "grpc://api.example.com");
    assert_eq!(g.file_name.as_deref(), Some("get-user.yml"));
    assert_eq!(col.root.request_count(), 1);
}

#[test]
fn get_summaries_returns_a_grpc_summary_with_its_kind() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(
        dir.path().join("my-api/get-user.yml"),
        grpc_item_yml("uid: g1\n"),
    )
    .unwrap();

    let col = repo.get_summaries("my-api").unwrap();
    assert_eq!(col.root.items.len(), 1, "{:?}", col.root.items);
    match &col.root.items[0] {
        rocket_collection::CollectionItem::Summary(s) => {
            assert_eq!(s.kind, rocket_collection::RequestKind::Grpc);
            assert_eq!(s.uid, "g1");
            assert_eq!(s.name, "Get User");
            assert_eq!(s.method, "GRPC");
            assert_eq!(s.url, "grpc://api.example.com");
            assert_eq!(s.file_name.as_deref(), Some("get-user.yml"));
        }
        other => panic!("expected a summary, got {other:?}"),
    }
}

#[test]
fn rename_item_and_move_keep_working_for_a_grpc_file() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "users").unwrap();
    fs::write(
        dir.path().join("my-api/get-user.yml"),
        grpc_item_yml("uid: g1\n"),
    )
    .unwrap();

    repo.move_item("my-api", "get-user.yml", "my-api", "users/get-user.yml")
        .unwrap();
    assert!(dir.path().join("my-api/users/get-user.yml").exists());
    repo.delete_request("my-api", "users/get-user.yml").unwrap();
    assert!(!dir.path().join("my-api/users/get-user.yml").exists());
}

#[test]
fn a_uid_less_grpc_file_has_one_stable_uid_everywhere() {
    let (dir, repo) = setup();
    repo.create("my-api").expect("create");
    fs::write(dir.path().join("my-api/get-user.yml"), grpc_item_yml("")).expect("write");

    let summary_uid = match &repo.get_summaries("my-api").expect("summaries").root.items[0] {
        rocket_collection::CollectionItem::Summary(s) => s.uid.clone(),
        other => panic!("expected a summary, got {other:?}"),
    };
    let loaded = repo.get_grpc_request("my-api", "get-user.yml").expect("get");
    let again = repo.get_grpc_request("my-api", "get-user.yml").expect("get again");
    let tree_uid = match &repo.get("my-api").expect("tree").root.items[0] {
        rocket_collection::CollectionItem::Grpc(g) => g.uid.clone(),
        other => panic!("expected a grpc item, got {other:?}"),
    };

    assert!(!summary_uid.is_empty(), "the sidebar needs an id to open the tab by");
    assert_eq!(summary_uid, loaded.uid);
    assert_eq!(loaded.uid, again.uid, "a read must give the same uid every time");
    assert_eq!(loaded.uid, tree_uid);
}

#[test]
fn saving_collection_settings_keeps_scripts_metadata_and_request_settings() {
    let (dir, repo) = setup();
    repo.create("my-api").expect("create");
    let file = dir.path().join("my-api/opencollection.yml");
    fs::write(
        &file,
        "opencollection: 1.0.0\ninfo:\n  name: my-api\nrequest:\n  headers:\n    - name: X-A\n      value: \"1\"\n  scripts:\n    - type: before-request\n      code: \"// collection script\"\n  metadata:\n    - name: x-meta\n      value: m\n  settings:\n    timeout: 5000\n",
    )
    .expect("write");

    let mut settings = repo.get_settings("my-api").expect("settings");
    settings.headers.clear();
    repo.save_settings("my-api", &settings).expect("save");

    let saved = fs::read_to_string(&file).expect("read");
    assert!(saved.contains("// collection script"), "scripts kept: {saved}");
    assert!(saved.contains("x-meta"), "metadata kept: {saved}");
    assert!(saved.contains("timeout: 5000"), "request settings kept: {saved}");
    assert!(!saved.contains("X-A"), "the edited header list is respected: {saved}");
}

#[test]
fn saving_settings_without_defaults_writes_no_request_block() {
    let (dir, repo) = setup();
    repo.create("my-api").expect("create");
    repo.save_settings("my-api", &CollectionSettings::default())
        .expect("save");
    let saved = fs::read_to_string(dir.path().join("my-api/opencollection.yml")).expect("read");
    assert!(!saved.contains("request:"), "{saved}");
}

#[test]
fn settings_script_context_roots_roundtrip_and_keep_other_extensions() {
    let (_dir, repo) = setup();
    repo.create("col").expect("create");
    let path = repo.settings_path("col");
    let existing = std::fs::read_to_string(&path).expect("read");
    let with_other = format!(
        "{existing}extensions:\n  rocketapi:\n    sandboxMode: safe\n    keep: me\n  other:\n    x: 1\n"
    );
    std::fs::write(&path, with_other).expect("write fixture");

    let mut settings = repo.get_settings("col").expect("get");
    assert!(settings.script_context_roots.is_empty());
    settings.script_context_roots = vec!["../shared".into(), "./more".into()];
    repo.save_settings("col", &settings).expect("save");

    let loaded = repo.get_settings("col").expect("reload");
    assert_eq!(loaded.script_context_roots, vec!["../shared", "./more"]);
    let yaml = std::fs::read_to_string(&path).expect("read back");
    assert!(
        yaml.contains("keep: me"),
        "other rocketapi keys kept: {yaml}"
    );
    assert!(yaml.contains("other:"), "other extensions kept: {yaml}");
    assert!(
        yaml.contains("additionalContextRoots"),
        "key written: {yaml}"
    );
}

#[test]
fn settings_script_context_roots_empty_removes_the_key() {
    let (_dir, repo) = setup();
    repo.create("col").expect("create");
    let mut settings = repo.get_settings("col").expect("get");
    settings.script_context_roots = vec!["../shared".into()];
    repo.save_settings("col", &settings).expect("save");
    settings.script_context_roots.clear();
    repo.save_settings("col", &settings).expect("save empty");
    let yaml = std::fs::read_to_string(repo.settings_path("col")).expect("read");
    assert!(
        !yaml.contains("additionalContextRoots"),
        "key removed: {yaml}"
    );
}

#[test]
fn collection_root_path_returns_the_directory_and_rejects_unknown() {
    let (_dir, repo) = setup();
    repo.create("col").expect("create");
    let path = repo.collection_root_path("col").expect("exists");
    assert!(path.is_dir());
    assert!(repo.collection_root_path("missing").is_err());
    assert!(repo.collection_root_path("../escape").is_err());
}

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
    assert!(collection.root.items.iter().all(
        |i| !matches!(i, rocket_collection::CollectionItem::Folder(f) if f.name == "node_modules")
    ));
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
    let nested = repo
        .create_script_file("col", "lib", "helper.js")
        .expect("nested");
    assert_eq!(nested, "lib/helper.js");

    let text = text_of(&dir, "col/lib/helper.js");
    assert!(text.contains("module.exports"));
    assert!(
        text.contains("helper.js"),
        "template names the file: {text}"
    );
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
    assert_eq!(
        text_of(&dir, "col/utils.js"),
        "keep me",
        "existing file untouched"
    );
    assert!(repo.create_script_file("col", "", "../evil").is_err());
    assert!(repo.create_script_file("col", "", "a/b").is_err());
    assert!(repo
        .create_script_file("col", "no-such-folder", "x")
        .is_err());
    assert!(repo.create_script_file("col", "../..", "x").is_err());
}

#[test]
fn script_save_and_read_roundtrip_and_reject_non_scripts() {
    let (dir, repo) = setup();
    repo.create("col").expect("create");
    repo.create_script_file("col", "", "utils")
        .expect("create script");

    repo.save_script_file("col", "utils.js", "module.exports = 42;")
        .expect("save");
    assert_eq!(
        repo.read_script_file("col", "utils.js").expect("read"),
        "module.exports = 42;"
    );

    let settings_before = text_of(&dir, "col/opencollection.yml");
    assert!(repo
        .save_script_file("col", "opencollection.yml", "x")
        .is_err());
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

    let renamed = repo
        .rename_script_file("col", "lib/a.js", "c")
        .expect("rename");
    assert_eq!(renamed, "lib/c.js");
    assert!(!dir.path().join("col/lib/a.js").exists());
    assert!(dir.path().join("col/lib/c.js").exists());

    assert!(repo.rename_script_file("col", "lib/c.js", "b").is_err());
    assert_eq!(
        text_of(&dir, "col/lib/b.js"),
        "b content",
        "target untouched"
    );
    assert!(dir.path().join("col/lib/c.js").exists(), "source untouched");
    assert!(repo.rename_script_file("col", "lib/c.js", "../x").is_err());

    repo.delete_script_file("col", "lib/c.js").expect("delete");
    assert!(!dir.path().join("col/lib/c.js").exists());
    assert!(repo.delete_script_file("col", "lib/c.js").is_err());
    assert!(repo
        .delete_script_file("col", "opencollection.yml")
        .is_err());
    assert!(dir.path().join("col/opencollection.yml").exists());
}

#[test]
fn script_create_returns_a_normalised_relative_path() {
    let (dir, repo) = setup();
    repo.create("col").expect("create");
    repo.create_folder("col", "lib").expect("lib");
    repo.create_folder("col", "b").expect("b");
    repo.create_folder("col", "a").expect("a");

    assert_eq!(
        repo.create_script_file("col", "./lib", "one").expect("dot"),
        "lib/one.js"
    );
    assert_eq!(
        repo.create_script_file("col", "a/../b", "two")
            .expect("dotdot"),
        "b/two.js"
    );
    assert_eq!(
        repo.create_script_file("col", "lib/", "three")
            .expect("slash"),
        "lib/three.js"
    );
    let abs = dir.path().join("col/lib");
    if let Ok(rel) = repo.create_script_file("col", &abs.to_string_lossy(), "four") {
        assert_eq!(rel, "lib/four.js");
    }
}

#[test]
fn script_ops_reject_folders_the_tree_hides() {
    let (dir, repo) = setup();
    repo.create("col").expect("create");
    let hidden_dirs = [
        ".git",
        "node_modules",
        "environments",
        "flows",
        "lib/.cache",
    ];
    for hidden in hidden_dirs {
        fs::create_dir_all(dir.path().join("col").join(hidden)).expect("dir");
    }
    for hidden in hidden_dirs {
        assert!(
            repo.create_script_file("col", hidden, "x").is_err(),
            "create in {hidden}"
        );
        let rel = format!("{hidden}/x.js");
        fs::write(dir.path().join("col").join(&rel), "data").expect("fixture");
        assert!(repo.read_script_file("col", &rel).is_err(), "read {rel}");
        assert!(
            repo.save_script_file("col", &rel, "y").is_err(),
            "save {rel}"
        );
        assert!(
            repo.rename_script_file("col", &rel, "z").is_err(),
            "rename {rel}"
        );
        assert!(
            repo.delete_script_file("col", &rel).is_err(),
            "delete {rel}"
        );
        assert_eq!(text_of(&dir, &format!("col/{rel}")), "data");
    }
    // A nested `flows` folder is visible in the tree, so scripts there are allowed.
    fs::create_dir_all(dir.path().join("col/lib/flows")).expect("nested flows");
    assert_eq!(
        repo.create_script_file("col", "lib/flows", "ok")
            .expect("nested flows"),
        "lib/flows/ok.js"
    );
}

#[test]
fn script_rename_returns_a_normalised_path() {
    let (_dir, repo) = setup();
    repo.create("col").expect("create");
    repo.create_folder("col", "lib").expect("lib");
    repo.create_script_file("col", "lib", "a").expect("a");
    assert_eq!(
        repo.rename_script_file("col", "./lib/../lib/a.js", "b")
            .expect("rename"),
        "lib/b.js"
    );
}

fn bruno_flow(yaml: &str) -> Option<String> {
    let doc: serde_yaml::Value = serde_yaml::from_str(yaml).expect("parse opencollection.yml");
    doc.get("extensions")
        .and_then(|v| v.get("bruno"))
        .and_then(|v| v.get("scripts"))
        .and_then(|v| v.get("flow"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

#[test]
fn settings_default_save_writes_no_bruno_extension() {
    let (_dir, repo) = setup();
    repo.create("col").expect("create");
    let settings = repo.get_settings("col").expect("get");
    assert_eq!(settings.script_flow, rocket_collection::ScriptFlow::Sandwich);
    repo.save_settings("col", &settings).expect("save");
    let yaml = fs::read_to_string(repo.settings_path("col")).expect("read");
    assert!(!yaml.contains("bruno"), "no bruno key for sandwich: {yaml}");
    assert!(!yaml.contains("flow"), "no flow key for sandwich: {yaml}");
}

#[test]
fn settings_script_flow_reads_a_bruno_authored_file() {
    let (_dir, repo) = setup();
    repo.create("col").expect("create");
    let path = repo.settings_path("col");
    let existing = fs::read_to_string(&path).expect("read");
    fs::write(
        &path,
        format!("{existing}extensions:\n  bruno:\n    scripts:\n      flow: sequential\n"),
    )
    .expect("write fixture");
    let loaded = repo.get_settings("col").expect("get");
    assert_eq!(loaded.script_flow, rocket_collection::ScriptFlow::Sequential);
}

#[test]
fn settings_script_flow_sequential_roundtrips_and_keeps_other_extensions() {
    let (_dir, repo) = setup();
    repo.create("col").expect("create");
    let path = repo.settings_path("col");
    let existing = fs::read_to_string(&path).expect("read");
    fs::write(
        &path,
        format!(
            "{existing}extensions:\n  rocketapi:\n    sandboxMode: developer\n    keep: me\n    scripts:\n      additionalContextRoots:\n        - ../shared\n  bruno:\n    other: 1\n  other:\n    x: 1\n"
        ),
    )
    .expect("write fixture");

    let mut settings = repo.get_settings("col").expect("get");
    assert_eq!(settings.sandbox_mode, SandboxMode::Developer);
    assert_eq!(settings.script_context_roots, vec!["../shared"]);
    settings.script_flow = rocket_collection::ScriptFlow::Sequential;
    repo.save_settings("col", &settings).expect("save");

    let loaded = repo.get_settings("col").expect("reload");
    assert_eq!(loaded.script_flow, rocket_collection::ScriptFlow::Sequential);
    assert_eq!(loaded.sandbox_mode, SandboxMode::Developer);
    assert_eq!(loaded.script_context_roots, vec!["../shared"]);
    let yaml = fs::read_to_string(&path).expect("read back");
    assert_eq!(bruno_flow(&yaml).as_deref(), Some("sequential"), "{yaml}");
    assert!(yaml.contains("keep: me"), "rocketapi keys kept: {yaml}");
    assert!(yaml.contains("other: 1"), "bruno siblings kept: {yaml}");
    assert!(yaml.contains("x: 1"), "foreign namespaces kept: {yaml}");

    // A read-modify-write that only edits variables (the script-side
    // `rok.setCollectionVar` path) keeps the flow.
    let mut again = repo.get_settings("col").expect("get again");
    again.variables.push(CollectionVariable {
        key: "k".into(),
        value: "v".into(),
        initial_value: String::new(),
        enabled: true,
        secret: false,
    });
    repo.save_settings("col", &again).expect("save variables");
    assert_eq!(
        repo.get_settings("col").expect("reload").script_flow,
        rocket_collection::ScriptFlow::Sequential
    );
}

#[test]
fn settings_script_flow_back_to_sandwich_removes_the_bruno_stub() {
    let (_dir, repo) = setup();
    repo.create("col").expect("create");
    let mut settings = repo.get_settings("col").expect("get");
    settings.script_flow = rocket_collection::ScriptFlow::Sequential;
    repo.save_settings("col", &settings).expect("save sequential");
    settings.script_flow = rocket_collection::ScriptFlow::Sandwich;
    repo.save_settings("col", &settings).expect("save sandwich");
    let yaml = fs::read_to_string(repo.settings_path("col")).expect("read");
    assert!(!yaml.contains("bruno"), "bruno stub removed: {yaml}");
    assert!(yaml.contains("sandboxMode: safe"), "rocketapi kept: {yaml}");
}

#[test]
fn path_exists_sees_hidden_items_and_ignores_case() {
    let (dir, repo) = setup();
    repo.create("col").expect("create");
    let root = dir.path().join("col");
    fs::create_dir_all(root.join("environments")).expect("environments");
    fs::create_dir_all(root.join("Reports")).expect("reports");
    fs::write(root.join("Reports").join("get-users.yml"), "x").expect("file");
    assert!(repo.path_exists("col", "opencollection.yml").expect("check"));
    assert!(repo.path_exists("col", "environments").expect("check"));
    assert!(repo.path_exists("col", "reports").expect("check"));
    assert!(repo.path_exists("col", "REPORTS/Get-Users.yml").expect("check"));
    assert!(!repo.path_exists("col", "reports/other.yml").expect("check"));
    assert!(!repo.path_exists("col", "missing/other.yml").expect("check"));
}

#[cfg(unix)]
#[test]
fn path_exists_refuses_a_symlinked_parent() {
    let (dir, repo) = setup();
    repo.create("col").expect("create");
    let outside = dir.path().join("outside");
    fs::create_dir_all(&outside).expect("outside");
    std::os::unix::fs::symlink(&outside, dir.path().join("col").join("link")).expect("symlink");
    assert!(repo.path_exists("col", "link/x.yml").is_err());
    assert!(repo.path_exists("col", "link").expect("the link itself exists"));
}

#[test]
fn create_folder_exclusive_refuses_an_existing_folder_and_keeps_its_metadata() {
    let (dir, repo) = setup();
    repo.create("col").expect("create");
    repo.create_folder_exclusive("col", "reports")
        .expect("first create");
    let folder_yml = dir.path().join("col/reports/folder.yml");
    let before = fs::read_to_string(&folder_yml).expect("folder.yml");
    let err = repo
        .create_folder_exclusive("col", "reports")
        .expect_err("exists");
    assert!(matches!(err, DomainError::AlreadyExists(_)));
    assert_eq!(fs::read_to_string(&folder_yml).expect("folder.yml"), before);
    assert!(repo.create_folder_exclusive("col", "missing/child").is_err());
}

#[test]
fn create_request_exclusive_never_replaces_a_file() {
    let (dir, repo) = setup();
    repo.create("col").expect("create");
    let request = Request::new("One", HttpMethod::Get, "https://one.example.com");
    let path = repo
        .create_request_exclusive("col", "one.yml", &request)
        .expect("first create");
    assert_eq!(path, "one.yml");
    let file = dir.path().join("col/one.yml");
    let before = fs::read_to_string(&file).expect("file");
    let other = Request::new("Two", HttpMethod::Post, "https://two.example.com");
    let err = repo
        .create_request_exclusive("col", "one.yml", &other)
        .expect_err("exists");
    assert!(matches!(err, DomainError::AlreadyExists(_)));
    assert_eq!(fs::read_to_string(&file).expect("file"), before);
}

#[test]
fn move_item_no_replace_keeps_the_destination() {
    let (dir, repo) = setup();
    repo.create("col").expect("create");
    repo.create_folder("col", "a").expect("a");
    repo.create_folder("col", "b").expect("b");
    fs::write(dir.path().join("col/a/x.yml"), "from a").expect("x in a");
    fs::write(dir.path().join("col/b/x.yml"), "from b").expect("x in b");
    let err = repo
        .move_item_no_replace("col", "a/x.yml", "col", "b/x.yml")
        .expect_err("destination exists");
    assert!(matches!(err, DomainError::AlreadyExists(_)));
    assert_eq!(
        fs::read_to_string(dir.path().join("col/b/x.yml")).expect("read"),
        "from b"
    );
    assert!(dir.path().join("col/a/x.yml").is_file());
    repo.move_item_no_replace("col", "a/x.yml", "col", "b/y.yml")
        .expect("free destination");
    assert!(dir.path().join("col/b/y.yml").is_file());
}

#[test]
fn path_exists_treats_unicode_variants_as_the_same_name() {
    let (dir, repo) = setup();
    repo.create("col").expect("create");
    fs::create_dir_all(dir.path().join("col").join("caf\u{e9}")).expect("dir");
    assert!(repo.path_exists("col", "cafe\u{301}").expect("check"));
}
