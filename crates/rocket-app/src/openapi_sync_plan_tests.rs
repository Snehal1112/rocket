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
