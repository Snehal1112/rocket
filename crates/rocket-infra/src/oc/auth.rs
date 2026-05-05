//! Auth and request-settings structs for the OpenCollection YAML format.

use rocket_shared::oauth2::{OAuth2AdditionalParameters, OAuth2Settings, OAuth2TokenConfig};
use serde::{Deserialize, Serialize};

/// OpenCollection Auth — discriminated by `type` field. String "inherit" for inheritance.
/// Uses custom serde since it's a oneOf with a string shorthand.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OcAuth {
    /// String shorthand: "inherit".
    Inherit(String),
    /// Object form: dispatched by `type` field.
    Typed(OcAuthTyped),
}

/// Typed auth — discriminated by `type` field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OcAuthTyped {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "basic")]
    Basic { username: String, password: String },
    #[serde(rename = "bearer")]
    Bearer { token: String },
    #[serde(rename = "apikey", rename_all = "camelCase")]
    ApiKey {
        key: String,
        value: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        placement: Option<String>,
    },
    #[serde(rename = "digest")]
    Digest { username: String, password: String },
    #[serde(rename = "ntlm")]
    Ntlm {
        username: String,
        password: String,
        domain: String,
    },
    #[serde(rename = "wsse")]
    Wsse { username: String, password: String },
    #[serde(rename = "awsv4", rename_all = "camelCase")]
    AwsV4 {
        access_key_id: String,
        secret_access_key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_token: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        service: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        region: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        profile_name: Option<String>,
    },
    #[serde(rename = "oauth2", rename_all = "camelCase")]
    OAuth2 {
        flow: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        access_token_url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        refresh_token_url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        authorization_url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        callback_url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        credentials: Option<OcOAuth2Credentials>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resource_owner: Option<OcOAuth2ResourceOwner>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        state: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pkce: Option<OcOAuth2PKCE>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        additional_parameters: Option<OAuth2AdditionalParameters>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        token_config: Option<OAuth2TokenConfig>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        settings: Option<OAuth2Settings>,
    },
}

/// OAuth2 client credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcOAuth2Credentials {
    pub client_id: String,
    pub client_secret: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<String>,
}

/// OAuth2 resource owner credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcOAuth2ResourceOwner {
    pub username: String,
    pub password: String,
}

/// OAuth2 PKCE configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcOAuth2PKCE {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
}

/// A value that can be a boolean or the string "inherit".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InheritableBoolean {
    Value(bool),
    Inherit(String),  // "inherit"
}

/// A value that can be a number or the string "inherit".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InheritableNumber {
    Value(f64),
    Inherit(String),  // "inherit"
}

/// HTTP request execution settings.
/// Schema: { encodeUrl, timeout, followRedirects, maxRedirects }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcHttpRequestSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encode_url: Option<InheritableBoolean>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<InheritableNumber>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow_redirects: Option<InheritableBoolean>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_redirects: Option<InheritableNumber>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_ssl: Option<InheritableBoolean>,
}

/// GraphQL request execution settings (same fields as HTTP settings).
/// Schema: { encodeUrl, timeout, followRedirects, maxRedirects }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcGraphQLRequestSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encode_url: Option<InheritableBoolean>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<InheritableNumber>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow_redirects: Option<InheritableBoolean>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_redirects: Option<InheritableNumber>,
}

/// Proxy auth for OC file format (schema uses disabled + username + password).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcProxyAuth {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

/// Proxy connection config for OC file format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcProxyConnectionConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<OcProxyAuth>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bypass_proxy: Option<String>,
}

/// Proxy configuration for OC file format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcProxy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherit: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<OcProxyConnectionConfig>,
}
