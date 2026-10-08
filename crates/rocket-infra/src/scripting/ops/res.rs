use crate::scripting::ops::ScriptOpError;
use crate::scripting::state::{ScriptInputState, ScriptOutputState};
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

/// The request URL. The final redirect URL is not tracked, so this is the best known value.
#[op2]
#[string]
pub fn op_res_get_url(state: &OpState) -> Result<String, ScriptOpError> {
    get_response(state)?;
    Ok(state.borrow::<ScriptInputState>().request.url.clone())
}

/// Returns `{ body, headers, total }` in bytes as JSON.
#[op2]
#[string]
pub fn op_res_get_size(state: &OpState) -> Result<String, ScriptOpError> {
    let response = get_response(state)?;
    // Each header line is "key: value\r\n", which adds four bytes.
    let headers: usize = response
        .headers
        .iter()
        .map(|h| h.key.len() + h.value.len() + 4)
        .sum();
    let body = response.size_bytes;
    Ok(
        serde_json::json!({ "body": body, "headers": headers, "total": body + headers })
            .to_string(),
    )
}

/// res.setBody(body) - replaces the body later scripts see. Takes the raw text.
#[op2(fast)]
pub fn op_res_set_body(state: &mut OpState, #[string] body: String) -> Result<(), ScriptOpError> {
    get_response(state)?;
    state.borrow_mut::<ScriptOutputState>().response_body = Some(body);
    Ok(())
}
