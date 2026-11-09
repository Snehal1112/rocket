//! `ExportBackedSyncCollections` over a real folder, a real export and the real journaled
//! file transaction.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rocket_collection::{CollectionFileTxFactory, CollectionRepository};
use rocket_export::{bruno_zip, zip_entries, SourceFile};
use rocket_infra::{FsCollectionFileSource, FsCollectionRepo, FsFileTxFactory};
use rocket_shared::error::{DomainError, DomainResult};
use tempfile::TempDir;

use super::sync_collections::ExportBackedSyncCollections;
use super::sync_ports::{base_from_reader, sha256_hex, FileChange, SyncCollectionPort};
use crate::collection_export_service::CollectionExportService;
use crate::collection_lock::collection_lock;

const OC: &str = "opencollection: \"1.0.0\"\nuid: col-uid-1\ninfo:\n  name: c\n";
const LOGIN: &str = "uid: req-uid-1\ninfo:\n  name: Login\n  type: http\nhttp:\n  method: POST\n  url: https://api.test/login\n  headers:\n    - name: Authorization\n      value: Bearer LOCALSECRET\n    - name: Accept\n      value: application/json\nexamples:\n  - name: ok\n";
const FOLDER: &str = "info:\n  name: users\n  type: folder\n";
const GET: &str =
    "info:\n  name: Get\n  type: http\nhttp:\n  method: GET\n  url: https://api.test/users\n";

struct Rig {
    _ws: TempDir,
    port: Arc<ExportBackedSyncCollections>,
    repo: Arc<FsCollectionRepo>,
    export: Arc<CollectionExportService>,
    root: PathBuf,
}

fn rig_with(tx: Arc<dyn CollectionFileTxFactory>) -> Rig {
    let ws = TempDir::new().expect("tempdir");
    let collections = ws.path().join("collections");
    let root = collections.join("c");
    std::fs::create_dir_all(&root).expect("mkdir");
    let repo = Arc::new(FsCollectionRepo::new_standalone(collections));
    let source = FsCollectionFileSource::new(Arc::new(Mutex::new(ws.path().to_path_buf())));
    let export = Arc::new(CollectionExportService::new(Box::new(source)));
    let port = Arc::new(ExportBackedSyncCollections::new(
        Arc::clone(&export),
        repo.clone() as Arc<dyn CollectionRepository>,
        tx,
    ));
    Rig {
        _ws: ws,
        port,
        repo,
        export,
        root,
    }
}

fn rig() -> Rig {
    rig_with(Arc::new(FsFileTxFactory::new()))
}

fn write(rig: &Rig, rel: &str, text: &str) {
    let path = rig.root.join(rel);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, text).expect("write");
}

fn read(rig: &Rig, rel: &str) -> String {
    std::fs::read_to_string(rig.root.join(rel)).expect("read")
}

fn seed(rig: &Rig) {
    write(rig, "opencollection.yml", OC);
    write(rig, "Login.yml", LOGIN);
    write(rig, "users/folder.yml", FOLDER);
    write(rig, "users/Get.yml", GET);
}

/// Every file under the root except the transaction's own `.rocket` folder.
fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, prefix: &str, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).expect("lists") {
            let entry = entry.expect("entry");
            let name = entry.file_name().to_string_lossy().into_owned();
            if prefix.is_empty() && name == ".rocket" {
                continue;
            }
            let rel = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if entry.file_type().expect("type").is_dir() {
                walk(&entry.path(), &rel, out);
            } else {
                out.insert(rel, std::fs::read(entry.path()).expect("read"));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, "", &mut out);
    out
}

/// The bytes the export puts in the ZIP for one raw file, as a peer would send them.
fn scrubbed(rel: &str, raw: &str) -> Vec<u8> {
    let file = SourceFile {
        rel_path: rel.to_string(),
        bytes: raw.as_bytes().to_vec(),
    };
    let (zip, _) = bruno_zip(vec![file], "c", false).expect("exports");
    zip_entries(Cursor::new(zip)).expect("entries").remove(0).1
}

