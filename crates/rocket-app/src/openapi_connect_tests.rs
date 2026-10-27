//! Tests for connecting a hand-made collection to a spec. They reuse the world of the apply tests.

use super::*;
use crate::openapi_connect::{request_digest, ConnectOutcome, ConnectPreview, ConnectRow};
use rocket_collection::openapi_source::SyncInterval;
use rocket_collection::ConsentState;
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

// ---- confirm ----

/// The request the importer would write for `key`, so a matched request starts equal to it.
fn spec_request(key: &str) -> rocket_collection::Request {
    rocket_import::ParsedSpec::parse(&v1())
        .expect("spec")
        .converted_request(key)
        .expect("operation")
}

fn save_hand(w: &World, path: &str, request: &rocket_collection::Request) {
    w.repo
        .save_request(HAND, path, request)
        .expect("save request");
}

fn uid_of(w: &World, path: &str) -> String {
    w.repo.get_request(HAND, path).expect("get").uid
}

fn link(w: &World, path: &str, key: &str) -> ConnectRow {
    ConnectRow::Link {
        uid: uid_of(w, path),
        key: key.to_string(),
    }
}

async fn confirm(
    w: &World,
    rows: Vec<ConnectRow>,
    interval: Option<SyncInterval>,
) -> DomainResult<ConnectOutcome> {
    let preview = w.svc.preview(HAND, LINK, None).await.expect("preview");
    w.svc
        .confirm(
            HAND,
            LINK,
            None,
            rows,
            GroupBy::Tags,
            interval,
            &preview.digest,
            &preview.spec_sha256,
        )
        .await
}

fn hand_source(w: &World) -> rocket_collection::openapi_source::OpenApiSource {
    rocket_infra::FsOpenApiSourceRepo
        .load(&hand_root(w))
        .expect("load")
        .expect("source")
}

#[tokio::test]
async fn confirm_links_matched_rows_so_an_untouched_request_is_not_changed_by_you() {
    let w = world().await;
    w.repo.create(HAND).expect("create");
    save_hand(&w, "list-pets", &spec_request("listPets"));
    let rows = vec![
        link(&w, "list-pets", "listPets"),
        ConnectRow::Skip {
            key: "getPet".into(),
        },
        ConnectRow::Skip {
            key: "getInventory".into(),
        },
    ];
    let outcome = confirm(&w, rows, None).await.expect("confirm");
    assert_eq!(outcome.linked, 1);
    assert!(outcome.warnings.is_empty(), "{:?}", outcome.warnings);
    let changes = w.svc.changes(HAND).expect("changes");
    assert!(changes.modified.is_empty(), "{:?}", changes.modified);
    assert_eq!(hand_source(&w).operations.len(), 1);
    assert_eq!(hand_source(&w).operations[0].request_path, "list-pets");
}

#[tokio::test]
async fn confirm_links_an_edited_request_so_the_edit_reads_as_changed_by_you() {
    let w = world().await;
    w.repo.create(HAND).expect("create");
    let mut edited = spec_request("listPets");
    edited.url = "{{baseUrl}}/pets?mine=1".to_string();
    save_hand(&w, "list-pets", &edited);
    let rows = vec![
        link(&w, "list-pets", "listPets"),
        ConnectRow::Skip {
            key: "getPet".into(),
        },
        ConnectRow::Skip {
            key: "getInventory".into(),
        },
    ];
    confirm(&w, rows, None).await.expect("confirm");
    let changes = w.svc.changes(HAND).expect("changes");
    assert_eq!(changes.modified.len(), 1);
    assert_eq!(changes.modified[0].key.as_deref(), Some("listPets"));
    assert!(
        changes.modified[0].fields.contains(&"url".to_string()),
        "{:?}",
        changes.modified[0].fields
    );
}

#[tokio::test]
async fn confirm_adds_the_spec_only_rows_as_new_requests() {
    let w = world().await;
    hand_made(&w, &[("Mine", "GET", "{{baseUrl}}/mine")]);
    let mine = uid_of(&w, "Mine");
    let rows = vec![
        ConnectRow::Keep { uid: mine },
        ConnectRow::Add {
            key: "listPets".into(),
        },
        ConnectRow::Skip {
            key: "getPet".into(),
        },
        ConnectRow::Skip {
            key: "getInventory".into(),
        },
    ];
    let outcome = confirm(&w, rows, None).await.expect("confirm");
    assert_eq!(outcome.added, 1);
    let source = hand_source(&w);
    assert_eq!(source.operations.len(), 1);
    let added = w
        .repo
        .get_request(HAND, &source.operations[0].request_path)
        .expect("added request");
    assert_eq!(added.url, "{{baseUrl}}/pets");
    // The added request needs the base URL variable.
    let settings = w.repo.get_settings(HAND).expect("settings");
    let base = settings
        .variables
        .iter()
        .find(|v| v.key == "baseUrl")
        .expect("baseUrl");
    assert_eq!(base.value, "https://api.example.com");
}

