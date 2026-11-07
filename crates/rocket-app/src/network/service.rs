//! The network service: lifecycle, settings, addresses and the shared state of one node.
//!
//! One `Mutex<State>` guards the node table, the pairing sessions and the presence of each
//! paired device. A guard is never held across an `.await`.

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

use chrono::Utc;
use rocket_audit::{AuditActor, AuditEventKind, SecurityAuditPublisher};
use rocket_network::limits::{
    MANUAL_PROBE_INTERVAL, MAX_PAIRED_DEVICES, PAIR_REQUESTS_PER_MINUTE, PROTOCOL_VERSION,
};
use rocket_network::{
    default_display_name, initials_for, Announcement, BlockedDevice, BlockedDeviceRepository,
    DeviceId, DeviceIdentityStore, Discovery, DiscoverySink, HelloResponse, ManualAddress,
    ManualAddressRepository, NetworkEvent, NetworkSettings, NetworkSettingsRepository, NodeProfile,
    PairedDeviceRepository, PairingId, PeerConnector, PeerHandler, PeerIdentity, PeerMessage,
    PeerServerControl, PeerTransport, PeerTransportFactory, QuarantineReport, SlidingWindow,
    TransferSink,
};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::EventPublisher;
use tokio::sync::watch;

use crate::stop_signal::run_every;

use super::connections::ConnEntry;
use super::nodes::{Seen, ServiceSink};
use super::pairing::SessionEntry;
use super::status::{
    warning_for, NetworkState, KEYCHAIN_OFF_REASON, NOT_STARTED_REASON, TRANSPORT_OFF_REASON,
};

/// The text shown when the paired list is full.
pub fn paired_limit_message() -> String {
    format!("You've reached the limit of {MAX_PAIRED_DEVICES} paired devices. Remove one to pair.")
}

/// Everything the service needs from outside. All of it is a port.
pub struct NetworkDeps {
    pub identity: Arc<dyn DeviceIdentityStore>,
    pub paired: Arc<dyn PairedDeviceRepository>,
    pub blocked: Arc<dyn BlockedDeviceRepository>,
    pub addresses: Arc<dyn ManualAddressRepository>,
    pub settings: Arc<dyn NetworkSettingsRepository>,
    pub quarantine: Vec<Arc<dyn QuarantineReport>>,
    pub discovery: Arc<dyn Discovery>,
    pub transport: Arc<dyn PeerTransportFactory>,
    pub events: Arc<dyn EventPublisher>,
    pub audit: Arc<dyn SecurityAuditPublisher>,
    pub os_user_name: Option<String>,
}

/// What exists only while the node runs.
pub(super) struct Runtime {
    pub local: PeerIdentity,
    pub connector: Arc<dyn PeerConnector>,
    pub stop: watch::Receiver<bool>,
    pub rt: tokio::runtime::Handle,
}

pub(super) struct State {
    pub net: NetworkState,
    pub port: Option<u16>,
    pub listener_available: bool,
    pub discovery_available: bool,
    pub settings: NetworkSettings,
    pub profile: NodeProfile,
    pub warnings: Vec<String>,
    pub runtime: Option<Runtime>,
    pub seen: HashMap<DeviceId, Seen>,
    pub sessions: HashMap<PairingId, SessionEntry>,
    pub cooldowns: HashMap<DeviceId, Instant>,
    /// The service-level limit on incoming pairing requests.
    pub pair_window: SlidingWindow,
    pub conns: HashMap<DeviceId, ConnEntry>,
}

pub struct NetworkService {
    me: Weak<NetworkService>,
    pub(super) deps: NetworkDeps,
    state: Mutex<State>,
    /// Serialises writes to the paired list that must not undo a removal.
    paired_lock: Mutex<()>,
    /// Receives the transfer frames. It lives outside `State` because `prepare` resets `State`
    /// on every start. The cycle with the transfer service lasts as long as the app.
    pub(super) transfer_sink: Mutex<Option<Arc<dyn TransferSink>>>,
}

/// Keeps the node running. Dropping it, or calling `stop`, ends every task and closes the
/// listener and discovery.
pub struct NetworkHandle {
    stop: watch::Sender<bool>,
    server: Option<Arc<dyn PeerServerControl>>,
    discovery: Option<Arc<dyn Discovery>>,
    stopped: AtomicBool,
}