#[test]
fn exists_and_collection_uid_follow_the_collection() {
    let rig = rig();
    seed(&rig);
    assert!(rig.port.exists("c").expect("exists"));
    assert!(!rig.port.exists("missing").expect("exists"));
    assert_eq!(
        rig.port.collection_uid("c").expect("uid"),
        Some("col-uid-1".to_string())
    );
}

#[test]
fn manifest_is_stable_across_two_calls() {
    let rig = rig();
    seed(&rig);
    let first = rig.port.manifest("c").expect("manifest");
    let second = rig.port.manifest("c").expect("manifest");
    assert_eq!(first, second);
    assert_eq!(first.base_hash(), second.base_hash());
}

#[test]
fn manifest_paths_have_no_root_folder_and_skip_files_the_sync_does_not_carry() {
    let rig = rig();
    seed(&rig);
    write(&rig, "environments/dev.yml", "name: dev\nvariables: []\n");
    write(&rig, "flows/f.yml", "name: f\n");
    write(&rig, ".rocket/contracts/c.yml", "a: 1\n");
    write(&rig, "notes.txt", "x");
    write(&rig, ".git/config", "x");
    let manifest = rig.port.manifest("c").expect("manifest");
    let paths: Vec<&str> = manifest.paths().collect();
    assert_eq!(
        paths,
        vec![
            "Login.yml",
            "opencollection.yml",
            "users/Get.yml",
            "users/folder.yml"
        ]
    );
}

#[test]
fn a_secret_change_does_not_change_the_manifest() {
    let rig = rig();
    seed(&rig);
    let before = rig.port.manifest("c").expect("manifest");
    write(
        &rig,
        "Login.yml",
        &LOGIN.replace("LOCALSECRET", "OTHERSECRET"),
    );
    let after_secret = rig.port.manifest("c").expect("manifest");
    assert_eq!(before, after_secret);
    // A real change to the same file does change that path and no other.
    write(
        &rig,
        "Login.yml",
        &LOGIN.replace("api.test/login", "api.test/signin"),
    );
    let after_url = rig.port.manifest("c").expect("manifest");
    assert_ne!(before.get("Login.yml"), after_url.get("Login.yml"));
    assert_eq!(before.get("users/Get.yml"), after_url.get("users/Get.yml"));
}

#[test]
fn the_first_send_zip_gives_the_same_manifest() {
    let rig = rig();
    seed(&rig);
    write(&rig, "environments/dev.yml", "name: dev\nvariables: []\n");
    // The first send may include environments. The base must not.
    let sent = rig.export.export_bruno("c", true).expect("exports").bytes;
    let base = base_from_reader(Cursor::new(sent)).expect("base");
    assert_eq!(base.manifest, rig.port.manifest("c").expect("manifest"));
}

#[test]
fn export_files_returns_scrubbed_bytes_in_the_asked_order() {
    let rig = rig();
    seed(&rig);
    let files = rig
        .port
        .export_files("c", &["users/Get.yml".to_string(), "Login.yml".to_string()])
        .expect("exports");
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["users/Get.yml", "Login.yml"]);
    let login = String::from_utf8(files[1].bytes.clone()).expect("utf8");
    assert!(!login.contains("LOCALSECRET"));
    assert!(!login.contains("req-uid-1"));
    let manifest = rig.port.manifest("c").expect("manifest");
    assert_eq!(
        manifest.get("Login.yml"),
        Some(sha256_hex(&files[1].bytes).as_str())
    );
}

#[test]
fn export_files_refuses_a_path_the_collection_does_not_carry() {
    let rig = rig();
    seed(&rig);
    let err = rig
        .port
        .export_files("c", &["nope.yml".to_string()])
        .expect_err("refused");
    assert!(matches!(err, DomainError::NotFound(_)));
}

#[test]
fn apply_put_merges_and_keeps_local_secret_and_uid() {
    let rig = rig();
    seed(&rig);
    let peer = "info:\n  name: Login\n  type: http\nhttp:\n  method: POST\n  url: https://api.test/v2/login\n  headers:\n    - name: Authorization\n      value: Bearer PEERSECRET\n    - name: Accept\n      value: application/json\n";
    let received = scrubbed("Login.yml", peer);
    rig.port
        .apply(
            "c",
            &[FileChange::Put {
                path: "Login.yml".into(),
                bytes: received.clone(),
            }],
        )
        .expect("applies");
    let text = read(&rig, "Login.yml");
    assert!(text.contains("https://api.test/v2/login"));
    assert!(text.contains("LOCALSECRET"));
    assert!(!text.contains("PEERSECRET"));
    assert!(text.contains("uid: req-uid-1"));
    assert!(text.contains("examples"));
    // Both sides now hash alike: the manifest entry is the hash of the received bytes.
    let manifest = rig.port.manifest("c").expect("manifest");
    assert_eq!(
        manifest.get("Login.yml"),
        Some(sha256_hex(&received).as_str())
    );
}

