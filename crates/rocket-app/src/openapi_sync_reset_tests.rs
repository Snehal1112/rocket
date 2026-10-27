//! Tests for reset, restore and discard. They reuse the world of the apply tests.

use super::*;
use rocket_collection::openapi_backup::OpenApiBackupRepository;

const OLDER: &str =
    "These requests cannot be reset because the sync data is from an older version.";

fn path_of(w: &World, key: &str) -> String {
    w.source()
        .operations
        .iter()
        .find(|o| o.key == key)
        .expect("entry")
        .request_path
        .clone()
}

fn load(w: &World, key: &str) -> rocket_collection::Request {
    w.repo.get_request(&w.name, &path_of(w, key)).expect("get")
}

fn edit(w: &World, key: &str, url: &str) {
    let path = path_of(w, key);
    let mut request = w.repo.get_request(&w.name, &path).expect("get");
    request.url = url.to_string();
    w.repo.save_request(&w.name, &path, &request).expect("save");
}

fn backups(w: &World) -> Option<rocket_collection::OpenApiBackups> {
    rocket_infra::FsOpenApiBackupRepo
        .load(&w.root)
        .expect("backups")
}

fn keys(list: &[&str]) -> Vec<String> {
    list.iter().map(|k| k.to_string()).collect()
}

#[tokio::test]
async fn reset_restores_spec_fields_and_writes_a_backup() {
    let w = world().await;
    let original = load(&w, "listPets").url;
    edit(&w, "listPets", "https://edited.example.com/x");
    let report = w.svc.reset(&w.name, &keys(&["listPets"])).expect("reset");
    assert_eq!(report.reset, keys(&["listPets"]));
    assert_eq!(load(&w, "listPets").url, original);
    let saved = backups(&w).expect("file");
    assert_eq!(saved.entries.len(), 1);
    assert_eq!(saved.entries[0].key, "listPets");
    assert_eq!(saved.entries[0].request.url, "https://edited.example.com/x");
    assert_eq!(saved.entries[0].request_path, path_of(&w, "listPets"));
}

#[tokio::test]
async fn reset_keeps_scripts_and_docs() {
    let w = world().await;
    let path = path_of(&w, "listPets");
    let mut request = w.repo.get_request(&w.name, &path).expect("get");
    request.url = "https://edited.example.com/x".to_string();
    request.pre_request_script = Some("rok.log('mine')".to_string());
    request.tags = vec!["mine".to_string()];
    w.repo.save_request(&w.name, &path, &request).expect("save");
    w.svc.reset(&w.name, &keys(&["listPets"])).expect("reset");
    let after = load(&w, "listPets");
    assert_eq!(after.pre_request_script.as_deref(), Some("rok.log('mine')"));
    assert_eq!(after.tags, vec!["mine".to_string()]);
}

#[tokio::test]
async fn resetting_an_unchanged_request_writes_no_backup() {
    let w = world().await;
    let report = w.svc.reset(&w.name, &keys(&["listPets"])).expect("reset");
    assert!(report.reset.is_empty());
    assert_eq!(report.skipped, keys(&["listPets"]));
    assert!(backups(&w).is_none());
}

#[tokio::test]
async fn resetting_twice_keeps_the_latest_backup_only() {
    let w = world().await;
    edit(&w, "listPets", "https://first.example.com/x");
    w.svc.reset(&w.name, &keys(&["listPets"])).expect("reset");
    edit(&w, "listPets", "https://second.example.com/x");
    w.svc.reset(&w.name, &keys(&["listPets"])).expect("reset");
    let saved = backups(&w).expect("file");
    assert_eq!(saved.entries.len(), 1);
    assert_eq!(saved.entries[0].request.url, "https://second.example.com/x");
}

#[tokio::test]
async fn a_key_without_an_operation_is_skipped() {
    let w = world().await;
    edit(&w, "listPets", "https://edited.example.com/x");
    let report = w
        .svc
        .reset(&w.name, &keys(&["listPets", "nope"]))
        .expect("reset");
    assert_eq!(report.reset, keys(&["listPets"]));
    assert_eq!(report.skipped, keys(&["nope"]));
}

