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
pub struct OAuth2ResourceOwner {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
pub struct OAuth2TokenConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<OAuth2TokenPlacement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
}