#[tokio::test]
async fn a_skipped_spec_only_row_does_not_come_back_as_new_after_a_check() {
    let w = world().await;
    hand_made(&w, &[("Mine", "GET", "{{baseUrl}}/mine")]);
    let rows = vec![
        ConnectRow::Keep {
            uid: uid_of(&w, "Mine"),
        },
        ConnectRow::Skip {
            key: "listPets".into(),
        },
        ConnectRow::Skip {
            key: "getPet".into(),
        },
        ConnectRow::Skip {
            key: "getInventory".into(),
        },
    ];
    confirm(&w, rows, None).await.expect("confirm");
    // The spec gains one operation; the skipped ones stay hidden.
    let more = spec(json!({
        "/pets": {"get": op("listPets", "pets", json!({}))},
        "/pets/{id}": {"get": op("getPet", "pets", json!({"parameters": [
            {"name": "id", "in": "path", "required": true, "schema": {"type": "string"}}]}))},
        "/inventory": {"get": op("getInventory", "store", json!({}))},
        "/cats": {"get": op("listCats", "cats", json!({}))},
    }));
    *w.fetcher.body.lock().expect("lock") = more;
    // Enabling checks needs the approval the confirm granted.
    w.svc.check(&hand_root(&w)).await.expect("check");
    let plan = w.svc.plan(HAND).expect("plan");
    let keys: Vec<&str> = plan.plan.items.iter().map(|i| i.key()).collect();
    assert_eq!(keys, vec!["listCats"]);
}

#[tokio::test]
async fn an_operation_no_row_mentions_is_skipped() {
    let w = world().await;
    hand_made(&w, &[]);
    let outcome = confirm(
        &w,
        vec![ConnectRow::Add {
            key: "listPets".into(),
        }],
        None,
    )
    .await
    .expect("confirm");
    assert_eq!(outcome.added, 1);
    assert_eq!(outcome.skipped, 2);
    assert!(hand_source(&w).operations.len() == 1);
}

#[tokio::test]
async fn a_kept_row_changes_nothing() {
    let w = world().await;
    hand_made(&w, &[("Mine", "GET", "{{baseUrl}}/mine")]);
    let before = snapshot(
        &hand_root(&w)
            .join("Mine.yml")
            .parent()
            .expect("parent")
            .to_path_buf(),
    )
    .get("Mine.yml")
    .cloned();
    let rows = vec![ConnectRow::Keep {
        uid: uid_of(&w, "Mine"),
    }];
    confirm(&w, rows, None).await.expect("confirm");
    let after = snapshot(&hand_root(&w)).get("Mine.yml").cloned();
    assert_eq!(before, after);
    let changes = w.svc.changes(HAND).expect("changes");
    assert_eq!(changes.added.len(), 1);
}

#[tokio::test]
async fn confirm_rolls_back_when_the_commit_fails() {
    let w = world_with(Arc::new(Wrapper {
        inner: rocket_infra::FsFileTxFactory::new(),
        fail_stage_at: Some(1),
        recover_blocked: Arc::new(AtomicBool::new(false)),
    }))
    .await;
    hand_made(&w, &[("List pets", "GET", "{{baseUrl}}/pets")]);
    let before = snapshot(&hand_root(&w));
    let rows = vec![
        ConnectRow::Add {
            key: "listPets".into(),
        },
        ConnectRow::Add {
            key: "getPet".into(),
        },
        ConnectRow::Add {
            key: "getInventory".into(),
        },
    ];
    let result = confirm(&w, rows, Some(SyncInterval::Hourly)).await;
    assert!(result.is_err());
    // Opening a transaction writes the .gitignore of .rocket; nothing else may appear.
    let mut after = snapshot(&hand_root(&w));
    after.remove(".rocket/.gitignore");
    assert_eq!(after, before);
    for name in [
        "openapi-source.yml",
        "openapi-baseline.json",
        "openapi-local.json",
        "openapi-applied.spec",
    ] {
        assert!(!hand_root(&w).join(".rocket").join(name).exists(), "{name}");
    }
    let status = w.svc.status(&hand_root(&w), false);
    assert!(status.is_err(), "no source, so no status");
}

