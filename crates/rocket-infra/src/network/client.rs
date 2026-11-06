//! The peer client. Every REST call opens its own TLS 1.3 connection with our device
//! certificate, sends one HTTP/1.1 request and closes.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::header::{CONTENT_TYPE, HOST};
use hyper::{Method, Request};
use hyper_util::rt::TokioIo;
use rocket_network::limits::MAX_REST_BODY_BYTES;
use rocket_network::{
    commitment_from_base64, DecisionRequest, DeviceId, HelloResponse, NodeProfile, Nonce,
    NonceMessage, PairRequest, PairRequestResponse, PairStatusResponse, PairingId, PeerCallError,
    PeerConnector, PeerIdentity, PeerMessage, PeerSocket,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::rustls::{CertificateError, Error as TlsError};
use tokio_rustls::TlsConnector;

use crate::network::server::is_valid_pairing_id;
use crate::network::{client_config, peer_identity_from_certs, TlsIdentity};

/// TCP connect plus the TLS handshake.
pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// One request and its reply, or one socket upgrade with the Hello exchange. The server
/// imports this constant for its per-request timeout, so it is declared only here.
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) type ClientTls = TlsStream<TcpStream>;

/// A reply as it came off the wire. The tests use it to check raw statuses.
pub(crate) struct RawReply {
    pub(crate) peer: PeerIdentity,
    pub(crate) status: u16,
    pub(crate) body: Bytes,
}

pub struct RustlsPeerConnector {
    identity: Arc<TlsIdentity>,
}

impl RustlsPeerConnector {
    pub fn new(identity: Arc<TlsIdentity>) -> Self {
        RustlsPeerConnector { identity }
    }

    async fn call_json<T: DeserializeOwned>(
        &self,
        addr: SocketAddr,
        expected: Option<DeviceId>,
        method: Method,
        path: &str,
        body: Option<Vec<u8>>,
    ) -> Result<(PeerIdentity, T), PeerCallError> {
        let reply = send_request(&self.identity, addr, expected, method, path, body, &[]).await?;
        check_status(&reply)?;
        let value = serde_json::from_slice(&reply.body).map_err(|_| {
            PeerCallError::Protocol("the peer sent an unreadable reply".to_string())
        })?;
        Ok((reply.peer, value))
    }
}

#[async_trait]
impl PeerConnector for RustlsPeerConnector {
    async fn hello(
        &self,
        addr: SocketAddr,
    ) -> Result<(PeerIdentity, HelloResponse), PeerCallError> {
        // No expected id: this call is how a node's key is learned.
        let (peer, mut hello): (PeerIdentity, HelloResponse) = self
            .call_json(addr, None, Method::GET, "/hello", None)
            .await?;
        // The certificate decides. A body that claims another id is refused.
        if hello.device_id != peer.device_id {
            return Err(PeerCallError::IdentityMismatch);
        }
        hello.profile = validated_profile(&hello.profile)?;
        Ok((peer, hello))
    }

    async fn pair_request(
        &self,
        addr: SocketAddr,
        expected: &DeviceId,
        req: &PairRequest,
    ) -> Result<(PeerIdentity, PairRequestResponse), PeerCallError> {
        let body = json_body(req)?;
        let (peer, reply): (PeerIdentity, PairRequestResponse) = self
            .call_json(
                addr,
                Some(*expected),
                Method::POST,
                "/pair/request",
                Some(body),
            )
            .await?;
        if !is_valid_pairing_id(&reply.pairing_id.0)
            || commitment_from_base64(&reply.commitment).is_err()
        {
            return Err(PeerCallError::Protocol(
                "the peer sent a malformed pairing reply".to_string(),
            ));
        }
        Ok((peer, reply))
    }

    async fn pair_nonce(
        &self,
        addr: SocketAddr,
        expected: &DeviceId,
        id: &PairingId,
        msg: &NonceMessage,
    ) -> Result<NonceMessage, PeerCallError> {
        let path = pairing_path(id, "/nonce")?;
        let (_, reply): (PeerIdentity, NonceMessage) = self
            .call_json(
                addr,
                Some(*expected),
                Method::POST,
                &path,
                Some(json_body(msg)?),
            )
            .await?;
        if Nonce::parse_base64(&reply.nonce).is_err() {
            return Err(PeerCallError::Protocol(
                "the peer sent a malformed nonce".to_string(),
            ));
        }
        Ok(reply)
    }

    async fn pair_decision(
        &self,
        addr: SocketAddr,
        expected: &DeviceId,
        id: &PairingId,
        req: &DecisionRequest,
    ) -> Result<(), PeerCallError> {
        let path = pairing_path(id, "/decision")?;
        let reply = send_request(
            &self.identity,
            addr,
            Some(*expected),
            Method::POST,
            &path,
            Some(json_body(req)?),
            &[],
        )
        .await?;
        check_status(&reply)
    }

    async fn pair_status(
        &self,
        addr: SocketAddr,
        expected: &DeviceId,
        id: &PairingId,
    ) -> Result<PairStatusResponse, PeerCallError> {
        let path = pairing_path(id, "")?;
        let (_, reply): (PeerIdentity, PairStatusResponse) = self
            .call_json(addr, Some(*expected), Method::GET, &path, None)
            .await?;
        Ok(reply)
    }

