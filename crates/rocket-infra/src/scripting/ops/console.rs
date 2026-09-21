use crate::scripting::ops::redact;
use crate::scripting::state::ScriptOutputState;
/// Console ops — capture log/warn/error into ScriptOutputState, redacting
/// any known secret values first (see ops/mod.rs::redact).
use deno_core::{op2, OpState};
use rocket_scripting::ConsoleLevel;

#[op2(fast)]
pub fn op_console_log(state: &mut OpState, #[string] msg: String) {
    let redacted = redact(state, msg);
    state
        .borrow_mut::<ScriptOutputState>()
        .add_console(ConsoleLevel::Log, redacted);
}

#[op2(fast)]
pub fn op_console_warn(state: &mut OpState, #[string] msg: String) {
    let redacted = redact(state, msg);
    state
        .borrow_mut::<ScriptOutputState>()
        .add_console(ConsoleLevel::Warn, redacted);
}

#[op2(fast)]
pub fn op_console_error(state: &mut OpState, #[string] msg: String) {
    let redacted = redact(state, msg);
    state
        .borrow_mut::<ScriptOutputState>()
        .add_console(ConsoleLevel::Error, redacted);
}
