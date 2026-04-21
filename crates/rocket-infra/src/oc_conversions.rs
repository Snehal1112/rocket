//! Conversions between OpenCollection YAML structs (Oc*) and domain types.
//!
//! OcDescription is a re-export of Description (same type), so no conversion
//! is needed for descriptions — they flow through unchanged.

use crate::opencollection::*;
use rocket_collection::collection::Collection;
use rocket_collection::folder::{CollectionItem, Folder, OpaqueProtocolItem};
use rocket_collection::settings::{CollectionSettings, CollectionVariable};
use rocket_collection::Request;
use rocket_environment::environment::Environment;
use rocket_environment::variable::Variable;
use rocket_shared::action::{ActionSelector, ActionSetVariable, ActionVariable, HttpRequestExample};
use rocket_shared::description::Documentation;
use rocket_shared::oauth2::{
    OAuth2AdditionalParameters, OAuth2ClientCredentials, OAuth2Flow, OAuth2PKCE,
    OAuth2ResourceOwner, OAuth2Settings, OAuth2TokenConfig,
};
use rocket_shared::types::{Auth, Body, BodyMode, FormDataEntry, FormDataType, Header, HttpMethod, PathParam, QueryParam, RequestSettings, RequestSettingValue};
use rocket_shared::variable_value::VariableValue;
use rocket_workspace::{CollectionReference, CollectionRefType, WorkspaceConfig, WorkspaceEnvironmentsConfig};

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
            OcHttpRequestBody::Sparql { data } => Body {
                mode: BodyMode::Sparql,
                content: Some(data),
                form_data: None,
                file_path: None,
            },
            OcHttpRequestBody::FormUrlEncoded { data } => Body {
                mode: BodyMode::FormUrlEncoded,
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
        content_type: None,
        description: f.description,
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
        content_type: p.content_type,
        description: p.description,
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
            BodyMode::Sparql => OcHttpRequestBody::Sparql {
                data: b.content.unwrap_or_default(),
            },
            BodyMode::FormUrlEncoded => {
                let entries = b.form_data.unwrap_or_default();
                OcHttpRequestBody::FormUrlEncoded {
                    data: entries.into_iter().map(entry_to_form_field).collect(),
                }
            }
            BodyMode::FormData => {
                let entries = b.form_data.unwrap_or_default();
                OcHttpRequestBody::MultipartForm {
                    data: entries.into_iter().map(entry_to_multipart).collect(),
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
        description: e.description,
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
        description: e.description,
        content_type: e.content_type,
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
                let oauth_flow = match flow.as_str() {
                    "client_credentials" => OAuth2Flow::ClientCredentials {
                        access_token_url: access_token_url.unwrap_or_default(),
                        refresh_token_url,
                        credentials: creds,
                        scope,
                        additional_parameters,
                        token_config,
                        settings,
                    },
                    "resource_owner_password_credentials" => {
                        OAuth2Flow::ResourceOwnerPassword {
                            access_token_url: access_token_url.unwrap_or_default(),
                            refresh_token_url,
                            credentials: creds,
                            resource_owner: resource_owner.map(oc_ro_to_domain),
                            scope,
                            additional_parameters,
                            token_config,
                            settings,
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
                        additional_parameters,
                        token_config,
                        settings,
                    },
                    "implicit" | _ => OAuth2Flow::Implicit {
                        authorization_url: authorization_url.unwrap_or_default(),
                        callback_url,
                        client_id: creds.client_id,
                        scope,
                        state,
                        additional_parameters,
                        token_config,
                        settings,
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
    Option<OAuth2AdditionalParameters>,
    Option<OAuth2TokenConfig>,
    Option<OAuth2Settings>,
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
            additional_parameters,
            token_config,
            settings,
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
            additional_parameters,
            token_config,
            settings,
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
            additional_parameters,
            token_config,
            settings,
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
            additional_parameters,
            token_config,
            settings,
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

// ============================================================
// Settings conversions
// ============================================================

fn oc_settings_to_domain(oc: OcHttpRequestSettings) -> RequestSettings {
    RequestSettings {
        encode_url: oc.encode_url.map(inheritable_bool_to_domain),
        timeout: oc.timeout.map(inheritable_number_to_domain),
        follow_redirects: oc.follow_redirects.map(inheritable_bool_to_domain),
        max_redirects: oc.max_redirects.map(inheritable_number_to_domain),
        verify_ssl: oc.verify_ssl.map(inheritable_bool_to_domain),
    }
}

fn domain_settings_to_oc(s: RequestSettings) -> OcHttpRequestSettings {
    OcHttpRequestSettings {
        encode_url: s.encode_url.map(domain_bool_to_inheritable),
        timeout: s.timeout.map(domain_number_to_inheritable),
        follow_redirects: s.follow_redirects.map(domain_bool_to_inheritable),
        max_redirects: s.max_redirects.map(domain_number_to_inheritable),
        verify_ssl: s.verify_ssl.map(domain_bool_to_inheritable),
    }
}

fn inheritable_bool_to_domain(ib: InheritableBoolean) -> RequestSettingValue<bool> {
    match ib {
        InheritableBoolean::Value(v) => RequestSettingValue::Value(v),
        InheritableBoolean::Inherit(s) => RequestSettingValue::Inherit(s),
    }
}

fn inheritable_number_to_domain(n: InheritableNumber) -> RequestSettingValue<f64> {
    match n {
        InheritableNumber::Value(v) => RequestSettingValue::Value(v),
        InheritableNumber::Inherit(s) => RequestSettingValue::Inherit(s),
    }
}

fn domain_bool_to_inheritable(v: RequestSettingValue<bool>) -> InheritableBoolean {
    match v {
        RequestSettingValue::Value(b) => InheritableBoolean::Value(b),
        RequestSettingValue::Inherit(s) => InheritableBoolean::Inherit(s),
    }
}

fn domain_number_to_inheritable(v: RequestSettingValue<f64>) -> InheritableNumber {
    match v {
        RequestSettingValue::Value(n) => InheritableNumber::Value(n),
        RequestSettingValue::Inherit(s) => InheritableNumber::Inherit(s),
    }
}

// ============================================================
// Variable conversions
// ============================================================

/// Convert an OcVariable to a CollectionVariable.
/// The persisted YAML value is loaded into both fields so the UI shows
/// the saved value in both Initial and Current columns on reload.
pub fn oc_variable_to_collection_variable(v: OcVariable) -> CollectionVariable {
    let val = v.value.as_ref().map(|vv| vv.data().to_string()).unwrap_or_default();
    CollectionVariable {
        key:           v.name,
        value:         val.clone(),
        initial_value: val,
        enabled:       !v.disabled.unwrap_or(false),
        secret:        false,
    }
}

/// Convert a CollectionVariable to an OcVariable.
/// Persists the current value (value) when set, falling back to initial_value.
/// This ensures whatever the user typed and saved in the UI is round-tripped correctly.
pub fn collection_variable_to_oc_variable(cv: CollectionVariable) -> OcVariable {
    let persisted = if !cv.value.is_empty() { cv.value } else { cv.initial_value };
    OcVariable {
        name:        cv.key,
        value:       if persisted.is_empty() { None } else { Some(VariableValue::simple(persisted)) },
        description: None,
        disabled:    if cv.enabled { None } else { Some(true) },
    }
}

impl From<OcVariable> for Variable {
    fn from(oc: OcVariable) -> Self {
        Variable {
            key: oc.name,
            value: oc.value.as_ref().map(|v| v.data().to_string()).unwrap_or_default(),
            enabled: !oc.disabled.unwrap_or(false),
            secret: false,
            description: oc.description,
            value_variants: None,
            secret_type: None,
        }
    }
}

impl From<Variable> for OcVariable {
    fn from(v: Variable) -> Self {
        OcVariable {
            name: v.key,
            value: Some(VariableValue::simple(v.value)),
            description: v.description,
            // Omit disabled entirely when enabled (cleaner YAML output).
            disabled: if v.enabled { None } else { Some(true) },
        }
    }
}

impl From<OcSecretVariable> for Variable {
    fn from(oc: OcSecretVariable) -> Self {
        Variable {
            key: oc.name,
            // Secrets don't store values in YAML.
            value: String::new(),
            enabled: !oc.disabled.unwrap_or(false),
            secret: true,
            description: oc.description,
            value_variants: None,
            secret_type: oc.secret_type,
        }
    }
}

// ============================================================
// Environment conversions
// ============================================================

impl From<OcEnvironment> for Environment {
    fn from(oc: OcEnvironment) -> Self {
        Environment {
            name: oc.name,
            variables: oc.variables.into_iter().map(Variable::from).collect(),
            color: oc.color,
            description: oc.description,
            extends: oc.extends,
            dot_env_file_path: oc.dot_env_file_path,
            // Domain uses Vec<serde_json::Value> as a placeholder for client certs.
            client_certificates: oc.client_certificates.into_iter()
                .map(|c| serde_json::to_value(c).unwrap_or_default())
                .collect(),
        }
    }
}

impl From<Environment> for OcEnvironment {
    fn from(env: Environment) -> Self {
        OcEnvironment {
            name: env.name,
            color: env.color,
            description: env.description,
            variables: env.variables.into_iter().map(OcVariable::from).collect(),
            client_certificates: env.client_certificates.into_iter()
                .filter_map(|v| serde_json::from_value(v).ok())
                .collect(),
            extends: env.extends,
            dot_env_file_path: env.dot_env_file_path,
        }
    }
}

// ============================================================
// WorkspaceConfig ↔ OcWorkspaceConfig conversions
// ============================================================

impl From<OcWorkspaceCollectionRef> for CollectionReference {
    fn from(r: OcWorkspaceCollectionRef) -> Self {
        match r.path {
            Some(p) if p.is_absolute() => CollectionReference {
                name: r.name,
                ref_type: CollectionRefType::External,
                path: Some(p),
            },
            // Any non-absolute path (relative or absent) is treated as Embedded.
            // External collections in the spec use absolute paths, matching Bruno's convention.
            _ => CollectionReference {
                name: r.name,
                ref_type: CollectionRefType::Embedded,
                path: None,
            },
        }
    }
}

impl From<CollectionReference> for OcWorkspaceCollectionRef {
    fn from(r: CollectionReference) -> Self {
        OcWorkspaceCollectionRef {
            path: match r.ref_type {
                CollectionRefType::Embedded => {
                    Some(std::path::PathBuf::from(format!("collections/{}", r.name)))
                }
                CollectionRefType::External => r.path,
            },
            name: r.name,
        }
    }
}

impl From<OcWorkspaceConfig> for WorkspaceConfig {
    fn from(oc: OcWorkspaceConfig) -> Self {
        WorkspaceConfig {
            name: oc.info.name,
            description: oc.docs,
            collections: oc.collections.into_iter().map(CollectionReference::from).collect(),
            environments: WorkspaceEnvironmentsConfig {
                active_environment: oc.environments.and_then(|e| e.active_environment),
            },
            global_environment: oc.global_environment,
        }
    }
}

impl From<WorkspaceConfig> for OcWorkspaceConfig {
    fn from(w: WorkspaceConfig) -> Self {
        let has_active_env = w.environments.active_environment.is_some();
        OcWorkspaceConfig {
            opencollection: Some("1.0.0".into()),
            info: OcWorkspaceInfo {
                name: w.name,
                workspace_type: Some("workspace".into()),
            },
            collections: w.collections.into_iter().map(OcWorkspaceCollectionRef::from).collect(),
            docs: w.description,
            environments: if has_active_env {
                Some(OcWorkspaceEnvironments {
                    active_environment: w.environments.active_environment,
                })
            } else {
                None
            },
            global_environment: w.global_environment,
        }
    }
}

// ============================================================
// OcHttpRequest ↔ Request conversions
// ============================================================

/// Convert an OC HTTP request to a domain Request.
pub fn oc_http_request_to_request(oc: OcHttpRequest) -> Request {
    // Info section.
    let name = oc.info.name;
    let description = oc.info.description;
    let seq = oc.info.seq;
    let tags = oc.info.tags;

    // HTTP section.
    let method = oc.http.method.parse::<HttpMethod>().unwrap_or(HttpMethod::Get);
    let url = oc.http.url;
    let headers: Vec<Header> = oc.http.headers.into_iter().map(Header::from).collect();
    let (query_params, path_params) = split_params(oc.http.params);
    let body: Option<Body> = oc.http.body.map(Body::from);
    let auth: Auth = oc.http.auth.map(Auth::from).unwrap_or(Auth::None);

    // Runtime section.
    let (pre_request_script, post_response_script, tests) = extract_scripts(&oc.runtime);
    let assertions = oc.runtime.as_ref()
        .map(|r| r.assertions.clone())
        .unwrap_or_default();
    let actions = extract_actions(&oc.runtime);
    let variables = oc.runtime.as_ref()
        .map(|r| r.variables.iter().map(|v| serde_json::to_value(v).unwrap_or_default()).collect())
        .unwrap_or_default();
    let runtime_auth = oc.runtime.as_ref()
        .and_then(|r| r.auth.clone())
        .map(Auth::from);

    // Examples.
    let examples = oc.examples.unwrap_or_default().into_iter()
        .map(|e| HttpRequestExample {
            name: e.name,
            description: e.description,
            request: e.request.and_then(|r| serde_json::to_value(r).ok()),
            response: e.response.and_then(|r| serde_json::to_value(r).ok()),
        })
        .collect();

    // Settings.
    let settings = oc.settings.map(oc_settings_to_domain);

    // Docs.
    let docs: Option<Documentation> = oc.docs.map(Documentation::text);

    Request {
        uid: oc.uid.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        name,
        method,
        url,
        headers,
        query_params,
        path_params,
        body,
        auth,
        file_name: None,
        seq,
        tags,
        description,
        pre_request_script,
        post_response_script,
        tests,
        assertions,
        actions,
        examples,
        docs,
        variables,
        runtime_auth,
        settings,
    }
}

/// Convert a domain Request back to an OC HTTP request.
pub fn request_to_oc_http_request(req: Request) -> OcHttpRequest {
    // Extract params, runtime_auth, and settings before req fields are consumed.
    let params = merge_params(&req.query_params, &req.path_params);
    let runtime_auth = req.runtime_auth.map(OcAuth::from);
    let settings = req.settings.map(domain_settings_to_oc);

    let info = OcHttpRequestInfo {
        name: req.name,
        description: req.description,
        request_type: Some("http".into()),
        seq: req.seq,
        tags: req.tags,
    };

    let http = OcHttpRequestDetails {
        method: req.method.to_string(),
        url: req.url,
        headers: req.headers.into_iter().map(OcHttpRequestHeader::from).collect(),
        params,
        body: req.body.map(OcHttpRequestBody::from),
        auth: if req.auth == Auth::None { None } else { Some(OcAuth::from(req.auth)) },
    };

    let mut scripts = Vec::new();
    if let Some(code) = req.pre_request_script {
        scripts.push(OcScript { script_type: "before-request".into(), code });
    }
    if let Some(code) = req.post_response_script {
        scripts.push(OcScript { script_type: "after-response".into(), code });
    }
    if let Some(code) = req.tests {
        scripts.push(OcScript { script_type: "tests".into(), code });
    }

    let actions: Vec<OcAction> = req.actions.into_iter().map(|a| {
        OcAction::SetVariable {
            description: a.description,
            phase: a.phase,
            selector: OcActionSelector { expression: a.selector.expression, method: a.selector.method },
            variable: OcActionVariable { name: a.variable.name, scope: a.variable.scope },
            disabled: a.disabled,
        }
    }).collect();

    let has_runtime = !scripts.is_empty()
        || !req.assertions.is_empty()
        || !actions.is_empty()
        || !req.variables.is_empty()
        || runtime_auth.is_some();
    let runtime = if has_runtime {
        Some(OcHttpRequestRuntime {
            variables: req.variables.into_iter()
                .filter_map(|v| serde_json::from_value::<OcVariable>(v).ok())
                .collect(),
            scripts,
            assertions: req.assertions,
            actions,
            auth: runtime_auth,
        })
    } else {
        None
    };

    let examples = if req.examples.is_empty() {
        None
    } else {
        Some(req.examples.into_iter().map(|e| {
            OcHttpRequestExample {
                name: e.name,
                description: e.description,
                request: e.request.and_then(|v| serde_json::from_value(v).ok()),
                response: e.response.and_then(|v| serde_json::from_value(v).ok()),
            }
        }).collect())
    };

    let docs = req.docs.and_then(|d| d.content().map(String::from));

    OcHttpRequest {
        uid: Some(req.uid),
        info,
        http,
        runtime,
        settings,
        examples,
        docs,
    }
}

// ============================================================
// ProtocolRequest — lossless multi-protocol wrapper
// ============================================================

/// A request that can be any protocol type.
/// HTTP requests are fully converted to domain types.
/// Other protocols preserve their YAML data losslessly.
#[derive(Debug, Clone, PartialEq)]
pub enum ProtocolRequest {
    Http(rocket_collection::Request),
    GraphQL(serde_yaml::Value),
    Grpc(serde_yaml::Value),
    WebSocket(serde_yaml::Value),
}

/// Convert an OcItem to a ProtocolRequest.
/// HTTP items are fully converted; others are stored as opaque YAML.
pub fn oc_item_to_protocol_request(item: OcItem) -> Option<ProtocolRequest> {
    match item {
        OcItem::Http(oc) => Some(ProtocolRequest::Http(oc_http_request_to_request(oc))),
        OcItem::GraphQL(oc) => {
            serde_yaml::to_value(oc).ok().map(ProtocolRequest::GraphQL)
        }
        OcItem::Grpc(oc) => {
            serde_yaml::to_value(oc).ok().map(ProtocolRequest::Grpc)
        }
        OcItem::WebSocket(oc) => {
            serde_yaml::to_value(oc).ok().map(ProtocolRequest::WebSocket)
        }
        // Folders and script files are not requests.
        OcItem::Folder(_) | OcItem::ScriptFile(_) => None,
    }
}

/// Convert a ProtocolRequest back to an OcItem for writing.
pub fn protocol_request_to_oc_item(pr: ProtocolRequest) -> Option<OcItem> {
    match pr {
        ProtocolRequest::Http(req) => Some(OcItem::Http(request_to_oc_http_request(req))),
        ProtocolRequest::GraphQL(val) => {
            serde_yaml::from_value::<OcGraphQLRequest>(val).ok().map(OcItem::GraphQL)
        }
        ProtocolRequest::Grpc(val) => {
            serde_yaml::from_value::<OcGrpcRequest>(val).ok().map(OcItem::Grpc)
        }
        ProtocolRequest::WebSocket(val) => {
            serde_yaml::from_value::<OcWebSocketRequest>(val).ok().map(OcItem::WebSocket)
        }
    }
}

// ============================================================
// Folder conversions
// ============================================================

/// Convert an OC folder to a domain Folder, recursively converting items.
pub fn oc_folder_to_folder(oc: OcFolder) -> Folder {
    let name = oc.info.name;
    let items = oc
        .items
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| match &item {
            OcItem::Http(_) => {
                if let OcItem::Http(req) = item {
                    Some(CollectionItem::Request(oc_http_request_to_request(req)))
                } else {
                    None
                }
            }
            OcItem::Folder(_) => {
                if let OcItem::Folder(f) = item {
                    Some(CollectionItem::Folder(oc_folder_to_folder(f)))
                } else {
                    None
                }
            }
            OcItem::GraphQL(ref gql) => {
                let name = gql.info.name.clone();
                serde_yaml::to_value(&item).ok().map(|raw| {
                    CollectionItem::OpaqueItem(OpaqueProtocolItem {
                        protocol: "graphql".into(),
                        name,
                        raw,
                    })
                })
            }
            OcItem::Grpc(ref grpc) => {
                let name = grpc.info.name.clone();
                serde_yaml::to_value(&item).ok().map(|raw| {
                    CollectionItem::OpaqueItem(OpaqueProtocolItem {
                        protocol: "grpc".into(),
                        name,
                        raw,
                    })
                })
            }
            OcItem::WebSocket(ref ws) => {
                let name = ws.info.name.clone();
                serde_yaml::to_value(&item).ok().map(|raw| {
                    CollectionItem::OpaqueItem(OpaqueProtocolItem {
                        protocol: "websocket".into(),
                        name,
                        raw,
                    })
                })
            }
            OcItem::ScriptFile(_) => None,
        })
        .collect();

    Folder {
        uid: uuid::Uuid::new_v4().to_string(),
        name,
        dir_name: None,
        items,
    }
}

/// Convert a domain Folder back to an OC folder.
pub fn folder_to_oc_folder(folder: Folder) -> OcFolder {
    let items: Vec<OcItem> = folder
        .items
        .into_iter()
        .map(|item| match item {
            CollectionItem::Request(req) => OcItem::Http(request_to_oc_http_request(req)),
            CollectionItem::Folder(f) => OcItem::Folder(folder_to_oc_folder(f)),
            CollectionItem::OpaqueItem(opaque) => {
                serde_yaml::from_value::<OcItem>(opaque.raw.clone()).unwrap_or_else(|_| {
                    OcItem::Folder(OcFolder {
                        info: OcFolderInfo {
                            name: opaque.name,
                            uid: None,
                            description: None,
                            folder_type: Some("folder".into()),
                            seq: None,
                            tags: Vec::new(),
                            request: None,
                        },
                        items: None,
                        request: None,
                        docs: None,
                    })
                })
            }
        })
        .collect();

    OcFolder {
        info: OcFolderInfo {
            name: folder.name,
            uid: None,
            description: None,
            folder_type: Some("folder".into()),
            seq: None,
            tags: Vec::new(),
            request: None,
        },
        items: if items.is_empty() { None } else { Some(items) },
        request: None,
        docs: None,
    }
}

// ============================================================
// Collection conversions
// ============================================================

/// Convert an OC collection to a domain Collection.
pub fn oc_collection_to_collection(oc: OcCollection) -> Collection {
    let name = oc
        .info
        .as_ref()
        .map(|i| i.name.clone())
        .unwrap_or_else(|| "Untitled".into());

    // Convert items into the root folder.
    let items = oc
        .items
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| match &item {
            OcItem::Http(_) => {
                if let OcItem::Http(req) = item {
                    Some(CollectionItem::Request(oc_http_request_to_request(req)))
                } else {
                    None
                }
            }
            OcItem::Folder(_) => {
                if let OcItem::Folder(f) = item {
                    Some(CollectionItem::Folder(oc_folder_to_folder(f)))
                } else {
                    None
                }
            }
            OcItem::GraphQL(ref gql) => {
                let name = gql.info.name.clone();
                serde_yaml::to_value(&item).ok().map(|raw| {
                    CollectionItem::OpaqueItem(OpaqueProtocolItem {
                        protocol: "graphql".into(),
                        name,
                        raw,
                    })
                })
            }
            OcItem::Grpc(ref grpc) => {
                let name = grpc.info.name.clone();
                serde_yaml::to_value(&item).ok().map(|raw| {
                    CollectionItem::OpaqueItem(OpaqueProtocolItem {
                        protocol: "grpc".into(),
                        name,
                        raw,
                    })
                })
            }
            OcItem::WebSocket(ref ws) => {
                let name = ws.info.name.clone();
                serde_yaml::to_value(&item).ok().map(|raw| {
                    CollectionItem::OpaqueItem(OpaqueProtocolItem {
                        protocol: "websocket".into(),
                        name,
                        raw,
                    })
                })
            }
            OcItem::ScriptFile(_) => None,
        })
        .collect();

    let root = Folder {
        uid: uuid::Uuid::new_v4().to_string(),
        name: name.clone(),
        dir_name: None,
        items,
    };

    // Convert request defaults to collection settings.
    let settings = if let Some(defaults) = oc.request {
        CollectionSettings {
            description: oc.docs,
            readme: oc.readme.clone(),
            auth: defaults.auth.map(Auth::from),
            headers: defaults
                .headers
                .unwrap_or_default()
                .into_iter()
                .map(Header::from)
                .collect(),
            variables: defaults
                .variables
                .unwrap_or_default()
                .into_iter()
                .map(oc_variable_to_collection_variable)
                .collect(),
        }
    } else {
        CollectionSettings {
            description: oc.docs,
            readme: oc.readme,
            ..CollectionSettings::default()
        }
    };

    Collection {
        name,
        root,
        settings,
    }
}

/// Convert a domain Collection back to an OC collection.
pub fn collection_to_oc_collection(col: Collection) -> OcCollection {
    let items: Vec<OcItem> = col
        .root
        .items
        .into_iter()
        .map(|item| match item {
            CollectionItem::Request(req) => OcItem::Http(request_to_oc_http_request(req)),
            CollectionItem::Folder(f) => OcItem::Folder(folder_to_oc_folder(f)),
            CollectionItem::OpaqueItem(opaque) => {
                serde_yaml::from_value::<OcItem>(opaque.raw.clone()).unwrap_or_else(|_| {
                    OcItem::Folder(OcFolder {
                        info: OcFolderInfo {
                            name: opaque.name,
                            uid: None,
                            description: None,
                            folder_type: Some("folder".into()),
                            seq: None,
                            tags: Vec::new(),
                            request: None,
                        },
                        items: None,
                        request: None,
                        docs: None,
                    })
                })
            }
        })
        .collect();

    let request = {
        let has_defaults = !col.settings.headers.is_empty()
            || col.settings.auth.is_some()
            || !col.settings.variables.is_empty();
        if has_defaults {
            Some(OcRequestDefaults {
                headers: if col.settings.headers.is_empty() {
                    None
                } else {
                    Some(
                        col.settings
                            .headers
                            .into_iter()
                            .map(OcHttpRequestHeader::from)
                            .collect(),
                    )
                },
                metadata: None,
                auth: col.settings.auth.map(OcAuth::from),
                variables: if col.settings.variables.is_empty() {
                    None
                } else {
                    Some(
                        col.settings
                            .variables
                            .into_iter()
                            .map(collection_variable_to_oc_variable)
                            .collect(),
                    )
                },
                scripts: None,
                settings: None,
            })
        } else {
            None
        }
    };

    OcCollection {
        opencollection: Some("1.0.0".into()),
        uid: None,
        info: Some(OcInfo {
            name: col.name,
            summary: None,
            version: None,
            authors: None,
        }),
        config: None,
        items: if items.is_empty() { None } else { Some(items) },
        request,
        docs: col.settings.description,
        readme: col.settings.readme,
        bundled: None,
        extensions: None,
    }
}

/// Extract pre-request, post-response, and test scripts from runtime.
fn extract_scripts(runtime: &Option<OcHttpRequestRuntime>) -> (Option<String>, Option<String>, Option<String>) {
    let Some(rt) = runtime else { return (None, None, None) };
    let mut pre = None;
    let mut post = None;
    let mut tests = None;
    for script in &rt.scripts {
        match script.script_type.as_str() {
            "before-request" => pre = Some(script.code.clone()),
            "after-response" => post = Some(script.code.clone()),
            "tests" => tests = Some(script.code.clone()),
            _ => {}
        }
    }
    (pre, post, tests)
}

/// Extract action-set-variable entries from runtime.
fn extract_actions(runtime: &Option<OcHttpRequestRuntime>) -> Vec<ActionSetVariable> {
    let Some(rt) = runtime else { return Vec::new() };
    rt.actions.iter().map(|a| {
        match a {
            OcAction::SetVariable { description, phase, selector, variable, disabled } => {
                ActionSetVariable {
                    phase: phase.clone(),
                    selector: ActionSelector {
                        expression: selector.expression.clone(),
                        method: selector.method.clone(),
                    },
                    variable: ActionVariable {
                        name: variable.name.clone(),
                        scope: variable.scope.clone(),
                    },
                    disabled: *disabled,
                    description: description.clone(),
                }
            }
        }
    }).collect()
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
    fn body_sparql_roundtrip() {
        let oc = OcHttpRequestBody::Sparql { data: "SELECT ?s WHERE { ?s ?p ?o }".into() };
        let body: Body = oc.into();
        assert_eq!(body.mode, BodyMode::Sparql);
        assert_eq!(body.content.as_deref(), Some("SELECT ?s WHERE { ?s ?p ?o }"));
        let back: OcHttpRequestBody = body.into();
        assert!(matches!(back, OcHttpRequestBody::Sparql { ref data } if data == "SELECT ?s WHERE { ?s ?p ?o }"));
    }

    #[test]
    fn body_form_urlencoded_oc_to_domain() {
        let oc = OcHttpRequestBody::FormUrlEncoded { data: vec![
            OcFormField { name: "user".into(), value: "admin".into(), description: None, disabled: None },
            OcFormField { name: "pass".into(), value: "secret".into(), description: None, disabled: Some(true) },
        ]};
        let body: Body = oc.into();
        assert_eq!(body.mode, BodyMode::FormUrlEncoded);
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

    // ---- CollectionVariable tests ----

    #[test]
    fn collection_variable_save_persists_current_value_over_initial() {
        // Scenario: user has both a current value and an initial value set.
        // On save, the current value (value) takes precedence and is persisted to YAML.
        let cv = CollectionVariable {
            key: "HOST".into(),
            value: "http://production.com".into(),    // current value — should win
            initial_value: "http://localhost".into(), // initial value — lower priority
            enabled: true,
            secret: false,
        };
        let oc = collection_variable_to_oc_variable(cv);
        assert_eq!(oc.value.as_ref().map(|v| v.data()), Some("http://production.com"),
            "YAML should store current value (value) when set");
    }

    #[test]
    fn collection_variable_save_falls_back_to_initial_when_value_empty() {
        // Scenario: user only filled in the initial value, leaving current value blank.
        // The initial value should be persisted.
        let cv = CollectionVariable {
            key: "HOST".into(),
            value: "".into(),
            initial_value: "http://localhost".into(),
            enabled: true,
            secret: false,
        };
        let oc = collection_variable_to_oc_variable(cv);
        assert_eq!(oc.value.as_ref().map(|v| v.data()), Some("http://localhost"),
            "YAML should store initial_value when current value is empty");
    }

    #[test]
    fn collection_variable_roundtrip_restores_value_in_both_fields() {
        // After a save/load roundtrip the persisted value should appear in both
        // initial_value and value so the UI shows it in both columns on reload.
        let cv = CollectionVariable {
            key: "BASE_URL".into(),
            value: "http://production.com".into(),
            initial_value: "http://localhost:8080".into(),
            enabled: true,
            secret: false,
        };
        let oc = collection_variable_to_oc_variable(cv);
        let back = oc_variable_to_collection_variable(oc);
        // current value wins on save, so both fields reflect it after reload.
        assert_eq!(back.initial_value, "http://production.com");
        assert_eq!(back.value, "http://production.com");
    }

    // ---- Variable tests ----

    #[test]
    fn variable_oc_to_domain() {
        let oc = OcVariable {
            name: "HOST".into(),
            value: Some(VariableValue::simple("localhost")),
            description: Some(Description::text("Server host")),
            disabled: Some(true),
        };
        let v: Variable = oc.into();
        assert_eq!(v.key, "HOST");
        assert_eq!(v.value, "localhost");
        assert!(!v.enabled);
        assert!(!v.secret);
        assert!(v.description.is_some());
    }

    #[test]
    fn variable_domain_to_oc() {
        let v = Variable::new("BASE_URL", "https://api.example.com");
        let oc: OcVariable = v.into();
        assert_eq!(oc.name, "BASE_URL");
        assert!(oc.value.is_some());
        assert_eq!(oc.disabled, None);
    }

    #[test]
    fn secret_variable_oc_to_domain() {
        let oc = OcSecretVariable {
            secret: true,
            name: "API_KEY".into(),
            description: None,
            disabled: None,
            secret_type: Some("string".into()),
        };
        let v: Variable = oc.into();
        assert_eq!(v.key, "API_KEY");
        assert!(v.secret);
        assert!(v.enabled);
        assert_eq!(v.secret_type, Some("string".into()));
    }

    #[test]
    fn environment_oc_to_domain() {
        let oc = OcEnvironment {
            name: "production".into(),
            color: Some("#FF0000".into()),
            description: Some(Description::text("Prod env")),
            variables: vec![
                OcVariable { name: "HOST".into(), value: Some(VariableValue::simple("api.prod.com")), description: None, disabled: None },
            ],
            client_certificates: Vec::new(),
            extends: Some("base".into()),
            dot_env_file_path: Some(".env.prod".into()),
        };
        let env: Environment = oc.into();
        assert_eq!(env.name, "production");
        assert_eq!(env.color, Some("#FF0000".into()));
        assert_eq!(env.variables.len(), 1);
        assert_eq!(env.variables[0].key, "HOST");
        assert_eq!(env.extends, Some("base".into()));
    }

    #[test]
    fn environment_roundtrip() {
        let original = Environment {
            name: "staging".into(),
            variables: vec![Variable::new("URL", "https://staging.example.com")],
            color: Some("#00FF00".into()),
            description: None,
            extends: None,
            dot_env_file_path: None,
            client_certificates: Vec::new(),
        };
        let oc: OcEnvironment = original.clone().into();
        let back: Environment = oc.into();
        assert_eq!(original.name, back.name);
        assert_eq!(original.color, back.color);
        assert_eq!(original.variables.len(), back.variables.len());
        assert_eq!(original.variables[0].key, back.variables[0].key);
    }

    // ---- OcHttpRequest ↔ Request tests ----

    #[test]
    fn oc_http_request_to_domain_basic() {
        let yaml = r#"
info:
  name: Get Users
  type: http
  seq: 1
  tags:
    - api
http:
  method: GET
  url: "https://api.example.com/users"
  headers:
    - name: Accept
      value: application/json
"#;
        let oc: OcHttpRequest = serde_yaml::from_str(yaml).unwrap();
        let req = oc_http_request_to_request(oc);
        assert_eq!(req.name, "Get Users");
        assert_eq!(req.method, HttpMethod::Get);
        assert_eq!(req.url, "https://api.example.com/users");
        assert_eq!(req.headers.len(), 1);
        assert_eq!(req.headers[0].key, "Accept");
        assert_eq!(req.seq, Some(1));
        assert_eq!(req.tags, vec!["api"]);
    }

    #[test]
    fn oc_http_request_with_runtime() {
        let yaml = r#"
info:
  name: Test
  type: http
http:
  method: POST
  url: "https://api.example.com"
runtime:
  scripts:
    - type: before-request
      code: "let x = 1;"
    - type: after-response
      code: "console.log(res.status);"
    - type: tests
      code: "expect(res.status).to.equal(200);"
  assertions:
    - expression: res.status
      operator: eq
      value: "200"
  actions:
    - type: set-variable
      phase: after-response
      selector:
        expression: res.body.token
        method: jsonq
      variable:
        name: authToken
        scope: collection
"#;
        let oc: OcHttpRequest = serde_yaml::from_str(yaml).unwrap();
        let req = oc_http_request_to_request(oc);
        assert_eq!(req.pre_request_script, Some("let x = 1;".into()));
        assert_eq!(req.post_response_script, Some("console.log(res.status);".into()));
        assert_eq!(req.tests, Some("expect(res.status).to.equal(200);".into()));
        assert_eq!(req.assertions.len(), 1);
        assert_eq!(req.actions.len(), 1);
        assert_eq!(req.actions[0].variable.scope, "collection");
    }

    // ---- ProtocolRequest lossless roundtrip tests ----

    #[test]
    fn graphql_lossless_roundtrip() {
        let yaml = r#"
info:
  name: Get Users
  type: graphql
graphql:
  url: "https://api.example.com/graphql"
  body:
    query: "query { users { id name } }"
"#;
        let oc_item: OcItem = serde_yaml::from_str(yaml).unwrap();
        let pr = oc_item_to_protocol_request(oc_item).unwrap();
        assert!(matches!(pr, ProtocolRequest::GraphQL(_)));
        let back = protocol_request_to_oc_item(pr).unwrap();
        match back {
            OcItem::GraphQL(gql) => {
                assert_eq!(gql.info.name, "Get Users");
                assert_eq!(gql.graphql.url, "https://api.example.com/graphql");
            }
            _ => panic!("expected GraphQL"),
        }
    }

    #[test]
    fn grpc_lossless_roundtrip() {
        let yaml = r#"
info:
  name: Get User
  type: grpc
grpc:
  url: "localhost:50051"
  method: "users.UserService/GetUser"
  methodType: unary
"#;
        let oc_item: OcItem = serde_yaml::from_str(yaml).unwrap();
        let pr = oc_item_to_protocol_request(oc_item).unwrap();
        assert!(matches!(pr, ProtocolRequest::Grpc(_)));
        let back = protocol_request_to_oc_item(pr).unwrap();
        assert!(matches!(back, OcItem::Grpc(_)));
    }

    #[test]
    fn websocket_lossless_roundtrip() {
        let yaml = r#"
info:
  name: Chat
  type: websocket
websocket:
  url: "wss://chat.example.com/ws"
  message:
    type: json
    data: '{"action": "subscribe"}'
"#;
        let oc_item: OcItem = serde_yaml::from_str(yaml).unwrap();
        let pr = oc_item_to_protocol_request(oc_item).unwrap();
        assert!(matches!(pr, ProtocolRequest::WebSocket(_)));
        let back = protocol_request_to_oc_item(pr).unwrap();
        assert!(matches!(back, OcItem::WebSocket(_)));
    }

    #[test]
    fn http_item_converts_to_domain() {
        let yaml = r#"
info:
  name: Simple GET
  type: http
http:
  method: GET
  url: https://example.com
"#;
        let oc_item: OcItem = serde_yaml::from_str(yaml).unwrap();
        let pr = oc_item_to_protocol_request(oc_item).unwrap();
        match pr {
            ProtocolRequest::Http(req) => {
                assert_eq!(req.name, "Simple GET");
                assert_eq!(req.method, HttpMethod::Get);
            }
            _ => panic!("expected Http"),
        }
    }

    #[test]
    fn domain_request_to_oc_roundtrip() {
        let yaml = r#"
info:
  name: Create User
  type: http
  seq: 5
http:
  method: POST
  url: "https://api.example.com/users"
  headers:
    - name: Content-Type
      value: application/json
  body:
    type: json
    data: '{"name": "John"}'
  auth:
    type: bearer
    token: my-token
runtime:
  scripts:
    - type: before-request
      code: "let x = 1;"
  assertions:
    - expression: res.status
      operator: eq
      value: "201"
docs: "Creates a user."
"#;
        let oc: OcHttpRequest = serde_yaml::from_str(yaml).unwrap();
        let req = oc_http_request_to_request(oc);
        let back = request_to_oc_http_request(req);
        assert_eq!(back.info.name, "Create User");
        assert_eq!(back.info.seq, Some(5));
        assert_eq!(back.http.method, "POST");
        assert!(back.http.body.is_some());
        assert!(back.http.auth.is_some());
        assert!(back.runtime.is_some());
        let rt = back.runtime.unwrap();
        assert_eq!(rt.scripts.len(), 1);
        assert_eq!(rt.assertions.len(), 1);
        assert_eq!(back.docs, Some("Creates a user.".into()));
    }

    // ---- Folder + Collection tests ----

    #[test]
    fn oc_folder_to_domain() {
        let yaml = r#"
info:
  name: Users
  type: folder
items:
  - info:
      name: Get Users
      type: http
    http:
      method: GET
      url: "https://api.example.com/users"
  - info:
      name: Create User
      type: http
    http:
      method: POST
      url: "https://api.example.com/users"
"#;
        let oc: OcFolder = serde_yaml::from_str(yaml).unwrap();
        let folder = oc_folder_to_folder(oc);
        assert_eq!(folder.name, "Users");
        assert_eq!(folder.items.len(), 2);
        assert!(matches!(&folder.items[0], CollectionItem::Request(r) if r.name == "Get Users"));
        assert!(matches!(&folder.items[1], CollectionItem::Request(r) if r.name == "Create User"));
    }

    #[test]
    fn oc_collection_to_domain() {
        let yaml = r#"
opencollection: "0.1"
info:
  name: My API
request:
  headers:
    - name: Accept
      value: application/json
  auth:
    type: bearer
    token: "{{token}}"
items:
  - info:
      name: Health Check
      type: http
    http:
      method: GET
      url: /health
"#;
        let oc: OcCollection = serde_yaml::from_str(yaml).unwrap();
        let col = oc_collection_to_collection(oc);
        assert_eq!(col.name, "My API");
        assert_eq!(col.root.items.len(), 1);
        assert_eq!(col.settings.headers.len(), 1);
        assert_eq!(col.settings.headers[0].key, "Accept");
        assert!(col.settings.auth.is_some());
    }

    #[test]
    fn collection_roundtrip() {
        use rocket_shared::types::HttpMethod;

        let mut col = Collection::new("Test API");
        col.root
            .add_request(Request::new("Get Users", HttpMethod::Get, "/users"));
        col.settings
            .headers
            .push(Header::new("Accept", "application/json"));

        let oc = collection_to_oc_collection(col);
        assert_eq!(oc.info.as_ref().unwrap().name, "Test API");
        assert!(oc.items.is_some());
        assert_eq!(oc.items.as_ref().unwrap().len(), 1);
        assert!(oc.request.is_some());

        let back = oc_collection_to_collection(oc);
        assert_eq!(back.name, "Test API");
        assert_eq!(back.root.items.len(), 1);
        assert_eq!(back.settings.headers.len(), 1);
    }

    #[test]
    fn params_survive_roundtrip() {
        let yaml = r#"
info:
  name: Parameterised
  type: http
http:
  method: GET
  url: "https://api.example.com/users/:id"
  params:
    - name: page
      value: "1"
      type: query
      description: Page number
    - name: id
      value: "42"
      type: path
    - name: limit
      value: "10"
      type: query
      disabled: true
"#;
        let oc: OcHttpRequest = serde_yaml::from_str(yaml).unwrap();
        let req = oc_http_request_to_request(oc);
        assert_eq!(req.query_params.len(), 2);
        assert_eq!(req.path_params.len(), 1);
        assert_eq!(req.query_params[0].key, "page");
        assert!(req.query_params[0].description.is_some());
        assert!(!req.query_params[1].enabled);
        assert_eq!(req.path_params[0].name, "id");

        let back = request_to_oc_http_request(req);
        assert_eq!(back.http.params.len(), 3);
        assert_eq!(back.http.params[0].param_type, Some("query".into()));
        assert_eq!(back.http.params[2].param_type, Some("path".into()));
    }

    #[test]
    fn runtime_auth_survives_roundtrip() {
        let yaml = r#"
info:
  name: Runtime Auth
  type: http
http:
  method: GET
  url: "https://api.example.com"
runtime:
  auth:
    type: bearer
    token: runtime-token
"#;
        let oc: OcHttpRequest = serde_yaml::from_str(yaml).unwrap();
        let req = oc_http_request_to_request(oc);
        assert!(req.runtime_auth.is_some());
        match req.runtime_auth.as_ref().unwrap() {
            Auth::Bearer { token } => assert_eq!(token, "runtime-token"),
            _ => panic!("expected Bearer"),
        }

        let back = request_to_oc_http_request(req);
        let rt = back.runtime.unwrap();
        assert!(rt.auth.is_some());
    }

    #[test]
    fn oauth2_typed_subfields_roundtrip() {
        let yaml = r#"
info:
  name: OAuth2 Test
  type: http
http:
  method: GET
  url: "https://api.example.com"
  auth:
    type: oauth2
    flow: client_credentials
    accessTokenUrl: "https://auth.example.com/token"
    credentials:
      clientId: my-id
      clientSecret: my-secret
    additionalParameters:
      accessTokenRequest:
        - name: audience
          value: "https://api.example.com"
    tokenConfig:
      id: my-token
      placement:
        header: Authorization
    settings:
      autoFetchToken: true
      autoRefreshToken: false
"#;
        let oc: OcHttpRequest = serde_yaml::from_str(yaml).unwrap();
        let req = oc_http_request_to_request(oc);
        match &req.auth {
            Auth::OAuth2(flow) => {
                assert!(matches!(flow, rocket_shared::oauth2::OAuth2Flow::ClientCredentials { .. }));
            }
            _ => panic!("expected OAuth2"),
        }
        let back = request_to_oc_http_request(req);
        let auth = back.http.auth.unwrap();
        match auth {
            OcAuth::Typed(OcAuthTyped::OAuth2 { additional_parameters, token_config, settings, .. }) => {
                assert!(additional_parameters.is_some());
                assert!(token_config.is_some());
                assert!(settings.is_some());
            }
            _ => panic!("expected OAuth2"),
        }
    }

    #[test]
    fn settings_survive_roundtrip() {
        let yaml = r#"
info:
  name: With Settings
  type: http
http:
  method: GET
  url: "https://api.example.com"
settings:
  encodeUrl: true
  timeout: 30000
  followRedirects: inherit
  maxRedirects: 5
"#;
        let oc: OcHttpRequest = serde_yaml::from_str(yaml).unwrap();
        let req = oc_http_request_to_request(oc);
        let s = req.settings.as_ref().unwrap();
        assert!(matches!(s.encode_url, Some(RequestSettingValue::Value(true))));
        assert!(matches!(s.follow_redirects, Some(RequestSettingValue::Inherit(_))));

        let back = request_to_oc_http_request(req);
        let os = back.settings.unwrap();
        assert_eq!(os.encode_url, Some(InheritableBoolean::Value(true)));
        assert_eq!(os.timeout, Some(InheritableNumber::Value(30000.0)));
        assert_eq!(os.follow_redirects, Some(InheritableBoolean::Inherit("inherit".into())));
        assert_eq!(os.max_redirects, Some(InheritableNumber::Value(5.0)));
    }

    #[test]
    fn multipart_metadata_preserved_in_roundtrip() {
        let part = OcMultipartFormPart {
            name: "avatar".into(),
            part_type: "file".into(),
            value: OcMultipartValue::Single("/tmp/avatar.png".into()),
            description: Some(Description::text("User avatar")),
            content_type: Some("image/png".into()),
            disabled: None,
        };
        let entry = multipart_to_entry(part);
        assert_eq!(entry.content_type, Some("image/png".into()));
        assert_eq!(entry.description, Some(Description::text("User avatar")));
        let back = entry_to_multipart(entry);
        assert_eq!(back.content_type, Some("image/png".into()));
        assert_eq!(back.description, Some(Description::text("User avatar")));
    }

    #[test]
    fn non_http_items_preserved_in_folder_roundtrip() {
        let yaml = r#"
info:
  name: Mixed
  type: folder
items:
  - info:
      name: Get Users
      type: http
    http:
      method: GET
      url: "https://api.example.com/users"
  - info:
      name: GQL Query
      type: graphql
    graphql:
      url: "https://api.example.com/graphql"
      body:
        query: "query { users { id } }"
"#;
        let oc: OcFolder = serde_yaml::from_str(yaml).unwrap();
        let folder = oc_folder_to_folder(oc);
        assert_eq!(folder.items.len(), 2);
        assert!(matches!(&folder.items[0], CollectionItem::Request(_)));
        assert!(matches!(&folder.items[1], CollectionItem::OpaqueItem(o) if o.protocol == "graphql"));

        let back = folder_to_oc_folder(folder);
        let items = back.items.unwrap();
        assert_eq!(items.len(), 2);
        assert!(matches!(&items[0], OcItem::Http(_)));
        assert!(matches!(&items[1], OcItem::GraphQL(_)));
    }

    #[test]
    fn body_multipart_form_uses_formdata_mode() {
        let oc = OcHttpRequestBody::MultipartForm { data: vec![
            OcMultipartFormPart {
                name: "file".into(),
                part_type: "file".into(),
                value: OcMultipartValue::Single("/tmp/test.txt".into()),
                description: None,
                content_type: Some("text/plain".into()),
                disabled: None,
            },
        ]};
        let body: Body = oc.into();
        assert_eq!(body.mode, BodyMode::FormData);
    }

    #[test]
    fn body_formurlencoded_roundtrip() {
        use rocket_shared::types::{FormDataEntry, FormDataType};
        let body = Body {
            mode: BodyMode::FormUrlEncoded,
            content: None,
            form_data: Some(vec![FormDataEntry {
                key: "user".into(),
                value: "admin".into(),
                entry_type: FormDataType::Text,
                enabled: true,
                content_type: None,
                description: None,
            }]),
            file_path: None,
        };
        let oc: OcHttpRequestBody = body.into();
        assert!(matches!(oc, OcHttpRequestBody::FormUrlEncoded { .. }));
    }

    #[test]
    fn body_formdata_roundtrip_emits_multipart() {
        use rocket_shared::types::{FormDataEntry, FormDataType};
        let body = Body {
            mode: BodyMode::FormData,
            content: None,
            form_data: Some(vec![FormDataEntry {
                key: "name".into(),
                value: "test".into(),
                entry_type: FormDataType::Text,
                enabled: true,
                content_type: None,
                description: None,
            }]),
            file_path: None,
        };
        let oc: OcHttpRequestBody = body.into();
        assert!(matches!(oc, OcHttpRequestBody::MultipartForm { .. }));
    }

    #[test]
    fn collection_to_oc_has_correct_version() {
        use rocket_collection::Collection;
        use rocket_collection::CollectionSettings;
        use rocket_collection::Folder;
        let col = Collection {
            name: "Test".into(),
            root: Folder { uid: "uid".into(), name: "Test".into(), dir_name: None, items: vec![] },
            settings: CollectionSettings::default(),
        };
        let oc = super::collection_to_oc_collection(col);
        assert_eq!(oc.opencollection.as_deref(), Some("1.0.0"));
    }
}

#[cfg(test)]
mod workspace_conversion_tests {
    use super::*;
    use rocket_workspace::{CollectionRefType, WorkspaceConfig};
    use std::path::PathBuf;

    #[test]
    fn workspace_config_to_oc_workspace_config() {
        let mut cfg = WorkspaceConfig::new("My API");
        cfg.description = Some("A great API".into());
        cfg.add_embedded_collection("users");
        cfg.add_external_collection("shared", PathBuf::from("/abs/path/shared"));
        cfg.environments.active_environment = Some("Production".into());
        cfg.global_environment = Some("Prod Global".into());

        let oc = OcWorkspaceConfig::from(cfg);
        assert_eq!(oc.opencollection.as_deref(), Some("1.0.0"));
        assert_eq!(oc.info.name, "My API");
        assert_eq!(oc.info.workspace_type.as_deref(), Some("workspace"));
        assert_eq!(oc.docs.as_deref(), Some("A great API"));
        assert_eq!(oc.collections.len(), 2);
        // Embedded → relative path collections/<name>
        assert_eq!(oc.collections[0].path, Some(PathBuf::from("collections/users")));
        // External → absolute path preserved
        assert_eq!(oc.collections[1].path, Some(PathBuf::from("/abs/path/shared")));
        assert_eq!(oc.environments.as_ref().unwrap().active_environment.as_deref(), Some("Production"));
        assert_eq!(oc.global_environment.as_deref(), Some("Prod Global"));
    }

    #[test]
    fn oc_workspace_config_to_workspace_config() {
        let oc = OcWorkspaceConfig {
            opencollection: Some("1.0.0".into()),
            info: OcWorkspaceInfo { name: "Acme".into(), workspace_type: Some("workspace".into()) },
            collections: vec![
                OcWorkspaceCollectionRef { name: "api".into(), path: Some(PathBuf::from("collections/api")) },
                OcWorkspaceCollectionRef { name: "ext".into(), path: Some(PathBuf::from("/abs/ext")) },
            ],
            docs: Some("Docs here".into()),
            environments: Some(OcWorkspaceEnvironments { active_environment: Some("Staging".into()) }),
            global_environment: Some("Global".into()),
        };
        let cfg = WorkspaceConfig::from(oc);
        assert_eq!(cfg.name, "Acme");
        assert_eq!(cfg.description.as_deref(), Some("Docs here"));
        assert_eq!(cfg.collections.len(), 2);
        // Relative path → Embedded
        assert_eq!(cfg.collections[0].ref_type, CollectionRefType::Embedded);
        // Absolute path → External
        assert_eq!(cfg.collections[1].ref_type, CollectionRefType::External);
        assert_eq!(cfg.environments.active_environment.as_deref(), Some("Staging"));
        assert_eq!(cfg.global_environment.as_deref(), Some("Global"));
    }
}