#[test]
fn apply_put_of_a_new_file_writes_it_as_received() {
    let rig = rig();
    seed(&rig);
    let received = scrubbed(
        "users/New.yml",
        "info:\n  name: New\n  type: http\nhttp:\n  method: GET\n  url: https://x\n",
    );
    rig.port
        .apply(
            "c",
            &[FileChange::Put {
                path: "users/New.yml".into(),
                bytes: received.clone(),
            }],
        )
        .expect("applies");
    assert_eq!(
        std::fs::read(rig.root.join("users/New.yml")).expect("read"),
        received
    );
}

#[test]
fn apply_put_as_writes_as_received_and_refuses_an_existing_path() {
    let rig = rig();
    seed(&rig);
    let received = scrubbed(
        "Login.yml",
        "info:\n  name: Login (from peer)\n  type: http\n",
    );
    rig.port
        .apply(
            "c",
            &[FileChange::PutAs {
                path: "Login (from peer).yml".into(),
                bytes: received.clone(),
            }],
        )
        .expect("applies");
    assert_eq!(
        std::fs::read(rig.root.join("Login (from peer).yml")).expect("read"),
        received
    );
    // Mine stays.
    assert!(read(&rig, "Login.yml").contains("LOCALSECRET"));

    let before = snapshot(&rig.root);
    let err = rig
        .port
        .apply(
            "c",
            &[FileChange::PutAs {
                path: "login.yml".into(),
                bytes: received,
            }],
        )
        .expect_err("a case variant is taken");
    assert!(matches!(err, DomainError::AlreadyExists(_)));
    assert_eq!(snapshot(&rig.root), before);
}

#[test]
fn apply_rename_moves_the_file_with_its_local_content() {
    let rig = rig();
    seed(&rig);
    rig.port
        .apply(
            "c",
            &[FileChange::Rename {
                from: "Login.yml".into(),
                to: "auth/Login.yml".into(),
            }],
        )
        .expect("applies");
    assert!(!rig.root.join("Login.yml").exists());
    let moved = read(&rig, "auth/Login.yml");
    assert!(moved.contains("LOCALSECRET"));
    assert!(moved.contains("uid: req-uid-1"));
}

#[test]
fn apply_rename_refuses_a_missing_source_and_a_taken_target() {
    let rig = rig();
    seed(&rig);
    let before = snapshot(&rig.root);
    let missing = rig
        .port
        .apply(
            "c",
            &[FileChange::Rename {
                from: "Nope.yml".into(),
                to: "X.yml".into(),
            }],
        )
        .expect_err("refused");
    assert!(matches!(missing, DomainError::NotFound(_)));
    let taken = rig
        .port
        .apply(
            "c",
            &[FileChange::Rename {
                from: "Login.yml".into(),
                to: "users/Get.yml".into(),
            }],
        )
        .expect_err("refused");
    assert!(matches!(taken, DomainError::AlreadyExists(_)));
    assert_eq!(snapshot(&rig.root), before);
}

#[test]
fn apply_delete_removes_the_file() {
    let rig = rig();
    seed(&rig);
    rig.port
        .apply(
            "c",
            &[FileChange::Delete {
                path: "users/Get.yml".into(),
            }],
        )
        .expect("applies");
    assert!(!rig.root.join("users/Get.yml").exists());
    assert!(rig.root.join("Login.yml").exists());
}

