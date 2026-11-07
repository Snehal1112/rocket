//! End-to-end send tests: two real nodes over real TLS on 127.0.0.1, a real
//! `CollectionExportService` over an in-memory file source and a real `FsTransferStaging`
//! in a temp folder.
//!
//! Only the import is a fake. The shipped adapter lives in `src-tauri` and `rocket-app`
//! must not depend on it. The fake runs the real strict ZIP validator first, so an unsafe
//! ZIP is refused the way the app refuses it.

use std::collections::HashMap;
#[allow(unused_imports)]
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use rocket_audit::AuditEventKind;
use rocket_export::{CollectionFileSource, SourceFile};
use rocket_infra::FsTransferStaging;
use rocket_network::limits::{SENDER_RESULT_TIMEOUT, TRANSFER_OFFER_TTL};
use rocket_network::{DeviceId, Direction, Outcome, TransferEvent};
use rocket_shared::error::{DomainError, DomainResult};
use tempfile::TempDir;

use super::e2e_tests::{online_pair, wait_until, E2eNode};
use super::{
    CollectionImportPort, ImportedCollection, Timing, TransferDeps, TransferService,
    TransferStaging, UNSAFE_ZIP_MESSAGE,
};
use crate::collection_export_service::CollectionExportService;

/// The production timings. Nothing in these tests waits for the idle limit.
pub(super) fn normal() -> Timing {
    Timing {
        offer_ttl: TRANSFER_OFFER_TTL,
        idle_limit: Duration::from_secs(120),
        result_timeout: SENDER_RESULT_TIMEOUT,
    }
}

/// Production timings except an offer that expires after 1 s.
pub(super) fn short_offer() -> Timing {
    Timing {
        offer_ttl: Duration::from_secs(1),
        ..normal()
    }
}

// ----- fakes -----

/// Collections held in memory, keyed by name.
pub(super) struct MemSource(HashMap<String, Vec<SourceFile>>);

impl CollectionFileSource for MemSource {
    fn list_files(&self, collection: &str) -> DomainResult<Vec<SourceFile>> {
        self.0
            .get(collection)
            .cloned()
            .ok_or_else(|| DomainError::NotFound(collection.to_string()))
    }
}

/// The import port of a receiver. It records what the service imported.
#[derive(Default)]
pub(super) struct RecordingImport {
    existing: Mutex<Vec<String>>,
    calls: Mutex<Vec<(String, Vec<u8>)>>,
    /// While true, `import_zip` waits after it counted its entry (a slow import).
    hold: AtomicBool,
    entered: AtomicUsize,
    /// The next import fails with this text, as an importer failure (not an unsafe ZIP).
    fail_next: Mutex<Option<String>>,
}

impl RecordingImport {
    fn with_existing(names: &[&str]) -> Arc<Self> {
        let import = RecordingImport::default();
        *import.existing() = names.iter().map(|n| n.to_string()).collect();
        Arc::new(import)
    }

    fn existing(&self) -> MutexGuard<'_, Vec<String>> {
        self.existing.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Makes every import wait (or stop waiting). The import runs on a blocking thread.
    pub(super) fn hold_imports(&self, on: bool) {
        self.hold.store(on, Ordering::SeqCst);
    }

    /// How many imports started.
    pub(super) fn entered(&self) -> usize {
        self.entered.load(Ordering::SeqCst)
    }

    /// Makes the next import fail with `text`.
    pub(super) fn fail_next(&self, text: &str) {
        *self.fail_next.lock().unwrap_or_else(|e| e.into_inner()) = Some(text.to_string());
    }

