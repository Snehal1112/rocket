//! Tests for the cached counts, the pending id and the `changes` read model. They reuse the world of the apply tests.

use super::*;
use rocket_import::{fingerprint, SkipRequest, SyncItem};

#[tokio::test]
async fn status_reports_the_pending_id() {
    let w = world().await;
    assert_eq!(
        w.svc.status(&w.root, false).expect("status").pending_id,
        None
    );
    w.check_with(v2()).await;
    let hash = w.source().pending_sha256.expect("hash");
    let id = w.svc.status(&w.root, false).expect("status").pending_id;
    assert_eq!(id.as_deref(), Some(&hash[..12]));
}

#[tokio::test]
async fn check_stores_the_waiting_count() {
    let w = world().await;
    w.check_with(v2()).await;
    assert_eq!(w.svc.local_state(&w.name).expect("local").pending_count, 3);
    w.check_with(v1()).await;
    assert_eq!(w.svc.local_state(&w.name).expect("local").pending_count, 0);
}

#[tokio::test]
async fn a_partial_apply_stores_the_remaining_count() {
    let w = world().await;
    w.check_with(v2()).await;
    let plan = w.plan();
    w.svc
        .apply(&w.name, &w.approve(&plan, &["listPets"]))
        .await
        .expect("apply");
    let local = w.svc.local_state(&w.name).expect("local");
    assert_eq!(local.pending_count, 2);
    assert!(local.last_synced_at.is_none());
}

#[tokio::test]
async fn a_finalized_apply_stores_the_sync_facts() {
    let w = world().await;
    w.check_with(v2()).await;
    let plan = w.plan();
    w.svc
        .apply(&w.name, &w.approve(&plan, &[]))
        .await
        .expect("apply");
    let local = w.svc.local_state(&w.name).expect("local");
    assert_eq!(local.pending_count, 0);
    assert!(local.last_synced_at.is_some());
    assert_eq!(local.last_synced_updated, 3);
}

#[tokio::test]
async fn skipped_items_are_not_counted_as_waiting() {
    let w = world().await;
    w.check_with(v2()).await;
    let view = w.svc.plan(&w.name).expect("plan");
    let fp = match view.plan.items.iter().find(|i| i.key() == "listPets") {
        Some(SyncItem::Changed { spec, .. }) => fingerprint(spec),
        other => panic!("unexpected item {other:?}"),
    };
    let selection = Selection {
        skips: vec![SkipRequest {
            key: "listPets".to_string(),
            fingerprint: fp,
        }],
        ..Selection::default()
    };
    w.svc.apply(&w.name, &selection).await.expect("skip");
    assert_eq!(w.svc.count_pending(&w.name).expect("count"), 2);
}

#[tokio::test]
async fn check_survives_a_failing_count() {
    let w = world().await;
    w.check_with(v2()).await;
    assert_eq!(w.svc.local_state(&w.name).expect("local").pending_count, 3);
    std::fs::write(w.root.join(".rocket/openapi-baseline.json"), "{").expect("corrupt");
    *w.fetcher.body.lock().expect("lock") = v3();
    let out = w.svc.check(&w.root).await.expect("check");
    assert!(matches!(out, CheckOutcome::Changed { .. }), "{out:?}");
    assert_eq!(w.svc.local_state(&w.name).expect("local").pending_count, 3);
}

#[tokio::test]
async fn a_partial_apply_counts_variable_changes_like_count_pending() {
    let w = world().await;
    w.check_with(v2().replace("https://api.example.com", "https://api2.example.com"))
        .await;
    let plan = w.plan();
    assert_eq!(plan.variable_changes.len(), 1);
    w.svc
        .apply(&w.name, &w.approve(&plan, &["listPets"]))
        .await
        .expect("apply");
    let count = w.svc.count_pending(&w.name).expect("count");
    assert_eq!(count, 3);
    assert_eq!(
        w.svc.local_state(&w.name).expect("local").pending_count,
        count
    );
}

fn path_of(w: &World, key: &str) -> String {
    w.source()
        .operations
        .iter()
        .find(|o| o.key == key)
        .expect("entry")
        .request_path
        .clone()
}

