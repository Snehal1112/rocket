//! Duplicates requests through the real `FsCollectionRepo` and checks the files on disk.

use std::fs;
use std::sync::Arc;

use rocket_collection::{CollectionItem, CollectionRepository, Request};
use rocket_infra::FsCollectionRepo;
use rocket_shared::events::DomainEvent;
use rocket_shared::types::HttpMethod;
use tempfile::TempDir;

use crate::collection_service::CollectionService;
use crate::test_doubles::{RecordingPublisher, SharedPublisher};

fn request(name: &str, url: &str) -> Request {
    Request::new(name, HttpMethod::Get, url.to_string())
}

/// Collection `c` holds `auth/Login.yml` (url `http://one`) and `other/Login.yml` (url
/// `http://two`), both with the display name `Login`.
fn world() -> (TempDir, CollectionService, Arc<RecordingPublisher>) {
    let dir = TempDir::new().expect("tempdir");
    let repo = FsCollectionRepo::new_standalone(dir.path().to_path_buf());
    repo.create("c").expect("create collection");
    for folder in ["auth", "other"] {
        repo.create_folder("c", folder).expect("create folder");
    }
    repo.save_request("c", "auth/Login.yml", &request("Login", "http://one"))
        .expect("save one");
    repo.save_request("c", "other/Login.yml", &request("Login", "http://two"))
        .expect("save two");
    let events = RecordingPublisher::new();
    let service = CollectionService::new(
        Box::new(FsCollectionRepo::new_standalone(dir.path().to_path_buf())),
        Box::new(SharedPublisher(events.clone())),
    );
    (dir, service, events)
}

fn saved_paths(events: &RecordingPublisher) -> Vec<String> {
    events
        .events()
        .into_iter()
        .filter_map(|e| match e {
            DomainEvent::RequestSaved { path, .. } => Some(path),
            _ => None,
        })
        .collect()
}

fn read_url(dir: &TempDir, rel: &str) -> String {
    let text = fs::read_to_string(dir.path().join("c").join(rel)).expect("read file");
    text.lines()
        .find_map(|l| l.trim().strip_prefix("url:").map(|u| u.trim().to_string()))
        .expect("url line")
}

#[test]
fn duplicating_a_request_in_a_folder_stays_in_that_folder() {
    let (dir, service, _events) = world();
    service
        .duplicate_request("c", "auth/Login.yml")
        .expect("duplicate");
    assert!(dir.path().join("c/auth/Login copy.yml").is_file());
    assert!(!dir.path().join("c/Login copy.yml").exists());
}

#[test]
fn duplicating_twice_makes_copy_then_copy_2() {
    let (dir, service, _events) = world();
    let first = service
        .duplicate_request("c", "auth/Login.yml")
        .expect("first");
    let second = service
        .duplicate_request("c", "auth/Login.yml")
        .expect("second");
    assert_eq!(first.name, "Login copy");
    assert_eq!(second.name, "Login copy 2");
    assert!(dir.path().join("c/auth/Login copy.yml").is_file());
    assert!(dir.path().join("c/auth/Login copy 2.yml").is_file());
}

#[test]
fn duplicating_never_overwrites_an_existing_copy() {
    let (dir, service, _events) = world();
    let repo = FsCollectionRepo::new_standalone(dir.path().to_path_buf());
    repo.save_request("c", "auth/Login copy.yml", &request("Keep", "http://keep"))
        .expect("seed copy");
    service
        .duplicate_request("c", "auth/Login.yml")
        .expect("duplicate");
    assert_eq!(read_url(&dir, "auth/Login copy.yml"), "http://keep");
    assert!(dir.path().join("c/auth/Login copy 2.yml").is_file());
}

#[test]
fn duplicating_uses_the_full_path_not_the_display_name() {
    let (dir, service, _events) = world();
    let copy = service
        .duplicate_request("c", "other/Login.yml")
        .expect("duplicate");
    assert_eq!(copy.url, "http://two");
    assert!(dir.path().join("c/other/Login copy.yml").is_file());
    assert!(!dir.path().join("c/auth/Login copy.yml").exists());
}

#[test]
fn a_case_variant_counts_as_taken() {
    let (dir, service, _events) = world();
    let repo = FsCollectionRepo::new_standalone(dir.path().to_path_buf());
    repo.save_request("c", "auth/login copy.yml", &request("x", "http://keep"))
        .expect("seed variant");
    service
        .duplicate_request("c", "auth/Login.yml")
        .expect("duplicate");
    assert!(dir.path().join("c/auth/Login copy 2.yml").is_file());
    assert!(
        !dir.path().join("c/auth/Login copy.yml").is_file()
            || dir.path().join("c/auth/login copy.yml").is_file()
    );
}

#[test]
fn the_copy_has_its_own_uid_and_is_listed_in_the_tree() {
    let (_dir, service, events) = world();
    let source = service.get_request("c", "auth/Login.yml").expect("source");
    let copy = service
        .duplicate_request("c", "auth/Login.yml")
        .expect("duplicate");
    assert_ne!(copy.uid, source.uid);
    let collection = service.get("c").expect("collection");
    let count = collection
        .root
        .items
        .iter()
        .find_map(|item| match item {
            CollectionItem::Folder(f) if f.name == "auth" => Some(
                f.items
                    .iter()
                    .filter(|i| matches!(i, CollectionItem::Request(_)))
                    .count(),
            ),
            _ => None,
        })
        .expect("auth folder");
    assert_eq!(count, 2);
    assert!(saved_paths(&events).contains(&"auth/Login copy.yml".to_string()));
}

#[test]
fn duplicating_a_request_in_the_root_has_no_leading_slash() {
    let (dir, service, events) = world();
    let repo = FsCollectionRepo::new_standalone(dir.path().to_path_buf());
    repo.save_request("c", "Solo.yml", &request("Solo", "http://solo"))
        .expect("seed solo");
    service
        .duplicate_request("c", "Solo.yml")
        .expect("duplicate");
    assert!(dir.path().join("c/Solo copy.yml").is_file());
    assert!(saved_paths(&events).contains(&"Solo copy.yml".to_string()));
}

#[test]
fn two_duplicates_from_two_threads_get_different_files() {
    let (dir, service, events) = world();
    let service = Arc::new(service);
    let results: Vec<_> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let service = service.clone();
                scope.spawn(move || service.duplicate_request("c", "auth/Login.yml"))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("thread"))
            .collect()
    });
    for r in &results {
        assert!(r.is_ok(), "duplicate failed: {r:?}");
    }
    let mut paths = saved_paths(&events);
    paths.retain(|p| p.contains("copy"));
    paths.sort();
    paths.dedup();
    assert_eq!(paths.len(), 2);
    for p in &paths {
        assert!(dir.path().join("c").join(p).is_file());
    }
}