impl NetworkHandle {
    /// A handle for a node that is off. It owns nothing but the stop signal.
    fn inert(stop: watch::Sender<bool>) -> NetworkHandle {
        NetworkHandle {
            stop,
            server: None,
            discovery: None,
            stopped: AtomicBool::new(false),
        }
    }

    /// Ends the node. Calling it again does nothing.
    pub fn stop(&self) {
        if self.stopped.swap(true, Ordering::SeqCst) {
            return;
        }
        let _ = self.stop.send(true);
        if let Some(server) = &self.server {
            server.stop();
        }
        if let Some(discovery) = &self.discovery {
            if let Err(e) = discovery.withdraw() {
                tracing::debug!(code = "network_withdraw_failed", error = %e, "withdraw failed");
            }
            discovery.stop();
        }
    }
}

impl Drop for NetworkHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The profile for a saved name, else the OS user name. A name that does not validate falls
/// back to "Rocket user", so the node always has a valid profile.
fn profile_for(saved: Option<&str>, os_user: Option<&str>) -> NodeProfile {
    let name = match saved {
        Some(name) => name.to_string(),
        None => default_display_name(os_user),
    };
    NodeProfile::new(&name).unwrap_or_else(|_| {
        let fallback = default_display_name(None);
        NodeProfile {
            initials: initials_for(&fallback),
            display_name: fallback,
        }
    })
}

/// The window of the service-level pairing limit.
pub(super) const PAIR_REQUEST_WINDOW: Duration = Duration::from_secs(60);

impl NetworkService {
    pub fn new(deps: NetworkDeps) -> Arc<NetworkService> {
        let profile = profile_for(None, deps.os_user_name.as_deref());
        let state = State {
            net: NetworkState::Off {
                reason: NOT_STARTED_REASON.to_string(),
            },
            port: None,
            listener_available: true,
            discovery_available: true,
            settings: NetworkSettings::default(),
            profile,
            warnings: Vec::new(),
            runtime: None,
            seen: HashMap::new(),
            sessions: HashMap::new(),
            cooldowns: HashMap::new(),
            pair_window: SlidingWindow::new(PAIR_REQUESTS_PER_MINUTE as usize, PAIR_REQUEST_WINDOW),
            conns: HashMap::new(),
        };
        Arc::new_cyclic(|me| NetworkService {
            me: me.clone(),
            deps,
            state: Mutex::new(state),
            paired_lock: Mutex::new(()),
            transfer_sink: Mutex::new(None),
        })
    }

    pub(super) fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Held while the paired list is changed together with a decision about it.
    pub(super) fn paired_guard(&self) -> MutexGuard<'_, ()> {
        self.paired_lock.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The service as an `Arc`, for tasks that outlive one call.
    pub(super) fn arc(&self) -> Option<Arc<NetworkService>> {
        self.me.upgrade()
    }

    /// The tokio clock as a std instant, so paused time works in tests.
    pub(super) fn now() -> Instant {
        tokio::time::Instant::now().into_std()
    }

    pub(super) fn emit(&self, event: NetworkEvent) {
        self.deps.events.publish(&event);
    }

    pub(super) fn audit(&self, actor: AuditActor, kind: AuditEventKind) {
        self.deps.audit.publish(actor.into(), None, kind);
    }

    /// Runs a task on the node's runtime, or on the current one before `start`.
    pub(super) fn spawn<F>(&self, task: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let handle = self.lock().runtime.as_ref().map(|r| r.rt.clone());
        let handle = handle.or_else(|| tokio::runtime::Handle::try_current().ok());
        if let Some(handle) = handle {
            drop(handle.spawn(task));
        }
    }

    pub(super) fn connector(&self) -> Option<Arc<dyn PeerConnector>> {
        self.lock()
            .runtime
            .as_ref()
            .map(|r| Arc::clone(&r.connector))
    }

    pub(super) fn local_id(&self) -> Option<DeviceId> {
        self.lock().runtime.as_ref().map(|r| r.local.device_id)
    }

    pub(super) fn hello_response(&self) -> HelloResponse {
        let st = self.lock();
        HelloResponse {
            device_id: st
                .runtime
                .as_ref()
                .map(|r| r.local.device_id)
                .unwrap_or_else(|| DeviceId::from_bytes([0; 32])),
            version: PROTOCOL_VERSION,
            profile: st.profile.clone(),
        }
    }

    pub(super) fn local_hello_message(&self) -> PeerMessage {
        PeerMessage::Hello {
            version: PROTOCOL_VERSION,
            profile: self.lock().profile.clone(),
        }
    }

