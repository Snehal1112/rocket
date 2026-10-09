//! Read-only views that the workspace assistant's MCP tools return, and the
//! pure helpers that build them.
//!
//! Masking rules (spec section 3): a secret variable never carries a value,
//! and RocketVault values never appear (an environment lists its vault
//! references by name only). Literal credentials in auth fields, in
//! credential-named headers, query parameters and form fields, and in a
//! URL's user-info part are replaced with `REDACTED`. A value made only of
//! `{{variable}}` references, optionally after an auth scheme word such as
//! `Bearer`, is kept, because it carries no secret.
//!
//! These structs are MCP tool results, not IPC DTOs, so their fields keep
//! plain snake_case names.

use std::collections::HashSet;

use rocket_collection::{
    CollectionItem, CollectionSettings, CollectionVariable, Folder, FolderSettings, Request,
};
use rocket_environment::{Environment, Variable};
use rocket_history::HistoryEntry;
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::types::{Auth, Body, BodyMode, FormDataEntry, Header, QueryParam};
use serde_json::Value;

use crate::redaction::{is_sensitive_header, redact_url_secrets, redaction_forms, REDACTED};
use crate::runner_sequence::folder_dir_name;

/// Most request entries the outline lists before it falls back to counts.
pub const OUTLINE_ENTRY_CAP: usize = 400;

/// Upper bound for the rendered outline text, on top of the entry cap.
pub const OUTLINE_BYTE_CAP: usize = 16 * 1024;

/// Largest response body a tool result carries, in bytes.
pub const RESPONSE_BODY_CAP_BYTES: usize = 8 * 1024;

/// Most history entries `get_history` returns.
pub const HISTORY_LIMIT_MAX: usize = 10;

/// Auth scheme words that may come before a `{{variable}}` reference in a
/// header value without making the value a literal credential.
const AUTH_SCHEMES: &[&str] = &["bearer", "basic", "token", "digest", "apikey"];

/// A header, query parameter or form field whose name contains one of these
/// parts (case-insensitive) holds a credential.
const CREDENTIAL_NAME_PARTS: &[&str] = &[
    "token",
    "secret",
    "password",
    "passwd",
    "api-key",
    "api_key",
    "apikey",
    "auth",
    "session",
    "cookie",
    "credential",
    "signature",
    "sig",
    "private",
    "key",
    "code",
];

/// Auth fields that hold a URL. They are shown through `mask_url`.
const AUTH_URL_FIELDS: &[&str] = &[
    "accessTokenUrl",
    "authorizationUrl",
    "refreshTokenUrl",
    "callbackUrl",
];

/// Auth fields shown as they are. Every other string in an auth block is a
/// credential and is masked unless it holds only `{{variable}}` references,
/// so a field added to `Auth` later is masked by default.
const AUTH_VISIBLE_FIELDS: &[&str] = &[
    "authType",
    "flow",
    "username",
    "key",
    "placement",
    "region",
    "service",
    "profileName",
    "domain",
    "clientId",
    "accessTokenUrl",
    "authorizationUrl",
    "refreshTokenUrl",
    "callbackUrl",
    "scope",
    "method",
    "source",
    "name",
    "id",
    "signatureMethod",
    "version",
    "realm",
    "type",
];

/// One collection in a `list_collections` result.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CollectionBrief {
    pub name: String,
    pub request_count: usize,
    /// The collection's run switch ("Allow the agent to run requests in this
    /// collection").
    pub run_allowed: bool,
    /// Environment names, for `get_environment`.
    pub environments: Vec<String>,
}

/// A header, query parameter or form field with its value masked when it is
/// a literal credential.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MaskedPair {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

/// A variable. `value` is `None` for a secret variable.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MaskedVariable {
    pub key: String,
    pub value: Option<String>,
    pub enabled: bool,
    pub secret: bool,
}

/// A request body. `content` is kept for raw modes (JSON, XML, text); a
/// url-encoded body masks credential-named fields; form data is in `form`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MaskedBody {
    pub mode: String,
    pub content: Option<String>,
    pub form: Vec<MaskedPair>,
    pub file_path: Option<String>,
}

/// The full definition of one HTTP request, with credentials masked.
/// Scripts are returned in full.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MaskedRequest {
    pub path: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers: Vec<MaskedPair>,
    pub query_params: Vec<MaskedPair>,
    pub body: Option<MaskedBody>,
    /// The auth block in its stored shape (`authType` plus fields), masked.
    pub auth: Value,
    pub variables: Vec<MaskedVariable>,
    pub pre_request_script: Option<String>,
    pub post_response_script: Option<String>,
    pub tests: Option<String>,
}

impl MaskedRequest {
    pub fn from_request(path: &str, request: &Request) -> Self {
        Self {
            path: path.to_string(),
            name: request.name.clone(),
            method: request.method.to_string(),
            url: mask_url(&request.url),
            headers: request.headers.iter().map(mask_header).collect(),
            query_params: request.query_params.iter().map(mask_query_param).collect(),
            body: request.body.as_ref().map(mask_body),
            auth: mask_auth(&request.auth),
            variables: request
                .variables
                .iter()
                .map(mask_collection_variable)
                .collect(),
            pre_request_script: request.pre_request_script.clone(),
            post_response_script: request.post_response_script.clone(),
            tests: request.tests.clone(),
        }
    }
}

/// A collection's settings: auth, default headers, variables and the run
/// switch.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MaskedSettings {
    pub auth_type: String,
    pub auth: Value,
    pub headers: Vec<MaskedPair>,
    pub variables: Vec<MaskedVariable>,
    pub run_allowed: bool,
}

