# rok.grpc Plan 01: Core Exchange Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A tested `GrpcService::exchange` that runs any of the four gRPC method types as one non-interactive exchange and returns the status, the messages, the headers and the trailers, with the spec's limits.

**Architecture:** Plain host types go in `rocket-scripting`, limit constants in `rocket-grpc`, a confined proto path in `rocket-collection` (rule) and `ProtoLoader` (symlink-safe resolution, implemented in `rocket-infra`), and the exchange in `rocket-app`'s `GrpcService`. Nothing here is reachable from a script yet.

**Tech Stack:** Rust, tokio, async-trait, tonic executor already in `rocket-infra`.

**Spec:** `docs/superpowers/specs/2026-11-12-rok-grpc-design.md`

## Global Constraints

- Limits (spec table): response messages 1000, response bytes 10 MiB (`10 * 1024 * 1024`), input messages 1000, default time 30 s, time ceiling 5 min.
- Hitting the message or byte cap: the call ends, messages so far are kept, status `RESOURCE_EXHAUSTED` (code 8), `truncated` true. Hitting the time limit: status `DEADLINE_EXCEEDED` (code 4), messages so far kept.
- Setup errors are `DomainError::InvalidInput`. Messages name a field path or a cap, never a value.
- Proto path in confined mode: relative, no `..`, empty or `.` segment, no backslash, colon or NUL, inside the collection after symlinks resolve.
- Unary and server streaming take exactly one input message. Client streaming and bidirectional take one or more. Zero input messages means the request's selected saved message.
- Plan-index rulings 1 to 6 apply.

## Review Focus

- A streaming call whose server sends nothing and finishes with an error status: resolve with that status and empty `messages`, not a hang.
- A server that streams past the 1000-message cap: the call is aborted and returns the first 1000 with `truncated`.
- A bidirectional call where the server replies before all input is sent: replies are kept in order.
- A message that does not fit the proto in the middle of the input list: nothing is sent, a setup error names the index.
- A proto symlink that points outside the collection: refused.

---

### Task 1: Host types and limit constants

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-scripting/src/host.rs`, `crates/rocket-scripting/src/lib.rs`
- Create: `crates/rocket-grpc/src/limits.rs`
- Modify: `crates/rocket-grpc/src/lib.rs`

**Interfaces:**
- Produces in `rocket-scripting`:
  - `pub enum HostGrpcTarget { Saved { path: String }, AdHoc { url: String, service: String, method: String, proto_file: String, tls: bool } }`
  - `pub struct HostGrpcRequest { pub target: HostGrpcTarget, pub messages: Vec<String>, pub metadata: Vec<(String, String)>, pub timeout_ms: Option<u64>, pub runtime_vars: HashMap<String, String> }`. `messages` holds one JSON text per message. All derive `Debug, Clone, PartialEq, Eq` and, for the request types, `Deserialize` with `#[serde(rename_all = "camelCase")]` and `#[serde(tag = "kind")]` on the target enum (the engine builds them from JSON; `runtime_vars` has `#[serde(default)]`).
  - `pub struct HostGrpcStatus { pub code: i32, pub name: String, pub message: String }`
  - `pub struct HostGrpcResponse { pub status: HostGrpcStatus, pub messages: Vec<String>, pub headers: Vec<(String, String)>, pub trailers: Vec<(String, String)>, pub truncated: bool }` (derives `Serialize` with camelCase).
  - `ScriptHost::grpc(&self, _request: HostGrpcRequest) -> Result<HostGrpcResponse, HostError>`, default `Err(HostError::Unavailable)`.
- Produces in `rocket-grpc`: `pub const MAX_RESPONSE_MESSAGES: usize = 1000;`, `MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024`, `MAX_INPUT_MESSAGES: usize = 1000`, `DEFAULT_CALL_TIMEOUT: Duration = 30 s`, `MAX_CALL_TIMEOUT: Duration = 5 min`, and `pub fn clamp_call_timeout(requested_ms: Option<u64>) -> Duration` (None gives the default, anything above the ceiling gives the ceiling, 0 gives the default). All re-exported from the crate root.

