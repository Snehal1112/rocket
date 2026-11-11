//! The first linked send over the real stack on disk: the real export on the sender, the real
//! Bruno import on the receiver (as the transfer import adapter runs it) and the real
//! `ExportBackedSyncCollections` on both ends. A fresh link must read `in_sync` on both ends.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rocket_audit::publisher::RecordingSecurityAuditPublisher;
use rocket_collection::CollectionRepository;
use rocket_environment::{EnvironmentRepository, EnvironmentRepositoryFactory};
use rocket_import::ImportService;
use rocket_infra::{
    FsCollectionFileSource, FsCollectionRepo, FsEnvironmentRepo, FsFileTxFactory, FsImportFiles,
    SharedPathCollectionRepo,
};
use rocket_network::link::LinkState;
use tempfile::TempDir;

use super::sync::{LinkEnd, SyncDeps, SyncService};
use super::sync_collections::ExportBackedSyncCollections;
use super::sync_fakes::{dev, fast, lid, FakeLinks, RecordingSyncEvents, ANA, BO};
use super::sync_ports::SyncCollectionPort;
use super::transfer_fakes::FakePeers;
use super::transfer_ports::receiver_display_name;
use crate::collection_export_service::CollectionExportService;

const NAME: &str = "Orders";

struct Envs(PathBuf);

impl EnvironmentRepositoryFactory for Envs {
    fn for_collection(&self, collection: &str) -> Box<dyn EnvironmentRepository> {
        Box::new(FsEnvironmentRepo::new(
            self.0
                .join("collections")
                .join(collection)
                .join("environments"),
        ))
    }
}

/// One node: a workspace on disk, the real sync port and a sync service over fake links.
struct Node {
    dir: TempDir,
    export: Arc<CollectionExportService>,
    port: Arc<ExportBackedSyncCollections>,
    sync: Arc<SyncService>,
    links: Arc<FakeLinks>,
}

impl Node {
    fn new(peer: u8, peer_name: &str) -> Node {
        let dir = TempDir::new().expect("tempdir");
        let shared = Arc::new(Mutex::new(dir.path().to_path_buf()));
        let export = Arc::new(
            CollectionExportService::new(Box::new(FsCollectionFileSource::new(shared.clone())))
                .with_collections(Box::new(SharedPathCollectionRepo::new(shared.clone()))),
        );
        let port = Arc::new(ExportBackedSyncCollections::new(
            Arc::clone(&export),
            Arc::new(SharedPathCollectionRepo::new(shared)),
            Arc::new(FsFileTxFactory::new()),
        ));
        let peers = Arc::new(FakePeers::default());
        peers.add(dev(peer), peer_name);
        let links = Arc::new(FakeLinks::default());
        let sync = SyncService::new_with_timing(
            SyncDeps {
                network: peers,
                links: links.clone(),
                collections: port.clone(),
                events: Arc::new(RecordingSyncEvents::default()),
                audit: Arc::new(RecordingSecurityAuditPublisher::new()),
            },
            fast(),
        );
        Node {
            dir,
            export,
            port,
            sync,
            links,
        }
    }

    fn ws(&self) -> &Path {
        self.dir.path()
    }

    fn repo(&self) -> FsCollectionRepo {
        FsCollectionRepo::new_standalone(self.ws().join("collections"))
    }

    fn root_text(&self, collection: &str) -> String {
        std::fs::read_to_string(
            self.ws()
                .join("collections")
                .join(collection)
                .join("opencollection.yml"),
        )
        .expect("root file")
    }

    /// The single link's state as `links()` shows it.
    fn link_state(&self) -> LinkState {
        let views = self.sync.links().expect("links");
        assert_eq!(views.len(), 1, "one link");
        views[0].state
    }

    /// Imports the ZIP like the transfer import adapter: a display name with the sender,
    /// a ZIP named after it, the real Bruno import.
    fn import(&self, zip: &[u8]) -> String {
        let display = receiver_display_name(NAME, "Ana");
        let zips = TempDir::new().expect("tempdir");
        let path = zips.path().join(format!("{display}.zip"));
        std::fs::write(&path, zip).expect("write zip");
        let service = ImportService::new(
            self.ws().to_path_buf(),
            Box::new(self.repo()),
            Box::new(Envs(self.ws().to_path_buf())),
            Arc::new(FsImportFiles),
        );
        let report = service
            .import_auto_from_zip(&path, "default", false)
            .expect("imported");
        report
            .created_collections
            .into_iter()
            .next()
            .expect("one collection")
    }
}

