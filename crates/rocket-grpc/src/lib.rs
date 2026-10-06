//! gRPC protocol engine: parses `.proto` sources into descriptors and converts
//! JSON to and from protobuf at runtime. It holds no network or file I/O; the
//! concrete transport and file reader live in `rocket-infra`.

pub mod codec;
pub mod registry;

#[cfg(test)]
mod test_support;

pub use codec::{empty_message_json, json_to_message, message_to_json};
pub use prost_reflect::{DynamicMessage, MessageDescriptor, MethodDescriptor};
pub use registry::{GrpcMethodInfo, GrpcServiceInfo, ProtoFileReader, ProtoLoader, ProtoRegistry};