#[test]
fn apply_with_a_bad_change_changes_nothing() {
    let rig = rig();
    seed(&rig);
    let before = snapshot(&rig.root);
    let changes = vec![
        FileChange::Put {
            path: "Login.yml".into(),
            bytes: scrubbed("Login.yml", "info:\n  name: Login\n  type: http\n"),
        },
        FileChange::Delete {
            path: "users/Get.yml".into(),
        },
        FileChange::Rename {
            from: "Missing.yml".into(),
            to: "Other.yml".into(),
        },
    ];
    assert!(rig.port.apply("c", &changes).is_err());
    assert_eq!(snapshot(&rig.root), before);
}

#[test]
fn apply_with_invalid_yaml_changes_nothing() {
    let rig = rig();
    seed(&rig);
    let before = snapshot(&rig.root);
    let changes = vec![
        FileChange::Put {
            path: "Login.yml".into(),
            bytes: scrubbed("Login.yml", "info:\n  name: Login\n  type: http\n"),
        },
        FileChange::Put {
            path: "users/Get.yml".into(),
            bytes: b"a: [1, 2".to_vec(),
        },
    ];
    let err = rig.port.apply("c", &changes).expect_err("refused");
    assert!(matches!(err, DomainError::InvalidInput(_)));
    assert_eq!(snapshot(&rig.root), before);
}

#[test]
fn apply_refuses_paths_the_export_does_not_carry() {
    let rig = rig();
    seed(&rig);
    write(
        &rig,
        "environments/prod.yml",
        "name: prod\nvariables:\n  - name: k\n    value: SECRETVALUE\n",
    );
    let before = snapshot(&rig.root);
    let body = b"a: 1\n".to_vec();
    for bad in [
        "environments/prod.yml",
        ".rocket/x.yml",
        "../x.yml",
        "flows/a.yml",
        "workspace.yml",
        "notes.txt",
        "/abs.yml",
        "",
    ] {
        let put = rig.port.apply(
            "c",
            &[FileChange::Put {
                path: bad.into(),
                bytes: body.clone(),
            }],
        );
        assert!(
            matches!(put, Err(DomainError::InvalidInput(_))),
            "put {bad}"
        );
        let delete = rig
            .port
            .apply("c", &[FileChange::Delete { path: bad.into() }]);
        assert!(
            matches!(delete, Err(DomainError::InvalidInput(_))),
            "delete {bad}"
        );
    }
    let rename = rig.port.apply(
        "c",
        &[FileChange::Rename {
            from: "Login.yml".into(),
            to: "environments/x.yml".into(),
        }],
    );
    assert!(matches!(rename, Err(DomainError::InvalidInput(_))));
    assert_eq!(snapshot(&rig.root), before);
}

#[test]
fn an_empty_change_list_is_a_no_op() {
    let rig = rig();
    seed(&rig);
    let before = snapshot(&rig.root);
    rig.port.apply("c", &[]).expect("applies");
    assert_eq!(snapshot(&rig.root), before);
}

#[test]
fn apply_rolls_back_every_file_when_the_commit_fails_at_any_step() {
    let changes = vec![
        FileChange::Put {
            path: "Login.yml".into(),
            bytes: scrubbed(
                "Login.yml",
                "info:\n  name: Login\n  type: http\nhttp:\n  url: https://changed\n",
            ),
        },
        FileChange::Put {
            path: "new/Fresh.yml".into(),
            bytes: scrubbed("new/Fresh.yml", "info:\n  name: Fresh\n  type: http\n"),
        },
        FileChange::Delete {
            path: "users/Get.yml".into(),
        },
    ];
    let mut failed = 0;
    for step in 0..40 {
        let rig = rig_with(Arc::new(FsFileTxFactory::with_fault(step)));
        seed(&rig);
        let before = snapshot(&rig.root);
        let result = rig.port.apply("c", &changes);
        let after = snapshot(&rig.root);
        match result {
            Err(_) => {
                failed += 1;
                assert_eq!(
                    after, before,
                    "a fault at step {step} left a half-applied sync"
                );
            }
            Ok(()) => {
                assert!(after.contains_key("new/Fresh.yml"), "step {step}");
                assert!(!after.contains_key("users/Get.yml"), "step {step}");
                assert!(String::from_utf8_lossy(&after["Login.yml"]).contains("https://changed"));
            }
        }
    }
    assert!(
        failed >= 3,
        "the fault hooks should fail early steps, failed {failed}"
    );
}

