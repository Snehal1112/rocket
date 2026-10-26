//! The collection lock is shared by editor saves and the sync apply.

use crate::collection_lock::collection_lock;
use crate::collection_service::CollectionService;
use crate::test_doubles::{RecordingPublisher, SharedPublisher};
use rocket_collection::{CollectionFileTxFactory, CollectionRepository, Request};
use rocket_infra::{FsCollectionRepo, FsFileTxFactory};
use rocket_shared::events::{DomainEvent, EventPublisher};
use rocket_shared::types::HttpMethod;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;

const WAIT: Duration = Duration::from_millis(300);
const LONG: Duration = Duration::from_secs(10);

fn request(name: &str) -> Request {
    Request::new(name, HttpMethod::Get, "http://one".to_string())
}

fn repo(dir: &TempDir) -> FsCollectionRepo {
    FsCollectionRepo::new_standalone(dir.path().to_path_buf())
}

/// A collection `c` with one saved request, and its root.
fn world() -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("tempdir");
    let r = repo(&dir);
    r.create("c").expect("create");
    r.save_request("c", "Login.yml", &request("Login"))
        .expect("seed");
    let root = r.collection_root_path("c").expect("root");
    (dir, root)
}

/// Stops the first `RequestSaved` until the test lets it go, so the save is held inside the
/// lock. Events are published while the lock is held.
struct Gate {
    inside: Mutex<Sender<()>>,
    release: Mutex<Receiver<()>>,
}

impl EventPublisher for Gate {
    fn publish(&self, event: DomainEvent) {
        if matches!(event, DomainEvent::RequestSaved { .. }) {
            self.inside.lock().expect("lock").send(()).expect("inside");
            let _ = self.release.lock().expect("lock").recv_timeout(LONG);
        }
    }
}

fn plain_service(dir: &TempDir) -> CollectionService {
    CollectionService::new(
        Box::new(repo(dir)),
        Box::new(SharedPublisher(RecordingPublisher::new())),
    )
}

#[test]
fn editor_save_waits_for_a_running_apply_and_the_reverse() {
    let (dir, root) = world();
    let svc = Arc::new(plain_service(&dir));

    // Forward: an apply holds the lock, the editor save waits and then lands.
    let lock = collection_lock(&root);
    let guard = lock.lock().expect("apply lock");
    let (done_tx, done_rx) = channel();
    let saver = {
        let svc = Arc::clone(&svc);
        std::thread::spawn(move || {
            svc.save_request("c", "Edited.yml", &request("Edited"))
                .expect("save");
            done_tx.send(()).expect("done");
        })
    };
    assert!(
        done_rx.recv_timeout(WAIT).is_err(),
        "the save must wait for the apply"
    );
    // The apply writes its own file while the save waits.
    repo(&dir)
        .save_request("c", "Applied.yml", &request("Applied"))
        .expect("apply write");
    drop(guard);
    done_rx.recv_timeout(LONG).expect("the save finishes");
    saver.join().expect("join");
    let names: Vec<String> = ["Login.yml", "Edited.yml", "Applied.yml"]
        .iter()
        .map(|p| repo(&dir).get_request("c", p).expect("kept").name)
        .collect();
    assert_eq!(names, ["Login", "Edited", "Applied"]);

    // Reverse: a save is running inside the lock, so an apply cannot take it.
    let (inside_tx, inside_rx) = channel();
    let (release_tx, release_rx) = channel();
    let held = Arc::new(CollectionService::new(
        Box::new(repo(&dir)),
        Box::new(Gate {
            inside: Mutex::new(inside_tx),
            release: Mutex::new(release_rx),
        }),
    ));
    let saver = {
        let held = Arc::clone(&held);
        std::thread::spawn(move || {
            held.save_request("c", "Later.yml", &request("Later"))
                .expect("save");
        })
    };
    inside_rx.recv_timeout(LONG).expect("the save is running");
    assert!(
        lock.try_lock().is_err(),
        "an apply must wait for a running save"
    );
    release_tx.send(()).expect("release");
    saver.join().expect("join");
    assert!(lock.try_lock().is_ok(), "the lock is free again");
}

#[test]
fn lock_order_is_collection_lock_then_tx_guard_everywhere() {
    let (dir, root) = world();
    let svc = Arc::new(plain_service(&dir));
    let factory = Arc::new(FsFileTxFactory::new());

    // The apply order: collection lock first, then the transaction's own guard.
    let lock = collection_lock(&root);
    let guard = lock.lock().expect("apply lock");
    let saver = {
        let svc = Arc::clone(&svc);
        std::thread::spawn(move || {
            svc.save_request("c", "Edited.yml", &request("Edited"))
                .expect("save");
        })
    };
    let tx = factory.begin(&root).expect("tx guard under the lock");
    drop(tx);
    drop(guard);
    saver.join().expect("the save finishes");
    // The apply never calls back into the service, so the lock is free once it ends.
    assert!(lock.try_lock().is_ok());
}

#[test]
fn a_method_that_calls_another_locked_method_does_not_deadlock() {
    let (dir, root) = world();
    let svc = Arc::new(plain_service(&dir));
    let (tx, rx) = channel();
    let worker = {
        let svc = Arc::clone(&svc);
        std::thread::spawn(move || {
            // Each of these calls another locked method inside.
            let copy = svc.duplicate_request("c", "Login.yml").expect("duplicate");
            svc.create_request("c", "Fresh", &request("Fresh"))
                .expect("create");
            svc.move_item("c", "Login.yml", "c", "Moved.yml")
                .expect("move");
            svc.rename_request("c", "Moved.yml", "Renamed")
                .expect("rename");
            tx.send(copy.name).expect("done");
        })
    };
    let name = rx.recv_timeout(LONG).expect("no deadlock");
    worker.join().expect("join");
    assert_eq!(name, "Login copy");
    assert!(collection_lock(&root).try_lock().is_ok());
}

#[test]
fn a_move_between_two_collections_locks_both_without_deadlock() {
    let dir = TempDir::new().expect("tempdir");
    let r = repo(&dir);
    r.create("a").expect("a");
    r.create("b").expect("b");
    r.save_request("a", "One.yml", &request("One"))
        .expect("one");
    r.save_request("b", "Two.yml", &request("Two"))
        .expect("two");
    let svc = Arc::new(plain_service(&dir));
    let (tx, rx) = channel();
    let mut workers = Vec::new();
    for (from, to, file, dest) in [
        ("a", "b", "One.yml", "One2.yml"),
        ("b", "a", "Two.yml", "Two2.yml"),
    ] {
        let svc = Arc::clone(&svc);
        let tx = tx.clone();
        workers.push(std::thread::spawn(move || {
            svc.move_item(from, file, to, dest).expect("move");
            tx.send(()).expect("done");
        }));
    }
    for _ in 0..2 {
        rx.recv_timeout(LONG).expect("no deadlock");
    }
    for w in workers {
        w.join().expect("join");
    }
}
