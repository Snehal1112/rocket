# Protocol parity, Plan 11: gRPC model, persistence and the proto engine

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** gRPC becomes a first-class, typed collection item that loads, saves, renames and shows in the sidebar payload, and Rocket gets the engine that a gRPC client needs before it can send anything: a `.proto` parser that lists services and methods with their call shape, and a JSON to protobuf codec that works on descriptors loaded at runtime. Nothing in this plan touches the network. Plan 12 sends calls, Plan 13 adds the UI and the Bruno import.

**Architecture:** `rocket-collection` gains `GrpcRequest` and `CollectionItem::Grpc`, plus defaulted `get_grpc_request` and `save_grpc_request` repository methods that sit next to Plan 05's `request_kind`. `rocket-infra` converts `GrpcRequest` to and from the existing `OcGrpcRequest` YAML shape and stops loading gRPC files as opaque items. A new crate `rocket-grpc` holds the pure protocol logic (`ProtoRegistry`, `ProtoFileReader`, `ProtoLoader`, the JSON codec) and does no file or network I/O. The concrete file reader and loader live in `rocket-infra`, which is the only crate that reads files. Persistence stays OpenCollection YAML.

**Tech Stack:** Rust, serde, serde_yaml, `protox` and `prost-reflect` (new), React + TypeScript for the minimal type and guard changes. Run Rust tests with `cargo test -j4 -p <crate> <name>`.

**Spec:** `docs/superpowers/specs/opencollection-spec-reference.md`, section 2.5 `GrpcRequest` and `GrpcMetadata`, with the correction in Task 1 Step 16. Behaviour reference for the whole series: https://docs.usebruno.com/send-requests/grpc/overview (method picker, proto file or server reflection, unary and the three streaming types, message list, metadata, streamed response).

**Depends on:** Plan 05 Task 1. It adds `RequestKind` (already holding `Grpc`), `RequestSummary.kind`, `CollectionRepository::request_kind` and the kind-aware `rename_request`, `update_request_docs` and request-variables code that this plan extends. Plan 05 Task 3 is not required, but if it has landed, Task 1 Step 15 edits the lines it changed. Plan 08 (WebSocket) makes the same kind of change as Plan 05 and Task 1 here in the same files (`folder.rs`, `repository.rs`, `tree.rs`, `schema_shape_tests.rs`, `fs_collection/tests.rs`, `conversions/folder.rs`, `collection_service.rs`). Whichever plan lands second resolves the textual merge by keeping both sides; the logic does not collide. Rebase carefully.

## Crate choices (checked against this repo's lockfile on 2026-10-05)

| Crate | Version | Why |
|---|---|---|
| `protox` | 0.9.1 | Compiles `.proto` text to descriptors in pure Rust. No `protoc` binary has to ship with a desktop app. Its `FileResolver` trait lets the engine read imports through a `ProtoFileReader`, so the pure crate does no file I/O. |
| `prost-reflect` | 0.16.5, feature `serde` | `DynamicMessage` plus the protobuf JSON mapping (enums by name, 64-bit integers as strings, `oneof`, `map`, well-known types such as `Timestamp`). It is what turns the editor's JSON into bytes without generated code. |
| `prost-types` | 0.14 | `FileDescriptorProto`, which reflection returns. |
| `tonic` | 0.14.6 (Plan 12) | Transport. Needs Rust 1.88, the repo builds with 1.94. |
| `tonic-reflection` | 0.14.6, no default features (Plan 12) | Only the generated reflection client messages are used. The `server` feature is a dev-dependency for tests. |

Compatibility facts, verified by resolving a copy of the real `Cargo.lock` with every dependency of Plans 11 to 13 added: `tokio 1.50`, `hyper 1.8`, `h2 0.4.13`, `tower 0.5.3`, `rustls 0.23.37` and `tokio-rustls 0.26.4` are already locked and are reused, and no existing locked package changes version. The lockfile only gains packages: `tonic`, `tonic-prost`, `tonic-reflection`, `prost`, `prost-derive`, `prost-types`, `prost-reflect`, `protox`, `protox-parse`, `tokio-stream`, `hyper-timeout`, the parser helpers `logos*`, `miette*`, `beef`, `ordered-float`, `serde-value`, `unicode-width`, and for tests `axum`, `axum-core`, `matchit` (tonic's `router` feature) and `rcgen`, `pem`, `yasna`. After adding the dependencies run `git diff Cargo.lock` and confirm it only adds packages.

`reqwest` in this repo uses `native-tls`, and tonic has no `native-tls` option, so gRPC TLS uses rustls with the `ring` provider (`tls-ring`) and the operating system's root certificates (`tls-native-roots`, which reads the OS trust store through `rustls-native-certs`). A corporate root installed in the OS is trusted by both stacks. Plan 12 states the TLS limits.

## Global Constraints

- Never apply `#[serde(rename_all = "camelCase")]` to the `Oc*` persistence structs in `crates/rocket-infra/src/oc/`. The domain `GrpcRequest` carries camelCase exactly like `Request` and Plan 05's `GraphQlRequest` do, because domain JSON is the IPC JSON in this codebase. The listing types `GrpcMethodInfo` and `GrpcServiceInfo` in `rocket-grpc` are IPC-only view types that are never persisted, so they carry camelCase too.
- Every new persisted field is optional and skipped when empty, so an older build still reads the file. The OpenCollection schema is `additionalProperties: false`. The only key this plan adds on disk that the schema does not list is the top-level `uid`, the same deviation HTTP requests have. It is added to `KNOWN_DEFERRED` as `GrpcRequest.uid`.
- The schema keeps gRPC auth in the `grpc` block and allows only `variables`, `scripts` and `assertions` in `runtime` (checked against https://schema.opencollection.com/opencollection/v1.0.0.json on 2026-10-05). Never write `runtime.auth` for gRPC.
- Never `unwrap()` in production paths. Tests may use `.expect("reason")`.
- Always pass `-j4` to cargo. Never run `cargo test --workspace`.
- Commit with the `dev-workflow-skills:1-git-commit` skill, conventional subjects, and stage by explicit path only. Peer sessions share this repo's git index.
- Frontend: shadcn/ui primitives and `lucide-react` only. Zustand: narrow selectors only.
- Only `rocket-infra` reads files. `rocket-grpc` takes a `ProtoFileReader`.

## Review Focus

1. An opaque-era gRPC file (no `uid`, `methodType`, one message string) must load, keep its whole call description through a save, and not be rewritten by a read (Task 1 tests `get_grpc_request_gives_a_uid_less_file_an_in_memory_uid` and `an_opaque_era_grpc_file_keeps_its_call_description_through_load_and_save`).
2. gRPC auth must be written inside the `grpc` block and never in `runtime`, because the schema rejects `runtime.auth` for gRPC. The spec reference doc says the opposite, so it is corrected in this plan (Task 1 tests `auth_is_written_in_the_grpc_block_never_in_runtime` and `saved_grpc_request_only_uses_schema_keys_besides_deferred`).
3. One untitled message is stored as a plain string, several or titled ones as variants with their `selected` flag, so a Bruno or hand-written file round-trips (Task 1 tests `a_single_untitled_message_is_written_as_a_string_and_titled_ones_as_variants` and `a_single_untitled_message_is_saved_as_a_plain_string`).
4. Rename, docs and request variables must not fall into HTTP parsing for a gRPC file. Before this plan they would fail or, worse, rewrite the file as HTTP (Task 1 tests `request_variables_work_for_a_grpc_file` and `rename_request_keeps_a_grpc_item_grpc`).
5. Descriptor and codec edge inputs: an import name that climbs out of its include directory must not be read (Task 2 tests `imports_cannot_escape_the_include_directories` and `a_symlink_that_leaves_the_include_directory_is_not_followed`), descriptors that arrive in any order must still build a registry (Task 2 test `descriptors_in_reverse_order_still_build_a_registry`), a 64-bit integer above 2^53 must keep every digit and two members of one `oneof` must be rejected (Task 3 tests `sixty_four_bit_integers_keep_every_digit` and `two_members_of_one_oneof_are_rejected`).

---

## Task 1: `GrpcRequest`, `CollectionItem::Grpc` and infra persistence

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

This task is a delta on Plan 05 Task 1. Apply Plan 05 Task 1 first. Where this task says "Plan 05's code", it means the code that plan's steps write. The files below are the ones both plans touch.

**Files:**
- Create: `crates/rocket-collection/src/grpc_request.rs`
- Modify: `crates/rocket-collection/src/lib.rs`, `folder.rs`, `repository.rs`
- Modify: `crates/rocket-infra/src/oc/grpc.rs`
- Create: `crates/rocket-infra/src/conversions/grpc.rs`
- Modify: `crates/rocket-infra/src/conversions/mod.rs`, `folder.rs`, `tests.rs`
- Modify: `crates/rocket-infra/src/fs_collection/mod.rs`, `requests.rs`, `tree.rs`, `variables.rs`, `tests.rs`, `schema_shape_tests.rs`
- Modify: `crates/rocket-infra/src/shared_path_collection_repo.rs`
- Modify: `crates/rocket-app/src/runner_sequence.rs`, `contract_service.rs`, `collection_service.rs`
- Modify: `src/lib/tauri-api.ts`, `src/components/collections/CollectionNode.tsx`, `FolderNode.tsx`, `src/lib/contracts/collectPaths.ts`
- Modify tests: `src/lib/contracts/collectPaths.test.ts`, `src/components/collections/__tests__/CollectionNode.test.tsx`
- Modify docs: `crates/rocket-collection/CLAUDE.md`, `crates/rocket-infra/CLAUDE.md`, `docs/superpowers/specs/opencollection-spec-reference.md`

**Interfaces:**
- Consumes (Plan 05): `rocket_collection::RequestKind` (variant `Grpc`), `RequestSummary.kind`, `CollectionRepository::request_kind`, `CollectionItem::GraphQl`, `FsCollectionRepo`'s `requests::request_kind`, and its kind-aware `runtime_variables_of` and `with_runtime_variables` helpers in `fs_collection/variables.rs`.
- Produces:
  - `rocket_collection::GrpcMethodType { Unary (default), ClientStreaming, ServerStreaming, BidiStreaming }` serialized as the spec strings, with `as_str`, `parse`, `from_streaming_flags(client, server)`, `client_streams`, `server_streams`.
  - `rocket_collection::GrpcRequest`, `GrpcMetadataEntry { key, value, enabled, description }`, `GrpcMessage { title, selected, content }`, `GrpcScript { script_type, code }`.
  - `CollectionItem::Grpc(Box<GrpcRequest>)`, serde tag `"grpc"`. Counts as one request in `Folder::request_count`.
  - `CollectionRepository::get_grpc_request(&self, collection: &str, path: &str) -> DomainResult<GrpcRequest>` and `save_grpc_request(&self, collection: &str, path: &str, request: &GrpcRequest) -> DomainResult<String>`. Both have defaults that return `InvalidInput`, so the eleven test doubles keep compiling.
  - `rocket_infra::conversions::{grpc_to_oc, oc_grpc_to_domain}` (crate-internal).
  - `CollectionService::{get_grpc_request, save_grpc_request}` and gRPC-aware `rename_request` and `update_request_docs`.
  - A sidebar summary for a gRPC file: `RequestSummary { kind: Grpc, method: "GRPC", url, name, uid, file_name }`.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.** Section 2.5 and `GrpcMetadata`. Then open https://schema.opencollection.com/opencollection/v1.0.0.json and read `GrpcRequestDetails` and `GrpcRequestRuntime` yourself: this plan relies on `auth` being allowed in `grpc` and not in `runtime`, which the reference doc states the other way round.

- [ ] **Step 2: Write the failing domain tests**

Create `crates/rocket-collection/src/grpc_request.rs` with only this test module first (the implementation goes above it in Step 4):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_request_has_unary_defaults() {
        let r = GrpcRequest::new("Say Hello", "localhost:50051");
        assert!(!r.uid.is_empty());
        assert_eq!(r.method_type, GrpcMethodType::Unary);
        assert!(r.messages.is_empty());
        assert_eq!(r.auth, Auth::None);
    }

    #[test]
    fn method_type_uses_the_spec_strings() {
        assert_eq!(
            serde_json::to_string(&GrpcMethodType::BidiStreaming).expect("serialize"),
            "\"bidi-streaming\""
        );
        assert_eq!(
            GrpcMethodType::parse("client-streaming"),
            Some(GrpcMethodType::ClientStreaming)
        );
        assert_eq!(GrpcMethodType::parse("nope"), None);
        assert_eq!(GrpcMethodType::ServerStreaming.as_str(), "server-streaming");
    }

    #[test]
    fn streaming_flags_map_to_all_four_types() {
        assert_eq!(
            GrpcMethodType::from_streaming_flags(false, false),
            GrpcMethodType::Unary
        );
        assert_eq!(
            GrpcMethodType::from_streaming_flags(true, false),
            GrpcMethodType::ClientStreaming
        );
        assert_eq!(
            GrpcMethodType::from_streaming_flags(false, true),
            GrpcMethodType::ServerStreaming
        );
        assert_eq!(
            GrpcMethodType::from_streaming_flags(true, true),
            GrpcMethodType::BidiStreaming
        );
        assert!(GrpcMethodType::BidiStreaming.client_streams());
        assert!(GrpcMethodType::BidiStreaming.server_streams());
        assert!(!GrpcMethodType::Unary.client_streams());
        assert!(!GrpcMethodType::ClientStreaming.server_streams());
    }

    #[test]
    fn json_shape_is_camel_case_and_skips_empty_fields() {
        let mut r = GrpcRequest::new("Say Hello", "localhost:50051");
        r.method = Some("demo.Greeter/SayHello".into());
        r.method_type = GrpcMethodType::ServerStreaming;
        r.proto_file_path = Some("protos/greeter.proto".into());
        let v = serde_json::to_value(&r).expect("serialize");
        assert_eq!(v["methodType"], "server-streaming");
        assert_eq!(v["protoFilePath"], "protos/greeter.proto");
        assert!(v.get("metadata").is_none(), "empty lists are skipped: {v}");
    }

    #[test]
    fn minimal_payload_deserializes_with_defaults() {
        let r: GrpcRequest =
            serde_json::from_str(r#"{"name":"A","url":"h:1"}"#).expect("minimal payload");
        assert!(!r.uid.is_empty());
        assert_eq!(r.method_type, GrpcMethodType::Unary);
        assert!(r.metadata.is_empty());
    }

    #[test]
    fn metadata_entry_defaults_to_enabled() {
        let e: GrpcMetadataEntry =
            serde_json::from_str(r#"{"key":"k","value":"v"}"#).expect("entry");
        assert!(e.enabled);
    }
}
```

Append to the `tests` module of `crates/rocket-collection/src/folder.rs`:

```rust
    #[test]
    fn grpc_item_counts_as_a_request_and_serializes_with_the_grpc_tag() {
        let mut root = Folder::new("root");
        root.items.push(CollectionItem::Grpc(Box::new(crate::GrpcRequest::new(
            "Say Hello",
            "localhost:50051",
        ))));
        assert_eq!(root.request_count(), 1);
        let v = serde_json::to_value(&root.items[0]).expect("serialize");
        assert_eq!(v["type"], "grpc");
        assert_eq!(v["name"], "Say Hello");
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-collection grpc`
Expected: FAIL to compile (`GrpcRequest`, `GrpcMethodType` and `CollectionItem::Grpc` not found).

- [ ] **Step 4: Implement the domain types**

Put this above the test module in `crates/rocket-collection/src/grpc_request.rs`:

```rust
use rocket_shared::assertion::Assertion;
use rocket_shared::description::Description;
use rocket_shared::types::Auth;
use serde::{Deserialize, Serialize};

use crate::settings::CollectionVariable;

/// The four gRPC call shapes. The serialized strings match the OpenCollection
/// `methodType` values, so the IPC payload and the YAML file agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum GrpcMethodType {
    #[default]
    #[serde(rename = "unary")]
    Unary,
    #[serde(rename = "client-streaming")]
    ClientStreaming,
    #[serde(rename = "server-streaming")]
    ServerStreaming,
    #[serde(rename = "bidi-streaming")]
    BidiStreaming,
}

impl GrpcMethodType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unary => "unary",
            Self::ClientStreaming => "client-streaming",
            Self::ServerStreaming => "server-streaming",
            Self::BidiStreaming => "bidi-streaming",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "unary" => Some(Self::Unary),
            "client-streaming" => Some(Self::ClientStreaming),
            "server-streaming" => Some(Self::ServerStreaming),
            "bidi-streaming" => Some(Self::BidiStreaming),
            _ => None,
        }
    }

    /// Builds the type from the two streaming flags of a method descriptor.
    pub fn from_streaming_flags(client_streaming: bool, server_streaming: bool) -> Self {
        match (client_streaming, server_streaming) {
            (false, false) => Self::Unary,
            (true, false) => Self::ClientStreaming,
            (false, true) => Self::ServerStreaming,
            (true, true) => Self::BidiStreaming,
        }
    }

    /// True when the client sends more than one message.
    pub fn client_streams(&self) -> bool {
        matches!(self, Self::ClientStreaming | Self::BidiStreaming)
    }

    /// True when the server sends more than one message.
    pub fn server_streams(&self) -> bool {
        matches!(self, Self::ServerStreaming | Self::BidiStreaming)
    }
}

