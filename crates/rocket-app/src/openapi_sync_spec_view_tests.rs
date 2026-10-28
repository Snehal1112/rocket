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
    // The source says a newer spec waits, so showing the old one as in sync would mislead.
    let err = w.svc.spec_view(&w.name).expect_err("must not show in sync");
    assert!(
        err.to_string()
            .contains("A newer spec is waiting but its copy is missing"),
        "{err}"
    );
}

#[tokio::test]
async fn a_missing_pending_file_gives_the_same_fixed_sentence() {
    let w = world().await;
    w.check_with(v2()).await;
    std::fs::remove_file(w.root.join(".rocket/openapi-pending.spec")).expect("remove");
    let err = w.svc.spec_view(&w.name).expect_err("must not show in sync");
    assert!(
        err.to_string()
            .contains("A newer spec is waiting but its copy is missing"),
        "{err}"
    );
}

#[tokio::test]
async fn a_damaged_local_state_does_not_break_the_viewer() {
    let w = world().await;
    std::fs::write(w.root.join(".rocket/openapi-local.json"), "{ not json").expect("damage");
    let view = w.svc.spec_view(&w.name).expect("view");
    assert_eq!(view.state, SpecViewState::InSync);
}

const RICH_SPEC: &str = r#"
openapi: 3.0.3
info: { title: Pets, version: '3' }
servers:
  - url: https://api.example.com
  - url: https://staging.example.com
security:
  - bearer: []
paths:
  /pets:
    post:
      operationId: addPet
      requestBody:
        required: true
        content:
          application/json:
            schema: { type: object, required: [name], properties: { name: { type: string } } }
      responses:
        '201':
          description: created
          content:
            application/json:
              schema: { type: object, properties: { id: { type: integer } } }
components:
  securitySchemes:
    bearer: { type: http, scheme: bearer }
"#;

/// Stores `text` as the applied copy the source vouches for.
fn use_applied_copy(w: &World, text: &str) {
    std::fs::write(w.root.join(".rocket/openapi-applied.spec"), text).expect("write");
    let mut source = w.source();
    source.spec_sha256 = crate::openapi_sync_service::sha256_hex(text.as_bytes());
    rocket_infra::FsOpenApiSourceRepo
        .save(&w.root, &source)
        .expect("save source");
}

#[tokio::test]
async fn spec_view_carries_the_request_body_and_responses() {
    let w = world().await;
    use_applied_copy(&w, RICH_SPEC);
    let view = w.svc.spec_view(&w.name).expect("view");
    let op = &view.groups[0].operations[0];
    assert_eq!(op.request_body.len(), 1);
    assert_eq!(op.request_body[0].content_type, "application/json");
    assert!(op.request_body[0].required);
    assert_eq!(
        op.request_body[0]
            .schema
            .as_ref()
            .expect("schema")
            .properties[0]
            .0,
        "name"
    );
    let created = &op.responses[0];
    assert_eq!(
        created.bodies[0].schema.as_ref().expect("schema").kind,
        "object"
    );
}

#[tokio::test]
async fn spec_view_carries_servers_and_security() {
    let w = world().await;
    use_applied_copy(&w, RICH_SPEC);
    let view = w.svc.spec_view(&w.name).expect("view");
    assert_eq!(
        view.servers,
        ["https://api.example.com", "https://staging.example.com"]
    );
    assert_eq!(view.groups[0].operations[0].security, ["bearer"]);
}

#[tokio::test]
async fn an_old_pending_copy_without_bodies_still_loads() {
    let w = world().await;
    let plain = "openapi: 3.0.3\ninfo: { title: Old, version: '1' }\npaths:\n  /a:\n    get:\n      responses:\n        '200': { description: ok }\n";
    use_applied_copy(&w, plain);
    let view = w.svc.spec_view(&w.name).expect("view");
    let op = &view.groups[0].operations[0];
    assert!(op.request_body.is_empty());
    assert!(op.responses[0].bodies.is_empty());
    assert!(op.security.is_empty());
    assert!(view.servers.is_empty());
}