#[test]
fn apply_waits_for_the_collection_lock() {
    let rig = rig();
    seed(&rig);
    let lock = collection_lock(rig.repo.as_ref(), &rig.root);
    let guard = lock.lock();
    let done = Arc::new(AtomicBool::new(false));
    let worker = {
        let port = Arc::clone(&rig.port);
        let done = Arc::clone(&done);
        std::thread::spawn(move || {
            port.apply(
                "c",
                &[FileChange::Delete {
                    path: "users/Get.yml".into(),
                }],
            )
            .expect("applies");
            done.store(true, Ordering::SeqCst);
        })
    };
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        !done.load(Ordering::SeqCst),
        "apply ran while the collection lock was held"
    );
    assert!(rig.root.join("users/Get.yml").exists());
    drop(guard);
    worker.join().expect("join");
    assert!(done.load(Ordering::SeqCst));
    assert!(!rig.root.join("users/Get.yml").exists());
}

#[test]
fn collection_root_is_the_absolute_folder() {
    let rig = rig();
    seed(&rig);
    let root = rig.port.collection_root("c").expect("root");
    assert_eq!(
        std::fs::canonicalize(root).expect("canonical"),
        std::fs::canonicalize(&rig.root).expect("canonical")
    );
    assert!(rig.port.collection_root("missing").is_err());
}

#[test]
fn apply_put_over_a_case_variant_of_an_existing_path_is_refused() {
    let rig = rig();
    seed(&rig);
    let before = snapshot(&rig.root);
    let received = scrubbed("users/get.yml", GET);
    let err = rig
        .port
        .apply(
            "c",
            &[FileChange::Put {
                path: "Users/get.yml".into(),
                bytes: received.clone(),
            }],
        )
        .expect_err("refused");
    assert!(matches!(err, DomainError::AlreadyExists(_)));
    // Only the file name differs here.
    let err = rig
        .port
        .apply(
            "c",
            &[FileChange::Put {
                path: "users/get.yml".into(),
                bytes: received,
            }],
        )
        .expect_err("refused");
    assert!(matches!(err, DomainError::AlreadyExists(_)));
    assert!(!err.to_string().contains("get.yml"));
    assert_eq!(snapshot(&rig.root), before);
}

#[test]
fn apply_rename_to_a_case_variant_of_an_existing_path_is_refused() {
    let rig = rig();
    seed(&rig);
    let before = snapshot(&rig.root);
    let err = rig
        .port
        .apply(
            "c",
            &[FileChange::Rename {
                from: "Login.yml".into(),
                to: "USERS/get.yml".into(),
            }],
        )
        .expect_err("refused");
    assert!(matches!(err, DomainError::AlreadyExists(_)));
    assert_eq!(snapshot(&rig.root), before);
}

#[test]
fn apply_runs_renames_before_deletes() {
    let rig = rig();
    seed(&rig);
    // In the given order the delete would remove the file the rename wants to move.
    rig.port
        .apply(
            "c",
            &[
                FileChange::Delete {
                    path: "Login.yml".into(),
                },
                FileChange::Rename {
                    from: "Login.yml".into(),
                    to: "auth/Login.yml".into(),
                },
            ],
        )
        .expect("applies");
    assert!(!rig.root.join("Login.yml").exists());
    assert!(read(&rig, "auth/Login.yml").contains("LOCALSECRET"));
}

#[test]
fn apply_runs_deletes_before_puts() {
    let rig = rig();
    seed(&rig);
    let received = scrubbed("users/Get.yml", GET);
    // In the given order the delete would remove what the put wrote.
    rig.port
        .apply(
            "c",
            &[
                FileChange::Put {
                    path: "users/Get.yml".into(),
                    bytes: received.clone(),
                },
                FileChange::Delete {
                    path: "users/Get.yml".into(),
                },
            ],
        )
        .expect("applies");
    assert_eq!(
        std::fs::read(rig.root.join("users/Get.yml")).expect("read"),
        received
    );
}

