//! End-to-end send tests: two real nodes over real TLS on 127.0.0.1, a real
//! `CollectionExportService` over an in-memory file source and a real `FsTransferStaging`
//! in a temp folder.
//!
//! Only the import is a fake. The shipped adapter lives in `src-tauri` and `rocket-app`
//! must not depend on it. The fake runs the real strict ZIP validator first, so an unsafe
//! ZIP is refused the way the app refuses it.

use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use rocket_audit::AuditEventKind;
use rocket_export::{CollectionFileSource, SourceFile};
use rocket_infra::FsTransferStaging;
use rocket_network::limits::{
    CHUNK_BYTES, MAX_PENDING_INCOMING, MAX_TRANSFER_BYTES, MAX_UNACKED_CHUNKS,
    SENDER_RESULT_TIMEOUT, TRANSFER_OFFER_TTL,
};
use rocket_network::{DeviceId, Direction, Outcome, TransferEvent};
use rocket_shared::error::{DomainError, DomainResult};
use tempfile::TempDir;

use super::e2e_tests::{e2e_node, online_pair, pair, row, wait_until, E2eNode};
use super::{
    CollectionImportPort, ImportedCollection, NetworkHandle, NodeStatus, StagedFile, StagedZip,
    Timing, TransferDeps, TransferService, TransferStaging, UNSAFE_ZIP_MESSAGE,
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
        .send(&receiver.id(), name, false, false)
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

    push_and_accept(&a, &b, "Orders").await;

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

    // Audit uses the short device id and never the payload. Each side writes exactly one row.
    let sent_rows = |x: &Xfer| {
        x.node
            .audited()
            .iter()
            .filter(|k| matches!(k, AuditEventKind::NetworkCollectionSent { .. }))
            .count()
    };
    let received_rows = |x: &Xfer| {
        x.node
            .audited()
            .iter()
            .filter(|k| matches!(k, AuditEventKind::NetworkCollectionReceived { .. }))
            .count()
    };
    assert_eq!((sent_rows(&a), received_rows(&a)), (1, 0));
    assert_eq!((sent_rows(&b), received_rows(&b)), (0, 1));
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
        .send(&b.id(), "Orders", false, false)
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
    // The receiver's own card ends Declined, and a decline is not audited there.
    assert!(b.has_finished(Direction::Receive, Outcome::Declined));
    assert!(
        received_outcomes(&b).is_empty(),
        "a decline writes no audit row"
    );
    assert!(sent_outcomes(&b).is_empty());
}

#[tokio::test]
async fn an_offer_nobody_answers_expires_on_both_sides_and_a_late_accept_fails() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Orders", orders("Orders"))], &[], short_offer());
    let b = attach(b, vec![], &[], short_offer());

    let id = a
        .svc
        .send(&b.id(), "Orders", false, false)
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
        .send(&b.id(), "Orders", false, false)
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

    // The sender ends Cancelled, and no byte left, so it audits nothing.
    wait_until("the sender ends Cancelled", || {
        a.has_finished(Direction::Send, Outcome::Cancelled)
    })
    .await;
    assert!(!a.has_finished(Direction::Send, Outcome::Completed));
    assert!(
        sent_outcomes(&a).is_empty(),
        "no chunk was sent, so no audit row"
    );
    // Spec section 10: the receiver shows "<name> cancelled the transfer." The frontend builds
    // that text from `display_name`, so the event must carry the sender's name.
    let cancelled: Vec<TransferEvent> = b
        .events()
        .into_iter()
        .filter(|e| {
            matches!(
                e,
                TransferEvent::Finished {
                    outcome: Outcome::Cancelled,
                    ..
                }
            )
        })
        .collect();
    assert_eq!(cancelled.len(), 1, "one Finished for the receiver");
    assert!(matches!(
        &cancelled[0],
        TransferEvent::Finished { direction: Direction::Receive, display_name, message: None, .. }
            if display_name == "ann"
    ));
}