#[tokio::test]
async fn confirm_refuses_a_stale_digest() {
    let w = world().await;
    hand_made(&w, &[("List pets", "GET", "{{baseUrl}}/pets")]);
    let preview = w.svc.preview(HAND, LINK, None).await.expect("preview");
    // The collection changes after the preview.
    let mut request = w.repo.get_request(HAND, "List pets").expect("get");
    request.url = "{{baseUrl}}/changed".to_string();
    w.repo
        .save_request(HAND, "List pets", &request)
        .expect("save");
    let edited = snapshot(&hand_root(&w));
    let err = w
        .svc
        .confirm(
            HAND,
            LINK,
            None,
            Vec::new(),
            GroupBy::Tags,
            None,
            &preview.digest,
            &preview.spec_sha256,
        )
        .await
        .expect_err("stale");
    assert!(
        err.to_string()
            .contains("The collection changed. Run the preview again."),
        "{err}"
    );
    assert_eq!(snapshot(&hand_root(&w)), edited);
}

#[tokio::test]
async fn consent_is_granted_for_the_typed_link_after_the_files() {
    let w = world().await;
    hand_made(&w, &[]);
    confirm(&w, Vec::new(), Some(SyncInterval::Hourly))
        .await
        .expect("confirm");
    let status = w.svc.status(&hand_root(&w), false).expect("status");
    assert_eq!(status.consent, ConsentState::Granted);
    assert_eq!(status.interval, SyncInterval::Hourly);
    assert_eq!(status.location, LINK);
}

struct FailingConsent;

impl rocket_collection::OpenApiConsentStore for FailingConsent {
    fn get(&self, _: &Path, _: &str) -> DomainResult<Option<rocket_collection::OpenApiConsent>> {
        Ok(None)
    }
    fn grant(&self, _: &Path, _: &str, _: &str) -> DomainResult<()> {
        Err(DomainError::Internal("cannot write /home/me/secret".into()))
    }
    fn revoke(&self, _: &Path, _: &str) -> DomainResult<()> {
        Ok(())
    }
}

#[tokio::test]
async fn a_failed_consent_adds_a_warning_and_keeps_the_files() {
    let w = world().await;
    hand_made(&w, &[]);
    let svc = OpenApiSyncService::new(
        Arc::new(SharedFetcher(Arc::clone(&w.fetcher))),
        Box::new(rocket_infra::FsOpenApiSourceRepo),
        Arc::new(NoSecrets(Arc::clone(&w.secret_reads))),
    )
    .with_consent(Arc::new(FailingConsent))
    .with_backups(Arc::new(rocket_infra::FsOpenApiBackupRepo))
    .with_apply(deps(
        &w.ws_path,
        Arc::new(rocket_infra::FsFileTxFactory::new()),
    ));
    let preview = svc.preview(HAND, LINK, None).await.expect("preview");
    let outcome = svc
        .confirm(
            HAND,
            LINK,
            None,
            Vec::new(),
            GroupBy::Tags,
            Some(SyncInterval::Hourly),
            &preview.digest,
            &preview.spec_sha256,
        )
        .await
        .expect("confirm still succeeds");
    assert!(
        outcome
            .warnings
            .contains(&"Checks are not enabled yet.".to_string()),
        "{:?}",
        outcome.warnings
    );
    assert!(!outcome.warnings.iter().any(|w| w.contains("secret")));
    assert!(hand_root(&w).join(".rocket/openapi-source.yml").exists());
}

#[tokio::test]
async fn confirm_stores_the_applied_copy_so_view_spec_works() {
    let w = world().await;
    hand_made(&w, &[]);
    confirm(&w, Vec::new(), None).await.expect("confirm");
    let view = w.svc.spec_view(HAND).expect("view");
    assert_eq!(view.state, crate::openapi_sync_view::SpecViewState::InSync);
    assert_eq!(
        view.groups
            .iter()
            .map(|g| g.operations.len())
            .sum::<usize>(),
        3
    );
}

#[tokio::test]
async fn confirm_refuses_a_collection_that_already_has_a_source() {
    let w = world().await;
    let err = w
        .svc
        .confirm(
            &w.name,
            LINK,
            None,
            Vec::new(),
            GroupBy::Tags,
            None,
            "x",
            "y",
        )
        .await
        .expect_err("refused");
    assert!(matches!(err, DomainError::Conflict(_)), "{err:?}");
}

