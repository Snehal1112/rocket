//! gRPC protocol engine: parses `.proto` sources into descriptors. It holds no
//! network or file I/O; the concrete file reader lives in `rocket-infra`.

pub mod registry;

#[cfg(test)]
mod test_support;

pub use prost_reflect::MethodDescriptor;
pub use registry::{GrpcMethodInfo, GrpcServiceInfo, ProtoFileReader, ProtoLoader, ProtoRegistry};