    /// The `(display name, ZIP bytes)` of every import, in order.
    fn calls(&self) -> Vec<(String, Vec<u8>)> {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl CollectionImportPort for RecordingImport {
    fn existing_names(&self) -> DomainResult<Vec<String>> {
        Ok(self.existing().clone())
    }

    fn import_zip(&self, zip_path: &Path, display_name: &str) -> DomainResult<ImportedCollection> {
        self.entered.fetch_add(1, Ordering::SeqCst);
        while self.hold.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(10));
        }
        // Same order and errors as the shipped adapter: a validator refusal is exactly
        // `InvalidInput(UNSAFE_ZIP_MESSAGE)`, any other failure is another variant.
        rocket_import::transfer_zip::validate(&rocket_infra::FsImportFiles, zip_path)
            .map_err(|_| DomainError::InvalidInput(UNSAFE_ZIP_MESSAGE.to_string()))?;
        if let Some(text) = self
            .fail_next
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            return Err(DomainError::Internal(text));
        }
        let bytes = std::fs::read(zip_path).map_err(|e| DomainError::Internal(e.to_string()))?;
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((display_name.to_string(), bytes));
        Ok(ImportedCollection {
            name: display_name.to_string(),
        })
    }
}

// ----- the harness -----

/// A node with a real transfer service on top of its real network service.
pub(super) struct Xfer {
    pub(super) node: E2eNode,
    pub(super) svc: Arc<TransferService>,
    pub(super) import: Arc<RecordingImport>,
    pub(super) staging_dir: TempDir,
}

impl Xfer {
    pub(super) fn id(&self) -> DeviceId {
        self.node.id()
    }

    pub(super) fn events(&self) -> Vec<TransferEvent> {
        self.node.events.transfers()
    }

    /// The `(direction, outcome)` of every `Finished` event, in order.
    pub(super) fn finished(&self) -> Vec<(Direction, Outcome)> {
        self.node.events.finished_outcomes()
    }

    pub(super) fn has_finished(&self, direction: Direction, outcome: Outcome) -> bool {
        self.finished().contains(&(direction, outcome))
    }

    /// The `message` of every `Finished` event, in order.
    pub(super) fn finished_messages(&self) -> Vec<Option<String>> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                TransferEvent::Finished { message, .. } => Some(message),
                _ => None,
            })
            .collect()
    }

    /// The `(transfer id, collection name, size)` of every incoming offer.
    pub(super) fn offers(&self) -> Vec<(String, String, u64)> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                TransferEvent::OfferIncoming {
                    transfer_id,
                    collection_name,
                    size_bytes,
                    ..
                } => Some((transfer_id, collection_name, size_bytes)),
                _ => None,
            })
            .collect()
    }

    /// The `(done, total)` of every progress event for one direction.
    pub(super) fn progress(&self, which: Direction) -> Vec<(u64, u64)> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                TransferEvent::Progress {
                    direction,
                    done_bytes,
                    total_bytes,
                    ..
                } if direction == which => Some((done_bytes, total_bytes)),
                _ => None,
            })
            .collect()
    }

    /// Files left in this node's staging folder.
    pub(super) fn staged_files(&self) -> usize {
        count_files(self.staging_dir.path())
    }
}

impl Drop for Xfer {
    /// A failed assert must not leave a held import spinning, or the runtime never stops.
    fn drop(&mut self) {
        self.import.hold_imports(false);
    }
}

pub(super) fn count_files(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| {
                    let path = entry.path();
                    if path.is_dir() {
                        count_files(&path)
                    } else {
                        1
                    }
                })
                .sum()
        })
        .unwrap_or(0)
}

/// Puts a transfer service on `node`. `sources` are the collections it can send, `existing`
/// are collection names the receiver already has. The wiring mirrors `src-tauri/src/lib.rs`:
/// purge the staging folder first, then build the service.
pub(super) fn attach(
    node: E2eNode,
    sources: Vec<(&str, Vec<SourceFile>)>,
    existing: &[&str],
    timing: Timing,
) -> Xfer {
    attach_in(
        node,
        sources,
        existing,
        timing,
        TempDir::new().expect("tempdir"),
    )
}