#[tokio::test]
async fn confirm_with_zero_matches_still_succeeds() {
    let w = world().await;
    hand_made(&w, &[("Mine", "GET", "{{baseUrl}}/mine")]);
    let rows = vec![
        ConnectRow::Keep {
            uid: uid_of(&w, "Mine"),
        },
        ConnectRow::Add {
            key: "listPets".into(),
        },
        ConnectRow::Add {
            key: "getPet".into(),
        },
        ConnectRow::Add {
            key: "getInventory".into(),
        },
    ];
    let outcome = confirm(&w, rows, None).await.expect("confirm");
    assert_eq!((outcome.linked, outcome.added, outcome.skipped), (0, 3, 0));
}

#[tokio::test]
async fn confirm_rejects_rows_that_name_a_missing_request_or_operation() {
    let w = world().await;
    hand_made(&w, &[]);
    let bad_key = vec![ConnectRow::Add { key: "nope".into() }];
    let err = confirm(&w, bad_key, None).await.expect_err("bad key");
    assert!(matches!(err, DomainError::InvalidInput(_)), "{err:?}");
    let bad_uid = vec![ConnectRow::Link {
        uid: "ghost".into(),
        key: "listPets".into(),
    }];
    let err = confirm(&w, bad_uid, None).await.expect_err("bad uid");
    assert!(matches!(err, DomainError::InvalidInput(_)), "{err:?}");
    assert!(!hand_root(&w).join(".rocket").exists());
}

#[tokio::test]
async fn confirm_rejects_an_operation_or_request_chosen_twice() {
    let w = world().await;
    hand_made(
        &w,
        &[
            ("A", "GET", "{{baseUrl}}/pets"),
            ("B", "GET", "{{baseUrl}}/pets"),
        ],
    );
    let rows = vec![link(&w, "A", "listPets"), link(&w, "B", "listPets")];
    let err = confirm(&w, rows, None).await.expect_err("twice");
    assert!(matches!(err, DomainError::InvalidInput(_)), "{err:?}");
    assert!(!hand_root(&w).join(".rocket").exists());
}

#[tokio::test]
async fn confirm_refuses_a_spec_that_changed_since_the_preview() {
    let w = world().await;
    hand_made(&w, &[("List pets", "GET", "{{baseUrl}}/pets")]);
    let preview = w.svc.preview(HAND, LINK, None).await.expect("preview");
    // The spec gains an operation the user never saw.
    *w.fetcher.body.lock().expect("lock") = v2();
    let before = snapshot(&hand_root(&w));
    let err = w
        .svc
        .confirm(
            HAND,
            LINK,
            None,
            vec![ConnectRow::Add {
                key: "listPets".into(),
            }],
            GroupBy::Tags,
            None,
            &preview.digest,
            &preview.spec_sha256,
        )
        .await
        .expect_err("stale spec");
    assert!(
        err.to_string()
            .contains("The spec changed. Run the preview again."),
        "{err}"
    );
    assert_eq!(snapshot(&hand_root(&w)), before);
}

#[tokio::test]
async fn a_link_only_connect_still_gets_the_base_url_variable() {
    let w = world().await;
    w.repo.create(HAND).expect("create");
    save_hand(&w, "list-pets", &spec_request("listPets"));
    let rows = vec![
        link(&w, "list-pets", "listPets"),
        ConnectRow::Skip {
            key: "getPet".into(),
        },
        ConnectRow::Skip {
            key: "getInventory".into(),
        },
    ];
    confirm(&w, rows, None).await.expect("confirm");
    let settings = w.repo.get_settings(HAND).expect("settings");
    let base = settings
        .variables
        .iter()
        .find(|v| v.key == "baseUrl")
        .expect("baseUrl");
    assert_eq!(base.value, "https://api.example.com");
}

