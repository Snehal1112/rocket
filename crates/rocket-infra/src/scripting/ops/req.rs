use deno_core::{op2, OpState};
use url::Url;
use crate::scripting::state::{ScriptInputState, ScriptOutputState};
use crate::scripting::ops::ScriptOpError;
use rocket_scripting::ScriptPhase;

fn guard_before_request(state: &OpState) -> Result<(), ScriptOpError> {
    let phase = &state.borrow::<ScriptInputState>().phase;
    if *phase != ScriptPhase::BeforeRequest {
        Err(ScriptOpError(format!(
            "req mutations are not allowed in {} scripts",
            phase
        )))
    } else {
        Ok(())
    }
}

// ── req read ops ──────────────────────────────────────────────────────────────

#[op2]
#[string]
pub fn op_req_get_url(state: &OpState) -> String {
    state.borrow::<ScriptInputState>().request.url.clone()
}

#[op2]
#[string]
pub fn op_req_get_host(state: &OpState) -> String {
    let url = &state.borrow::<ScriptInputState>().request.url;
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(String::from))
        .unwrap_or_default()
}

#[op2]
#[string]
pub fn op_req_get_path(state: &OpState) -> String {
    let url = &state.borrow::<ScriptInputState>().request.url;
    Url::parse(url)
        .map(|u| u.path().to_string())
        .unwrap_or_default()
}

#[op2]
#[string]
pub fn op_req_get_query_string(state: &OpState) -> String {
    let url = &state.borrow::<ScriptInputState>().request.url;
    Url::parse(url)
        .ok()
        .and_then(|u| u.query().map(String::from))
        .unwrap_or_default()
}

#[op2]
#[string]
pub fn op_req_get_method(state: &OpState) -> String {
    state
        .borrow::<ScriptInputState>()
        .request
        .method
        .to_string()
}

#[op2]
#[string]
pub fn op_req_get_auth_mode(state: &OpState) -> String {
    use rocket_shared::types::Auth;
    match &state.borrow::<ScriptInputState>().request.auth {
        Auth::None => "none",
        Auth::Basic { .. } => "basic",
        Auth::Bearer { .. } => "bearer",
        Auth::ApiKey { .. } => "api-key",
        Auth::OAuth2(_) => "oauth2",
        Auth::AwsSigV4 { .. } => "aws-sig-v4",
        _ => "inherit",
    }
    .to_string()
}

#[op2]
#[string]
pub fn op_req_get_header(state: &OpState, #[string] name: String) -> String {
    state
        .borrow::<ScriptInputState>()
        .request
        .headers
        .iter()
        .find(|h| h.key.eq_ignore_ascii_case(&name))
        .map(|h| h.value.clone())
        .unwrap_or_default()
}

/// Returns JSON object of all headers as { key: value }.
#[op2]
#[string]
pub fn op_req_get_headers(state: &OpState) -> String {
    let headers: serde_json::Map<String, serde_json::Value> = state
        .borrow::<ScriptInputState>()
        .request
        .headers
        .iter()
        .map(|h| (h.key.clone(), serde_json::Value::String(h.value.clone())))
        .collect();
    serde_json::to_string(&headers).unwrap_or_else(|_| "{}".into())
}

/// Returns request body content as string, or empty if none.
#[op2]
#[string]
pub fn op_req_get_body(state: &OpState) -> String {
    state
        .borrow::<ScriptInputState>()
        .request
        .body
        .as_ref()
        .and_then(|b| b.content.clone())
        .unwrap_or_default()
}

#[op2(fast)]
pub fn op_req_get_timeout(state: &OpState) -> u32 {
    state
        .borrow::<ScriptInputState>()
        .request
        .options
        .timeout_ms as u32
}

#[op2]
#[string]
pub fn op_req_get_execution_mode(state: &OpState) -> String {
    state.borrow::<ScriptInputState>().execution_mode.clone()
}

