//! OcRequestDefaults and OcRequestSettings — shared by folder and collection.

use serde::{Deserialize, Serialize};

use super::auth::{InheritableBoolean, InheritableNumber, OcAuth};
use super::grpc::OcGrpcMetadata;
use super::http::{OcHttpRequestHeader, OcScript};
use super::variables::OcVariable;

/// Generic request settings — shared by OcRequestDefaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcRequestSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encode_url: Option<InheritableBoolean>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<InheritableNumber>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow_redirects: Option<InheritableBoolean>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_redirects: Option<InheritableNumber>,
}

/// Request defaults — applied to all requests in a folder or collection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct OcRequestDefaults {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<Vec<OcHttpRequestHeader>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Vec<OcGrpcMetadata>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<OcAuth>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<Vec<OcVariable>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scripts: Option<Vec<OcScript>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<OcRequestSettings>,
}
