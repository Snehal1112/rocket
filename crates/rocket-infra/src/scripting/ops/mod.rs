pub mod console;
pub mod req;
pub mod res;
pub mod rok;

/// JS-visible error for op failures (phase guard, unavailable state, etc.).
#[derive(Debug, thiserror::Error, deno_error::JsError)]
#[class(type)]
#[error("{0}")]
pub struct ScriptOpError(pub String);
