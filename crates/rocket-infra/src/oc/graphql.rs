//! GraphQL request structs for the OpenCollection YAML format.

use rocket_shared::assertion::Assertion as OcAssertion;
use rocket_shared::description::Description as OcDescription;
use serde::{Deserialize, Serialize};

use super::auth::{OcAuth, OcGraphQLRequestSettings};
use super::http::{OcAction, OcHttpRequestHeader, OcHttpRequestParam, OcScript};
use super::variables::OcVariable;

/// GraphQL request info.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcGraphQLRequestInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<OcDescription>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
    pub request_type: Option<String>,  // "graphql"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// GraphQL body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcGraphQLBody {
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<String>,
}

/// GraphQL body variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcGraphQLBodyVariant {
    pub title: String,
    #[serde(default)]
    pub selected: bool,
    pub body: OcGraphQLBody,
}

/// GraphQL body — either a single body or array of variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OcGraphQLBodyOrVariants {
    Single(OcGraphQLBody),
    Variants(Vec<OcGraphQLBodyVariant>),
}

/// GraphQL request protocol details.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcGraphQLRequestDetails {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    pub url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<OcHttpRequestHeader>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<OcHttpRequestParam>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<OcGraphQLBodyOrVariants>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<OcAuth>,
}

/// GraphQL request runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcGraphQLRequestRuntime {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<OcVariable>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<OcScript>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assertions: Vec<OcAssertion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<OcAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<OcAuth>,
}

/// Complete GraphQL request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcGraphQLRequest {
    pub info: OcGraphQLRequestInfo,
    pub graphql: OcGraphQLRequestDetails,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<OcGraphQLRequestRuntime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<OcGraphQLRequestSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
}
