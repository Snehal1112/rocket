//! Tests for moving a connected collection to another spec link. They reuse the apply world.

use super::connect_tests::snapshot;
use super::*;
use crate::openapi_change_link::ChangeLinkPreview;
use rocket_collection::openapi_backup::{OpenApiBackupRepository, OpenApiBackups};
use rocket_collection::openapi_source::SpecLocation;
use rocket_collection::ConsentState;
use std::collections::HashMap;

const OTHER: &str = "https://example.com/specs/other.json";

/// Serves `body` at the second link, with its own etag.
fn serve_other(w: &World, body: String) {
    w.fetcher
        .files
        .lock()
        .expect("lock")
        .insert(OTHER.to_string(), (body, "other-etag".to_string()));
}

fn source_of(w: &World) -> OpenApiSource {
    rocket_infra::FsOpenApiSourceRepo
        .load(&w.root)
        .expect("load")
        .expect("source")
}

async fn preview_other(w: &World) -> ChangeLinkPreview {
    w.svc
        .change_link_preview(&w.name, OTHER, None)
        .await
        .expect("preview")
}

async fn move_to_other(w: &World) -> DomainResult<crate::openapi_change_link::ChangeLinkOutcome> {
    let preview = preview_other(w).await;
    w.svc
        .change_link_confirm(&w.name, OTHER, None, &preview.spec_sha256)
        .await
}

#[derive(Default)]
struct MapSecrets(std::sync::Mutex<HashMap<(String, String), String>>);

impl SecretStore for MapSecrets {
    fn get(&self, id: &str, key: &str) -> DomainResult<Option<String>> {
        Ok(self
            .0
            .lock()
            .expect("lock")
            .get(&(id.to_string(), key.to_string()))
            .cloned())
    }
    fn set(&self, id: &str, key: &str, value: &str) -> DomainResult<()> {
        self.0
            .lock()
            .expect("lock")
            .insert((id.to_string(), key.to_string()), value.to_string());
        Ok(())
    }
    fn delete(&self, id: &str, key: &str) -> DomainResult<()> {
        self.0
            .lock()
            .expect("lock")
            .remove(&(id.to_string(), key.to_string()));
        Ok(())
    }
}

/// A service on the world's files with a secret store that remembers, and a Github source.
fn github_world(w: &World, secrets: Arc<MapSecrets>) -> OpenApiSyncService {
    let mut source = source_of(w);
    source.location = SpecLocation::Github {
        owner: "acme".into(),
        repo: "api".into(),
        git_ref: "main".into(),
        path: "openapi.json".into(),
    };
    source.source_id = "sid-1".into();
    rocket_infra::FsOpenApiSourceRepo
        .save(&w.root, &source)
        .expect("save");
    secrets.set("sid-1", "token", "t0").expect("secret");
    let svc = OpenApiSyncService::new(
        Arc::new(SharedFetcher(Arc::clone(&w.fetcher))),
        Box::new(rocket_infra::FsOpenApiSourceRepo),
        secrets,
    )
    .with_consent(Arc::new(rocket_collection::InMemoryConsentStore::new()))
    .with_backups(Arc::new(rocket_infra::FsOpenApiBackupRepo))
    .with_apply(deps(
        &w.ws_path,
        Arc::new(rocket_infra::FsFileTxFactory::new()),
    ));
    svc.grant_current(&w.root).expect("consent");
    svc
}

async fn confirm_with(
    svc: &OpenApiSyncService,
    w: &World,
    link: &str,
) -> DomainResult<crate::openapi_change_link::ChangeLinkOutcome> {
    let preview = svc
        .change_link_preview(&w.name, link, None)
        .await
        .expect("preview");
    svc.change_link_confirm(&w.name, link, None, &preview.spec_sha256)
        .await
}

#[tokio::test]
async fn preview_counts_matched_added_and_removed() {
    let w = world().await;
    serve_other(&w, v2());
    let p = preview_other(&w).await;
    assert_eq!((p.matched, p.added, p.removed), (2, 1, 1));
    assert_eq!(p.title, COLLECTION);
    assert!(!p.same_spec);
}

#[tokio::test]
async fn preview_reports_the_same_spec() {
    let w = world().await;
    serve_other(&w, v1());
    assert!(preview_other(&w).await.same_spec);
}

#[tokio::test]
async fn preview_refuses_the_current_link() {
    let w = world().await;
    let err = w
        .svc
        .change_link_preview(&w.name, LINK, None)
        .await
        .expect_err("same link");
    assert!(matches!(&err, DomainError::Conflict(m) if m == "This is already the spec link."));
}

