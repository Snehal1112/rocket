//! Builds the peer transport from the device key. One TLS identity is shared by the server
//! and the connector, so both directions show the same certificate.

use std::sync::Arc;

use rocket_network::{DeviceKeyMaterial, PeerTransport, PeerTransportFactory};
use rocket_shared::error::DomainResult;

use crate::network::{build_tls_identity, RustlsPeerConnector, RustlsPeerServer, TlsIdentity};

#[derive(Debug, Default, Clone, Copy)]
pub struct RustlsPeerTransportFactory {
    /// Servers bind 127.0.0.1 on a port the OS picks. Only end-to-end tests set it.
    #[cfg(any(test, feature = "test-support"))]
    local_test: bool,
}

impl RustlsPeerTransportFactory {
    pub fn new() -> Self {
        RustlsPeerTransportFactory::default()
    }

    /// A factory whose servers listen on loopback with an OS port, so two nodes run side by
    /// side in one test without mDNS (Plan 04b).
    #[cfg(any(test, feature = "test-support"))]
    pub fn local_test() -> Self {
        RustlsPeerTransportFactory { local_test: true }
    }

    fn server_for(&self, identity: Arc<TlsIdentity>) -> RustlsPeerServer {
        #[cfg(any(test, feature = "test-support"))]
        if self.local_test {
            return RustlsPeerServer::local_test(identity);
        }
        RustlsPeerServer::new(identity)
    }
}

impl PeerTransportFactory for RustlsPeerTransportFactory {
    fn build(&self, key: &DeviceKeyMaterial) -> DomainResult<PeerTransport> {
        let identity = Arc::new(build_tls_identity(key)?);
        Ok(PeerTransport {
            local: identity.local.clone(),
            connector: Arc::new(RustlsPeerConnector::new(Arc::clone(&identity))),
            server: Arc::new(self.server_for(identity)),
        })
    }
}

#[cfg(test)]
mod tests {
    use rocket_network::limits::PROTOCOL_VERSION;
    use rocket_network::{DeviceKeyMaterial, PairRequest, PeerConnector, PeerTransportFactory};

    use super::*;
    use crate::network::peer_test_support::{profile, start_node};
    use crate::network::tls_identity::test_support::key_material;

    #[test]
    fn the_same_key_gives_the_same_local_identity() {
        let factory = RustlsPeerTransportFactory::new();
        let key = key_material();

        let a = factory.build(&key).expect("build a");
        let b = factory.build(&key).expect("build b");
        let other = factory.build(&key_material()).expect("build other");

        assert_eq!(a.local, b.local);
        assert_ne!(a.local.device_id, other.local.device_id);
    }

    #[test]
    fn a_broken_key_is_an_error() {
        let broken = DeviceKeyMaterial {
            pkcs8_der: vec![1, 2, 3],
        };
        assert!(RustlsPeerTransportFactory::new().build(&broken).is_err());
    }

    #[tokio::test]
    async fn the_built_connector_presents_the_built_identity() {
        let node = start_node("Hub").await;
        let transport = RustlsPeerTransportFactory::new()
            .build(&key_material())
            .expect("build");

        let (peer, _) = transport.connector.hello(node.addr).await.expect("hello");
        assert_eq!(peer.device_id, node.id());
        let request = PairRequest {
            version: PROTOCOL_VERSION,
            profile: profile("Bob"),
        };
        transport
            .connector
            .pair_request(node.addr, &node.id(), &request)
            .await
            .expect("pair request");

        let expected = format!(
            "pair_request:{}:127.0.0.1:Bob",
            transport.local.device_id.short()
        );
        assert!(node.handler.calls().contains(&expected));
    }

    #[tokio::test]
    async fn the_local_test_factory_listens_on_loopback() {
        use rocket_network::PeerHandler;
        use std::net::SocketAddr;

        use crate::network::peer_test_support::{connector, identity, FakeHandler};

        let key = key_material();
        let transport = RustlsPeerTransportFactory::local_test()
            .build(&key)
            .expect("build");
        // The handler answers hello with the same key the server shows.
        let tls = Arc::new(build_tls_identity(&key).expect("identity"));
        let (handler, _sockets) = FakeHandler::new(&tls, "Hub");
        let as_handler: Arc<dyn PeerHandler> = handler;

        let port = transport.server.start(as_handler).await.expect("start");
        assert_ne!(port, 0);
        let (peer, _) = connector(&identity())
            .hello(SocketAddr::from(([127, 0, 0, 1], port)))
            .await
            .expect("hello on loopback");
        assert_eq!(peer.device_id, transport.local.device_id);
        transport.server.stop();
    }
}
