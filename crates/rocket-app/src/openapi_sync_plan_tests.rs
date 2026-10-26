//! Tests for the read-only plan of a pending spec. They reuse the world of the apply tests.

use super::*;

type Listing = BTreeMap<String, (u64, std::time::SystemTime)>;

/// Every file and folder under the collection root with its size and mtime.
fn listing(root: &Path) -> Listing {
    fn walk(root: &Path, dir: &Path, out: &mut Listing) {
        for e in std::fs::read_dir(dir).expect("dir") {
            let p = e.expect("entry").path();
            let meta = std::fs::metadata(&p).expect("meta");
            let rel = p
                .strip_prefix(root)
                .expect("rel")
                .to_string_lossy()
                .to_string();
            out.insert(rel, (meta.len(), meta.modified().expect("mtime")));
            if p.is_dir() {
                walk(root, &p, out);
            }
        }
    }
    let mut out = Listing::new();
    walk(root, root, &mut out);
    out
}

#[tokio::test]
async fn plan_is_empty_when_no_pending_copy() {
    let w = world().await;
    let view = w.svc.plan(&w.name).expect("plan");
    assert!(!view.pending);
    assert!(view.plan.items.is_empty());
    assert!(view.item_digests.is_empty());
    assert!(view.variable_digests.is_empty());
    assert!(view.unchanged.is_empty());
}

#[tokio::test]
async fn plan_lists_new_changed_and_removed_items_after_a_changed_check() {
    let w = world().await;
    w.check_with(v2()).await;
    let view = w.svc.plan(&w.name).expect("plan");
    assert!(view.pending);
    let keys: BTreeSet<&str> = view.plan.items.iter().map(SyncItem::key).collect();
    assert_eq!(
        keys,
        BTreeSet::from(["listPets", "createPet", "getInventory"])
    );
    assert_eq!(view.item_digests.len(), view.plan.items.len());
    for (item, digest) in view.plan.items.iter().zip(&view.item_digests) {
        assert_eq!(&item_digest(item), digest);
    }
    // The view holds the same plan the test helper builds by hand. New requests get fresh uids
    // on every build, so the digests are compared and not the plans.
    let by_hand: Vec<String> = w.plan().items.iter().map(item_digest).collect();
    assert_eq!(view.item_digests, by_hand);
}

#[tokio::test]
async fn plan_unchanged_lists_operations_not_in_the_plan() {
    let w = world().await;
    w.check_with(v2()).await;
    let view = w.svc.plan(&w.name).expect("plan");
    let keys: Vec<&str> = view.unchanged.iter().map(|u| u.key.as_str()).collect();
    assert_eq!(keys, vec!["getPet"]);
    assert!(view.unchanged[0].request_path.is_some());
    assert!(!view.unchanged[0].name.is_empty());

    // A skipped operation is hidden from the plan and is not reported as unchanged.
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
    let after = w.svc.plan(&w.name).expect("plan");
    assert_eq!(after.plan.skipped_hidden, 1);
    assert!(after.plan.items.iter().all(|i| i.key() != "listPets"));
    let keys: Vec<&str> = after.unchanged.iter().map(|u| u.key.as_str()).collect();
    assert_eq!(keys, vec!["getPet"]);
}

#[tokio::test]
async fn plan_fails_with_conflict_when_the_pending_copy_hash_differs() {
    let w = world().await;
    w.check_with(v2()).await;
    std::fs::write(w.root.join(".rocket/openapi-pending.spec"), b"{}").expect("tamper");
    let err = w.svc.plan(&w.name).expect_err("conflict");
    assert!(matches!(err, DomainError::Conflict(_)), "got {err:?}");
}

#[tokio::test]
async fn plan_does_not_write_any_file() {
    let w = world().await;
    w.check_with(v2()).await;
    let before = listing(&w.root);
    let bytes = w.snapshot();
    w.svc.plan(&w.name).expect("plan");
    w.svc.plan(&w.name).expect("plan again");
    assert_eq!(listing(&w.root), before);
    assert_eq!(w.snapshot(), bytes);
}

