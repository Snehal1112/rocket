//! The node table, the node list for the panel, the discovery sink and manual probes.
//!
//! A node is listed only after `/hello` proves it: the certificate key hashes to the id that
//! was announced, and the `HelloResponse` says the same id. The first address that answers
//! decides. A wrong answer ends the search, so a stranger cannot borrow another node's id.

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Weak};
use std::time::Duration;

use rocket_network::limits::PROTOCOL_VERSION;
use rocket_network::{
    initials_for, DeviceId, DiscoveryEvent, DiscoverySink, DiscoverySource, HelloResponse,
    NetworkEvent, NodeProfile, PeerConnector, Presence,
};
use tokio::runtime::Handle;

use super::service::NetworkService;
use super::status::NetworkState;

const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_ADDRESSES_PER_NODE: usize = 8;

/// What the node table knows about one verified node.
pub(super) struct Seen {
    pub profile: NodeProfile,
    pub version: u32,
    /// Addresses from mDNS, the one that answered `/hello` first.
    pub mdns: Vec<SocketAddr>,
    /// Addresses from the saved manual list that answered `/hello`.
    pub manual: Vec<SocketAddr>,
}

impl Seen {
    pub(super) fn source(&self) -> DiscoverySource {
        if self.mdns.is_empty() {
            DiscoverySource::Manual
        } else {
            DiscoverySource::Mdns
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeGroup {
    Paired,
    Nearby,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeStatus {
    Online,
    Offline,
    Pairing,
    NeedsUpdate,
    Available,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeView {
    pub device_id: String,
    pub short_id: String,
    pub display_name: String,
    pub initials: String,
    pub group: NodeGroup,
    pub status: NodeStatus,
    pub source: Option<DiscoverySource>,
}

/// Receives mDNS events on the mDNS thread and hands them to the runtime.
pub(super) struct ServiceSink {
    svc: Weak<NetworkService>,
    rt: Handle,
}

impl ServiceSink {
    pub(super) fn new(svc: Weak<NetworkService>, rt: Handle) -> ServiceSink {
        ServiceSink { svc, rt }
    }
}

impl DiscoverySink for ServiceSink {
    fn on_event(&self, event: DiscoveryEvent) {
        let Some(svc) = self.svc.upgrade() else {
            return;
        };
        drop(
            self.rt
                .spawn(async move { svc.handle_discovery(event).await }),
        );
    }
}

/// A stable text of the table, to tell whether a change is worth an event.
fn signature(seen: &HashMap<DeviceId, Seen>) -> Vec<String> {
    let mut rows: Vec<String> = seen
        .iter()
        .map(|(id, s)| {
            format!(
                "{}|{}|{}|{:?}|{:?}",
                id.to_base32(),
                s.profile.display_name,
                s.version,
                s.mdns,
                s.manual
            )
        })
        .collect();
    rows.sort();
    rows
}

/// An IP literal needs no lookup. A name (for example a Tailscale name) is resolved.
async fn resolve(host: &str, port: u16) -> Option<SocketAddr> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Some(SocketAddr::new(ip, port));
    }
    let mut found = tokio::net::lookup_host((host, port)).await.ok()?;
    found.next()
}

fn group_rank(group: NodeGroup) -> u8 {
    match group {
        NodeGroup::Paired => 0,
        NodeGroup::Nearby => 1,
    }
}

fn status_rank(status: NodeStatus) -> u8 {
    match status {
        NodeStatus::Online => 0,
        NodeStatus::Pairing => 1,
        NodeStatus::NeedsUpdate => 2,
        NodeStatus::Available => 3,
        NodeStatus::Offline => 4,
    }
}

impl NetworkService {
    /// Calls `/hello` on the addresses in order. The first one that answers decides: it must
    /// be consistent (key and body agree, and the claimed id matches when there is one), or
    /// the result is `None`. The returned profile is re-validated.
    pub(super) async fn verify_hello(
        &self,
        connector: &Arc<dyn PeerConnector>,
        addresses: &[SocketAddr],
        claimed: Option<&DeviceId>,
    ) -> Option<(SocketAddr, HelloResponse)> {
        for address in addresses {
            let Ok(Ok((identity, hello))) =
                tokio::time::timeout(HELLO_TIMEOUT, connector.hello(*address)).await
            else {
                continue;
            };
            let consistent = identity.device_id == hello.device_id
                && claimed.is_none_or(|c| *c == hello.device_id);
            if !consistent {
                return None;
            }
            let profile = hello.profile.validated().ok()?;
            return Some((*address, HelloResponse { profile, ..hello }));
        }
        None
    }

    pub(super) async fn handle_discovery(&self, event: DiscoveryEvent) {
        match event {
            DiscoveryEvent::Seen {
                claimed_id,
                addresses,
                ..
            } => self.on_seen(claimed_id, addresses).await,
            DiscoveryEvent::Gone { claimed_id } => self.on_gone(claimed_id),
        }
    }

    async fn on_seen(&self, claimed: DeviceId, addresses: Vec<SocketAddr>) {
        if self.local_id() == Some(claimed) {
            return;
        }
        let Some(connector) = self.connector() else {
            return;
        };
        let Some((verified, hello)) = self
            .verify_hello(&connector, &addresses, Some(&claimed))
            .await
        else {
            tracing::debug!(
                code = "network_sighting_unverified",
                id = %claimed.short(),
                "a sighting did not verify"
            );
            return;
        };
        let changed = {
            let mut st = self.lock();
            let before = signature(&st.seen);
            let entry = st.seen.entry(claimed).or_insert_with(|| Seen {
                profile: hello.profile.clone(),
                version: hello.version,
                mdns: Vec::new(),
                manual: Vec::new(),
            });
            entry.profile = hello.profile.clone();
            entry.version = hello.version;
            let mut merged = vec![verified];
            for known in &entry.mdns {
                if !merged.contains(known) {
                    merged.push(*known);
                }
            }
            merged.truncate(MAX_ADDRESSES_PER_NODE);
            entry.mdns = merged;
            before != signature(&st.seen)
        };
        // A new sighting means "retry at once" for a paired device.
        self.wake_device(&claimed);
        if changed {
            self.emit(NetworkEvent::NodesChanged);
        }
    }

    fn on_gone(&self, claimed: DeviceId) {
        let changed = {
            let mut st = self.lock();
            let drop_row = match st.seen.get_mut(&claimed) {
                Some(seen) => {
                    seen.mdns.clear();
                    Some(seen.manual.is_empty())
                }
                None => None,
            };
            match drop_row {
                Some(true) => {
                    st.seen.remove(&claimed);
                    true
                }
                Some(false) => true,
                None => false,
            }
        };
        if changed {
            self.emit(NetworkEvent::NodesChanged);
        }
    }

    /// Probes every saved address and replaces the manual addresses in the table.
    pub(super) async fn probe_manual(&self) {
        let Some(connector) = self.connector() else {
            return;
        };
        let entries = match self.deps.addresses.list() {
            Ok(entries) => entries,
            Err(e) => {
                tracing::warn!(code = "network_addresses_unreadable", error = %e, "probe skipped");
                return;
            }
        };
        let mut found: Vec<(SocketAddr, HelloResponse)> = Vec::new();
        for entry in entries {
            let Some(address) = resolve(&entry.host, entry.port).await else {
                continue;
            };
            if let Some(hit) = self.verify_hello(&connector, &[address], None).await {
                found.push(hit);
            }
        }
        let local = self.local_id();
        let (changed, woken) = {
            let mut st = self.lock();
            let before = signature(&st.seen);
            for seen in st.seen.values_mut() {
                seen.manual.clear();
            }
            let mut woken = Vec::new();
            for (address, hello) in found {
                if Some(hello.device_id) == local {
                    continue;
                }
                let entry = st.seen.entry(hello.device_id).or_insert_with(|| Seen {
                    profile: hello.profile.clone(),
                    version: hello.version,
                    mdns: Vec::new(),
                    manual: Vec::new(),
                });
                entry.profile = hello.profile.clone();
                entry.version = hello.version;
                if !entry.manual.contains(&address) {
                    entry.manual.push(address);
                }
                woken.push(hello.device_id);
            }
            st.seen
                .retain(|_, seen| !(seen.mdns.is_empty() && seen.manual.is_empty()));
            (before != signature(&st.seen), woken)
        };
        for id in woken {
            self.wake_device(&id);
        }
        if changed {
            self.emit(NetworkEvent::NodesChanged);
        }
    }

    /// The name to show for a device: the live name, else the one saved at pairing.
    pub(super) fn known_name(&self, id: &DeviceId) -> String {
        let live = self
            .lock()
            .seen
            .get(id)
            .map(|s| s.profile.display_name.clone());
        if let Some(name) = live {
            return name;
        }
        match self.deps.paired.get(id) {
            Ok(Some(device)) => device.name_at_pairing,
            _ => "Unknown device".to_string(),
        }
    }

    #[cfg(test)]
    pub(crate) fn mdns_addresses(&self, id: &DeviceId) -> Vec<SocketAddr> {
        self.lock()
            .seen
            .get(id)
            .map(|s| s.mdns.clone())
            .unwrap_or_default()
    }

    pub fn list_nodes(&self) -> Vec<NodeView> {
        let paired = self.deps.paired.list().unwrap_or_default();
        let blocked: HashSet<DeviceId> = self
            .deps
            .blocked
            .list()
            .unwrap_or_default()
            .into_iter()
            .map(|b| b.device_id)
            .collect();
        let st = self.lock();
        if !matches!(st.net, NetworkState::Running) {
            return Vec::new();
        }
        let local = st.runtime.as_ref().map(|r| r.local.device_id);
        let pairing: HashSet<DeviceId> = st
            .sessions
            .values()
            .filter(|e| !e.session.is_terminal())
            .map(|e| e.session.peer().device_id)
            .collect();
        let mut rows = Vec::new();
        let mut paired_ids = HashSet::new();
        for device in &paired {
            paired_ids.insert(device.device_id);
            if Some(device.device_id) == local {
                continue;
            }
            let seen = st.seen.get(&device.device_id);
            let conn = st.conns.get(&device.device_id);
            let name = seen
                .map(|s| s.profile.display_name.clone())
                .unwrap_or_else(|| device.name_at_pairing.clone());
            let status = if pairing.contains(&device.device_id) {
                NodeStatus::Pairing
            } else if seen.is_some_and(|s| s.version != PROTOCOL_VERSION)
                || conn.is_some_and(|c| c.needs_update)
            {
                NodeStatus::NeedsUpdate
            } else if conn.is_some_and(|c| c.presence.state() == Presence::Online) {
                NodeStatus::Online
            } else {
                NodeStatus::Offline
            };
            rows.push(NodeView {
                device_id: device.device_id.to_base32(),
                short_id: device.device_id.short(),
                initials: initials_for(&name),
                display_name: name,
                group: NodeGroup::Paired,
                status,
                source: seen.map(Seen::source),
            });
        }
        for (id, seen) in &st.seen {
            if paired_ids.contains(id) || blocked.contains(id) || Some(*id) == local {
                continue;
            }
            let status = if pairing.contains(id) {
                NodeStatus::Pairing
            } else if seen.version != PROTOCOL_VERSION {
                NodeStatus::NeedsUpdate
            } else {
                NodeStatus::Available
            };
            rows.push(NodeView {
                device_id: id.to_base32(),
                short_id: id.short(),
                display_name: seen.profile.display_name.clone(),
                initials: seen.profile.initials.clone(),
                group: NodeGroup::Nearby,
                status,
                source: Some(seen.source()),
            });
        }
        rows.sort_by(|a, b| {
            group_rank(a.group)
                .cmp(&group_rank(b.group))
                .then(status_rank(a.status).cmp(&status_rank(b.status)))
                .then_with(|| {
                    a.display_name
                        .to_lowercase()
                        .cmp(&b.display_name.to_lowercase())
                })
                .then_with(|| a.device_id.cmp(&b.device_id))
        });
        rows
    }
}
