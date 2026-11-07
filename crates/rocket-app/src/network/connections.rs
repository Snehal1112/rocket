//! Presence: one task per paired device keeps a WebSocket open when it can.
//!
//! The task dials when the dialer rule allows it, accepts sockets the peer opened (they arrive
//! through the entry's inbox), pings, answers pings, and ends when the pairing is removed. A
//! `NotPaired` answer to a dial means the peer removed us: we remove it too.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rocket_audit::{AuditActor, AuditEventKind};
use rocket_network::limits::{DIAL_GRACE, PING_INTERVAL, PROTOCOL_VERSION};
use rocket_network::{
    keep_dialed_by, should_dial, DeviceId, DialHint, NetworkEvent, PeerCallError, PeerMessage,
    PeerSocket, Presence, PresenceTracker,
};
use rocket_shared::error::{DomainError, DomainResult};
use tokio::sync::{mpsc, watch, Notify};
use tokio::time::MissedTickBehavior;

use super::service::NetworkService;

/// How long to wait for the peer's socket when there is no address to dial.
const NO_ADDRESS_WAIT: Duration = Duration::from_secs(30);
const DIAL_TIMEOUT: Duration = Duration::from_secs(10);
const UNPAIR_SEND_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_REMEMBERED_ADDRESSES: usize = 5;
/// After a pairing is saved here, a "not paired" answer is not trusted for this long. The
/// other side may still be saving its half of the pairing.
const NOT_PAIRED_GRACE: Duration = Duration::from_secs(30);
/// Sockets the peer opened that wait for the task to take them.
const INBOX_SIZE: usize = 4;

/// The live state of one paired device's connection.
pub(super) struct ConnEntry {
    pub presence: PresenceTracker,
    /// Sockets the peer opened to us arrive here.
    pub inbox: mpsc::Sender<PeerSocket>,
    /// Pinged when a new sighting says "retry at once".
    pub wake: Arc<Notify>,
    pub outbound: Option<mpsc::Sender<PeerMessage>>,
    pub dialed_by: Option<DeviceId>,
    pub needs_update: bool,
    /// Set for a pairing saved a moment ago. Until then `NotPaired` is not a removal.
    pub young_until: Option<Instant>,
}

enum DialResult {
    Socket(PeerSocket),
    NotPaired,
    Failed,
}

enum SocketEnd {
    /// The socket closed or went silent. Retry after the backoff.
    Closed,
    /// The peer sent `Unpair`. The pairing is gone.
    Unpaired,
    /// The node is stopping.
    Stopped,
    /// The pairing was removed here.
    Removed,
}

/// Whether a Hello frame carries the protocol version we speak.
fn version_matches(hello: &PeerMessage) -> bool {
    matches!(hello, PeerMessage::Hello { version, .. } if *version == PROTOCOL_VERSION)
}

/// Chunks and acks belong to a transfer that an offer or an answer opened after this check
/// passed. A block closes the socket, which ends the transfer, so they skip the file read.
fn needs_block_check(message: &PeerMessage) -> bool {
    !matches!(
        message,
        PeerMessage::Chunk { .. } | PeerMessage::ChunkAck { .. }
    )
}

impl NetworkService {
    /// Asks the task of a paired device to retry now.
    pub(super) fn wake_device(&self, id: &DeviceId) {
        if let Some(entry) = self.lock().conns.get(id) {
            entry.wake.notify_one();
        }
    }

    fn with_conn<R>(&self, id: &DeviceId, act: impl FnOnce(&mut ConnEntry) -> R) -> Option<R> {
        self.lock().conns.get_mut(id).map(act)
    }

    /// Starts the task for a paired device, once. `young` is true for a pairing saved just now.
    pub(super) fn ensure_connection(&self, id: &DeviceId, young: bool) {
        let Some(me) = self.arc() else {
            return;
        };
        let started = {
            let mut st = self.lock();
            let Some(stop) = st.runtime.as_ref().map(|r| r.stop.clone()) else {
                return;
            };
            if st.conns.contains_key(id) {
                return;
            }
            let (inbox_tx, inbox_rx) = mpsc::channel(INBOX_SIZE);
            let wake = Arc::new(Notify::new());
            st.conns.insert(
                *id,
                ConnEntry {
                    presence: PresenceTracker::new(),
                    inbox: inbox_tx,
                    wake: Arc::clone(&wake),
                    outbound: None,
                    dialed_by: None,
                    needs_update: false,
                    young_until: young.then(|| Self::now() + NOT_PAIRED_GRACE),
                },
            );
            (inbox_rx, wake, stop)
        };
        let (inbox, wake, stop) = started;
        let id = *id;
        self.spawn(async move { me.run_device(id, inbox, wake, stop).await });
    }