#[tokio::test]
async fn the_receiver_cancels_a_pending_offer_and_the_sender_sees_declined() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Orders", orders("Orders"))], &[], normal());
    let b = attach(b, vec![], &[], normal());

    let id = a
        .svc
        .send(&b.id(), "Orders", false, false)
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
    assert!(b
        .node
        .audited()
        .contains(&AuditEventKind::NetworkCollectionReceived {
            device_id: a.id().short(),
            collection: "Orders (from ann)".to_string(),
            outcome: "completed".to_string(),
        }));
}

#[tokio::test]
async fn sending_to_an_offline_peer_fails_before_any_offer() {
    let (a, b, _ha, hb) = online_pair().await;
    let a = attach(a, vec![("Orders", orders("Orders"))], &[], normal());
    let b = attach(b, vec![], &[], normal());

    hb.stop();
    drop(hb);
    wait_until("A shows B as not online", || {
        row(&a.node, &b.id()) != Some(NodeStatus::Online)
    })
    .await;

    let err = a
        .svc
        .send(&b.id(), "Orders", false, false)
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
        .send(&b.id(), "Nope", false, false)
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
        .send(&b.id(), "Empty", false, false)
        .await
        .expect_err("an empty collection is refused");
    assert_eq!(
        err,
        DomainError::InvalidInput("There is nothing to send in that collection.".into())
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
        .send(&b.id(), "Orders", false, false)
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
    // One chunk gives exactly one progress event on each side, and it is the full size.
    let size = calls[0].1.len() as u64;
    assert_eq!(a.progress(Direction::Send), vec![(size, size)]);
    assert_eq!(b.progress(Direction::Receive), vec![(size, size)]);
}

#[tokio::test]
async fn two_nodes_pushing_to_each_other_at_once_both_complete() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("FromAnn", orders("FromAnn"))], &[], normal());
    let b = attach(b, vec![("FromBen", orders("FromBen"))], &[], normal());

    let (a_id, b_id) = (a.id(), b.id());
    let (id_ab, id_ba) = tokio::join!(
        a.svc.send(&b_id, "FromAnn", false, false),
        b.svc.send(&a_id, "FromBen", false, false)
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

// ----- more fixtures -----

/// The outcomes of every sender audit row on a node, in order.
fn sent_outcomes(x: &Xfer) -> Vec<String> {
    x.node
        .audited()
        .into_iter()
        .filter_map(|k| match k {
            AuditEventKind::NetworkCollectionSent { outcome, .. } => Some(outcome),
            _ => None,
        })
        .collect()
}

/// The outcomes of every receiver audit row on a node, in order.
fn received_outcomes(x: &Xfer) -> Vec<String> {
    x.node
        .audited()
        .into_iter()
        .filter_map(|k| match k {
            AuditEventKind::NetworkCollectionReceived { outcome, .. } => Some(outcome),
            _ => None,
        })
        .collect()
}

/// A ZIP with one entry that escapes its folder.
fn bad_zip() -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("../evil.yml", zip::write::SimpleFileOptions::default())
        .expect("start entry");
    writer
        .write_all(b"info:\n  name: evil\n")
        .expect("write entry");
    let bytes = writer.finish().expect("finish").into_inner();
    let archive = zip::ZipArchive::new(Cursor::new(bytes.clone())).expect("reads back");
    assert_eq!(
        archive.file_names().next(),
        Some("../evil.yml"),
        "the writer kept the unsafe name"
    );
    bytes
}

/// A fresh started node, linked to and paired with `hub`, with `hub` as the initiator.
async fn join_peer(hub: &E2eNode, name: &str) -> (E2eNode, NetworkHandle) {
    let node = e2e_node(name);
    let handle = node.svc.start().await;
    node.svc
        .add_address("127.0.0.1", hub.port())
        .await
        .expect("peer saves the hub address");
    hub.svc
        .add_address("127.0.0.1", node.port())
        .await
        .expect("hub saves the peer address");
    wait_until("each lists the other", || {
        row(&node, &hub.id()).is_some() && row(hub, &node.id()).is_some()
    })
    .await;
    pair(hub, &node).await;
    wait_until("both rows show Online", || {
        row(&node, &hub.id()) == Some(NodeStatus::Online)
            && row(hub, &node.id()) == Some(NodeStatus::Online)
    })
    .await;
    (node, handle)
}