#[tokio::test]
async fn preview_refuses_an_unconnected_collection() {
    let w = world().await;
    w.repo.create("Hand").expect("create");
    let err = w
        .svc
        .change_link_preview("Hand", OTHER, None)
        .await
        .expect_err("not connected");
    assert!(
        matches!(&err, DomainError::Conflict(m) if m == "This collection is not connected to an OpenAPI spec.")
    );
}

#[tokio::test]
async fn preview_refuses_an_empty_link() {
    let w = world().await;
    let err = w
        .svc
        .change_link_preview(&w.name, "  ", None)
        .await
        .expect_err("empty");
    assert!(matches!(err, DomainError::InvalidInput(_)));
}

#[tokio::test]
async fn preview_writes_nothing() {
    let w = world().await;
    serve_other(&w, v2());
    let before = snapshot(&w.root);
    preview_other(&w).await;
    assert_eq!(snapshot(&w.root), before);
}

#[tokio::test]
async fn confirm_keeps_baseline_skips_backups_and_interval() {
    let w = world().await;
    w.svc
        .set_interval(&w.root, SyncInterval::Daily)
        .expect("interval");
    rocket_infra::FsOpenApiBackupRepo
        .save(&w.root, &OpenApiBackups::default())
        .expect("backups");
    let baseline = std::fs::read(w.root.join(".rocket/openapi-baseline.json")).expect("baseline");
    let backups =
        std::fs::read(w.root.join(".rocket/openapi-reset-backups.json")).expect("backups");
    let local_before = w.svc.local_state(&w.name).expect("local");
    let applied = std::fs::read(w.root.join(".rocket/openapi-applied.spec")).expect("applied");
    let old = source_of(&w);
    serve_other(&w, v2());
    move_to_other(&w).await.expect("confirm");
    let now = source_of(&w);
    assert_eq!(now.operations, old.operations);
    assert_eq!(now.spec_sha256, old.spec_sha256);
    assert_eq!(now.source_id, old.source_id);
    assert_eq!(
        std::fs::read(w.root.join(".rocket/openapi-baseline.json")).expect("baseline"),
        baseline
    );
    assert_eq!(
        std::fs::read(w.root.join(".rocket/openapi-reset-backups.json")).expect("backups"),
        backups
    );
    assert_eq!(
        std::fs::read(w.root.join(".rocket/openapi-applied.spec")).expect("applied"),
        applied
    );
    let local_after = w.svc.local_state(&w.name).expect("local");
    assert_eq!(local_after.skips, local_before.skips);
    assert_eq!(local_after.sync_interval, local_before.sync_interval);
}

#[tokio::test]
async fn confirm_clears_validators() {
    let w = world().await;
    let mut source = source_of(&w);
    source.etag = Some("old-etag".into());
    source.last_modified = Some("old-date".into());
    rocket_infra::FsOpenApiSourceRepo
        .save(&w.root, &source)
        .expect("save");
    serve_other(&w, v2());
    move_to_other(&w).await.expect("confirm");
    let now = source_of(&w);
    assert_eq!(now.etag.as_deref(), Some("other-etag"));
    assert_eq!(now.last_modified, None);
    assert_eq!(now.location.display_url(), OTHER);
}

#[tokio::test]
async fn confirm_stages_a_differing_spec_as_pending() {
    let w = world().await;
    serve_other(&w, v2());
    let outcome = move_to_other(&w).await.expect("confirm");
    assert!(outcome.pending);
    let now = source_of(&w);
    assert_eq!(now.pending_sha256, Some(sha256_hex(v2().as_bytes())));
    let pending = rocket_infra::FsOpenApiSourceRepo
        .load_pending(&w.root)
        .expect("load")
        .expect("pending");
    assert_eq!(pending, v2().into_bytes());
    assert_eq!(now.spec_sha256, sha256_hex(v1().as_bytes()));
}

#[tokio::test]
async fn confirm_with_an_identical_spec_leaves_nothing_pending() {
    let w = world().await;
    w.check_with(v2()).await;
    assert!(source_of(&w).pending_sha256.is_some());
    serve_other(&w, v1());
    let outcome = move_to_other(&w).await.expect("confirm");
    assert!(!outcome.pending);
    assert_eq!(source_of(&w).pending_sha256, None);
    assert!(rocket_infra::FsOpenApiSourceRepo
        .load_pending(&w.root)
        .expect("load")
        .is_none());
}

#[tokio::test]
async fn confirm_grants_consent_for_the_typed_link() {
    let w = world().await;
    serve_other(&w, v2());
    move_to_other(&w).await.expect("confirm");
    assert_eq!(
        w.svc.consent_state(&w.root).expect("state"),
        ConsentState::Granted
    );
}

