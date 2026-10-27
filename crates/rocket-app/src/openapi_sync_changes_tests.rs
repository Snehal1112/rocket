//! Tests for the cached counts and the pending id. They reuse the world of the apply tests.

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
