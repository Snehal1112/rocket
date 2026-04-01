use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2ClientCredentials {
    pub client_id: String,
    pub client_secret: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<String>,  // "basic_auth_header" | "body"
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2ResourceOwner {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2PKCE {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,  // "S256" | "plain"
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
    pub placement: Option<OAuth2TokenPlacement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2AdditionalParameter {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<String>,  // "header" | "query" | "body"
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
}

/// OAuth2 flow — discriminated by `flow` field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "flow", rename_all = "snake_case")]
pub enum OAuth2Flow {
    #[serde(rename = "client_credentials")]
    ClientCredentials {
        #[serde(rename = "accessTokenUrl")]
        access_token_url: String,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "refreshTokenUrl")]
        refresh_token_url: Option<String>,
        credentials: OAuth2ClientCredentials,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "additionalParameters")]
        additional_parameters: Option<OAuth2AdditionalParameters>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "tokenConfig")]
        token_config: Option<OAuth2TokenConfig>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        settings: Option<OAuth2Settings>,
    },
    #[serde(rename = "resource_owner_password_credentials")]
    ResourceOwnerPassword {
        #[serde(rename = "accessTokenUrl")]
        access_token_url: String,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "refreshTokenUrl")]
        refresh_token_url: Option<String>,
        credentials: OAuth2ClientCredentials,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "resourceOwner")]
        resource_owner: Option<OAuth2ResourceOwner>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "additionalParameters")]
        additional_parameters: Option<OAuth2AdditionalParameters>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "tokenConfig")]
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
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "refreshTokenUrl")]
        refresh_token_url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "callbackUrl")]
        callback_url: Option<String>,
        credentials: OAuth2ClientCredentials,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        state: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pkce: Option<OAuth2PKCE>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "additionalParameters")]
        additional_parameters: Option<OAuth2AdditionalParameters>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "tokenConfig")]
        token_config: Option<OAuth2TokenConfig>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        settings: Option<OAuth2Settings>,
    },
    #[serde(rename = "implicit")]
    Implicit {
        #[serde(rename = "authorizationUrl")]
        authorization_url: String,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "callbackUrl")]
        callback_url: Option<String>,
        #[serde(rename = "clientId")]
        client_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        state: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "additionalParameters")]
        additional_parameters: Option<OAuth2AdditionalParameters>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "tokenConfig")]
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
            client_id: "id".into(), client_secret: "secret".into(),
            placement: Some("basic_auth_header".into()),
        };
        let json = serde_json::to_string(&creds).unwrap();
        let back: OAuth2ClientCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(creds, back);
    }

    #[test]
    fn pkce_config() {
        let pkce = OAuth2PKCE { enabled: true, method: Some("S256".into()) };
        let json = serde_json::to_string(&pkce).unwrap();
        let back: OAuth2PKCE = serde_json::from_str(&json).unwrap();
        assert_eq!(pkce, back);
    }

    #[test]
    fn token_placement_header() {
        let p = OAuth2TokenPlacement::Header { header: "Authorization".into() };
        let json = serde_json::to_string(&p).unwrap();
        let back: OAuth2TokenPlacement = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn token_placement_query() {
        let p = OAuth2TokenPlacement::Query { query: "access_token".into() };
        let json = serde_json::to_string(&p).unwrap();
        let back: OAuth2TokenPlacement = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn token_config_with_placement() {
        let tc = OAuth2TokenConfig {
            id: Some("my-token".into()),
            placement: Some(OAuth2TokenPlacement::Header { header: "Authorization".into() }),
        };
        let json = serde_json::to_string(&tc).unwrap();
        let back: OAuth2TokenConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(tc, back);
    }

    #[test]
    fn additional_parameter() {
        let ap = OAuth2AdditionalParameter {
            name: "audience".into(), value: "https://api.example.com".into(),
            placement: Some("body".into()),
        };
        assert_eq!(ap.name, "audience");
    }

    #[test]
    fn settings() {
        let s = OAuth2Settings { auto_fetch_token: Some(true), auto_refresh_token: Some(false) };
        let json = serde_json::to_string(&s).unwrap();
        let back: OAuth2Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn client_credentials_flow_serde() {
        let flow = OAuth2Flow::ClientCredentials {
            access_token_url: "https://auth.example.com/token".into(),
            refresh_token_url: None,
            credentials: OAuth2ClientCredentials { client_id: "id".into(), client_secret: "s".into(), placement: None },
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
            credentials: OAuth2ClientCredentials { client_id: "id".into(), client_secret: "s".into(), placement: None },
            scope: Some("openid".into()),
            state: Some("random-state".into()),
            pkce: Some(OAuth2PKCE { enabled: true, method: Some("S256".into()) }),
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
            credentials: OAuth2ClientCredentials { client_id: "id".into(), client_secret: "s".into(), placement: None },
            resource_owner: Some(OAuth2ResourceOwner { username: "user".into(), password: "pass".into() }),
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