/// A slow receiver disk: every append sleeps first, then the real staging writes.
pub(super) struct SlowStaging {
    inner: FsTransferStaging,
    delay: Duration,
}

struct SlowZip {
    inner: Box<dyn StagedZip>,
    delay: Duration,
}

impl StagedZip for SlowZip {
    fn append(&mut self, bytes: &[u8]) -> DomainResult<()> {
        std::thread::sleep(self.delay);
        self.inner.append(bytes)
    }

    fn finish(self: Box<Self>) -> DomainResult<StagedFile> {
        self.inner.finish()
    }

    fn discard(self: Box<Self>) {
        self.inner.discard()
    }
}

impl TransferStaging for SlowStaging {
    fn create(&self, transfer_id: &str) -> DomainResult<Box<dyn StagedZip>> {
        Ok(Box::new(SlowZip {
            inner: self.inner.create(transfer_id)?,
            delay: self.delay,
        }))
    }

    fn purge(&self) -> DomainResult<()> {
        self.inner.purge()
    }

    fn release(&self, file: &StagedFile) -> DomainResult<()> {
        self.inner.release(file)
    }
}

/// A receiving node whose staging is slow. It has nothing to send.
fn attach_slow(node: E2eNode, delay: Duration) -> Xfer {
    let staging_dir = TempDir::new().expect("tempdir");
    let staging = Arc::new(SlowStaging {
        inner: FsTransferStaging::new(staging_dir.path().to_path_buf()),
        delay,
    });
    staging.purge().expect("purge at start");
    let export = Arc::new(CollectionExportService::new(Box::new(MemSource(
        HashMap::new(),
    ))));
    let import = RecordingImport::with_existing(&[]);
    let svc = TransferService::new_with_timing(
        TransferDeps {
            network: node.svc.clone(),
            export,
            importer: import.clone(),
            staging,
            events: node.events.clone(),
            audit: node.audit.clone(),
        },
        normal(),
    );
    Xfer {
        node,
        svc,
        import,
        staging_dir,
    }
}

const SLOW_CHUNK: Duration = Duration::from_millis(40);

/// True once the receiver got some bytes but not all of them.
fn mid_transfer(receiver: &Xfer) -> bool {
    receiver
        .progress(Direction::Receive)
        .iter()
        .any(|(done, total)| *done > 0 && done < total)
}

// ----- dropped connection -----

