//! Tests for deleting requests from the sync page. They reuse the apply world.

use super::*;
use crate::openapi_sync_delete::DeleteTarget;
use rocket_collection::openapi_baseline::OpenApiBaselineRepository;
use std::sync::atomic::Ordering;

fn add_request(w: &World, file: &str, name: &str) {
    let request = rocket_collection::Request::new(
        name,
        rocket_shared::types::HttpMethod::Get,
        "https://x.example.com",
    );
    w.repo.save_request(&w.name, file, &request).expect("save");
}

fn added_target(w: &World, name: &str) -> DeleteTarget {
    let view = w.svc.changes(&w.name).expect("changes");
    let row = view
        .added
        .iter()
        .find(|r| r.name == name)
        .expect("added row");
    DeleteTarget {
        uid: row.uid.clone(),
        request_path: row.request_path.clone().expect("path"),
    }
}

fn target_of(w: &World, key: &str) -> DeleteTarget {
    let op = w
        .source()
        .operations
        .iter()
        .find(|o| o.key == key)
        .cloned()
        .expect("operation");
    DeleteTarget {
        uid: op.uid,
        request_path: op.request_path,
    }
}

fn edit_url(w: &World, key: &str) {
    let path = target_of(w, key).request_path;
    let mut request = w.repo.get_request(&w.name, &path).expect("get");
    request.url = "https://edited.example.com/x".to_string();
    w.repo.save_request(&w.name, &path, &request).expect("save");
}

fn exists(w: &World, path: &str) -> bool {
    w.repo.get_request(&w.name, path).is_ok()
}

#[tokio::test]
async fn deleting_a_request_added_by_you_removes_the_file() {
    let w = world().await;
    add_request(&w, "mine", "Mine");
    let target = added_target(&w, "Mine");
    let count = w.svc.delete_requests(&w.name, &[target]).expect("delete");
    assert_eq!(count, 1);
    assert!(!exists(&w, "mine"));
    assert!(w.svc.changes(&w.name).expect("changes").added.is_empty());
}

#[tokio::test]
async fn deleting_an_edited_removed_request_removes_the_file() {
    let w = world().await;
    edit_url(&w, "getInventory");
    w.check_with(v2()).await;
    let target = target_of(&w, "getInventory");
    assert!(exists(&w, &target.request_path));
    w.svc
        .delete_requests(&w.name, std::slice::from_ref(&target))
        .expect("delete");
    assert!(!exists(&w, &target.request_path));
}

#[tokio::test]
async fn deleting_a_removed_request_drops_its_source_operation() {
    let w = world().await;
    edit_url(&w, "getInventory");
    w.check_with(v2()).await;
    let target = target_of(&w, "getInventory");
    w.svc.delete_requests(&w.name, &[target]).expect("delete");
    assert!(w
        .source()
        .operations
        .iter()
        .all(|o| o.key != "getInventory"));
    // The plan no longer reports the removal, and the count follows.
    assert!(w.plan().items.iter().all(|i| i.key() != "getInventory"));
}

#[tokio::test]
async fn the_baseline_entry_is_dropped_with_the_operation() {
    let w = world().await;
    edit_url(&w, "getInventory");
    w.check_with(v2()).await;
    let target = target_of(&w, "getInventory");
    w.svc.delete_requests(&w.name, &[target]).expect("delete");
    let baseline = rocket_infra::FsOpenApiBaselineRepo
        .load(&w.root)
        .expect("load")
        .expect("baseline");
    assert!(baseline.operations.iter().all(|o| o.key != "getInventory"));
    assert!(baseline.operations.iter().any(|o| o.key == "listPets"));
}

#[tokio::test]
async fn a_synced_unedited_request_is_refused() {
    let w = world().await;
    let target = target_of(&w, "listPets");
    let err = w
        .svc
        .delete_requests(&w.name, std::slice::from_ref(&target))
        .expect_err("refused");
    assert!(err.to_string().contains("still part of the spec"), "{err}");
    assert!(exists(&w, &target.request_path));
}