- [ ] **Step 1: Write failing tests** in `host.rs` tests: `host_grpc_request_reads_the_engine_json_shape` (a saved target and an ad hoc target, camelCase keys, `kind` tag, missing `runtimeVars` defaults to empty), `host_grpc_response_writes_camel_case_keys`, `script_host_grpc_defaults_to_unavailable`. In `limits.rs`: `clamp_call_timeout_uses_the_default_for_none_and_zero`, `clamp_call_timeout_caps_at_five_minutes`, `clamp_call_timeout_keeps_a_valid_value`.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-scripting host_grpc` and `cargo test -j4 -p rocket-grpc clamp_call` and expect compile failures.
- [ ] **Step 3: Implement** the types, the trait method and the constants as listed. The existing `script_host_is_object_safe` test must still pass.
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-scripting` and `cargo test -j4 -p rocket-grpc`. Expect pass.
- [ ] **Step 5: Stage** the six paths above. Do not commit.

---

### Task 2: Confined proto path

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-collection/src/iteration_data.rs` (extract the shared rule), `crates/rocket-collection/src/lib.rs`
- Modify: `crates/rocket-grpc/src/registry.rs` (trait), `crates/rocket-infra/src/grpc/mod.rs` (`FsProtoLoader`), and every test double of `ProtoLoader` (search `impl ProtoLoader for`, including `crates/rocket-app/src/test_doubles.rs`)

**Interfaces:**
- Produces in `rocket-collection`: `pub fn validate_relative_path(raw: &str) -> Result<(), String>`. It holds the lexical rule of `validate_data_file_path` (not empty, no leading `/`, no backslash, colon or NUL, no empty, `.` or `..` segment). `validate_data_file_path` calls it, so its messages and tests stay as they are.
- Produces in `rocket-grpc`: `ProtoLoader::resolve_within(&self, root: &Path, relative: &str) -> DomainResult<PathBuf>`, a required method (no default, so no implementation can skip the check). It returns the canonical file path, and fails with `DomainError::InvalidInput` when the canonical file is not under the canonical root or is not a regular file.
- `rocket-grpc` must not depend on `rocket-collection`. The lexical check therefore runs in `GrpcService` (Task 3). `resolve_within` still rejects anything that canonicalises outside the root.

- [ ] **Step 1: Write failing tests.** `rocket-collection`: `validate_relative_path_rejects_the_same_forms_as_the_data_file_rule` (table: `""`, `/a`, `a\\b`, `C:x`, `a//b`, `./a`, `a/./b`, `a/../b`, `a/` rejected; `protos/orders.proto` accepted). `rocket-infra` (`FsProtoLoader`): `resolve_within_returns_the_canonical_path_of_a_file_inside_the_root`, `resolve_within_refuses_a_symlink_that_leaves_the_root` (unix only, `#[cfg(unix)]`), `resolve_within_refuses_a_directory`, `resolve_within_refuses_a_missing_file`.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-collection validate_relative` and `cargo test -j4 -p rocket-infra resolve_within`. Expect failures.
- [ ] **Step 3: Implement** the extraction and `resolve_within` (canonicalize root and joined file, `starts_with`, `is_file`). Update the doubles to a simple in-memory version that joins and returns the path.
- [ ] **Step 4: Run** the focused tests plus `cargo test -j4 -p rocket-collection` and `cargo check -j4 -p rocket-app --tests`. Expect pass.
- [ ] **Step 5: Stage** explicit paths. Do not commit.

---

