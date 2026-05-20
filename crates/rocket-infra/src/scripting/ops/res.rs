/// Stub — res.* ops implemented in SP3-04.
use deno_core::op2;

#[op2(fast)]
pub fn op_res_get_status() -> u32 {
    0
}