pub(super) fn attach_in(
    node: E2eNode,
    sources: Vec<(&str, Vec<SourceFile>)>,
    existing: &[&str],
    timing: Timing,
    staging_dir: TempDir,
) -> Xfer {
    let staging = Arc::new(FsTransferStaging::new(staging_dir.path().to_path_buf()));
    staging.purge().expect("purge at start");
    let map: HashMap<String, Vec<SourceFile>> = sources
        .into_iter()
        .map(|(name, files)| (name.to_string(), files))
        .collect();
    let export = Arc::new(CollectionExportService::new(Box::new(MemSource(map))));
    let import = RecordingImport::with_existing(existing);
    let svc = TransferService::new_with_timing(
        TransferDeps {
            network: node.svc.clone(),
            export,
            importer: import.clone(),
            staging,
            events: node.events.clone(),
            audit: node.audit.clone(),
        },
        timing,
    );
    Xfer {
        node,
        svc,
        import,
        staging_dir,
    }
}

// ----- fixtures -----

pub(super) fn file(path: &str, text: &str) -> SourceFile {
    SourceFile {
        rel_path: path.to_string(),
        bytes: text.as_bytes().to_vec(),
    }
}

/// A small collection with a folder and a request.
pub(super) fn orders(name: &str) -> Vec<SourceFile> {
    vec![
        file(
            "opencollection.yml",
            &format!("opencollection: \"1.0.0\"\ninfo:\n  name: \"{name}\"\n"),
        ),
        file("users/folder.yml", "info:\n  name: users\n"),
        file(
            "users/get.yml",
            "info:\n  name: get\nhttp:\n  method: GET\n  url: https://api.example.test/users\n",
        ),
    ]
}

/// The smallest collection that exports: one file.
pub(super) fn tiny(name: &str) -> Vec<SourceFile> {
    vec![file(
        "opencollection.yml",
        &format!("opencollection: \"1.0.0\"\ninfo:\n  name: \"{name}\"\n"),
    )]
}

/// Bytes a ZIP cannot shrink, so a fixture has a known minimum size.
pub(super) fn noise(len: usize) -> Vec<u8> {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8
        })
        .collect()
}

/// `orders` plus a script file of incompressible bytes, so the ZIP spans several chunks.
pub(super) fn bulky(name: &str, js_len: usize) -> Vec<SourceFile> {
    let mut files = orders(name);
    files.push(SourceFile {
        rel_path: "shared/blob.js".to_string(),
        bytes: noise(js_len),
    });
    files
}

/// Every entry of a ZIP as `(name, bytes)`.
pub(super) fn zip_entries(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("a valid zip");
    (0..archive.len())
        .map(|i| {
            let mut entry = archive.by_index(i).expect("entry");
            let mut data = Vec::new();
            entry.read_to_end(&mut data).expect("entry bytes");
            (entry.name().to_string(), data)
        })
        .collect()
}

/// Waits for the first offer on `receiver` and returns `(transfer id, name, size)`.
pub(super) async fn wait_offer(receiver: &Xfer) -> (String, String, u64) {
    wait_until("the receiver shows the offer", || {
        !receiver.offers().is_empty()
    })
    .await;
    receiver.offers().remove(0)
}

/// Sends, waits for the offer, accepts it and waits until both sides finished.
async fn push_and_accept(sender: &Xfer, receiver: &Xfer, name: &str) -> String {
    let id = sender
        .svc
        .send(&receiver.id(), name, false)
        .await
        .expect("the offer is sent")
        .transfer_id;
    let (offer_id, _, _) = wait_offer(receiver).await;
    assert_eq!(offer_id, id, "both sides use the sender's transfer id");
    receiver.svc.respond(&id, true).await.expect("accept");
    wait_until("both sides finished", || {
        !sender.finished().is_empty() && !receiver.finished().is_empty()
    })
    .await;
    id
}

// ----- the happy path -----

