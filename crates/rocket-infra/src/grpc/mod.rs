//! gRPC transport and `.proto` file access.

mod channel;
mod codec;
mod executor;
mod proto_reader;
mod reflection;

#[cfg(test)]
mod test_server;

pub use executor::TonicGrpcExecutor;
pub use proto_reader::{FsProtoFileReader, FsProtoLoader};