    /// Takes a socket the peer opened. Only a paired, unblocked device gets one.
    pub(super) async fn accept_socket(&self, socket: PeerSocket) {
        let id = socket.peer.device_id;
        if !self.paired_and_not_blocked(&id) {
            return;
        }
        self.ensure_connection(&id, false);
        let inbox = self.lock().conns.get(&id).map(|c| c.inbox.clone());
        if let Some(inbox) = inbox {
            // A full or closed inbox drops the socket, which closes it.
            let _ = inbox.try_send(socket);
        }
    }

    // ----- the task -----

    async fn run_device(
        &self,
        id: DeviceId,
        mut inbox: mpsc::Receiver<PeerSocket>,
        wake: Arc<Notify>,
        mut stop: watch::Receiver<bool>,
    ) {
        let Some(local) = self.local_id() else {
            return;
        };
        let mut idle_since = Self::now();
        let mut skip_dial = false;
        loop {
            if *stop.borrow() {
                return;
            }
            if !matches!(self.deps.paired.get(&id), Ok(Some(_))) {
                self.lock().conns.remove(&id);
                return;
            }
            let (candidates, hint) = self.dial_candidates(&id);
            let waited = Self::now().saturating_duration_since(idle_since);
            let allowed = should_dial(&local, &id, hint, waited);
            let mut socket = None;
            if !skip_dial && !candidates.is_empty() && allowed {
                match self.dial(&id, &candidates).await {
                    DialResult::Socket(found) => socket = Some(found),
                    DialResult::NotPaired if self.is_young(&id) => {
                        // The other side may still be saving its half of a new pairing.
                        self.mark_disconnected(&id);
                    }
                    DialResult::NotPaired => {
                        // The peer removed us while we were away.
                        self.remove_local(&id, true);
                        return;
                    }
                    DialResult::Failed => self.mark_disconnected(&id),
                }
            }
            skip_dial = false;

            if socket.is_none() {
                let delay = if candidates.is_empty() {
                    NO_ADDRESS_WAIT
                } else if !allowed {
                    DIAL_GRACE
                        .saturating_sub(waited)
                        .max(Duration::from_secs(1))
                } else {
                    self.retry_delay(&id)
                };
                tokio::select! {
                    next = inbox.recv() => match next {
                        Some(next) => socket = Some(next),
                        None => return,
                    },
                    _ = wake.notified() => {}
                    _ = tokio::time::sleep(delay) => {}
                    _ = stop.changed() => return,
                }
            }

            if let Some(socket) = socket {
                let end = self.run_socket(&id, socket, &mut inbox, &mut stop).await;
                // Whatever the end, open transfers with this device must not wait for a timer.
                self.notify_peer_gone(&id);
                match end {
                    SocketEnd::Closed => {
                        self.mark_disconnected(&id);
                        idle_since = Self::now();
                        skip_dial = true;
                        // A wake that came while the socket was live is stale. Drop it, or
                        // the retry delay would be skipped.
                        let _ = tokio::time::timeout(Duration::ZERO, wake.notified()).await;
                    }
                    SocketEnd::Stopped => {
                        // The entry keeps a sender into the socket. Drop it, or the socket
                        // stays open after the node stopped.
                        self.with_conn(&id, |c| c.outbound = None);
                        return;
                    }
                    SocketEnd::Unpaired | SocketEnd::Removed => return,
                }
            }
        }
    }

    /// Addresses to try, and whether the dialer rule reads them as mDNS or manual.
    fn dial_candidates(&self, id: &DeviceId) -> (Vec<SocketAddr>, DialHint) {
        let last = self
            .deps
            .paired
            .get(id)
            .ok()
            .flatten()
            .map(|p| p.last_addresses)
            .unwrap_or_default();
        let (mdns, manual) = self
            .lock()
            .seen
            .get(id)
            .map(|s| (s.mdns.clone(), s.manual.clone()))
            .unwrap_or_default();
        let hint = if mdns.is_empty() && !manual.is_empty() {
            DialHint::ManualAddress
        } else {
            DialHint::Mdns
        };
        let mut out: Vec<SocketAddr> = Vec::new();
        for address in mdns.into_iter().chain(manual).chain(last) {
            if !out.contains(&address) {
                out.push(address);
            }
        }
        (out, hint)
    }

