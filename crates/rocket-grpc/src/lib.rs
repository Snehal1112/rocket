// The async_trait macro triggers this lint on newer toolchains.
#![allow(clippy::double_must_use)]
//! gRPC protocol engine: parses `.proto` sources into descriptors and converts
//! JSON to and from protobuf at runtime. It holds no network or file I/O; the
//! concrete transport and file reader live in `rocket-infra`.

pub mod call;
pub mod codec;
pub mod events;
pub mod limits;
pub mod registry;
pub mod target;

#[cfg(test)]
mod test_support;

pub use call::{grpc_code_name, GrpcCall, GrpcExecutor, GrpcStatus, GrpcUnaryResponse};
pub use call::{GrpcStreamEvent, GrpcStreamHandle};
pub use codec::{empty_message_json, json_to_message, message_to_json};
pub use events::{GrpcEvent, GrpcMetadataPair};
pub use limits::{
    clamp_call_timeout, DEFAULT_CALL_TIMEOUT, MAX_CALL_TIMEOUT, MAX_INPUT_MESSAGES,
    MAX_RESPONSE_BYTES, MAX_RESPONSE_MESSAGES,
};
pub use prost_reflect::{DynamicMessage, MessageDescriptor, MethodDescriptor};
pub use registry::{GrpcMethodInfo, GrpcServiceInfo, ProtoFileReader, ProtoLoader, ProtoRegistry};
pub use target::{parse_target, Target};
