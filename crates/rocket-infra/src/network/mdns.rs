//! mDNS discovery on `mdns-sd`. Announces this node as `_rocket._tcp.local.` and reports
//! other nodes. TXT ids are only claims: Plan 04 confirms each with `/hello` before listing it.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use mdns_sd::{RecvTimeoutError, ServiceDaemon, ServiceEvent, ServiceInfo};
use rocket_network::limits::MDNS_SERVICE_TYPE;
use rocket_network::{
    Announcement, DeviceId, Discovery, DiscoveryEvent, DiscoverySink, NodeProfile,
};
use rocket_shared::error::{DomainError, DomainResult};

use crate::network::lock::lock_ignore_poison;

const TXT_VERSION: &str = "v";
const TXT_ID: &str = "id";
const TXT_NAME: &str = "name";
const TXT_INITIALS: &str = "initials";
/// How often the browse thread checks its stop flag.
const BROWSE_POLL: Duration = Duration::from_millis(500);
/// How long `stop` waits for the goodbye packet to go out.
const GOODBYE_WAIT: Duration = Duration::from_millis(500);
/// A flood of fake records cannot grow the name table past this.
pub(crate) const MAX_KNOWN_INSTANCES: usize = 1024;

pub(crate) fn instance_name(id: &DeviceId) -> String {
    id.short()
}

pub(crate) fn host_name(id: &DeviceId) -> String {
    format!(
        "rocket-{}.local.",
        id.short().replace('-', "").to_ascii_lowercase()
    )
}

/// The TXT record of spec 7.1. Nothing else is announced.
pub(crate) fn txt_record(announcement: &Announcement) -> Vec<(&'static str, String)> {
    vec![
        (TXT_VERSION, announcement.version.to_string()),
        (TXT_ID, announcement.device_id.to_base32()),
        (TXT_NAME, announcement.profile.display_name.clone()),
        (TXT_INITIALS, announcement.profile.initials.clone()),
    ]
}

