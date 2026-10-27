//! Tests for connecting a hand-made collection to a spec. They reuse the world of the apply tests.

use super::*;
use crate::openapi_connect::{request_digest, ConnectPreview};
use rocket_import::MatchRequest;
use rocket_shared::types::HttpMethod;

const HAND: &str = "Hand";

/// Creates a collection without a source next to the imported one.
fn hand_made(w: &World, requests: &[(&str, &str, &str)]) {
    w.repo.create(HAND).expect("create");
    for (name, method, url) in requests {
        let method: HttpMethod = method.parse().expect("method");
        let request = rocket_collection::Request::new(*name, method, *url);
        w.repo
            .save_request(HAND, name, &request)
            .expect("save request");
    }
}

fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).expect("read_dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                walk(&path, root, out);
            } else {
                let rel = path.strip_prefix(root).expect("prefix");
                out.insert(
                    rel.to_string_lossy().to_string(),
                    std::fs::read(&path).expect("read"),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

fn hand_root(w: &World) -> PathBuf {
    w.ws_path.join("collections").join(HAND)
}

fn matched_keys(preview: &ConnectPreview) -> Vec<String> {
    preview
        .matched
        .iter()
        .map(|p| p.operation.key.clone())
        .collect()
}

#[tokio::test]
async fn preview_matches_the_pets_collection() {
    let w = world().await;
    hand_made(
        &w,
        &[
            ("List pets", "GET", "{{baseUrl}}/pets"),
            ("Get pet", "GET", "{{baseUrl}}/pets/:id"),
            ("Mine", "GET", "{{baseUrl}}/mine"),
        ],
    );
    let preview = w.svc.preview(HAND, LINK, None).await.expect("preview");
    let mut keys = matched_keys(&preview);
    keys.sort();
    assert_eq!(keys, vec!["getPet".to_string(), "listPets".to_string()]);
    assert_eq!(preview.in_spec_only.len(), 1);
    assert_eq!(preview.in_spec_only[0].key, "getInventory");
    assert_eq!(preview.yours_only.len(), 1);
    assert_eq!(preview.yours_only[0].name, "Mine");
    assert_eq!(preview.spec_title, COLLECTION);
    assert_eq!(preview.spec_version.as_deref(), Some("1"));
    assert!(!preview.digest.is_empty());
}

#[tokio::test]
async fn preview_matches_a_url_with_the_server_origin() {
    let w = world().await;
    hand_made(&w, &[("List pets", "GET", "https://api.example.com/pets")]);
    let preview = w.svc.preview(HAND, LINK, None).await.expect("preview");
    assert_eq!(matched_keys(&preview), vec!["listPets".to_string()]);
}

#[tokio::test]
async fn preview_writes_nothing() {
    let w = world().await;
    hand_made(&w, &[("List pets", "GET", "{{baseUrl}}/pets")]);
    let before = snapshot(&hand_root(&w));
    w.svc.preview(HAND, LINK, None).await.expect("preview");
    assert_eq!(snapshot(&hand_root(&w)), before);
    assert!(!hand_root(&w).join(".rocket").exists());
}

#[tokio::test]
async fn preview_stores_no_token_and_reads_no_keychain() {
    let w = world().await;
    hand_made(&w, &[("List pets", "GET", "{{baseUrl}}/pets")]);
    let reads_before = w.secret_reads.load(std::sync::atomic::Ordering::SeqCst);
    w.svc
        .preview(
            HAND,
            "https://github.com/acme/api/blob/main/openapi.yaml",
            Some("ghp_x"),
        )
        .await
        .expect("preview");
    assert_eq!(
        w.secret_reads.load(std::sync::atomic::Ordering::SeqCst),
        reads_before
    );
    // The typed token went to the fetcher for this one request only.
    let requests = w.fetcher.requests.lock().expect("lock");
    assert_eq!(
        requests.last().and_then(|r| r.bearer_token.as_deref()),
        Some("ghp_x")
    );
}

#[tokio::test]
async fn preview_refuses_a_collection_that_has_a_source() {
    let w = world().await;
    let err = w
        .svc
        .preview(&w.name, LINK, None)
        .await
        .expect_err("refused");
    assert!(matches!(err, DomainError::Conflict(_)), "{err:?}");
}

#[tokio::test]
async fn preview_needs_a_typed_link() {
    let w = world().await;
    hand_made(&w, &[]);
    let err = w.svc.preview(HAND, "  ", None).await.expect_err("refused");
    assert!(matches!(err, DomainError::InvalidInput(_)), "{err:?}");
}

#[tokio::test]
async fn preview_rejects_a_token_for_a_non_github_link() {
    let w = world().await;
    hand_made(&w, &[]);
    let err = w
        .svc
        .preview(HAND, LINK, Some("ghp_x"))
        .await
        .expect_err("refused");
    assert!(
        err.to_string()
            .contains("Tokens work with GitHub links only."),
        "{err}"
    );
}

struct FailingFetcher;

#[async_trait]
impl SpecFetcher for FailingFetcher {
    async fn fetch(&self, _: SpecFetchRequest) -> Result<SpecFetchOutcome, SpecFetchFailure> {
        Err(SpecFetchFailure::Network(
            "cannot reach /home/me/secret/host".into(),
        ))
    }
}

#[tokio::test]
async fn preview_failure_gives_the_fixed_sentence() {
    let w = world().await;
    hand_made(&w, &[("List pets", "GET", "{{baseUrl}}/pets")]);
    let svc = OpenApiSyncService::new(
        Arc::new(FailingFetcher),
        Box::new(rocket_infra::FsOpenApiSourceRepo),
        Arc::new(NoSecrets(Arc::clone(&w.secret_reads))),
    )
    .with_apply(deps(
        &w.ws_path,
        Arc::new(rocket_infra::FsFileTxFactory::new()),
    ));
    let before = snapshot(&hand_root(&w));
    let err = svc.preview(HAND, LINK, None).await.expect_err("fails");
    assert!(!err.to_string().contains("secret"), "{err}");
    assert!(!err.to_string().is_empty());
    assert_eq!(snapshot(&hand_root(&w)), before);
}

fn mr(uid: &str, url: &str, http: bool) -> MatchRequest {
    MatchRequest {
        uid: uid.into(),
        request_path: format!("p/{uid}"),
        method: "GET".into(),
        url: url.into(),
        name: uid.into(),
        http,
    }
}

#[test]
fn the_digest_changes_when_a_request_url_changes() {
    let a = request_digest(&[mr("r1", "/a", true), mr("r2", "/b", true)]);
    let b = request_digest(&[mr("r1", "/a", true), mr("r2", "/c", true)]);
    assert_ne!(a, b);
    // The order of the list does not matter.
    assert_eq!(
        a,
        request_digest(&[mr("r2", "/b", true), mr("r1", "/a", true)])
    );
}

#[test]
fn the_digest_ignores_graphql_requests() {
    let a = request_digest(&[mr("r1", "/a", true)]);
    let b = request_digest(&[mr("r1", "/a", true), mr("g1", "/graphql", false)]);
    assert_eq!(a, b);
}
