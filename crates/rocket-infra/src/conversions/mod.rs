mod auth;
mod body;
mod environment;
mod folder;
mod folder_settings;
mod grpc;
mod graphql;
mod header;
mod param;
mod request;
mod request_settings;
mod variables;
mod websocket;
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
pub use grpc::{derived_grpc_uid, grpc_to_oc, oc_grpc_to_domain};
#[allow(unused_imports)]
pub use folder::{
    collection_to_oc_collection, folder_to_oc_folder, oc_collection_to_collection,
    oc_folder_to_folder, oc_item_to_collection_item,
};
#[allow(unused_imports)]
pub use folder_settings::{apply_folder_settings, oc_folder_to_folder_settings};
#[allow(unused_imports)]
pub use graphql::{graphql_to_oc, oc_graphql_to_domain};
#[allow(unused_imports)]
pub use header::*;
#[allow(unused_imports)]
pub use param::{merge_params, split_params};
pub use request::{oc_http_request_to_request, request_to_oc_http_request};
#[allow(unused_imports)]
pub use variables::*;
#[allow(unused_imports)]
pub use websocket::{
    derived_websocket_uid, oc_websocket_to_request, websocket_to_oc_websocket, with_file_identity,
};
#[allow(unused_imports)]
pub use workspace::*;
