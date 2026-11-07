//! The transfer service: sends one collection to a paired node and receives one from it.
//!
//! The service reaches the network through `PeerSender` (the network service in production).
//! Frames and peer-gone notices arrive through `TransferSink`, go into one bounded queue and
//! are handled by one worker in order. Every rule (offer shape, expiry, chunk order, limits)
//! lives in the Plan 01 machines in `rocket-network`; this file and its siblings do the I/O
//! around them. The worker only runs short sync handlers; the finalize step and every outbound
//! frame of a handler run as their own tasks. State lives behind one `std::sync::Mutex` that is
//! never held across an `.await`. Whoever removes a transfer from the state emits its single
//! `Finished` event.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use rocket_audit::{AuditActor, AuditEventKind, SecurityAuditPublisher};
use rocket_network::limits::{SENDER_RESULT_TIMEOUT, TRANSFER_OFFER_TTL};
use rocket_network::{
    DeviceId, Outcome, PeerMessage, TransferEvent, TransferId, TransferLimits, TransferSink,
};
use rocket_shared::error::DomainResult;
use rocket_shared::events::EventPublisher;
use tokio::sync::mpsc;

use crate::collection_export_service::CollectionExportService;

use super::service::NetworkService;
use super::transfer_ports::{CollectionImportPort, TransferStaging};
use super::transfer_send::Out;

/// Items that may wait for the worker. A full queue drops a frame; the lost frame ends its
/// transfer by the sequence rule or the idle limit (ruling 22).
const FRAME_QUEUE: usize = 256;

/// A receive that gets no frame, or a send that gets no ack, for this long is failed. The
/// peer-gone hook ends a dead connection at once; this is the fallback for a half-open one.
const DEFAULT_IDLE_LIMIT: Duration = Duration::from_secs(120);

/// The wire name of an outcome, for the audit `outcome` field. A test pins it to serde.
pub(super) fn outcome_name(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Completed => "completed",
        Outcome::Unconfirmed => "unconfirmed",
        Outcome::Declined => "declined",
        Outcome::Expired => "expired",
        Outcome::Cancelled => "cancelled",
        Outcome::Failed => "failed",
        Outcome::TooBig => "too_big",
        Outcome::Busy => "busy",
        Outcome::Damaged => "damaged",
        Outcome::Rejected => "rejected",
    }
}

/// What the worker handles, in arrival order.
enum Item {
    Frame(DeviceId, PeerMessage),
    Gone(DeviceId),
}

/// The three waits of the service. Only tests change them (`new_with_timing`).
#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// How long an offer may stay unanswered. The machines and the offer timers use it.
    pub offer_ttl: Duration,
    /// How long an accepted receive may stay silent, and how long a send with a full ack
    /// window waits for the next `ChunkAck`.
    pub idle_limit: Duration,
    /// How long the sender waits for the receiver's `TransferResult`.
    pub result_timeout: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            offer_ttl: TRANSFER_OFFER_TTL,
            idle_limit: DEFAULT_IDLE_LIMIT,
            result_timeout: SENDER_RESULT_TIMEOUT,
        }
    }
}

/// What the transfer service needs from the network node.
#[async_trait]
pub trait PeerSender: Send + Sync {
    /// Sends one frame to a paired, online device. Offline is `InvalidInput`.
    async fn send_to(&self, device: &DeviceId, message: PeerMessage) -> DomainResult<()>;
    /// The display name and short id of a paired, unblocked device.
    fn peer_label(&self, device: &DeviceId) -> Option<(String, String)>;
    /// True when the device has a live connection.
    fn is_online(&self, device: &DeviceId) -> bool;
    /// Sets the receiver of the transfer frames and peer-gone notices.
    fn set_transfer_sink(&self, sink: Arc<dyn TransferSink>);
}

