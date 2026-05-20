/// Console ops — capture log/warn/error into ScriptOutputState.
use deno_core::{op2, OpState};
use rocket_scripting::ConsoleLevel;
use crate::scripting::state::ScriptOutputState;

#[op2(fast)]
pub fn op_console_log(state: &mut OpState, #[string] msg: String) {
    state.borrow_mut::<ScriptOutputState>().add_console(ConsoleLevel::Log, msg);
}

#[op2(fast)]
pub fn op_console_warn(state: &mut OpState, #[string] msg: String) {
    state.borrow_mut::<ScriptOutputState>().add_console(ConsoleLevel::Warn, msg);
}

#[op2(fast)]
pub fn op_console_error(state: &mut OpState, #[string] msg: String) {
    state.borrow_mut::<ScriptOutputState>().add_console(ConsoleLevel::Error, msg);
}