#[tokio::test]
async fn an_edited_request_that_is_still_in_the_spec_is_refused() {
    let w = world().await;
    edit_url(&w, "listPets");
    let target = target_of(&w, "listPets");
    w.svc
        .delete_requests(&w.name, std::slice::from_ref(&target))
        .expect_err("refused");
    assert!(exists(&w, &target.request_path));
}

#[tokio::test]
async fn an_unedited_removed_request_is_refused() {
    let w = world().await;
    w.check_with(v2()).await;
    let target = target_of(&w, "getInventory");
    w.svc
        .delete_requests(&w.name, std::slice::from_ref(&target))
        .expect_err("refused");
    assert!(exists(&w, &target.request_path));
}

#[tokio::test]
async fn a_changed_uid_is_refused() {
    let w = world().await;
    add_request(&w, "mine", "Mine");
    let mut target = added_target(&w, "Mine");
    target.uid = "some-other-uid".to_string();
    let err = w
        .svc
        .delete_requests(&w.name, &[target])
        .expect_err("refused");
    assert!(err.to_string().contains("changed on disk"), "{err}");
    assert!(exists(&w, "mine"));
}

#[tokio::test]
async fn a_path_outside_the_collection_is_refused() {
    let w = world().await;
    add_request(&w, "mine", "Mine");
    let uid = added_target(&w, "Mine").uid;
    for path in ["../outside", "/etc/passwd", "a/../../b"] {
        let err = w
            .svc
            .delete_requests(
                &w.name,
                &[DeleteTarget {
                    uid: uid.clone(),
                    request_path: path.to_string(),
                }],
            )
            .expect_err(path);
        assert!(
            matches!(err, DomainError::InvalidInput(_)),
            "{path}: {err:?}"
        );
    }
    assert!(exists(&w, "mine"));
}

#[tokio::test]
async fn delete_refuses_while_an_apply_runs() {
    let blocked = Arc::new(AtomicBool::new(true));
    let factory = Wrapper {
        inner: rocket_infra::FsFileTxFactory::with_crash(2),
        fail_stage_at: None,
        recover_blocked: Arc::clone(&blocked),
    };
    let w = world_with(Arc::new(factory)).await;
    add_request(&w, "mine", "Mine");
    let target = added_target(&w, "Mine");
    w.check_with(v2()).await;
    let plan = w.plan();
    w.svc
        .apply(&w.name, &w.approve(&plan, &[]))
        .await
        .expect_err("recovery is blocked");
    let err = w.svc.delete_requests(&w.name, &[target]).expect_err("busy");
    assert!(matches!(err, DomainError::Conflict(_)), "{err:?}");
    assert!(exists(&w, "mine"));
}

#[tokio::test]
async fn a_failed_delete_leaves_all_files() {
    let factory = Wrapper {
        inner: rocket_infra::FsFileTxFactory::new(),
        fail_stage_at: Some(2),
        recover_blocked: Arc::new(AtomicBool::new(false)),
    };
    let w = world_with(Arc::new(factory)).await;
    add_request(&w, "one", "One");
    add_request(&w, "two", "Two");
    let targets = [added_target(&w, "One"), added_target(&w, "Two")];
    w.svc
        .delete_requests(&w.name, &targets)
        .expect_err("injected failure");
    assert!(exists(&w, "one") && exists(&w, "two"));
}

#[tokio::test]
async fn delete_never_touches_the_network() {
    let w = world().await;
    add_request(&w, "mine", "Mine");
    let target = added_target(&w, "Mine");
    w.fetcher.requests.lock().expect("lock").clear();
    w.svc.delete_requests(&w.name, &[target]).expect("delete");
    assert!(w.fetcher.requests.lock().expect("lock").is_empty());
    assert_eq!(w.secret_reads.load(Ordering::SeqCst), 0);
}