#[tokio::test]
async fn status_of_a_collection_without_a_source_is_not_found() {
    let w = world().await;
    std::fs::remove_file(w.root.join(".rocket/openapi-source.yml")).expect("remove");
    let err = w.svc.status_of(&w.name).expect_err("no source");
    assert!(matches!(err, DomainError::NotFound(_)), "got {err:?}");
}

#[tokio::test]
async fn status_of_an_unreadable_source_file_is_an_error() {
    let w = world().await;
    std::fs::write(
        w.root.join(".rocket/openapi-source.yml"),
        b"{{{ not yaml: [",
    )
    .expect("write");
    let err = w.svc.status_of(&w.name).expect_err("unreadable");
    assert!(!matches!(err, DomainError::NotFound(_)), "got {err:?}");
}

/// The first spec with `getInventory` renamed to `listInventory` on the same path.
fn v_renamed() -> String {
    spec(json!({
        "/pets": {"get": op("listPets", "pets", json!({}))},
        "/pets/{id}": {"get": op("getPet", "pets", json!({"parameters": [
            {"name": "id", "in": "path", "required": true, "schema": {"type": "string"}}]}))},
        "/inventory": {"get": op("listInventory", "store", json!({}))},
    }))
}

fn rename_link() -> Vec<(String, String)> {
    vec![("getInventory".to_string(), "listInventory".to_string())]
}

#[tokio::test]
async fn a_plan_with_links_has_the_digests_that_apply_checks() {
    let w = world().await;
    w.check_with(v_renamed()).await;
    let plain = w.svc.plan(&w.name).expect("plan");
    assert!(plain
        .plan
        .items
        .iter()
        .any(|i| matches!(i, SyncItem::New { key, .. } if key == "listInventory")));
    let linked = w.svc.plan_linked(&w.name, &rename_link()).expect("plan");
    assert!(linked
        .plan
        .items
        .iter()
        .any(|i| matches!(i, SyncItem::Changed { key, .. } if key == "listInventory")));
    // Approve the plan's own digests with the same links: nothing is stale.
    let items = linked
        .plan
        .items
        .iter()
        .zip(&linked.item_digests)
        .map(|(i, digest)| ApprovedItem {
            key: i.key().to_string(),
            digest: digest.clone(),
            choices: match i {
                SyncItem::Changed { merge, .. } => merge
                    .changes
                    .iter()
                    .map(|c| (c.field.clone(), Choice::Spec))
                    .collect(),
                _ => BTreeMap::new(),
            },
        })
        .collect();
    let selection = Selection {
        items,
        rename_links: rename_link(),
        ..Selection::default()
    };
    let report = w.svc.apply(&w.name, &selection).await.expect("apply");
    assert!(report.stale.is_empty(), "{:?}", report.stale);
    assert!(report.applied.iter().any(|a| a.key == "listInventory"));
}

#[tokio::test]
async fn a_plan_with_a_refused_link_is_the_plan_without_it() {
    let w = world().await;
    w.check_with(v_renamed()).await;
    let bad = vec![("nope".to_string(), "listInventory".to_string())];
    let linked = w.svc.plan_linked(&w.name, &bad).expect("plan");
    let plain = w.svc.plan(&w.name).expect("plan");
    assert_eq!(linked.item_digests, plain.item_digests);
}

#[tokio::test]
async fn apply_with_only_skips_records_them() {
    let w = world().await;
    w.check_with(v2()).await;
    let plan = w.svc.plan(&w.name).expect("plan");
    let item = plan.plan.items.first().expect("item");
    let selection = Selection {
        skips: vec![SkipRequest {
            key: item.key().to_string(),
            fingerprint: item.fingerprint().to_string(),
        }],
        ..Selection::default()
    };
    let report = w.svc.apply(&w.name, &selection).await.expect("apply");
    assert_eq!(report.skipped, 1);
    assert!(w
        .svc
        .local_state(&w.name)
        .expect("local")
        .skips
        .contains_key(item.key()));
}
