//! Pairing: sessions, the local user's requests and decisions, cancel and the sweeper.
//!
//! The initiator is the node whose user picked the other one. The responder is the node that
//! received the request. Both keep a `PairingSession` in memory only. `PairingFinished` is
//! published once per session, from `take_finished`, whatever ended it.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use rocket_audit::{AuditActor, AuditEventKind};
use rocket_network::limits::{DECLINE_COOLDOWN, MAX_PAIRED_DEVICES, PAIRING_TTL, PROTOCOL_VERSION};
use rocket_network::{
    commitment_from_base64, BlockedDevice, DecisionRequest, DeviceId, HelloResponse, NetworkEvent,
    Nonce, NonceMessage, PairRequest, PairedDevice, PairingCode, PairingId, PairingRole,
    PairingSession, PairingState, PairingStatus, PeerCallError, PeerConnector, PeerIdentity,
};
use rocket_shared::error::{DomainError, DomainResult};
use tokio::sync::watch;

use crate::stop_signal::{run_every, sleep_or_stop};

use super::service::{paired_limit_message, NetworkService};
use super::status::NetworkState;

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const SWEEP_INTERVAL: Duration = Duration::from_secs(5);
const CALL_TIMEOUT: Duration = Duration::from_secs(10);
/// Polls that may fail one after the other before the initiator gives up.
const MAX_POLL_FAILURES: u32 = 10;
/// An initiator that waits for the answer expires this much later than its timer says, so a
/// last-second accept on the responder still reaches it through one more poll.
const EXPIRY_GRACE: Duration = Duration::from_secs(11);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingDecision {
    Accept,
    Decline,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingStarted {
    pub pairing_id: PairingId,
    pub code: PairingCode,
}

/// One open or finished pairing, seen from this node.
pub(super) struct SessionEntry {
    pub session: PairingSession,
    /// The address the initiator reached the peer on. `None` for a responder.
    pub addr: Option<SocketAddr>,
    pub created: Instant,
    /// `PairingFinished` was published.
    pub notified: bool,
    /// The paired device was saved.
    pub saved: bool,
    /// Saving has started, so it runs once.
    pub claimed: bool,
    /// The initiator sent its accept and now polls for the answer.
    pub awaiting: bool,
}

impl SessionEntry {
    pub(super) fn new(session: PairingSession, addr: Option<SocketAddr>, created: Instant) -> Self {
        SessionEntry {
            session,
            addr,
            created,
            notified: false,
            saved: false,
            claimed: false,
            awaiting: false,
        }
    }
}

fn user_error(text: &str) -> DomainError {
    DomainError::InvalidInput(text.to_string())
}

fn pairing_failed() -> DomainError {
    user_error("Pairing failed. Try again.")
}

fn busy() -> DomainError {
    user_error("The other device is busy. Try again in a minute.")
}

fn not_open() -> DomainError {
    user_error("That pairing request is no longer open.")
}

fn call_error(error: PeerCallError) -> DomainError {
    match error {
        PeerCallError::Busy => busy(),
        _ => pairing_failed(),
    }
}

/// The time a session is judged against. A waiting initiator gets `EXPIRY_GRACE` of slack.
fn expiry_clock(entry: &SessionEntry) -> Instant {
    let now = NetworkService::now();
    if entry.awaiting {
        now.checked_sub(EXPIRY_GRACE).unwrap_or(now)
    } else {
        now
    }
}

/// The `PairingFinished` event for a session that just ended, once. A session that reached
/// `Paired` is announced only after the device was saved.
fn take_finished(entry: &mut SessionEntry) -> Option<NetworkEvent> {
    if entry.notified || !entry.session.is_terminal() {
        return None;
    }
    if entry.session.state() == PairingState::Paired && !entry.saved {
        return None;
    }
    let outcome = entry.session.outcome()?;
    entry.notified = true;
    Some(NetworkEvent::PairingFinished {
        pairing_id: entry.session.id().0.clone(),
        outcome: outcome.as_str().to_string(),
        display_name: entry.session.peer_profile().display_name.clone(),
    })
}

impl NetworkService {
    /// Runs `act` on a session after expiring it if due, then publishes what ended. Returns
    /// `None` when the session does not exist.
    pub(super) fn with_session<R>(
        &self,
        id: &PairingId,
        act: impl FnOnce(&mut SessionEntry) -> R,
    ) -> Option<R> {
        let (result, finished) = {
            let mut st = self.lock();
            let entry = st.sessions.get_mut(id)?;
            entry.session.expire_if_due(expiry_clock(entry));
            let result = act(entry);
            (result, take_finished(entry))
        };
        if let Some(event) = finished {
            self.emit(event);
            self.emit(NetworkEvent::NodesChanged);
        }
        Some(result)
    }

    // ----- starting a pairing -----

    pub async fn request_pairing(
        self: &Arc<Self>,
        device_id: &DeviceId,
    ) -> DomainResult<PairingStarted> {
        let (local, connector) = {
            let st = self.lock();
            match st.runtime.as_ref() {
                Some(rt) if matches!(st.net, NetworkState::Running) => {
                    (rt.local.clone(), Arc::clone(&rt.connector))
                }
                _ => return Err(user_error("Networking is off.")),
            }
        };
        if *device_id == local.device_id {
            return Err(user_error("You cannot pair with this device."));
        }
        if self.deps.paired.get(device_id)?.is_some() {
            return Err(user_error("This device is already paired."));
        }
        if self.deps.paired.list()?.len() >= MAX_PAIRED_DEVICES {
            return Err(user_error(&paired_limit_message()));
        }
        if self.deps.blocked.contains(device_id)? {
            return Err(user_error(
                "This device is blocked. Unblock it in Settings to pair.",
            ));
        }
        let addresses = {
            let st = self.lock();
            if st
                .sessions
                .values()
                .any(|e| !e.session.is_terminal() && e.session.peer().device_id == *device_id)
            {
                return Err(user_error("A pairing with this device is already open."));
            }
            match st.seen.get(device_id) {
                Some(seen) => seen
                    .mdns
                    .iter()
                    .chain(seen.manual.iter())
                    .copied()
                    .collect::<Vec<_>>(),
                None => return Err(user_error("That device is no longer nearby.")),
            }
        };

        // Deviation 8: a hello first, so the key is checked before any pairing message.
        let Some((address, hello)) = self
            .verify_hello(&connector, &addresses, Some(device_id))
            .await
        else {
            return Err(pairing_failed());
        };
        self.initiate(&local, &connector, address, hello, device_id)
            .await
    }

    /// The pairing messages, after `/hello` proved the key.
    async fn initiate(
        self: &Arc<Self>,
        local: &PeerIdentity,
        connector: &Arc<dyn PeerConnector>,
        address: SocketAddr,
        hello: HelloResponse,
        device_id: &DeviceId,
    ) -> DomainResult<PairingStarted> {
        if hello.version != PROTOCOL_VERSION {
            return Err(user_error(
                "This device needs an update before it can pair.",
            ));
        }
        let request = PairRequest {
            version: PROTOCOL_VERSION,
            profile: self.lock().profile.clone(),
        };
        let (peer_identity, reply) = match tokio::time::timeout(
            CALL_TIMEOUT,
            connector.pair_request(address, device_id, &request),
        )
        .await
        {
            Ok(Ok(reply)) => reply,
            Ok(Err(e)) => return Err(call_error(e)),
            Err(_) => return Err(pairing_failed()),
        };
        if peer_identity.device_id != *device_id {
            return Err(pairing_failed());
        }
        let pairing_id = reply.pairing_id.clone();
        let Ok(commitment) = commitment_from_base64(&reply.commitment) else {
            self.abort_remote(address, *device_id, &pairing_id).await;
            return Err(pairing_failed());
        };

        let na = Nonce::random();
        let mut session = PairingSession::new_initiator(
            pairing_id.clone(),
            peer_identity,
            hello.profile.clone(),
            local.spki_der.clone(),
            commitment,
            na.clone(),
            Self::now(),
        );
        let message = NonceMessage {
            nonce: na.to_base64(),
        };
        let revealed = match tokio::time::timeout(
            CALL_TIMEOUT,
            connector.pair_nonce(address, device_id, &pairing_id, &message),
        )
        .await
        {
            Ok(Ok(revealed)) => revealed,
            Ok(Err(e)) => {
                self.abort_remote(address, *device_id, &pairing_id).await;
                return Err(call_error(e));
            }
            Err(_) => {
                self.abort_remote(address, *device_id, &pairing_id).await;
                return Err(pairing_failed());
            }
        };
        let code = Nonce::parse_base64(&revealed.nonce).and_then(|nb| session.receive_reveal(nb));
        let code = match code {
            Ok(code) => code,
            Err(_) => {
                // A wrong nonce: the commitment check failed. Save nothing and drop the peer's side.
                tracing::info!(code = "network_pairing_failed", id = %device_id.short(), "commitment mismatch");
                self.abort_remote(address, *device_id, &pairing_id).await;
                self.emit(NetworkEvent::PairingFinished {
                    pairing_id: pairing_id.0.clone(),
                    outcome: "failed".to_string(),
                    display_name: hello.profile.display_name.clone(),
                });
                return Err(pairing_failed());
            }
        };

        self.lock().sessions.insert(
            pairing_id.clone(),
            SessionEntry::new(session, Some(address), Self::now()),
        );
        self.emit(NetworkEvent::NodesChanged);
        Ok(PairingStarted { pairing_id, code })
    }

    /// Tells the peer to drop its session. Best effort: errors are ignored.
    async fn abort_remote(&self, address: SocketAddr, expected: DeviceId, id: &PairingId) {
        if let Some(connector) = self.connector() {
            let _ = tokio::time::timeout(
                CALL_TIMEOUT,
                connector.pair_decision(address, &expected, id, &DecisionRequest { accept: false }),
            )
            .await;
        }
    }

    // ----- answering a pairing -----

    pub async fn respond_pairing(
        self: &Arc<Self>,
        id: &PairingId,
        decision: PairingDecision,
    ) -> DomainResult<()> {
        let accept = decision == PairingDecision::Accept;
        let (applied, role, address, peer, name, state) = self
            .with_session(id, |entry| {
                let applied = entry.session.decide_local(accept);
                (
                    applied,
                    entry.session.role(),
                    entry.addr,
                    entry.session.peer().clone(),
                    entry.session.peer_profile().display_name.clone(),
                    entry.session.state(),
                )
            })
            .ok_or_else(not_open)?;
        if applied.is_err() {
            return Err(if state == PairingState::Expired {
                user_error("The request expired.")
            } else {
                not_open()
            });
        }
        if decision == PairingDecision::Block {
            // A paired device that is blocked is also unpaired and disconnected.
            self.block_peer(&peer, &name).await;
        }
        if !accept && role == PairingRole::Responder {
            self.start_cooldown(&peer.device_id);
        }
        match role {
            PairingRole::Responder => {
                if accept {
                    self.complete_pairing(id);
                }
                Ok(())
            }
            PairingRole::Initiator => {
                self.finish_as_initiator(id, address, peer.device_id, accept)
                    .await
            }
        }
    }

    /// Sends the initiator's decision. On accept, a background task polls for the answer.
    async fn finish_as_initiator(
        self: &Arc<Self>,
        id: &PairingId,
        address: Option<SocketAddr>,
        expected: DeviceId,
        accept: bool,
    ) -> DomainResult<()> {
        let (Some(address), Some(connector)) = (address, self.connector()) else {
            return Err(pairing_failed());
        };
        let sent = tokio::time::timeout(
            CALL_TIMEOUT,
            connector.pair_decision(address, &expected, id, &DecisionRequest { accept }),
        )
        .await;
        if !accept {
            // A decline is best effort. The session already ended locally.
            return Ok(());
        }
        if !matches!(sent, Ok(Ok(()))) {
            // The peer could not take the decision. This is a failure, not a user cancel.
            let _ = self.with_session(id, |entry| entry.session.fail());
            return Err(pairing_failed());
        }
        let Some(stop) = self.lock().runtime.as_ref().map(|r| r.stop.clone()) else {
            return Err(user_error("Networking is off."));
        };
        if let Some(entry) = self.lock().sessions.get_mut(id) {
            entry.awaiting = true;
        }
        let me = Arc::clone(self);
        let id = id.clone();
        self.spawn(async move { me.poll_initiator(id, address, expected, stop).await });
        Ok(())
    }

    /// Polls `pair_status` once a second until the answer is final or the session expires.
    /// It keeps its own loop instead of `run_every`: it carries a failure counter and returns from
    /// several places, which a tick closure would only hide.
    async fn poll_initiator(
        &self,
        id: PairingId,
        address: SocketAddr,
        expected: DeviceId,
        mut stop: watch::Receiver<bool>,
    ) {
        let Some(connector) = self.connector() else {
            return;
        };
        let mut failures = 0u32;
        loop {
            if !sleep_or_stop(&mut stop, POLL_INTERVAL).await {
                return;
            }
            match self.with_session(&id, |entry| entry.session.is_terminal()) {
                None => return,
                Some(true) => {
                    self.complete_pairing(&id);
                    return;
                }
                Some(false) => {}
            }
            let reply =
                tokio::time::timeout(CALL_TIMEOUT, connector.pair_status(address, &expected, &id))
                    .await;
            match reply {
                Ok(Ok(answer)) => {
                    failures = 0;
                    match answer.status {
                        // The local timer expires the session; the peer's "expired" needs no action.
                        PairingStatus::Pending => {}
                        PairingStatus::Expired => {
                            // The peer is done. Drop the grace so the local timer ends it.
                            let _ = self.with_session(&id, |entry| entry.awaiting = false);
                        }
                        PairingStatus::Accepted => {
                            let _ = self.with_session(&id, |entry| {
                                let _ = entry.session.decide_remote(true);
                            });
                            self.complete_pairing(&id);
                            return;
                        }
                        PairingStatus::Declined => {
                            let _ = self.with_session(&id, |entry| {
                                let _ = entry.session.decide_remote(false);
                            });
                            return;
                        }
                    }
                }
                _ => {
                    failures += 1;
                    if failures >= MAX_POLL_FAILURES {
                        // The peer stopped answering partway. The user sees "Pairing failed".
                        let _ = self.with_session(&id, |entry| entry.session.fail());
                        return;
                    }
                }
            }
        }
    }

    pub fn cancel_pairing(&self, id: &PairingId) -> DomainResult<()> {
        let (was_live, address, peer) = self
            .with_session(id, |entry| {
                let was_live = !entry.session.is_terminal();
                entry.session.cancel();
                (was_live, entry.addr, entry.session.peer().device_id)
            })
            .ok_or_else(not_open)?;
        if let (true, Some(address), Some(me)) = (was_live, address, self.arc()) {
            let id = id.clone();
            self.spawn(async move { me.abort_remote(address, peer, &id).await });
        }
        Ok(())
    }

    // ----- finishing -----

    /// Saves the paired device once a session is `Paired`, in either role. Runs once.
    pub(super) fn complete_pairing(&self, id: &PairingId) {
        let job = {
            let mut st = self.lock();
            let Some(entry) = st.sessions.get_mut(id) else {
                return;
            };
            if entry.claimed || entry.session.state() != PairingState::Paired {
                return;
            }
            entry.claimed = true;
            (
                entry.session.peer().device_id,
                entry.session.peer_profile().display_name.clone(),
                entry.addr,
            )
        };
        let (device_id, name, address) = job;
        let result = {
            let _guard = self.paired_guard();
            self.deps.paired.upsert(PairedDevice {
                device_id,
                name_at_pairing: name,
                paired_at: Utc::now(),
                last_addresses: address.into_iter().collect(),
            })
        };
        let finished = {
            let mut st = self.lock();
            st.sessions.get_mut(id).and_then(|entry| match &result {
                Ok(()) => {
                    entry.saved = true;
                    take_finished(entry)
                }
                Err(_) => {
                    entry.notified = true;
                    Some(NetworkEvent::PairingFinished {
                        pairing_id: id.0.clone(),
                        outcome: "failed".to_string(),
                        display_name: entry.session.peer_profile().display_name.clone(),
                    })
                }
            })
        };
        match result {
            Ok(()) => {
                tracing::info!(code = "network_device_paired", id = %device_id.short(), "paired");
                self.audit(
                    AuditActor::User,
                    AuditEventKind::NetworkDevicePaired {
                        device_id: device_id.to_base32(),
                    },
                );
                self.ensure_connection(&device_id, true);
            }
            Err(e) => {
                tracing::warn!(code = "network_pairing_save_failed", error = %e, "pairing not saved");
            }
        }
        if let Some(event) = finished {
            self.emit(event);
        }
        self.emit(NetworkEvent::NodesChanged);
    }

    pub(super) fn start_cooldown(&self, device_id: &DeviceId) {
        let until = Self::now() + DECLINE_COOLDOWN;
        self.lock().cooldowns.insert(*device_id, until);
    }

    /// Adds the peer to the block list. The sender sees the same "declined" as for a decline.
    /// A paired device is also unpaired: it gets `Unpair` on a live socket, the socket closes,
    /// and `PairedRemoved { by_peer: false }` and an unpair audit record follow.
    pub(super) async fn block_peer(&self, peer: &PeerIdentity, name: &str) {
        let device = BlockedDevice {
            device_id: peer.device_id,
            name: name.to_string(),
            blocked_at: Utc::now(),
        };
        match self.deps.blocked.add(device) {
            Ok(()) => {
                tracing::info!(code = "network_device_blocked", id = %peer.device_id.short(), "blocked");
                self.audit(
                    AuditActor::User,
                    AuditEventKind::NetworkDeviceBlocked {
                        device_id: peer.device_id.to_base32(),
                    },
                );
                self.start_cooldown(&peer.device_id);
            }
            Err(e) => {
                tracing::warn!(code = "network_block_failed", error = %e, "block not saved");
                return;
            }
        }
        if matches!(self.deps.paired.get(&peer.device_id), Ok(Some(_))) {
            // The pairing goes too, so a blocked device never stays paired.
            if let Err(e) = self.unpair_by_user(&peer.device_id).await {
                tracing::warn!(code = "network_unpair_failed", error = %e, "pairing not removed");
            }
        }
        self.emit(NetworkEvent::NodesChanged);
    }

    // ----- the sweeper -----

    pub(super) async fn sweep_loop(&self, mut stop: watch::Receiver<bool>) {
        run_every(&mut stop, SWEEP_INTERVAL, || async move {
            self.sweep_sessions();
        })
        .await;
    }

    /// Expires due sessions (and publishes them), then drops old finished ones.
    pub(super) fn sweep_sessions(&self) {
        let now = Self::now();
        let finished: Vec<NetworkEvent> = {
            let mut st = self.lock();
            let mut events = Vec::new();
            for entry in st.sessions.values_mut() {
                entry.session.expire_if_due(expiry_clock(entry));
                if let Some(event) = take_finished(entry) {
                    events.push(event);
                }
            }
            st.sessions.retain(|_, entry| {
                !(entry.session.is_terminal()
                    && now.saturating_duration_since(entry.created) > PAIRING_TTL * 2)
            });
            st.cooldowns.retain(|_, until| *until > now);
            events
        };
        if !finished.is_empty() {
            for event in finished {
                self.emit(event);
            }
            self.emit(NetworkEvent::NodesChanged);
        }
    }
}
