/// Stub — req.* ops implemented in SP3-04.
use deno_core::op2;

#[op2]
#[string]
pub fn op_req_get_url() -> String {
    String::new()
}
