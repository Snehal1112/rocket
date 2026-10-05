use crate::scripting::ops::ScriptOpError;
use crate::scripting::state::ScriptInputState;
use deno_core::{op2, OpState};

fn get_response(state: &OpState) -> Result<rocket_http::HttpResponse, ScriptOpError> {
    state
        .borrow::<ScriptInputState>()
        .response
        .clone()
        .ok_or_else(|| ScriptOpError("res is not available in before-request scripts".into()))
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

/// Returns a JSON array of `{ key, value }` for every response header.
#[op2]
#[string]
pub fn op_res_get_header_list(state: &OpState) -> Result<String, ScriptOpError> {
    let items: Vec<serde_json::Value> = get_response(state)?
        .headers
        .iter()
        .map(|h| serde_json::json!({ "key": h.key, "value": h.value }))
        .collect();
    Ok(serde_json::to_string(&items).unwrap_or_else(|_| "[]".into()))
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