/// The sender's collection: the root as `create` writes it plus `root_extra`, and three files.
fn seed_sender(node: &Node, root_extra: &str) {
    node.repo().create(NAME).expect("created");
    let root = node.ws().join("collections").join(NAME);
    let oc = root.join("opencollection.yml");
    let mut text = std::fs::read_to_string(&oc).expect("root");
    text.push_str(root_extra);
    std::fs::write(&oc, text).expect("root written");
    std::fs::create_dir_all(root.join("users")).expect("mkdir");
    std::fs::create_dir_all(root.join("orders")).expect("mkdir");
    std::fs::write(root.join("users/folder.yml"), "info:\n  name: users\n").expect("folder");
    std::fs::write(
        root.join("users/get.yml"),
        "info:\n  name: get\nhttp:\n  method: GET\n  url: https://api.example.test/users\n  headers:\n    - name: Authorization\n      value: Bearer A-TOKEN\nuid: a-uid\n",
    )
    .expect("get");
    std::fs::write(
        root.join("orders/list.yml"),
        "info:\n  name: list\nhttp:\n  method: GET\n  url: https://api.example.test/orders\n",
    )
    .expect("list");
}

const RICH_ROOT: &str = "docs: hello team\nconfig:\n  proxy:\n    disabled: true\nrequest:\n  auth:\n    type: bearer\n    token: SECRET\n  variables:\n    - name: base\n      value: https://x.test\n  scripts:\n    - type: before-request\n      code: console.log(1)\n";
const HEADERS_ROOT: &str = "request:\n  headers:\n    - name: X-Team\n      value: core\n";

/// Runs the first linked send of one case and returns both nodes and the received name.
fn first_send(root_extra: &str, receiver_has_the_name: bool) -> (Node, Node, String) {
    let ana = Node::new(BO, "Bo");
    let bo = Node::new(ANA, "Ana");
    seed_sender(&ana, root_extra);
    if receiver_has_the_name {
        bo.repo().create(NAME).expect("receiver's own collection");
    }
    let zip = ana.export.export_bruno(NAME, false).expect("export").bytes;

    ana.sync
        .link_first_transfer(lid(1), dev(BO), NAME, &zip, LinkEnd::Sender)
        .expect("sender linked");
    let received = bo.import(&zip);
    bo.sync
        .link_first_transfer(lid(1), dev(ANA), &received, &zip, LinkEnd::Receiver)
        .expect("receiver linked");
    (ana, bo, received)
}

fn assert_fresh_links_in_sync(root_extra: &str, receiver_has_the_name: bool) {
    let (ana, bo, received) = first_send(root_extra, receiver_has_the_name);
    let a = ana.links.record(&lid(1)).expect("sender link");
    let b = bo.links.record(&lid(1)).expect("receiver link");
    assert_eq!(a.base_hash(), b.base_hash(), "both ends share one base");
    assert_eq!(ana.link_state(), LinkState::InSync, "sender");
    assert_eq!(bo.link_state(), LinkState::InSync, "receiver");
    assert_eq!(
        bo.port.manifest(&received).expect("manifest"),
        b.base,
        "the receiver's export is the base"
    );
}

#[test]
fn a_plain_root_links_in_sync_on_both_ends() {
    assert_fresh_links_in_sync("", false);
}

#[test]
fn a_root_with_docs_and_config_links_in_sync_on_both_ends() {
    assert_fresh_links_in_sync(RICH_ROOT, false);
}

#[test]
fn a_root_with_only_request_headers_links_in_sync_on_both_ends() {
    assert_fresh_links_in_sync(HEADERS_ROOT, false);
}

#[test]
fn a_suffixed_import_links_in_sync_on_both_ends() {
    assert_fresh_links_in_sync("", true);
}

#[test]
fn a_suffixed_import_of_a_rich_root_links_in_sync_and_keeps_both_names() {
    let (ana, bo, received) = first_send(RICH_ROOT, true);
    assert_eq!(received, "Orders (from Ana)");
    assert_eq!(ana.link_state(), LinkState::InSync);
    assert_eq!(bo.link_state(), LinkState::InSync);
    // Each end keeps its own collection name and uid.
    let a: serde_yaml::Value = serde_yaml::from_str(&ana.root_text(NAME)).expect("yaml");
    let b: serde_yaml::Value = serde_yaml::from_str(&bo.root_text(&received)).expect("yaml");
    assert_eq!(a["info"]["name"].as_str(), Some(NAME));
    assert_eq!(b["info"]["name"].as_str(), Some("Orders (from Ana)"));
    let uid = bo.port.collection_uid(&received).expect("uid");
    assert!(uid.is_some(), "the receiver keeps its own uid");
    assert_ne!(uid, ana.port.collection_uid(NAME).expect("uid"));
    // The docs of the sent root landed.
    assert_eq!(b["docs"].as_str(), Some("hello team"));
}

#[test]
fn renaming_the_collection_on_one_end_is_not_a_change_to_sync() {
    let (ana, bo, received) = first_send(RICH_ROOT, false);
    let oc = ana
        .ws()
        .join("collections")
        .join(NAME)
        .join("opencollection.yml");
    let text = ana
        .root_text(NAME)
        .replace("name: Orders", "name: Orders Renamed");
    std::fs::write(oc, text).expect("renamed");
    assert_eq!(ana.link_state(), LinkState::InSync, "sender");
    assert_eq!(bo.link_state(), LinkState::InSync, "receiver");
    assert!(bo
        .root_text(&received)
        .contains(&format!("name: {received}\n")));
}