#[test]
fn apply_put_after_a_rename_vacates_the_path_writes_a_new_file() {
    let rig = rig();
    seed(&rig);
    let received = scrubbed("Login.yml", "info:\n  name: Fresh\n  type: http\n");
    rig.port
        .apply(
            "c",
            &[
                FileChange::Put {
                    path: "Login.yml".into(),
                    bytes: received.clone(),
                },
                FileChange::Rename {
                    from: "Login.yml".into(),
                    to: "auth/Login.yml".into(),
                },
            ],
        )
        .expect("applies");
    assert_eq!(
        std::fs::read(rig.root.join("Login.yml")).expect("read"),
        received
    );
    assert!(read(&rig, "auth/Login.yml").contains("LOCALSECRET"));
}

#[test]
fn apply_if_unchanged_applies_when_the_manifest_matches() {
    let rig = rig();
    seed(&rig);
    let expected = rig.port.manifest("c").expect("manifest");
    // A change to a secret only is not a change to the manifest.
    write(
        &rig,
        "Login.yml",
        &LOGIN.replace("LOCALSECRET", "OTHERSECRET"),
    );
    rig.port
        .apply_if_unchanged(
            "c",
            &expected,
            &[FileChange::Delete {
                path: "users/Get.yml".into(),
            }],
        )
        .expect("applies");
    assert!(!rig.root.join("users/Get.yml").exists());
}

#[test]
fn apply_if_unchanged_refuses_when_a_local_save_landed_after_the_manifest() {
    let rig = rig();
    seed(&rig);
    let expected = rig.port.manifest("c").expect("manifest");
    write(
        &rig,
        "Login.yml",
        &LOGIN.replace("api.test/login", "api.test/signin"),
    );
    let before = snapshot(&rig.root);
    let err = rig
        .port
        .apply_if_unchanged(
            "c",
            &expected,
            &[FileChange::Delete {
                path: "users/Get.yml".into(),
            }],
        )
        .expect_err("conflict");
    assert!(matches!(err, DomainError::Conflict(_)));
    assert!(!err.to_string().contains("Login"));
    assert_eq!(snapshot(&rig.root), before);
}

#[test]
fn apply_if_unchanged_waits_for_the_lock_and_sees_a_save_made_while_it_waited() {
    let rig = rig();
    seed(&rig);
    let expected = rig.port.manifest("c").expect("manifest");
    let lock = collection_lock(rig.repo.as_ref(), &rig.root);
    let guard = lock.lock();
    let worker = {
        let port = Arc::clone(&rig.port);
        std::thread::spawn(move || {
            port.apply_if_unchanged(
                "c",
                &expected,
                &[FileChange::Delete {
                    path: "users/Get.yml".into(),
                }],
            )
        })
    };
    std::thread::sleep(Duration::from_millis(100));
    // A local save that holds the lock lands after the manifest was read.
    write(
        &rig,
        "users/Get.yml",
        &GET.replace("api.test/users", "api.test/people"),
    );
    drop(guard);
    let result = worker.join().expect("join");
    assert!(matches!(result, Err(DomainError::Conflict(_))));
    assert!(rig.root.join("users/Get.yml").exists());
}

/// A crashing factory whose recovery can be switched off, so the journal of a crashed commit
/// stays on disk until the test switches it on. Off, it reports that there is nothing to do.
struct Switchable {
    inner: FsFileTxFactory,
    recover_off: Arc<AtomicBool>,
}

impl CollectionFileTxFactory for Switchable {
    fn begin(&self, root: &Path) -> DomainResult<Box<dyn rocket_collection::CollectionFileTx>> {
        self.inner.begin(root)
    }
    fn recover(&self, root: &Path) -> DomainResult<rocket_collection::RecoveryOutcome> {
        if self.recover_off.load(Ordering::SeqCst) {
            return Ok(rocket_collection::RecoveryOutcome::Nothing);
        }
        self.inner.recover(root)
    }
}

fn crashing_rig(step: usize) -> (Rig, Arc<AtomicBool>) {
    let off = Arc::new(AtomicBool::new(true));
    let factory = Switchable {
        inner: FsFileTxFactory::with_crash(step),
        recover_off: Arc::clone(&off),
    };
    (rig_with(Arc::new(factory)), off)
}

fn journal(rig: &Rig) -> PathBuf {
    rig.root.join(".rocket").join("apply-journal.json")
}

