mod auth;
mod body;
mod environment;
mod folder;
mod header;
mod param;
mod protocol;
mod request;
mod request_settings;
mod variables;
mod workspace;

#[cfg(test)]
mod tests;

// Re-export everything that was pub in oc_conversions.rs.
// #[allow(unused_imports)] because this is a pub(crate) module — the compiler
// cannot see external consumers but these are used throughout the crate.
#[allow(unused_imports)]
pub use auth::*;
#[allow(unused_imports)]
pub use body::*;
#[allow(unused_imports)]
pub use environment::*;
#[allow(unused_imports)]
pub use folder::{
    collection_to_oc_collection, folder_to_oc_folder,
    oc_collection_to_collection, oc_folder_to_folder,
};
#[allow(unused_imports)]
pub use header::*;
#[allow(unused_imports)]
pub use param::{merge_params, split_params};
#[allow(unused_imports)]
pub use protocol::{oc_item_to_protocol_request, protocol_request_to_oc_item, ProtocolRequest};
pub use request::{oc_http_request_to_request, request_to_oc_http_request};
#[allow(unused_imports)]
pub use variables::*;
#[allow(unused_imports)]
pub use workspace::*;