    /// Paired and not blocked. Any read error counts as not paired.
    pub(super) fn paired_and_not_blocked(&self, device: &DeviceId) -> bool {
        let paired = matches!(self.deps.paired.get(device), Ok(Some(_)));
        paired && matches!(self.deps.blocked.contains(device), Ok(false))
    }

    // ----- lifecycle -----

    /// Loads settings and damaged-file warnings, and clears what a previous run left.
    fn prepare(&self) {
        let settings = match self.deps.settings.load() {
            Ok(settings) => settings,
            Err(e) => {
                tracing::warn!(code = "network_settings_unreadable", error = %e, "using defaults");
                NetworkSettings::default()
            }
        };
        // The repositories set a damaged file aside only while they read it, so read each list
        // once before the warnings are collected.
        let _ = self.deps.paired.list();
        let _ = self.deps.blocked.list();
        let _ = self.deps.addresses.list();
        let warnings: Vec<String> = self
            .deps
            .quarantine
            .iter()
            .filter_map(|report| report.take_quarantined())
            .map(|file| warning_for(&file))
            .collect();
        let profile = profile_for(
            settings.display_name.as_deref(),
            self.deps.os_user_name.as_deref(),
        );
        let mut st = self.lock();
        st.settings = settings;
        st.profile = profile;
        st.warnings = warnings;
        st.runtime = None;
        st.port = None;
        st.listener_available = true;
        st.discovery_available = true;
        st.seen.clear();
        st.sessions.clear();
        st.cooldowns.clear();
        st.pair_window.clear();
        st.conns.clear();
    }

    fn set_off(&self, reason: &str) {
        self.lock().net = NetworkState::Off {
            reason: reason.to_string(),
        };
        self.emit(NetworkEvent::StatusChanged);
    }

    /// Starts the node. It never fails: a problem lands in `status()`.
    pub async fn start(self: &Arc<Self>) -> NetworkHandle {
        let (stop_tx, stop_rx) = watch::channel(false);
        let rt = tokio::runtime::Handle::current();
        self.prepare();

        let key = match self.deps.identity.load_or_create() {
            Ok(key) => key,
            Err(e) => {
                tracing::warn!(code = "network_keychain_unavailable", error = %e, "networking is off");
                self.set_off(KEYCHAIN_OFF_REASON);
                return NetworkHandle::inert(stop_tx);
            }
        };
        let built = self.deps.transport.build(&key);
        drop(key);
        let PeerTransport {
            local,
            connector,
            server,
        } = match built {
            Ok(transport) => transport,
            Err(e) => {
                tracing::warn!(code = "network_transport_failed", error = %e, "networking is off");
                self.set_off(TRANSPORT_OFF_REASON);
                return NetworkHandle::inert(stop_tx);
            }
        };
        {
            let mut st = self.lock();
            st.runtime = Some(Runtime {
                local,
                connector,
                stop: stop_rx.clone(),
                rt: rt.clone(),
            });
            st.net = NetworkState::Running;
        }

        let handler: Arc<dyn PeerHandler> = self.clone();
        match server.start(handler).await {
            Ok(port) => {
                let mut st = self.lock();
                st.port = Some(port);
                st.listener_available = true;
            }
            Err(e) => {
                tracing::warn!(code = "network_listener_failed", error = %e, "others cannot connect");
                let mut st = self.lock();
                st.port = None;
                st.listener_available = false;
            }
        }

        let sink: Arc<dyn DiscoverySink> = Arc::new(ServiceSink::new(Arc::downgrade(self), rt));
        let discovery_ok = match self.deps.discovery.start(sink) {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(code = "network_discovery_unavailable", error = %e, "no mDNS");
                false
            }
        };
        self.lock().discovery_available = discovery_ok;
        self.announce();

        // Background work and the final events.
        let sweeper = Arc::clone(self);
        let sweep_stop = stop_rx.clone();
        self.spawn(async move { sweeper.sweep_loop(sweep_stop).await });
        for device in self.deps.paired.list().unwrap_or_default() {
            self.ensure_connection(&device.device_id, false);
        }
        let me = Arc::clone(self);
        let probe_stop = stop_rx.clone();
        self.spawn(async move { me.manual_probe_loop(probe_stop).await });
        self.emit(NetworkEvent::StatusChanged);
        self.emit(NetworkEvent::NodesChanged);

