//! OpenCollection YAML file-format structs.
//! These mirror the OpenCollection JSON schema for on-disk YAML serialization.
//! Domain types from rocket-shared are re-used where field names match.

use serde::{Deserialize, Serialize};

// Re-export domain types that map directly to schema types.
pub use rocket_shared::description::{Description as OcDescription, Documentation as OcDocumentation};
pub use rocket_shared::variable_value::{VariableValue as OcVariableValue, VariableValueVariant as OcVariableValueVariant};

/// OpenCollection Variable — schema field names: name, value, description, disabled.
/// Our domain Variable uses `key` instead of `name` and `enabled` instead of `disabled`,
/// so we need a separate YAML struct.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcVariable {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<OcVariableValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<OcDescription>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

/// OpenCollection SecretVariable — schema: { secret: true, name, description, disabled, type }.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcSecretVariable {
    pub secret: bool,  // always true
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<OcDescription>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
    pub secret_type: Option<String>,  // "string"|"number"|"boolean"|"null"|"object"
}

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
        additional_parameters: Option<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        token_config: Option<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        settings: Option<serde_json::Value>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::description::Description;
    use rocket_shared::variable_value::VariableValue;

    #[test]
    fn oc_description_yaml_string() {
        let yaml = "\"A simple description\"";
        let desc: OcDescription = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(desc.content(), Some("A simple description"));
    }

    #[test]
    fn oc_description_yaml_object() {
        let yaml = "content: \"# Docs\"\ntype: text/markdown";
        let desc: OcDescription = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(desc.content(), Some("# Docs"));
        assert_eq!(desc.content_type(), Some("text/markdown"));
    }

    #[test]
    fn oc_description_yaml_null() {
        let yaml = "null";
        let desc: OcDescription = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(desc.content(), None);
    }

    #[test]
    fn oc_variable_yaml_simple() {
        let yaml = "name: BASE_URL\nvalue: https://api.example.com";
        let var: OcVariable = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(var.name, "BASE_URL");
        assert_eq!(var.value.as_ref().unwrap().data(), "https://api.example.com");
    }

    #[test]
    fn oc_variable_yaml_typed_value() {
        let yaml = "name: COUNT\nvalue:\n  type: number\n  data: \"42\"";
        let var: OcVariable = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(var.value.as_ref().unwrap().value_type(), Some("number"));
        assert_eq!(var.value.as_ref().unwrap().data(), "42");
    }

    #[test]
    fn oc_variable_yaml_with_description_and_disabled() {
        let yaml = "name: HOST\nvalue: localhost\ndescription: The API host\ndisabled: true";
        let var: OcVariable = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(var.disabled, Some(true));
        assert!(var.description.is_some());
    }

    #[test]
    fn oc_secret_variable_yaml() {
        let yaml = "secret: true\nname: API_KEY\ntype: string\ndisabled: false";
        let sv: OcSecretVariable = serde_yaml::from_str(yaml).unwrap();
        assert!(sv.secret);
        assert_eq!(sv.name, "API_KEY");
        assert_eq!(sv.secret_type, Some("string".into()));
    }

    #[test]
    fn oc_variable_yaml_roundtrip() {
        let var = OcVariable {
            name: "HOST".into(),
            value: Some(VariableValue::simple("localhost")),
            description: Some(Description::text("Server host")),
            disabled: Some(false),
        };
        let yaml = serde_yaml::to_string(&var).unwrap();
        let back: OcVariable = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(var, back);
    }

    #[test]
    fn oc_variable_value_variant_yaml() {
        let yaml = "title: Production\nselected: true\nvalue: https://prod.example.com";
        let variant: OcVariableValueVariant = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(variant.title, "Production");
        assert!(variant.selected);
    }

    #[test]
    fn oc_auth_inherit_yaml() {
        let yaml = "inherit";
        let auth: OcAuth = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(auth, OcAuth::Inherit(s) if s == "inherit"));
    }

    #[test]
    fn oc_auth_basic_yaml() {
        let yaml = "type: basic\nusername: user\npassword: pass";
        let auth: OcAuth = serde_yaml::from_str(yaml).unwrap();
        match auth {
            OcAuth::Typed(OcAuthTyped::Basic { username, password }) => {
                assert_eq!(username, "user");
                assert_eq!(password, "pass");
            }
            _ => panic!("expected Basic"),
        }
    }

    #[test]
    fn oc_auth_bearer_yaml() {
        let yaml = "type: bearer\ntoken: my-token-123";
        let auth: OcAuth = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(auth, OcAuth::Typed(OcAuthTyped::Bearer { .. })));
    }

    #[test]
    fn oc_auth_apikey_yaml() {
        let yaml = "type: apikey\nkey: X-API-Key\nvalue: abc123\nplacement: header";
        let auth: OcAuth = serde_yaml::from_str(yaml).unwrap();
        match auth {
            OcAuth::Typed(OcAuthTyped::ApiKey {
                key,
                value,
                placement,
            }) => {
                assert_eq!(key, "X-API-Key");
                assert_eq!(value, "abc123");
                assert_eq!(placement, Some("header".into()));
            }
            _ => panic!("expected ApiKey"),
        }
    }

    #[test]
    fn oc_auth_awsv4_yaml() {
        let yaml =
            "type: awsv4\naccessKeyId: AKIA...\nsecretAccessKey: secret\nregion: us-east-1\nservice: s3";
        let auth: OcAuth = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(auth, OcAuth::Typed(OcAuthTyped::AwsV4 { .. })));
    }

    #[test]
    fn oc_auth_digest_yaml() {
        let yaml = "type: digest\nusername: admin\npassword: secret";
        let auth: OcAuth = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(auth, OcAuth::Typed(OcAuthTyped::Digest { .. })));
    }

    #[test]
    fn oc_auth_oauth2_client_credentials_yaml() {
        let yaml = "type: oauth2\nflow: client_credentials\naccessTokenUrl: https://auth.example.com/token\ncredentials:\n  clientId: my-id\n  clientSecret: my-secret";
        let auth: OcAuth = serde_yaml::from_str(yaml).unwrap();
        match auth {
            OcAuth::Typed(OcAuthTyped::OAuth2 {
                flow,
                access_token_url,
                credentials,
                ..
            }) => {
                assert_eq!(flow, "client_credentials");
                assert_eq!(
                    access_token_url,
                    Some("https://auth.example.com/token".into())
                );
                assert!(credentials.is_some());
            }
            _ => panic!("expected OAuth2"),
        }
    }

    #[test]
    fn oc_auth_oauth2_authorization_code_yaml() {
        let yaml = "type: oauth2\nflow: authorization_code\nauthorizationUrl: https://auth.example.com/authorize\naccessTokenUrl: https://auth.example.com/token\ncredentials:\n  clientId: id\n  clientSecret: secret\npkce:\n  enabled: true\n  method: S256";
        let auth: OcAuth = serde_yaml::from_str(yaml).unwrap();
        match auth {
            OcAuth::Typed(OcAuthTyped::OAuth2 { flow, pkce, .. }) => {
                assert_eq!(flow, "authorization_code");
                assert!(pkce.is_some());
                assert_eq!(pkce.unwrap().method, Some("S256".into()));
            }
            _ => panic!("expected OAuth2"),
        }
    }

    #[test]
    fn inheritable_boolean_value() {
        let yaml = "true";
        let v: InheritableBoolean = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(v, InheritableBoolean::Value(true));
    }

    #[test]
    fn inheritable_boolean_inherit() {
        let yaml = "inherit";
        let v: InheritableBoolean = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(v, InheritableBoolean::Inherit("inherit".into()));
    }

    #[test]
    fn inheritable_number_value() {
        let yaml = "5000";
        let v: InheritableNumber = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(v, InheritableNumber::Value(5000.0));
    }

    #[test]
    fn inheritable_number_inherit() {
        let yaml = "inherit";
        let v: InheritableNumber = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(v, InheritableNumber::Inherit("inherit".into()));
    }

    #[test]
    fn oc_http_request_settings_yaml() {
        let yaml = "encodeUrl: true\ntimeout: 30000\nfollowRedirects: inherit\nmaxRedirects: 5";
        let settings: OcHttpRequestSettings = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(settings.encode_url, Some(InheritableBoolean::Value(true)));
        assert_eq!(settings.timeout, Some(InheritableNumber::Value(30000.0)));
        assert_eq!(settings.follow_redirects, Some(InheritableBoolean::Inherit("inherit".into())));
        assert_eq!(settings.max_redirects, Some(InheritableNumber::Value(5.0)));
    }

    #[test]
    fn oc_http_request_settings_roundtrip() {
        let settings = OcHttpRequestSettings {
            encode_url: Some(InheritableBoolean::Value(false)),
            timeout: Some(InheritableNumber::Inherit("inherit".into())),
            follow_redirects: None,
            max_redirects: Some(InheritableNumber::Value(10.0)),
        };
        let yaml = serde_yaml::to_string(&settings).unwrap();
        let back: OcHttpRequestSettings = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(settings, back);
    }

    #[test]
    fn oc_graphql_request_settings_yaml() {
        let yaml = "encodeUrl: false\ntimeout: inherit";
        let settings: OcGraphQLRequestSettings = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(settings.encode_url, Some(InheritableBoolean::Value(false)));
        assert_eq!(settings.timeout, Some(InheritableNumber::Inherit("inherit".into())));
    }
}