#[tokio::test]
async fn a_pushed_collection_is_imported_once_and_both_sides_see_it_finish() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Orders", orders("Orders"))], &[], normal());
    let b = attach(b, vec![], &[], normal());

    let id = push_and_accept(&a, &b, "Orders").await;

    assert!(a.has_finished(Direction::Send, Outcome::Completed));
    assert!(b.has_finished(Direction::Receive, Outcome::Completed));

    // The receiver imported once, under the offered name, from a real Bruno ZIP.
    let calls = b.import.calls();
    assert_eq!(calls.len(), 1, "imported exactly once");
    let (display, bytes) = &calls[0];
    assert_eq!(display, "Orders");
    let names: Vec<String> = zip_entries(bytes).into_iter().map(|(n, _)| n).collect();
    assert!(names.contains(&"Orders/opencollection.yml".to_string()));
    assert!(names.contains(&"Orders/users/get.yml".to_string()));
    let (_, offered_name, offered_size) = b.offers().remove(0);
    assert_eq!(offered_name, "Orders");
    assert_eq!(
        offered_size,
        bytes.len() as u64,
        "the offer states the exact size"
    );

    // Progress ended at the full size on both sides.
    assert_eq!(
        a.progress(Direction::Send).last().map(|p| p.0),
        Some(offered_size)
    );
    assert_eq!(
        b.progress(Direction::Receive).last().map(|p| p.0),
        Some(offered_size)
    );

    // The offer came before the end on the receiver.
    let kinds: Vec<&str> = b
        .events()
        .iter()
        .map(|e| match e {
            TransferEvent::OfferIncoming { .. } => "offer",
            TransferEvent::Progress { .. } => "progress",
            TransferEvent::Finished { .. } => "finished",
        })
        .collect();
    assert_eq!(kinds.first(), Some(&"offer"));
    assert_eq!(kinds.last(), Some(&"finished"));

    // Audit uses the short device id and never the payload.
    assert!(a
        .node
        .audited()
        .contains(&AuditEventKind::NetworkCollectionSent {
            device_id: b.id().short(),
            collection: "Orders".to_string(),
            outcome: "completed".to_string(),
        }));
    assert!(b
        .node
        .audited()
        .contains(&AuditEventKind::NetworkCollectionReceived {
            device_id: a.id().short(),
            collection: "Orders".to_string(),
            outcome: "completed".to_string(),
        }));
    let _ = id;

    // Nothing is left in staging.
    wait_until("staging is empty", || b.staged_files() == 0).await;
}

#[tokio::test]
async fn declining_imports_nothing_and_the_sender_sees_declined() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Orders", orders("Orders"))], &[], normal());
    let b = attach(b, vec![], &[], normal());

    let id = a
        .svc
        .send(&b.id(), "Orders", false)
        .await
        .expect("offer sent")
        .transfer_id;
    wait_offer(&b).await;
    b.svc.respond(&id, false).await.expect("decline");

    wait_until("the sender sees Declined", || {
        a.has_finished(Direction::Send, Outcome::Declined)
    })
    .await;
    assert!(b.import.calls().is_empty(), "nothing was imported");
    assert_eq!(b.staged_files(), 0);
    assert!(!a
        .node
        .audited()
        .iter()
        .any(|k| matches!(k, AuditEventKind::NetworkCollectionSent { .. })));
}

#[tokio::test]
async fn an_offer_nobody_answers_expires_on_both_sides_and_a_late_accept_fails() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Orders", orders("Orders"))], &[], short_offer());
    let b = attach(b, vec![], &[], short_offer());

    let id = a
        .svc
        .send(&b.id(), "Orders", false)
        .await
        .expect("offer sent")
        .transfer_id;
    wait_offer(&b).await;

    wait_until("both sides show Expired", || {
        a.has_finished(Direction::Send, Outcome::Expired)
            && b.has_finished(Direction::Receive, Outcome::Expired)
    })
    .await;
    assert!(
        b.svc.respond(&id, true).await.is_err(),
        "a late accept is refused"
    );
    assert!(b.import.calls().is_empty());
    assert_eq!(b.staged_files(), 0);
}

#[tokio::test]
async fn the_sender_cancels_a_pending_offer_and_the_receiver_closes_it() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Orders", orders("Orders"))], &[], normal());
    let b = attach(b, vec![], &[], normal());

    let id = a
        .svc
        .send(&b.id(), "Orders", false)
        .await
        .expect("offer sent")
        .transfer_id;
    wait_offer(&b).await;
    a.svc.cancel(&id).await.expect("the sender cancels");

    // The receiver gets `TransferAbort { cancelled }` for a pending offer and closes it.
    wait_until("the receiver ends Cancelled", || {
        b.has_finished(Direction::Receive, Outcome::Cancelled)
    })
    .await;
    assert!(
        b.svc.respond(&id, true).await.is_err(),
        "an accept after the cancel is refused"
    );
    assert!(b.import.calls().is_empty(), "nothing was imported");
    assert_eq!(b.staged_files(), 0);
    assert!(!a.has_finished(Direction::Send, Outcome::Completed));
}

