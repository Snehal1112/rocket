# CLAUDE.md

## What This Is

`rocket-grpc` is the gRPC protocol engine. It parses `.proto` source into descriptors and converts protobuf JSON to and from messages at runtime, with no generated code. It does no file or network I/O.

## Commands

```bash
cargo check -j4 -p rocket-grpc
cargo test -j4 -p rocket-grpc <test_name>
```

## Layout

| File | Role |
|---|---|
| `registry.rs` | `ProtoRegistry` (descriptor pool, services, methods), `ProtoFileReader`, `ProtoLoader`, the `GrpcServiceInfo` and `GrpcMethodInfo` view types. |
| `codec.rs` | `json_to_message`, `message_to_json`, `empty_message_json`. |
| `call.rs` | `GrpcCall`, `GrpcExecutor` (`unary`, `open_stream` and `reflect`), `GrpcStatus`, `GrpcUnaryResponse`, `GrpcStreamEvent` and `GrpcStreamHandle`. |

## Rules

- No file or network access here. A `.proto` is read through `ProtoFileReader`. The filesystem implementation (`FsProtoFileReader`, `FsProtoLoader`) and the transport (`TonicGrpcExecutor`) live in `rocket-infra`.
- Errors are `DomainError::InvalidInput` for anything the user typed (bad proto, bad JSON), `NotFound` for an unknown method.
- `GrpcServiceInfo` and `GrpcMethodInfo` are IPC view types with camelCase serde. They are never persisted.
- Tests use the in-memory `MemReader` and the greeter proto in `test_support.rs`. The same proto is on disk in `crates/rocket-infra/test-fixtures/grpc` for the transport tests.