#[tokio::test]
async fn the_same_repo_keeps_the_token() {
    let w = world().await;
    let secrets = Arc::new(MapSecrets::default());
    let svc = github_world(&w, Arc::clone(&secrets));
    let outcome = confirm_with(&svc, &w, "https://github.com/acme/api/blob/main/other.json")
        .await
        .expect("confirm");
    assert!(outcome.token_kept);
    assert_eq!(
        secrets.get("sid-1", "token").expect("get").as_deref(),
        Some("t0")
    );
}

#[tokio::test]
async fn a_different_repo_gets_a_fresh_source_id_and_no_token() {
    let w = world().await;
    let secrets = Arc::new(MapSecrets::default());
    let svc = github_world(&w, Arc::clone(&secrets));
    let outcome = confirm_with(&svc, &w, "https://github.com/acme/other/blob/main/x.json")
        .await
        .expect("confirm");
    assert!(!outcome.token_kept);
    let now = source_of(&w);
    assert_ne!(now.source_id, "sid-1");
    assert!(!now.source_id.is_empty());
    // Nothing is stored under the new id, so the old repo's token can never be sent to this one.
    assert_eq!(secrets.get(&now.source_id, "token").expect("get"), None);
    // The old id keeps its token, so a clone that still points at the old repo keeps working.
    assert_eq!(
        secrets.get("sid-1", "token").expect("get").as_deref(),
        Some("t0")
    );
    assert_eq!(
        svc.consent_state(&w.root).expect("state"),
        ConsentState::Granted
    );
}

#[tokio::test]
async fn a_https_link_gets_a_fresh_source_id_too() {
    let w = world().await;
    let secrets = Arc::new(MapSecrets::default());
    let svc = github_world(&w, Arc::clone(&secrets));
    confirm_with(&svc, &w, OTHER).await.expect("confirm");
    assert_ne!(source_of(&w).source_id, "sid-1");
}

#[tokio::test]
async fn a_typed_token_is_stored_under_the_new_source_id() {
    let w = world().await;
    let secrets = Arc::new(MapSecrets::default());
    let svc = github_world(&w, Arc::clone(&secrets));
    let link = "https://github.com/acme/other/blob/main/x.json";
    let preview = svc
        .change_link_preview(&w.name, link, Some("t1"))
        .await
        .expect("preview");
    let outcome = svc
        .change_link_confirm(&w.name, link, Some("t1"), &preview.spec_sha256)
        .await
        .expect("confirm");
    assert!(outcome.token_kept);
    let id = source_of(&w).source_id;
    assert_eq!(
        secrets.get(&id, "token").expect("get").as_deref(),
        Some("t1")
    );
    assert_eq!(
        secrets.get("sid-1", "token").expect("get").as_deref(),
        Some("t0")
    );
}

#[tokio::test]
async fn the_same_repo_keeps_its_source_id() {
    let w = world().await;
    let secrets = Arc::new(MapSecrets::default());
    let svc = github_world(&w, Arc::clone(&secrets));
    confirm_with(&svc, &w, "https://github.com/acme/api/blob/main/other.json")
        .await
        .expect("confirm");
    assert_eq!(source_of(&w).source_id, "sid-1");
}

#[tokio::test]
async fn a_changed_spec_gives_stale_spec() {
    let w = world().await;
    serve_other(&w, v2());
    let preview = preview_other(&w).await;
    serve_other(&w, v3());
    let err = w
        .svc
        .change_link_confirm(&w.name, OTHER, None, &preview.spec_sha256)
        .await
        .expect_err("stale");
    assert!(matches!(err, DomainError::Conflict(_)));
    assert_eq!(source_of(&w).location.display_url(), LINK);
}

#[tokio::test]
async fn confirm_refuses_while_an_apply_runs() {
    let blocked = Arc::new(AtomicBool::new(true));
    let factory = Wrapper {
        inner: rocket_infra::FsFileTxFactory::with_crash(2),
        fail_stage_at: None,
        recover_blocked: Arc::clone(&blocked),
    };
    let w = world_with(Arc::new(factory)).await;
    w.check_with(v2()).await;
    let plan = w.plan();
    let selection = w.approve(&plan, &[]);
    w.svc
        .apply(&w.name, &selection)
        .await
        .expect_err("recovery is blocked");
    serve_other(&w, v3());
    let err = move_to_other(&w).await.expect_err("busy");
    assert!(matches!(err, DomainError::Conflict(_)), "{err:?}");
    assert_eq!(source_of(&w).location.display_url(), LINK);
}