#[tokio::test]
async fn the_receiver_cancels_a_pending_offer_and_the_sender_sees_declined() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Orders", orders("Orders"))], &[], normal());
    let b = attach(b, vec![], &[], normal());

    let id = a
        .svc
        .send(&b.id(), "Orders", false)
        .await
        .expect("offer sent")
        .transfer_id;
    wait_offer(&b).await;
    // Plan 03 ruling 11: the receiver's cancel of a pending offer is a decline.
    b.svc.cancel(&id).await.expect("the receiver cancels");

    wait_until("the sender sees Declined", || {
        a.has_finished(Direction::Send, Outcome::Declined)
    })
    .await;
    assert!(b.import.calls().is_empty(), "nothing was imported");
    assert_eq!(b.staged_files(), 0);
    assert!(!a
        .node
        .audited()
        .iter()
        .any(|k| matches!(k, AuditEventKind::NetworkCollectionSent { .. })));
}

#[tokio::test]
async fn a_name_clash_gets_the_from_sender_suffix() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Orders", orders("Orders"))], &[], normal());
    let b = attach(b, vec![], &["Orders"], normal());

    push_and_accept(&a, &b, "Orders").await;

    let calls = b.import.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].0, "Orders (from ann)",
        "ann is the sender's display name"
    );
    assert!(b.has_finished(Direction::Receive, Outcome::Completed));
}

#[tokio::test]
async fn sending_to_an_offline_peer_fails_before_any_offer() {
    let (a, b, _ha, hb) = online_pair().await;
    let a = attach(a, vec![("Orders", orders("Orders"))], &[], normal());
    let b = attach(b, vec![], &[], normal());

    hb.stop();
    drop(hb);
    wait_until("A shows B as not online", || {
        super::e2e_tests::row(&a.node, &b.id()) != Some(super::NodeStatus::Online)
    })
    .await;

    let err = a
        .svc
        .send(&b.id(), "Orders", false)
        .await
        .expect_err("the peer is offline");
    assert_eq!(
        err,
        DomainError::InvalidInput("That node is offline.".into())
    );
    // The online check runs before the export, so a collection that does not exist gives
    // the same error.
    let err = a
        .svc
        .send(&b.id(), "Nope", false)
        .await
        .expect_err("the peer is offline");
    assert_eq!(
        err,
        DomainError::InvalidInput("That node is offline.".into())
    );
    assert!(a.events().is_empty(), "no transfer was started");
    assert!(b.events().is_empty(), "no offer reached the peer");
    assert!(!a
        .node
        .audited()
        .iter()
        .any(|k| matches!(k, AuditEventKind::NetworkCollectionSent { .. })));
}

#[tokio::test]
async fn a_collection_with_no_files_is_refused_before_any_offer() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Empty", vec![])], &[], normal());
    let b = attach(b, vec![], &[], normal());

    let err = a
        .svc
        .send(&b.id(), "Empty", false)
        .await
        .expect_err("an empty collection is refused");
    assert!(
        matches!(err, DomainError::InvalidInput(_)),
        "unexpected error: {err}"
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(a.events().is_empty(), "no transfer was started");
    assert!(b.events().is_empty(), "no offer reached the peer");
}