fn edit_url(w: &World, key: &str) {
    let path = path_of(w, key);
    let mut request = w.repo.get_request(&w.name, &path).expect("get");
    request.url = "https://edited.example.com/x".to_string();
    w.repo.save_request(&w.name, &path, &request).expect("save");
}

#[tokio::test]
async fn changes_is_empty_for_a_fresh_import() {
    let w = world().await;
    let view = w.svc.changes(&w.name).expect("changes");
    assert!(view.baseline_trusted);
    assert!(view.modified.is_empty());
    assert!(view.added.is_empty());
    assert!(view.deleted.is_empty());
}

#[tokio::test]
async fn changes_lists_a_modified_request_with_its_field_names() {
    let w = world().await;
    let path = path_of(&w, "getPet");
    let mut request = w.repo.get_request(&w.name, &path).expect("get");
    request.url = "https://edited.example.com/pets".to_string();
    request
        .headers
        .push(rocket_shared::types::Header::new("Accept", "text/plain"));
    w.repo.save_request(&w.name, &path, &request).expect("save");
    let view = w.svc.changes(&w.name).expect("changes");
    assert_eq!(view.modified.len(), 1);
    let row = &view.modified[0];
    assert_eq!(row.key.as_deref(), Some("getPet"));
    assert_eq!(row.request_path.as_deref(), Some(path.as_str()));
    assert!(row.fields.contains(&"url".to_string()), "{:?}", row.fields);
    assert!(
        row.fields.contains(&"header:Accept".to_string()),
        "{:?}",
        row.fields
    );
    assert!(view.added.is_empty() && view.deleted.is_empty());
}

#[tokio::test]
async fn changes_ignores_scripts_tests_and_docs() {
    let w = world().await;
    let path = path_of(&w, "getPet");
    let mut request = w.repo.get_request(&w.name, &path).expect("get");
    request.pre_request_script = Some("console.log(1);".to_string());
    request.docs = Some(rocket_shared::description::Description::text("docs"));
    w.repo.save_request(&w.name, &path, &request).expect("save");
    let view = w.svc.changes(&w.name).expect("changes");
    assert!(view.modified.is_empty(), "{:?}", view.modified);
}

#[tokio::test]
async fn changes_lists_an_added_request() {
    let w = world().await;
    let request = rocket_collection::Request::new(
        "Mine",
        rocket_shared::types::HttpMethod::Get,
        "https://x.example.com",
    );
    w.repo
        .save_request(&w.name, "mine", &request)
        .expect("save");
    let view = w.svc.changes(&w.name).expect("changes");
    assert_eq!(view.added.len(), 1);
    assert_eq!(view.added[0].key, None);
    assert_eq!(view.added[0].name, "Mine");
    assert_eq!(view.added[0].method, "GET");
    assert!(view.modified.is_empty() && view.deleted.is_empty());
}

#[tokio::test]
async fn changes_lists_a_deleted_request() {
    let w = world().await;
    let path = path_of(&w, "listPets");
    w.repo.delete_request(&w.name, &path).expect("delete");
    let view = w.svc.changes(&w.name).expect("changes");
    assert_eq!(view.deleted.len(), 1);
    assert_eq!(view.deleted[0].key.as_deref(), Some("listPets"));
    assert_eq!(view.deleted[0].request_path.as_deref(), Some(path.as_str()));
    assert!(view.deleted[0].fields.is_empty());
}

#[tokio::test]
async fn changes_without_a_trusted_baseline_uses_hashes() {
    let w = world().await;
    std::fs::remove_file(w.root.join(".rocket/openapi-baseline.json")).expect("remove");
    edit_url(&w, "getPet");
    let view = w.svc.changes(&w.name).expect("changes");
    assert!(!view.baseline_trusted);
    assert_eq!(view.modified.len(), 1);
    assert_eq!(view.modified[0].key.as_deref(), Some("getPet"));
    assert!(view.modified[0].fields.is_empty());
}

#[tokio::test]
async fn changes_does_not_write_any_file() {
    let w = world().await;
    edit_url(&w, "getPet");
    let before = w.snapshot();
    w.svc.changes(&w.name).expect("changes");
    assert_eq!(before, w.snapshot());
}

