//! Auth and request-settings structs for the OpenCollection YAML format.

use rocket_shared::oauth2::{
    OAuth2AdditionalParameters, OAuth2PKCE, OAuth2Settings, OAuth2TokenConfig,
};
use serde::{Deserialize, Serialize};

/// OpenCollection Auth — discriminated by `type` field. String "inherit" for inheritance.
/// Uses custom serde since it's a oneOf with a string shorthand.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OcAuth {
    /// String shorthand: "inherit".
    Inherit(String),
    /// Object form: dispatched by `type` field.
    Typed(Box<OcAuthTyped>),
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
    #[serde(rename = "oauth1")]
    OAuth1(OcOAuth1),
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
        additional_parameters: Box<Option<OAuth2AdditionalParameters>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        token_config: Box<Option<OAuth2TokenConfig>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        settings: Option<OAuth2Settings>,
    },
}

/// OAuth1 auth fields as persisted on disk (spec `AuthOAuth1`, minus the `type` tag).
/// A dedicated type so the on-disk shape stays independent of the domain type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OcOAuth1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumer_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumer_secret: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token_secret: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_key: Option<OcOAuth1PrivateKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realm: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_body_hash: Option<bool>,
}

/// OAuth1 private key: `{ type: "text" | "file", value }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcOAuth1PrivateKey {
    #[serde(rename = "type")]
    pub key_type: String,
    pub value: String,
}

/// OAuth2 client credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcOAuth2Credentials {
    pub client_id: String,
    /// Absent for the implicit flow, whose spec credentials hold only `clientId`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<String>,
}

/// OAuth2 resource owner credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcOAuth2ResourceOwner {
    pub username: String,
    pub password: String,
}

/// OAuth2 PKCE configuration as persisted on disk: spec shape (`disabled`, `method`),
/// with the legacy `enabled` field still accepted on read. This is a dedicated type,
/// not an alias to the domain `OAuth2PKCE`, so a future domain-only change to that
/// type can't silently change the on-disk shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", from = "OcOAuth2PKCEWire")]
pub struct OcOAuth2PKCE {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
}

impl OcOAuth2PKCE {
    /// Returns true unless PKCE was explicitly disabled. Mirrors the domain
    /// type's helper of the same name.
    pub fn is_enabled(&self) -> bool {
        !self.disabled.unwrap_or(false)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OcOAuth2PKCEWire {
    #[serde(default)]
    disabled: Option<bool>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    method: Option<String>,
}

impl From<OcOAuth2PKCEWire> for OcOAuth2PKCE {
    fn from(w: OcOAuth2PKCEWire) -> Self {
        // `disabled` wins when both are present, because it is the spec field.
        let is_disabled = w.disabled.or(w.enabled.map(|on| !on)).unwrap_or(false);
        OcOAuth2PKCE {
            disabled: is_disabled.then_some(true),
            method: w.method,
        }
    }
}

impl From<OAuth2PKCE> for OcOAuth2PKCE {
    fn from(p: OAuth2PKCE) -> Self {
        OcOAuth2PKCE {
            disabled: p.disabled,
            method: p.method,
        }
    }
}

impl From<OcOAuth2PKCE> for OAuth2PKCE {
    fn from(p: OcOAuth2PKCE) -> Self {
        OAuth2PKCE {
            disabled: p.disabled,
            method: p.method,
        }
    }
}

/// A value that can be a boolean or the string "inherit".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InheritableBoolean {
    Value(bool),
    Inherit(String), // "inherit"
}

/// A value that can be a number or the string "inherit".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InheritableNumber {
    Value(f64),
    Inherit(String), // "inherit"
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
