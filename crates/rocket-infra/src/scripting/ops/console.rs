/// Stub — console ops implemented in SP3-05.
use deno_core::op2;

#[op2(fast)]
pub fn op_console_log(#[string] _msg: String) {}

#[op2(fast)]
pub fn op_console_warn(#[string] _msg: String) {}

#[op2(fast)]
pub fn op_console_error(#[string] _msg: String) {}
