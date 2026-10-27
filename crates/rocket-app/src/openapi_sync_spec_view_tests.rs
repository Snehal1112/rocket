//! Tests for the read-only spec view. They reuse the world of the apply tests.

use super::*;
use crate::openapi_sync_view::SpecViewState;

fn keys(view: &crate::openapi_sync_view::SpecView) -> Vec<(&str, bool)> {
    view.groups
        .iter()
        .flat_map(|g| g.operations.iter())
        .map(|o| (o.key.as_str(), o.is_new))
        .collect()
}

#[tokio::test]
async fn uses_the_applied_copy_when_nothing_waits() {
    let w = world().await;
    let view = w.svc.spec_view(&w.name).expect("view");
    assert_eq!(view.state, SpecViewState::InSync);
    assert_eq!(view.title, COLLECTION);
    assert_eq!(view.version.as_deref(), Some("1"));
    assert!(keys(&view).iter().all(|(_, is_new)| !is_new));
    assert_eq!(keys(&view).len(), 3);
}

#[tokio::test]
async fn uses_the_pending_copy_when_present() {
    let w = world().await;
    w.check_with(v2()).await;
    let view = w.svc.spec_view(&w.name).expect("view");
    assert_eq!(view.state, SpecViewState::UpdatesWaiting);
    let rows = keys(&view);
    // getInventory is gone in v2 and createPet is new.
    assert!(rows.contains(&("createPet", true)), "{rows:?}");
    assert!(rows.contains(&("listPets", false)), "{rows:?}");
    assert!(!rows.iter().any(|(k, _)| *k == "getInventory"), "{rows:?}");
    let list = view
        .groups
        .iter()
        .flat_map(|g| g.operations.iter())
        .find(|o| o.key == "listPets")
        .expect("listPets");
    assert_eq!(list.parameters.len(), 1);
}

#[tokio::test]
async fn groups_are_by_tag() {
    let w = world().await;
    let view = w.svc.spec_view(&w.name).expect("view");
    let tags: Vec<&str> = view.groups.iter().map(|g| g.tag.as_str()).collect();
    assert_eq!(tags, vec!["pets", "store"]);
}

#[tokio::test]
async fn reports_no_copy_for_an_old_import() {
    let w = world().await;
    std::fs::remove_file(w.root.join(".rocket/openapi-applied.spec")).expect("remove");
    let view = w.svc.spec_view(&w.name).expect("view");
    assert_eq!(view.state, SpecViewState::NoCopy);
    assert!(view.groups.is_empty());
}

#[tokio::test]
async fn ignores_an_applied_copy_that_does_not_match_the_source() {
    let w = world().await;
    std::fs::write(w.root.join(".rocket/openapi-applied.spec"), v2()).expect("write");
    let view = w.svc.spec_view(&w.name).expect("view");
    assert_eq!(view.state, SpecViewState::NoCopy);
    assert!(view.groups.is_empty());
}

#[tokio::test]
async fn never_touches_the_network() {
    let w = world().await;
    w.fetcher.requests.lock().expect("lock").clear();
    w.svc.spec_view(&w.name).expect("view");
    assert!(w.fetcher.requests.lock().expect("lock").is_empty());
    assert_eq!(w.secret_reads.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[tokio::test]
async fn an_unparsable_copy_gives_a_fixed_sentence() {
    let w = world().await;
    let junk = "C:\\secret\\path <html>";
    std::fs::write(w.root.join(".rocket/openapi-applied.spec"), junk).expect("write");
    // The source says this is the applied spec, so the copy is trusted and fails to parse.
    let mut source = w.source();
    source.spec_sha256 = crate::openapi_sync_service::sha256_hex(junk.as_bytes());
    rocket_infra::FsOpenApiSourceRepo
        .save(&w.root, &source)
        .expect("save source");
    let err = w.svc.spec_view(&w.name).expect_err("must fail");
    assert!(
        err.to_string().contains("The saved spec cannot be shown."),
        "{err}"
    );
    assert!(!err.to_string().contains("secret"), "{err}");
}

#[tokio::test]
async fn the_overview_version_falls_back_to_the_applied_copy() {
    let w = world().await;
    let overview = w.svc.overview(&w.name).expect("overview");
    assert_eq!(overview.spec_version.as_deref(), Some("1"));
}

#[tokio::test]
async fn a_leftover_pending_file_without_a_hash_is_not_shown_as_waiting() {
    let w = world().await;
    // A failed clear leaves the file behind, but the source holds no pending hash.
    rocket_infra::FsOpenApiSourceRepo
        .save_pending(
            &w.root,
            v2().replace(r#""version":"1""#, r#""version":"9""#)
                .as_bytes(),
        )
        .expect("stray pending file");
    assert!(w.source().pending_sha256.is_none());
    let view = w.svc.spec_view(&w.name).expect("view");
    assert_eq!(view.state, SpecViewState::InSync);
    assert_eq!(view.version.as_deref(), Some("1"));
    assert!(keys(&view).iter().all(|(_, is_new)| !is_new));
    let overview = w.svc.overview(&w.name).expect("overview");
    assert_eq!(overview.spec_version.as_deref(), Some("1"));
}

#[tokio::test]
async fn a_pending_file_that_does_not_match_its_hash_is_not_shown() {
    let w = world().await;
    w.check_with(v2()).await;
    std::fs::write(w.root.join(".rocket/openapi-pending.spec"), "{}").expect("tamper");
    let view = w.svc.spec_view(&w.name).expect("view");
    assert_eq!(view.state, SpecViewState::InSync);
}
