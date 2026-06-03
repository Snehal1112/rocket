use deno_core::{op2, OpState};
use crate::scripting::state::{ScriptInputState, ScriptOutputState};
use rocket_scripting::{CollectionVarWrite, EnvVarWrite, NextRequest};

// ── Variable reads ────────────────────────────────────────────────────────────

/// rok.getVar(key) — reads from the runtime scope (highest priority).
#[op2]
#[string]
pub fn op_rok_get_var(state: &OpState, #[string] key: String) -> String {
    state.borrow::<ScriptInputState>().variables.runtime
        .get(&key).cloned().unwrap_or_default()
}

/// rok.getEnvVar(key) — reads from the active environment scope.
#[op2]
#[string]
pub fn op_rok_get_env_var(state: &OpState, #[string] key: String) -> String {
    state.borrow::<ScriptInputState>().variables.env
        .get(&key).cloned().unwrap_or_default()
}

/// rok.hasEnvVar(key) — true if key exists in the active environment.
#[op2(fast)]
pub fn op_rok_has_env_var(state: &OpState, #[string] key: String) -> bool {
    state.borrow::<ScriptInputState>().variables.env.contains_key(&key)
}

/// rok.getEnvName() — name of the active environment, or empty string if none.
#[op2]
#[string]
pub fn op_rok_get_env_name(state: &OpState) -> String {
    state.borrow::<ScriptInputState>().env_name.clone().unwrap_or_default()
}

/// rok.getCollectionVar(key) — reads from the collection variable scope.
#[op2]
#[string]
pub fn op_rok_get_collection_var(state: &OpState, #[string] key: String) -> String {
    state.borrow::<ScriptInputState>().variables.collection
        .get(&key).cloned().unwrap_or_default()
}

/// rok.getGlobalEnvVar(key) — reads from the global environment scope.
#[op2]
#[string]
pub fn op_rok_get_global_env_var(state: &OpState, #[string] key: String) -> String {
    state.borrow::<ScriptInputState>().variables.global_env
        .get(&key).cloned().unwrap_or_default()
}

/// rok.interpolate(template) — resolves {{var}} tokens using the flattened variable context.
#[op2]
#[string]
pub fn op_rok_interpolate(state: &OpState, #[string] template: String) -> String {
    let flat = state.borrow::<ScriptInputState>().variables.flatten();
    let mut result = template;
    for (key, value) in &flat {
        result = result.replace(&format!("{{{{{}}}}}", key), value);
    }
    result
}

// ── Variable writes ───────────────────────────────────────────────────────────

/// rok.setVar(key, jsonValue) — writes to runtime scope (in-memory only).
#[op2(fast)]
pub fn op_rok_set_var(state: &mut OpState, #[string] key: String, #[string] json_value: String) {
    let value = serde_json::from_str(&json_value).unwrap_or(serde_json::Value::Null);
    state.borrow_mut::<ScriptOutputState>().runtime_vars.insert(key, value);
}

/// rok.setEnvVar(key, jsonValue, persist) — writes to active environment.
#[op2(fast)]
pub fn op_rok_set_env_var(
    state: &mut OpState,
    #[string] key: String,
    #[string] json_value: String,
    persist: bool,
) {
    let value = serde_json::from_str(&json_value).unwrap_or(serde_json::Value::Null);
    state.borrow_mut::<ScriptOutputState>().env_var_writes.push(EnvVarWrite { key, value, persist });
}

/// rok.deleteEnvVar(key) — tombstone write (value=null) to active environment.
#[op2(fast)]
pub fn op_rok_delete_env_var(state: &mut OpState, #[string] key: String) {
    state.borrow_mut::<ScriptOutputState>().env_var_writes.push(EnvVarWrite {
        key,
        value: serde_json::Value::Null,
        persist: true,
    });
}

/// rok.setCollectionVar(key, jsonValue) — writes to collection variable scope.
#[op2(fast)]
pub fn op_rok_set_collection_var(
    state: &mut OpState,
    #[string] key: String,
    #[string] json_value: String,
) {
    let value = serde_json::from_str(&json_value).unwrap_or(serde_json::Value::Null);
    state.borrow_mut::<ScriptOutputState>().collection_var_writes.push(CollectionVarWrite { key, value });
}

/// rok.setGlobalEnvVar(key, jsonValue) — writes to global environment scope.
#[op2(fast)]
pub fn op_rok_set_global_env_var(
    state: &mut OpState,
    #[string] key: String,
    #[string] json_value: String,
) {
    let value = serde_json::from_str(&json_value).unwrap_or(serde_json::Value::Null);
    state.borrow_mut::<ScriptOutputState>().global_env_var_writes.push(EnvVarWrite {
        key,
        value,
        persist: false,
    });
}

// ── Runner ops ────────────────────────────────────────────────────────────────

/// rok.runner.setNextRequest(name | null) — controls flow in collection runner.
#[op2(fast)]
pub fn op_rok_set_next_request(state: &mut OpState, #[string] name: String) {
    let next = if name.is_empty() || name == "null" {
        NextRequest::Stop
    } else {
        NextRequest::Name(name)
    };
    state.borrow_mut::<ScriptOutputState>().next_request = Some(next);
}

/// rok.runner.skipRequest() — marks this request to be skipped in a runner.
#[op2(fast)]
pub fn op_rok_skip_request(state: &mut OpState) {
    state.borrow_mut::<ScriptOutputState>().skip_request = true;
}