fn crash_changes() -> Vec<FileChange> {
    vec![
        FileChange::Put {
            path: "new/Fresh.yml".into(),
            bytes: scrubbed("new/Fresh.yml", "info:\n  name: Fresh\n  type: http\n"),
        },
        FileChange::Delete {
            path: "users/Get.yml".into(),
        },
    ]
}

/// The crashed apply is all or nothing: both of its changes, or neither.
fn assert_settled(rig: &Rig, step: usize) {
    let fresh = rig.root.join("new/Fresh.yml").exists();
    let get_gone = !rig.root.join("users/Get.yml").exists();
    assert_eq!(fresh, get_gone, "step {step} left a half-applied sync");
}

#[test]
fn a_journal_left_behind_is_recovered_before_the_next_apply() {
    let mut with_journal = 0;
    for step in 0..40 {
        let (rig, recover_off) = crashing_rig(step);
        seed(&rig);
        if rig.port.apply("c", &crash_changes()).is_ok() {
            continue;
        }
        if !journal(&rig).exists() {
            continue;
        }
        with_journal += 1;
        recover_off.store(false, Ordering::SeqCst);
        rig.port
            .apply(
                "c",
                &[FileChange::Delete {
                    path: "Login.yml".into(),
                }],
            )
            .unwrap_or_else(|e| panic!("step {step}: {e}"));
        assert!(
            !journal(&rig).exists(),
            "step {step}: the journal is still there"
        );
        assert!(!rig.root.join("Login.yml").exists(), "step {step}");
        assert_settled(&rig, step);
    }
    assert!(
        with_journal >= 3,
        "the crash hooks should leave journals, got {with_journal}"
    );
}

#[test]
fn apply_if_unchanged_recovers_a_journal_before_it_compares() {
    let mut with_journal = 0;
    let mut rolled_forward = 0;
    let mut rolled_back = 0;
    for step in 0..40 {
        let (rig, recover_off) = crashing_rig(step);
        seed(&rig);
        let expected = rig.port.manifest("c").expect("manifest");
        if rig.port.apply("c", &crash_changes()).is_ok() || !journal(&rig).exists() {
            continue;
        }
        with_journal += 1;
        recover_off.store(false, Ordering::SeqCst);
        let result = rig.port.apply_if_unchanged(
            "c",
            &expected,
            &[FileChange::Delete {
                path: "Login.yml".into(),
            }],
        );
        assert!(
            !journal(&rig).exists(),
            "step {step}: the journal is still there"
        );
        assert_settled(&rig, step);
        if rig.root.join("new/Fresh.yml").exists() {
            // Rolled forward: the tree moved on, so the old manifest no longer holds.
            rolled_forward += 1;
            assert!(
                matches!(result, Err(DomainError::Conflict(_))),
                "step {step}: {result:?}"
            );
            assert!(rig.root.join("Login.yml").exists(), "step {step}");
        } else {
            // Rolled back: the tree is as it was, so the check passes and the change lands.
            rolled_back += 1;
            assert_eq!(result, Ok(()), "step {step}");
            assert!(!rig.root.join("Login.yml").exists(), "step {step}");
        }
    }
    assert!(with_journal >= 3, "got {with_journal}");
    assert!(rolled_forward + rolled_back == with_journal);
}

#[test]
fn the_manifest_and_export_files_share_one_filter_with_the_zip_base() {
    let rig = rig();
    seed(&rig);
    write(&rig, "Flows/x.yml", "info:\n  name: x\n  type: http\n");
    write(&rig, "Environments/e.yml", "name: e\nvariables: []\n");
    write(
        &rig,
        "users/.hidden/a.yml",
        "info:\n  name: a\n  type: http\n",
    );
    write(&rig, "Node_Modules/n.yml", "a: 1\n");
    let sent = rig.export.export_bruno("c", true).expect("exports").bytes;
    let base = base_from_reader(Cursor::new(sent)).expect("base");
    let manifest = rig.port.manifest("c").expect("manifest");
    assert_eq!(manifest, base.manifest);
    for bad in [
        "Flows/x.yml",
        "Environments/e.yml",
        "users/.hidden/a.yml",
        "Node_Modules/n.yml",
    ] {
        assert!(manifest.get(bad).is_none(), "{bad}");
        let err = rig
            .port
            .export_files("c", &[bad.to_string()])
            .expect_err("refused");
        assert!(matches!(err, DomainError::NotFound(_)), "{bad}");
    }
}