/// One metadata (header) line sent with a gRPC call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcMetadataEntry {
    pub key: String,
    pub value: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Description>,
}

fn default_true() -> bool {
    true
}

impl GrpcMetadataEntry {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            enabled: true,
            description: None,
        }
    }
}

/// One saved message body. A request with a single untitled message is written
/// as a plain string in the file; every other shape is written as variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcMessage {
    #[serde(default)]
    pub title: String,
    /// The message the editor shows first and a unary or server-streaming call sends.
    #[serde(default)]
    pub selected: bool,
    pub content: String,
}

/// A script stored under `runtime.scripts`. Kept as-is so a file round-trips.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcScript {
    #[serde(rename = "type")]
    pub script_type: String,
    pub code: String,
}

/// A saved gRPC request (OpenCollection `GrpcRequest`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcRequest {
    #[serde(default = "crate::generate_uid")]
    pub uid: String,
    pub name: String,
    /// On-disk file name. Filled in when the collection is loaded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Description>,
    pub url: String,
    /// Full RPC name, `package.Service/Method`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default)]
    pub method_type: GrpcMethodType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proto_file_path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metadata: Vec<GrpcMetadataEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<GrpcMessage>,
    /// The OpenCollection schema keeps gRPC auth in the `grpc` block, not in `runtime`.
    #[serde(default)]
    pub auth: Auth,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<CollectionVariable>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<GrpcScript>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assertions: Vec<Assertion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
}

impl GrpcRequest {
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            uid: crate::generate_uid(),
            name: name.into(),
            file_name: None,
            seq: None,
            tags: Vec::new(),
            description: None,
            url: url.into(),
            method: None,
            method_type: GrpcMethodType::Unary,
            proto_file_path: None,
            metadata: Vec::new(),
            messages: Vec::new(),
            auth: Auth::None,
            variables: Vec::new(),
            scripts: Vec::new(),
            assertions: Vec::new(),
            docs: None,
        }
    }
}
```

In `crates/rocket-collection/src/lib.rs` add `pub mod grpc_request;` after `pub mod folder;` and this re-export after the `folder` one:

```rust
pub use grpc_request::{
    GrpcMessage, GrpcMetadataEntry, GrpcMethodType, GrpcRequest, GrpcScript,
};
```

In `crates/rocket-collection/src/folder.rs`:
- add `use crate::grpc_request::GrpcRequest;` at the top;
- add this variant to `CollectionItem`, after Plan 05's `GraphQl` variant and before `OpaqueItem`, and change the doc of `OpaqueItem` to say "Raw YAML for protocols that have no typed variant yet (WebSocket)":

```rust
    /// A typed gRPC request. Boxed for the same reason `Request` is.
    #[serde(rename = "grpc")]
    Grpc(Box<GrpcRequest>),
```

- in `Folder::request_count`, add the arm `CollectionItem::Grpc(_) => 1,`.

In `crates/rocket-collection/src/repository.rs`, add `use crate::grpc_request::GrpcRequest;` (the `DomainError` import and `RequestKind` come from Plan 05) and add these two defaulted methods after Plan 05's `request_kind`:

```rust
    /// Read one gRPC request file. Repositories without gRPC support keep this default.
    fn get_grpc_request(&self, _collection: &str, _path: &str) -> DomainResult<GrpcRequest> {
        Err(DomainError::InvalidInput(
            "this repository does not support gRPC requests".into(),
        ))
    }

    /// Save a gRPC request. Returns the actual filename written, like `save_request`.
    fn save_grpc_request(
        &self,
        _collection: &str,
        _path: &str,
        _request: &GrpcRequest,
    ) -> DomainResult<String> {
        Err(DomainError::InvalidInput(
            "this repository does not support gRPC requests".into(),
        ))
    }
```

- [ ] **Step 5: Run the domain tests to verify they pass**

Run: `cargo test -j4 -p rocket-collection`
Expected: PASS. The crate has no exhaustive match outside `request_count`.

- [ ] **Step 6: Write the failing infra conversion tests**

Append to `crates/rocket-infra/src/conversions/tests.rs`:

```rust
const FULL_GRPC_YAML: &str = r#"
uid: grpc-1
info:
  name: Say Hello
  type: grpc
  seq: 2
  tags:
    - smoke
grpc:
  url: grpcs://api.example.com:443
  method: demo.greeter.v1.Greeter/SayHello
  methodType: server-streaming
  protoFilePath: protos/greeter.proto
  metadata:
    - name: x-trace
      value: abc
    - name: x-off
      value: '1'
      disabled: true
  message: '{"name": "ada"}'
  auth:
    type: bearer
    token: t
runtime:
  variables:
    - name: tenant
      value: acme
  scripts:
    - type: before-request
      code: console.log(1)
docs: Greets people
"#;

#[test]
fn oc_grpc_request_converts_to_the_domain_type() {
    use rocket_collection::{GrpcMethodType, GrpcRequest};

    let oc: OcGrpcRequest = serde_yaml::from_str(FULL_GRPC_YAML).expect("parse");
    let g: GrpcRequest = oc_grpc_to_domain(oc);
    assert_eq!(g.uid, "grpc-1");
    assert_eq!(g.name, "Say Hello");
    assert_eq!(g.seq, Some(2));
    assert_eq!(g.tags, vec!["smoke".to_string()]);
    assert_eq!(g.url, "grpcs://api.example.com:443");
    assert_eq!(
        g.method.as_deref(),
        Some("demo.greeter.v1.Greeter/SayHello")
    );
    assert_eq!(g.method_type, GrpcMethodType::ServerStreaming);
    assert_eq!(g.proto_file_path.as_deref(), Some("protos/greeter.proto"));
    assert_eq!(g.metadata.len(), 2);
    assert!(g.metadata[0].enabled);
    assert!(
        !g.metadata[1].enabled,
        "disabled: true becomes enabled: false"
    );
    assert_eq!(g.messages.len(), 1);
    assert_eq!(g.messages[0].title, "");
    assert!(g.messages[0].selected);
    assert_eq!(g.messages[0].content, "{\"name\": \"ada\"}");
    assert!(matches!(g.auth, Auth::Bearer { .. }));
    assert_eq!(g.variables.len(), 1);
    assert_eq!(g.scripts.len(), 1);
    assert_eq!(g.docs.as_deref(), Some("Greets people"));
}

#[test]
fn a_grpc_request_survives_domain_oc_yaml_and_back() {
    let oc: OcGrpcRequest = serde_yaml::from_str(FULL_GRPC_YAML).expect("parse");
    let g = oc_grpc_to_domain(oc);

    let yaml = serde_yaml::to_string(&grpc_to_oc(&g)).expect("serialize");
    let again: OcGrpcRequest = serde_yaml::from_str(&yaml).expect("reparse");
    assert_eq!(oc_grpc_to_domain(again), g, "{yaml}");
}

#[test]
fn a_single_untitled_message_is_written_as_a_string_and_titled_ones_as_variants() {
    use rocket_collection::{GrpcMessage, GrpcRequest};

    let mut g = GrpcRequest::new("A", "h:1");
    g.messages = vec![GrpcMessage {
        title: String::new(),
        selected: true,
        content: "{}".into(),
    }];
    assert!(matches!(
        grpc_to_oc(&g).grpc.message,
        Some(OcGrpcMessageOrVariants::Single(ref s)) if s == "{}"
    ));

    g.messages = vec![
        GrpcMessage {
            title: "first".into(),
            selected: false,
            content: "{\"a\": 1}".into(),
        },
        GrpcMessage {
            title: "second".into(),
            selected: true,
            content: "{\"a\": 2}".into(),
        },
    ];
    let Some(OcGrpcMessageOrVariants::Variants(v)) = grpc_to_oc(&g).grpc.message else {
        panic!("expected variants");
    };
    assert_eq!(v.len(), 2);
    assert!(!v[0].selected);
    assert!(v[1].selected);
    assert_eq!(v[1].message, "{\"a\": 2}");

    g.messages.clear();
    assert!(grpc_to_oc(&g).grpc.message.is_none());
}

#[test]
fn an_unknown_method_type_loads_as_unary() {
    use rocket_collection::GrpcMethodType;

    let yaml = "info:\n  name: A\n  type: grpc\ngrpc:\n  url: h:1\n  methodType: duplex\n";
    let oc: OcGrpcRequest = serde_yaml::from_str(yaml).expect("parse");
    assert_eq!(oc_grpc_to_domain(oc).method_type, GrpcMethodType::Unary);
}

#[test]
fn auth_is_written_in_the_grpc_block_never_in_runtime() {
    // The schema allows auth in `grpc` only. An auth that an older file kept in `runtime`
    // is read, then written back in its schema position.
    let yaml = "info:\n  name: A\n  type: grpc\ngrpc:\n  url: h:1\nruntime:\n  auth:\n    type: bearer\n    token: t\n";
    let oc: OcGrpcRequest = serde_yaml::from_str(yaml).expect("parse");
    let g = oc_grpc_to_domain(oc);
    assert!(matches!(g.auth, Auth::Bearer { .. }));

    let back = grpc_to_oc(&g);
    assert!(back.grpc.auth.is_some());
    assert!(back.runtime.is_none(), "{:?}", back.runtime);
}

#[test]
fn an_empty_uid_is_not_written_and_a_missing_one_loads_empty() {
    let oc: OcGrpcRequest =
        serde_yaml::from_str("info:\n  name: A\n  type: grpc\ngrpc:\n  url: h:1\n").expect("parse");
    let mut g = oc_grpc_to_domain(oc);
    assert_eq!(g.uid, "");
    assert!(grpc_to_oc(&g).uid.is_none());
    g.uid = "u1".into();
    assert_eq!(grpc_to_oc(&g).uid.as_deref(), Some("u1"));
}

