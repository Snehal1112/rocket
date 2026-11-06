//! Persistence records and the discovered-node view. Persistence is snake_case.

use std::net::SocketAddr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::device::DeviceId;
use crate::profile::NodeProfile;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedDevice {
    pub device_id: DeviceId,
    pub name_at_pairing: String,
    pub paired_at: DateTime<Utc>,
    #[serde(default)]
    pub last_addresses: Vec<SocketAddr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockedDevice {
    pub device_id: DeviceId,
    pub name: String,
    pub blocked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualAddress {
    pub host: String,
    pub port: u16,
    pub added_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NetworkSettings {
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub intro_seen: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscoverySource {
    Mdns,
    Manual,
}

/// A node seen on the network. Held in memory only, never persisted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredNode {
    pub device_id: DeviceId,
    pub profile: NodeProfile,
    pub addresses: Vec<SocketAddr>,
    pub source: DiscoverySource,
    pub last_seen: DateTime<Utc>,
    pub protocol_version: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

    fn when() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 6, 10, 30, 0)
            .single()
            .expect("valid date")
    }

    #[test]
    fn paired_device_json_is_snake_case_and_round_trips() {
        let device = PairedDevice {
            device_id: DeviceId::from_bytes([4; 32]),
            name_at_pairing: "Ada".to_string(),
            paired_at: when(),
            last_addresses: vec!["192.168.1.20:47821".parse().expect("addr")],
        };
        let value = serde_json::to_value(&device).expect("serializes");
        let object = value.as_object().expect("object");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "device_id",
                "last_addresses",
                "name_at_pairing",
                "paired_at"
            ]
        );
        assert_eq!(object["last_addresses"], json!(["192.168.1.20:47821"]));
        let back: PairedDevice = serde_json::from_value(value).expect("deserializes");
        assert_eq!(back, device);
    }

    #[test]
    fn paired_device_without_addresses_still_loads() {
        let id = DeviceId::from_bytes([4; 32]).to_base32();
        let text = format!(
            r#"{{"device_id":"{id}","name_at_pairing":"Ada","paired_at":"2026-11-06T10:30:00Z"}}"#
        );
        let device: PairedDevice = serde_json::from_str(&text).expect("old file loads");
        assert!(device.last_addresses.is_empty());
    }

    #[test]
    fn blocked_device_and_manual_address_round_trip() {
        let blocked = BlockedDevice {
            device_id: DeviceId::from_bytes([5; 32]),
            name: "Mallory".to_string(),
            blocked_at: when(),
        };
        let text = serde_json::to_string(&blocked).expect("serializes");
        let back: BlockedDevice = serde_json::from_str(&text).expect("deserializes");
        assert_eq!(back, blocked);

        let address = ManualAddress {
            host: "laptop.tail1234.ts.net".to_string(),
            port: 47821,
            added_at: when(),
        };
        let text = serde_json::to_string(&address).expect("serializes");
        let back: ManualAddress = serde_json::from_str(&text).expect("deserializes");
        assert_eq!(back, address);
    }

    #[test]
    fn settings_default_to_visible_with_no_name() {
        let settings = NetworkSettings::default();
        assert!(!settings.hidden);
        assert_eq!(settings.display_name, None);
        assert!(!settings.intro_seen);
    }

    #[test]
    fn settings_fill_missing_fields_with_defaults() {
        let settings: NetworkSettings = serde_json::from_str("{}").expect("empty loads");
        assert_eq!(settings, NetworkSettings::default());
        let settings: NetworkSettings =
            serde_json::from_str(r#"{"hidden":true}"#).expect("partial loads");
        assert!(settings.hidden);
        assert_eq!(settings.display_name, None);
    }

    #[test]
    fn discovery_source_names_are_stable() {
        assert_eq!(
            serde_json::to_string(&DiscoverySource::Mdns).expect("serializes"),
            "\"Mdns\""
        );
        assert_eq!(
            serde_json::to_string(&DiscoverySource::Manual).expect("serializes"),
            "\"Manual\""
        );
    }

    #[test]
    fn discovered_node_holds_what_the_panel_needs() {
        let node = DiscoveredNode {
            device_id: DeviceId::from_bytes([6; 32]),
            profile: NodeProfile::new("Grace Hopper").expect("valid"),
            addresses: vec!["10.0.0.5:47822".parse().expect("addr")],
            source: DiscoverySource::Mdns,
            last_seen: when(),
            protocol_version: 1,
        };
        assert_eq!(node.clone(), node);
        assert_eq!(node.profile.initials, "GH");
    }
}