/// Reads a TXT record. `None` when the version or id does not parse or the name is not a
/// valid display name. The initials are rebuilt from the name, never taken from the record.
pub(crate) fn parse_txt(
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Option<(DeviceId, NodeProfile, u32)> {
    let version = lookup(TXT_VERSION)?.parse::<u32>().ok()?;
    let id = DeviceId::parse_base32(&lookup(TXT_ID)?).ok()?;
    let profile = NodeProfile::new(&lookup(TXT_NAME)?).ok()?;
    Some((id, profile, version))
}

/// Addresses a peer can be dialled on, sorted and without duplicates. IPv6 link-local needs a
/// scope id that the record does not give, so it is skipped.
pub(crate) fn dialable_addresses(ips: &[IpAddr], port: u16) -> Vec<SocketAddr> {
    if port == 0 {
        return Vec::new();
    }
    let mut addresses: Vec<SocketAddr> = ips
        .iter()
        .copied()
        .filter(|ip| !ip.is_unspecified() && !ip.is_multicast() && !is_ipv6_link_local(ip))
        .map(|ip| SocketAddr::new(ip, port))
        .collect();
    addresses.sort();
    addresses.dedup();
    addresses
}

fn is_ipv6_link_local(ip: &IpAddr) -> bool {
    matches!(ip, IpAddr::V6(v6) if (v6.segments()[0] & 0xffc0) == 0xfe80)
}

/// Remembers which instance name stands for which claimed id, because a goodbye packet only
/// carries the name.
#[derive(Default)]
pub(crate) struct BrowseState {
    known: HashMap<String, DeviceId>,
}

impl BrowseState {
    pub(crate) fn resolved(
        &mut self,
        fullname: &str,
        port: u16,
        ips: &[IpAddr],
        lookup: &dyn Fn(&str) -> Option<String>,
        own: Option<DeviceId>,
    ) -> Option<DiscoveryEvent> {
        let (claimed_id, profile, version) = parse_txt(lookup)?;
        // The instance name must be the short form of the claimed id.
        if !fullname.starts_with(&format!("{}.", instance_name(&claimed_id))) {
            return None;
        }
        if own == Some(claimed_id) {
            return None;
        }
        let addresses = dialable_addresses(ips, port);
        if addresses.is_empty() {
            return None;
        }
        if !self.known.contains_key(fullname) && self.known.len() >= MAX_KNOWN_INSTANCES {
            return None;
        }
        self.known.insert(fullname.to_string(), claimed_id);
        Some(DiscoveryEvent::Seen {
            claimed_id,
            profile,
            addresses,
            version,
        })
    }

    pub(crate) fn removed(&mut self, fullname: &str) -> Option<DiscoveryEvent> {
        self.known
            .remove(fullname)
            .map(|claimed_id| DiscoveryEvent::Gone { claimed_id })
    }
}

#[derive(Default)]
struct MdnsInner {
    daemon: Option<ServiceDaemon>,
    announced: Option<String>,
    browse_stop: Option<Arc<AtomicBool>>,
}

impl MdnsInner {
    /// The daemon is made on first use, so `MdnsDiscovery::new()` cannot fail.
    fn daemon(&mut self) -> DomainResult<ServiceDaemon> {
        if let Some(daemon) = &self.daemon {
            return Ok(daemon.clone());
        }
        let daemon = ServiceDaemon::new().map_err(mdns_error)?;
        self.daemon = Some(daemon.clone());
        Ok(daemon)
    }
}

pub struct MdnsDiscovery {
    inner: Mutex<MdnsInner>,
    /// The announced id, so our own record is never reported.
    own: Arc<Mutex<Option<DeviceId>>>,
}

impl MdnsDiscovery {
    pub fn new() -> Self {
        MdnsDiscovery {
            inner: Mutex::new(MdnsInner::default()),
            own: Arc::new(Mutex::new(None)),
        }
    }

    fn lock(&self) -> MutexGuard<'_, MdnsInner> {
        lock_ignore_poison(&self.inner)
    }
}

impl Default for MdnsDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

fn mdns_error(error: mdns_sd::Error) -> DomainError {
    DomainError::Internal(format!("mDNS is not available: {error}"))
}

impl Discovery for MdnsDiscovery {
    fn start(&self, sink: Arc<dyn DiscoverySink>) -> DomainResult<()> {
        let mut inner = self.lock();
        let daemon = inner.daemon()?;
        if let Some(previous) = inner.browse_stop.take() {
            previous.store(true, Ordering::SeqCst);
            let _ = daemon.stop_browse(MDNS_SERVICE_TYPE);
        }
        let events = daemon.browse(MDNS_SERVICE_TYPE).map_err(mdns_error)?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let own = Arc::clone(&self.own);
        std::thread::Builder::new()
            .name("rocket-mdns-browse".to_string())
            .spawn(move || browse_loop(events, sink, own, thread_stop))
            .map_err(|e| DomainError::Internal(format!("start the mDNS browser: {e}")))?;
        inner.browse_stop = Some(stop);
        Ok(())
    }

    fn announce(&self, announcement: &Announcement) -> DomainResult<()> {
        let mut inner = self.lock();
        let daemon = inner.daemon()?;
        if let Some(previous) = inner.announced.take() {
            let _ = daemon.unregister(&previous);
        }
        let properties = txt_record(announcement);
        let info = ServiceInfo::new(
            MDNS_SERVICE_TYPE,
            &instance_name(&announcement.device_id),
            &host_name(&announcement.device_id),
            (),
            announcement.port,
            &properties[..],
        )
        .map_err(mdns_error)?
        .enable_addr_auto();
        let fullname = info.get_fullname().to_string();
        daemon.register(info).map_err(mdns_error)?;
        inner.announced = Some(fullname);
        *lock_ignore_poison(&self.own) = Some(announcement.device_id);
        Ok(())
    }

