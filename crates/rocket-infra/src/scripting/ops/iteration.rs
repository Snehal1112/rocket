//! Read-only ops for the data-driven iteration (`rok.runner.*`).

use crate::scripting::state::ScriptInputState;
use deno_core::{op2, OpState};

/// Zero-based index of the current iteration, 0 outside a data-driven run.
#[op2(fast)]
pub fn op_rok_iteration_index(state: &OpState) -> u32 {
    state
        .borrow::<ScriptInputState>()
        .iteration
        .as_ref()
        .map_or(0, |i| i.index as u32)
}

/// Number of iterations in the run, 1 outside a data-driven run.
#[op2(fast)]
pub fn op_rok_iteration_total(state: &OpState) -> u32 {
    state
        .borrow::<ScriptInputState>()
        .iteration
        .as_ref()
        .map_or(1, |i| i.total as u32)
}

/// The current row as JSON object text, `{}` outside a data-driven run.
#[op2]
#[string]
pub fn op_rok_iteration_data(state: &OpState) -> String {
    match state.borrow::<ScriptInputState>().iteration.as_ref() {
        Some(i) => serde_json::to_string(&i.data).unwrap_or_else(|_| "{}".to_string()),
        None => "{}".to_string(),
    }
}