#[tokio::test]
async fn the_sender_shows_sent_only_after_the_receivers_result() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Orders", orders("Orders"))], &[], normal());
    let b = attach(b, vec![], &[], normal());
    b.import.hold_imports(true);

    let id = a
        .svc
        .send(&b.id(), "Orders", false)
        .await
        .expect("offer sent")
        .transfer_id;
    wait_offer(&b).await;
    b.svc.respond(&id, true).await.expect("accept");
    wait_until("the receiver started the import", || {
        b.import.entered() == 1
    })
    .await;

    // Every chunk is out and the receiver is still importing, so it has sent no result. This
    // sleep only gives a wrong early "Sent" time to show up; the asserts hold either way.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        a.finished().is_empty(),
        "the sender must wait for the result"
    );
    assert!(!a
        .node
        .audited()
        .iter()
        .any(|k| matches!(k, AuditEventKind::NetworkCollectionSent { .. })));

    b.import.hold_imports(false);
    wait_until("both sides completed", || {
        a.has_finished(Direction::Send, Outcome::Completed)
            && b.has_finished(Direction::Receive, Outcome::Completed)
    })
    .await;
    assert!(a
        .node
        .audited()
        .contains(&AuditEventKind::NetworkCollectionSent {
            device_id: b.id().short(),
            collection: "Orders".to_string(),
            outcome: "completed".to_string(),
        }));
}

// ----- edge inputs -----

#[tokio::test]
async fn a_name_with_spaces_and_unicode_survives_the_trip() {
    let name = "Caf\u{e9} Orders \u{6771}\u{4eac} v2";
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![(name, orders(name))], &[], normal());
    let b = attach(b, vec![], &[], normal());

    push_and_accept(&a, &b, name).await;

    let (_, offered_name, _) = b.offers().remove(0);
    assert_eq!(offered_name, name);
    let calls = b.import.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].0, name,
        "the display name keeps spaces and unicode"
    );
    let root = format!("{name}/opencollection.yml");
    assert!(
        zip_entries(&calls[0].1).iter().any(|(n, _)| *n == root),
        "the ZIP root folder keeps the name"
    );
}

#[tokio::test]
async fn a_one_file_collection_travels_in_a_single_chunk() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Tiny", tiny("Tiny"))], &[], normal());
    let b = attach(b, vec![], &[], normal());

    push_and_accept(&a, &b, "Tiny").await;

    let calls = b.import.calls();
    assert_eq!(calls.len(), 1);
    assert!(
        calls[0].1.len() < rocket_network::limits::CHUNK_BYTES,
        "the whole ZIP fits one chunk"
    );
    assert_eq!(zip_entries(&calls[0].1).len(), 1);
    assert!(a.has_finished(Direction::Send, Outcome::Completed));
}

#[tokio::test]
async fn two_nodes_pushing_to_each_other_at_once_both_complete() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("FromAnn", orders("FromAnn"))], &[], normal());
    let b = attach(b, vec![("FromBen", orders("FromBen"))], &[], normal());

    let (a_id, b_id) = (a.id(), b.id());
    let (id_ab, id_ba) = tokio::join!(
        a.svc.send(&b_id, "FromAnn", false),
        b.svc.send(&a_id, "FromBen", false)
    );
    let id_ab = id_ab.expect("A's offer is sent").transfer_id;
    let id_ba = id_ba.expect("B's offer is sent").transfer_id;
    assert_ne!(id_ab, id_ba, "two transfers, two ids");

    wait_until("each node shows the other's offer", || {
        !a.offers().is_empty() && !b.offers().is_empty()
    })
    .await;
    b.svc.respond(&id_ab, true).await.expect("B accepts A's");
    a.svc.respond(&id_ba, true).await.expect("A accepts B's");

    wait_until("both directions completed on both nodes", || {
        a.has_finished(Direction::Send, Outcome::Completed)
            && a.has_finished(Direction::Receive, Outcome::Completed)
            && b.has_finished(Direction::Send, Outcome::Completed)
            && b.has_finished(Direction::Receive, Outcome::Completed)
    })
    .await;
    assert_eq!(b.import.calls().len(), 1);
    assert_eq!(a.import.calls().len(), 1);
    assert_eq!(b.import.calls()[0].0, "FromAnn");
    assert_eq!(a.import.calls()[0].0, "FromBen");
    wait_until("both staging folders are empty", || {
        a.staged_files() == 0 && b.staged_files() == 0
    })
    .await;
}