#[tokio::test]
async fn a_dropped_sender_fails_the_receiver_and_clears_staging() {
    let (a, b, ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Big", bulky("Big", 600 * 1024))], &[], normal());
    let b = attach(b, vec![], &[], normal());
    a.svc.set_chunk_delay(SLOW_CHUNK);

    let id = a
        .svc
        .send(&b.id(), "Big", false, false)
        .await
        .expect("offer sent")
        .transfer_id;
    wait_offer(&b).await;
    b.svc.respond(&id, true).await.expect("accept");
    wait_until("the receiver got some chunks", || mid_transfer(&b)).await;

    ha.stop();
    drop(ha);

    wait_until("the receiver ends Failed", || {
        b.has_finished(Direction::Receive, Outcome::Failed)
    })
    .await;
    assert!(
        b.import.calls().is_empty(),
        "a partial transfer is never imported"
    );
    assert!(
        received_outcomes(&b).is_empty(),
        "a failed receive is not audited"
    );
    wait_until("the staged file is deleted", || b.staged_files() == 0).await;
}

#[tokio::test]
async fn a_dropped_receiver_fails_the_sender_and_a_restart_leaves_no_staged_files() {
    let (a, b, _ha, hb) = online_pair().await;
    let a = attach(a, vec![("Big", bulky("Big", 600 * 1024))], &[], normal());
    let b = attach(b, vec![], &[], normal());
    a.svc.set_chunk_delay(SLOW_CHUNK);

    let id = a
        .svc
        .send(&b.id(), "Big", false, false)
        .await
        .expect("offer sent")
        .transfer_id;
    wait_offer(&b).await;
    b.svc.respond(&id, true).await.expect("accept");
    wait_until("the receiver got some chunks", || mid_transfer(&b)).await;

    hb.stop();
    drop(hb);

    wait_until("the sender ends Failed", || {
        a.has_finished(Direction::Send, Outcome::Failed)
    })
    .await;
    assert!(b.import.calls().is_empty());
    // Chunks left the node before the drop, so the send is audited with its outcome.
    assert_eq!(sent_outcomes(&a), vec!["failed".to_string()]);

    // The receiver app restarts: a new staging object on the same folder runs the startup purge.
    let restarted = FsTransferStaging::new(b.staging_dir.path().to_path_buf());
    restarted.purge().expect("purge after restart");
    assert_eq!(
        count_files(b.staging_dir.path()),
        0,
        "no staged file survives a restart"
    );
}

#[tokio::test]
async fn a_restart_purges_files_a_crash_left_behind() {
    let dir = TempDir::new().expect("tempdir");
    let staging = FsTransferStaging::new(dir.path().to_path_buf());
    let mut staged = staging
        .create("0123456789abcdef0123456789abcdef")
        .expect("create the staged file");
    staged.append(b"half a zip").expect("append");
    // A crash never runs finish or discard.
    std::mem::forget(staged);
    assert!(count_files(dir.path()) >= 1, "the crash left a staged file");

    // The same order as `lib.rs`: purge first, then build the service.
    let node = attach_in(e2e_node("ben"), vec![], &[], normal(), dir);
    assert_eq!(node.staged_files(), 0, "the startup purge removed it");
}

// ----- unpair during a transfer -----

#[tokio::test]
async fn unpairing_during_a_transfer_ends_both_sides_and_imports_nothing() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Big", bulky("Big", 600 * 1024))], &[], normal());
    let b = attach(b, vec![], &[], normal());
    a.svc.set_chunk_delay(SLOW_CHUNK);

    let id = a
        .svc
        .send(&b.id(), "Big", false, false)
        .await
        .expect("offer sent")
        .transfer_id;
    wait_offer(&b).await;
    b.svc.respond(&id, true).await.expect("accept");
    wait_until("the receiver got some chunks", || mid_transfer(&b)).await;

    b.node
        .svc
        .remove_paired(&a.id())
        .await
        .expect("B unpairs A");

    let ended = |x: &Xfer, d: Direction| {
        x.has_finished(d, Outcome::Failed) || x.has_finished(d, Outcome::Cancelled)
    };
    wait_until("both sides ended without completing", || {
        ended(&a, Direction::Send) && ended(&b, Direction::Receive)
    })
    .await;
    assert!(!a.has_finished(Direction::Send, Outcome::Completed));
    assert!(!b.has_finished(Direction::Receive, Outcome::Completed));
    assert!(b.import.calls().is_empty());
    wait_until("staging is empty and both unpaired", || {
        b.staged_files() == 0
            && !a.node.paired.contains(&b.id())
            && !b.node.paired.contains(&a.id())
    })
    .await;
}

// ----- damaged and unsafe payloads -----

