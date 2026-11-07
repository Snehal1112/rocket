//! The transfer hooks of `NetworkService`: a bounded send to one device, the label of a paired
//! device, and the sink that receives the transfer frames.

use std::sync::Arc;
use std::time::Duration;

use rocket_network::{DeviceId, PeerMessage, TransferSink};
use rocket_shared::error::{DomainError, DomainResult};

use super::service::NetworkService;

/// How long a frame may wait for room on the connection.
const SEND_TIMEOUT: Duration = Duration::from_secs(2);

fn offline_error() -> DomainError {
    DomainError::InvalidInput("That node is offline.".to_string())
}

impl NetworkService {
    /// Sends one frame on the live connection of a paired device. The wait for room is bounded.
    pub async fn send_to(&self, device: &DeviceId, message: PeerMessage) -> DomainResult<()> {
        let outbound = self
            .lock()
            .conns
            .get(device)
            .and_then(|c| c.outbound.clone());
        let Some(outbound) = outbound else {
            return Err(offline_error());
        };
        match tokio::time::timeout(SEND_TIMEOUT, outbound.send(message)).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(offline_error()),
            Err(_) => Err(DomainError::Io(
                "The node did not take the message in time.".to_string(),
            )),
        }
    }

    /// True when the device has a live connection that can take a frame.
    pub fn is_online(&self, device: &DeviceId) -> bool {
        self.lock()
            .conns
            .get(device)
            .is_some_and(|c| c.outbound.is_some())
    }

    /// Sets the receiver of the transfer frames. A second call replaces the first.
    pub fn set_transfer_sink(&self, sink: Arc<dyn TransferSink>) {
        *self.transfer_sink.lock().unwrap_or_else(|e| e.into_inner()) = Some(sink);
    }

    /// The display name and short id of a paired device. A device that is not paired, or is
    /// blocked, has no label.
    pub fn peer_label(&self, device: &DeviceId) -> Option<(String, String)> {
        if !self.paired_and_not_blocked(device) {
            return None;
        }
        Some((self.known_name(device), device.short()))
    }

    /// Hands a transfer frame to the sink. Without a sink the frame is dropped.
    pub(super) fn deliver_transfer_frame(&self, from: &DeviceId, message: PeerMessage) {
        let sink = self
            .transfer_sink
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(sink) = sink {
            sink.on_frame(*from, message);
        }
    }

    /// Tells the sink that the connection to a device ended or its pairing was removed.
    /// Without a sink nothing happens. No lock is held while the sink runs.
    pub(super) fn notify_peer_gone(&self, device: &DeviceId) {
        let sink = self
            .transfer_sink
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(sink) = sink {
            sink.on_peer_gone(*device);
        }
    }
}
