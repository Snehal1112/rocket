use deno_core::{op2, OpState};
use crate::scripting::state::ScriptInputState;
use crate::scripting::ops::ScriptOpError;

fn get_response(state: &OpState) -> Result<rocket_http::HttpResponse, ScriptOpError> {
    state
        .borrow::<ScriptInputState>()
        .response
        .clone()
        .ok_or_else(|| ScriptOpError(
            "res is not available in before-request scripts".into()
        ))
}

#[op2(fast)]
pub fn op_res_get_status(state: &OpState) -> Result<u32, ScriptOpError> {
    Ok(get_response(state)?.status as u32)
}

#[op2]
#[string]
pub fn op_res_get_status_text(state: &OpState) -> Result<String, ScriptOpError> {
    Ok(get_response(state)?.status_text.clone())
}

#[op2]
#[string]
pub fn op_res_get_header(
    state: &OpState,
    #[string] name: String,
) -> Result<String, ScriptOpError> {
    let res = get_response(state)?;
    Ok(res
        .headers
        .iter()
        .find(|h| h.key.eq_ignore_ascii_case(&name))
        .map(|h| h.value.clone())
        .unwrap_or_default())
}

/// Returns JSON object of all headers as { key: value }.
#[op2]
#[string]
pub fn op_res_get_headers(state: &OpState) -> Result<String, ScriptOpError> {
    let res = get_response(state)?;
    let map: serde_json::Map<String, serde_json::Value> = res
        .headers
        .iter()
        .map(|h| (h.key.clone(), serde_json::Value::String(h.value.clone())))
        .collect();
    Ok(serde_json::to_string(&map).unwrap_or_else(|_| "{}".into()))
}

#[op2]
#[string]
pub fn op_res_get_body(state: &OpState) -> Result<String, ScriptOpError> {
    Ok(get_response(state)?.body.clone())
}

#[op2(fast)]
pub fn op_res_get_response_time(state: &OpState) -> Result<u32, ScriptOpError> {
    Ok(get_response(state)?.duration_ms as u32)
}
