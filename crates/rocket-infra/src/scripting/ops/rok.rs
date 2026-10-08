use crate::scripting::ops::ScriptOpError;
use crate::scripting::state::{ScriptInputState, ScriptOutputState};
use deno_core::{op2, OpState};
use rocket_scripting::{CollectionVarWrite, EnvVarWrite, NextRequest, SandboxMode};
use std::collections::{BTreeSet, HashMap};

// ── Variable reads ────────────────────────────────────────────────────────────

/// rok.getVar(key) — reads from the runtime scope (highest priority).
#[op2]
#[string]
pub fn op_rok_get_var(state: &OpState, #[string] key: String) -> String {
    state
        .borrow::<ScriptInputState>()
        .variables
        .runtime
        .get(&key)
        .cloned()
        .unwrap_or_default()
}

/// rok.getEnvVar(key) — reads from the active environment scope.
#[op2]
#[string]
pub fn op_rok_get_env_var(state: &OpState, #[string] key: String) -> String {
    state
        .borrow::<ScriptInputState>()
        .variables
        .env
        .get(&key)
        .cloned()
        .unwrap_or_default()
}

/// rok.getSecretVar('alias.secretName') — reads a fetched External Secret value.
#[op2]
#[string]
pub fn op_rok_get_secret_var(state: &OpState, #[string] key: String) -> String {
    state
        .borrow::<ScriptInputState>()
        .variables
        .external_secrets
        .get(&key)
        .cloned()
        .unwrap_or_default()
}

/// rok.hasEnvVar(key) — true if key exists in the active environment.
#[op2(fast)]
pub fn op_rok_has_env_var(state: &OpState, #[string] key: String) -> bool {
    state
        .borrow::<ScriptInputState>()
        .variables
        .env
        .contains_key(&key)
}

/// rok.getEnvName() — name of the active environment, or empty string if none.
#[op2]
#[string]
pub fn op_rok_get_env_name(state: &OpState) -> String {
    state
        .borrow::<ScriptInputState>()
        .env_name
        .clone()
        .unwrap_or_default()
}

/// rok.getCollectionVar(key) — reads from the collection variable scope.
#[op2]
#[string]
pub fn op_rok_get_collection_var(state: &OpState, #[string] key: String) -> String {
    state
        .borrow::<ScriptInputState>()
        .variables
        .collection
        .get(&key)
        .cloned()
        .unwrap_or_default()
}

/// rok.getFolderVar(key) — reads the request's merged folder-chain scope.
/// The innermost folder wins and disabled entries are already left out.
#[op2]
#[string]
pub fn op_rok_get_folder_var(state: &OpState, #[string] key: String) -> String {
    state
        .borrow::<ScriptInputState>()
        .variables
        .folder
        .get(&key)
        .cloned()
        .unwrap_or_default()
}

/// rok.getGlobalEnvVar(key) — reads from the global environment scope.
#[op2]
#[string]
pub fn op_rok_get_global_env_var(state: &OpState, #[string] key: String) -> String {
    state
        .borrow::<ScriptInputState>()
        .variables
        .global_env
        .get(&key)
        .cloned()
        .unwrap_or_default()
}

fn scope_json(map: &HashMap<String, String>) -> String {
    serde_json::to_string(map).unwrap_or_else(|_| "{}".into())
}

/// rok.getAllEnvVars() — every variable of the active environment as a JSON object.
#[op2]
#[string]
pub fn op_rok_get_all_env_vars(state: &OpState) -> String {
    scope_json(&state.borrow::<ScriptInputState>().variables.env)
}

/// rok.getAllVars() — every runtime variable as a JSON object.
#[op2]
#[string]
pub fn op_rok_get_all_vars(state: &OpState) -> String {
    scope_json(&state.borrow::<ScriptInputState>().variables.runtime)
}

/// rok.getAllGlobalEnvVars() — every global environment variable as a JSON object.
#[op2]
#[string]
pub fn op_rok_get_all_global_env_vars(state: &OpState) -> String {
    scope_json(&state.borrow::<ScriptInputState>().variables.global_env)
}