### Task 3: GrpcService::exchange

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/grpc_service.rs` (or a new `crates/rocket-app/src/grpc_service/exchange.rs` if the file grows past a reviewable size), `crates/rocket-app/src/lib.rs`
- Test: `crates/rocket-app/src/grpc_service.rs` tests or the new module, using the in-memory executor and `MemReader` greeter proto already used there (read how existing tests build `GrpcService`).

**Interfaces:**
- Consumes: Task 1 constants and `clamp_call_timeout`; Task 2 `validate_relative_path` (via `rocket-collection`, already a dependency) and `ProtoLoader::resolve_within`.
- Produces:
  - `pub struct GrpcExchangeInput { pub execute: GrpcExecuteInput, pub messages: Vec<String>, pub confine_proto: bool }`. `execute.message` is ignored. `execute.timeout` is replaced by the clamped value.
  - `pub struct GrpcExchangeOutput { pub status: GrpcStatus, pub headers: Vec<GrpcMetadataPair>, pub trailers: Vec<GrpcMetadataPair>, pub messages: Vec<String>, pub truncated: bool }`
  - `GrpcService::exchange(&self, input: GrpcExchangeInput) -> DomainResult<GrpcExchangeOutput>`.

Behaviour (decisions the implementer cannot make alone):
- Resolve variables and the call through the existing `prepare_call`. Load descriptors through `registry_for`. When `confine_proto` is true, `registry_from_file` first runs `validate_relative_path` on the resolved path, then `proto_loader.resolve_within(collection_dir, relative)`, and refuses an absolute or `~/` path (no `expand_home_prefix`). Reflection (no proto path) stays allowed.
- Method type comes from the registry method flags (`GrpcMethodType::from_streaming_flags`). Not from `request.method_type`.
- Input rules: zero `messages` means one message from `prepare_message`. Unary and server streaming with more than one message is a setup error. More than `MAX_INPUT_MESSAGES` is a setup error. Every message is variable-resolved and checked with `json_to_message` against the input descriptor before anything is sent, and the setup error names the message index.
- Unary: `executor.unary`, map to the output, `messages` holds the one reply if `message_json` is some.
- Streams: `open_stream` with the one message as `initial_json` for server streaming, none for the other two. For client streaming and bidirectional, send each message through `handle.outbound`, then drop the sender (half-close), concurrently with reading `handle.events` (a bidirectional server may reply while input is still being sent, and a bounded channel would deadlock otherwise). Collect `Headers`, `Message` and `Finished` into the output.
- Caps: stop collecting at `MAX_RESPONSE_MESSAGES` or `MAX_RESPONSE_BYTES`, call `handle.abort.abort()`, and return status `GrpcStatus::new(8, ...)` with `truncated` true. The whole call runs under `tokio::time::timeout(clamped)`. On timeout abort the stream and return `GrpcStatus::new(4, ...)` with the messages so far.
- The stream ending without `Finished` returns status UNKNOWN (code 2) with message "the call ended without a status".

- [ ] **Step 1: Write failing tests** (names are the contract): `exchange_runs_a_unary_call_and_returns_the_reply`, `exchange_uses_the_saved_message_when_no_messages_are_given`, `exchange_rejects_two_messages_for_a_unary_method`, `exchange_collects_every_message_of_a_server_stream`, `exchange_sends_all_messages_of_a_client_stream_then_half_closes`, `exchange_keeps_replies_of_a_bidirectional_call_in_order`, `exchange_returns_the_error_status_of_a_stream_with_no_messages`, `exchange_stops_at_the_message_cap_and_marks_truncated`, `exchange_stops_at_the_byte_cap_and_marks_truncated`, `exchange_returns_what_arrived_when_the_time_limit_hits`, `exchange_names_the_index_of_a_message_that_does_not_fit_the_proto` (and asserts nothing was sent), `exchange_refuses_an_absolute_proto_path_when_confined`, `exchange_refuses_a_dotdot_proto_path_when_confined`, `exchange_allows_an_absolute_proto_path_when_not_confined`. The fake executor needs a streaming mode that can emit N messages, stay silent, or finish with an error status; extend the existing test double rather than adding a second one.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-app exchange_` and expect compile failures.
- [ ] **Step 3: Implement** `GrpcExchangeInput`, `GrpcExchangeOutput` and `exchange` per the behaviour above. Share code with `call_unary` and `prepare_session` instead of copying it (extract small private helpers if needed).
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-app grpc_service` and `cargo test -j4 -p rocket-app exchange_`. Expect pass. Run `cargo check -j4 -p rocket-infra --tests`.
- [ ] **Step 5: Stage** explicit paths. Do not commit.
