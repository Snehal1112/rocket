//! Conversions between OpenCollection YAML structs (Oc*) and domain types.
//!
//! OcDescription is a re-export of Description (same type), so no conversion
//! is needed for descriptions — they flow through unchanged.

use crate::opencollection::*;
use rocket_shared::oauth2::{
    OAuth2ClientCredentials, OAuth2Flow, OAuth2PKCE, OAuth2ResourceOwner,
};
use rocket_shared::types::{Auth, Body, BodyMode, FormDataEntry, FormDataType, Header, PathParam, QueryParam};

// ============================================================
// Header conversions
// ============================================================

impl From<OcHttpRequestHeader> for Header {
    fn from(oc: OcHttpRequestHeader) -> Self {
        Header {
            key: oc.name,
            value: oc.value,
            enabled: !oc.disabled.unwrap_or(false),
            description: oc.description,
        }
    }
}

impl From<Header> for OcHttpRequestHeader {
    fn from(h: Header) -> Self {
        OcHttpRequestHeader {
            name: h.key,
            value: h.value,
            description: h.description,
            // Omit disabled entirely when enabled (cleaner YAML output).
            disabled: if h.enabled { None } else { Some(true) },
        }
    }
}

// ============================================================
// Param conversions
// ============================================================

impl From<OcHttpRequestParam> for QueryParam {
    fn from(oc: OcHttpRequestParam) -> Self {
        QueryParam {
            key: oc.name,
            value: oc.value,
            enabled: !oc.disabled.unwrap_or(false),
            description: oc.description,
        }
    }
}

impl From<QueryParam> for OcHttpRequestParam {
    fn from(q: QueryParam) -> Self {
        OcHttpRequestParam {
            name: q.key,
            value: q.value,
            description: q.description,
            param_type: Some("query".into()),
            disabled: if q.enabled { None } else { Some(true) },
        }
    }
}

impl From<OcHttpRequestParam> for PathParam {
    fn from(oc: OcHttpRequestParam) -> Self {
        PathParam {
            name: oc.name,
            value: oc.value,
            description: oc.description,
        }
    }
}

impl From<PathParam> for OcHttpRequestParam {
    fn from(p: PathParam) -> Self {
        OcHttpRequestParam {
            name: p.name,
            value: p.value,
            description: p.description,
            param_type: Some("path".into()),
            // Path params have no enabled/disabled concept in the schema.
            disabled: None,
        }
    }
}

/// Split OC params into query params and path params by their type field.
/// Params with no type or an unrecognised type default to query.
pub fn split_params(params: Vec<OcHttpRequestParam>) -> (Vec<QueryParam>, Vec<PathParam>) {
    let mut query = Vec::new();
    let mut path = Vec::new();
    for p in params {
        match p.param_type.as_deref() {
            Some("path") => path.push(PathParam::from(p)),
            _ => query.push(QueryParam::from(p)),
        }
    }
    (query, path)
}

/// Merge query and path params back into a single OC param list.
/// Query params come first, path params follow.
pub fn merge_params(query: &[QueryParam], path: &[PathParam]) -> Vec<OcHttpRequestParam> {
    let mut params: Vec<OcHttpRequestParam> =
        query.iter().cloned().map(OcHttpRequestParam::from).collect();
    params.extend(path.iter().cloned().map(OcHttpRequestParam::from));
    params
}

// ============================================================
// Body conversions
// ============================================================

