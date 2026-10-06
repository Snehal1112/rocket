use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::description::Description;
use crate::error::DomainError;

// ============================================================
// HttpMethod
// ============================================================

/// An HTTP request method. The nine standard methods have their own variant, and any other
/// valid method token is kept as `Custom`, exactly as written, because methods are
/// case-sensitive on the wire. It serializes as a plain string so request files, IPC payloads
/// and scripts all keep exchanging method names as text.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Options,
    Head,
    Trace,
    Connect,
    Custom(String),
}

/// True when `s` is a legal HTTP method token (RFC 9110 `token`, at most 64 characters).
/// Anything else, such as text with spaces or line breaks, must never reach the request line.
pub fn is_valid_method_token(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
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
            HttpMethod::Trace => write!(f, "TRACE"),
            HttpMethod::Connect => write!(f, "CONNECT"),
            HttpMethod::Custom(name) => write!(f, "{name}"),
        }
    }
}

impl FromStr for HttpMethod {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_uppercase().as_str() {
            "GET" => Ok(HttpMethod::Get),
            "POST" => Ok(HttpMethod::Post),
            "PUT" => Ok(HttpMethod::Put),
            "PATCH" => Ok(HttpMethod::Patch),
            "DELETE" => Ok(HttpMethod::Delete),
            "OPTIONS" => Ok(HttpMethod::Options),
            "HEAD" => Ok(HttpMethod::Head),
            "TRACE" => Ok(HttpMethod::Trace),
            "CONNECT" => Ok(HttpMethod::Connect),
            _ if is_valid_method_token(s) => Ok(HttpMethod::Custom(s.to_string())),
            _ => Err(DomainError::InvalidInput(format!(
                "Invalid HTTP method: {s}"
            ))),
        }
    }
}

impl TryFrom<String> for HttpMethod {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<HttpMethod> for String {
    fn from(method: HttpMethod) -> Self {
        method.to_string()
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
    /// A JSON GraphQL payload built by the app. Resolution escapes values inside its strings and
    /// then turns it into `Json`.
    #[serde(rename = "graphql")]
    GraphQl,
    #[serde(rename = "xml")]
    Xml,
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "sparql")]
    Sparql,
    #[serde(rename = "formurlencoded")]
    FormUrlEncoded,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Description>,
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
        placement: String, // "header" | "query"
    },
    OAuth2(Box<crate::oauth2::OAuth2Flow>),
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
    OAuth1(Box<OAuth1Auth>),
}

/// OAuth 1.0 configuration. Every field is optional, matching the OpenCollection `AuthOAuth1` shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OAuth1Auth {
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
    /// One of `HMAC-SHA1`, `HMAC-SHA256`, `HMAC-SHA512`, `RSA-SHA1`, `RSA-SHA256`,
    /// `RSA-SHA512` or `PLAINTEXT`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_key: Option<OAuth1PrivateKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realm: Option<String>,
    /// `header`, `query` or `body`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_body_hash: Option<bool>,
}

/// PEM private key for the RSA signature methods.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuth1PrivateKey {
    /// `text` for an inline key, `file` for a file path.
    #[serde(rename = "type")]
    pub key_type: String,
    pub value: String,
}

// ============================================================
// RequestSettings
// ============================================================

/// A setting value that can be a concrete value or "inherit".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestSettingValue<T> {
    Value(T),
    Inherit(String),
}