impl MaskedSettings {
    pub fn from_settings(settings: &CollectionSettings) -> Self {
        let auth = settings.auth.as_ref().map(mask_auth).unwrap_or(Value::Null);
        Self {
            auth_type: auth_type_name(&auth),
            auth,
            headers: settings.headers.iter().map(mask_header).collect(),
            variables: settings
                .variables
                .iter()
                .map(mask_collection_variable)
                .collect(),
            run_allowed: settings.agent_autonomy_enabled,
        }
    }
}

/// An environment: variable names with non-secret values, and its
/// RocketVault references as `alias.secretName` names.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MaskedEnvironment {
    pub name: String,
    pub variables: Vec<MaskedVariable>,
    pub vault_references: Vec<String>,
}

impl MaskedEnvironment {
    pub fn from_environment(env: &Environment) -> Self {
        Self {
            name: env.name.clone(),
            variables: env.variables.iter().map(mask_env_variable).collect(),
            vault_references: env
                .external_secrets
                .iter()
                .flat_map(|binding| {
                    binding
                        .secret_names
                        .iter()
                        .map(move |secret| format!("{}.{}", binding.alias, secret.name))
                })
                .collect(),
        }
    }
}

/// One past run of a request. History stores no response body, so none is
/// returned; `run_request` returns the body of a fresh run.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct HistoryBrief {
    pub timestamp: String,
    pub method: String,
    pub url: String,
    pub status: u16,
    pub duration_ms: u64,
    pub response_size: usize,
    pub run_source: rocket_shared::RunSource,
}

impl HistoryBrief {
    pub fn from_entry(entry: &HistoryEntry) -> Self {
        Self {
            timestamp: entry.timestamp.to_rfc3339(),
            method: entry.method.clone(),
            url: mask_url(&entry.url),
            status: entry.status,
            duration_ms: entry.duration_ms,
            response_size: entry.response_size,
            run_source: entry.run_source,
        }
    }
}

/// One collection's section of the workspace outline.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OutlineCollection {
    pub name: String,
    pub run_allowed: bool,
    /// False when the collection's tree could not be read.
    pub readable: bool,
    pub entries: Vec<OutlineEntry>,
}

/// One HTTP request in the outline: method and path relative to the
/// collection root.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OutlineEntry {
    pub method: String,
    pub path: String,
}

/// Whether `value` holds only `{{variable}}` references, optionally next to
/// one auth scheme word (`Bearer {{token}}`). Such a value carries no secret.
pub(crate) fn is_reference_only(value: &str) -> bool {
    let mut rest = String::new();
    let mut remaining = value;
    let mut saw_reference = false;
    while let Some(start) = remaining.find("{{") {
        rest.push_str(&remaining[..start]);
        match remaining[start..].find("}}") {
            Some(end) => {
                saw_reference = true;
                remaining = &remaining[start + end + 2..];
            }
            None => {
                rest.push_str(&remaining[start..]);
                remaining = "";
            }
        }
    }
    rest.push_str(remaining);
    let rest = rest.trim();
    saw_reference
        && (rest.is_empty()
            || AUTH_SCHEMES
                .iter()
                .any(|scheme| rest.eq_ignore_ascii_case(scheme)))
}

/// Whether a header, query parameter or form field name holds a credential.
pub(crate) fn is_credential_name(name: &str) -> bool {
    if is_sensitive_header(name) {
        return true;
    }
    // A percent-encoded name such as `client%5Fsecret` is matched decoded.
    let decoded = percent_encoding::percent_decode_str(name).decode_utf8_lossy();
    let lower = decoded.to_ascii_lowercase();
    if is_sensitive_header(&decoded) {
        return true;
    }
    CREDENTIAL_NAME_PARTS.iter().any(|part| lower.contains(part))
}

/// The value to show for a named field: masked when the name holds a
/// credential and the value is a non-empty literal.
pub(crate) fn mask_named_value(name: &str, value: &str) -> String {
    if value.is_empty() || !is_credential_name(name) || is_reference_only(value) {
        value.to_string()
    } else {
        REDACTED.to_string()
    }
}

pub(crate) fn mask_header(header: &Header) -> MaskedPair {
    MaskedPair {
        key: header.key.clone(),
        value: mask_named_value(&header.key, &header.value),
        enabled: header.enabled,
    }
}

fn mask_query_param(param: &QueryParam) -> MaskedPair {
    MaskedPair {
        key: param.key.clone(),
        value: mask_named_value(&param.key, &param.value),
        enabled: param.enabled,
    }
}

fn mask_form_entry(entry: &FormDataEntry) -> MaskedPair {
    MaskedPair {
        key: entry.key.clone(),
        value: mask_named_value(&entry.key, &entry.value),
        enabled: entry.enabled,
    }
}

pub(crate) fn mask_body(body: &Body) -> MaskedBody {
    let mode = serde_json::to_value(&body.mode)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    let content = if matches!(body.mode, BodyMode::FormUrlEncoded) {
        body.content.as_deref().map(mask_query_string)
    } else {
        body.content.clone()
    };
    MaskedBody {
        mode,
        content,
        form: body.form_data.iter().flatten().map(mask_form_entry).collect(),
        file_path: body.file_path.clone(),
    }
}

pub(crate) fn mask_collection_variable(variable: &CollectionVariable) -> MaskedVariable {
    MaskedVariable {
        key: variable.key.clone(),
        value: (!variable.secret).then(|| variable.value.clone()),
        enabled: variable.enabled,
        secret: variable.secret,
    }
}

