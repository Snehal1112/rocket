use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::description::Description;
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Description>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Description>,
}

impl Header {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            enabled: true,
            description: None,
        }
    }

    pub fn disabled(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            enabled: false,
            description: None,
        }
    }
}

// ============================================================
// PathParam
// ============================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathParam {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Description>,
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
    #[serde(rename = "sparql")]
    Sparql,
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
    /// Path to a local file used when mode is Binary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
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
        placement: String,  // "header" | "query"
    },
    OAuth2(crate::oauth2::OAuth2Flow),
    #[serde(rename_all = "camelCase")]
    AwsSigV4 {
        access_key: String,
        secret_key: String,
        region: String,
        service: String,
        session_token: Option<String>,
        profile_name: Option<String>,
    },
    /// Inherits auth from the parent collection or folder.
    Inherit,
    Wsse {
        username: String,
        password: String,
    },
    Digest {
        username: String,
        password: String,
    },
    Ntlm {
        username: String,
        password: String,
        domain: String,
    },
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_param_serialization_roundtrip() {
        let param = QueryParam { key: "page".into(), value: "1".into(), enabled: true, description: None };
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
            file_path: None,
        };
        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("\"mode\":\"json\""));
    }

    #[test]
    fn body_mode_sparql_serialization() {
        let body = Body {
            mode: BodyMode::Sparql,
            content: Some("SELECT ?s WHERE { ?s ?p ?o }".into()),
            form_data: None,
            file_path: None,
        };
        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("\"mode\":\"sparql\""));
        let back: Body = serde_json::from_str(&json).unwrap();
        assert_eq!(body, back);
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
        use crate::oauth2::{OAuth2Flow, OAuth2ClientCredentials};
        let auth = Auth::OAuth2(OAuth2Flow::ClientCredentials {
            access_token_url: "https://auth.example.com/token".into(),
            refresh_token_url: None,
            credentials: OAuth2ClientCredentials {
                client_id: "my-client".into(),
                client_secret: "my-secret".into(),
                placement: None,
            },
            scope: Some("read write".into()),
            additional_parameters: None,
            token_config: None,
            settings: None,
        });
        let json = serde_json::to_string(&auth).unwrap();
        assert!(json.contains("\"authType\":\"o-auth2\""));
        assert!(json.contains("\"flow\":\"client_credentials\""));
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
            profile_name: None,
        };
        let json = serde_json::to_string(&auth).unwrap();
        assert!(json.contains("\"authType\":\"aws-sig-v4\""));
        assert!(json.contains("\"accessKey\":\"AKIAIOSFODNN7EXAMPLE\""));
        assert!(json.contains("\"sessionToken\":\"FwoGZXIvY...\""));
        let parsed: Auth = serde_json::from_str(&json).unwrap();
        assert_eq!(auth, parsed);
    }

    #[test]
    fn auth_awsv4_with_profile_name() {
        let auth = Auth::AwsSigV4 {
            access_key: "AK".into(),
            secret_key: "SK".into(),
            region: "us-east-1".into(),
            service: "s3".into(),
            session_token: None,
            profile_name: Some("prod".into()),
        };
        let json = serde_json::to_string(&auth).unwrap();
        assert!(json.contains("profileName") || json.contains("profile_name"));
        let back: Auth = serde_json::from_str(&json).unwrap();
        assert_eq!(auth, back);
    }

    #[test]
    fn header_has_description() {
        let h = Header {
            key: "Auth".into(),
            value: "Bearer tk".into(),
            enabled: true,
            description: Some(Description::text("Auth header")),
        };
        assert!(h.description.is_some());
    }

    #[test]
    fn query_param_has_description() {
        let p = QueryParam {
            key: "page".into(),
            value: "1".into(),
            enabled: true,
            description: Some(Description::text("Page number")),
        };
        assert!(p.description.is_some());
    }

    #[test]
    fn path_param_full() {
        let p = PathParam { name: "id".into(), value: "123".into(), description: None };
        assert_eq!(p.name, "id");
    }

    #[test]
    fn auth_inherit_serde() {
        let auth = Auth::Inherit;
        let json = serde_json::to_string(&auth).unwrap();
        let back: Auth = serde_json::from_str(&json).unwrap();
        assert_eq!(auth, back);
    }

    #[test]
    fn auth_wsse_serde() {
        let auth = Auth::Wsse { username: "user".into(), password: "pass".into() };
        let json = serde_json::to_string(&auth).unwrap();
        let back: Auth = serde_json::from_str(&json).unwrap();
        assert_eq!(auth, back);
    }

    #[test]
    fn auth_digest_serde() {
        let auth = Auth::Digest { username: "admin".into(), password: "secret".into() };
        let json = serde_json::to_string(&auth).unwrap();
        let back: Auth = serde_json::from_str(&json).unwrap();
        assert_eq!(auth, back);
    }

    #[test]
    fn auth_ntlm_serde() {
        let auth = Auth::Ntlm {
            username: "CORP\\user".into(),
            password: "p".into(),
            domain: "CORP".into(),
        };
        let json = serde_json::to_string(&auth).unwrap();
        let back: Auth = serde_json::from_str(&json).unwrap();
        assert_eq!(auth, back);
    }

    #[test]
    fn auth_apikey_placement_values() {
        let auth = Auth::ApiKey { key: "X-Key".into(), value: "123".into(), placement: "header".into() };
        let json = serde_json::to_string(&auth).unwrap();
        let back: Auth = serde_json::from_str(&json).unwrap();
        assert_eq!(auth, back);

        let auth2 = Auth::ApiKey { key: "token".into(), value: "abc".into(), placement: "query".into() };
        let json2 = serde_json::to_string(&auth2).unwrap();
        let back2: Auth = serde_json::from_str(&json2).unwrap();
        assert_eq!(auth2, back2);
    }
}