impl From<OcHttpRequestBody> for Body {
    fn from(oc: OcHttpRequestBody) -> Self {
        match oc {
            OcHttpRequestBody::Json { data } => Body {
                mode: BodyMode::Json,
                content: Some(data),
                form_data: None,
                file_path: None,
            },
            OcHttpRequestBody::Text { data } => Body {
                mode: BodyMode::Text,
                content: Some(data),
                form_data: None,
                file_path: None,
            },
            OcHttpRequestBody::Xml { data } => Body {
                mode: BodyMode::Xml,
                content: Some(data),
                form_data: None,
                file_path: None,
            },
            // No Sparql mode in domain — map to Text.
            OcHttpRequestBody::Sparql { data } => Body {
                mode: BodyMode::Text,
                content: Some(data),
                form_data: None,
                file_path: None,
            },
            OcHttpRequestBody::FormUrlEncoded { data } => Body {
                mode: BodyMode::FormData,
                content: None,
                form_data: Some(data.into_iter().map(form_field_to_entry).collect()),
                file_path: None,
            },
            OcHttpRequestBody::MultipartForm { data } => Body {
                mode: BodyMode::FormData,
                content: None,
                form_data: Some(data.into_iter().map(multipart_to_entry).collect()),
                file_path: None,
            },
            OcHttpRequestBody::File { data } => Body {
                mode: BodyMode::Binary,
                content: None,
                form_data: None,
                file_path: data.first().map(|f| f.file_path.clone()),
            },
        }
    }
}

/// Convert an OC form field to a domain form-data entry.
fn form_field_to_entry(f: OcFormField) -> FormDataEntry {
    FormDataEntry {
        key: f.name,
        value: f.value,
        entry_type: FormDataType::Text,
        enabled: !f.disabled.unwrap_or(false),
    }
}

/// Convert an OC multipart form part to a domain form-data entry.
fn multipart_to_entry(p: OcMultipartFormPart) -> FormDataEntry {
    let entry_type = if p.part_type == "file" {
        FormDataType::File
    } else {
        FormDataType::Text
    };
    let value = match p.value {
        OcMultipartValue::Single(s) => s,
        OcMultipartValue::Multiple(v) => v.join(","),
    };
    FormDataEntry {
        key: p.name,
        value,
        entry_type,
        enabled: !p.disabled.unwrap_or(false),
    }
}

impl From<Body> for OcHttpRequestBody {
    fn from(b: Body) -> Self {
        match b.mode {
            BodyMode::Json => OcHttpRequestBody::Json {
                data: b.content.unwrap_or_default(),
            },
            BodyMode::Text => OcHttpRequestBody::Text {
                data: b.content.unwrap_or_default(),
            },
            BodyMode::Xml => OcHttpRequestBody::Xml {
                data: b.content.unwrap_or_default(),
            },
            BodyMode::FormData => {
                let entries = b.form_data.unwrap_or_default();
                // If any entry is a file type, emit multipart-form.
                if entries.iter().any(|e| e.entry_type == FormDataType::File) {
                    OcHttpRequestBody::MultipartForm {
                        data: entries.into_iter().map(entry_to_multipart).collect(),
                    }
                } else {
                    OcHttpRequestBody::FormUrlEncoded {
                        data: entries.into_iter().map(entry_to_form_field).collect(),
                    }
                }
            }
            BodyMode::Binary => OcHttpRequestBody::File {
                data: b
                    .file_path
                    .map(|fp| vec![OcFileBodyVariant {
                        file_path: fp,
                        content_type: None,
                        selected: true,
                    }])
                    .unwrap_or_default(),
            },
            BodyMode::None => OcHttpRequestBody::Text {
                data: String::new(),
            },
        }
    }
}

/// Convert a domain form-data entry back to an OC form field.
fn entry_to_form_field(e: FormDataEntry) -> OcFormField {
    OcFormField {
        name: e.key,
        value: e.value,
        description: None,
        disabled: if e.enabled { None } else { Some(true) },
    }
}

/// Convert a domain form-data entry back to an OC multipart part.
fn entry_to_multipart(e: FormDataEntry) -> OcMultipartFormPart {
    OcMultipartFormPart {
        name: e.key,
        part_type: match e.entry_type {
            FormDataType::File => "file".into(),
            FormDataType::Text => "text".into(),
        },
        value: OcMultipartValue::Single(e.value),
        description: None,
        content_type: None,
        disabled: if e.enabled { None } else { Some(true) },
    }
}

// ============================================================
// Auth conversions
// ============================================================

impl From<OcAuth> for Auth {
    fn from(oc: OcAuth) -> Self {
        match oc {
            OcAuth::Inherit(ref s) if s == "inherit" => Auth::Inherit,
            OcAuth::Inherit(_) => Auth::None,
            OcAuth::Typed(typed) => typed.into(),
        }
    }
}