        NetworkHandle {
            stop: stop_tx,
            server: Some(server),
            discovery: Some(Arc::clone(&self.deps.discovery)),
            stopped: AtomicBool::new(false),
        }
    }

    /// Probes once at start, then again after every `MANUAL_PROBE_INTERVAL`, until stopped.
    async fn manual_probe_loop(&self, mut stop: watch::Receiver<bool>) {
        self.probe_manual().await;
        run_every(&mut stop, MANUAL_PROBE_INTERVAL, || self.probe_manual()).await;
    }

    /// Announces this node on mDNS, unless it is hidden, off, without a port or without mDNS.
    pub(super) fn announce(&self) {
        let announcement = {
            let st = self.lock();
            let running = matches!(st.net, NetworkState::Running);
            if !running || st.settings.hidden || !st.discovery_available {
                return;
            }
            let (Some(runtime), Some(port)) = (st.runtime.as_ref(), st.port) else {
                return;
            };
            Announcement {
                device_id: runtime.local.device_id,
                profile: st.profile.clone(),
                port,
                version: PROTOCOL_VERSION,
            }
        };
        if let Err(e) = self.deps.discovery.announce(&announcement) {
            tracing::warn!(code = "network_announce_failed", error = %e, "announce failed");
        }
    }

    fn is_running(&self) -> bool {
        matches!(self.lock().net, NetworkState::Running)
    }

    // ----- settings -----

    /// Applies `change` to a copy of the settings, saves it, then keeps it in memory.
    fn update_settings(
        &self,
        change: impl FnOnce(&mut NetworkSettings),
    ) -> DomainResult<NetworkSettings> {
        let mut settings = self.lock().settings.clone();
        change(&mut settings);
        self.deps.settings.save(&settings)?;
        self.lock().settings = settings.clone();
        Ok(settings)
    }

    pub async fn set_hidden(self: &Arc<Self>, hidden: bool) -> DomainResult<()> {
        self.update_settings(|s| s.hidden = hidden)?;
        if self.is_running() {
            if hidden {
                if let Err(e) = self.deps.discovery.withdraw() {
                    tracing::warn!(code = "network_withdraw_failed", error = %e, "withdraw failed");
                }
            } else {
                self.announce();
            }
        }
        self.emit(NetworkEvent::StatusChanged);
        Ok(())
    }

    pub async fn set_display_name(self: &Arc<Self>, name: &str) -> DomainResult<()> {
        let profile = NodeProfile::new(name)?;
        let saved = profile.display_name.clone();
        self.update_settings(|s| s.display_name = Some(saved))?;
        self.lock().profile = profile;
        self.announce();
        self.emit(NetworkEvent::StatusChanged);
        Ok(())
    }

    pub fn mark_intro_seen(&self) -> DomainResult<()> {
        self.update_settings(|s| s.intro_seen = true)?;
        self.emit(NetworkEvent::StatusChanged);
        Ok(())
    }

    // ----- manual addresses and the block list -----

    pub fn list_addresses(&self) -> DomainResult<Vec<ManualAddress>> {
        self.deps.addresses.list()
    }

    pub async fn add_address(self: &Arc<Self>, host: &str, port: u16) -> DomainResult<()> {
        let host = host.trim();
        let bad_host = host.is_empty()
            || host.len() > 253
            || host
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || c == '/' || c == '@');
        if bad_host || port == 0 {
            return Err(DomainError::InvalidInput(
                "Enter a host name or IP address and a port.".to_string(),
            ));
        }
        self.deps.addresses.add(ManualAddress {
            host: host.to_string(),
            port,
            added_at: Utc::now(),
        })?;
        self.probe_manual().await;
        Ok(())
    }

    pub fn remove_address(&self, host: &str, port: u16) -> DomainResult<()> {
        self.deps.addresses.remove(host.trim(), port)?;
        if let Some(me) = self.arc() {
            self.spawn(async move { me.probe_manual().await });
        }
        Ok(())
    }

    pub fn list_blocked(&self) -> DomainResult<Vec<BlockedDevice>> {
        self.deps.blocked.list()
    }

    pub fn unblock(&self, device_id: &DeviceId) -> DomainResult<()> {
        self.deps.blocked.remove(device_id)?;
        // The Nearby list may show this device again, and Settings lists the blocked ones.
        self.emit(NetworkEvent::NodesChanged);
        self.emit(NetworkEvent::StatusChanged);
        Ok(())
    }
}
