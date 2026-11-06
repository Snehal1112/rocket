//! REST bodies between peers. snake_case JSON. Not IPC DTOs.

use serde::{Deserialize, Serialize};

use crate::device::DeviceId;
use crate::pairing::{PairingId, PairingStatus};
use crate::profile::NodeProfile;

/// Reply of `GET /hello`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloResponse {
    pub device_id: DeviceId,
    pub version: u32,
    pub profile: NodeProfile,
}

/// Body of `POST /pair/request`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairRequest {
    pub version: u32,
    pub profile: NodeProfile,
}

/// Reply of `POST /pair/request`. `commitment` is base64.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairRequestResponse {
    pub pairing_id: PairingId,
    pub commitment: String,
}

/// Body and reply of `POST /pair/{id}/nonce`. `nonce` is base64.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NonceMessage {
    pub nonce: String,
}

/// Body of `POST /pair/{id}/decision`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionRequest {
    pub accept: bool,
}

/// Reply of `GET /pair/{id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairStatusResponse {
    pub status: PairingStatus,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn profile() -> NodeProfile {
        NodeProfile::new("Ada Lovelace").expect("valid name")
    }

    #[test]
    fn hello_response_is_snake_case_json() {
        let hello = HelloResponse {
            device_id: DeviceId::from_bytes([1; 32]),
            version: 1,
            profile: profile(),
        };
        let value = serde_json::to_value(&hello).expect("serializes");
        assert_eq!(
            value,
            json!({
                "device_id": DeviceId::from_bytes([1; 32]).to_base32(),
                "version": 1,
                "profile": {"display_name": "Ada Lovelace", "initials": "AL"}
            })
        );
        let back: HelloResponse = serde_json::from_value(value).expect("deserializes");
        assert_eq!(back, hello);
    }

    #[test]
    fn pair_request_round_trips() {
        let request = PairRequest {
            version: 1,
            profile: profile(),
        };
        let text = serde_json::to_string(&request).expect("serializes");
        let back: PairRequest = serde_json::from_str(&text).expect("deserializes");
        assert_eq!(back, request);
    }

    #[test]
    fn pair_request_response_keys_are_stable() {
        let response = PairRequestResponse {
            pairing_id: PairingId("8d2b0c5e-0000-4000-8000-000000000001".to_string()),
            commitment: "AAAA".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&response).expect("serializes"),
            json!({"pairing_id": "8d2b0c5e-0000-4000-8000-000000000001", "commitment": "AAAA"})
        );
    }

    #[test]
    fn nonce_message_and_decision_keys_are_stable() {
        assert_eq!(
            serde_json::to_value(NonceMessage {
                nonce: "abc".to_string()
            })
            .expect("serializes"),
            json!({"nonce": "abc"})
        );
        assert_eq!(
            serde_json::to_value(DecisionRequest { accept: true }).expect("serializes"),
            json!({"accept": true})
        );
    }

    #[test]
    fn pair_status_response_uses_snake_case_status() {
        let response = PairStatusResponse {
            status: PairingStatus::Declined,
        };
        assert_eq!(
            serde_json::to_value(&response).expect("serializes"),
            json!({"status": "declined"})
        );
        let back: PairStatusResponse =
            serde_json::from_str(r#"{"status":"pending"}"#).expect("deserializes");
        assert_eq!(back.status, PairingStatus::Pending);
    }

    #[test]
    fn a_wrong_type_is_rejected() {
        assert!(serde_json::from_str::<DecisionRequest>(r#"{"accept":"yes"}"#).is_err());
        assert!(serde_json::from_str::<PairRequest>(r#"{"version":"1"}"#).is_err());
    }
}
