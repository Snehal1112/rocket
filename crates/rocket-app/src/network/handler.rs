//! The peer handler: what the peer server calls. Strangers can reach only `/hello` and the
//! pairing calls, and every pairing call is bound to the key that opened the session.

use std::net::IpAddr;

use async_trait::async_trait;
use rocket_network::limits::{MAX_PAIRED_DEVICES, PROTOCOL_VERSION};
use rocket_network::{
    commitment_to_base64, DecisionRequest, DeviceId, HelloResponse, NetworkEvent, Nonce,
    NonceMessage, PairRequest, PairRequestResponse, PairStatusResponse, PairingCode, PairingId,
    PairingRole, PairingSession, PairingState, PairingStatus, PeerError, PeerHandler, PeerIdentity,
    PeerMessage, PeerSocket,
};

use super::pairing::SessionEntry;
use super::service::NetworkService;
use super::status::NetworkState;

/// The session must belong to this key and play the responder.
fn check_session(entry: &SessionEntry, peer: &PeerIdentity) -> Result<(), PeerError> {
    if entry.session.role() != PairingRole::Responder
        || entry.session.peer().device_id != peer.device_id
    {
        return Err(PeerError::Forbidden);
    }
    Ok(())
}

#[async_trait]
impl PeerHandler for NetworkService {
    async fn hello(&self) -> HelloResponse {
        self.hello_response()
    }

    async fn pair_request(
        &self,
        peer: PeerIdentity,
        _remote_ip: IpAddr,
        req: PairRequest,
    ) -> Result<PairRequestResponse, PeerError> {
        if req.version != PROTOCOL_VERSION {
            return Err(PeerError::BadRequest);
        }
        let profile = req.profile.validated().map_err(|_| PeerError::BadRequest)?;
        // Any read error counts as blocked.
        if self.deps.blocked.contains(&peer.device_id).unwrap_or(true) {
            tracing::info!(code = "network_request_blocked", id = %peer.device_id.short(), "refused");
            return Err(PeerError::Forbidden);
        }
        let already_paired = self
            .deps
            .paired
            .get(&peer.device_id)
            .map_err(|_| PeerError::Internal)?
            .is_some();
        let paired_count = self
            .deps
            .paired
            .list()
            .map_err(|_| PeerError::Internal)?
            .len();

        let now = Self::now();
        let id = PairingId::new();
        let commit =
            {
                let mut st = self.lock();
                let Some(local) = st.runtime.as_ref().map(|r| r.local.clone()) else {
                    return Err(PeerError::Forbidden);
                };
                if !matches!(st.net, NetworkState::Running) || st.settings.hidden {
                    return Err(PeerError::Forbidden);
                }
                if peer.device_id == local.device_id {
                    return Err(PeerError::BadRequest);
                }
                if !already_paired && paired_count >= MAX_PAIRED_DEVICES {
                    return Err(PeerError::Forbidden);
                }
                if st
                    .cooldowns
                    .get(&peer.device_id)
                    .is_some_and(|until| *until > now)
                {
                    return Err(PeerError::Busy);
                }
                if st.sessions.values().any(|e| {
                    !e.session.is_terminal() && e.session.peer().device_id == peer.device_id
                }) {
                    return Err(PeerError::Busy);
                }
                // The service-level limit; `SlidingWindow` is tested in Plan 01.
                if !st.pair_window.try_hit(now) {
                    return Err(PeerError::Busy);
                }
                let (session, commit) = PairingSession::new_responder(
                    id.clone(),
                    peer.clone(),
                    profile,
                    local.spki_der.clone(),
                    Nonce::random(),
                    now,
                );
                st.sessions
                    .insert(id.clone(), SessionEntry::new(session, None, now));
                commit
            };
        Ok(PairRequestResponse {
            pairing_id: id,
            commitment: commitment_to_base64(&commit),
        })
    }

    async fn pair_nonce(
        &self,
        peer: PeerIdentity,
        id: PairingId,
        msg: NonceMessage,
    ) -> Result<NonceMessage, PeerError> {
        let na = Nonce::parse_base64(&msg.nonce).map_err(|_| PeerError::BadRequest)?;
        let (nb, code, name) = self
            .with_session(
                &id,
                |entry| -> Result<(Nonce, PairingCode, String), PeerError> {
                    check_session(entry, &peer)?;
                    if entry.session.is_terminal() {
                        return Err(PeerError::NotFound);
                    }
                    let (nb, code) = entry
                        .session
                        .receive_nonce(na)
                        .map_err(|_| PeerError::BadRequest)?;
                    Ok((nb, code, entry.session.peer_profile().display_name.clone()))
                },
            )
            .ok_or(PeerError::NotFound)??;
        // The notice appears only now, after the commitment was revealed.
        self.emit(NetworkEvent::PairingIncoming {
            pairing_id: id.0.clone(),
            device_id: peer.device_id.to_base32(),
            short_id: peer.device_id.short(),
            display_name: name,
            code: code.digits(),
        });
        self.emit(NetworkEvent::NodesChanged);
        Ok(NonceMessage {
            nonce: nb.to_base64(),
        })
    }

    async fn pair_decision(
        &self,
        peer: PeerIdentity,
        id: PairingId,
        req: DecisionRequest,
    ) -> Result<(), PeerError> {
        let state = self
            .with_session(&id, |entry| -> Result<PairingState, PeerError> {
                check_session(entry, &peer)?;
                // A decision for a finished session is acknowledged and ignored.
                if entry.session.is_terminal() {
                    return Ok(entry.session.state());
                }
                entry
                    .session
                    .decide_remote(req.accept)
                    .map_err(|_| PeerError::BadRequest)?;
                Ok(entry.session.state())
            })
            .ok_or(PeerError::NotFound)??;
        if state == PairingState::Paired {
            self.complete_pairing(&id);
        }
        Ok(())
    }

    async fn pair_status(
        &self,
        peer: PeerIdentity,
        id: PairingId,
    ) -> Result<PairStatusResponse, PeerError> {
        let (status, state) = self
            .with_session(
                &id,
                |entry| -> Result<(PairingStatus, PairingState), PeerError> {
                    check_session(entry, &peer)?;
                    Ok((entry.session.status(), entry.session.state()))
                },
            )
            .ok_or(PeerError::NotFound)??;
        if state == PairingState::Paired {
            self.complete_pairing(&id);
        }
        Ok(PairStatusResponse { status })
    }

    async fn is_paired(&self, device: &DeviceId) -> bool {
        self.paired_and_not_blocked(device)
    }

    async fn on_peer_socket(&self, socket: PeerSocket) {
        self.accept_socket(socket).await;
    }

    fn local_hello(&self) -> PeerMessage {
        self.local_hello_message()
    }
}