#[test]
fn errors_never_quote_the_paths_they_concern() {
    let rig = rig();
    seed(&rig);
    // A symlinked parent makes the existence check fail.
    let outside = TempDir::new().expect("tempdir");
    std::os::unix::fs::symlink(outside.path(), rig.root.join("linked")).expect("symlink");
    // A folder sits where a file is wanted.
    std::fs::create_dir_all(rig.root.join("FolderPathMarker.yml")).expect("mkdir");
    let body = b"a: 1\n".to_vec();
    let probes = [
        FileChange::PutAs {
            path: "linked/SymlinkPathMarker.yml".into(),
            bytes: body.clone(),
        },
        FileChange::Put {
            path: "linked/SymlinkPathMarker.yml".into(),
            bytes: body.clone(),
        },
        FileChange::Put {
            path: "FolderPathMarker.yml".into(),
            bytes: body.clone(),
        },
        FileChange::Rename {
            from: "Login.yml".into(),
            to: "linked/SymlinkPathMarker.yml".into(),
        },
    ];
    for probe in probes {
        let err = rig.port.apply("c", &[probe]).expect_err("refused");
        let text = err.to_string();
        assert!(!text.contains("PathMarker"), "{text}");
        assert!(!text.contains("linked"), "{text}");
    }
    // A failing commit.
    let changes = [
        FileChange::Put {
            path: "new/CommitPathMarker.yml".into(),
            bytes: scrubbed(
                "new/CommitPathMarker.yml",
                "info:\n  name: x\n  type: http\n",
            ),
        },
        FileChange::Put {
            path: "Login.yml".into(),
            bytes: scrubbed(
                "Login.yml",
                "info:\n  name: Login\n  type: http\nhttp:\n  url: https://changed\n",
            ),
        },
        FileChange::Delete {
            path: "users/Get.yml".into(),
        },
    ];
    let mut failed = 0;
    for step in 0..40 {
        let rig = rig_with(Arc::new(FsFileTxFactory::with_fault(step)));
        seed(&rig);
        if let Err(err) = rig.port.apply("c", &changes) {
            failed += 1;
            let text = err.to_string();
            assert!(
                !text.contains("PathMarker") && !text.contains(".rocket"),
                "{text}"
            );
            assert!(!text.contains("injected"), "{text}");
            assert!(matches!(err, DomainError::Internal(_)), "{err:?}");
        }
    }
    assert!(failed >= 3);
}

#[test]
fn an_unreadable_export_error_does_not_quote_the_file() {
    let rig = rig();
    seed(&rig);
    write(&rig, "users/BrokenPathMarker.yml", "a: [1, 2");
    let err = rig.port.manifest("c").expect_err("refused");
    assert!(!err.to_string().contains("PathMarker"), "{err}");
}

#[test]
fn apply_if_unchanged_with_no_changes_still_checks_the_manifest() {
    let rig = rig();
    seed(&rig);
    let expected = rig.port.manifest("c").expect("manifest");
    rig.port
        .apply_if_unchanged("c", &expected, &[])
        .expect("unchanged is fine");
    write(
        &rig,
        "Login.yml",
        &LOGIN.replace("api.test/login", "api.test/signin"),
    );
    let err = rig
        .port
        .apply_if_unchanged("c", &expected, &[])
        .expect_err("conflict");
    assert!(matches!(err, DomainError::Conflict(_)));
}

#[test]
fn a_missing_collection_error_does_not_quote_its_name() {
    let rig = rig();
    seed(&rig);
    let changes = [FileChange::Delete {
        path: "Login.yml".into(),
    }];
    let err = rig
        .port
        .apply("MissingMarker", &changes)
        .expect_err("refused");
    assert!(!err.to_string().contains("MissingMarker"), "{err}");
    let err = rig
        .port
        .collection_root("MissingMarker")
        .expect_err("refused");
    assert!(!err.to_string().contains("MissingMarker"), "{err}");
    let err = rig
        .port
        .collection_uid("MissingMarker")
        .expect_err("refused");
    assert!(!err.to_string().contains("MissingMarker"), "{err}");
}