    fn withdraw(&self) -> DomainResult<()> {
        let mut inner = self.lock();
        let Some(fullname) = inner.announced.take() else {
            return Ok(());
        };
        let Some(daemon) = inner.daemon.clone() else {
            return Ok(());
        };
        daemon.unregister(&fullname).map(|_| ()).map_err(mdns_error)
    }

    fn stop(&self) {
        let mut inner = self.lock();
        if let Some(stop) = inner.browse_stop.take() {
            stop.store(true, Ordering::SeqCst);
        }
        let Some(daemon) = inner.daemon.take() else {
            return;
        };
        if let Some(fullname) = inner.announced.take() {
            // Send the goodbye before the daemon exits, so peers drop us at once.
            if let Ok(done) = daemon.unregister(&fullname) {
                let _ = done.recv_timeout(GOODBYE_WAIT);
            }
        }
        let _ = daemon.shutdown();
    }
}

/// Runs on its own thread: maps daemon events and hands them to the sink.
fn browse_loop(
    events: mdns_sd::Receiver<ServiceEvent>,
    sink: Arc<dyn DiscoverySink>,
    own: Arc<Mutex<Option<DeviceId>>>,
    stop: Arc<AtomicBool>,
) {
    let mut state = BrowseState::default();
    while !stop.load(Ordering::SeqCst) {
        let event = match events.recv_timeout(BROWSE_POLL) {
            Ok(event) => event,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        let own_id = *lock_ignore_poison(&own);
        let mapped = match event {
            ServiceEvent::ServiceResolved(service) => {
                let ips: Vec<IpAddr> = service
                    .get_addresses()
                    .iter()
                    .map(|ip| ip.to_ip_addr())
                    .collect();
                let lookup = |key: &str| service.get_property_val_str(key).map(str::to_string);
                state.resolved(service.get_fullname(), service.port, &ips, &lookup, own_id)
            }
            ServiceEvent::ServiceRemoved(_, fullname) => state.removed(&fullname),
            _ => None,
        };
        if let Some(event) = mapped {
            sink.on_event(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use rocket_network::limits::MDNS_SERVICE_TYPE;
    use rocket_network::{Announcement, DeviceId, Discovery, DiscoveryEvent, DiscoverySink};

    use super::*;
    use crate::network::peer_test_support::profile;

    fn device(n: u8) -> DeviceId {
        DeviceId::from_bytes([n; 32])
    }

    fn announcement(id: DeviceId) -> Announcement {
        Announcement {
            device_id: id,
            profile: profile("Ada Lovelace"),
            port: 47821,
            version: 1,
        }
    }

    fn lookup_from(pairs: Vec<(&'static str, String)>) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> =
            pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        move |key: &str| map.get(key).cloned()
    }

    fn fullname(id: &DeviceId) -> String {
        format!("{}.{}", instance_name(id), MDNS_SERVICE_TYPE)
    }

    fn lan() -> Vec<IpAddr> {
        vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20))]
    }

    #[test]
    fn txt_record_holds_exactly_v_id_name_and_initials() {
        let id = device(1);
        let txt = txt_record(&announcement(id));
        assert_eq!(
            txt,
            vec![
                ("v", "1".to_string()),
                ("id", id.to_base32()),
                ("name", "Ada Lovelace".to_string()),
                ("initials", "AL".to_string()),
            ]
        );
    }

    #[test]
    fn txt_record_round_trips_through_parse_txt() {
        let id = device(2);
        let lookup = lookup_from(txt_record(&announcement(id)));
        let (parsed_id, parsed_profile, version) = parse_txt(&lookup).expect("parses");
        assert_eq!(parsed_id, id);
        assert_eq!(parsed_profile, profile("Ada Lovelace"));
        assert_eq!(version, 1);
    }

    #[test]
    fn parse_txt_refuses_a_bad_version_id_or_name() {
        let good = device(3).to_base32();
        let cases: Vec<Vec<(&'static str, String)>> = vec![
            vec![("id", good.clone()), ("name", "Ada".into())],
            vec![
                ("v", "x".into()),
                ("id", good.clone()),
                ("name", "Ada".into()),
            ],
            vec![
                ("v", "1".into()),
                ("id", "NOTANID".into()),
                ("name", "Ada".into()),
            ],
            vec![
                ("v", "1".into()),
                ("id", good.to_lowercase()),
                ("name", "Ada".into()),
            ],
            vec![("v", "1".into()), ("name", "Ada".into())],
            vec![("v", "1".into()), ("id", good.clone()), ("name", "".into())],
            vec![
                ("v", "1".into()),
                ("id", good.clone()),
                ("name", "bad\u{7}name".into()),
            ],
            vec![
                ("v", "1".into()),
                ("id", good.clone()),
                ("name", "x".repeat(41)),
            ],
            vec![("v", "1".into()), ("id", good.clone())],
        ];
        for (i, pairs) in cases.into_iter().enumerate() {
            let lookup = lookup_from(pairs);
            assert!(parse_txt(&lookup).is_none(), "case {i}");
        }
    }

    #[test]
    fn forged_initials_are_rebuilt_from_the_name() {
        let id = device(4);
        let lookup = lookup_from(vec![
            ("v", "1".into()),
            ("id", id.to_base32()),
            ("name", "Ada Lovelace".into()),
            ("initials", "ZZ".into()),
        ]);
        let (_, parsed_profile, _) = parse_txt(&lookup).expect("parses");
        assert_eq!(parsed_profile.initials, "AL");
    }

    #[test]
    fn a_resolved_record_is_seen_with_its_dialable_addresses() {
        let id = device(5);
        let lookup = lookup_from(txt_record(&announcement(id)));
        let ips = vec![
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            IpAddr::V6("fe80::1".parse::<Ipv6Addr>().expect("ipv6")),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20)),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20)),
            IpAddr::V6("2001:db8::5".parse::<Ipv6Addr>().expect("ipv6")),
            IpAddr::V4(Ipv4Addr::new(224, 0, 0, 251)),
        ];
        let mut state = BrowseState::default();

        let event = state.resolved(&fullname(&id), 47822, &ips, &lookup, None);

        assert_eq!(
            event,
            Some(DiscoveryEvent::Seen {
                claimed_id: id,
                profile: profile("Ada Lovelace"),
                addresses: vec![
                    "192.168.1.20:47822".parse::<SocketAddr>().expect("addr"),
                    "[2001:db8::5]:47822".parse::<SocketAddr>().expect("addr"),
                ],
                version: 1,
            })
        );
    }

    #[test]
    fn an_instance_name_that_does_not_match_the_id_is_ignored() {
        let id = device(6);
        let lookup = lookup_from(txt_record(&announcement(id)));
        let mut state = BrowseState::default();
        let spoofed = fullname(&device(7));
        assert_eq!(state.resolved(&spoofed, 47821, &lan(), &lookup, None), None);
    }

    #[test]
    fn our_own_record_is_ignored() {
        let id = device(8);
        let lookup = lookup_from(txt_record(&announcement(id)));
        let mut state = BrowseState::default();
        assert_eq!(
            state.resolved(&fullname(&id), 47821, &lan(), &lookup, Some(id)),
            None
        );
    }

    #[test]
    fn port_zero_or_no_dialable_address_is_ignored() {
        let id = device(9);
        let lookup = lookup_from(txt_record(&announcement(id)));
        let mut state = BrowseState::default();
        assert_eq!(
            state.resolved(&fullname(&id), 0, &lan(), &lookup, None),
            None
        );
        let link_local = vec![IpAddr::V6("fe80::2".parse::<Ipv6Addr>().expect("ipv6"))];
        assert_eq!(
            state.resolved(&fullname(&id), 47821, &link_local, &lookup, None),
            None
        );
    }

    #[test]
    fn a_goodbye_is_gone_only_for_a_known_name() {
        let id = device(10);
        let lookup = lookup_from(txt_record(&announcement(id)));
        let mut state = BrowseState::default();
        assert_eq!(state.removed(&fullname(&id)), None, "never seen");

        state.resolved(&fullname(&id), 47821, &lan(), &lookup, None);
        assert_eq!(
            state.removed(&fullname(&id)),
            Some(DiscoveryEvent::Gone { claimed_id: id })
        );
        assert_eq!(state.removed(&fullname(&id)), None, "reported once");
    }

    #[test]
    fn the_name_table_is_capped_but_known_names_still_update() {
        let mut state = BrowseState::default();
        let mut first = None;
        for i in 0..MAX_KNOWN_INSTANCES {
            let mut bytes = [0u8; 32];
            bytes[0] = (i & 0xff) as u8;
            bytes[1] = (i >> 8) as u8;
            let id = DeviceId::from_bytes(bytes);
            first.get_or_insert(id);
            let lookup = lookup_from(txt_record(&announcement(id)));
            assert!(state
                .resolved(&fullname(&id), 47821, &lan(), &lookup, None)
                .is_some());
        }
        let extra = device(250);
        let lookup = lookup_from(txt_record(&announcement(extra)));
        assert_eq!(
            state.resolved(&fullname(&extra), 47821, &lan(), &lookup, None),
            None
        );

        let known = first.expect("one id");
        let lookup = lookup_from(txt_record(&announcement(known)));
        assert!(state
            .resolved(&fullname(&known), 47821, &lan(), &lookup, None)
            .is_some());
    }

    #[test]
    fn instance_and_host_names_come_from_the_short_id() {
        let id = device(11);
        let short = id.short();
        assert_eq!(instance_name(&id), short);
        assert_eq!(
            host_name(&id),
            format!(
                "rocket-{}.local.",
                short.replace('-', "").to_ascii_lowercase()
            )
        );
    }

    #[test]
    fn withdraw_and_stop_before_start_are_harmless() {
        let discovery = MdnsDiscovery::new();
        discovery.withdraw().expect("nothing to withdraw");
        discovery.stop();
        discovery.stop();
    }

    struct ChannelSink(Mutex<std::sync::mpsc::Sender<DiscoveryEvent>>);

    impl DiscoverySink for ChannelSink {
        fn on_event(&self, event: DiscoveryEvent) {
            let _ = lock_ignore_poison(&self.0).send(event);
        }
    }

    fn wait_for(
        events: &std::sync::mpsc::Receiver<DiscoveryEvent>,
        wanted: impl Fn(&DiscoveryEvent) -> bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match events.recv_timeout(left) {
                Ok(event) if wanted(&event) => return,
                Ok(_) => continue,
                Err(_) => panic!("the event did not arrive within 10 seconds"),
            }
        }
    }

    #[test]
    #[ignore = "needs a network with multicast; manual check 1 covers it where CI has none"]
    fn two_daemons_see_each_other_over_multicast() {
        let announcer = MdnsDiscovery::new();
        let browser = MdnsDiscovery::new();
        let (sender, events) = std::sync::mpsc::channel();
        browser
            .start(Arc::new(ChannelSink(Mutex::new(sender))))
            .expect("browse");
        let id = device(42);

        announcer.announce(&announcement(id)).expect("announce");
        wait_for(
            &events,
            |e| matches!(e, DiscoveryEvent::Seen { claimed_id, .. } if *claimed_id == id),
        );

        announcer.withdraw().expect("withdraw");
        wait_for(
            &events,
            |e| matches!(e, DiscoveryEvent::Gone { claimed_id } if *claimed_id == id),
        );

        announcer.stop();
        browser.stop();
    }
}
