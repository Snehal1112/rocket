//! The one frame sink of the node: contract check frames to the contract check service, sync
//! frames to the sync service, the rest to the transfer service. All three get every
//! peer-gone notice.

use std::sync::Arc;

use rocket_network::{DeviceId, PeerMessage, TransferId, TransferSink};

use super::contract_check::ContractCheckService;
use super::sync::SyncService;
use super::transfer::TransferService;

pub struct FrameRouter {
    transfer: Arc<TransferService>,
    sync: Arc<SyncService>,
    contract_check: Arc<ContractCheckService>,
}

impl FrameRouter {
    pub fn new(
        transfer: Arc<TransferService>,
        sync: Arc<SyncService>,
        contract_check: Arc<ContractCheckService>,
    ) -> Arc<FrameRouter> {
        Arc::new(FrameRouter {
            transfer,
            sync,
            contract_check,
        })
    }
}

impl TransferSink for FrameRouter {
    fn on_frame(&self, from: DeviceId, message: PeerMessage) {
        // Contract check frames never fall through to the transfer service.
        if self.contract_check.claims(&message) {
            self.contract_check.on_frame(from, message);
            return;
        }
        if let PeerMessage::SyncOffer { transfer_id, .. } = &message {
            // A sync payload may not take over an id the transfer service already uses.
            let taken =
                TransferId::parse(transfer_id).is_ok_and(|id| self.transfer.has_transfer(&id));
            if taken {
                tracing::debug!(code = "network_sync_offer_id_taken", "sync offer dropped");
                return;
            }
        }
        if self.sync.claims(&from, &message) {
            self.sync.on_frame(from, message);
        } else {
            self.transfer.on_frame(from, message);
        }
    }

    fn on_peer_gone(&self, device: DeviceId) {
        self.transfer.on_peer_gone(device);
        self.sync.on_peer_gone(device);
        self.contract_check.on_peer_gone(device);
    }
}