fn mask_env_variable(variable: &Variable) -> MaskedVariable {
    MaskedVariable {
        key: variable.key.clone(),
        value: (!variable.secret).then(|| variable.value.clone()),
        enabled: variable.enabled,
        secret: variable.secret,
    }
}

/// Masks credential-named values in a `name=value&...` string.
pub(crate) fn mask_query_string(query: &str) -> String {
    query
        .split('&')
        .map(|pair| match pair.split_once('=') {
            Some((name, value)) => format!("{name}={}", mask_named_value(name, value)),
            None => pair.to_string(),
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// Masks a URL's user-info password and its credential-named query values.
/// A URL with `{{variables}}` that does not parse is handled the same way,
/// because this works on the text, not on a parsed URL.
pub(crate) fn mask_url(url: &str) -> String {
    let (before_fragment, fragment) = match url.find('#') {
        Some(i) => (&url[..i], &url[i..]),
        None => (url, ""),
    };
    let (base, query) = match before_fragment.find('?') {
        Some(i) => (&before_fragment[..i], Some(&before_fragment[i + 1..])),
        None => (before_fragment, None),
    };
    let mut out = mask_userinfo(base);
    if let Some(query) = query {
        out.push('?');
        out.push_str(&mask_query_string(query));
    }
    if let Some(fragment) = fragment.strip_prefix('#') {
        out.push('#');
        // A fragment such as `access_token=...` holds name=value pairs.
        out.push_str(&mask_query_string(fragment));
    }
    out
}

fn mask_userinfo(base: &str) -> String {
    let Some(scheme_end) = base.find("://") else {
        return base.to_string();
    };
    let authority_start = scheme_end + 3;
    let rest = &base[authority_start..];
    let authority_end = rest.find('/').unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let Some(at) = authority.rfind('@') else {
        return base.to_string();
    };
    let userinfo = &authority[..at];
    let Some(colon) = userinfo.find(':') else {
        // User-info without a colon is a bare token (`ghp_XXXX@host`).
        if userinfo.is_empty() || is_reference_only(userinfo) {
            return base.to_string();
        }
        return format!("{}{REDACTED}{}", &base[..authority_start], &rest[at..]);
    };
    let password = &userinfo[colon + 1..];
    if password.is_empty() || is_reference_only(password) {
        return base.to_string();
    }
    format!(
        "{}{}:{}{}",
        &base[..authority_start],
        &userinfo[..colon],
        REDACTED,
        &rest[at..]
    )
}

/// The auth block in its stored JSON shape, with every string outside
/// `AUTH_VISIBLE_FIELDS` masked unless it is empty or reference-only.
pub(crate) fn mask_auth(auth: &Auth) -> Value {
    let mut value = serde_json::to_value(auth).unwrap_or(Value::Null);
    mask_auth_value(&mut value, None);
    value
}

fn mask_auth_value(value: &mut Value, field: Option<&str>) {
    match value {
        Value::String(text) => {
            if field.is_some_and(|f| AUTH_URL_FIELDS.contains(&f)) {
                *text = mask_url(text);
                return;
            }
            let visible = field.is_some_and(|f| AUTH_VISIBLE_FIELDS.contains(&f));
            if !visible && !text.is_empty() && !is_reference_only(text) {
                *text = REDACTED.to_string();
            }
        }
        Value::Array(items) => {
            for item in items {
                mask_auth_value(item, field);
            }
        }
        Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                mask_auth_value(item, Some(key.as_str()));
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn auth_type_name(auth: &Value) -> String {
    auth.get("authType")
        .and_then(Value::as_str)
        .unwrap_or("none")
        .to_string()
}

/// Adds a credential value and, for a multi-word value such as
/// `Bearer sk-live-1`, each word after the first, so an echo of the token
/// alone is masked too.
fn add_credential(value: &str, out: &mut HashSet<String>) {
    if value.is_empty() || value == REDACTED {
        return;
    }
    out.insert(value.to_string());
    for word in value.split_whitespace().skip(1) {
        out.insert(word.to_string());
    }
}

fn add_credential_pairs(query: &str, out: &mut HashSet<String>) {
    for pair in query.split('&') {
        if let Some((name, value)) = pair.split_once('=') {
            if mask_named_value(name, value) != value {
                add_credential(value, out);
            }
        }
    }
}

/// Collects the literal strings that `mask_auth` replaced.
fn collect_masked_auth(original: &Value, masked: &Value, out: &mut HashSet<String>) {
    match (original, masked) {
        (Value::String(orig), Value::String(mask)) if mask == REDACTED => {
            add_credential(orig, out);
        }
        (Value::Object(orig), Value::Object(mask)) => {
            for (key, value) in orig {
                if let Some(masked_value) = mask.get(key) {
                    collect_masked_auth(value, masked_value, out);
                }
            }
        }
        (Value::Array(orig), Value::Array(mask)) => {
            for (value, masked_value) in orig.iter().zip(mask) {
                collect_masked_auth(value, masked_value, out);
            }
        }
        _ => {}
    }
}

fn add_url_credentials(url: &str, out: &mut HashSet<String>) {
    let before_fragment = url.split('#').next().unwrap_or(url);
    if let Some((_, fragment)) = url.split_once('#') {
        add_credential_pairs(fragment, out);
    }
    let (base, query) = match before_fragment.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (before_fragment, None),
    };
    if let Some(query) = query {
        add_credential_pairs(query, out);
    }
    if let Some(scheme_end) = base.find("://") {
        let rest = &base[scheme_end + 3..];
        let authority = &rest[..rest.find('/').unwrap_or(rest.len())];
        if let Some(at) = authority.rfind('@') {
            let userinfo = &authority[..at];
            let secret = userinfo.split_once(':').map_or(userinfo, |(_, p)| p);
            if !is_reference_only(secret) {
                add_credential(secret, out);
            }
        }
    }
}

/// Adds the credentials inside the URL-valued fields of an auth block.
fn collect_auth_url_credentials(value: &Value, out: &mut HashSet<String>) {
    if let Value::Object(map) = value {
        for (key, item) in map {
            match item {
                Value::String(url) if AUTH_URL_FIELDS.contains(&key.as_str()) => {
                    add_url_credentials(url, out);
                }
                other => collect_auth_url_credentials(other, out),
            }
        }
    }
}

/// The `Authorization: Basic ...` value the executor sends for a login.
pub(crate) fn basic_header_forms(username: &str, password: &str, out: &mut HashSet<String>) {
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
    out.insert(format!("Basic {encoded}"));
    out.insert(encoded);
}

/// Every auth block that applies to a request: its own, the folder chain's
/// and the collection's.
fn applicable_auths<'a>(
    request: &'a Request,
    settings: Option<&'a CollectionSettings>,
    folders: &'a [FolderSettings],
) -> Vec<&'a Auth> {
    let mut auths = vec![&request.auth];
    auths.extend(folders.iter().filter_map(|f| f.auth.as_ref()));
    auths.extend(settings.and_then(|s| s.auth.as_ref()));
    auths
}

/// The literal credentials `mask_auth` and `mask_named_value` would mask in
/// a request, in the collection settings and in the folder chain that apply
/// to it, plus the `Basic` header values built from a literal login. A
/// response that echoes the request (an `/anything` endpoint) must not
/// return them either.
pub(crate) fn literal_credential_values(
    request: &Request,
    settings: Option<&CollectionSettings>,
    folders: &[FolderSettings],
) -> HashSet<String> {
    let mut out = HashSet::new();
    for auth in applicable_auths(request, settings, folders) {
        if let Ok(original) = serde_json::to_value(auth) {
            let masked = mask_auth(auth);
            collect_masked_auth(&original, &masked, &mut out);
            collect_auth_url_credentials(&original, &mut out);
        }
        if let Auth::Basic { username, password } = auth {
            if !password.is_empty() && !is_reference_only(password) && !username.contains("{{") {
                basic_header_forms(username, password, &mut out);
            }
        }
    }
    let mut headers: Vec<&Header> = request.headers.iter().collect();
    headers.extend(folders.iter().flat_map(|f| f.headers.iter()));
    if let Some(settings) = settings {
        headers.extend(settings.headers.iter());
    }
    for header in headers {
        if mask_named_value(&header.key, &header.value) != header.value {
            add_credential(&header.value, &mut out);
        }
    }
    for param in &request.query_params {
        if mask_named_value(&param.key, &param.value) != param.value {
            add_credential(&param.value, &mut out);
        }
    }
    add_url_credentials(&request.url, &mut out);
    if let Some(body) = request.body.as_ref() {
        for entry in body.form_data.iter().flatten() {
            if mask_named_value(&entry.key, &entry.value) != entry.value {
                add_credential(&entry.value, &mut out);
            }
        }
        if matches!(body.mode, BodyMode::FormUrlEncoded) {
            if let Some(content) = body.content.as_deref() {
                add_credential_pairs(content, &mut out);
            }
        }
    }
    out
}

/// `Basic` header values for a login whose password is a `{{variable}}`
/// holding a secret: the username is paired with each secret the run
/// resolved. Over-inclusive on purpose, since the variable is not resolved
/// here.
pub(crate) fn basic_header_values_from_secrets(
    request: &Request,
    settings: Option<&CollectionSettings>,
    folders: &[FolderSettings],
    run_secrets: &HashSet<String>,
) -> HashSet<String> {
    let mut out = HashSet::new();
    for auth in applicable_auths(request, settings, folders) {
        if let Auth::Basic { username, password } = auth {
            if is_reference_only(password) && !username.contains("{{") {
                for secret in run_secrets {
                    basic_header_forms(username, secret, &mut out);
                }
            }
        }
    }
    out
}

/// Cuts `text` to at most `max_bytes`, backing off to a character
/// boundary. Returns the text and whether it was cut.
pub(crate) fn truncate_utf8(text: &str, max_bytes: usize) -> (String, bool) {
    if text.len() <= max_bytes {
        return (text.to_string(), false);
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_string(), true)
}

/// Masks the known secret values in a response body, then cuts it to
/// `RESPONSE_BODY_CAP_BYTES`. Masking first means a cut can never leave a
/// fragment of a secret behind.
pub(crate) fn mask_response_body(body: &str, secret_values: &HashSet<String>) -> (String, bool) {
    // Encoded and per-line forms of each secret are masked too.
    let mut all: HashSet<String> = HashSet::new();
    for secret in secret_values {
        all.extend(redaction_forms(secret));
    }
    let masked = redact_url_secrets(body, &all);
    truncate_utf8(&masked, RESPONSE_BODY_CAP_BYTES)
}

/// A requested history limit, where 0 means the maximum.
pub(crate) fn history_limit(requested: usize) -> usize {
    if requested == 0 {
        HISTORY_LIMIT_MAX
    } else {
        requested.min(HISTORY_LIMIT_MAX)
    }
}

/// Normalizes a folder filter such as `/auth/v2/` to `auth/v2`. An empty
/// value means no filter. Traversal segments, empty segments and
/// backslashes are refused.
pub(crate) fn normalize_folder(folder: &str) -> DomainResult<Option<String>> {
    let trimmed = folder.trim_matches('/');
    if trimmed.is_empty() {
        return Ok(None);
    }
    let invalid = trimmed.contains('\0')
        || trimmed.contains('\\')
        || trimmed
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..");
    if invalid {
        return Err(DomainError::InvalidInput("invalid folder path".to_string()));
    }
    Ok(Some(trimmed.to_string()))
}

/// Every HTTP request in a summary tree, depth first, with paths built from
/// folder directory names (`folder_dir_name`), so they match what
/// `get_request` and `run_request` expect. Non-HTTP items are skipped, as
/// the old `list_collection_requests` did.
pub(crate) fn outline_entries(root: &Folder) -> Vec<OutlineEntry> {
    let mut out = Vec::new();
    walk_outline(root, "", &mut out);
    out
}

fn walk_outline(folder: &Folder, prefix: &str, out: &mut Vec<OutlineEntry>) {
    for item in &folder.items {
        match item {
            CollectionItem::Summary(summary) => {
                if !summary.kind.is_http() {
                    continue;
                }
                let Some(file_name) = summary.file_name.as_ref() else {
                    continue;
                };
                out.push(OutlineEntry {
                    method: summary.method.clone(),
                    path: format!("{prefix}{file_name}"),
                });
            }
            CollectionItem::Folder(sub) => {
                let sub_prefix = format!("{prefix}{}/", folder_dir_name(sub));
                walk_outline(sub, &sub_prefix, out);
            }
            CollectionItem::Request(_)
            | CollectionItem::OpaqueItem(_)
            | CollectionItem::GraphQl(_)
            | CollectionItem::WebSocket(_)
            | CollectionItem::Grpc(_)
            | CollectionItem::ScriptFile(_) => {}
        }
    }
}

/// Keeps the entries under `folder` (already normalized).
pub(crate) fn filter_folder(entries: Vec<OutlineEntry>, folder: &str) -> Vec<OutlineEntry> {
    let prefix = format!("{folder}/");
    entries
        .into_iter()
        .filter(|entry| entry.path.starts_with(&prefix))
        .collect()
}

/// Collapses a user-chosen name or path to one safe line: control
/// characters (newlines included) become spaces and backticks become
/// apostrophes, so a name cannot start a heading or fake an instruction.
fn single_line(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '`' => '\'',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect()
}

/// Cuts `text` to at most `OUTLINE_BYTE_CAP` bytes at a line boundary and
/// adds a visible note.
fn cap_outline_bytes(mut text: String) -> String {
    if text.len() <= OUTLINE_BYTE_CAP {
        return text;
    }
    const NOTE: &str = "... outline truncated. Call get_workspace_outline with a collection, \
and optionally a folder, to see the rest.\n";
    let mut cut = OUTLINE_BYTE_CAP - NOTE.len();
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    text.truncate(cut);
    if let Some(newline) = text.rfind('\n') {
        text.truncate(newline + 1);
    }
    text.push_str(NOTE);
    text
}

/// Renders the outline as compact text. Up to `OUTLINE_ENTRY_CAP` entries
/// are listed. Above the cap, several collections are shown as counts only;
/// a single collection lists the first `OUTLINE_ENTRY_CAP` entries and
/// counts the rest.
pub(crate) fn render_outline(collections: &[OutlineCollection]) -> String {
    let total: usize = collections.iter().map(|c| c.entries.len()).sum();
    let counts_only = total > OUTLINE_ENTRY_CAP && collections.len() > 1;
    let mut out = format!(
        "Workspace outline: {} collection(s), {total} request(s). Each line is the \
         method and the request path relative to its collection. \"run: on\" means \
         the user allows running requests in that collection.\n",
        collections.len()
    );
    if counts_only {
        out.push_str(&format!(
            "The workspace has more than {OUTLINE_ENTRY_CAP} requests, so only counts \
             are listed. Call get_workspace_outline with a collection, and optionally \
             a folder, to list requests.\n"
        ));
    }
    let mut remaining = OUTLINE_ENTRY_CAP;
    for collection in collections {
        let run = if collection.run_allowed {
            "run: on"
        } else {
            "run: off"
        };
        if !collection.readable {
            out.push_str(&format!(
                "\n## {} ({run}, could not be read)\n",
                single_line(&collection.name)
            ));
            continue;
        }
        out.push_str(&format!(
            "\n## {} ({run}, {} request(s))\n",
            single_line(&collection.name),
            collection.entries.len()
        ));
        if counts_only {
            continue;
        }
        let shown = collection.entries.len().min(remaining);
        for entry in &collection.entries[..shown] {
            out.push_str(&format!(
                "{} {}\n",
                entry.method,
                single_line(&entry.path)
            ));
        }
        remaining -= shown;
        if shown < collection.entries.len() {
            out.push_str(&format!(
                "... {} more not shown. Call get_workspace_outline with this collection \
                 and a folder to see them.\n",
                collection.entries.len() - shown
            ));
        }
    }
    cap_outline_bytes(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::{Collection, RequestKind, RequestSummary};
    use rocket_environment::{ExternalSecretBinding, ExternalSecretRef};
    use rocket_shared::types::{FormDataType, HttpMethod};

    #[test]
    fn reference_only_values_are_kept_and_literal_credentials_are_masked() {
        let cases = [
            ("Authorization", "Bearer {{token}}", "Bearer {{token}}"),
            ("Authorization", "{{scheme}} {{token}}", "{{scheme}} {{token}}"),
            ("Authorization", "Bearer sk-live-abc123", REDACTED),
            ("Authorization", "Bearer abc{{suffix}}", REDACTED),
            ("Authorization", "{{token", REDACTED),
            ("X-Api-Key", "{{apiKey}}", "{{apiKey}}"),
            ("X-Session-Token", "s-123456", REDACTED),
            ("Accept", "application/json", "application/json"),
            ("Authorization", "", ""),
        ];
        for (name, value, expected) in cases {
            assert_eq!(mask_named_value(name, value), expected, "{name}: {value}");
        }
    }

    #[test]
    fn disabled_sensitive_headers_are_masked_too() {
        let masked = mask_header(&Header::disabled("Cookie", "session=abcdef123"));
        assert_eq!(masked.value, REDACTED);
        assert!(!masked.enabled);
    }

    #[test]
    fn urls_lose_userinfo_passwords_and_credential_query_values() {
        assert_eq!(
            mask_url("https://alice:hunter22@api.test/x?api_key=sk-live-1&page=2#top"),
            format!("https://alice:{REDACTED}@api.test/x?api_key={REDACTED}&page=2#top")
        );
        assert_eq!(
            mask_url("{{baseUrl}}/x?token={{token}}"),
            "{{baseUrl}}/x?token={{token}}"
        );
        assert_eq!(
            mask_url("https://{{user}}:{{pass}}@api.test/"),
            "https://{{user}}:{{pass}}@api.test/"
        );
        assert_eq!(mask_url("https://api.test/plain"), "https://api.test/plain");
    }

    #[test]
    fn auth_blocks_mask_literal_secrets_and_keep_names_and_references() {
        let basic = mask_auth(&Auth::Basic {
            username: "alice".into(),
            password: "hunter22".into(),
        });
        assert_eq!(basic["authType"], "basic");
        assert_eq!(basic["username"], "alice");
        assert_eq!(basic["password"], REDACTED);

        let bearer = mask_auth(&Auth::Bearer {
            token: "{{token}}".into(),
        });
        assert_eq!(bearer["token"], "{{token}}");

        let api_key = mask_auth(&Auth::ApiKey {
            key: "X-Api-Key".into(),
            value: "sk-live-1".into(),
            placement: "header".into(),
        });
        assert_eq!(api_key["key"], "X-Api-Key");
        assert_eq!(api_key["value"], REDACTED);
        assert_eq!(api_key["placement"], "header");

        let aws = mask_auth(&Auth::AwsSigV4 {
            access_key: "AKIAEXAMPLE".into(),
            secret_key: "wJalrXUtnFEMI".into(),
            region: "eu-west-1".into(),
            service: "execute-api".into(),
            session_token: None,
            profile_name: None,
        });
        assert_eq!(aws["secretKey"], REDACTED);
        assert_eq!(aws["region"], "eu-west-1");
    }

    #[test]
    fn credential_named_form_fields_and_url_encoded_bodies_are_masked() {
        let form = Body {
            mode: BodyMode::FormData,
            content: None,
            form_data: Some(vec![
                FormDataEntry {
                    key: "password".into(),
                    value: "hunter22".into(),
                    entry_type: FormDataType::Text,
                    enabled: true,
                    content_type: None,
                    description: None,
                },
                FormDataEntry {
                    key: "username".into(),
                    value: "alice".into(),
                    entry_type: FormDataType::Text,
                    enabled: true,
                    content_type: None,
                    description: None,
                },
            ]),
            file_path: None,
        };
        let masked = mask_body(&form);
        assert_eq!(masked.mode, "formdata");
        assert_eq!(masked.form[0].value, REDACTED);
        assert_eq!(masked.form[1].value, "alice");

        let encoded = Body {
            mode: BodyMode::FormUrlEncoded,
            content: Some("client_secret=cs-1&grant_type=client_credentials".into()),
            form_data: None,
            file_path: None,
        };
        let expected = format!("client_secret={REDACTED}&grant_type=client_credentials");
        assert_eq!(mask_body(&encoded).content.as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn a_request_view_masks_headers_query_auth_and_url_but_keeps_scripts_whole() {
        let mut request = Request::new(
            "Login",
            HttpMethod::Post,
            "https://alice:hunter22@api.test/login",
        )
        .with_header("Authorization", "Bearer sk-live-abc123")
        .with_auth(Auth::Bearer {
            token: "sk-live-abc123".into(),
        });
        request.query_params.push(QueryParam {
            key: "api_key".into(),
            value: "sk-live-q".into(),
            enabled: true,
            description: None,
        });
        let script = format!("const big = '{}';", "x".repeat(20_000));
        request.tests = Some(script.clone());

        let view = MaskedRequest::from_request("auth/login.yml", &request);
        let json = serde_json::to_string(&view).expect("serialize");
        for secret in ["hunter22", "sk-live-abc123", "sk-live-q"] {
            assert!(!json.contains(secret), "{secret} leaked into the request view");
        }
        assert_eq!(view.method, "POST");
        assert_eq!(view.path, "auth/login.yml");
        assert_eq!(
            view.tests.as_deref(),
            Some(script.as_str()),
            "scripts are returned in full"
        );
    }

    #[test]
    fn secret_variables_never_carry_a_value() {
        let secret = CollectionVariable {
            key: "clientSecret".into(),
            value: "cs-live-999".into(),
            initial_value: "cs-initial-999".into(),
            enabled: true,
            secret: true,
        };
        let masked = mask_collection_variable(&secret);
        assert_eq!(masked.value, None);
        assert!(masked.secret);
        let json = serde_json::to_string(&masked).expect("serialize");
        assert!(!json.contains("cs-live-999"));
        assert!(!json.contains("cs-initial-999"));
    }

    #[test]
    fn an_environment_lists_vault_references_by_name_only() {
        let mut env = Environment::new("dev");
        env.set_variable(Variable::new("HOST", "api.example.com"));
        env.set_variable(Variable::secret("API_KEY", "sk-live-abc"));
        env.external_secrets.push(ExternalSecretBinding {
            alias: "prod".into(),
            connection_id: "conn-1".into(),
            vault_name: "main".into(),
            secret_names: vec![ExternalSecretRef {
                name: "db-pass".into(),
                secret_id: "id-1".into(),
            }],
        });

        let view = MaskedEnvironment::from_environment(&env);
        assert_eq!(view.vault_references, vec!["prod.db-pass".to_string()]);
        let host = view
            .variables
            .iter()
            .find(|v| v.key == "HOST")
            .expect("HOST is listed");
        assert_eq!(host.value.as_deref(), Some("api.example.com"));
        let json = serde_json::to_string(&view).expect("serialize");
        assert!(!json.contains("sk-live-abc"));
    }

    fn summary(method: &str, file: &str) -> RequestSummary {
        RequestSummary {
            uid: format!("uid-{file}"),
            name: file.into(),
            method: method.into(),
            url: String::new(),
            file_name: Some(file.into()),
            kind: Default::default(),
        }
    }

    #[test]
    fn outline_entries_walk_folders_by_directory_name_and_skip_non_http_items() {
        let mut collection = Collection::new("api");
        collection.root.add_summary(summary("GET", "ping.yml"));
        let mut graphql = summary("POST", "query.yml");
        graphql.kind = RequestKind::GraphQl;
        collection.root.add_summary(graphql);
        let mut folder = Folder::new("Auth Flows");
        folder.dir_name = Some("auth".into());
        folder.add_summary(summary("POST", "login.yml"));
        collection.root.add_subfolder(folder);

        let entries = outline_entries(&collection.root);
        let lines: Vec<String> = entries
            .iter()
            .map(|e| format!("{} {}", e.method, e.path))
            .collect();
        assert_eq!(lines, vec!["GET ping.yml", "POST auth/login.yml"]);
        assert_eq!(filter_folder(entries, "auth").len(), 1);
    }

    fn outline_collection(name: &str, count: usize) -> OutlineCollection {
        OutlineCollection {
            name: name.to_string(),
            run_allowed: false,
            readable: true,
            entries: (0..count)
                .map(|i| OutlineEntry {
                    method: "GET".to_string(),
                    path: format!("r{i}.yml"),
                })
                .collect(),
        }
    }

    #[test]
    fn names_and_paths_are_collapsed_to_one_safe_line() {
        let mut collection = outline_collection("evil\n## x (run: on)`", 1);
        collection.entries[0].path = "a\r\n## fake\nb.yml".to_string();
        let text = render_outline(&[collection]);

        assert!(text.contains("\n## evil ## x (run: on)' (run: off, 1 request(s))\n"), "{text}");
        assert!(text.contains("\nGET a  ## fake b.yml\n"), "{text}");
        assert_eq!(text.lines().filter(|l| l.starts_with("## ")).count(), 1);
    }

    #[test]
    fn the_outline_is_cut_at_the_byte_cap_with_a_note() {
        let collections: Vec<OutlineCollection> = (0..1500)
            .map(|i| outline_collection(&format!("collection-{i:04}-{}", "x".repeat(30)), 1))
            .collect();
        let text = render_outline(&collections);

        assert!(text.len() <= OUTLINE_BYTE_CAP, "{}", text.len());
        assert!(text.contains("outline truncated"), "{text}");
        assert!(text.ends_with('\n'));
    }

    fn entry_lines(text: &str) -> usize {
        text.lines().filter(|l| l.starts_with("GET ")).count()
    }

    #[test]
    fn outline_at_the_cap_lists_every_request() {
        let text = render_outline(&[outline_collection("a", 200), outline_collection("b", 200)]);
        assert_eq!(entry_lines(&text), OUTLINE_ENTRY_CAP);
        assert!(!text.contains("only counts"));
    }

    #[test]
    fn outline_one_over_the_cap_falls_back_to_counts() {
        let text = render_outline(&[outline_collection("a", 200), outline_collection("b", 201)]);
        assert_eq!(entry_lines(&text), 0);
        assert!(text.contains("only counts"));
        assert!(text.contains("## b (run: off, 201 request(s))"));
    }

    #[test]
    fn a_single_collection_over_the_cap_lists_the_first_400_and_counts_the_rest() {
        let text = render_outline(&[outline_collection("big", 405)]);
        assert_eq!(entry_lines(&text), OUTLINE_ENTRY_CAP);
        assert!(text.contains("... 5 more not shown"));
    }

    #[test]
    fn an_unreadable_collection_is_named_in_the_outline() {
        let mut broken = outline_collection("broken", 0);
        broken.readable = false;
        let text = render_outline(&[broken, outline_collection("ok", 1)]);
        assert!(text.contains("## broken (run: off, could not be read)"));
        assert!(text.contains("GET r0.yml"));
    }

    #[test]
    fn folder_paths_are_normalized_and_traversal_is_refused() {
        assert_eq!(normalize_folder("/auth/v2/").expect("valid"), Some("auth/v2".to_string()));
        assert_eq!(normalize_folder("/").expect("root"), None);
        for bad in ["../x", "auth/../../x", "auth//v2", "a\\b", "./auth"] {
            assert!(normalize_folder(bad).is_err(), "{bad} must be refused");
        }
    }

    #[test]
    fn truncation_never_splits_a_character() {
        let text = format!("a{}", "é".repeat(5_000));
        let (cut, truncated) = truncate_utf8(&text, RESPONSE_BODY_CAP_BYTES);
        assert!(truncated);
        assert_eq!(cut.len(), RESPONSE_BODY_CAP_BYTES - 1);
        assert!(text.starts_with(&cut));
        let (whole, truncated) = truncate_utf8("short", RESPONSE_BODY_CAP_BYTES);
        assert_eq!(whole, "short");
        assert!(!truncated);
    }

    #[test]
    fn a_secret_straddling_the_cut_is_masked_before_truncation() {
        let secret = "sk-live-straddling-secret".to_string();
        let body = format!("{}{secret}{}", "x".repeat(RESPONSE_BODY_CAP_BYTES - 5), "y".repeat(100));
        let secrets: HashSet<String> = [secret.clone()].into_iter().collect();
        let (masked, truncated) = mask_response_body(&body, &secrets);
        assert!(truncated);
        assert!(masked.len() <= RESPONSE_BODY_CAP_BYTES);
        assert!(!masked.contains("sk-live"), "no fragment of the secret may survive the cut");
    }

    #[test]
    fn history_limit_defaults_to_and_caps_at_ten() {
        assert_eq!(history_limit(0), HISTORY_LIMIT_MAX);
        assert_eq!(history_limit(3), 3);
        assert_eq!(history_limit(50), HISTORY_LIMIT_MAX);
    }

    #[test]
    fn bare_token_userinfo_is_masked() {
        assert_eq!(
            mask_url("https://ghp_abcdef123456@github.com/x"),
            format!("https://{REDACTED}@github.com/x")
        );
        assert_eq!(
            mask_url("https://{{token}}@github.com/x"),
            "https://{{token}}@github.com/x"
        );
    }

    #[test]
    fn credential_names_cover_keys_signatures_codes_and_encoded_names() {
        for name in ["access_key", "apiKey", "X-Amz-Signature", "sig", "code", "client%5Fsecret"] {
            assert_eq!(mask_named_value(name, "literal-value-1"), REDACTED, "{name}");
        }
        assert_eq!(
            mask_query_string("client%5Fsecret=cs-1234567&page=2"),
            format!("client%5Fsecret={REDACTED}&page=2")
        );
    }

    #[test]
    fn url_fragments_with_credentials_are_masked() {
        assert_eq!(
            mask_url("https://app.test/cb#access_token=abc123456&state=xyz"),
            format!("https://app.test/cb#access_token={REDACTED}&state=xyz")
        );
    }

    #[test]
    fn auth_url_fields_go_through_url_masking() {
        let mut auth = serde_json::json!({
            "authType": "oauth2",
            "accessTokenUrl": "https://u:pw-secret-1@idp.test/token?client_secret=cs-12345678",
            "clientId": "client-1",
        });
        mask_auth_value(&mut auth, None);
        let json = auth.to_string();
        assert!(!json.contains("pw-secret-1"), "{json}");
        assert!(!json.contains("cs-12345678"), "{json}");
        assert!(json.contains("idp.test/token"), "{json}");
    }

    #[test]
    fn literal_credentials_cover_headers_auth_query_and_userinfo() {
        let mut request = Request::new("R", HttpMethod::Get, "https://alice:pw-literal-1@api.test/x?token=tk-literal-2")
            .with_header("Authorization", "Bearer sk-literal-3")
            .with_auth(Auth::Bearer {
                token: "sk-literal-4".into(),
            });
        request.query_params.push(QueryParam {
            key: "api_key".into(),
            value: "qk-literal-5".into(),
            enabled: true,
            description: None,
        });
        let found = literal_credential_values(&request, None, &[]);
        for secret in [
            "pw-literal-1",
            "tk-literal-2",
            "sk-literal-3",
            "Bearer sk-literal-3",
            "sk-literal-4",
            "qk-literal-5",
        ] {
            assert!(found.contains(secret), "{secret} missing from {found:?}");
        }
        assert!(!found.contains("alice"));
    }

    #[test]
    fn basic_auth_header_values_and_folder_credentials_are_collected() {
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode("alice:hunter2-literal");
        let request = Request::new("R", HttpMethod::Get, "https://api.test/x").with_auth(Auth::Basic {
            username: "alice".into(),
            password: "hunter2-literal".into(),
        });
        let folder = FolderSettings {
            auth: Some(Auth::Bearer {
                token: "folder-bearer-1".into(),
            }),
            headers: vec![Header::new("X-Api-Key", "folder-key-22")],
            ..Default::default()
        };
        let found = literal_credential_values(&request, None, &[folder]);
        assert!(found.contains(&encoded));
        assert!(found.contains(&format!("Basic {encoded}")));
        assert!(found.contains("folder-bearer-1"));
        assert!(found.contains("folder-key-22"));
    }

    #[test]
    fn basic_auth_with_a_secret_variable_password_pairs_the_username_with_run_secrets() {
        use base64::Engine;
        let request = Request::new("R", HttpMethod::Get, "https://api.test/x").with_auth(Auth::Basic {
            username: "alice".into(),
            password: "{{pw}}".into(),
        });
        let secrets: HashSet<String> = ["s3cret-value".to_string()].into_iter().collect();
        let found = basic_header_values_from_secrets(&request, None, &[], &secrets);
        let encoded = base64::engine::general_purpose::STANDARD.encode("alice:s3cret-value");
        assert!(found.contains(&encoded));
    }

    #[test]
    fn credentials_in_auth_url_fields_are_collected() {
        let mut out = HashSet::new();
        let value = serde_json::json!({
            "authType": "oauth2",
            "accessTokenUrl": "https://idp.test/token?client_secret=cs-12345678",
        });
        collect_auth_url_credentials(&value, &mut out);
        assert!(out.contains("cs-12345678"));
    }
}