#[tokio::test]
async fn a_tampered_chunk_is_damaged_and_nothing_is_imported() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Big", bulky("Big", 100 * 1024))], &[], normal());
    let b = attach(b, vec![], &[], normal());
    a.svc.set_tamper(Some(1));

    let id = a
        .svc
        .send(&b.id(), "Big", false, false)
        .await
        .expect("offer sent")
        .transfer_id;
    let (_, _, size) = wait_offer(&b).await;
    assert!(
        size > 2 * rocket_network::limits::CHUNK_BYTES as u64,
        "several chunks"
    );
    b.svc.respond(&id, true).await.expect("accept");

    wait_until("the receiver ends Damaged", || {
        b.has_finished(Direction::Receive, Outcome::Damaged)
    })
    .await;
    wait_until("the sender ends Damaged", || {
        a.has_finished(Direction::Send, Outcome::Damaged)
    })
    .await;
    // Ruling: every chunk left the sender, so the audit rule gives one row with the wire
    // outcome "damaged" (the brief said no row; the carried rule replaces it).
    assert_eq!(sent_outcomes(&a), vec!["damaged".to_string()]);
    assert!(
        b.import.calls().is_empty(),
        "a damaged payload is never imported"
    );
    // A tampered payload is evidence, so the receiver audits it (index R6).
    assert_eq!(received_outcomes(&b), vec!["damaged".to_string()]);
    wait_until("staging is empty", || b.staged_files() == 0).await;
}

#[tokio::test]
async fn an_unsafe_zip_is_rejected_and_nothing_is_imported() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![], &[], normal());
    let b = attach(b, vec![], &[], normal());

    let id = a
        .svc
        .offer_raw(&b.id(), "Evil", bad_zip())
        .await
        .expect("a well-formed offer goes out");
    wait_offer(&b).await;
    b.svc.respond(&id, true).await.expect("accept");

    wait_until("the receiver ends Rejected", || {
        b.has_finished(Direction::Receive, Outcome::Rejected)
    })
    .await;
    assert!(
        b.import.calls().is_empty(),
        "an unsafe ZIP is never imported"
    );
    wait_until("the sender ends Rejected", || {
        a.has_finished(Direction::Send, Outcome::Rejected)
    })
    .await;
    assert!(!a.has_finished(Direction::Send, Outcome::Completed));
    assert_eq!(received_outcomes(&b), vec!["rejected".to_string()]);
    // Ruling: the whole ZIP left the sender, so it audits one "rejected" row (carried rule).
    assert_eq!(sent_outcomes(&a), vec!["rejected".to_string()]);
    wait_until("staging is empty", || b.staged_files() == 0).await;
}

const IMPORT_FAILURE: &str = "import-failure-marker-3b8e61d4";

#[tokio::test]
async fn a_failing_import_fails_both_sides_and_carries_the_importer_message() {
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Orders", orders("Orders"))], &[], normal());
    let b = attach(b, vec![], &[], normal());
    b.import.fail_next(IMPORT_FAILURE);

    let id = a
        .svc
        .send(&b.id(), "Orders", false, false)
        .await
        .expect("offer sent")
        .transfer_id;
    wait_offer(&b).await;
    b.svc.respond(&id, true).await.expect("accept");

    wait_until("the receiver ends Failed", || {
        b.has_finished(Direction::Receive, Outcome::Failed)
    })
    .await;
    wait_until("the sender ends Failed", || {
        a.has_finished(Direction::Send, Outcome::Failed)
    })
    .await;
    // Ruling: the importer's text is only logged. The event carries the fixed sentence, so
    // no receiver-local detail reaches the wire or the UI.
    assert_eq!(
        b.finished_messages(),
        vec![Some("The collection could not be imported.".to_string())]
    );
    let all_events = format!("{:?}{:?}", a.events(), b.events());
    assert!(
        !all_events.contains(IMPORT_FAILURE),
        "the importer text leaked"
    );
    assert!(
        received_outcomes(&b).is_empty(),
        "a failed import is not audited on the receiver"
    );
    // The whole ZIP left the sender, so it audits one row with its outcome.
    assert_eq!(sent_outcomes(&a), vec!["failed".to_string()]);
    wait_until("staging is empty", || b.staged_files() == 0).await;
}

// ----- limits -----