#[async_trait]
impl PeerSender for NetworkService {
    async fn send_to(&self, device: &DeviceId, message: PeerMessage) -> DomainResult<()> {
        NetworkService::send_to(self, device, message).await
    }

    fn peer_label(&self, device: &DeviceId) -> Option<(String, String)> {
        NetworkService::peer_label(self, device)
    }

    fn is_online(&self, device: &DeviceId) -> bool {
        NetworkService::is_online(self, device)
    }

    fn set_transfer_sink(&self, sink: Arc<dyn TransferSink>) {
        NetworkService::set_transfer_sink(self, sink);
    }
}

/// Everything the service needs from outside. All of it is a port or a service.
pub struct TransferDeps {
    pub network: Arc<dyn PeerSender>,
    pub export: Arc<CollectionExportService>,
    pub importer: Arc<dyn CollectionImportPort>,
    pub staging: Arc<dyn TransferStaging>,
    pub events: Arc<dyn EventPublisher>,
    pub audit: Arc<dyn SecurityAuditPublisher>,
}

/// The open transfers and the offer limits. `limits` knows every transfer in both maps.
#[derive(Default)]
pub(super) struct Inner {
    pub outgoing: HashMap<TransferId, Out>,
    pub limits: TransferLimits,
}

impl Inner {
    /// Removes an outgoing transfer and frees its slot in the limits.
    pub(super) fn take_out(&mut self, id: &TransferId) -> Option<Out> {
        self.limits.release(id);
        self.outgoing.remove(id)
    }
}

pub struct TransferService {
    me: Weak<TransferService>,
    pub(super) deps: TransferDeps,
    pub(super) timing: Timing,
    inner: Mutex<Inner>,
    frames: mpsc::Sender<Item>,
    worker: Mutex<Option<mpsc::Receiver<Item>>>,
    /// Test hook: a sleep before each chunk is taken. Not in a release build.
    #[cfg(test)]
    pub(super) chunk_delay: Mutex<Duration>,
    /// Test hook: the chunk with this sequence number is sent with one byte flipped. Not in a
    /// release build.
    #[cfg(test)]
    pub(super) tamper: Mutex<Option<u32>>,
}

impl TransferService {
    /// Builds the service and registers it as the frame sink of the network node.
    pub fn new(deps: TransferDeps) -> Arc<TransferService> {
        Self::new_with_timing(deps, Timing::default())
    }

    /// Like `new`, with other waits. Tests use it to shorten the offer lifetime, the idle
    /// limit and the wait for the result.
    #[doc(hidden)]
    pub fn new_with_timing(deps: TransferDeps, timing: Timing) -> Arc<TransferService> {
        let (frames, queue) = mpsc::channel(FRAME_QUEUE);
        let svc = Arc::new_cyclic(|me| TransferService {
            me: me.clone(),
            deps,
            timing,
            inner: Mutex::new(Inner::default()),
            frames,
            worker: Mutex::new(Some(queue)),
            #[cfg(test)]
            chunk_delay: Mutex::new(Duration::ZERO),
            #[cfg(test)]
            tamper: Mutex::new(None),
        });
        let sink: Arc<dyn TransferSink> = svc.clone();
        svc.deps.network.set_transfer_sink(sink);
        svc
    }

    /// Test hook: wait this long before each chunk is taken from the ZIP.
    #[cfg(test)]
    pub(crate) fn set_chunk_delay(&self, delay: Duration) {
        *self.chunk_delay.lock().unwrap_or_else(|e| e.into_inner()) = delay;
    }

    /// Test hook: flip one byte of the chunk with this sequence number before it is framed.
    #[cfg(test)]
    pub(crate) fn set_tamper(&self, seq: Option<u32>) {
        *self.tamper.lock().unwrap_or_else(|e| e.into_inner()) = seq;
    }

    pub(super) fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(super) fn arc(&self) -> Option<Arc<TransferService>> {
        self.me.upgrade()
    }