#[op2]
#[string]
pub fn op_req_get_execution_platform(state: &OpState) -> String {
    state
        .borrow::<ScriptInputState>()
        .execution_platform
        .clone()
}

// ── req write ops (BeforeRequest only) ───────────────────────────────────────

#[op2(fast)]
pub fn op_req_set_url(
    state: &mut OpState,
    #[string] url: String,
) -> Result<(), ScriptOpError> {
    guard_before_request(state)?;
    let out = state.borrow_mut::<ScriptOutputState>();
    out.request_mutations.url = Some(url);
    out.any_request_mutation = true;
    Ok(())
}

#[op2(fast)]
pub fn op_req_set_method(
    state: &mut OpState,
    #[string] method: String,
) -> Result<(), ScriptOpError> {
    guard_before_request(state)?;
    let out = state.borrow_mut::<ScriptOutputState>();
    out.request_mutations.method = Some(method);
    out.any_request_mutation = true;
    Ok(())
}

#[op2(fast)]
pub fn op_req_set_header(
    state: &mut OpState,
    #[string] name: String,
    #[string] value: String,
) -> Result<(), ScriptOpError> {
    guard_before_request(state)?;
    let out = state.borrow_mut::<ScriptOutputState>();
    out.request_mutations.headers_set.insert(name, value);
    out.any_request_mutation = true;
    Ok(())
}

/// Accepts JSON object of { name: value } pairs.
#[op2(fast)]
pub fn op_req_set_headers(
    state: &mut OpState,
    #[string] headers_json: String,
) -> Result<(), ScriptOpError> {
    guard_before_request(state)?;
    let map: std::collections::HashMap<String, String> =
        serde_json::from_str(&headers_json).unwrap_or_default();
    let out = state.borrow_mut::<ScriptOutputState>();
    out.request_mutations.headers_set.extend(map);
    out.any_request_mutation = true;
    Ok(())
}

#[op2(fast)]
pub fn op_req_delete_header(
    state: &mut OpState,
    #[string] name: String,
) -> Result<(), ScriptOpError> {
    guard_before_request(state)?;
    let out = state.borrow_mut::<ScriptOutputState>();
    out.request_mutations.headers_deleted.push(name);
    out.any_request_mutation = true;
    Ok(())
}

/// Accepts JSON array of header name strings.
#[op2(fast)]
pub fn op_req_delete_headers(
    state: &mut OpState,
    #[string] names_json: String,
) -> Result<(), ScriptOpError> {
    guard_before_request(state)?;
    let names: Vec<String> = serde_json::from_str(&names_json).unwrap_or_default();
    let out = state.borrow_mut::<ScriptOutputState>();
    out.request_mutations.headers_deleted.extend(names);
    out.any_request_mutation = true;
    Ok(())
}

#[op2(fast)]
pub fn op_req_set_body(
    state: &mut OpState,
    #[string] body_json: String,
) -> Result<(), ScriptOpError> {
    guard_before_request(state)?;
    let value: serde_json::Value =
        serde_json::from_str(&body_json).unwrap_or(serde_json::Value::Null);
    let out = state.borrow_mut::<ScriptOutputState>();
    out.request_mutations.body = Some(value);
    out.any_request_mutation = true;
    Ok(())
}

#[op2(fast)]
pub fn op_req_set_timeout(state: &mut OpState, ms: u32) -> Result<(), ScriptOpError> {
    guard_before_request(state)?;
    let out = state.borrow_mut::<ScriptOutputState>();
    out.request_mutations.timeout_ms = Some(ms as u64);
    out.any_request_mutation = true;
    Ok(())
}

#[op2(fast)]
pub fn op_req_set_max_redirects(state: &mut OpState, n: u32) -> Result<(), ScriptOpError> {
    guard_before_request(state)?;
    let out = state.borrow_mut::<ScriptOutputState>();
    out.request_mutations.max_redirects = Some(n);
    out.any_request_mutation = true;
    Ok(())
}
