use std::path::PathBuf;

use deno_core::{op2, OpState};

use crate::scripting::local_modules::resolve_local_module;
use crate::scripting::ops::ScriptOpError;
use crate::scripting::state::ScriptInputState;

/// Backs `require()` for local `.js` files.
///
/// `from_dir` is the directory of the requiring file, or an empty string for the
/// top-level script. Returns a JSON object string `{ path, dir, source }` that
/// `bootstrap.js` parses. All root checks happen in `resolve_local_module`, so a
/// forged `from_dir` cannot widen access.
#[op2]
#[string]
pub fn op_require_local(
    state: &mut OpState,
    #[string] from_dir: String,
    #[string] name: String,
) -> Result<String, ScriptOpError> {
    let input = state.borrow::<ScriptInputState>();
    let Some(roots) = input.local_roots.as_ref() else {
        return Err(ScriptOpError(format!(
            "Cannot require '{name}': local file requires are not available in this script"
        )));
    };
    let from = if from_dir.is_empty() {
        roots.collection_root.clone()
    } else {
        PathBuf::from(from_dir)
    };
    let module = resolve_local_module(roots, &from, &name).map_err(ScriptOpError)?;
    Ok(serde_json::json!({
        "path": module.path.to_string_lossy(),
        "dir": module.dir.to_string_lossy(),
        "source": module.source,
    })
    .to_string())
}