/// Request-level execution settings.
/// Values are optional; None means "inherit from collection/folder".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encode_url: Option<RequestSettingValue<bool>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<RequestSettingValue<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow_redirects: Option<RequestSettingValue<bool>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_redirects: Option<RequestSettingValue<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_ssl: Option<RequestSettingValue<bool>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_param_serialization_roundtrip() {
        let param = QueryParam {
            key: "page".into(),
            value: "1".into(),
            enabled: true,
            description: None,
        };
        let json = serde_json::to_string(&param).unwrap();
        let parsed: QueryParam = serde_json::from_str(&json).unwrap();
        assert_eq!(param, parsed);
    }

    #[test]
    fn http_method_from_string() {
        assert_eq!(HttpMethod::from_str("GET"), Ok(HttpMethod::Get));
        assert_eq!(HttpMethod::from_str("post"), Ok(HttpMethod::Post));
        assert!(HttpMethod::from_str("NOT A METHOD").is_err());
    }

    #[test]
    fn http_method_trace_and_connect_are_standard() {
        assert_eq!(HttpMethod::from_str("trace"), Ok(HttpMethod::Trace));
        assert_eq!(HttpMethod::from_str("CONNECT"), Ok(HttpMethod::Connect));
        assert_eq!(HttpMethod::Trace.to_string(), "TRACE");
        assert_eq!(HttpMethod::Connect.to_string(), "CONNECT");
    }

    #[test]
    fn http_method_custom_token_is_kept_as_written() {
        let purge = HttpMethod::from_str("PURGE").expect("valid token");
        assert_eq!(purge, HttpMethod::Custom("PURGE".into()));
        assert_eq!(purge.to_string(), "PURGE");
        // Methods are case-sensitive on the wire, so a custom token is not upper-cased.
        assert_eq!(
            HttpMethod::from_str("m-search")
                .expect("valid token")
                .to_string(),
            "m-search"
        );
        // A standard name in any case maps to the standard variant, as before.
        assert_eq!(HttpMethod::from_str("Get"), Ok(HttpMethod::Get));
    }

    #[test]
    fn http_method_rejects_non_token_text() {
        for bad in [
            "",
            "GET ME",
            "PU RGE",
            "A/B",
            "BAD\n",
            "BAD\r\nHost: x",
            "caf\u{e9}",
        ] {
            assert!(
                HttpMethod::from_str(bad).is_err(),
                "{bad:?} must be rejected"
            );
        }
        assert!(HttpMethod::from_str(&"A".repeat(65)).is_err());
    }

    #[test]
    fn http_method_serializes_as_a_plain_string() {
        assert_eq!(
            serde_json::to_string(&HttpMethod::Get).expect("ser"),
            "\"GET\""
        );
        assert_eq!(
            serde_json::to_string(&HttpMethod::Custom("PURGE".into())).expect("ser"),
            "\"PURGE\""
        );
        assert_eq!(
            serde_json::from_str::<HttpMethod>("\"TRACE\"").expect("de"),
            HttpMethod::Trace
        );
        assert_eq!(
            serde_json::from_str::<HttpMethod>("\"PURGE\"").expect("de"),
            HttpMethod::Custom("PURGE".into())
        );
        assert!(serde_json::from_str::<HttpMethod>("\"BAD METHOD\"").is_err());
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
        use crate::oauth2::{OAuth2ClientCredentials, OAuth2Flow};
        let auth = Auth::OAuth2(Box::new(OAuth2Flow::ClientCredentials {
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
        }));
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
        let p = PathParam {
            name: "id".into(),
            value: "123".into(),
            description: None,
        };
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
    fn auth_oauth1_serializes_as_a_flat_o_auth1_object() {
        // The frontend reads and writes this exact shape, so pin it.
        let auth = Auth::OAuth1(Box::new(OAuth1Auth {
            consumer_key: Some("ck".into()),
            signature_method: Some("HMAC-SHA1".into()),
            ..Default::default()
        }));
        let json = serde_json::to_value(&auth).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "authType": "o-auth1",
                "consumerKey": "ck",
                "signatureMethod": "HMAC-SHA1",
            })
        );
        assert_eq!(serde_json::from_value::<Auth>(json).unwrap(), auth);
    }

    #[test]
    fn auth_wsse_serde() {
        let auth = Auth::Wsse {
            username: "user".into(),
            password: "pass".into(),
        };
        let json = serde_json::to_string(&auth).unwrap();
        let back: Auth = serde_json::from_str(&json).unwrap();
        assert_eq!(auth, back);
    }

    #[test]
    fn auth_digest_serde() {
        let auth = Auth::Digest {
            username: "admin".into(),
            password: "secret".into(),
        };
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
        let auth = Auth::ApiKey {
            key: "X-Key".into(),
            value: "123".into(),
            placement: "header".into(),
        };
        let json = serde_json::to_string(&auth).unwrap();
        let back: Auth = serde_json::from_str(&json).unwrap();
        assert_eq!(auth, back);

        let auth2 = Auth::ApiKey {
            key: "token".into(),
            value: "abc".into(),
            placement: "query".into(),
        };
        let json2 = serde_json::to_string(&auth2).unwrap();
        let back2: Auth = serde_json::from_str(&json2).unwrap();
        assert_eq!(auth2, back2);
    }
}