    /// The time the machines are given. It follows the tokio clock, so paused time works in tests.
    pub(super) fn now() -> Instant {
        tokio::time::Instant::now().into_std()
    }

    pub(super) fn emit(&self, event: TransferEvent) {
        self.deps.events.publish(&event);
    }

    pub(super) fn audit(&self, kind: AuditEventKind) {
        self.deps.audit.publish(AuditActor::User.into(), None, kind);
    }

    /// Sends a frame and logs a failure. Used where the answer to a failure is "carry on".
    pub(super) async fn tell(&self, device: &DeviceId, message: PeerMessage) {
        if let Err(e) = self.deps.network.send_to(device, message).await {
            tracing::debug!(code = "network_transfer_send_failed", error = %e, "frame not sent");
        }
    }

    /// Sends a frame from its own task. The frame worker uses this, so a slow peer never holds
    /// back the other transfers.
    pub(super) fn tell_later(&self, device: DeviceId, message: PeerMessage) {
        let Some(me) = self.arc() else {
            return;
        };
        drop(tokio::spawn(async move { me.tell(&device, message).await }));
    }

    /// Starts the worker on first use. Every caller runs inside the tokio runtime.
    pub(super) fn ensure_worker(&self) {
        let queue = self.worker.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(queue) = queue {
            drop(tokio::spawn(Self::run_worker(self.me.clone(), queue)));
        }
    }

    async fn run_worker(me: Weak<TransferService>, mut queue: mpsc::Receiver<Item>) {
        while let Some(item) = queue.recv().await {
            let Some(svc) = me.upgrade() else {
                break;
            };
            match item {
                Item::Frame(from, message) => svc.handle_frame(from, message),
                Item::Gone(device) => svc.handle_gone(&device),
            }
        }
    }

    /// Puts an item into the queue. A full queue drops a frame, but a peer-gone notice must
    /// not be lost, so it runs right here.
    fn enqueue(&self, item: Item) {
        match self.frames.try_send(item) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(Item::Gone(device)))
            | Err(mpsc::error::TrySendError::Closed(Item::Gone(device))) => {
                self.handle_gone(&device);
            }
            Err(_) => {
                tracing::debug!(code = "network_transfer_frame_dropped", "frame queue is full");
            }
        }
    }

    fn handle_frame(&self, from: DeviceId, message: PeerMessage) {
        match message {
            PeerMessage::OfferAnswer {
                transfer_id,
                accept,
            } => {
                if let Ok(id) = TransferId::parse(&transfer_id) {
                    self.on_answer(&from, &id, accept);
                }
            }
            PeerMessage::TransferAbort {
                transfer_id,
                reason,
            } => {
                if let Ok(id) = TransferId::parse(&transfer_id) {
                    self.abort_outgoing(&from, &id, reason);
                }
            }
            PeerMessage::TransferResult {
                transfer_id,
                outcome,
            } => {
                if let Ok(id) = TransferId::parse(&transfer_id) {
                    self.on_result(&from, &id, outcome);
                }
            }
            PeerMessage::ChunkAck {
                transfer_id,
                next_seq,
            } => {
                if let Ok(id) = TransferId::parse(&transfer_id) {
                    self.on_ack(&from, &id, next_seq);
                }
            }
            // The receiver side handles offers and chunks (Task 3).
            _ => {}
        }
    }

    /// The device's connection ended or its pairing was removed: end its transfers now.
    pub(super) fn handle_gone(&self, device: &DeviceId) {
        self.outgoing_gone(device);
        // The receiver side joins in Task 3.
    }
}

impl TransferSink for TransferService {
    fn on_frame(&self, from: DeviceId, message: PeerMessage) {
        self.ensure_worker();
        self.enqueue(Item::Frame(from, message));
    }

    fn on_peer_gone(&self, device: DeviceId) {
        self.ensure_worker();
        self.enqueue(Item::Gone(device));
    }
}
