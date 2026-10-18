//! Moves requests and folders through the real `FsCollectionRepo` and `FsFlowRepo`, and checks
//! that the flow files on disk follow.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use rocket_collection::{CollectionRepository, Request};
use rocket_flow::{Flow, FlowNodeKind, RequestSource};
use rocket_infra::{FsCollectionRepo, FsFlowRepo};
use rocket_shared::events::DomainEvent;
use rocket_shared::types::HttpMethod;
use tempfile::TempDir;

use crate::collection_service::CollectionService;
use crate::flow_references::test_support::flow_with;
use crate::flow_service::FlowService;
use crate::test_doubles::{RecordingPublisher, SharedPublisher};

/// Real repositories under one temp base directory. Collections `c` and `d` exist, and `c` holds
/// the requests `auth/login.yml` and `auth/sub/me.yml`.
fn world() -> (
    TempDir,
    CollectionService,
    Arc<FlowService>,
    Arc<RecordingPublisher>,
) {
    let dir = TempDir::new().expect("tempdir");
    let repo = FsCollectionRepo::new_standalone(dir.path().to_path_buf());
    for name in ["c", "d"] {
        repo.create(name).expect("create collection");
    }
    repo.create_folder("c", "auth").expect("create folder");
    repo.create_folder("c", "auth/sub").expect("create subfolder");
    for path in ["auth/login.yml", "auth/sub/me.yml"] {
        repo.save_request(
            "c",
            path,
            &Request::new("Req", HttpMethod::Get, "http://localhost".to_string()),
        )
        .expect("save request");
    }
    let flows = Arc::new(FlowService::new(Box::new(FsFlowRepo::new(
        dir.path().to_path_buf(),
    ))));
    let events = RecordingPublisher::new();
    let service = CollectionService::new(
        Box::new(FsCollectionRepo::new_standalone(dir.path().to_path_buf())),
        Box::new(SharedPublisher(events.clone())),
    )
    .with_flow_references(flows.clone());
    (dir, service, flows, events)
}

fn save(flows: &FlowService, collection: &str, name: &str, paths: &[&str]) {
    flows
        .save(collection, flow_with(name, paths))
        .expect("save flow");
}

fn flow_file(dir: &TempDir, collection: &str, name: &str) -> PathBuf {
    dir.path()
        .join(collection)
        .join("flows")
        .join(format!("{}.yml", name.to_lowercase()))
}

fn saved_paths(flow: &Flow) -> Vec<String> {
    flow.nodes
        .iter()
        .filter_map(|n| match &n.kind {
            FlowNodeKind::Request {
                source: RequestSource::Saved { request_path },
                ..
            } => Some(request_path.clone()),
            _ => None,
        })
        .collect()
}

fn rewrite_events(events: &RecordingPublisher) -> Vec<DomainEvent> {
    events
        .events()
        .into_iter()
        .filter(|e| matches!(e, DomainEvent::FlowRequestPathsRewritten { .. }))
        .collect()
}

#[test]
fn moving_a_request_rewrites_the_saved_flow_on_disk() {
    let (dir, service, flows, events) = world();
    save(&flows, "c", "Checkout", &["auth/login.yml"]);
    let file = flow_file(&dir, "c", "Checkout");
    let before = fs::read_to_string(&file).expect("read flow file");

    service
        .move_item("c", "auth/login.yml", "c", "identity/login.yml")
        .expect("move");

    let after = fs::read_to_string(&file).expect("read flow file");
    assert_ne!(before, after, "the flow file must change on disk");
    let flow = flows.get("c", "Checkout").expect("get flow");
    assert_eq!(saved_paths(&flow), vec!["identity/login.yml".to_string()]);
    rocket_flow::validate(&flow).expect("flow still valid");
    service
        .get_request("c", "identity/login.yml")
        .expect("the new path resolves to a saved request");
    match rewrite_events(&events).as_slice() {
        [DomainEvent::FlowRequestPathsRewritten { flows, failed, .. }] => {
            assert_eq!(flows, &vec!["Checkout".to_string()]);
            assert!(failed.is_empty());
        }
        other => panic!("expected one rewrite event, got {other:?}"),
    }
}

#[test]
fn renaming_a_folder_rewrites_every_flow_below_it() {
    let (_dir, service, flows, events) = world();
    save(&flows, "c", "A", &["auth/login.yml"]);
    save(&flows, "c", "B", &["auth/login.yml", "auth/sub/me.yml"]);

    service.move_item("c", "auth", "c", "identity").expect("move");

    assert_eq!(
        saved_paths(&flows.get("c", "A").expect("get A")),
        vec!["identity/login.yml".to_string()]
    );
    assert_eq!(
        saved_paths(&flows.get("c", "B").expect("get B")),
        vec![
            "identity/login.yml".to_string(),
            "identity/sub/me.yml".to_string()
        ]
    );
    match rewrite_events(&events).as_slice() {
        [DomainEvent::FlowRequestPathsRewritten { flows, .. }] => {
            let mut names = flows.clone();
            names.sort();
            assert_eq!(names, vec!["A".to_string(), "B".to_string()]);
        }
        other => panic!("expected one rewrite event, got {other:?}"),
    }
}

#[test]
fn a_corrupt_flow_file_is_left_untouched_and_the_move_succeeds() {
    let (dir, service, flows, _events) = world();
    save(&flows, "c", "Good", &["auth/login.yml"]);
    let broken = dir.path().join("c").join("flows").join("broken.yml");
    fs::write(&broken, b"nodes: [oops").expect("write broken flow");

    let result = service.move_item("c", "auth/login.yml", "c", "identity/login.yml");

    assert!(result.is_ok());
    assert_eq!(
        saved_paths(&flows.get("c", "Good").expect("get flow")),
        vec!["identity/login.yml".to_string()]
    );
    assert_eq!(
        fs::read(&broken).expect("read broken flow"),
        b"nodes: [oops".to_vec()
    );
}

#[test]
fn a_flow_of_another_collection_is_untouched() {
    let (dir, service, flows, _events) = world();
    save(&flows, "c", "Mine", &["auth/login.yml"]);
    save(&flows, "d", "Theirs", &["auth/login.yml"]);
    let file = flow_file(&dir, "d", "Theirs");
    let before = fs::read(&file).expect("read flow file");

    service
        .move_item("c", "auth/login.yml", "c", "identity/login.yml")
        .expect("move");

    assert_eq!(fs::read(&file).expect("read flow file"), before);
    assert_eq!(
        saved_paths(&flows.get("d", "Theirs").expect("get flow")),
        vec!["auth/login.yml".to_string()]
    );
}

#[test]
fn a_move_between_collections_reports_the_dangling_flow() {
    let (dir, service, flows, events) = world();
    save(&flows, "c", "Checkout", &["auth/login.yml"]);
    let file = flow_file(&dir, "c", "Checkout");
    let before = fs::read(&file).expect("read flow file");

    service
        .move_item("c", "auth/login.yml", "d", "login.yml")
        .expect("move");

    assert_eq!(fs::read(&file).expect("read flow file"), before);
    match rewrite_events(&events).as_slice() {
        [DomainEvent::FlowRequestPathsRewritten { flows, failed, .. }] => {
            assert!(flows.is_empty());
            assert_eq!(failed.len(), 1);
            assert_eq!(failed[0].flow, "Checkout");
            assert!(failed[0].message.contains("moved out of the collection"));
        }
        other => panic!("expected one rewrite event, got {other:?}"),
    }
}