#[test]
fn grpc_items_are_typed_in_a_folder_and_written_back_as_grpc() {
    let yaml = r#"
info:
  name: Mixed
  type: folder
items:
  - info:
      name: Say Hello
      type: grpc
    grpc:
      url: "localhost:50051"
      method: demo.Greeter/SayHello
"#;
    let oc: OcFolder = serde_yaml::from_str(yaml).unwrap();
    let folder = oc_folder_to_folder(oc);
    assert!(matches!(&folder.items[0], CollectionItem::Grpc(g) if g.name == "Say Hello"));

    let back = folder_to_oc_folder(folder);
    assert!(matches!(&back.items.unwrap()[0], OcItem::Grpc(_)));
}
```

- [ ] **Step 7: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-infra oc_grpc`
Expected: FAIL to compile (`oc_grpc_to_domain` and `grpc_to_oc` not found).

- [ ] **Step 8: Implement the OC struct changes and the conversions**

In `crates/rocket-infra/src/oc/grpc.rs`, add `uid` as the first field of `OcGrpcRequest`:

```rust
    /// Stable identity for tab deduplication across reloads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
```

and add `Default` to the derive list of `OcGrpcRequestRuntime`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct OcGrpcRequestRuntime {
```

(If `cargo check -j4 -p rocket-infra` reports another place that builds an `OcGrpcRequest { .. }` literal, add `uid: None`.)

Create `crates/rocket-infra/src/conversions/grpc.rs`:

```rust
//! Conversions between the domain `GrpcRequest` and the OC gRPC structs.

use crate::oc::*;
use rocket_collection::settings::CollectionVariable;
use rocket_collection::{GrpcMessage, GrpcMetadataEntry, GrpcMethodType, GrpcRequest, GrpcScript};
use rocket_shared::types::Auth;

use super::auth::persisted_oc_auth;

/// Convert an OC gRPC request to the domain type.
///
/// The OpenCollection schema keeps gRPC auth in the `grpc` block. An auth found in
/// `runtime` (which the schema does not allow) is accepted on read and written back
/// in the `grpc` block.
pub fn oc_grpc_to_domain(oc: OcGrpcRequest) -> GrpcRequest {
    let method_type = match oc.grpc.method_type.as_deref() {
        None => GrpcMethodType::default(),
        Some(raw) => GrpcMethodType::parse(raw).unwrap_or_else(|| {
            tracing::warn!(
                method_type = raw,
                "unknown gRPC methodType, treating it as unary"
            );
            GrpcMethodType::default()
        }),
    };

    let messages = match oc.grpc.message {
        None => Vec::new(),
        Some(OcGrpcMessageOrVariants::Single(content)) => vec![GrpcMessage {
            title: String::new(),
            selected: true,
            content,
        }],
        Some(OcGrpcMessageOrVariants::Variants(variants)) => variants
            .into_iter()
            .map(|v| GrpcMessage {
                title: v.title,
                selected: v.selected,
                content: v.message,
            })
            .collect(),
    };

    let runtime_auth = oc.runtime.as_ref().and_then(|r| r.auth.clone());
    let (variables, scripts, assertions) = match oc.runtime {
        Some(rt) => (
            rt.variables
                .into_iter()
                .map(CollectionVariable::from)
                .collect(),
            rt.scripts
                .into_iter()
                .map(|s| GrpcScript {
                    script_type: s.script_type,
                    code: s.code,
                })
                .collect(),
            rt.assertions,
        ),
        None => (Vec::new(), Vec::new(), Vec::new()),
    };

    GrpcRequest {
        uid: oc.uid.unwrap_or_default(),
        name: oc.info.name,
        file_name: None,
        seq: oc.info.seq,
        tags: oc.info.tags,
        description: oc.info.description,
        url: oc.grpc.url,
        method: oc.grpc.method,
        method_type,
        proto_file_path: oc.grpc.proto_file_path,
        metadata: oc
            .grpc
            .metadata
            .into_iter()
            .map(|m| GrpcMetadataEntry {
                key: m.name,
                value: m.value,
                enabled: !m.disabled.unwrap_or(false),
                description: m.description,
            })
            .collect(),
        messages,
        auth: oc
            .grpc
            .auth
            .or(runtime_auth)
            .map(Auth::from)
            .unwrap_or(Auth::None),
        variables,
        scripts,
        assertions,
        docs: oc.docs,
    }
}

/// Convert a domain gRPC request back to the OC struct.
pub fn grpc_to_oc(g: &GrpcRequest) -> OcGrpcRequest {
    // One untitled message is the plain string form. Anything else keeps its titles.
    let message = match g.messages.as_slice() {
        [] => None,
        [only] if only.title.is_empty() => {
            Some(OcGrpcMessageOrVariants::Single(only.content.clone()))
        }
        many => Some(OcGrpcMessageOrVariants::Variants(
            many.iter()
                .map(|m| OcGrpcMessageVariant {
                    title: m.title.clone(),
                    selected: m.selected,
                    message: m.content.clone(),
                })
                .collect(),
        )),
    };

    let has_runtime = !g.variables.is_empty() || !g.scripts.is_empty() || !g.assertions.is_empty();
    // The schema allows only variables, scripts and assertions in `runtime`.
    let runtime = has_runtime.then(|| OcGrpcRequestRuntime {
        variables: g.variables.iter().cloned().map(OcVariable::from).collect(),
        scripts: g
            .scripts
            .iter()
            .map(|s| OcScript {
                script_type: s.script_type.clone(),
                code: s.code.clone(),
            })
            .collect(),
        assertions: g.assertions.clone(),
        auth: None,
    });

    OcGrpcRequest {
        uid: if g.uid.is_empty() {
            None
        } else {
            Some(g.uid.clone())
        },
        info: OcGrpcRequestInfo {
            name: g.name.clone(),
            description: g.description.clone(),
            request_type: Some("grpc".into()),
            seq: g.seq,
            tags: g.tags.clone(),
        },
        grpc: OcGrpcRequestDetails {
            url: g.url.clone(),
            method: g.method.clone(),
            method_type: Some(g.method_type.as_str().to_string()),
            proto_file_path: g.proto_file_path.clone(),
            metadata: g
                .metadata
                .iter()
                .map(|m| OcGrpcMetadata {
                    name: m.key.clone(),
                    value: m.value.clone(),
                    description: m.description.clone(),
                    disabled: if m.enabled { None } else { Some(true) },
                })
                .collect(),
            message,
            auth: persisted_oc_auth(g.auth.clone()),
        },
        runtime,
        docs: g.docs.clone(),
    }
}
```

In `crates/rocket-infra/src/conversions/mod.rs` add `mod grpc;` after `mod folder;` and this export next to the other `pub use` lines:

```rust
pub use grpc::{grpc_to_oc, oc_grpc_to_domain};
```

In `crates/rocket-infra/src/conversions/folder.rs`:
- add `use super::grpc::{grpc_to_oc, oc_grpc_to_domain};`;
- change the doc comment of `oc_item_to_collection_item` to say "gRPC becomes a typed `Grpc` item. GraphQL (Plan 05) is typed too. WebSocket items become `OpaqueItem`s that hold their raw YAML, so nothing is lost on load.";
- replace the `OcItem::Grpc(grpc)` arm with:

```rust
        OcItem::Grpc(grpc) => Some(CollectionItem::Grpc(Box::new(oc_grpc_to_domain(grpc)))),
```

- in both `folder_to_oc_folder` and `collection_to_oc_collection`, add this arm after the `CollectionItem::Summary(_) => None,` arm:

```rust
            CollectionItem::Grpc(g) => Some(OcItem::Grpc(grpc_to_oc(&g))),
```

- [ ] **Step 9: Run the conversion tests to verify they pass**

Run: `cargo test -j4 -p rocket-infra conversions`
Expected: PASS for the seven new tests and every older conversion test. (If the crate does not compile far enough to run tests, do Steps 10 to 12 first and come back.)

- [ ] **Step 10: Write the failing repository tests**

Append to `crates/rocket-infra/src/fs_collection/tests.rs`:

```rust
fn grpc_item_yml(uid_line: &str) -> String {
    format!(
        "{uid_line}info:\n  name: Get User\n  type: grpc\ngrpc:\n  url: grpc://api.example.com\n  method: users.UserService/GetUser\n  methodType: unary\n"
    )
}

fn sample_grpc_request() -> rocket_collection::GrpcRequest {
    use rocket_collection::{GrpcMessage, GrpcMetadataEntry, GrpcMethodType};

    let mut g = rocket_collection::GrpcRequest::new("Say Hello", "localhost:50051");
    g.method = Some("demo.greeter.v1.Greeter/SayHello".into());
    g.method_type = GrpcMethodType::ServerStreaming;
    g.proto_file_path = Some("protos/greeter.proto".into());
    g.metadata = vec![GrpcMetadataEntry::new("x-trace", "abc")];
    g.messages = vec![GrpcMessage {
        title: String::new(),
        selected: true,
        content: "{\"name\": \"ada\"}".into(),
    }];
    g
}

#[test]
fn grpc_request_round_trips_through_the_repo() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    let g = sample_grpc_request();

    let saved = repo
        .save_grpc_request("my-api", "say-hello.yml", &g)
        .unwrap();
    assert_eq!(saved, "say-hello.yml");

    let back = repo.get_grpc_request("my-api", "say-hello.yml").unwrap();
    assert_eq!(back.uid, g.uid);
    assert_eq!(back.method, g.method);
    assert_eq!(back.method_type, g.method_type);
    assert_eq!(back.proto_file_path, g.proto_file_path);
    assert_eq!(back.metadata, g.metadata);
    assert_eq!(back.messages, g.messages);
    assert_eq!(back.file_name.as_deref(), Some("say-hello.yml"));

    let raw = read_yaml_value(&dir.path().join("my-api/say-hello.yml"));
    assert_eq!(raw["info"]["type"].as_str(), Some("grpc"), "{raw:?}");
    assert!(raw.get("grpc").is_some(), "{raw:?}");
    assert!(raw.get("http").is_none(), "{raw:?}");
    assert_eq!(
        raw["grpc"]["methodType"].as_str(),
        Some("server-streaming"),
        "{raw:?}"
    );
}

#[test]
fn a_single_untitled_message_is_saved_as_a_plain_string() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.save_grpc_request("my-api", "a.yml", &sample_grpc_request())
        .unwrap();
    let raw = read_yaml_value(&dir.path().join("my-api/a.yml"));
    assert_eq!(
        raw["grpc"]["message"].as_str(),
        Some("{\"name\": \"ada\"}"),
        "{raw:?}"
    );
}

#[test]
fn get_grpc_request_gives_a_uid_less_file_an_in_memory_uid() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    let path = dir.path().join("my-api/get-user.yml");
    fs::write(&path, grpc_item_yml("")).unwrap();

    let g = repo.get_grpc_request("my-api", "get-user.yml").unwrap();
    assert!(!g.uid.is_empty());
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        grpc_item_yml(""),
        "a read must not rewrite the file"
    );
}

#[test]
fn an_opaque_era_grpc_file_keeps_its_call_description_through_load_and_save() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(dir.path().join("my-api/get-user.yml"), grpc_item_yml("")).unwrap();

    let g = repo.get_grpc_request("my-api", "get-user.yml").unwrap();
    repo.save_grpc_request("my-api", "get-user.yml", &g)
        .unwrap();

    let raw = read_yaml_value(&dir.path().join("my-api/get-user.yml"));
    assert_eq!(raw["info"]["name"].as_str(), Some("Get User"), "{raw:?}");
    assert_eq!(
        raw["grpc"]["url"].as_str(),
        Some("grpc://api.example.com"),
        "{raw:?}"
    );
    assert_eq!(
        raw["grpc"]["method"].as_str(),
        Some("users.UserService/GetUser"),
        "{raw:?}"
    );
    assert_eq!(raw["grpc"]["methodType"].as_str(), Some("unary"), "{raw:?}");
    assert!(
        raw["uid"].as_str().is_some(),
        "the save persists the uid: {raw:?}"
    );
}

#[test]
fn save_grpc_request_rejects_an_empty_uid() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    let mut g = sample_grpc_request();
    g.uid = String::new();
    assert!(repo.save_grpc_request("my-api", "a.yml", &g).is_err());
}

#[test]
fn save_grpc_request_keeps_stored_variables_when_the_payload_has_none() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(
        dir.path().join("my-api/get-user.yml"),
        "uid: g1\ninfo:\n  name: Get User\n  type: grpc\ngrpc:\n  url: h:1\nruntime:\n  variables:\n  - name: tenant\n    value: acme\n",
    )
    .unwrap();

    let mut g = repo.get_grpc_request("my-api", "get-user.yml").unwrap();
    g.variables.clear();
    g.name = "Renamed".into();
    repo.save_grpc_request("my-api", "get-user.yml", &g)
        .unwrap();

    let yaml = fs::read_to_string(dir.path().join("my-api/get-user.yml")).unwrap();
    assert!(yaml.contains("name: tenant"), "{yaml}");
    assert!(yaml.contains("name: Renamed"), "{yaml}");
}

#[test]
fn request_variables_work_for_a_grpc_file() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(
        dir.path().join("my-api/get-user.yml"),
        grpc_item_yml("uid: g1\n"),
    )
    .unwrap();

    let vars = vec![CollectionVariable {
        key: "tenant".into(),
        value: "acme".into(),
        initial_value: String::new(),
        enabled: true,
        secret: false,
    }];
    repo.save_request_variables("my-api", "get-user.yml", vars)
        .unwrap();
    let back = repo
        .get_request_variables("my-api", "get-user.yml")
        .unwrap();
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].key, "tenant");

    // The save must not turn the file into an HTTP request.
    let g = repo.get_grpc_request("my-api", "get-user.yml").unwrap();
    assert_eq!(g.url, "grpc://api.example.com");
    assert_eq!(g.variables.len(), 1);
}