#[tokio::test]
async fn reset_refuses_without_a_baseline() {
    let w = world().await;
    edit(&w, "listPets", "https://edited.example.com/x");
    std::fs::remove_file(w.root.join(".rocket/openapi-baseline.json")).expect("remove");
    let err = w
        .svc
        .reset(&w.name, &keys(&["listPets"]))
        .expect_err("refused");
    assert!(err.to_string().contains(OLDER), "{err}");
    assert_eq!(load(&w, "listPets").url, "https://edited.example.com/x");
    assert!(backups(&w).is_none());
}

#[tokio::test]
async fn reset_refuses_when_the_converter_version_differs() {
    let w = world().await;
    edit(&w, "listPets", "https://edited.example.com/x");
    let mut baseline = rocket_infra::FsOpenApiBaselineRepo
        .load(&w.root)
        .expect("load")
        .expect("baseline");
    baseline.converter_version += 1;
    rocket_infra::FsOpenApiBaselineRepo
        .save(&w.root, &baseline)
        .expect("save");
    let err = w
        .svc
        .reset(&w.name, &keys(&["listPets"]))
        .expect_err("refused");
    assert!(err.to_string().contains(OLDER), "{err}");
    assert!(backups(&w).is_none());
}

#[tokio::test]
async fn reset_is_a_conflict_while_an_apply_runs() {
    let tx = Arc::new(rocket_infra::FsFileTxFactory::new());
    let w = world_with(tx.clone()).await;
    edit(&w, "listPets", "https://edited.example.com/x");
    let open = tx.begin(&w.root).expect("begin");
    let err = w
        .svc
        .reset(&w.name, &keys(&["listPets"]))
        .expect_err("refused");
    assert!(matches!(err, DomainError::Conflict(_)), "{err:?}");
    assert_eq!(load(&w, "listPets").url, "https://edited.example.com/x");
    open.abort();
    w.svc
        .reset(&w.name, &keys(&["listPets"]))
        .expect("after the apply");
}

/// Edits a request file on the first staged write, like an editor save between the load and
/// the commit.
struct EditOnStage {
    inner: rocket_infra::FsFileTxFactory,
    edit: Arc<dyn Fn() + Send + Sync>,
}