#[tokio::test]
async fn a_collection_over_the_limit_is_refused_before_any_offer() {
    let (a, b, _ha, _hb) = online_pair().await;
    let over = MAX_TRANSFER_BYTES as usize + 1024 * 1024;
    let a = attach(a, vec![("Huge", bulky("Huge", over))], &[], normal());
    let b = attach(b, vec![], &[], normal());

    let err = a
        .svc
        .send(&b.id(), "Huge", false, false)
        .await
        .expect_err("over the limit");
    assert!(
        err.to_string().contains("too big to send"),
        "unexpected error: {err}"
    );

    // Nothing may reach the peer, so give a stray frame time to show up.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(b.events().is_empty(), "no offer was made");
    assert!(a.events().is_empty(), "no transfer was started");
    assert!(!a
        .node
        .audited()
        .iter()
        .any(|k| matches!(k, AuditEventKind::NetworkCollectionSent { .. })));
}

#[tokio::test]
async fn the_fourth_pending_offer_is_told_busy() {
    let (hub, first, _hh, _h1) = online_pair().await;
    let hub = attach(hub, vec![], &[], normal());
    let mut senders = vec![attach(first, vec![("C0", orders("C0"))], &[], normal())];
    let mut handles = Vec::new();
    for i in 1..=MAX_PENDING_INCOMING {
        let (node, handle) = join_peer(&hub.node, &format!("peer{i}")).await;
        handles.push(handle);
        let name = format!("C{i}");
        senders.push(attach(
            node,
            vec![(name.as_str(), orders(&name))],
            &[],
            normal(),
        ));
    }
    assert_eq!(senders.len(), MAX_PENDING_INCOMING + 1);

    // One offer from each of the first three peers, one at a time, so the order is known.
    for (i, sender) in senders.iter().take(MAX_PENDING_INCOMING).enumerate() {
        sender
            .svc
            .send(&hub.id(), &format!("C{i}"), false, false)
            .await
            .expect("offer sent");
        wait_until("the hub shows the offer", || hub.offers().len() == i + 1).await;
    }

    // The fourth peer is over the limit of pending offers.
    let fourth = &senders[MAX_PENDING_INCOMING];
    fourth
        .svc
        .send(&hub.id(), &format!("C{MAX_PENDING_INCOMING}"), false, false)
        .await
        .expect("the offer frame is sent");
    wait_until("the fourth sender is told busy", || {
        fourth.has_finished(Direction::Send, Outcome::Busy)
    })
    .await;
    assert_eq!(
        hub.offers().len(),
        MAX_PENDING_INCOMING,
        "still three pending"
    );
    for sender in senders.iter().take(MAX_PENDING_INCOMING) {
        assert!(
            sender.finished().is_empty(),
            "the first three are still waiting"
        );
    }
    drop(handles);
}

// ----- the secret check -----

const TOKEN: &str = "TOKEN-LITERAL-4f9c2e81";
const SECRET_VAR: &str = "SECRET-VALUE-b71d08c3";
const SECRET_ENV: &str = "ENV-SECRET-93ab55e0";
// Distinct long markers for values that must survive, so the positive control cannot match
// by accident.
const PLAIN_HEADER: &str = "header-plain-marker-5c1f02be";
const PLAIN_REGION: &str = "region-plain-marker-7d3e91aa";
const PLAIN_HOST: &str = "host-plain-marker-a86d4417";