#[test]
fn request_kind_reports_grpc_for_a_grpc_file() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(dir.path().join("my-api/get-user.yml"), grpc_item_yml("")).unwrap();
    assert_eq!(
        repo.request_kind("my-api", "get-user.yml").unwrap(),
        rocket_collection::RequestKind::Grpc
    );
}

#[test]
fn full_tree_loads_grpc_as_a_typed_item_with_its_file_name() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "users").unwrap();
    fs::write(
        dir.path().join("my-api/users/get-user.yml"),
        grpc_item_yml("uid: g1\n"),
    )
    .unwrap();

    let col = repo.get("my-api").unwrap();
    let users = col.root.find_folder("users").unwrap();
    let found = users.items.iter().find_map(|i| match i {
        rocket_collection::CollectionItem::Grpc(g) => Some(g),
        _ => None,
    });
    let g = found.expect("a typed Grpc item");
    assert_eq!(g.name, "Get User");
    assert_eq!(g.uid, "g1");
    assert_eq!(g.url, "grpc://api.example.com");
    assert_eq!(g.file_name.as_deref(), Some("get-user.yml"));
    assert_eq!(col.root.request_count(), 1);
}

#[test]
fn get_summaries_returns_a_grpc_summary_with_its_kind() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(
        dir.path().join("my-api/get-user.yml"),
        grpc_item_yml("uid: g1\n"),
    )
    .unwrap();

    let col = repo.get_summaries("my-api").unwrap();
    assert_eq!(col.root.items.len(), 1, "{:?}", col.root.items);
    match &col.root.items[0] {
        rocket_collection::CollectionItem::Summary(s) => {
            assert_eq!(s.kind, rocket_collection::RequestKind::Grpc);
            assert_eq!(s.uid, "g1");
            assert_eq!(s.name, "Get User");
            assert_eq!(s.method, "GRPC");
            assert_eq!(s.url, "grpc://api.example.com");
            assert_eq!(s.file_name.as_deref(), Some("get-user.yml"));
        }
        other => panic!("expected a summary, got {other:?}"),
    }
}

#[test]
fn rename_item_and_move_keep_working_for_a_grpc_file() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    repo.create_folder("my-api", "users").unwrap();
    fs::write(
        dir.path().join("my-api/get-user.yml"),
        grpc_item_yml("uid: g1\n"),
    )
    .unwrap();

    repo.move_item("my-api", "get-user.yml", "my-api", "users/get-user.yml")
        .unwrap();
    assert!(dir.path().join("my-api/users/get-user.yml").exists());
    repo.delete_request("my-api", "users/get-user.yml").unwrap();
    assert!(!dir.path().join("my-api/users/get-user.yml").exists());
}
```

Update tests that earlier plans wrote, because gRPC is no longer opaque and no longer skipped:

1. `build_folder_tree_loads_non_http_items_as_opaque`. Before Plan 05 it asserted `vec![("graphql", "List Users"), ("grpc", "Get User")]`. After Plan 05 it asserts `vec![("grpc", "Get User")]`. Replace that assertion with:

```rust
    // GraphQL and gRPC are typed now. Only WebSocket stays opaque.
    assert!(root.is_empty(), "no opaque item at the root: {root:?}");
    assert!(col.root.items.iter().any(
        |i| matches!(i, rocket_collection::CollectionItem::Grpc(g) if g.name == "Get User")
    ));
```

2. Plan 05's `get_summaries_skips_grpc_items_without_error` (renamed from `get_summaries_skips_non_http_items_without_error`). A gRPC file now returns a summary. Rename the test `get_summaries_skips_websocket_items_without_error` and change its fixture write from `GRPC_ITEM_YML` to `WEBSOCKET_ITEM_YML`, keeping the file name and assertions. If Plan 08 has already landed, delete the test instead: no non-HTTP type is skipped any more.
3. Plan 05's `get_summaries_returns_a_graphql_summary_with_its_kind` writes a gRPC file next to the GraphQL one and asserts `items.len() == 1, "gRPC is still skipped"` and reads `items[0]`. Files load in file-name order, so `get-user.yml` (gRPC) now comes before `list-users.yml`. Change it to assert two items and find the GraphQL summary by kind:

```rust
    let col = repo.get_summaries("my-api").unwrap();
    assert_eq!(col.root.items.len(), 2, "GraphQL and gRPC both listed: {:?}", col.root.items);
    let s = col
        .root
        .items
        .iter()
        .find_map(|i| match i {
            rocket_collection::CollectionItem::Summary(s)
                if s.kind == rocket_collection::RequestKind::GraphQl =>
            {
                Some(s)
            }
            _ => None,
        })
        .expect("a GraphQL summary");
    assert_eq!(s.uid, "g1");
    assert_eq!(s.method, "POST");
    assert_eq!(s.url, "https://api.example.com/graphql");
    assert_eq!(s.file_name.as_deref(), Some("list-users.yml"));
```

   (Keep the lines of that test that write the two files.)
4. In `reorder_items_writes_order_file_and_get_respects_it`, add the arm `CollectionItem::Grpc(g) => g.name.as_str(),` to `item_name`, next to the `GraphQl` arm Plan 05 added.

- [ ] **Step 11: Run the repository tests to verify they fail**

Run: `cargo test -j4 -p rocket-infra fs_collection::tests::grpc`
Expected: the crate compiles and the new tests FAIL at runtime, because `FsCollectionRepo` does not override `get_grpc_request` yet and the defaulted trait method returns `InvalidInput`.

- [ ] **Step 12: Implement the repository changes**

In `crates/rocket-infra/src/fs_collection/requests.rs`, add `GrpcRequest` to the `rocket_collection` import, `grpc_to_oc, oc_grpc_to_domain` to the `crate::conversions` import and `OcGrpcRequest` to the `crate::oc` import, then append:

```rust
pub(super) fn get_grpc_request(
    repo: &FsCollectionRepo,
    collection: &str,
    path: &str,
) -> DomainResult<GrpcRequest> {
    Collection::validate_name(collection)?;
    let collection_dir = repo.collection_path(collection);
    let file_path = resolve_request_path(repo, &collection_dir, path)?;
    if !file_path.exists() {
        return Err(DomainError::NotFound(format!("{}/{}", collection, path)));
    }
    let content = fs::read_to_string(&file_path)?;
    let oc: OcGrpcRequest = serde_yaml::from_str(&content)
        .map_err(|e| DomainError::Internal(format!("Failed to parse gRPC request: {e}")))?;
    let mut request = oc_grpc_to_domain(oc);
    request.file_name = file_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string());
    // A uid-less file gets an in-memory uid only; the next save persists it.
    if request.uid.is_empty() {
        request.uid = generate_uid();
    }
    Ok(request)
}

#[tracing::instrument(name = "collection_save_grpc_request", skip(repo, request), fields(collection_name = %collection, request_path = %path))]
pub(super) fn save_grpc_request(
    repo: &FsCollectionRepo,
    collection: &str,
    path: &str,
    request: &GrpcRequest,
) -> DomainResult<String> {
    Collection::validate_name(collection)?;
    let mutex = repo.collection_mutex(collection);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    if request.uid.is_empty() {
        return Err(DomainError::Internal(format!(
            "save_grpc_request: empty uid on request for '{path}' in collection '{collection}'; callers must construct via GrpcRequest::new()"
        )));
    }

    let collection_dir = repo.collection_path(collection);
    let normalized = request_filename_for(path);
    let file_path = repo.validate_path(&collection_dir, Path::new(&normalized))?;

    let mut oc = grpc_to_oc(request);

    // Request variables are saved on their own path, so an empty list in the
    // payload must keep what is on disk.
    if request.variables.is_empty() && file_path.exists() {
        if let Ok(existing_content) = fs::read_to_string(&file_path) {
            if let Ok(existing) = serde_yaml::from_str::<OcGrpcRequest>(&existing_content) {
                if let Some(existing_runtime) = existing.runtime {
                    if !existing_runtime.variables.is_empty() {
                        let runtime = oc.runtime.get_or_insert_with(Default::default);
                        runtime.variables = existing_runtime.variables;
                    }
                }
            }
        }
    }

    let yaml = serde_yaml::to_string(&oc).map_err(|e| {
        DomainError::Internal(format!("Failed to serialize gRPC request YAML: {e}"))
    })?;
    atomic_write(&file_path, yaml.as_bytes())?;

    let actual = file_path
        .strip_prefix(&collection_dir)
        .unwrap_or(&file_path)
        .to_string_lossy()
        .to_string();
    Ok(actual)
}
```

In `crates/rocket-infra/src/fs_collection/mod.rs`, add `GrpcRequest` to the `rocket_collection` import and these methods to `impl CollectionRepository for FsCollectionRepo`, after Plan 05's `request_kind`:

```rust
    fn get_grpc_request(&self, collection: &str, path: &str) -> DomainResult<GrpcRequest> {
        requests::get_grpc_request(self, collection, path)
    }

    fn save_grpc_request(
        &self,
        collection: &str,
        path: &str,
        request: &GrpcRequest,
    ) -> DomainResult<String> {
        requests::save_grpc_request(self, collection, path, request)
    }
```

In `crates/rocket-infra/src/shared_path_collection_repo.rs`, add `GrpcRequest` to its `rocket_collection` import and the same two methods, each delegating to `self.repo()`:

```rust
    fn get_grpc_request(&self, collection: &str, path: &str) -> DomainResult<GrpcRequest> {
        self.repo().get_grpc_request(collection, path)
    }

    fn save_grpc_request(
        &self,
        collection: &str,
        path: &str,
        request: &GrpcRequest,
    ) -> DomainResult<String> {
        self.repo().save_grpc_request(collection, path, request)
    }
```

In `crates/rocket-infra/src/fs_collection/tree.rs`:
- in `build_folder_tree`, add this match arm next to Plan 05's `GraphQl` arm, before `Ok(other) => Ok(other),`:

```rust
            Ok(Some(CollectionItem::Grpc(mut grpc))) => {
                grpc.file_name = Some(entry_name.to_string());
                Ok(Some(CollectionItem::Grpc(grpc)))
            }
```

- in `load_request_summary`, in the final `match serde_yaml::from_str::<OcItem>(&content)` that Plan 05 rewrote, add a gRPC arm before the `Ok(OcItem::Http(_)) | ...` arm and remove `OcItem::Grpc(_)` from the arm that returns `Ok(None)`:

```rust
            // A gRPC file is small, so the summary reads the few fields the sidebar needs.
            Ok(OcItem::Grpc(grpc)) => Ok(Some(RequestSummary {
                uid: grpc.uid.unwrap_or_default(),
                name: grpc.info.name,
                method: "GRPC".to_string(),
                url: grpc.grpc.url,
                file_name: Some(entry_name.to_string()),
                kind: RequestKind::Grpc,
            })),
```

  The remaining `Ok(None)` arm becomes `Ok(OcItem::WebSocket(_) | OcItem::ScriptFile(_)) => Ok(None),`. Update the doc comment of `load_request_summary` to say that GraphQL and gRPC files return a summary with their `kind` and WebSocket and script files return `Ok(None)`.

In `crates/rocket-infra/src/fs_collection/variables.rs`, extend the `crate::oc` import with `OcGrpcRequest, OcGrpcRequestRuntime` and replace Plan 05's two helpers `runtime_variables_of` and `with_runtime_variables` with these versions, which try HTTP, then GraphQL, then gRPC:

```rust
/// Reads `runtime.variables` from an HTTP, GraphQL or gRPC request file.
fn runtime_variables_of(content: &str) -> DomainResult<Vec<OcVariable>> {
    let http_err = match serde_yaml::from_str::<OcHttpRequest>(content) {
        Ok(req) => return Ok(req.runtime.map(|r| r.variables).unwrap_or_default()),
        Err(e) => e,
    };
    if let Ok(g) = serde_yaml::from_str::<OcGraphQLRequest>(content) {
        return Ok(g.runtime.map(|r| r.variables).unwrap_or_default());
    }
    match serde_yaml::from_str::<OcGrpcRequest>(content) {
        Ok(g) => Ok(g.runtime.map(|r| r.variables).unwrap_or_default()),
        // Keep the HTTP error: it is the precise one for a broken HTTP file.
        Err(_) => Err(DomainError::Internal(format!(
            "Failed to parse request file: {http_err}"
        ))),
    }
}