/// rok.hasVar(key) — true if the runtime scope holds key.
#[op2(fast)]
pub fn op_rok_has_var(state: &OpState, #[string] key: String) -> bool {
    state
        .borrow::<ScriptInputState>()
        .variables
        .runtime
        .contains_key(&key)
}

/// rok.hasGlobalEnvVar(key) — true if the global environment holds key.
#[op2(fast)]
pub fn op_rok_has_global_env_var(state: &OpState, #[string] key: String) -> bool {
    state
        .borrow::<ScriptInputState>()
        .variables
        .global_env
        .contains_key(&key)
}

/// rok.hasCollectionVar(key) — true if the collection scope holds key.
#[op2(fast)]
pub fn op_rok_has_collection_var(state: &OpState, #[string] key: String) -> bool {
    state
        .borrow::<ScriptInputState>()
        .variables
        .collection
        .contains_key(&key)
}

/// rok.getRequestVar(key) — reads from the request variable scope.
#[op2]
#[string]
pub fn op_rok_get_request_var(state: &OpState, #[string] key: String) -> String {
    state
        .borrow::<ScriptInputState>()
        .variables
        .request
        .get(&key)
        .cloned()
        .unwrap_or_default()
}

/// True if the host environment snapshot holds key.
#[op2(fast)]
pub fn op_rok_has_process_env(state: &OpState, #[string] key: String) -> bool {
    state
        .borrow::<ScriptInputState>()
        .variables
        .process_env
        .contains_key(&key)
}

/// rok.getProcessEnv(key) — reads the host environment snapshot.
#[op2]
#[string]
pub fn op_rok_get_process_env(state: &OpState, #[string] key: String) -> String {
    state
        .borrow::<ScriptInputState>()
        .variables
        .process_env
        .get(&key)
        .cloned()
        .unwrap_or_default()
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
    let out = state.borrow_mut::<ScriptOutputState>();
    out.runtime_var_deletes.retain(|k| k != &key);
    out.runtime_vars.insert(key, value);
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
    state
        .borrow_mut::<ScriptOutputState>()
        .env_var_writes
        .push(EnvVarWrite {
            key,
            value,
            persist,
        });
}