#[tokio::test]
async fn an_existing_base_url_variable_is_left_alone() {
    let w = world().await;
    w.repo.create(HAND).expect("create");
    let mut settings = w.repo.get_settings(HAND).expect("settings");
    settings
        .variables
        .push(rocket_collection::settings::CollectionVariable {
            key: "baseUrl".into(),
            value: "https://mine.example.com".into(),
            initial_value: String::new(),
            enabled: true,
            secret: false,
        });
    w.repo.save_settings(HAND, &settings).expect("save");
    save_hand(&w, "list-pets", &spec_request("listPets"));
    let rows = vec![
        link(&w, "list-pets", "listPets"),
        ConnectRow::Skip {
            key: "getPet".into(),
        },
        ConnectRow::Skip {
            key: "getInventory".into(),
        },
    ];
    confirm(&w, rows, None).await.expect("confirm");
    let after = w.repo.get_settings(HAND).expect("settings");
    let base = after
        .variables
        .iter()
        .find(|v| v.key == "baseUrl")
        .expect("baseUrl");
    assert_eq!(base.value, "https://mine.example.com");
}

/// Fails to open a transaction with a message that holds a path, like the real one can.
struct LeakyBegin(rocket_infra::FsFileTxFactory);

impl CollectionFileTxFactory for LeakyBegin {
    fn begin(&self, _: &Path) -> DomainResult<Box<dyn CollectionFileTx>> {
        Err(DomainError::InvalidInput(
            "\"/home/me/secret/x.yml\" goes through a symbolic link".into(),
        ))
    }
    fn recover(&self, root: &Path) -> DomainResult<RecoveryOutcome> {
        self.0.recover(root)
    }
    fn list_files(&self, root: &Path) -> DomainResult<std::collections::HashSet<String>> {
        self.0.list_files(root)
    }
}

#[tokio::test]
async fn an_infrastructure_failure_gives_a_fixed_sentence() {
    let w = world_with(Arc::new(LeakyBegin(rocket_infra::FsFileTxFactory::new()))).await;
    hand_made(&w, &[]);
    let err = confirm(&w, Vec::new(), None).await.expect_err("fails");
    let text = err.to_string();
    assert!(
        !text.contains("secret") && !text.contains("symbolic"),
        "{text}"
    );
    assert!(
        text.contains("Connecting failed. Nothing was changed."),
        "{text}"
    );
}

#[tokio::test]
async fn consent_uses_the_typed_link_structure_for_a_contents_api_link() {
    let w = world().await;
    hand_made(&w, &[]);
    let github = "https://github.com/acme/api/blob/main/openapi.yaml";
    let preview = w.svc.preview(HAND, github, None).await.expect("preview");
    w.svc
        .confirm(
            HAND,
            github,
            None,
            Vec::new(),
            GroupBy::Tags,
            None,
            &preview.digest,
            &preview.spec_sha256,
        )
        .await
        .expect("confirm");
    let status = w.svc.status(&hand_root(&w), false).expect("status");
    assert_eq!(status.consent, ConsentState::Granted);
}

#[tokio::test]
async fn a_failed_token_save_adds_a_warning_and_keeps_the_files() {
    let w = world().await;
    hand_made(&w, &[]);
    let github = "https://github.com/acme/api/blob/main/openapi.yaml";
    let svc = OpenApiSyncService::new(
        Arc::new(SharedFetcher(Arc::clone(&w.fetcher))),
        Box::new(rocket_infra::FsOpenApiSourceRepo),
        Arc::new(FailingSecrets),
    )
    .with_consent(Arc::new(rocket_collection::InMemoryConsentStore::new()))
    .with_backups(Arc::new(rocket_infra::FsOpenApiBackupRepo))
    .with_apply(deps(
        &w.ws_path,
        Arc::new(rocket_infra::FsFileTxFactory::new()),
    ));
    let preview = svc
        .preview(HAND, github, Some("ghp_x"))
        .await
        .expect("preview");
    let outcome = svc
        .confirm(
            HAND,
            github,
            Some("ghp_x"),
            Vec::new(),
            GroupBy::Tags,
            None,
            &preview.digest,
            &preview.spec_sha256,
        )
        .await
        .expect("confirm");
    assert!(
        outcome
            .warnings
            .contains(&"The token was not saved.".to_string()),
        "{:?}",
        outcome.warnings
    );
    assert!(hand_root(&w).join(".rocket/openapi-source.yml").exists());
}

struct FailingSecrets;

impl SecretStore for FailingSecrets {
    fn get(&self, _: &str, _: &str) -> DomainResult<Option<String>> {
        Ok(None)
    }
    fn set(&self, _: &str, _: &str, _: &str) -> DomainResult<()> {
        Err(DomainError::Internal(
            "keychain locked at /home/me/secret".into(),
        ))
    }
    fn delete(&self, _: &str, _: &str) -> DomainResult<()> {
        Ok(())
    }
}
