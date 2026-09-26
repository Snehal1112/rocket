use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2ClientCredentials {
    pub client_id: String,
    pub client_secret: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<String>, // "basic_auth_header" | "body"
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2ResourceOwner {
    pub username: String,
    pub password: String,
}

/// PKCE settings. The spec field is `disabled`, and an absent value means PKCE is on.
/// Data written before 2026-09 used `enabled`, so deserialization accepts both
/// and normalizes to `disabled`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", from = "OAuth2PKCEWire")]
pub struct OAuth2PKCE {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>, // "S256" | "plain"
}

impl OAuth2PKCE {
    /// Returns true unless PKCE was explicitly disabled.
    pub fn is_enabled(&self) -> bool {
        !self.disabled.unwrap_or(false)
    }
}

/// Input shape that accepts both the spec field (`disabled`) and the legacy field (`enabled`).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OAuth2PKCEWire {
    #[serde(default)]
    disabled: Option<bool>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    method: Option<String>,
}

impl From<OAuth2PKCEWire> for OAuth2PKCE {
    fn from(w: OAuth2PKCEWire) -> Self {
        // `disabled` wins when both are present, because it is the spec field.
        let is_disabled = w.disabled.or(w.enabled.map(|on| !on)).unwrap_or(false);
        OAuth2PKCE {
            disabled: is_disabled.then_some(true),
            method: w.method,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OAuth2TokenPlacement {
    Header { header: String },
    Query { query: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2TokenConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>, // "accessToken" | "idToken"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<OAuth2TokenPlacement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2AdditionalParameter {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<String>, // "header" | "query" | "body"
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2AdditionalParameters {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization_request: Option<Vec<OAuth2AdditionalParameter>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token_request: Option<Vec<OAuth2AdditionalParameter>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token_request: Option<Vec<OAuth2AdditionalParameter>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2Settings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_fetch_token: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_refresh_token: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "verifySsl")]
    pub verify_ssl: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub use_system_browser: Option<bool>,
}

/// OAuth2 flow — discriminated by `flow` field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "flow", rename_all = "snake_case")]
pub enum OAuth2Flow {
    #[serde(rename = "client_credentials")]
    ClientCredentials {
        #[serde(rename = "accessTokenUrl")]
        access_token_url: String,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            rename = "refreshTokenUrl"
        )]
        refresh_token_url: Option<String>,
        credentials: OAuth2ClientCredentials,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            rename = "additionalParameters"
        )]
        additional_parameters: Option<OAuth2AdditionalParameters>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            rename = "tokenConfig"
        )]
        token_config: Option<OAuth2TokenConfig>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        settings: Option<OAuth2Settings>,
    },
    #[serde(rename = "resource_owner_password_credentials")]
    ResourceOwnerPassword {
        #[serde(rename = "accessTokenUrl")]
        access_token_url: String,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            rename = "refreshTokenUrl"
        )]
        refresh_token_url: Option<String>,
        credentials: OAuth2ClientCredentials,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            rename = "resourceOwner"
        )]
        resource_owner: Option<OAuth2ResourceOwner>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            rename = "additionalParameters"
        )]
        additional_parameters: Option<OAuth2AdditionalParameters>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            rename = "tokenConfig"
        )]
        token_config: Option<OAuth2TokenConfig>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        settings: Option<OAuth2Settings>,
    },
    #[serde(rename = "authorization_code")]
    AuthorizationCode {
        #[serde(rename = "authorizationUrl")]
        authorization_url: String,
        #[serde(rename = "accessTokenUrl")]
        access_token_url: String,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            rename = "refreshTokenUrl"
        )]
        refresh_token_url: Option<String>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            rename = "callbackUrl"
        )]
        callback_url: Option<String>,
        credentials: OAuth2ClientCredentials,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        state: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pkce: Option<OAuth2PKCE>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            rename = "additionalParameters"
        )]
        additional_parameters: Option<OAuth2AdditionalParameters>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            rename = "tokenConfig"
        )]
        token_config: Option<OAuth2TokenConfig>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        settings: Option<OAuth2Settings>,
    },
    #[serde(rename = "implicit")]
    Implicit {
        #[serde(rename = "authorizationUrl")]
        authorization_url: String,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            rename = "callbackUrl"
        )]
        callback_url: Option<String>,
        #[serde(rename = "clientId")]
        client_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        state: Option<String>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            rename = "additionalParameters"
        )]
        additional_parameters: Option<OAuth2AdditionalParameters>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            rename = "tokenConfig"
        )]
        token_config: Option<OAuth2TokenConfig>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        settings: Option<OAuth2Settings>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_credentials_serde() {
        let creds = OAuth2ClientCredentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
            placement: Some("basic_auth_header".into()),
        };
        let json = serde_json::to_string(&creds).unwrap();
        let back: OAuth2ClientCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(creds, back);
    }

    #[test]
    fn pkce_config() {
        let pkce = OAuth2PKCE {
            disabled: None,
            method: Some("S256".into()),
        };
        let json = serde_json::to_string(&pkce).unwrap();
        assert!(!json.contains("enabled"), "legacy field must never be written: {json}");
        let back: OAuth2PKCE = serde_json::from_str(&json).unwrap();
        assert_eq!(pkce, back);
        assert!(back.is_enabled());
    }

    #[test]
    fn pkce_disabled_serializes_as_spec_field() {
        let pkce = OAuth2PKCE {
            disabled: Some(true),
            method: None,
        };
        assert_eq!(serde_json::to_string(&pkce).unwrap(), r#"{"disabled":true}"#);
        assert!(!pkce.is_enabled());
    }

    #[test]
    fn pkce_reads_legacy_enabled_field() {
        let on: OAuth2PKCE = serde_json::from_str(r#"{"enabled":true,"method":"S256"}"#).unwrap();
        assert_eq!(
            on,
            OAuth2PKCE {
                disabled: None,
                method: Some("S256".into())
            }
        );
        let off: OAuth2PKCE = serde_json::from_str(r#"{"enabled":false}"#).unwrap();
        assert_eq!(
            off,
            OAuth2PKCE {
                disabled: Some(true),
                method: None
            }
        );
    }

    #[test]
    fn pkce_disabled_wins_over_legacy_enabled() {
        let p: OAuth2PKCE = serde_json::from_str(r#"{"disabled":false,"enabled":false}"#).unwrap();
        assert!(p.is_enabled());
    }

    #[test]
    fn pkce_normalises_disabled_false_to_absent() {
        let p: OAuth2PKCE = serde_json::from_str(r#"{"disabled":false}"#).unwrap();
        assert_eq!(p.disabled, None);
    }

    #[test]
    fn token_placement_header() {
        let p = OAuth2TokenPlacement::Header {
            header: "Authorization".into(),
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: OAuth2TokenPlacement = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn token_placement_query() {
        let p = OAuth2TokenPlacement::Query {
            query: "access_token".into(),
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: OAuth2TokenPlacement = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn token_config_with_placement() {
        let tc = OAuth2TokenConfig {
            id: Some("my-token".into()),
            source: None,
            placement: Some(OAuth2TokenPlacement::Header {
                header: "Authorization".into(),
            }),
        };
        let json = serde_json::to_string(&tc).unwrap();
        let back: OAuth2TokenConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(tc, back);
    }

    #[test]
    fn additional_parameter() {
        let ap = OAuth2AdditionalParameter {
            name: "audience".into(),
            value: "https://api.example.com".into(),
            placement: Some("body".into()),
            enabled: true,
        };
        assert_eq!(ap.name, "audience");
    }

    #[test]
    fn settings() {
        let s = OAuth2Settings {
            auto_fetch_token: Some(true),
            auto_refresh_token: Some(false),
            verify_ssl: None,
            use_system_browser: None,
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: OAuth2Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn settings_use_system_browser_roundtrip() {
        let s = OAuth2Settings {
            auto_fetch_token: None,
            auto_refresh_token: None,
            verify_ssl: None,
            use_system_browser: Some(true),
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            json.contains("useSystemBrowser"),
            "must serialize as useSystemBrowser, got: {json}"
        );
        let back: OAuth2Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.use_system_browser, Some(true));
    }

    #[test]
    fn settings_verify_ssl_roundtrip() {
        let s = OAuth2Settings {
            auto_fetch_token: None,
            auto_refresh_token: None,
            verify_ssl: Some(false),
            use_system_browser: None,
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            json.contains("verifySsl"),
            "field must serialize as verifySsl, got: {json}"
        );
        let back: OAuth2Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.verify_ssl, Some(false));
    }

    #[test]
    fn settings_verify_ssl_omitted_when_none() {
        let s = OAuth2Settings {
            auto_fetch_token: None,
            auto_refresh_token: None,
            verify_ssl: None,
            use_system_browser: None,
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            !json.contains("verifySsl"),
            "None must be skipped, got: {json}"
        );
    }

    #[test]
    fn client_credentials_flow_serde() {
        let flow = OAuth2Flow::ClientCredentials {
            access_token_url: "https://auth.example.com/token".into(),
            refresh_token_url: None,
            credentials: OAuth2ClientCredentials {
                client_id: "id".into(),
                client_secret: "s".into(),
                placement: None,
            },
            scope: Some("read".into()),
            additional_parameters: None,
            token_config: None,
            settings: None,
        };
        let json = serde_json::to_string(&flow).unwrap();
        assert!(json.contains("client_credentials"));
        let back: OAuth2Flow = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, OAuth2Flow::ClientCredentials { .. }));
    }

    #[test]
    fn authorization_code_flow_with_pkce() {
        let flow = OAuth2Flow::AuthorizationCode {
            authorization_url: "https://auth.example.com/authorize".into(),
            access_token_url: "https://auth.example.com/token".into(),
            refresh_token_url: None,
            callback_url: Some("http://localhost:3000/callback".into()),
            credentials: OAuth2ClientCredentials {
                client_id: "id".into(),
                client_secret: "s".into(),
                placement: None,
            },
            scope: Some("openid".into()),
            state: Some("random-state".into()),
            pkce: Some(OAuth2PKCE {
                disabled: None,
                method: Some("S256".into()),
            }),
            additional_parameters: None,
            token_config: None,
            settings: None,
        };
        let json = serde_json::to_string(&flow).unwrap();
        assert!(json.contains("authorization_code"));
        let back: OAuth2Flow = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, OAuth2Flow::AuthorizationCode { .. }));
    }

    #[test]
    fn resource_owner_password_flow() {
        let flow = OAuth2Flow::ResourceOwnerPassword {
            access_token_url: "https://auth.example.com/token".into(),
            refresh_token_url: None,
            credentials: OAuth2ClientCredentials {
                client_id: "id".into(),
                client_secret: "s".into(),
                placement: None,
            },
            resource_owner: Some(OAuth2ResourceOwner {
                username: "user".into(),
                password: "pass".into(),
            }),
            scope: None,
            additional_parameters: None,
            token_config: None,
            settings: None,
        };
        let json = serde_json::to_string(&flow).unwrap();
        let back: OAuth2Flow = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, OAuth2Flow::ResourceOwnerPassword { .. }));
    }

    #[test]
    fn implicit_flow() {
        let flow = OAuth2Flow::Implicit {
            authorization_url: "https://auth.example.com/authorize".into(),
            callback_url: Some("http://localhost/cb".into()),
            client_id: "id".into(),
            scope: None,
            state: None,
            additional_parameters: None,
            token_config: None,
            settings: None,
        };
        let json = serde_json::to_string(&flow).unwrap();
        let back: OAuth2Flow = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, OAuth2Flow::Implicit { .. }));
    }
}
