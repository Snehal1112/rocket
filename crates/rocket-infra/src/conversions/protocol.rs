use crate::oc::*;

use super::request::{oc_http_request_to_request, request_to_oc_http_request};

/// A request that can be any protocol type.
/// HTTP requests are fully converted to domain types.
/// Other protocols preserve their YAML data losslessly.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum ProtocolRequest {
    Http(rocket_collection::Request),
    GraphQL(serde_yaml::Value),
    Grpc(serde_yaml::Value),
    WebSocket(serde_yaml::Value),
}

/// Convert an OcItem to a ProtocolRequest.
/// HTTP items are fully converted; others are stored as opaque YAML.
#[allow(dead_code)]
pub fn oc_item_to_protocol_request(item: OcItem) -> Option<ProtocolRequest> {
    match item {
        OcItem::Http(oc) => Some(ProtocolRequest::Http(oc_http_request_to_request(oc))),
        OcItem::GraphQL(oc) => {
            serde_yaml::to_value(oc).ok().map(ProtocolRequest::GraphQL)
        }
        OcItem::Grpc(oc) => {
            serde_yaml::to_value(oc).ok().map(ProtocolRequest::Grpc)
        }
        OcItem::WebSocket(oc) => {
            serde_yaml::to_value(oc).ok().map(ProtocolRequest::WebSocket)
        }
        // Folders and script files are not requests.
        OcItem::Folder(_) | OcItem::ScriptFile(_) => None,
    }
}

/// Convert a ProtocolRequest back to an OcItem for writing.
#[allow(dead_code)]
pub fn protocol_request_to_oc_item(pr: ProtocolRequest) -> Option<OcItem> {
    match pr {
        ProtocolRequest::Http(req) => Some(OcItem::Http(request_to_oc_http_request(&req))),
        ProtocolRequest::GraphQL(val) => {
            serde_yaml::from_value::<OcGraphQLRequest>(val).ok().map(OcItem::GraphQL)
        }
        ProtocolRequest::Grpc(val) => {
            serde_yaml::from_value::<OcGrpcRequest>(val).ok().map(OcItem::Grpc)
        }
        ProtocolRequest::WebSocket(val) => {
            serde_yaml::from_value::<OcWebSocketRequest>(val).ok().map(OcItem::WebSocket)
        }
    }
}