impl From<OcAuthTyped> for Auth {
    fn from(oc: OcAuthTyped) -> Self {
        match oc {
            OcAuthTyped::None => Auth::None,
            OcAuthTyped::Basic { username, password } => Auth::Basic { username, password },
            OcAuthTyped::Bearer { token } => Auth::Bearer { token },
            OcAuthTyped::ApiKey { key, value, placement } => Auth::ApiKey {
                key,
                value,
                placement: placement.unwrap_or_else(|| "header".into()),
            },
            OcAuthTyped::Digest { username, password } => Auth::Digest { username, password },
            OcAuthTyped::Ntlm { username, password, domain } => {
                Auth::Ntlm { username, password, domain }
            }
            OcAuthTyped::Wsse { username, password } => Auth::Wsse { username, password },
            OcAuthTyped::AwsV4 {
                access_key_id,
                secret_access_key,
                region,
                service,
                session_token,
                profile_name,
            } => Auth::AwsSigV4 {
                access_key: access_key_id,
                secret_key: secret_access_key,
                region: region.unwrap_or_default(),
                service: service.unwrap_or_default(),
                session_token,
                profile_name,
            },
            OcAuthTyped::OAuth2 {
                flow,
                access_token_url,
                refresh_token_url,
                authorization_url,
                callback_url,
                credentials,
                resource_owner,
                scope,
                state,
                pkce,
                additional_parameters,
                token_config,
                settings,
            } => {
                let creds = credentials.map(oc_creds_to_domain).unwrap_or_else(|| {
                    OAuth2ClientCredentials {
                        client_id: String::new(),
                        client_secret: String::new(),
                        placement: None,
                    }
                });
                let add_params = additional_parameters
                    .and_then(|v| serde_json::from_value(v).ok());
                let tok_cfg = token_config
                    .and_then(|v| serde_json::from_value(v).ok());
                let setts = settings
                    .and_then(|v| serde_json::from_value(v).ok());

                let oauth_flow = match flow.as_str() {
                    "client_credentials" => OAuth2Flow::ClientCredentials {
                        access_token_url: access_token_url.unwrap_or_default(),
                        refresh_token_url,
                        credentials: creds,
                        scope,
                        additional_parameters: add_params,
                        token_config: tok_cfg,
                        settings: setts,
                    },
                    "resource_owner_password_credentials" => {
                        OAuth2Flow::ResourceOwnerPassword {
                            access_token_url: access_token_url.unwrap_or_default(),
                            refresh_token_url,
                            credentials: creds,
                            resource_owner: resource_owner.map(oc_ro_to_domain),
                            scope,
                            additional_parameters: add_params,
                            token_config: tok_cfg,
                            settings: setts,
                        }
                    }
                    "authorization_code" => OAuth2Flow::AuthorizationCode {
                        authorization_url: authorization_url.unwrap_or_default(),
                        access_token_url: access_token_url.unwrap_or_default(),
                        refresh_token_url,
                        callback_url,
                        credentials: creds,
                        scope,
                        state,
                        pkce: pkce.map(oc_pkce_to_domain),
                        additional_parameters: add_params,
                        token_config: tok_cfg,
                        settings: setts,
                    },
                    "implicit" | _ => OAuth2Flow::Implicit {
                        authorization_url: authorization_url.unwrap_or_default(),
                        callback_url,
                        client_id: creds.client_id,
                        scope,
                        state,
                        additional_parameters: add_params,
                        token_config: tok_cfg,
                        settings: setts,
                    },
                };
                Auth::OAuth2(oauth_flow)
            }
        }
    }
}

fn oc_creds_to_domain(c: OcOAuth2Credentials) -> OAuth2ClientCredentials {
    OAuth2ClientCredentials {
        client_id: c.client_id,
        client_secret: c.client_secret,
        placement: c.placement,
    }
}

fn oc_ro_to_domain(r: OcOAuth2ResourceOwner) -> OAuth2ResourceOwner {
    OAuth2ResourceOwner {
        username: r.username,
        password: r.password,
    }
}