/// Returns the file content with `runtime.variables` replaced, for an HTTP, GraphQL or gRPC request file.
fn with_runtime_variables(content: &str, vars: Vec<OcVariable>) -> DomainResult<String> {
    let to_err = |e: serde_yaml::Error| {
        DomainError::Internal(format!("Failed to serialize request file: {e}"))
    };
    let http_err = match serde_yaml::from_str::<OcHttpRequest>(content) {
        Ok(mut req) => {
            let runtime = req.runtime.take().unwrap_or_default();
            req.runtime = Some(OcHttpRequestRuntime {
                variables: vars,
                ..runtime
            });
            return serde_yaml::to_string(&req).map_err(to_err);
        }
        Err(e) => e,
    };
    if let Ok(mut g) = serde_yaml::from_str::<OcGraphQLRequest>(content) {
        let runtime = g.runtime.take().unwrap_or_default();
        g.runtime = Some(OcGraphQLRequestRuntime {
            variables: vars,
            ..runtime
        });
        return serde_yaml::to_string(&g).map_err(to_err);
    }
    match serde_yaml::from_str::<OcGrpcRequest>(content) {
        Ok(mut g) => {
            let runtime = g.runtime.take().unwrap_or_default();
            g.runtime = Some(OcGrpcRequestRuntime {
                variables: vars,
                ..runtime
            });
            serde_yaml::to_string(&g).map_err(to_err)
        }
        Err(_) => Err(DomainError::Internal(format!(
            "Failed to parse request file: {http_err}"
        ))),
    }
}
```

- [ ] **Step 13: Update the schema-shape guard**

In `crates/rocket-infra/src/fs_collection/schema_shape_tests.rs`:

Add to `KNOWN_DEFERRED`, next to `"HttpRequest.uid"`:

```rust
    "GrpcRequest.uid",
```

Add these allow-lists after `GRAPHQL_BODY`:

```rust
const GRPC_REQUEST: &[&str] = &["info", "grpc", "runtime", "docs"];
const GRPC_DETAILS: &[&str] = &[
    "url",
    "method",
    "methodType",
    "protoFilePath",
    "metadata",
    "message",
    "auth",
];
const GRPC_RUNTIME: &[&str] = &["variables", "scripts", "assertions"];
const GRPC_METADATA: &[&str] = &["name", "value", "description", "disabled"];
const GRPC_MESSAGE_VARIANT: &[&str] = &["title", "selected", "message"];
```

Add this checker after `check_graphql_request`:

```rust
fn check_grpc_request(v: &mut Violations, at: &str, doc: &Value) {
    v.keys("GrpcRequest", at, doc, GRPC_REQUEST);
    if let Some(info) = doc.get("info") {
        v.keys("GrpcRequestInfo", at, info, ITEM_INFO);
    }
    if let Some(grpc) = doc.get("grpc") {
        v.keys("GrpcRequestDetails", at, grpc, GRPC_DETAILS);
        for m in seq(grpc.get("metadata")) {
            v.keys("GrpcMetadata", at, m, GRPC_METADATA);
        }
        for m in seq(grpc.get("message")) {
            v.keys("GrpcMessageVariant", at, m, GRPC_MESSAGE_VARIANT);
        }
        if let Some(auth) = grpc.get("auth") {
            check_auth(v, at, auth);
        }
    }
    if let Some(runtime) = doc.get("runtime") {
        v.keys("GrpcRequestRuntime", at, runtime, GRPC_RUNTIME);
        for s in seq(runtime.get("scripts")) {
            v.keys("Script", at, s, SCRIPT);
        }
        for var in seq(runtime.get("variables")) {
            v.keys("Variable", at, var, VARIABLE);
        }
    }
}
```

Add this test at the end of the file:

```rust
#[test]
fn saved_grpc_request_only_uses_schema_keys_besides_deferred() {
    use rocket_collection::{
        GrpcMessage, GrpcMetadataEntry, GrpcMethodType, GrpcRequest, GrpcScript,
    };

    let (dir, repo) = setup();
    repo.create("api").unwrap();
    let mut g = GrpcRequest::new("Say Hello", "localhost:50051");
    g.method = Some("demo.Greeter/SayHello".into());
    g.method_type = GrpcMethodType::BidiStreaming;
    g.proto_file_path = Some("protos/greeter.proto".into());
    g.metadata = vec![GrpcMetadataEntry::new("x-trace", "abc")];
    g.messages = vec![
        GrpcMessage {
            title: "first".into(),
            selected: true,
            content: "{}".into(),
        },
        GrpcMessage {
            title: "second".into(),
            selected: false,
            content: "{}".into(),
        },
    ];
    g.auth = Auth::Bearer { token: "t".into() };
    g.variables = vec![CollectionVariable {
        key: "tenant".into(),
        value: "acme".into(),
        initial_value: String::new(),
        enabled: true,
        secret: false,
    }];
    g.scripts = vec![GrpcScript {
        script_type: "before-request".into(),
        code: "x".into(),
    }];
    g.docs = Some("docs".into());
    let rel = repo.save_grpc_request("api", "say-hello.yml", &g).unwrap();

    let mut v = Violations::default();
    check_grpc_request(&mut v, &rel, &read_yaml(&dir.path().join("api").join(&rel)));
    assert!(v.0.is_empty(), "schema violations:\n{}", v.0.join("\n"));
}
```

- [ ] **Step 14: Fix the exhaustive matches in `rocket-app` and add the service methods**

In `crates/rocket-app/src/runner_sequence.rs`, in `collect_items`, add `CollectionItem::Grpc(_)` to the skip arm Plan 05 changed, and say why in the comment:

```rust
            // Plans 12 and 13 do not run gRPC from the Collection Runner. GraphQL becomes a
            // run step in Plan 06. Until then these, the other protocols and sidebar
            // summaries are not executable.
            CollectionItem::GraphQl(_)
            | CollectionItem::Grpc(_)
            | CollectionItem::OpaqueItem(_)
            | CollectionItem::Summary(_) => {}
```

and add this test next to `opaque_protocol_items_are_never_steps`:

```rust
    #[test]
    fn grpc_items_are_never_steps() {
        let mut collection = Collection::new("my-api");
        collection.root.add_request(req("Login", "login.yml"));
        collection
            .root
            .items
            .push(rocket_collection::CollectionItem::Grpc(Box::new(
                rocket_collection::GrpcRequest::new("Say Hello", "localhost:50051"),
            )));

        let items = flatten_run_set(&collection, None).expect("flatten");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Login");
    }
```

In `crates/rocket-app/src/contract_service.rs`, in `walk_folder`, add next to Plan 05's `GraphQl` arm:

```rust
            // Contracts describe HTTP request signatures only.
            CollectionItem::Grpc(_) => {}
```

and add this test to its `tests` module:

```rust
    #[test]
    fn walk_folder_skips_grpc_items() {
        let mut folder = Folder::new("root");
        folder.items.push(CollectionItem::Grpc(Box::new(
            rocket_collection::GrpcRequest::new("Say Hello", "localhost:50051"),
        )));
        let mut out = Vec::new();
        walk_folder(&folder, Path::new(""), &mut out);
        assert!(out.is_empty());
    }
```

In `crates/rocket-app/src/collection_service.rs`, add `GrpcRequest` to the `rocket_collection` import and these methods after Plan 05's `save_graphql_request`:

```rust
    /// Get the full gRPC request at `path`.
    pub fn get_grpc_request(&self, collection: &str, path: &str) -> DomainResult<GrpcRequest> {
        self.repo.get_grpc_request(collection, path)
    }

    /// Save a gRPC request and return it as stored (the file name may differ from `path`).
    pub fn save_grpc_request(
        &self,
        collection: &str,
        path: &str,
        request: &GrpcRequest,
    ) -> DomainResult<GrpcRequest> {
        let actual_path = self.repo.save_grpc_request(collection, path, request)?;
        self.events.publish(DomainEvent::RequestSaved {
            collection: collection.to_string(),
            path: actual_path.clone(),
        });
        self.repo.get_grpc_request(collection, &actual_path)
    }
```

Plan 05 put a GraphQL branch at the top of `rename_request` and `update_request_docs`. Turn each into a `match` on `request_kind` so gRPC gets its own branch and the file is read once. At the top of `rename_request`:

```rust
        match self.repo.request_kind(collection, old_path)? {
            RequestKind::GraphQl => {
                // Plan 05's GraphQL branch, unchanged.
                let mut request = self.repo.get_graphql_request(collection, old_path)?;
                request.name = new_name.to_string();
                let actual_path = self.repo.save_graphql_request(collection, old_path, &request)?;
                self.events.publish(DomainEvent::RequestSaved {
                    collection: collection.to_string(),
                    path: actual_path,
                });
                return Ok(());
            }
            RequestKind::Grpc => {
                let mut request = self.repo.get_grpc_request(collection, old_path)?;
                request.name = new_name.to_string();
                let actual_path = self.repo.save_grpc_request(collection, old_path, &request)?;
                self.events.publish(DomainEvent::RequestSaved {
                    collection: collection.to_string(),
                    path: actual_path,
                });
                return Ok(());
            }
            RequestKind::Http | RequestKind::WebSocket => {}
        }
```

At the top of `update_request_docs`:

```rust
        match self.repo.request_kind(collection, path)? {
            RequestKind::GraphQl => {
                // Plan 05's GraphQL branch, unchanged.
                let mut request = self.repo.get_graphql_request(collection, path)?;
                request.docs = docs.map(Documentation::text);
                let actual_path = self.repo.save_graphql_request(collection, path, &request)?;
                self.events.publish(DomainEvent::RequestSaved {
                    collection: collection.to_string(),
                    path: actual_path,
                });
                return Ok(());
            }
            RequestKind::Grpc => {
                let mut request = self.repo.get_grpc_request(collection, path)?;
                request.docs = docs;
                let actual_path = self.repo.save_grpc_request(collection, path, &request)?;
                self.events.publish(DomainEvent::RequestSaved {
                    collection: collection.to_string(),
                    path: actual_path,
                });
                return Ok(());
            }
            RequestKind::Http | RequestKind::WebSocket => {}
        }
```

(If Plan 08 has landed, its WebSocket branch moves out of the `Http | WebSocket` arm into its own arm the same way.) Add these tests to the `tests` module of `collection_service.rs`:

```rust
    #[test]
    fn rename_request_keeps_a_grpc_item_grpc() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let repo = rocket_infra::FsCollectionRepo::new_standalone(dir.path().to_path_buf());
        repo.create("api").expect("create collection");
        let g = GrpcRequest::new("Old", "localhost:50051");
        repo.save_grpc_request("api", "call.yml", &g).expect("save");

        let svc = CollectionService::new(Box::new(repo), Box::new(NullEventPublisher));
        svc.rename_request("api", "call.yml", "New").expect("rename");

        let back = svc.get_grpc_request("api", "call.yml").expect("get");
        assert_eq!(back.name, "New");
        assert_eq!(back.url, "localhost:50051");
        assert_eq!(back.uid, g.uid);
    }

    #[test]
    fn update_request_docs_keeps_a_grpc_item_grpc() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let repo = rocket_infra::FsCollectionRepo::new_standalone(dir.path().to_path_buf());
        repo.create("api").expect("create collection");
        repo.save_grpc_request("api", "call.yml", &GrpcRequest::new("A", "h:1"))
            .expect("save");

        let svc = CollectionService::new(Box::new(repo), Box::new(NullEventPublisher));
        svc.update_request_docs("api", "call.yml", Some("Calls the greeter".into()))
            .expect("docs");

        let back = svc.get_grpc_request("api", "call.yml").expect("get");
        assert_eq!(back.docs.as_deref(), Some("Calls the greeter"));
    }

    #[test]
    fn save_grpc_request_returns_the_stored_request_and_publishes_an_event() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let repo = rocket_infra::FsCollectionRepo::new_standalone(dir.path().to_path_buf());
        repo.create("api").expect("create collection");
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(repo),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );

        let saved = svc
            .save_grpc_request("api", "call.yml", &GrpcRequest::new("A", "h:1"))
            .expect("save");
        assert_eq!(saved.file_name.as_deref(), Some("call.yml"));
        let events = publisher.events.lock().expect("lock");
        assert!(
            matches!(events.as_slice(), [DomainEvent::RequestSaved { path, .. }] if path == "call.yml"),
            "{events:?}"
        );
    }
```

The third test reuses `RecordingEventPublisher` and `SharedEventPublisher`, which the `tests` module already defines.

- [ ] **Step 15: Frontend types and guards**

The backend now sends `{ type: 'grpc' }` items in a full tree, and a gRPC file as a `summary` with `kind: 'grpc'` in the sidebar payload. Until Plan 13 renders them, a full-tree gRPC item must never reach `RequestNode`, which only takes request and summary items.

In `src/lib/tauri-api.ts`, add these types before `export interface Folder` (Plan 05 already added `RequestKind` and `RequestSummary.kind`), and extend `CollectionItem` with `| ({ type: 'grpc' } & GrpcRequest)` before the `opaque` member:

```ts
export type GrpcMethodType = 'unary' | 'client-streaming' | 'server-streaming' | 'bidi-streaming';

export interface GrpcMessage {
  /** Empty for the single untitled message of a simple request. */
  title: string;
  /** The message the editor shows first and a unary call sends. */
  selected: boolean;
  /** Protobuf JSON text. */
  content: string;
}

export interface GrpcScript {
  type: string;
  code: string;
}

