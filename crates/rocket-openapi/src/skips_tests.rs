//! Tests for the skipped list and the un-skip. They reuse the world of the apply tests.

use super::*;
use rocket_import::SkipRequest;

fn skip_request(w: &World, key: &str) -> SkipRequest {
    let plan = w.plan();
    let item = plan.items.iter().find(|i| i.key() == key).expect("item");
    SkipRequest {
        key: key.to_string(),
        fingerprint: item.fingerprint().to_string(),
    }
}

async fn skip(w: &World, keys: &[&str]) {
    let selection = Selection {
        skips: keys.iter().map(|k| skip_request(w, k)).collect(),
        ..Selection::default()
    };
    w.svc.apply(&w.name, &selection).await.expect("skip");
}

fn local_skips(w: &World) -> Vec<String> {
    w.svc
        .local_state(&w.name)
        .expect("local")
        .skips
        .keys()
        .cloned()
        .collect()
}

#[tokio::test]
async fn skipped_lists_the_hidden_items_with_names() {
    let w = world().await;
    w.check_with(v2()).await;
    skip(&w, &["listPets"]).await;
    let rows = w.svc.skipped(&w.name).expect("skipped");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].key, "listPets");
    assert!(!rows[0].name.is_empty());
    assert_eq!(rows[0].method, "GET");
}

#[tokio::test]
async fn skipped_is_empty_when_nothing_is_pending() {
    let w = world().await;
    assert!(w.svc.skipped(&w.name).expect("skipped").is_empty());
}

#[tokio::test]
async fn a_skip_for_a_removed_operation_is_listed_by_key() {
    let w = world().await;
    w.check_with(v2()).await;
    let locals = w.svc.local_repo().expect("locals");
    let mut local = locals.load(&w.root).expect("load").unwrap_or_default();
    local.skips.insert("gone".into(), "fp".into());
    locals.save(&w.root, &local).expect("save");
    let rows = w.svc.skipped(&w.name).expect("skipped");
    let row = rows.iter().find(|r| r.key == "gone").expect("row");
    assert_eq!(row.name, "gone");
    assert!(row.method.is_empty());
    assert!(row.path.is_empty());
}

#[tokio::test]
async fn skipped_never_touches_the_network() {
    let w = world().await;
    w.check_with(v2()).await;
    skip(&w, &["listPets"]).await;
    w.fetcher.requests.lock().expect("lock").clear();
    w.svc.skipped(&w.name).expect("skipped");
    w.svc
        .unskip(&w.name, &["listPets".to_string()])
        .expect("unskip");
    assert!(w.fetcher.requests.lock().expect("lock").is_empty());
    assert_eq!(w.secret_reads.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[tokio::test]
async fn unskip_removes_only_the_named_keys() {
    let w = world().await;
    w.check_with(v2()).await;
    skip(&w, &["listPets", "getInventory"]).await;
    assert_eq!(local_skips(&w).len(), 2);
    let removed = w
        .svc
        .unskip(&w.name, &["listPets".to_string()])
        .expect("unskip");
    assert_eq!(removed, 1);
    assert_eq!(local_skips(&w), vec!["getInventory".to_string()]);
    let none = w
        .svc
        .unskip(&w.name, &["nope".to_string()])
        .expect("unskip");
    assert_eq!(none, 0);
}

#[tokio::test]
async fn unskip_refreshes_the_pending_count() {
    let w = world().await;
    w.check_with(v2()).await;
    skip(&w, &["listPets"]).await;
    assert_eq!(w.svc.local_state(&w.name).expect("local").pending_count, 2);
    w.svc
        .unskip(&w.name, &["listPets".to_string()])
        .expect("unskip");
    assert_eq!(w.svc.local_state(&w.name).expect("local").pending_count, 3);
}

#[tokio::test]
async fn an_unskipped_item_returns_to_the_list() {
    let w = world().await;
    w.check_with(v2()).await;
    skip(&w, &["listPets"]).await;
    assert!(w.plan().items.iter().all(|i| i.key() != "listPets"));
    w.svc
        .unskip(&w.name, &["listPets".to_string()])
        .expect("unskip");
    assert!(w.plan().items.iter().any(|i| i.key() == "listPets"));
    assert!(w.svc.skipped(&w.name).expect("skipped").is_empty());
}

#[tokio::test]
async fn unskip_refuses_while_an_apply_runs() {
    let blocked = Arc::new(AtomicBool::new(true));
    let factory = Wrapper {
        inner: rocket_infra::FsFileTxFactory::with_crash(2),
        fail_stage_at: None,
        recover_blocked: Arc::clone(&blocked),
    };
    let w = world_with(Arc::new(factory)).await;
    w.check_with(v2()).await;
    let locals = w.svc.local_repo().expect("locals");
    let mut local = locals.load(&w.root).expect("load").unwrap_or_default();
    local.skips.insert("listPets".into(), "fp".into());
    locals.save(&w.root, &local).expect("save");
    let plan = w.plan();
    w.svc
        .apply(&w.name, &w.approve(&plan, &[]))
        .await
        .expect_err("recovery is blocked");
    let err = w
        .svc
        .unskip(&w.name, &["listPets".to_string()])
        .expect_err("busy");
    assert!(matches!(err, DomainError::Conflict(_)), "{err:?}");
    assert!(local_skips(&w).contains(&"listPets".to_string()));
}

#[tokio::test]
async fn a_skip_whose_item_changed_again_is_not_listed() {
    let w = world().await;
    w.check_with(v2()).await;
    skip(&w, &["listPets"]).await;
    // The spec changes that endpoint again, so it is back in the plan and no longer hidden.
    w.check_with(v3()).await;
    assert!(w.plan().items.iter().any(|i| i.key() == "listPets"));
    let rows = w.svc.skipped(&w.name).expect("skipped");
    assert!(rows.iter().all(|r| r.key != "listPets"), "{rows:?}");
}