struct EditOnStageTx {
    inner: Box<dyn CollectionFileTx>,
    edit: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl CollectionFileTx for EditOnStageTx {
    fn stage_put(&mut self, rel: &str, bytes: &[u8]) -> DomainResult<()> {
        if let Some(edit) = self.edit.take() {
            edit();
        }
        self.inner.stage_put(rel, bytes)
    }
    fn stage_delete(&mut self, rel: &str) -> DomainResult<()> {
        self.inner.stage_delete(rel)
    }
    fn commit(self: Box<Self>) -> DomainResult<()> {
        self.inner.commit()
    }
    fn abort(self: Box<Self>) {
        self.inner.abort()
    }
}

impl CollectionFileTxFactory for EditOnStage {
    fn begin(&self, root: &Path) -> DomainResult<Box<dyn CollectionFileTx>> {
        Ok(Box::new(EditOnStageTx {
            inner: self.inner.begin(root)?,
            edit: Some(Arc::clone(&self.edit)),
        }))
    }
    fn recover(&self, root: &Path) -> DomainResult<RecoveryOutcome> {
        self.inner.recover(root)
    }
    fn list_files(&self, root: &Path) -> DomainResult<std::collections::HashSet<String>> {
        self.inner.list_files(root)
    }
}

#[tokio::test]
async fn reset_writes_nothing_when_the_file_changed_on_disk() {
    let slot: Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>> = Arc::new(Mutex::new(None));
    let late: Arc<dyn Fn() + Send + Sync> = {
        let slot = Arc::clone(&slot);
        Arc::new(move || {
            if let Some(f) = slot.lock().expect("lock").as_ref() {
                f();
            }
        })
    };
    let w = world_with(Arc::new(EditOnStage {
        inner: rocket_infra::FsFileTxFactory::new(),
        edit: late,
    }))
    .await;
    edit(&w, "listPets", "https://edited.example.com/x");
    let path = path_of(&w, "listPets");
    let repo_path = w.ws_path.join("collections");
    let name = w.name.clone();
    *slot.lock().expect("lock") = Some(Arc::new(move || {
        let repo = rocket_infra::FsCollectionRepo::new_standalone(repo_path.clone());
        let mut r = repo.get_request(&name, &path).expect("get");
        r.description = Some(rocket_shared::description::Description::text(
            "typed meanwhile",
        ));
        repo.save_request(&name, &path, &r).expect("save");
    }));
    let err = w
        .svc
        .reset(&w.name, &keys(&["listPets"]))
        .expect_err("must fail");
    assert!(matches!(err, DomainError::Conflict(_)), "{err:?}");
    // The editor's version is on disk and no backup was written.
    assert_eq!(load(&w, "listPets").url, "https://edited.example.com/x");
    assert!(load(&w, "listPets").description.is_some());
    assert!(backups(&w).is_none());
}

#[tokio::test]
async fn restore_puts_the_backed_up_request_back_and_removes_the_entry() {
    let w = world().await;
    edit(&w, "listPets", "https://edited.example.com/x");
    w.svc.reset(&w.name, &keys(&["listPets"])).expect("reset");
    w.svc.restore(&w.name, "listPets").expect("restore");
    assert_eq!(load(&w, "listPets").url, "https://edited.example.com/x");
    assert!(backups(&w).expect("file").entries.is_empty());
}

#[tokio::test]
async fn restore_recreates_a_deleted_request_with_a_numeric_suffix_when_the_path_is_taken() {
    let w = world().await;
    edit(&w, "listPets", "https://edited.example.com/x");
    w.svc.reset(&w.name, &keys(&["listPets"])).expect("reset");
    let path = path_of(&w, "listPets");
    w.repo.delete_request(&w.name, &path).expect("delete");
    // Another request now holds the old path.
    let squatter = rocket_collection::Request::new(
        "Squatter",
        rocket_shared::types::HttpMethod::Get,
        "https://squat.example.com",
    );
    w.repo
        .save_request(&w.name, &path, &squatter)
        .expect("squat");
    w.svc.restore(&w.name, "listPets").expect("restore");
    let moved = path_of(&w, "listPets");
    assert_ne!(moved, path);
    assert!(moved.ends_with("-2"), "{moved}");
    assert_eq!(load(&w, "listPets").url, "https://edited.example.com/x");
    assert_eq!(
        w.repo.get_request(&w.name, &path).expect("squatter").name,
        "Squatter"
    );
}

#[tokio::test]
async fn restore_recreates_a_deleted_request_at_its_old_path() {
    let w = world().await;
    edit(&w, "listPets", "https://edited.example.com/x");
    w.svc.reset(&w.name, &keys(&["listPets"])).expect("reset");
    let path = path_of(&w, "listPets");
    w.repo.delete_request(&w.name, &path).expect("delete");
    w.svc.restore(&w.name, "listPets").expect("restore");
    assert_eq!(path_of(&w, "listPets"), path);
    assert_eq!(load(&w, "listPets").url, "https://edited.example.com/x");
}

#[tokio::test]
async fn restore_works_after_the_spec_changed() {
    let w = world().await;
    edit(&w, "listPets", "https://edited.example.com/x");
    w.svc.reset(&w.name, &keys(&["listPets"])).expect("reset");
    w.check_with(v2()).await;
    w.svc.restore(&w.name, "listPets").expect("restore");
    assert_eq!(load(&w, "listPets").url, "https://edited.example.com/x");
}

#[tokio::test]
async fn restore_of_an_unknown_key_is_not_found() {
    let w = world().await;
    let err = w.svc.restore(&w.name, "listPets").expect_err("none");
    assert!(matches!(err, DomainError::NotFound(_)), "{err:?}");
}

#[tokio::test]
async fn discard_removes_only_the_entry() {
    let w = world().await;
    edit(&w, "listPets", "https://edited.example.com/x");
    edit(&w, "getPet", "https://edited.example.com/y");
    w.svc
        .reset(&w.name, &keys(&["listPets", "getPet"]))
        .expect("reset");
    w.svc.discard(&w.name, "listPets").expect("discard");
    let saved = backups(&w).expect("file");
    let left: Vec<&str> = saved.entries.iter().map(|e| e.key.as_str()).collect();
    assert_eq!(left, vec!["getPet"]);
    // The reset request stays reset.
    assert_ne!(load(&w, "listPets").url, "https://edited.example.com/x");
}

#[tokio::test]
async fn a_sync_that_updates_an_endpoint_drops_its_backup() {
    let w = world().await;
    edit(&w, "listPets", "https://edited.example.com/x");
    w.svc.reset(&w.name, &keys(&["listPets"])).expect("reset");
    w.check_with(v2()).await;
    let plan = w.plan();
    w.svc
        .apply(&w.name, &w.approve(&plan, &["listPets"]))
        .await
        .expect("apply");
    assert!(backups(&w).expect("file").entries.is_empty());
}

#[tokio::test]
async fn a_partial_apply_keeps_backups_of_endpoints_it_did_not_touch() {
    let w = world().await;
    edit(&w, "getPet", "https://edited.example.com/y");
    w.svc.reset(&w.name, &keys(&["getPet"])).expect("reset");
    w.check_with(v2()).await;
    let plan = w.plan();
    w.svc
        .apply(&w.name, &w.approve(&plan, &["listPets"]))
        .await
        .expect("apply");
    let saved = backups(&w).expect("file");
    assert_eq!(saved.entries.len(), 1);
    assert_eq!(saved.entries[0].key, "getPet");
}

#[tokio::test]
async fn reset_restore_and_discard_never_fetch() {
    let w = world().await;
    edit(&w, "listPets", "https://edited.example.com/x");
    w.fetcher.requests.lock().expect("lock").clear();
    w.svc.reset(&w.name, &keys(&["listPets"])).expect("reset");
    w.svc.restore(&w.name, "listPets").expect("restore");
    w.svc.reset(&w.name, &keys(&["listPets"])).expect("reset");
    w.svc.discard(&w.name, "listPets").expect("discard");
    assert!(w.fetcher.requests.lock().expect("lock").is_empty());
    assert_eq!(w.secret_reads.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_corrupt_backups_file_fails_reset_loudly() {
    let w = world().await;
    edit(&w, "listPets", "https://edited.example.com/x");
    std::fs::write(w.root.join(".rocket/openapi-reset-backups.json"), "{oops").expect("write");
    assert!(w.svc.reset(&w.name, &keys(&["listPets"])).is_err());
    assert_eq!(load(&w, "listPets").url, "https://edited.example.com/x");
}

#[tokio::test]
async fn reset_never_touches_another_request_at_the_old_path() {
    let w = world().await;
    edit(&w, "listPets", "https://edited.example.com/x");
    let path = path_of(&w, "listPets");
    w.repo.delete_request(&w.name, &path).expect("delete");
    let squatter = rocket_collection::Request::new(
        "Squatter",
        rocket_shared::types::HttpMethod::Get,
        "https://squat.example.com",
    );
    w.repo
        .save_request(&w.name, &path, &squatter)
        .expect("squat");
    let report = w.svc.reset(&w.name, &keys(&["listPets"])).expect("reset");
    assert_eq!(report.skipped, keys(&["listPets"]));
    assert_eq!(
        w.repo.get_request(&w.name, &path).expect("squatter").url,
        "https://squat.example.com"
    );
    assert!(backups(&w).is_none());
}

#[tokio::test]
async fn changes_lists_backups_with_differing_fields() {
    let w = world().await;
    edit(&w, "listPets", "https://edited.example.com/x");
    w.svc.reset(&w.name, &keys(&["listPets"])).expect("reset");
    let view = w.svc.changes(&w.name).expect("changes");
    // After the reset the request equals the spec, so only the backup remains.
    assert!(view.modified.is_empty());
    assert_eq!(view.backups.len(), 1);
    let row = &view.backups[0];
    assert_eq!(row.key, "listPets");
    assert_eq!(row.url, "https://edited.example.com/x");
    assert_eq!(row.request_path, path_of(&w, "listPets"));
    assert_eq!(row.fields, vec!["url".to_string()]);
    assert!(!row.saved_at.is_empty());
    assert!(view.reset_available);
}

#[tokio::test]
async fn changes_reports_reset_unavailable_without_a_trusted_baseline() {
    let w = world().await;
    std::fs::remove_file(w.root.join(".rocket/openapi-baseline.json")).expect("remove");
    let view = w.svc.changes(&w.name).expect("changes");
    assert!(!view.reset_available);
}

#[tokio::test]
async fn a_corrupt_backups_file_gives_an_error_not_an_empty_list() {
    let w = world().await;
    std::fs::write(w.root.join(".rocket/openapi-reset-backups.json"), "{oops").expect("write");
    assert!(w.svc.changes(&w.name).is_err());
}

#[tokio::test]
async fn changes_has_no_backups_for_a_fresh_import() {
    let w = world().await;
    let view = w.svc.changes(&w.name).expect("changes");
    assert!(view.backups.is_empty());
    assert!(view.reset_available);
}
