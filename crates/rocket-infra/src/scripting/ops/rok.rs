/// Stub — rok.* ops implemented in SP3-03.
use deno_core::op2;

#[op2]
#[string]
pub fn op_rok_get_var(#[string] _key: String) -> String {
    String::new()
}
