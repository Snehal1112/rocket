# OC-P08: YAML Structs — GraphQL + gRPC + WebSocket + Folder + Collection (33 types)

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** YAML structs for all remaining protocol types (GraphQL, gRPC, WebSocket), folder structure, and the top-level collection config (opencollection.yml).

**Architecture:** Continues `opencollection.rs`. Each protocol request type follows the same pattern: info + protocol-details + runtime + docs.

**Tech Stack:** Rust, serde, serde_yaml

**Prerequisite:** OC-P07 complete.

**Schema types (33):** GraphQLRequest, GraphQLRequestInfo, GraphQLRequestDetails, GraphQLRequestRuntime, GraphQLRequestSettings, GraphQLBody, GraphQLBodyVariant, GrpcRequest, GrpcRequestInfo, GrpcRequestDetails, GrpcRequestRuntime, GrpcMetadata, GrpcRequestMessage, GrpcMessage, GrpcMessageVariant, WebSocketRequest, WebSocketRequestInfo, WebSocketRequestDetails, WebSocketRequestRuntime, WebSocketMessage, WebSocketMessageVariant, Folder, FolderInfo, Item, CollectionConfig, Protobuf, ProtoFile, ProtoFileItem, ProtoFileImportPath, Info, Author, RequestDefaults, RequestSettings

---

## Task 1: GraphQL + gRPC + WebSocket request types

**Files:**
- Modify: `crates/rocket-infra/src/opencollection.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Implement all protocol request types**

Each follows the same pattern as HttpRequest:

**GraphQL:**
- `OcGraphQLRequest` — info, graphql, runtime, settings, docs
- `OcGraphQLRequestDetails` — method, url, headers, params, body (GraphQLBody | GraphQLBodyVariant[])
- `OcGraphQLBody` — query, variables
- `OcGraphQLBodyVariant` — title, selected, body
- `OcGraphQLRequestRuntime` — variables, scripts, assertions, actions, auth

**gRPC:**
- `OcGrpcRequest` — info, grpc, runtime, docs
- `OcGrpcRequestDetails` — url, method, methodType (unary|client-streaming|server-streaming|bidi-streaming), protoFilePath, metadata, message
- `OcGrpcMetadata` — name, value, description, disabled
- `OcGrpcMessage` — string
- `OcGrpcMessageVariant` — title, selected, message
- `OcGrpcRequestRuntime` — variables, scripts, assertions, auth

**WebSocket:**
- `OcWebSocketRequest` — info, websocket, runtime, docs
- `OcWebSocketRequestDetails` — url, headers, message (WebSocketMessage | WebSocketMessageVariant[])
- `OcWebSocketMessage` — type (text|json|xml|binary), data
- `OcWebSocketMessageVariant` — title, selected, message
- `OcWebSocketRequestRuntime` — variables, scripts, auth

Tests: parse one YAML example for each protocol type.

- [ ] **Step 2: Run tests + commit**

```bash
cargo test -p rocket-infra -- opencollection::tests
git add crates/rocket-infra/src/opencollection.rs
git commit -m "feat(infra): GraphQL + gRPC + WebSocket YAML structs"
```

---

## Task 2: OcFolder + OcItem + OcScriptFile

**Files:**
- Modify: `crates/rocket-infra/src/opencollection.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Implement folder and item types**

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcFolderInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<OcDescription>,
    #[serde(rename = "type")]
    pub r#type: String,  // "folder"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcFolder {
    pub info: OcFolderInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<OcItem>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<OcRequestDefaults>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<OcDocumentation>,
}

/// Item — oneOf[HttpRequest, GraphQLRequest, GrpcRequest, WebSocketRequest, Folder, ScriptFile]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OcItem {
    Http(OcHttpRequest),
    GraphQL(OcGraphQLRequest),
    Grpc(OcGrpcRequest),
    WebSocket(OcWebSocketRequest),
    Folder(OcFolder),
    ScriptFile(OcScriptFile),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcScriptFile {
    #[serde(rename = "type")]
    pub r#type: String,  // "script"
    pub script: String,
}
```

Tests: parse folder with nested items, parse OcItem dispatch.

- [ ] **Step 2: Run tests + commit**

```bash
cargo test -p rocket-infra -- opencollection::tests
git add crates/rocket-infra/src/opencollection.rs
git commit -m "feat(infra): OcFolder + OcItem + OcScriptFile YAML structs"
```

---

## Task 3: Top-level OcCollection (opencollection.yml) + OcInfo + OcCollectionConfig + OcRequestDefaults

**Files:**
- Modify: `crates/rocket-infra/src/opencollection.rs`
- Test: inline `#[cfg(test)]`

- [ ] **Step 1: Implement top-level collection struct**

```rust
/// Top-level OpenCollection document (opencollection.yml in bundled mode, or collection root).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcCollection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opencollection: Option<String>,  // spec version
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub info: Option<OcInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<OcCollectionConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<OcItem>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<OcRequestDefaults>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<OcDocumentation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<serde_yaml::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authors: Option<Vec<OcAuthor>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcAuthor {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcCollectionConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environments: Option<Vec<OcEnvironment>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protobuf: Option<OcProtobuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<OcProxy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_certificates: Option<Vec<OcClientCertificate>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcRequestDefaults {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<Vec<OcHttpRequestHeader>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Vec<OcGrpcMetadata>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<OcAuth>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<Vec<OcVariable>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scripts: Option<Vec<OcScript>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<OcRequestSettings>,
}

// Also: OcProtobuf, OcProtoFile, OcProtoFileImportPath, OcProxy, OcProxyConnectionConfig, OcProxyAuth, OcClientCertificate, OcEnvironment (full with color, extends, dotEnvFilePath)
```

Tests: parse a complete `opencollection.yml` with info, config (proxy + certs), request defaults, items.

- [ ] **Step 2: Run ALL infra tests**

```bash
cargo test -p rocket-infra
```
Expected: ALL PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-infra/src/opencollection.rs
git commit -m "feat(infra): OcCollection top-level + OcInfo + OcCollectionConfig + all remaining YAML structs"
```

---

## Milestone Checklist — OC-P08

- [ ] All 33 remaining schema types implemented
- [ ] GraphQL request full struct chain
- [ ] gRPC request full struct chain (with methodType, protoFilePath, metadata, message)
- [ ] WebSocket request full struct chain (with message types)
- [ ] OcFolder with nested OcItem dispatch
- [ ] OcScriptFile for shared scripts
- [ ] OcCollection top-level (opencollection.yml)
- [ ] OcInfo with summary, version, authors
- [ ] OcCollectionConfig with environments, protobuf, proxy, clientCertificates
- [ ] OcRequestDefaults with headers, metadata, auth, variables, scripts, settings
- [ ] `cargo test -p rocket-infra` — all pass

**After P08: All 89/89 schema types have corresponding YAML structs.**
