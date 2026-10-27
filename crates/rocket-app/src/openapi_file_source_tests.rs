//! Tests for a spec that lives in a file on this computer. They reuse the apply world, which
//! reads files through a counting wrapper around the real file reader.

use super::connect_tests::snapshot;
use super::*;
use crate::openapi_sync_service::sha256_hex;
use rocket_collection::openapi_source::SpecLocation;
use std::sync::atomic::Ordering;

const SPEC_FILE: &str = "spec.yaml";

fn spec_path(w: &World) -> PathBuf {
    w.root.join(SPEC_FILE)
}

fn write_spec(w: &World, text: &str) {
    std::fs::write(spec_path(w), text).expect("write spec");
}

/// Turns the imported collection into a file source whose spec is `text`, and approves it.
fn use_file_source(w: &World, text: &str) {
    write_spec(w, text);
    let mut source = w.source();
    source.location = SpecLocation::File {
        path: SPEC_FILE.to_string(),
    };
    source.spec_sha256 = sha256_hex(text.as_bytes());
    source.etag = None;
    source.last_modified = None;
    rocket_infra::FsOpenApiSourceRepo
        .save(&w.root, &source)
        .expect("save source");
    w.svc.grant_current(&w.root).expect("consent");
}

fn failed_code(outcome: &CheckOutcome) -> &str {
    match outcome {
        CheckOutcome::Failed { code, .. } => code,
        other => panic!("expected a failure, got {other:?}"),
    }
}

#[tokio::test]
async fn a_file_source_check_finds_a_changed_spec() {
    let w = world().await;
    use_file_source(&w, &v1());
    write_spec(&w, &v2());
    let outcome = w.svc.check(&w.root).await.expect("check");
    assert!(
        matches!(outcome, CheckOutcome::Changed { .. }),
        "{outcome:?}"
    );
    assert!(w.source().pending_sha256.is_some());
    assert!(!w.plan().items.is_empty());
}

#[tokio::test]
async fn same_bytes_are_not_an_update() {
    let w = world().await;
    use_file_source(&w, &v1());
    // The file is written again with the same bytes, so its time changes and its content not.
    write_spec(&w, &v1());
    let outcome = w.svc.check(&w.root).await.expect("check");
    assert!(matches!(outcome, CheckOutcome::Unchanged), "{outcome:?}");
    assert!(w.source().pending_sha256.is_none());
}

#[tokio::test]
async fn a_missing_file_changes_nothing() {
    let w = world().await;
    use_file_source(&w, &v1());
    write_spec(&w, &v2());
    w.svc.check(&w.root).await.expect("first check");
    let before = w.source();
    std::fs::remove_file(spec_path(&w)).expect("remove");
    let outcome = w.svc.check(&w.root).await.expect("check");
    assert_eq!(failed_code(&outcome), "file_missing");
    let after = w.source();
    assert_eq!(after.pending_sha256, before.pending_sha256);
    assert_eq!(after.spec_sha256, before.spec_sha256);
    let status = w.svc.status(&w.root, false).expect("status");
    assert_eq!(
        status.last_message.as_deref(),
        Some("The spec file was not found on this computer.")
    );
}

#[tokio::test]
async fn no_read_without_consent() {
    let w = world().await;
    use_file_source(&w, &v1());
    w.svc.revoke_consent(&w.root).expect("revoke");
    let outcome = w.svc.check(&w.root).await.expect("check");
    assert_eq!(failed_code(&outcome), "consent_required");
    assert_eq!(w.reader_reads.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_file_check_sends_no_validators_and_has_no_backoff() {
    let w = world().await;
    use_file_source(&w, &v1());
    w.fetcher.requests.lock().expect("lock").clear();
    // A missing file fails, and a failure of a link would back off for 15 minutes.
    std::fs::remove_file(spec_path(&w)).expect("remove");
    let locals = w.svc.local_repo().expect("locals");
    let mut local = locals.load(&w.root).expect("load").unwrap_or_default();
    local.sync_interval = rocket_collection::openapi_source::SyncInterval::Min5;
    locals.save(&w.root, &local).expect("save");
    let outcome = w.svc.check(&w.root).await.expect("check");
    assert_eq!(failed_code(&outcome), "file_missing");
    assert!(w.fetcher.requests.lock().expect("lock").is_empty());
    let later = chrono::Utc::now() + chrono::Duration::seconds(360);
    assert!(
        w.svc
            .due_targets(later)
            .iter()
            .any(|t| t.collection == w.name),
        "a file source is checked again on its normal interval"
    );
}

#[tokio::test]
async fn a_token_is_refused_for_a_file_link() {
    let w = world().await;
    write_spec(&w, &v2());
    let err = w
        .svc
        .change_link_preview(&w.name, SPEC_FILE, Some("tok"))
        .await
        .expect_err("a token is for GitHub links only");
    assert!(err.to_string().contains("GitHub"), "{err}");
}

#[tokio::test]
async fn connect_preview_reads_a_file_link() {
    let w = world().await;
    w.repo.create("Hand").expect("create");
    let spec = w.ws_path.join("collections/Hand/spec.yaml");
    std::fs::write(&spec, v1()).expect("write");
    w.fetcher.requests.lock().expect("lock").clear();
    let preview = w
        .svc
        .preview("Hand", "spec.yaml", None)
        .await
        .expect("preview");
    assert_eq!(preview.spec_sha256, sha256_hex(v1().as_bytes()));
    assert!(w.fetcher.requests.lock().expect("lock").is_empty());
}

#[tokio::test]
async fn change_link_to_a_file_reads_no_token() {
    let w = world().await;
    write_spec(&w, &v2());
    let preview = w
        .svc
        .change_link_preview(&w.name, SPEC_FILE, None)
        .await
        .expect("preview");
    w.svc
        .change_link_confirm(&w.name, SPEC_FILE, None, &preview.spec_sha256)
        .await
        .expect("confirm");
    assert_eq!(
        w.source().location,
        SpecLocation::File {
            path: SPEC_FILE.to_string()
        }
    );
    assert_eq!(w.secret_reads.load(Ordering::SeqCst), 0);
    w.svc.check(&w.root).await.expect("check");
    assert_eq!(w.secret_reads.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn disconnect_never_writes_the_spec_file() {
    let w = world().await;
    use_file_source(&w, &v1());
    let before = std::fs::read(spec_path(&w)).expect("read");
    w.svc.disconnect(&w.root, true).expect("disconnect");
    assert_eq!(std::fs::read(spec_path(&w)).expect("read"), before);
    // The state files go, and the folder keeps everything else.
    assert!(!w.root.join(".rocket/openapi-source.yml").exists());
    let _ = snapshot(&w.root);
}
