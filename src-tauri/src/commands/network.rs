//! Team network commands. They stay thin: parse text, call `NetworkService`, map the result.
//! DTOs are camelCase and live here only; the domain types are not renamed.

use std::sync::Arc;

use rocket_app::{
    NetworkHandle, NetworkService, NetworkState, NetworkStatus, NodeGroup, NodeStatus, NodeView,
    PairingDecision, PairingStarted, SendStarted, TransferService,
};
use rocket_network::{BlockedDevice, DeviceId, DiscoverySource, ManualAddress, PairingId};
use rocket_shared::error::DomainError;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use super::slot::{Slot, Stoppable};

// ----- DTOs -----

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStatusDto {
    /// `running` or `off`.
    pub state: &'static str,
    pub off_reason: Option<String>,
    pub port: Option<u16>,
    pub listener_available: bool,
    pub discovery_available: bool,
    pub hidden: bool,
    pub display_name: String,
    pub initials: String,
    pub device_short_id: Option<String>,
    pub intro_seen: bool,
    pub warnings: Vec<String>,
}

impl From<NetworkStatus> for NetworkStatusDto {
    fn from(status: NetworkStatus) -> Self {
        let (state, off_reason) = match status.state {
            NetworkState::Running => ("running", None),
            NetworkState::Off { reason } => ("off", Some(reason)),
        };
        Self {
            state,
            off_reason,
            port: status.port,
            listener_available: status.listener_available,
            discovery_available: status.discovery_available,
            hidden: status.hidden,
            display_name: status.profile.display_name,
            initials: status.profile.initials,
            device_short_id: status.device_short_id,
            intro_seen: status.intro_seen,
            warnings: status.warnings,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeViewDto {
    pub device_id: String,
    pub short_id: String,
    pub display_name: String,
    pub initials: String,
    /// `paired` or `nearby`.
    pub group: &'static str,
    /// `online`, `offline`, `pairing`, `needs_update` or `available`.
    pub status: &'static str,
    /// `mdns`, `manual` or none.
    pub source: Option<&'static str>,
}

impl From<NodeView> for NodeViewDto {
    fn from(view: NodeView) -> Self {
        Self {
            device_id: view.device_id,
            short_id: view.short_id,
            display_name: view.display_name,
            initials: view.initials,
            group: match view.group {
                NodeGroup::Paired => "paired",
                NodeGroup::Nearby => "nearby",
            },
            status: match view.status {
                NodeStatus::Online => "online",
                NodeStatus::Offline => "offline",
                NodeStatus::Pairing => "pairing",
                NodeStatus::NeedsUpdate => "needs_update",
                NodeStatus::Available => "available",
            },
            source: view.source.map(|source| match source {
                DiscoverySource::Mdns => "mdns",
                DiscoverySource::Manual => "manual",
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualAddressDto {
    pub host: String,
    pub port: u16,
    /// RFC 3339.
    pub added_at: String,
}

impl From<ManualAddress> for ManualAddressDto {
    fn from(address: ManualAddress) -> Self {
        Self {
            host: address.host,
            port: address.port,
            added_at: address.added_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockedDeviceDto {
    pub device_id: String,
    pub short_id: String,
    pub name: String,
    /// RFC 3339.
    pub blocked_at: String,
}

impl From<BlockedDevice> for BlockedDeviceDto {
    fn from(device: BlockedDevice) -> Self {
        Self {
            device_id: device.device_id.to_base32(),
            short_id: device.device_id.short(),
            name: device.name,
            blocked_at: device.blocked_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingStartedDto {
    pub pairing_id: String,
    /// Six digits without a space. The UI groups them as `482 913`.
    pub code: String,
}

impl From<PairingStarted> for PairingStartedDto {
    fn from(started: PairingStarted) -> Self {
        Self {
            pairing_id: started.pairing_id.0,
            code: started.code.digits(),
        }
    }
}

// ----- parsing -----

fn parse_device_id(raw: &str) -> Result<DeviceId, DomainError> {
    DeviceId::parse_base32(raw)
        .map_err(|_| DomainError::InvalidInput("That device id is not valid.".to_string()))
}

fn parse_decision(raw: &str) -> Result<PairingDecision, DomainError> {
    match raw {
        "accept" => Ok(PairingDecision::Accept),
        "decline" => Ok(PairingDecision::Decline),
        "block" => Ok(PairingDecision::Block),
        _ => Err(DomainError::InvalidInput(
            "The decision must be accept, decline or block.".to_string(),
        )),
    }
}

// ----- commands -----

#[tauri::command]
pub fn network_status(svc: State<'_, Arc<NetworkService>>) -> NetworkStatusDto {
    svc.status().into()
}

#[tauri::command]
pub fn network_list_nodes(svc: State<'_, Arc<NetworkService>>) -> Vec<NodeViewDto> {
    svc.list_nodes().into_iter().map(Into::into).collect()
}

#[tauri::command]
pub async fn network_set_visibility(
    hidden: bool,
    svc: State<'_, Arc<NetworkService>>,
) -> Result<(), DomainError> {
    svc.set_hidden(hidden).await
}

#[tauri::command]
pub async fn network_set_profile(
    display_name: String,
    svc: State<'_, Arc<NetworkService>>,
) -> Result<(), DomainError> {
    svc.set_display_name(&display_name).await
}

#[tauri::command]
pub fn network_mark_intro_seen(svc: State<'_, Arc<NetworkService>>) -> Result<(), DomainError> {
    svc.mark_intro_seen()
}

#[tauri::command]
pub fn network_list_addresses(
    svc: State<'_, Arc<NetworkService>>,
) -> Result<Vec<ManualAddressDto>, DomainError> {
    Ok(svc.list_addresses()?.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn network_add_address(
    host: String,
    port: u16,
    svc: State<'_, Arc<NetworkService>>,
) -> Result<(), DomainError> {
    svc.add_address(&host, port).await
}

#[tauri::command]
pub fn network_remove_address(
    host: String,
    port: u16,
    svc: State<'_, Arc<NetworkService>>,
) -> Result<(), DomainError> {
    svc.remove_address(&host, port)
}

#[tauri::command]
pub async fn network_request_pairing(
    device_id: String,
    svc: State<'_, Arc<NetworkService>>,
) -> Result<PairingStartedDto, DomainError> {
    let id = parse_device_id(&device_id)?;
    svc.request_pairing(&id).await.map(Into::into)
}

#[tauri::command]
pub async fn network_respond_pairing(
    pairing_id: String,
    decision: String,
    svc: State<'_, Arc<NetworkService>>,
) -> Result<(), DomainError> {
    let decision = parse_decision(&decision)?;
    svc.respond_pairing(&PairingId(pairing_id), decision).await
}

#[tauri::command]
pub fn network_cancel_pairing(
    pairing_id: String,
    svc: State<'_, Arc<NetworkService>>,
) -> Result<(), DomainError> {
    svc.cancel_pairing(&PairingId(pairing_id))
}

#[tauri::command]
pub async fn network_remove_paired(
    device_id: String,
    svc: State<'_, Arc<NetworkService>>,
) -> Result<(), DomainError> {
    let id = parse_device_id(&device_id)?;
    svc.remove_paired(&id).await
}

#[tauri::command]
pub fn network_list_blocked(
    svc: State<'_, Arc<NetworkService>>,
) -> Result<Vec<BlockedDeviceDto>, DomainError> {
    Ok(svc.list_blocked()?.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub fn network_unblock(
    device_id: String,
    svc: State<'_, Arc<NetworkService>>,
) -> Result<(), DomainError> {
    let id = parse_device_id(&device_id)?;
    svc.unblock(&id)
}

// ----- send a collection -----

/// What the UI gets after a push started: the transfer id and one line per item the export
/// dropped or blanked.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendStartedDto {
    pub transfer_id: String,
    pub notes: Vec<String>,
}

impl From<SendStarted> for SendStartedDto {
    fn from(started: SendStarted) -> Self {
        Self {
            transfer_id: started.transfer_id,
            notes: started.notes,
        }
    }
}

/// Starts a push of one collection to a paired node. Returns the transfer id and the notes.
/// With `keep_in_sync` the first send also creates a link on both sides.
#[tauri::command]
pub async fn network_send_collection(
    device_id: String,
    collection: String,
    include_environments: bool,
    keep_in_sync: bool,
    svc: State<'_, Arc<TransferService>>,
) -> Result<SendStartedDto, DomainError> {
    let id = parse_device_id(&device_id)?;
    let started = svc
        .send(&id, &collection, include_environments, keep_in_sync)
        .await?;
    Ok(SendStartedDto::from(started))
}

/// Accepts or declines an incoming offer.
#[tauri::command]
pub async fn network_respond_transfer(
    transfer_id: String,
    accept: bool,
    svc: State<'_, Arc<TransferService>>,
) -> Result<(), DomainError> {
    svc.respond(&transfer_id, accept).await
}

/// Cancels a transfer from either side.
#[tauri::command]
pub async fn network_cancel_transfer(
    transfer_id: String,
    svc: State<'_, Arc<TransferService>>,
) -> Result<(), DomainError> {
    svc.cancel(&transfer_id).await
}

// ----- the running node -----

impl Stoppable for NetworkHandle {
    fn stop(&self) {
        NetworkHandle::stop(self);
    }
}

/// The kept node handle. The exit paths call `stop_for_good()`, so a node that finishes starting
/// after the exit began is stopped at once.
pub(crate) type NetworkSlot<H = NetworkHandle> = Slot<H>;

/// Starts the node on the app runtime and keeps its handle in the slot.
pub(crate) fn start_network(app: &AppHandle) {
    let Some(svc) = app.try_state::<Arc<NetworkService>>() else {
        tracing::error!(code = "network_service_missing", "networking not started");
        return;
    };
    let svc = Arc::clone(svc.inner());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let handle = svc.start().await;
        match app.try_state::<NetworkSlot>() {
            Some(slot) => slot.replace(handle),
            None => handle.stop(),
        }
    });
}

#[cfg(test)]
mod tests {
    use rocket_app::{
        NetworkState, NetworkStatus, NodeGroup, NodeStatus, NodeView, PairingStarted,
    };
    use rocket_network::{
        BlockedDevice, DeviceId, DiscoverySource, ManualAddress, NodeProfile, PairingCode,
        PairingId,
    };
    use rocket_shared::error::DomainError;

    use super::*;

    fn keys(value: &serde_json::Value) -> Vec<String> {
        let mut keys: Vec<String> = value
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    }

    fn sample_status(state: NetworkState) -> NetworkStatus {
        NetworkStatus {
            state,
            port: Some(47821),
            listener_available: true,
            discovery_available: false,
            hidden: false,
            profile: NodeProfile::new("Sam Lee").expect("profile"),
            device_short_id: Some("K7QF-2M9X".to_string()),
            intro_seen: true,
            warnings: vec!["paired.yml was damaged and has been reset.".to_string()],
        }
    }

    #[test]
    fn the_status_dto_uses_camel_case_keys() {
        let dto = NetworkStatusDto::from(sample_status(NetworkState::Running));
        let value = serde_json::to_value(&dto).expect("json");
        assert_eq!(
            keys(&value),
            vec![
                "deviceShortId",
                "discoveryAvailable",
                "displayName",
                "hidden",
                "initials",
                "introSeen",
                "listenerAvailable",
                "offReason",
                "port",
                "state",
                "warnings"
            ]
        );
        assert_eq!(value["state"], "running");
        assert_eq!(value["offReason"], serde_json::Value::Null);
        assert_eq!(value["displayName"], "Sam Lee");
        assert_eq!(value["initials"], "SL");
        assert_eq!(value["port"], 47821);
        assert_eq!(value["discoveryAvailable"], false);
    }

    #[test]
    fn an_off_status_carries_its_reason() {
        let reason = "The keychain is not available, so networking is off.".to_string();
        let dto = NetworkStatusDto::from(sample_status(NetworkState::Off {
            reason: reason.clone(),
        }));
        let value = serde_json::to_value(&dto).expect("json");
        assert_eq!(value["state"], "off");
        assert_eq!(value["offReason"], reason);
    }

    #[test]
    fn the_node_dto_uses_camel_case_keys_and_snake_case_words() {
        let view = NodeView {
            device_id: "ABC".to_string(),
            short_id: "K7QF-2M9X".to_string(),
            display_name: "Ben Ortiz".to_string(),
            initials: "BO".to_string(),
            group: NodeGroup::Nearby,
            status: NodeStatus::NeedsUpdate,
            source: Some(DiscoverySource::Mdns),
        };
        let value = serde_json::to_value(NodeViewDto::from(view)).expect("json");
        assert_eq!(
            keys(&value),
            vec![
                "deviceId",
                "displayName",
                "group",
                "initials",
                "shortId",
                "source",
                "status"
            ]
        );
        assert_eq!(value["group"], "nearby");
        assert_eq!(value["status"], "needs_update");
        assert_eq!(value["source"], "mdns");
    }

    #[test]
    fn every_node_word_matches_the_contract() {
        let words = [
            (NodeStatus::Online, "online"),
            (NodeStatus::Offline, "offline"),
            (NodeStatus::Pairing, "pairing"),
            (NodeStatus::NeedsUpdate, "needs_update"),
            (NodeStatus::Available, "available"),
        ];
        for (status, word) in words {
            let view = NodeView {
                device_id: String::new(),
                short_id: String::new(),
                display_name: String::new(),
                initials: String::new(),
                group: NodeGroup::Paired,
                status,
                source: Some(DiscoverySource::Manual),
            };
            let value = serde_json::to_value(NodeViewDto::from(view)).expect("json");
            assert_eq!(value["status"], word);
            assert_eq!(value["group"], "paired");
            assert_eq!(value["source"], "manual");
        }
    }

    #[test]
    fn the_small_dtos_use_camel_case_keys() {
        let address = ManualAddress {
            host: "team.example".to_string(),
            port: 47821,
            added_at: chrono::Utc::now(),
        };
        let value = serde_json::to_value(ManualAddressDto::from(address)).expect("json");
        assert_eq!(keys(&value), vec!["addedAt", "host", "port"]);
        assert!(value["addedAt"].is_string());

        let blocked = BlockedDevice {
            device_id: DeviceId::from_bytes([3; 32]),
            name: "Dee".to_string(),
            blocked_at: chrono::Utc::now(),
        };
        let value = serde_json::to_value(BlockedDeviceDto::from(blocked)).expect("json");
        assert_eq!(
            keys(&value),
            vec!["blockedAt", "deviceId", "name", "shortId"]
        );
        assert_eq!(value["deviceId"], DeviceId::from_bytes([3; 32]).to_base32());

        let started = PairingStarted {
            pairing_id: PairingId("abc".to_string()),
            code: PairingCode(4_213),
        };
        let value = serde_json::to_value(PairingStartedDto::from(started)).expect("json");
        assert_eq!(keys(&value), vec!["code", "pairingId"]);
        assert_eq!(value["code"], "004213", "six digits, no space");
        assert_eq!(value["pairingId"], "abc");
    }

    #[test]
    fn decisions_parse_and_other_words_are_rejected() {
        assert_eq!(parse_decision("accept"), Ok(PairingDecision::Accept));
        assert_eq!(parse_decision("decline"), Ok(PairingDecision::Decline));
        assert_eq!(parse_decision("block"), Ok(PairingDecision::Block));
        for bad in ["", "Accept", "yes", "accept "] {
            assert!(
                matches!(parse_decision(bad), Err(DomainError::InvalidInput(_))),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn device_ids_parse_or_are_invalid_input() {
        let id = DeviceId::from_bytes([9; 32]);
        assert_eq!(parse_device_id(&id.to_base32()), Ok(id));
        for bad in ["", "nope", "K7QF-2M9X", &"A".repeat(51)] {
            assert_eq!(
                parse_device_id(bad),
                Err(DomainError::InvalidInput(
                    "That device id is not valid.".to_string()
                )),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn the_transfer_commands_are_in_the_acl_list() {
        let list = include_str!("../../app-commands.txt");
        for name in [
            "network_send_collection",
            "network_respond_transfer",
            "network_cancel_transfer",
        ] {
            assert!(
                list.lines().any(|line| line.trim() == name),
                "{name} is missing from app-commands.txt"
            );
        }
    }

    #[test]
    fn the_send_started_dto_uses_camel_case_keys() {
        let dto = SendStartedDto::from(SendStarted {
            transfer_id: "t1".to_string(),
            notes: vec!["flow: skipped".to_string()],
        });
        let value = serde_json::to_value(&dto).expect("json");
        assert_eq!(keys(&value), vec!["notes", "transferId"]);
        assert_eq!(value["transferId"], "t1");
        assert_eq!(value["notes"][0], "flow: skipped");
    }
}