fn oc_pkce_to_domain(p: OcOAuth2PKCE) -> OAuth2PKCE {
    OAuth2PKCE {
        enabled: p.enabled,
        method: p.method,
    }
}

impl From<Auth> for OcAuth {
    fn from(auth: Auth) -> Self {
        match auth {
            Auth::Inherit => OcAuth::Inherit("inherit".into()),
            Auth::None => OcAuth::Typed(OcAuthTyped::None),
            Auth::Basic { username, password } => {
                OcAuth::Typed(OcAuthTyped::Basic { username, password })
            }
            Auth::Bearer { token } => OcAuth::Typed(OcAuthTyped::Bearer { token }),
            Auth::ApiKey { key, value, placement } => OcAuth::Typed(OcAuthTyped::ApiKey {
                key,
                value,
                placement: Some(placement),
            }),
            Auth::Digest { username, password } => {
                OcAuth::Typed(OcAuthTyped::Digest { username, password })
            }
            Auth::Ntlm { username, password, domain } => {
                OcAuth::Typed(OcAuthTyped::Ntlm { username, password, domain })
            }
            Auth::Wsse { username, password } => {
                OcAuth::Typed(OcAuthTyped::Wsse { username, password })
            }
            Auth::AwsSigV4 {
                access_key,
                secret_key,
                region,
                service,
                session_token,
                profile_name,
            } => OcAuth::Typed(OcAuthTyped::AwsV4 {
                access_key_id: access_key,
                secret_access_key: secret_key,
                region: if region.is_empty() { None } else { Some(region) },
                service: if service.is_empty() { None } else { Some(service) },
                session_token,
                profile_name,
            }),
            Auth::OAuth2(flow) => {
                let (
                    flow_str,
                    access_token_url,
                    refresh_token_url,
                    authorization_url,
                    callback_url,
                    credentials,
                    resource_owner,
                    scope,
                    state,
                    pkce,
                    additional_parameters,
                    token_config,
                    settings,
                ) = domain_oauth2_to_oc_fields(flow);
                OcAuth::Typed(OcAuthTyped::OAuth2 {
                    flow: flow_str,
                    access_token_url,
                    refresh_token_url,
                    authorization_url,
                    callback_url,
                    credentials,
                    resource_owner,
                    scope,
                    state,
                    pkce,
                    additional_parameters,
                    token_config,
                    settings,
                })
            }
        }
    }
}

/// Extract all OC OAuth2 fields from a domain OAuth2Flow variant.
#[allow(clippy::type_complexity)]
fn domain_oauth2_to_oc_fields(
    flow: OAuth2Flow,
) -> (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<OcOAuth2Credentials>,
    Option<OcOAuth2ResourceOwner>,
    Option<String>,
    Option<String>,
    Option<OcOAuth2PKCE>,
    Option<serde_json::Value>,
    Option<serde_json::Value>,
    Option<serde_json::Value>,
) {
    match flow {
        OAuth2Flow::ClientCredentials {
            access_token_url,
            refresh_token_url,
            credentials,
            scope,
            additional_parameters,
            token_config,
            settings,
        } => (
            "client_credentials".into(),
            Some(access_token_url),
            refresh_token_url,
            None,
            None,
            Some(domain_creds_to_oc(credentials)),
            None,
            scope,
            None,
            None,
            additional_parameters.and_then(|v| serde_json::to_value(v).ok()),
            token_config.and_then(|v| serde_json::to_value(v).ok()),
            settings.and_then(|v| serde_json::to_value(v).ok()),
        ),
        OAuth2Flow::ResourceOwnerPassword {
            access_token_url,
            refresh_token_url,
            credentials,
            resource_owner,
            scope,
            additional_parameters,
            token_config,
            settings,
        } => (
            "resource_owner_password_credentials".into(),
            Some(access_token_url),
            refresh_token_url,
            None,
            None,
            Some(domain_creds_to_oc(credentials)),
            resource_owner.map(domain_ro_to_oc),
            scope,
            None,
            None,
            additional_parameters.and_then(|v| serde_json::to_value(v).ok()),
            token_config.and_then(|v| serde_json::to_value(v).ok()),
            settings.and_then(|v| serde_json::to_value(v).ok()),
        ),
        OAuth2Flow::AuthorizationCode {
            authorization_url,
            access_token_url,
            refresh_token_url,
            callback_url,
            credentials,
            scope,
            state,
            pkce,
            additional_parameters,
            token_config,
            settings,
        } => (
            "authorization_code".into(),
            Some(access_token_url),
            refresh_token_url,
            Some(authorization_url),
            callback_url,
            Some(domain_creds_to_oc(credentials)),
            None,
            scope,
            state,
            pkce.map(domain_pkce_to_oc),
            additional_parameters.and_then(|v| serde_json::to_value(v).ok()),
            token_config.and_then(|v| serde_json::to_value(v).ok()),
            settings.and_then(|v| serde_json::to_value(v).ok()),
        ),
        OAuth2Flow::Implicit {
            authorization_url,
            callback_url,
            client_id,
            scope,
            state,
            additional_parameters,
            token_config,
            settings,
        } => (
            "implicit".into(),
            None,
            None,
            Some(authorization_url),
            callback_url,
            Some(OcOAuth2Credentials {
                client_id,
                client_secret: String::new(),
                placement: None,
            }),
            None,
            scope,
            state,
            None,
            additional_parameters.and_then(|v| serde_json::to_value(v).ok()),
            token_config.and_then(|v| serde_json::to_value(v).ok()),
            settings.and_then(|v| serde_json::to_value(v).ok()),
        ),
    }
}