fn secret_collection() -> Vec<SourceFile> {
    vec![
        file(
            "opencollection.yml",
            "opencollection: \"1.0.0\"\ninfo:\n  name: Secrets\n",
        ),
        file(
            "users/get.yml",
            &format!(
                "info:\n  name: get\nhttp:\n  method: GET\n  url: https://api.example.test/orders\n  headers:\n    - name: Authorization\n      value: Bearer {TOKEN}\n    - name: X-Plain\n      value: {PLAIN_HEADER}\nruntime:\n  variables:\n    - name: region\n      value: {PLAIN_REGION}\n    - name: session_id\n      value: {SECRET_VAR}\n      secret: true\n"
            ),
        ),
        file(
            "environments/local.yml",
            &format!(
                "name: local\nvariables:\n  - name: db\n    value: {SECRET_ENV}\n    secret: true\n  - name: host\n    value: {PLAIN_HOST}\n"
            ),
        ),
    ]
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

#[tokio::test]
async fn a_pushed_collection_carries_neither_the_token_nor_the_secret_variable() {
    // The fixture really holds the values, so the test would catch a leak.
    for needle in [
        TOKEN,
        SECRET_VAR,
        SECRET_ENV,
        PLAIN_HEADER,
        PLAIN_REGION,
        PLAIN_HOST,
    ] {
        assert!(secret_collection()
            .iter()
            .any(|f| contains(&f.bytes, needle)));
    }

    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Secrets", secret_collection())], &[], normal());
    let b = attach(b, vec![], &[], normal());

    // Environments are included on purpose: their secret values must be blanked too.
    let started = a
        .svc
        .send(&b.id(), "Secrets", true, false)
        .await
        .expect("offer sent");
    let id = started.transfer_id.clone();
    // The notes name what was blanked or skipped, never the values themselves. If Plan 03
    // lists no note for this fixture, check what `ExportReport` holds for it; do not drop
    // the non-empty assert.
    assert!(
        !started.notes.is_empty(),
        "the blanked secrets are reported"
    );
    for note in &started.notes {
        for needle in [TOKEN, SECRET_VAR, SECRET_ENV] {
            assert!(
                !note.contains(needle),
                "a note holds a secret value: {note}"
            );
        }
    }
    wait_offer(&b).await;
    b.svc.respond(&id, true).await.expect("accept");
    wait_until("both sides finished", || {
        a.has_finished(Direction::Send, Outcome::Completed)
            && b.has_finished(Direction::Receive, Outcome::Completed)
    })
    .await;

    let calls = b.import.calls();
    assert_eq!(calls.len(), 1);
    let zip_bytes = &calls[0].1;
    assert!(!contains(zip_bytes, TOKEN) && !contains(zip_bytes, SECRET_VAR));

    // Unzip and scan every entry, not only the raw bytes: deflate could hide a value.
    let entries = zip_entries(zip_bytes);
    assert!(
        entries
            .iter()
            .any(|(n, _)| n.ends_with("environments/local.yml")),
        "the environment file was sent, so its secret was scrubbed and not left out"
    );
    for (name, data) in &entries {
        for needle in [TOKEN, SECRET_VAR, SECRET_ENV] {
            assert!(!contains(data, needle), "{name} holds a secret value");
        }
    }
    // Positive control: values that are not secret survive, so the scan reads real content.
    let all: Vec<u8> = entries.iter().flat_map(|(_, d)| d.clone()).collect();
    for marker in [PLAIN_HEADER, PLAIN_REGION, PLAIN_HOST] {
        assert!(contains(&all, marker), "{marker} did not survive the trip");
    }
}

#[tokio::test]
async fn a_send_without_environments_carries_no_environment_file() {
    // The source really has an environment file, so the test would catch a service that
    // ignores the flag.
    assert!(secret_collection()
        .iter()
        .any(|f| f.rel_path.starts_with("environments/")));
    let (a, b, _ha, _hb) = online_pair().await;
    let a = attach(a, vec![("Secrets", secret_collection())], &[], normal());
    let b = attach(b, vec![], &[], normal());

    // `push_and_accept` sends with `include_environments = false`, the dialog's default.
    push_and_accept(&a, &b, "Secrets").await;

    assert!(b.has_finished(Direction::Receive, Outcome::Completed));
    let calls = b.import.calls();
    assert_eq!(calls.len(), 1);
    let entries = zip_entries(&calls[0].1);
    let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
    assert!(
        names.iter().any(|n| n.ends_with("users/get.yml")),
        "the requests were sent: {names:?}"
    );
    assert!(
        !names.iter().any(|n| n.contains("environments/")),
        "no environment entry may be sent: {names:?}"
    );
    for (name, data) in &entries {
        assert!(
            !contains(data, PLAIN_HOST),
            "{name} holds an environment value"
        );
    }
}

// ----- after the final ack -----

#[tokio::test]
async fn a_receiver_gone_after_the_final_ack_leaves_the_sender_unconfirmed() {
    let (a, b, _ha, hb) = online_pair().await;
    let a = attach(a, vec![("Orders", orders("Orders"))], &[], normal());
    let b = attach(b, vec![], &[], normal());
    b.import.hold_imports(true);

    let id = a
        .svc
        .send(&b.id(), "Orders", false, false)
        .await
        .expect("offer sent")
        .transfer_id;
    wait_offer(&b).await;
    b.svc.respond(&id, true).await.expect("accept");
    // The import started, so every chunk arrived and the sender waits for the result.
    wait_until("the receiver started the import", || {
        b.import.entered() == 1
    })
    .await;
    // The receiver sent the final `ChunkAck` before it started the import; give it a moment
    // to reach the sender. Only after that ack does a lost peer give `Unconfirmed`.
    tokio::time::sleep(Duration::from_millis(300)).await;

    hb.stop();
    drop(hb);

    wait_until("the sender ends Unconfirmed", || {
        a.has_finished(Direction::Send, Outcome::Unconfirmed)
    })
    .await;
    assert!(
        !a.has_finished(Direction::Send, Outcome::Failed),
        "the bytes left, so the sender does not report a failure"
    );
    assert!(a
        .node
        .audited()
        .contains(&AuditEventKind::NetworkCollectionSent {
            device_id: b.id().short(),
            collection: "Orders".to_string(),
            outcome: "unconfirmed".to_string(),
        }));
    // Let the held import thread end.
    b.import.hold_imports(false);
}

// ----- flow control -----

/// The simulated slow disk of the receiver: every append waits this long first.
const SLOW_APPEND: Duration = Duration::from_millis(20);

// A multi-thread runtime, so the receiver's slow disk blocks only one worker thread and the
// sender is free to run ahead if nothing holds it back. On the single-thread runtime the
// sleep would also stop the sender, and the test could pass without the window.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_slow_receiver_keeps_the_sender_inside_the_ack_window() {
    let (a, b, _ha, _hb) = online_pair().await;
    // About 3 MiB of incompressible bytes, so about 96 chunks: three windows.
    let a = attach(
        a,
        vec![("Slow", bulky("Slow", 3 * 1024 * 1024))],
        &[],
        normal(),
    );
    let b = attach_slow(b, SLOW_APPEND);

    let id = a
        .svc
        .send(&b.id(), "Slow", false, false)
        .await
        .expect("offer sent")
        .transfer_id;
    let (_, _, size) = wait_offer(&b).await;
    assert!(
        size / CHUNK_BYTES as u64 > 2 * u64::from(MAX_UNACKED_CHUNKS),
        "the payload spans several windows"
    );
    b.svc.respond(&id, true).await.expect("accept");

    // Plan 03 emits one `Progress` per chunk on both sides, so the last one is the count so
    // far. The sender is read first: the receiver's count only grows, so the gap measured is
    // never smaller than the real one. One chunk of slack covers a receiver that emits its
    // progress just after it sent the ack.
    let limit = (u64::from(MAX_UNACKED_CHUNKS) + 1) * CHUNK_BYTES as u64;
    let last_done = |x: &Xfer, d: Direction| x.progress(d).last().map(|p| p.0).unwrap_or(0);
    let mut widest = 0u64;
    wait_until("both sides completed", || {
        let sent = last_done(&a, Direction::Send);
        let received = last_done(&b, Direction::Receive);
        widest = widest.max(sent.saturating_sub(received));
        a.has_finished(Direction::Send, Outcome::Completed)
            && b.has_finished(Direction::Receive, Outcome::Completed)
    })
    .await;
    assert!(
        widest <= limit,
        "the sender ran {widest} bytes ahead, the window allows {limit}"
    );
    assert!(widest > 0, "the sampler saw the sender ahead at least once");
    assert_eq!(b.import.calls().len(), 1, "the slow transfer still imports");
}