/// rok.deleteEnvVar(key) — tombstone write (value=null) to active environment.
#[op2(fast)]
pub fn op_rok_delete_env_var(state: &mut OpState, #[string] key: String) {
    state
        .borrow_mut::<ScriptOutputState>()
        .env_var_writes
        .push(EnvVarWrite {
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
    state
        .borrow_mut::<ScriptOutputState>()
        .collection_var_writes
        .push(CollectionVarWrite { key, value });
}

/// rok.setGlobalEnvVar(key, jsonValue) — writes to global environment scope.
#[op2(fast)]
pub fn op_rok_set_global_env_var(
    state: &mut OpState,
    #[string] key: String,
    #[string] json_value: String,
) {
    let value = serde_json::from_str(&json_value).unwrap_or(serde_json::Value::Null);
    state
        .borrow_mut::<ScriptOutputState>()
        .global_env_var_writes
        .push(EnvVarWrite {
            key,
            value,
            persist: false,
        });
}

/// Keys of a snapshot scope plus keys a script already wrote, in stable order.
fn scope_keys<'a>(
    snapshot: &HashMap<String, String>,
    written: impl Iterator<Item = &'a String>,
) -> BTreeSet<String> {
    snapshot.keys().cloned().chain(written.cloned()).collect()
}

/// rok.deleteVar(key) — removes a runtime variable.
#[op2(fast)]
pub fn op_rok_delete_var(state: &mut OpState, #[string] key: String) {
    let out = state.borrow_mut::<ScriptOutputState>();
    out.runtime_vars.remove(&key);
    out.runtime_var_deletes.push(key);
}

/// rok.deleteAllVars() — removes every runtime variable.
#[op2(fast)]
pub fn op_rok_delete_all_vars(state: &mut OpState) {
    let snapshot: Vec<String> = state
        .borrow::<ScriptInputState>()
        .variables
        .runtime
        .keys()
        .cloned()
        .collect();
    let out = state.borrow_mut::<ScriptOutputState>();
    let written: Vec<String> = out.runtime_vars.keys().cloned().collect();
    out.runtime_vars.clear();
    out.runtime_var_deletes.extend(snapshot);
    out.runtime_var_deletes.extend(written);
}

/// rok.deleteAllEnvVars() — null-writes every key of the active environment.
#[op2(fast)]
pub fn op_rok_delete_all_env_vars(state: &mut OpState) {
    let snapshot = state.borrow::<ScriptInputState>().variables.env.clone();
    let out = state.borrow_mut::<ScriptOutputState>();
    let keys = scope_keys(&snapshot, out.env_var_writes.iter().map(|w| &w.key));
    for key in keys {
        out.env_var_writes.push(EnvVarWrite {
            key,
            value: serde_json::Value::Null,
            persist: true,
        });
    }
}

/// rok.deleteCollectionVar(key) — null-writes one collection variable.
#[op2(fast)]
pub fn op_rok_delete_collection_var(state: &mut OpState, #[string] key: String) {
    state
        .borrow_mut::<ScriptOutputState>()
        .collection_var_writes
        .push(CollectionVarWrite {
            key,
            value: serde_json::Value::Null,
        });
}

/// rok.deleteAllCollectionVars() — null-writes every collection variable.
#[op2(fast)]
pub fn op_rok_delete_all_collection_vars(state: &mut OpState) {
    let snapshot = state.borrow::<ScriptInputState>().variables.collection.clone();
    let out = state.borrow_mut::<ScriptOutputState>();
    let keys = scope_keys(&snapshot, out.collection_var_writes.iter().map(|w| &w.key));
    for key in keys {
        out.collection_var_writes.push(CollectionVarWrite {
            key,
            value: serde_json::Value::Null,
        });
    }
}

/// rok.deleteGlobalEnvVar(key) — null-writes one global environment variable.
#[op2(fast)]
pub fn op_rok_delete_global_env_var(state: &mut OpState, #[string] key: String) {
    state
        .borrow_mut::<ScriptOutputState>()
        .global_env_var_writes
        .push(EnvVarWrite {
            key,
            value: serde_json::Value::Null,
            persist: true,
        });
}

/// rok.deleteAllGlobalEnvVars() — null-writes every global environment variable.
#[op2(fast)]
pub fn op_rok_delete_all_global_env_vars(state: &mut OpState) {
    let snapshot = state.borrow::<ScriptInputState>().variables.global_env.clone();
    let out = state.borrow_mut::<ScriptOutputState>();
    let keys = scope_keys(&snapshot, out.global_env_var_writes.iter().map(|w| &w.key));
    for key in keys {
        out.global_env_var_writes.push(EnvVarWrite {
            key,
            value: serde_json::Value::Null,
            persist: true,
        });
    }
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

/// rok.getCollectionName() — display name of the collection, or empty string.
#[op2]
#[string]
pub fn op_rok_get_collection_name(state: &OpState) -> String {
    state.borrow::<ScriptInputState>().collection_name.clone()
}

/// rok.isSafeMode() — true in Safe mode, false in Developer mode.
#[op2(fast)]
pub fn op_rok_is_safe_mode(state: &OpState) -> bool {
    state.borrow::<ScriptInputState>().sandbox_mode == SandboxMode::Safe
}

/// rok.cwd() — absolute collection directory. Developer mode only.
#[op2]
#[string]
pub fn op_rok_cwd(state: &OpState) -> Result<String, ScriptOpError> {
    let input = state.borrow::<ScriptInputState>();
    if input.sandbox_mode == SandboxMode::Safe {
        return Err(ScriptOpError("rok.cwd() requires Developer mode".into()));
    }
    input
        .collection_root
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or_else(|| ScriptOpError("rok.cwd() has no collection directory here".into()))
}