/** A saved gRPC request. Empty lists are absent, like the Rust side skips them. */
export interface GrpcRequest {
  uid: string;
  name: string;
  fileName?: string;
  seq?: number;
  tags?: string[];
  /** Polymorphic on the Rust side (string or typed). Passed back unchanged. */
  description?: unknown;
  url: string;
  /** `package.Service/Method`. */
  method?: string;
  methodType: GrpcMethodType;
  protoFilePath?: string;
  metadata?: Header[];
  messages?: GrpcMessage[];
  auth: Auth;
  variables?: CollectionVariable[];
  scripts?: GrpcScript[];
  assertions?: AssertionEntry[];
  docs?: string | null;
}
```

Run `yarn tsc --noEmit`. It flags every site that switches on `CollectionItem['type']`. Make each of these three guards exclude `'grpc'` as well (Plan 05 Task 3 already excluded `'graphql'` in the same lines; add `'grpc'` next to it):

- `src/components/collections/CollectionNode.tsx` and `FolderNode.tsx`: the `filterableItems` filter becomes `item.type !== 'opaque' && item.type !== 'grpc'`, and the render guard becomes `if (item.type === 'opaque' || item.type === 'grpc') return null;`.
- `src/lib/contracts/collectPaths.ts`: the branch becomes `} else if (item.type !== 'opaque' && item.type !== 'grpc') {`.

Update the two comments above those guards to say: "Opaque and typed gRPC full-tree items never render here: the sidebar loads summaries, where a gRPC file arrives as a `summary` with `kind: 'grpc'`."

Add to `src/lib/contracts/collectPaths.test.ts`, inside the `describe`:

```ts
  it('skips typed gRPC items like opaque ones', () => {
    const items: CollectionItem[] = [
      {
        type: 'grpc',
        uid: 'g1',
        name: 'Say Hello',
        url: 'localhost:50051',
        methodType: 'unary',
        auth: { authType: 'none' },
        fileName: 'say-hello.yml',
      },
    ];
    const folders: string[] = [];
    const requests: string[] = [];
    collectPaths(items, '', folders, requests);
    expect(requests).toEqual([]);
  });
```

Append to `src/components/collections/__tests__/CollectionNode.test.tsx`:

```tsx
describe('CollectionNode full-tree gRPC items', () => {
  // A full tree (get_collection) carries typed gRPC items. The sidebar renders summaries, so a
  // typed item must never reach RequestNode, which only takes request and summary items.
  const collectionWithGrpcItem: tauriApi.Collection = {
    name: 'my-collection',
    root: {
      uid: 'root',
      name: 'my-collection',
      items: [
        {
          type: 'summary',
          uid: 'req-1',
          name: 'List Orders',
          method: 'GET',
          url: 'https://api.example.com/orders',
          fileName: 'list-orders.yml',
        },
        {
          type: 'grpc',
          uid: 'g-1',
          name: 'Say Hello',
          url: 'localhost:50051',
          methodType: 'unary',
          auth: { authType: 'none' },
          fileName: 'say-hello.yml',
        },
      ],
    },
    settings: { headers: [], variables: [], sandboxMode: 'safe' },
  };

  beforeEach(() => {
    vi.mocked(tauriApi.getCollectionSummaries).mockResolvedValue(collectionWithGrpcItem);
    usePaneStore.setState({ activeCollection: summary.name });
  });

  it('does not render a typed gRPC item as a request row', async () => {
    renderNode();

    await waitFor(() => {
      expect(screen.getByTestId('request-item-GET-List Orders')).toBeInTheDocument();
    });
    expect(screen.queryByText('Say Hello')).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 16: Docs**

In `crates/rocket-collection/CLAUDE.md`: add a `Grpc — a saved gRPC request` line to the Domain Model tree, change the opaque line to "OpaqueProtocolItem — raw YAML passthrough for WebSocket", and add `"grpc"` to the `CollectionItem serde tag` bullet.

In `crates/rocket-infra/CLAUDE.md`, "Internal modules": say that the repo round-trips `OcHttpRequest`, `OcGraphQLRequest` and `OcGrpcRequest`, and only WebSocket lands as `OpaqueProtocolItem`.

In `docs/superpowers/specs/opencollection-spec-reference.md`, section 2.5, replace the YAML block with the shape the published schema defines (`auth` inside `grpc`, nothing but `variables`, `scripts` and `assertions` in `runtime`, no top-level `uid` or `settings`):

```yaml
grpc:
  url: string
  method: string        # full RPC name: "package.Service/Method"
  methodType: "unary" | "client-streaming" | "server-streaming" | "bidi-streaming"
  protoFilePath: string
  metadata: [GrpcMetadata]
  message: GrpcMessage | [GrpcMessageVariant]   # GrpcMessage is a string
  auth: Auth
runtime:
  variables, scripts, assertions
docs: string
```

Also add one sentence under the table of known deviations (or in the section itself): "Rocket also writes a top-level `uid` for gRPC requests, the same deviation as HTTP requests (`KNOWN_DEFERRED` in `schema_shape_tests.rs`)."

- [ ] **Step 17: Run the checks**

Run:
- `cargo check -j4 -p rocket-collection -p rocket-infra -p rocket-app`
- `cargo test -j4 -p rocket-collection`
- `cargo test -j4 -p rocket-infra conversions`
- `cargo test -j4 -p rocket-infra fs_collection`
- `cargo test -j4 -p rocket-app collection_service`
- `cargo test -j4 -p rocket-app runner_sequence`
- `cargo test -j4 -p rocket-app contract_service`
- `yarn test collectPaths CollectionNode`
- `yarn tsc --noEmit`
- `yarn check`

Expected: PASS everywhere. If `cargo check` reports another non-exhaustive match on `CollectionItem`, add a `Grpc` arm that skips it and mention the file in the commit body.

- [ ] **Step 18: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path:

```bash
git add crates/rocket-collection crates/rocket-infra/src crates/rocket-infra/CLAUDE.md \
  crates/rocket-app/src/runner_sequence.rs crates/rocket-app/src/contract_service.rs \
  crates/rocket-app/src/collection_service.rs \
  src/lib/tauri-api.ts src/lib/contracts src/components/collections \
  docs/superpowers/specs/opencollection-spec-reference.md
```

Suggested subject: `feat(collection): add a typed gRPC request item and persistence`.

---

## Task 2: `ProtoRegistry`, `ProtoFileReader` and the file loader

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `Cargo.toml` (workspace members and dependencies)
- Create: `crates/rocket-grpc/Cargo.toml`, `src/lib.rs`, `src/registry.rs`, `src/test_support.rs`, `CLAUDE.md`
- Modify: `crates/rocket-infra/Cargo.toml`, `src/lib.rs`
- Create: `crates/rocket-infra/src/grpc/mod.rs`, `src/grpc/proto_reader.rs`
- Create fixtures: `crates/rocket-infra/test-fixtures/grpc/common.proto`, `greeter.proto`

**Interfaces:**
- Consumes: `rocket_collection::GrpcMethodType` (Task 1), `rocket_shared::error::{DomainError, DomainResult}`.
- Produces:
  - `rocket_grpc::ProtoFileReader` (`fn read(&self, name: &str) -> Option<String>`), `ProtoLoader` (`fn load(&self, proto_file: &Path, extra_include_dirs: &[PathBuf]) -> DomainResult<ProtoRegistry>`).
  - `rocket_grpc::ProtoRegistry` (cheap to clone) with `compile(entry: &str, reader: Arc<dyn ProtoFileReader>) -> DomainResult<Self>`, `from_file_descriptors(files: Vec<FileDescriptorProto>) -> DomainResult<Self>`, `pool(&self) -> &DescriptorPool`, `services(&self) -> Vec<GrpcServiceInfo>`, `method(&self, full_name: &str) -> DomainResult<MethodDescriptor>`.
  - `rocket_grpc::GrpcServiceInfo { name, methods }` and `GrpcMethodInfo { name, full_name, method_type, input_type, output_type }`, camelCase view types for IPC.
  - `rocket_infra::{FsProtoFileReader, FsProtoLoader}`.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.** The relevant facts are `protoFilePath` and `method` in section 2.5.

- [ ] **Step 2: Create the crate skeleton**

In the workspace `Cargo.toml`, add `"crates/rocket-grpc",` to `members` (after `"crates/rocket-flow",`) and this line to `[workspace.dependencies]` under "Internal crates":

```toml
rocket-grpc = { path = "crates/rocket-grpc" }
```

Create `crates/rocket-grpc/Cargo.toml`:

```toml
[package]
name = "rocket-grpc"
version.workspace = true
edition.workspace = true

[dependencies]
rocket-shared.workspace = true
rocket-collection.workspace = true
serde.workspace = true
serde_json.workspace = true
prost-types = "0.14"
prost-reflect = { version = "0.16", features = ["serde"] }
protox = "0.9"
```

Create `crates/rocket-grpc/src/lib.rs`:

```rust
//! gRPC protocol engine: parses `.proto` sources into descriptors. It holds no
//! network or file I/O; the concrete file reader lives in `rocket-infra`.

pub mod registry;

#[cfg(test)]
mod test_support;

pub use prost_reflect::MethodDescriptor;
pub use registry::{GrpcMethodInfo, GrpcServiceInfo, ProtoFileReader, ProtoLoader, ProtoRegistry};
```

Create `crates/rocket-grpc/src/test_support.rs`. It holds the proto fixtures and an in-memory reader that Task 3 and Plan 12 reuse:

```rust
//! Shared fixtures for the unit tests in this crate.

use std::collections::HashMap;
use std::sync::Arc;

use crate::registry::{ProtoFileReader, ProtoRegistry};

pub const COMMON_PROTO: &str = r#"syntax = "proto3";
package demo.common;
enum Mood { MOOD_UNSPECIFIED = 0; HAPPY = 1; GRUMPY = 2; }
message Address { string street = 1; string city = 2; }
"#;

pub const GREETER_PROTO: &str = r#"syntax = "proto3";
package demo.greeter.v1;
import "common.proto";
import "google/protobuf/timestamp.proto";

service Greeter {
  rpc SayHello (HelloRequest) returns (HelloReply);
  rpc ListGreetings (HelloRequest) returns (stream HelloReply);
  rpc CollectNames (stream HelloRequest) returns (HelloReply);
  rpc Chat (stream HelloRequest) returns (stream HelloReply);
}

message HelloRequest {
  string name = 1;
  repeated string tags = 2;
  demo.common.Mood mood = 3;
  map<string, int64> counters = 4;
  oneof contact { string email = 5; string phone = 6; }
  demo.common.Address address = 7;
  google.protobuf.Timestamp sent_at = 8;
  bytes blob = 9;
  int64 big = 10;
  repeated demo.common.Address stops = 11;
  map<int32, demo.common.Address> by_id = 12;
}

message HelloReply { string message = 1; int32 sequence = 2; }
"#;

pub struct MemReader(pub HashMap<String, String>);

impl ProtoFileReader for MemReader {
    fn read(&self, name: &str) -> Option<String> {
        self.0.get(name).cloned()
    }
}

pub fn reader(files: &[(&str, &str)]) -> Arc<MemReader> {
    Arc::new(MemReader(
        files
            .iter()
            .map(|(n, s)| (n.to_string(), s.to_string()))
            .collect(),
    ))
}

pub fn greeter_registry() -> ProtoRegistry {
    ProtoRegistry::compile(
        "greeter.proto",
        reader(&[
            ("greeter.proto", GREETER_PROTO),
            ("common.proto", COMMON_PROTO),
        ]),
    )
    .expect("greeter fixture compiles")
}
```

- [ ] **Step 3: Write the failing registry tests**

Create `crates/rocket-grpc/src/registry.rs` with only this test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{greeter_registry, reader, COMMON_PROTO, GREETER_PROTO};

    #[test]
    fn lists_services_methods_and_their_call_shapes() {
        let services = greeter_registry().services();
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].name, "demo.greeter.v1.Greeter");
        let shapes: Vec<(&str, GrpcMethodType)> = services[0]
            .methods
            .iter()
            .map(|m| (m.name.as_str(), m.method_type))
            .collect();
        assert_eq!(
            shapes,
            vec![
                ("SayHello", GrpcMethodType::Unary),
                ("ListGreetings", GrpcMethodType::ServerStreaming),
                ("CollectNames", GrpcMethodType::ClientStreaming),
                ("Chat", GrpcMethodType::BidiStreaming),
            ]
        );
        assert_eq!(
            services[0].methods[0].full_name,
            "demo.greeter.v1.Greeter/SayHello"
        );
        assert_eq!(
            services[0].methods[0].input_type,
            "demo.greeter.v1.HelloRequest"
        );
    }

    #[test]
    fn method_lookup_accepts_a_leading_slash_and_rejects_bad_names() {
        let registry = greeter_registry();
        assert!(registry.method("/demo.greeter.v1.Greeter/Chat").is_ok());
        assert!(matches!(
            registry.method("demo.greeter.v1.Greeter"),
            Err(DomainError::InvalidInput(_))
        ));
        assert!(matches!(
            registry.method("demo.greeter.v1.Greeter/Nope"),
            Err(DomainError::NotFound(_))
        ));
        assert!(matches!(
            registry.method("other.Service/Chat"),
            Err(DomainError::NotFound(_))
        ));
    }

    #[test]
    fn a_missing_import_is_an_invalid_input_that_names_the_file() {
        let result =
            ProtoRegistry::compile("greeter.proto", reader(&[("greeter.proto", GREETER_PROTO)]));
        match result {
            Err(DomainError::InvalidInput(msg)) => {
                assert!(msg.contains("common.proto"), "got: {msg}");
                assert!(msg.contains("greeter.proto"), "got: {msg}");
            }
            other => panic!("expected InvalidInput, got {:?}", other.err()),
        }
    }

    #[test]
    fn a_syntax_error_is_an_invalid_input() {
        let result = ProtoRegistry::compile(
            "bad.proto",
            reader(&[("bad.proto", "syntax = \"proto3\";\nmessage {")]),
        );
        assert!(matches!(result, Err(DomainError::InvalidInput(_))));
    }

    #[test]
    fn an_entry_that_the_reader_does_not_have_is_an_error() {
        let result = ProtoRegistry::compile("nope.proto", reader(&[]));
        assert!(matches!(result, Err(DomainError::InvalidInput(_))));
    }

    #[test]
    fn descriptors_in_reverse_order_still_build_a_registry() {
        let compiled = ProtoRegistry::compile(
            "greeter.proto",
            reader(&[
                ("greeter.proto", GREETER_PROTO),
                ("common.proto", COMMON_PROTO),
            ]),
        )
        .expect("compile");
        let mut files: Vec<FileDescriptorProto> =
            compiled.pool().file_descriptor_protos().cloned().collect();
        files.reverse();
        let rebuilt = ProtoRegistry::from_file_descriptors(files).expect("rebuild");
        assert_eq!(rebuilt.services(), compiled.services());
    }

    #[test]
    fn missing_well_known_files_are_added_when_rebuilding() {
        let compiled = ProtoRegistry::compile(
            "greeter.proto",
            reader(&[
                ("greeter.proto", GREETER_PROTO),
                ("common.proto", COMMON_PROTO),
            ]),
        )
        .expect("compile");
        let files: Vec<FileDescriptorProto> = compiled
            .pool()
            .file_descriptor_protos()
            .filter(|f| !f.name().starts_with("google/protobuf/"))
            .cloned()
            .collect();
        let rebuilt = ProtoRegistry::from_file_descriptors(files).expect("rebuild");
        assert!(rebuilt
            .pool()
            .get_message_by_name("google.protobuf.Timestamp")
            .is_some());
    }
}
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-grpc registry`
Expected: FAIL to compile (`ProtoRegistry`, `ProtoFileReader` and `GrpcServiceInfo` are not defined). The first build downloads and compiles `protox` and `prost-reflect`, which takes a few minutes.

- [ ] **Step 5: Implement the registry**

Put this above the test module in `crates/rocket-grpc/src/registry.rs`:

```rust
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use prost_reflect::{DescriptorPool, MethodDescriptor};
use prost_types::{FileDescriptorProto, FileDescriptorSet};
use protox::file::{ChainFileResolver, File, FileResolver, GoogleFileResolver};
use protox::Compiler;
use rocket_collection::GrpcMethodType;
use rocket_shared::error::{DomainError, DomainResult};
use serde::Serialize;

/// Reads `.proto` source text by import name, for example `common.proto`.
/// Returning `None` means the file is not available.
pub trait ProtoFileReader: Send + Sync {
    fn read(&self, name: &str) -> Option<String>;
}

/// Loads a registry from a `.proto` file on disk. Implemented in `rocket-infra`.
pub trait ProtoLoader: Send + Sync {
    /// `proto_file` is an absolute path. `extra_include_dirs` are searched for
    /// imports after the directory that holds `proto_file`.
    fn load(
        &self,
        proto_file: &Path,
        extra_include_dirs: &[PathBuf],
    ) -> DomainResult<ProtoRegistry>;
}

/// One RPC method, as shown in the method picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcMethodInfo {
    pub name: String,
    /// `package.Service/Method`, the value stored in `GrpcRequest.method`.
    pub full_name: String,
    pub method_type: GrpcMethodType,
    pub input_type: String,
    pub output_type: String,
}

/// One service and its methods.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcServiceInfo {
    pub name: String,
    pub methods: Vec<GrpcMethodInfo>,
}