    /// Blocked, or the block list cannot be read. Fails closed.
    fn is_blocked(&self, id: &DeviceId) -> bool {
        self.deps.blocked.contains(id).unwrap_or(true)
    }

    fn is_young(&self, id: &DeviceId) -> bool {
        let now = Self::now();
        self.with_conn(id, |c| c.young_until.is_some_and(|until| until > now))
            .unwrap_or(false)
    }

    fn retry_delay(&self, id: &DeviceId) -> Duration {
        self.with_conn(id, |c| c.presence.next_retry_delay())
            .unwrap_or(Duration::from_secs(1))
    }

    async fn dial(&self, id: &DeviceId, candidates: &[SocketAddr]) -> DialResult {
        let Some(connector) = self.connector() else {
            return DialResult::Failed;
        };
        for address in candidates {
            let hello = self.local_hello_message();
            let attempt =
                tokio::time::timeout(DIAL_TIMEOUT, connector.open_socket(*address, id, hello))
                    .await;
            match attempt {
                Ok(Ok(socket)) => {
                    self.remember_address(id, *address);
                    return DialResult::Socket(socket);
                }
                // Only the genuine device (its key matched) can say "not paired".
                Ok(Err(PeerCallError::NotPaired)) => return DialResult::NotPaired,
                _ => continue,
            }
        }
        DialResult::Failed
    }

    /// Saves the address that worked first in the device's list.
    fn remember_address(&self, id: &DeviceId, address: SocketAddr) {
        let _guard = self.paired_guard();
        let Ok(Some(mut device)) = self.deps.paired.get(id) else {
            return;
        };
        if device.last_addresses.first() == Some(&address) {
            return;
        }
        device.last_addresses.retain(|a| *a != address);
        device.last_addresses.insert(0, address);
        device.last_addresses.truncate(MAX_REMEMBERED_ADDRESSES);
        if let Err(e) = self.deps.paired.upsert(device) {
            tracing::debug!(code = "network_address_not_saved", error = %e, "address not saved");
        }
    }

    fn set_connected(&self, id: &DeviceId, socket: &PeerSocket) {
        let changed = self
            .with_conn(id, |c| {
                let was = c.presence.state();
                c.presence.on_connected();
                c.outbound = Some(socket.outbound.clone());
                c.dialed_by = Some(socket.dialed_by);
                c.needs_update = false;
                was != Presence::Online
            })
            .unwrap_or(false);
        if changed {
            self.emit(NetworkEvent::NodesChanged);
        }
    }

    fn mark_disconnected(&self, id: &DeviceId) {
        let changed = self
            .with_conn(id, |c| {
                let was = c.presence.state();
                c.presence.on_disconnected();
                c.outbound = None;
                c.dialed_by = None;
                was != Presence::Offline
            })
            .unwrap_or(false);
        if changed {
            self.emit(NetworkEvent::NodesChanged);
        }
    }