    async fn open_socket(
        &self,
        addr: SocketAddr,
        expected: &DeviceId,
        hello: PeerMessage,
    ) -> Result<PeerSocket, PeerCallError> {
        crate::network::socket::open_client_socket(&self.identity, addr, expected, hello).await
    }
}

fn validated_profile(profile: &NodeProfile) -> Result<NodeProfile, PeerCallError> {
    profile
        .validated()
        .map_err(|_| PeerCallError::Protocol("the peer sent an invalid profile".to_string()))
}

fn json_body<T: Serialize>(value: &T) -> Result<Vec<u8>, PeerCallError> {
    serde_json::to_vec(value)
        .map_err(|e| PeerCallError::Protocol(format!("encode the request: {e}")))
}

fn pairing_path(id: &PairingId, suffix: &str) -> Result<String, PeerCallError> {
    if !is_valid_pairing_id(&id.0) {
        return Err(PeerCallError::Protocol("invalid pairing id".to_string()));
    }
    Ok(format!("/pair/{}{suffix}", id.0))
}

/// Maps a reply status to the call result.
pub(crate) fn check_status(reply: &RawReply) -> Result<(), PeerCallError> {
    match reply.status {
        200..=299 => Ok(()),
        403 if reply.body.as_ref() == b"not-paired" => Err(PeerCallError::NotPaired),
        503 => Err(PeerCallError::Busy),
        status => Err(PeerCallError::Rejected(status)),
    }
}

/// Opens TCP and TLS with our certificate. With `expected`, Plan 02's verifier refuses any
/// other key inside the handshake, before a byte of the request is sent.
pub(crate) async fn tls_connect(
    identity: &TlsIdentity,
    addr: SocketAddr,
    expected: Option<DeviceId>,
) -> Result<(ClientTls, PeerIdentity), PeerCallError> {
    let config = client_config(identity, expected)
        .map_err(|_| PeerCallError::Protocol("the TLS client could not be built".to_string()))?;
    let connector = TlsConnector::from(config);
    let attempt = async {
        let tcp = TcpStream::connect(addr)
            .await
            .map_err(|e| PeerCallError::Unreachable(e.to_string()))?;
        let _ = tcp.set_nodelay(true);
        // The verifier ignores the name. An IP name sends no SNI.
        connector
            .connect(ServerName::from(addr.ip()), tcp)
            .await
            .map_err(map_tls_error)
    };
    let tls = tokio::time::timeout(CONNECT_TIMEOUT, attempt)
        .await
        .map_err(|_| PeerCallError::Unreachable("the connection timed out".to_string()))??;
    let peer = peer_identity_from_certs(tls.get_ref().1.peer_certificates())
        .map_err(|_| PeerCallError::IdentityMismatch)?;
    // A second check, so a verifier change can never let another key through.
    if expected.is_some_and(|id| id != peer.device_id) {
        return Err(PeerCallError::IdentityMismatch);
    }
    Ok((tls, peer))
}

fn map_tls_error(error: std::io::Error) -> PeerCallError {
    match error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<TlsError>())
    {
        // The pinned key did not match: another node answers on this address.
        Some(TlsError::InvalidCertificate(CertificateError::ApplicationVerificationFailure)) => {
            PeerCallError::IdentityMismatch
        }
        Some(_) => PeerCallError::Protocol("the TLS handshake failed".to_string()),
        None => PeerCallError::Unreachable(error.to_string()),
    }
}

/// Sends one request on a new connection and reads the whole reply (at most 16 KiB).
pub(crate) async fn send_request(
    identity: &TlsIdentity,
    addr: SocketAddr,
    expected: Option<DeviceId>,
    method: Method,
    path: &str,
    body: Option<Vec<u8>>,
    headers: &[(&str, &str)],
) -> Result<RawReply, PeerCallError> {
    let (tls, peer) = tls_connect(identity, addr, expected).await?;
    let exchange = async {
        let (mut sender, connection) =
            hyper::client::conn::http1::handshake::<_, Full<Bytes>>(TokioIo::new(tls))
                .await
                .map_err(|e| PeerCallError::Unreachable(e.to_string()))?;
        // The connection future drives the socket. It ends when the server closes.
        let driver = tokio::spawn(async move {
            let _ = connection.await;
        });
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header(HOST, addr.to_string());
        if body.is_some() {
            builder = builder.header(CONTENT_TYPE, "application/json");
        }
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let request = builder
            .body(Full::new(Bytes::from(body.unwrap_or_default())))
            .map_err(|e| PeerCallError::Protocol(e.to_string()))?;
        let response = sender
            .send_request(request)
            .await
            .map_err(|e| PeerCallError::Unreachable(e.to_string()))?;
        let status = response.status().as_u16();
        let body = Limited::new(response.into_body(), MAX_REST_BODY_BYTES)
            .collect()
            .await
            .map_err(|_| PeerCallError::Protocol("the peer reply could not be read".to_string()))?
            .to_bytes();
        driver.abort();
        Ok(RawReply { peer, status, body })
    };
    tokio::time::timeout(REQUEST_TIMEOUT, exchange)
        .await
        .map_err(|_| PeerCallError::Unreachable("the request timed out".to_string()))?
}