struct ReaderResolver(Arc<dyn ProtoFileReader>);

impl FileResolver for ReaderResolver {
    fn open_file(&self, name: &str) -> Result<File, protox::Error> {
        match self.0.read(name) {
            Some(source) => File::from_source(name, &source),
            None => Err(protox::Error::file_not_found(name)),
        }
    }
}

/// A set of parsed protobuf descriptors. Cheap to clone.
#[derive(Clone)]
pub struct ProtoRegistry {
    pool: DescriptorPool,
}

impl ProtoRegistry {
    /// Compiles `entry` and everything it imports. Well-known imports such as
    /// `google/protobuf/timestamp.proto` resolve without the reader.
    pub fn compile(entry: &str, reader: Arc<dyn ProtoFileReader>) -> DomainResult<Self> {
        let mut resolver = ChainFileResolver::new();
        resolver.add(ReaderResolver(reader));
        resolver.add(GoogleFileResolver::new());
        let mut compiler = Compiler::with_file_resolver(resolver);
        compiler.include_imports(true);
        compiler
            .open_file(entry)
            .map_err(|e| DomainError::InvalidInput(format!("could not compile '{entry}': {e}")))?;
        Ok(Self {
            pool: compiler.descriptor_pool(),
        })
    }

    /// Builds a registry from descriptors in any order, such as the files a
    /// reflection server returns. Every import must be in `files`, except the
    /// well-known `google/protobuf/*` files, which are added when missing.
    pub fn from_file_descriptors(files: Vec<FileDescriptorProto>) -> DomainResult<Self> {
        let mut by_name: HashMap<String, FileDescriptorProto> = HashMap::new();
        for file in files {
            by_name.insert(file.name().to_string(), file);
        }
        let mut ordered = Vec::new();
        let mut seen = HashSet::new();
        let mut names: Vec<String> = by_name.keys().cloned().collect();
        names.sort();
        for name in names {
            visit(&name, &by_name, &mut seen, &mut ordered);
        }
        let mut pool = DescriptorPool::new();
        for file in well_known_files_needed(&ordered)? {
            pool.add_file_descriptor_proto(file)
                .map_err(|e| DomainError::InvalidInput(format!("invalid descriptors: {e}")))?;
        }
        pool.add_file_descriptor_set(FileDescriptorSet { file: ordered })
            .map_err(|e| DomainError::InvalidInput(format!("invalid descriptors: {e}")))?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &DescriptorPool {
        &self.pool
    }

    /// All services with their methods, sorted by service name then source order.
    pub fn services(&self) -> Vec<GrpcServiceInfo> {
        let mut out: Vec<GrpcServiceInfo> = self
            .pool
            .services()
            .map(|service| GrpcServiceInfo {
                name: service.full_name().to_string(),
                methods: service
                    .methods()
                    .map(|m| GrpcMethodInfo {
                        name: m.name().to_string(),
                        full_name: format!("{}/{}", service.full_name(), m.name()),
                        method_type: GrpcMethodType::from_streaming_flags(
                            m.is_client_streaming(),
                            m.is_server_streaming(),
                        ),
                        input_type: m.input().full_name().to_string(),
                        output_type: m.output().full_name().to_string(),
                    })
                    .collect(),
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Finds a method by `package.Service/Method`. A leading `/` is accepted.
    pub fn method(&self, full_name: &str) -> DomainResult<MethodDescriptor> {
        let trimmed = full_name.trim_start_matches('/');
        let (service, method) = trimmed.split_once('/').ok_or_else(|| {
            DomainError::InvalidInput(format!(
                "method '{full_name}' must look like package.Service/Method"
            ))
        })?;
        self.pool
            .get_service_by_name(service)
            .and_then(|s| s.methods().find(|m| m.name() == method))
            .ok_or_else(|| DomainError::NotFound(format!("gRPC method '{trimmed}'")))
    }
}

fn visit(
    name: &str,
    files: &HashMap<String, FileDescriptorProto>,
    seen: &mut HashSet<String>,
    out: &mut Vec<FileDescriptorProto>,
) {
    if !seen.insert(name.to_string()) {
        return;
    }
    if let Some(file) = files.get(name) {
        for dependency in &file.dependency {
            visit(dependency, files, seen, out);
        }
        out.push(file.clone());
    }
}

/// Returns the well-known files that `ordered` imports but does not contain.
fn well_known_files_needed(
    ordered: &[FileDescriptorProto],
) -> DomainResult<Vec<FileDescriptorProto>> {
    let present: HashSet<&str> = ordered.iter().map(|f| f.name()).collect();
    let mut missing: Vec<String> = ordered
        .iter()
        .flat_map(|f| f.dependency.iter().cloned())
        .filter(|d| d.starts_with("google/protobuf/") && !present.contains(d.as_str()))
        .collect();
    missing.sort();
    missing.dedup();
    if missing.is_empty() {
        return Ok(Vec::new());
    }
    let mut compiler = Compiler::with_file_resolver(GoogleFileResolver::new());
    compiler.include_imports(true);
    for name in &missing {
        compiler
            .open_file(name)
            .map_err(|e| DomainError::InvalidInput(format!("could not load '{name}': {e}")))?;
    }
    Ok(compiler.file_descriptor_set().file)
}
```

`ProtoRegistry::compile` takes the entry file by name, not by path: the reader decides where names live. `from_file_descriptors` exists for reflection (Plan 12), where a server returns descriptors in no particular order and may leave out the well-known `google/protobuf` files.

- [ ] **Step 6: Run the registry tests to verify they pass**

Run: `cargo test -j4 -p rocket-grpc registry`
Expected: PASS (7 tests).

- [ ] **Step 7: Write the failing file loader tests**

Create the fixtures. `crates/rocket-infra/test-fixtures/grpc/common.proto`:

```proto
syntax = "proto3";
package demo.common;
enum Mood { MOOD_UNSPECIFIED = 0; HAPPY = 1; GRUMPY = 2; }
message Address { string street = 1; string city = 2; }
```

`crates/rocket-infra/test-fixtures/grpc/greeter.proto`:

```proto
syntax = "proto3";
package demo.greeter.v1;
import "common.proto";
import "google/protobuf/timestamp.proto";

service Greeter {
  rpc SayHello (HelloRequest) returns (HelloReply);
  rpc ListGreetings (HelloRequest) returns (stream HelloReply);
  rpc CollectNames (stream HelloRequest) returns (HelloReply);
  rpc Chat (stream HelloRequest) returns (stream HelloReply);
}

message HelloRequest {
  string name = 1;
  repeated string tags = 2;
  demo.common.Mood mood = 3;
  map<string, int64> counters = 4;
  oneof contact { string email = 5; string phone = 6; }
  demo.common.Address address = 7;
  google.protobuf.Timestamp sent_at = 8;
  bytes blob = 9;
  int64 big = 10;
  repeated demo.common.Address stops = 11;
  map<int32, demo.common.Address> by_id = 12;
}

message HelloReply { string message = 1; int32 sequence = 2; }
```

In `crates/rocket-infra/Cargo.toml` add `rocket-grpc = { path = "../rocket-grpc" }` under `[dependencies]` (next to `rocket-flow.workspace = true`). Create `crates/rocket-infra/src/grpc/proto_reader.rs` with only this test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::GrpcMethodType;
    use tempfile::TempDir;

    fn fixtures() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-fixtures/grpc")
    }

    #[test]
    fn loads_the_greeter_fixture_with_its_imports() {
        let registry = FsProtoLoader
            .load(&fixtures().join("greeter.proto"), &[])
            .expect("load");
        let services = registry.services();
        assert_eq!(services[0].name, "demo.greeter.v1.Greeter");
        assert_eq!(
            services[0].methods[3].method_type,
            GrpcMethodType::BidiStreaming
        );
    }

    #[test]
    fn extra_include_dirs_resolve_imports_that_live_elsewhere() {
        let dir = TempDir::new().expect("tempdir");
        let shared = dir.path().join("protos");
        let nested = shared.join("deep");
        fs::create_dir_all(&nested).expect("mkdir");
        fs::write(
            shared.join("shared.proto"),
            "syntax = \"proto3\";\nmessage S { string v = 1; }\n",
        )
        .expect("write");
        fs::write(
            nested.join("deep.proto"),
            "syntax = \"proto3\";\nimport \"protos/shared.proto\";\nmessage D { S s = 1; }\n",
        )
        .expect("write");

        let entry = nested.join("deep.proto");
        assert!(
            FsProtoLoader.load(&entry, &[]).is_err(),
            "the import is not under the proto's own directory"
        );
        assert!(FsProtoLoader
            .load(&entry, &[dir.path().to_path_buf()])
            .is_ok());
    }

    #[test]
    fn a_missing_proto_file_is_not_found() {
        let err = FsProtoLoader
            .load(&fixtures().join("nope.proto"), &[])
            .err()
            .expect("error");
        assert!(matches!(err, DomainError::NotFound(_)), "got: {err:?}");
    }

    #[test]
    fn imports_cannot_escape_the_include_directories() {
        let dir = TempDir::new().expect("tempdir");
        let inner = dir.path().join("inner");
        fs::create_dir_all(&inner).expect("mkdir");
        fs::write(dir.path().join("secret.proto"), "syntax = \"proto3\";\n").expect("write");
        let reader = FsProtoFileReader::new(vec![inner.clone()]);
        assert!(reader.read("../secret.proto").is_none());
        assert!(reader
            .read(&dir.path().join("secret.proto").to_string_lossy())
            .is_none());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_that_leaves_the_include_directory_is_not_followed() {
        let dir = TempDir::new().expect("tempdir");
        let inner = dir.path().join("inner");
        fs::create_dir_all(&inner).expect("mkdir");
        fs::write(dir.path().join("outside.proto"), "syntax = \"proto3\";\n").expect("write");
        std::os::unix::fs::symlink(dir.path().join("outside.proto"), inner.join("link.proto"))
            .expect("symlink");
        let reader = FsProtoFileReader::new(vec![inner]);
        assert!(reader.read("link.proto").is_none());
    }
}
```

Create `crates/rocket-infra/src/grpc/mod.rs`:

```rust
//! gRPC transport and `.proto` file access.

mod proto_reader;

pub use proto_reader::{FsProtoFileReader, FsProtoLoader};
```

In `crates/rocket-infra/src/lib.rs` add `pub mod grpc;` after `pub mod fs_workspace_repo;` and, with the other re-exports:

```rust
pub use grpc::{FsProtoFileReader, FsProtoLoader};
```

- [ ] **Step 8: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-infra grpc::proto_reader`
Expected: FAIL to compile (`FsProtoFileReader` and `FsProtoLoader` not defined).

- [ ] **Step 9: Implement the reader and loader**

Put this above the test module in `crates/rocket-infra/src/grpc/proto_reader.rs`:

```rust
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use rocket_grpc::{ProtoFileReader, ProtoLoader, ProtoRegistry};
use rocket_shared::error::{DomainError, DomainResult};

/// Reads `.proto` imports from a list of include directories.
///
/// An import name must be a plain relative path. A name with `..`, a root or a
/// drive prefix is never read, and neither is a file whose real path (after
/// symlinks) leaves its include directory.
pub struct FsProtoFileReader {
    include_dirs: Vec<PathBuf>,
}

impl FsProtoFileReader {
    pub fn new(include_dirs: Vec<PathBuf>) -> Self {
        Self { include_dirs }
    }
}

impl ProtoFileReader for FsProtoFileReader {
    fn read(&self, name: &str) -> Option<String> {
        let relative = Path::new(name);
        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            return None;
        }
        for dir in &self.include_dirs {
            let Ok(base) = dir.canonicalize() else {
                continue;
            };
            let Ok(real) = dir.join(relative).canonicalize() else {
                continue;
            };
            if !real.starts_with(&base) || !real.is_file() {
                continue;
            }
            if let Ok(text) = fs::read_to_string(&real) {
                return Some(text);
            }
        }
        None
    }
}

/// Compiles a `.proto` file from disk.
pub struct FsProtoLoader;

impl ProtoLoader for FsProtoLoader {
    fn load(
        &self,
        proto_file: &Path,
        extra_include_dirs: &[PathBuf],
    ) -> DomainResult<ProtoRegistry> {
        if !proto_file.is_file() {
            return Err(DomainError::NotFound(format!(
                "proto file '{}'",
                proto_file.display()
            )));
        }
        let parent = proto_file.parent().ok_or_else(|| {
            DomainError::InvalidInput(format!("'{}' has no directory", proto_file.display()))
        })?;
        let entry = proto_file
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| {
                DomainError::InvalidInput(format!("'{}' is not a file name", proto_file.display()))
            })?;
        let mut dirs = vec![parent.to_path_buf()];
        dirs.extend(extra_include_dirs.iter().cloned());
        ProtoRegistry::compile(entry, Arc::new(FsProtoFileReader::new(dirs)))
    }
}
```

`FsProtoLoader::load` searches the proto file's own directory first and then `extra_include_dirs`, so `import "common.proto"` works next to the file and `import "protos/common.proto"` works when the collection root is an include directory (Plan 12 passes it).

- [ ] **Step 10: Run the loader tests to verify they pass**

Run: `cargo test -j4 -p rocket-infra grpc::proto_reader`
Expected: PASS (5 tests; the symlink test only runs on Unix).

- [ ] **Step 11: Document the crate**

Create `crates/rocket-grpc/CLAUDE.md`:

````markdown
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
| `call.rs` | Plan 12: `GrpcCall`, `GrpcExecutor`, the status and stream event types. |

## Rules

- No file or network access here. A `.proto` is read through `ProtoFileReader`. The filesystem implementation (`FsProtoFileReader`, `FsProtoLoader`) and the transport (`TonicGrpcExecutor`) live in `rocket-infra`.
- Errors are `DomainError::InvalidInput` for anything the user typed (bad proto, bad JSON), `NotFound` for an unknown method.
- `GrpcServiceInfo` and `GrpcMethodInfo` are IPC view types with camelCase serde. They are never persisted.
- Tests use the in-memory `MemReader` and the greeter proto in `test_support.rs`. The same proto is on disk in `crates/rocket-infra/test-fixtures/grpc` for the transport tests.
````

- [ ] **Step 12: Run the checks**

Run:
- `cargo check -j4 -p rocket-grpc -p rocket-infra`
- `cargo test -j4 -p rocket-grpc`
- `cargo test -j4 -p rocket-infra grpc`
- `git diff Cargo.lock` and confirm it only adds packages.

Expected: PASS. The first `cargo check -j4 -p rocket-infra` after the dependency change may take several minutes because of `deno_core`.

- [ ] **Step 13: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path:

```bash
git add Cargo.toml Cargo.lock crates/rocket-grpc crates/rocket-infra/Cargo.toml \
  crates/rocket-infra/src/lib.rs crates/rocket-infra/src/grpc crates/rocket-infra/test-fixtures/grpc
```

Suggested subject: `feat(grpc): parse .proto files into a descriptor registry`.

---

## Task 3: Dynamic JSON to protobuf codec

**Files:**
- Modify: `crates/rocket-grpc/Cargo.toml`, `src/lib.rs`
- Create: `crates/rocket-grpc/src/codec.rs`

**Interfaces:**
- Consumes: `ProtoRegistry` and the test fixtures from Task 2, `prost_reflect::{DynamicMessage, MessageDescriptor}`.
- Produces:
  - `rocket_grpc::json_to_message(desc: &MessageDescriptor, json: &str) -> DomainResult<DynamicMessage>`. Blank text is `{}`. Every failure is `DomainError::InvalidInput` that names the message type.
  - `rocket_grpc::message_to_json(message: &DynamicMessage) -> DomainResult<String>`. Pretty JSON, default-valued fields included, 64-bit integers as strings.
  - `rocket_grpc::empty_message_json(desc: &MessageDescriptor) -> DomainResult<String>`.
  - Re-exports `rocket_grpc::{DynamicMessage, MessageDescriptor}`.

- [ ] **Step 1: Write the failing codec tests**

In `crates/rocket-grpc/Cargo.toml` add a dev-dependency (the tests encode and decode bytes with `prost::Message`):

```toml
[dev-dependencies]
prost = "0.14"
```

Create `crates/rocket-grpc/src/codec.rs` with only this test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::greeter_registry;
    use prost::Message;

    fn hello_request() -> MessageDescriptor {
        greeter_registry()
            .pool()
            .get_message_by_name("demo.greeter.v1.HelloRequest")
            .expect("HelloRequest")
    }

    const FULL: &str = r#"{
        "name": "ada",
        "tags": ["a", "b"],
        "mood": "HAPPY",
        "counters": {"x": 5},
        "email": "ada@example.com",
        "address": {"street": "1 Main", "city": "Paris"},
        "sentAt": "2024-01-02T03:04:05Z",
        "blob": "aGk=",
        "big": "9007199254740993",
        "stops": [{"city": "Rome"}, {"city": "Oslo"}],
        "byId": {"7": {"city": "Kyiv"}}
    }"#;

    #[test]
    fn a_full_message_survives_json_to_bytes_to_json() {
        let desc = hello_request();
        let message = json_to_message(&desc, FULL).expect("parse");
        let bytes = message.encode_to_vec();
        let decoded = DynamicMessage::decode(desc, bytes.as_slice()).expect("decode");
        let json: serde_json::Value =
            serde_json::from_str(&message_to_json(&decoded).expect("write")).expect("json");
        assert_eq!(json["name"], "ada");
        assert_eq!(json["tags"], serde_json::json!(["a", "b"]));
        assert_eq!(json["mood"], "HAPPY");
        assert_eq!(json["counters"]["x"], "5");
        assert_eq!(json["email"], "ada@example.com");
        assert_eq!(json["address"]["city"], "Paris");
        assert_eq!(json["sentAt"], "2024-01-02T03:04:05Z");
        assert_eq!(json["blob"], "aGk=");
        assert_eq!(json["stops"][1]["city"], "Oslo");
        assert_eq!(json["byId"]["7"]["city"], "Kyiv");
    }

    #[test]
    fn sixty_four_bit_integers_keep_every_digit() {
        let desc = hello_request();
        let message = json_to_message(&desc, r#"{"big": "9007199254740993"}"#).expect("parse");
        let json: serde_json::Value =
            serde_json::from_str(&message_to_json(&message).expect("write")).expect("json");
        assert_eq!(json["big"], "9007199254740993");
    }

    #[test]
    fn an_enum_is_accepted_by_name_or_number_and_written_by_name() {
        let desc = hello_request();
        for text in [r#"{"mood":"GRUMPY"}"#, r#"{"mood":2}"#] {
            let message = json_to_message(&desc, text).expect("parse");
            assert!(message_to_json(&message)
                .expect("write")
                .contains("\"GRUMPY\""));
        }
    }

    #[test]
    fn two_members_of_one_oneof_are_rejected() {
        let desc = hello_request();
        let err =
            json_to_message(&desc, r#"{"email":"a@b.c","phone":"1"}"#).expect_err("oneof conflict");
        assert!(
            matches!(&err, DomainError::InvalidInput(m) if m.contains("oneof 'contact'")),
            "got: {err:?}"
        );
    }

    #[test]
    fn bad_input_is_an_invalid_input_that_names_the_message_type() {
        let desc = hello_request();
        for text in [
            r#"{"nope": 1}"#,
            r#"{"mood": "SAD"}"#,
            r#"{"tags": "x"}"#,
            r#"{} trailing"#,
            r#"{"name": "#,
        ] {
            let err = json_to_message(&desc, text).expect_err(text);
            assert!(
                matches!(&err, DomainError::InvalidInput(m) if m.contains("demo.greeter.v1.HelloRequest")),
                "{text}: {err:?}"
            );
        }
    }

    #[test]
    fn blank_text_is_an_empty_message() {
        let desc = hello_request();
        for text in ["", "   \n"] {
            let message = json_to_message(&desc, text).expect("blank");
            assert!(message.encode_to_vec().is_empty());
        }
    }

    #[test]
    fn the_template_lists_every_plain_field_at_its_default() {
        let json: serde_json::Value =
            serde_json::from_str(&empty_message_json(&hello_request()).expect("template"))
                .expect("json");
        assert_eq!(json["name"], "");
        assert_eq!(json["tags"], serde_json::json!([]));
        assert_eq!(json["mood"], "MOOD_UNSPECIFIED");
        assert_eq!(json["counters"], serde_json::json!({}));
        assert_eq!(json["big"], "0");
        assert!(
            json.get("email").is_none(),
            "unset oneof members are omitted"
        );
    }

    #[test]
    fn snake_case_field_names_are_accepted_on_input() {
        let desc = hello_request();
        let message =
            json_to_message(&desc, r#"{"by_id": {"1": {"city": "Rome"}}}"#).expect("snake case");
        assert!(message_to_json(&message)
            .expect("write")
            .contains("\"byId\""));
    }
}
```

Replace `crates/rocket-grpc/src/lib.rs` with:

```rust
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-grpc codec`
Expected: FAIL to compile (`json_to_message`, `message_to_json` and `empty_message_json` not defined).

- [ ] **Step 3: Implement the codec**

Put this above the test module in `crates/rocket-grpc/src/codec.rs`:

```rust
use prost_reflect::{DynamicMessage, MessageDescriptor, SerializeOptions};
use rocket_shared::error::{DomainError, DomainResult};

/// Parses `json` as the protobuf JSON mapping of `desc`. Blank text counts as
/// `{}`. Unknown fields, bad enum names and conflicting oneof members fail.
pub fn json_to_message(desc: &MessageDescriptor, json: &str) -> DomainResult<DynamicMessage> {
    let text = if json.trim().is_empty() { "{}" } else { json };
    let invalid =
        |e: String| DomainError::InvalidInput(format!("invalid {} message: {e}", desc.full_name()));
    let mut de = serde_json::Deserializer::from_str(text);
    let message =
        DynamicMessage::deserialize(desc.clone(), &mut de).map_err(|e| invalid(e.to_string()))?;
    de.end().map_err(|e| invalid(e.to_string()))?;
    Ok(message)
}

/// Pretty JSON for `message`. Fields that hold their default value are written,
/// so the result doubles as an editable template. 64-bit integers are strings.
pub fn message_to_json(message: &DynamicMessage) -> DomainResult<String> {
    let options = SerializeOptions::new().skip_default_fields(false);
    let mut out = Vec::new();
    let mut serializer = serde_json::Serializer::pretty(&mut out);
    message
        .serialize_with_options(&mut serializer, &options)
        .map_err(|e| DomainError::Serialization(format!("could not write message JSON: {e}")))?;
    String::from_utf8(out).map_err(|e| DomainError::Serialization(e.to_string()))
}

/// A JSON template for `desc` with every field at its default value.
pub fn empty_message_json(desc: &MessageDescriptor) -> DomainResult<String> {
    message_to_json(&DynamicMessage::new(desc.clone()))
}
```

Three behaviours are deliberate and pinned by the tests: blank text is an empty message, so an untouched editor can send; fields at their default are written, so the output doubles as an editable template; and both the camelCase and the original snake_case field names are accepted on input.

- [ ] **Step 4: Run the codec tests to verify they pass**

Run: `cargo test -j4 -p rocket-grpc`
Expected: PASS (15 tests across `registry` and `codec`).

- [ ] **Step 5: Run the checks**

Run:
- `cargo check -j4 -p rocket-grpc`
- `cargo test -j4 -p rocket-grpc`
- `cargo clippy -j4 -p rocket-grpc --all-targets` if clippy is part of your local routine.

Expected: PASS with no warnings from `rocket-grpc`.

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path:

```bash
git add crates/rocket-grpc Cargo.lock
```

Suggested subject: `feat(grpc): convert protobuf JSON to and from messages at runtime`.

---

## Next Plan

[Plan 12: gRPC execution and streaming](2026-10-05-protocol-parity-plan-12-grpc-execution-and-streaming.md). It depends on this plan (the registry, the codec and the file loader) and runs unary, streaming and reflection calls over tonic. Chain to it automatically when this one finishes.