    /// Runs one socket until it ends. A new socket from the inbox replaces it when it wins the
    /// duplicate rule (the one dialled by the lower id is kept).
    async fn run_socket(
        &self,
        id: &DeviceId,
        first: PeerSocket,
        inbox: &mut mpsc::Receiver<PeerSocket>,
        stop: &mut watch::Receiver<bool>,
    ) -> SocketEnd {
        let Some(local) = self.local_id() else {
            return SocketEnd::Stopped;
        };
        let keep = keep_dialed_by(&local, id);
        let mut socket = first;
        if !version_matches(&socket.remote_hello) {
            self.with_conn(id, |c| c.needs_update = true);
            return SocketEnd::Closed;
        }
        self.set_connected(id, &socket);
        let mut ticker =
            tokio::time::interval_at(tokio::time::Instant::now() + PING_INTERVAL, PING_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = stop.changed() => return SocketEnd::Stopped,
                frame = socket.inbound.recv() => match frame {
                    None => return SocketEnd::Closed,
                    // A blocked device gets no answer to any frame. Its entry goes, so the
                    // socket closes when this function returns.
                    Some(ref message) if needs_block_check(message) && self.is_blocked(id) => {
                        self.lock().conns.remove(id);
                        return SocketEnd::Removed;
                    }
                    Some(PeerMessage::Ping) => {
                        let _ = socket.outbound.try_send(PeerMessage::Pong);
                    }
                    Some(PeerMessage::Pong) => {
                        self.with_conn(id, |c| c.presence.on_pong());
                    }
                    Some(PeerMessage::Unpair) => {
                        self.remove_local(id, true);
                        return SocketEnd::Unpaired;
                    }
                    // A second Hello is a protocol error.
                    Some(PeerMessage::Hello { .. }) => return SocketEnd::Closed,
                    // The transfer service owns these frames. Any of them shows the peer is
                    // alive, so a long stream cannot starve the ping rule.
                    Some(
                        message @ (PeerMessage::CollectionOffer { .. }
                        | PeerMessage::OfferAnswer { .. }
                        | PeerMessage::Chunk { .. }
                        | PeerMessage::TransferAbort { .. }
                        | PeerMessage::ChunkAck { .. }
                        | PeerMessage::TransferResult { .. }),
                    ) => {
                        self.with_conn(id, |c| c.presence.on_pong());
                        self.deliver_transfer_frame(id, message);
                    }
                },
                _ = ticker.tick() => {
                    let state = self.with_conn(id, |c| c.presence.on_ping_sent());
                    if state != Some(Presence::Online) {
                        return SocketEnd::Closed;
                    }
                    let _ = socket.outbound.try_send(PeerMessage::Ping);
                }
                next = inbox.recv() => match next {
                    None => return SocketEnd::Removed,
                    Some(new) => {
                        let wins = new.dialed_by == keep || new.dialed_by == socket.dialed_by;
                        if wins && version_matches(&new.remote_hello) {
                            socket = new;
                            self.set_connected(id, &socket);
                        }
                    }
                },
            }
        }
    }

    // ----- removing a pairing -----

    /// Removes the pairing here and tells the user. `by_peer` is true when the peer did it.
    pub(super) fn remove_local(&self, id: &DeviceId, by_peer: bool) -> bool {
        let name = self.known_name(id);
        let removed = {
            let _guard = self.paired_guard();
            self.deps.paired.remove(id)
        };
        let removed = match removed {
            Ok(removed) => removed,
            Err(e) => {
                tracing::warn!(code = "network_unpair_failed", error = %e, "pairing not removed");
                return false;
            }
        };
        let entry = self.lock().conns.remove(id);
        drop(entry);
        // The pairing and the connection are gone, so are the open transfers.
        self.notify_peer_gone(id);
        if removed {
            tracing::info!(code = "network_device_unpaired", id = %id.short(), by_peer, "unpaired");
            let actor = if by_peer {
                AuditActor::System
            } else {
                AuditActor::User
            };
            self.audit(
                actor,
                AuditEventKind::NetworkDeviceUnpaired {
                    device_id: id.to_base32(),
                    by_peer,
                },
            );
            self.emit(NetworkEvent::PairedRemoved {
                device_id: id.to_base32(),
                display_name: name,
                by_peer,
            });
            self.emit(NetworkEvent::NodesChanged);
        }
        removed
    }

    /// Removes a paired device. When a socket is open, the peer is told first.
    pub async fn remove_paired(self: &Arc<Self>, device_id: &DeviceId) -> DomainResult<()> {
        if !self.unpair_by_user(device_id).await? {
            return Err(DomainError::InvalidInput(
                "That device is not paired.".to_string(),
            ));
        }
        Ok(())
    }

    /// The local user's unpair, shared by Remove and Block. Sends `Unpair` on a live socket,
    /// drops the connection entry (which closes the socket), audits and tells the UI. Returns
    /// false when the device was not paired.
    pub(super) async fn unpair_by_user(&self, device_id: &DeviceId) -> DomainResult<bool> {
        let outbound = self
            .lock()
            .conns
            .get(device_id)
            .and_then(|c| c.outbound.clone());
        let name = self.known_name(device_id);
        let removed = {
            let _guard = self.paired_guard();
            self.deps.paired.remove(device_id)?
        };
        if !removed {
            return Ok(false);
        }
        if let Some(outbound) = outbound {
            let _ =
                tokio::time::timeout(UNPAIR_SEND_TIMEOUT, outbound.send(PeerMessage::Unpair)).await;
        }
        let entry = self.lock().conns.remove(device_id);
        drop(entry);
        // The pairing and the connection are gone, so are the open transfers.
        self.notify_peer_gone(device_id);
        tracing::info!(code = "network_device_unpaired", id = %device_id.short(), by_peer = false, "unpaired");
        self.audit(
            AuditActor::User,
            AuditEventKind::NetworkDeviceUnpaired {
                device_id: device_id.to_base32(),
                by_peer: false,
            },
        );
        self.emit(NetworkEvent::PairedRemoved {
            device_id: device_id.to_base32(),
            display_name: name,
            by_peer: false,
        });
        self.emit(NetworkEvent::NodesChanged);
        Ok(true)
    }
}