#[tokio::test]
async fn changes_fails_clearly_without_a_source() {
    let w = world().await;
    std::fs::remove_file(w.root.join(".rocket/openapi-source.yml")).expect("remove");
    let err = w.svc.changes(&w.name).expect_err("no source");
    assert!(matches!(err, DomainError::NotFound(_)), "{err:?}");
}

#[tokio::test]
async fn overview_counts_after_a_changed_check() {
    let w = world().await;
    w.check_with(v2()).await;
    let view = w.svc.overview(&w.name).expect("overview");
    assert_eq!(view.endpoints, 3);
    assert_eq!(view.updates_waiting, 3);
    assert_eq!(view.changed_by_you, Some(0));
    assert_eq!(view.in_sync, Some(1));
}

#[tokio::test]
async fn overview_counts_your_changes_apart_from_waiting_ones() {
    let w = world().await;
    edit_url(&w, "getPet");
    let view = w.svc.overview(&w.name).expect("overview");
    assert_eq!(view.changed_by_you, Some(1));
    assert_eq!(view.in_sync, Some(2));
    assert_eq!(view.updates_waiting, 0);
}

#[tokio::test]
async fn overview_marks_counts_unknown_without_a_trusted_baseline() {
    let w = world().await;
    std::fs::remove_file(w.root.join(".rocket/openapi-baseline.json")).expect("remove");
    let view = w.svc.overview(&w.name).expect("overview");
    assert_eq!(view.in_sync, None);
    assert_eq!(view.changed_by_you, None);
    assert_eq!(view.endpoints, 3);
}

#[tokio::test]
async fn overview_reads_the_spec_version_of_the_pending_copy() {
    let w = world().await;
    assert_eq!(
        w.svc.overview(&w.name).expect("overview").spec_version,
        None
    );
    w.check_with(v2().replace(r#""version":"1""#, r#""version":"2.0.0""#))
        .await;
    let view = w.svc.overview(&w.name).expect("overview");
    assert_eq!(view.spec_version.as_deref(), Some("2.0.0"));
}

#[tokio::test]
async fn overview_carries_the_last_sync_facts() {
    let w = world().await;
    w.check_with(v2()).await;
    let plan = w.plan();
    w.svc
        .apply(&w.name, &w.approve(&plan, &[]))
        .await
        .expect("apply");
    let view = w.svc.overview(&w.name).expect("overview");
    assert!(view.last_synced_at.is_some());
    assert_eq!(view.last_synced_updated, 3);
    assert_eq!(view.updates_waiting, 0);
}

#[tokio::test]
async fn overview_makes_no_request_and_reads_no_token() {
    let w = world().await;
    w.check_with(v2()).await;
    let requests = w.fetcher.requests.lock().expect("lock").len();
    let reads = w.secret_reads.load(std::sync::atomic::Ordering::SeqCst);
    w.svc.overview(&w.name).expect("overview");
    w.svc.changes(&w.name).expect("changes");
    w.svc.badges();
    assert_eq!(w.fetcher.requests.lock().expect("lock").len(), requests);
    assert_eq!(
        w.secret_reads.load(std::sync::atomic::Ordering::SeqCst),
        reads
    );
}

#[tokio::test]
async fn badges_lists_only_collections_with_waiting_updates() {
    let w = world().await;
    assert!(w.svc.badges().is_empty());
    w.check_with(v2()).await;
    let badges = w.svc.badges();
    assert_eq!(badges.len(), 1);
    assert_eq!(badges[0].collection, w.name);
    assert_eq!(badges[0].waiting, 3);
}

#[tokio::test]
async fn badges_ignores_an_unreadable_state_file() {
    let w = world().await;
    w.check_with(v2()).await;
    w.repo.create("Other").expect("create");
    let other_root = w.ws_path.join("collections").join("Other");
    std::fs::create_dir_all(other_root.join(".rocket")).expect("dir");
    std::fs::copy(
        w.root.join(".rocket/openapi-source.yml"),
        other_root.join(".rocket/openapi-source.yml"),
    )
    .expect("copy");
    std::fs::write(other_root.join(".rocket/openapi-local.json"), "{").expect("corrupt");
    let badges = w.svc.badges();
    assert_eq!(badges.len(), 1);
    assert_eq!(badges[0].collection, w.name);
}
