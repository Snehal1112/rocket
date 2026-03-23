use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::error::DomainError;

// ============================================================
// HttpMethod
// ============================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Options,
    Head,
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpMethod::Get => write!(f, "GET"),
            HttpMethod::Post => write!(f, "POST"),
            HttpMethod::Put => write!(f, "PUT"),
            HttpMethod::Patch => write!(f, "PATCH"),
            HttpMethod::Delete => write!(f, "DELETE"),
            HttpMethod::Options => write!(f, "OPTIONS"),
            HttpMethod::Head => write!(f, "HEAD"),
        }
    }
}

impl FromStr for HttpMethod {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "GET" => Ok(HttpMethod::Get),
            "POST" => Ok(HttpMethod::Post),
            "PUT" => Ok(HttpMethod::Put),
            "PATCH" => Ok(HttpMethod::Patch),
            "DELETE" => Ok(HttpMethod::Delete),
            "OPTIONS" => Ok(HttpMethod::Options),
            "HEAD" => Ok(HttpMethod::Head),
            _ => Err(DomainError::InvalidInput(format!(
                "Invalid HTTP method: {}",
                s
            ))),
        }
    }
}

// ============================================================
// QueryParam
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QueryParam {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

// ============================================================
// Header
// ============================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

impl Header {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            enabled: true,
        }
    }

    pub fn disabled(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            enabled: false,
        }
    }
}

// ============================================================
// Body
// ============================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BodyMode {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "json")]
    Json,
    #[serde(rename = "xml")]
    Xml,
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "formdata")]
    FormData,
    #[serde(rename = "binary")]
    Binary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Body {
    pub mode: BodyMode,
    pub content: Option<String>,
    pub form_data: Option<Vec<FormDataEntry>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormDataEntry {
    pub key: String,
    pub value: String,
    pub entry_type: FormDataType,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FormDataType {
    Text,
    File,
}

// ============================================================
// Auth
// ============================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "authType", rename_all = "kebab-case")]
pub enum Auth {
    #[default]
    None,
    Basic {
        username: String,
        password: String,
    },
    Bearer {
        token: String,
    },
    #[serde(rename_all = "camelCase")]
    ApiKey {
        key: String,
        value: String,
        add_to: ApiKeyLocation,
    },
    #[serde(rename_all = "camelCase")]
    OAuth2 {
        grant_type: String,
        client_id: String,
        client_secret: String,
        token_url: String,
        scope: Option<String>,
        access_token: Option<String>,
        refresh_token: Option<String>,
        expires_at: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    AwsSigV4 {
        access_key: String,
        secret_key: String,
        region: String,
        service: String,
        session_token: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiKeyLocation {
    Header,
    Query,
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_param_serialization_roundtrip() {
        let param = QueryParam { key: "page".into(), value: "1".into(), enabled: true };
        let json = serde_json::to_string(&param).unwrap();
        let parsed: QueryParam = serde_json::from_str(&json).unwrap();
        assert_eq!(param, parsed);
    }

    #[test]
    fn http_method_from_string() {
        assert_eq!(HttpMethod::from_str("GET"), Ok(HttpMethod::Get));
        assert_eq!(HttpMethod::from_str("post"), Ok(HttpMethod::Post));
        assert!(HttpMethod::from_str("INVALID").is_err());
    }

    #[test]
    fn http_method_display() {
        assert_eq!(HttpMethod::Get.to_string(), "GET");
        assert_eq!(HttpMethod::Post.to_string(), "POST");
    }

    #[test]
    fn header_enabled_by_default() {
        let h = Header::new("Content-Type", "application/json");
        assert!(h.enabled);
    }

    #[test]
    fn body_mode_serialization() {
        let body = Body {
            mode: BodyMode::Json,
            content: Some("{\"key\":\"value\"}".into()),
            form_data: None,
        };
        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("\"mode\":\"json\""));
    }

    #[test]
    fn auth_none_is_default() {
        assert_eq!(Auth::default(), Auth::None);
    }

    #[test]
    fn auth_basic_serialization() {
        let auth = Auth::Basic {
            username: "user".into(),
            password: "pass".into(),
        };
        let json = serde_json::to_string(&auth).unwrap();
        assert!(json.contains("\"authType\":\"basic\""));
        assert!(json.contains("\"username\":\"user\""));
    }

    #[test]
    fn auth_tagged_deserialization() {
        let json = r#"{"authType":"bearer","token":"abc123"}"#;
        let auth: Auth = serde_json::from_str(json).unwrap();
        assert_eq!(
            auth,
            Auth::Bearer {
                token: "abc123".into()
            }
        );
    }

    #[test]
    fn auth_oauth2_serialization_roundtrip() {
        let auth = Auth::OAuth2 {
            grant_type: "client_credentials".into(),
            client_id: "my-client".into(),
            client_secret: "my-secret".into(),
            token_url: "https://auth.example.com/token".into(),
            scope: Some("read write".into()),
            access_token: Some("tok_abc".into()),
            refresh_token: None,
            expires_at: Some("2026-12-31T23:59:59Z".into()),
        };
        let json = serde_json::to_string(&auth).unwrap();
        assert!(json.contains("\"authType\":\"o-auth2\""));
        assert!(json.contains("\"grantType\":\"client_credentials\""));
        assert!(json.contains("\"clientId\":\"my-client\""));
        assert!(json.contains("\"tokenUrl\":\"https://auth.example.com/token\""));
        let parsed: Auth = serde_json::from_str(&json).unwrap();
        assert_eq!(auth, parsed);
    }

    #[test]
    fn auth_aws_sig_v4_serialization_roundtrip() {
        let auth = Auth::AwsSigV4 {
            access_key: "AKIAIOSFODNN7EXAMPLE".into(),
            secret_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into(),
            region: "us-east-1".into(),
            service: "s3".into(),
            session_token: Some("FwoGZXIvY...".into()),
        };
        let json = serde_json::to_string(&auth).unwrap();
        assert!(json.contains("\"authType\":\"aws-sig-v4\""));
        assert!(json.contains("\"accessKey\":\"AKIAIOSFODNN7EXAMPLE\""));
        assert!(json.contains("\"sessionToken\":\"FwoGZXIvY...\""));
        let parsed: Auth = serde_json::from_str(&json).unwrap();
        assert_eq!(auth, parsed);
    }
}