fn domain_creds_to_oc(c: OAuth2ClientCredentials) -> OcOAuth2Credentials {
    OcOAuth2Credentials {
        client_id: c.client_id,
        client_secret: c.client_secret,
        placement: c.placement,
    }
}

fn domain_ro_to_oc(r: OAuth2ResourceOwner) -> OcOAuth2ResourceOwner {
    OcOAuth2ResourceOwner {
        username: r.username,
        password: r.password,
    }
}

fn domain_pkce_to_oc(p: OAuth2PKCE) -> OcOAuth2PKCE {
    OcOAuth2PKCE {
        enabled: p.enabled,
        method: p.method,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::description::Description;

    #[test]
    fn header_oc_to_domain() {
        let oc = OcHttpRequestHeader {
            name: "Content-Type".into(),
            value: "application/json".into(),
            description: Some(Description::text("Content type")),
            disabled: Some(true),
        };
        let h: Header = oc.into();
        assert_eq!(h.key, "Content-Type");
        assert!(!h.enabled);
        assert!(h.description.is_some());
    }

    #[test]
    fn header_domain_to_oc() {
        let h = Header {
            key: "Accept".into(),
            value: "text/html".into(),
            enabled: true,
            description: None,
        };
        let oc: OcHttpRequestHeader = h.into();
        assert_eq!(oc.name, "Accept");
        assert_eq!(oc.disabled, None);  // Enabled → no disabled field.
    }

    #[test]
    fn header_roundtrip() {
        let original = Header {
            key: "X-Custom".into(),
            value: "val".into(),
            enabled: false,
            description: Some(Description::text("Custom header")),
        };
        let oc: OcHttpRequestHeader = original.clone().into();
        let back: Header = oc.into();
        assert_eq!(original, back);
    }

    #[test]
    fn param_split_by_type() {
        let params = vec![
            OcHttpRequestParam {
                name: "page".into(),
                value: "1".into(),
                description: None,
                param_type: Some("query".into()),
                disabled: None,
            },
            OcHttpRequestParam {
                name: "id".into(),
                value: "42".into(),
                description: None,
                param_type: Some("path".into()),
                disabled: None,
            },
            OcHttpRequestParam {
                name: "limit".into(),
                value: "10".into(),
                description: None,
                param_type: Some("query".into()),
                disabled: Some(true),
            },
        ];
        let (query, path) = split_params(params);
        assert_eq!(query.len(), 2);
        assert_eq!(path.len(), 1);
        assert_eq!(query[0].key, "page");
        assert!(query[0].enabled);
        assert!(!query[1].enabled);  // disabled: true → enabled: false.
        assert_eq!(path[0].name, "id");
    }

    #[test]
    fn param_merge_roundtrip() {
        let query = vec![QueryParam {
            key: "q".into(),
            value: "search".into(),
            enabled: true,
            description: None,
        }];
        let path = vec![PathParam {
            name: "id".into(),
            value: "1".into(),
            description: None,
        }];
        let merged = merge_params(&query, &path);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].param_type, Some("query".into()));
        assert_eq!(merged[1].param_type, Some("path".into()));
    }

    #[test]
    fn param_default_type_is_query() {
        let params = vec![OcHttpRequestParam {
            name: "x".into(),
            value: "1".into(),
            description: None,
            param_type: None,
            disabled: None,
        }];
        let (query, path) = split_params(params);
        assert_eq!(query.len(), 1);
        assert_eq!(path.len(), 0);
    }

    // ---- Body tests ----

    #[test]
    fn body_json_oc_to_domain() {
        let oc = OcHttpRequestBody::Json { data: r#"{"key":"val"}"#.into() };
        let body: Body = oc.into();
        assert_eq!(body.mode, BodyMode::Json);
        assert_eq!(body.content.unwrap(), r#"{"key":"val"}"#);
    }

    #[test]
    fn body_form_urlencoded_oc_to_domain() {
        let oc = OcHttpRequestBody::FormUrlEncoded { data: vec![
            OcFormField { name: "user".into(), value: "admin".into(), description: None, disabled: None },
            OcFormField { name: "pass".into(), value: "secret".into(), description: None, disabled: Some(true) },
        ]};
        let body: Body = oc.into();
        assert_eq!(body.mode, BodyMode::FormData);
        let fd = body.form_data.unwrap();
        assert_eq!(fd.len(), 2);
        assert_eq!(fd[0].key, "user");
        assert!(fd[0].enabled);
        assert!(!fd[1].enabled);
    }

    // ---- Auth tests ----

    #[test]
    fn auth_basic_oc_to_domain() {
        let oc = OcAuth::Typed(OcAuthTyped::Basic { username: "u".into(), password: "p".into() });
        let auth: Auth = oc.into();
        assert_eq!(auth, Auth::Basic { username: "u".into(), password: "p".into() });
    }

    #[test]
    fn auth_inherit_oc_to_domain() {
        let oc = OcAuth::Inherit("inherit".into());
        let auth: Auth = oc.into();
        assert_eq!(auth, Auth::Inherit);
    }

    #[test]
    fn auth_awsv4_oc_to_domain() {
        let oc = OcAuth::Typed(OcAuthTyped::AwsV4 {
            access_key_id: "AK".into(), secret_access_key: "SK".into(),
            region: Some("us-east-1".into()), service: Some("s3".into()),
            session_token: None, profile_name: None,
        });
        let auth: Auth = oc.into();
        match auth {
            Auth::AwsSigV4 { access_key, secret_key, region, service, .. } => {
                assert_eq!(access_key, "AK");
                assert_eq!(secret_key, "SK");
                assert_eq!(region, "us-east-1");
                assert_eq!(service, "s3");
            }
            _ => panic!("expected AwsSigV4"),
        }
    }

    #[test]
    fn auth_oauth2_client_credentials_oc_to_domain() {
        let oc = OcAuth::Typed(OcAuthTyped::OAuth2 {
            flow: "client_credentials".into(),
            access_token_url: Some("https://auth.example.com/token".into()),
            refresh_token_url: None,
            authorization_url: None,
            callback_url: None,
            credentials: Some(OcOAuth2Credentials { client_id: "id".into(), client_secret: "s".into(), placement: None }),
            resource_owner: None,
            scope: Some("read".into()),
            state: None,
            pkce: None,
            additional_parameters: None,
            token_config: None,
            settings: None,
        });
        let auth: Auth = oc.into();
        match auth {
            Auth::OAuth2(flow) => {
                assert!(matches!(flow, rocket_shared::oauth2::OAuth2Flow::ClientCredentials { .. }));
            }
            _ => panic!("expected OAuth2"),
        }
    }
}
