# OpenCollection Spec Reference

**Purpose**: Load this file when writing any code that reads or writes `.yml` files,
or touches collection / environment / request infrastructure.
This is the authoritative spec reference for RocketAPI infrastructure decisions.

**Usage in plans**: Add this line to any plan task that touches infra:
> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

---

## 1. Project Identity

**RocketAPI** is an open-source desktop API client (Bruno/Postman alternative) built with:

- **Backend**: Tauri v2 + Rust, DDD architecture, Cargo workspace
- **Frontend**: React 18, TypeScript, Vite, Zustand, shadcn/ui, Lucide React, Monaco Editor, CodeMirror 6
- **File format**: `.yml` only — **never `.json`** for any on-disk storage
- **Spec compliance**: OpenCollection v1.0.0 (`https://schema.opencollection.com/opencollection/v1.0.0.json`)

The codebase has these Rust crates:
`rocket-shared`, `rocket-collection`, `rocket-environment`, `rocket-http`,
`rocket-history`, `rocket-app`, `rocket-infra`, `rocket-git`, `rocket-import`, `rocket-workspace`,
`rocket-audit` (audit trail), `rocket-scripting` (script sandbox), `rocket-acp` (AI agent sessions),
`rocket-flow` (visual workflow builder) and `rocket-grpc` (gRPC protocol engine: `.proto` parsing,
JSON to protobuf, call types; no I/O).

---

## 2. OpenCollection Spec — Complete Reference

### 2.1 Top-Level Collection File (`opencollection.yml`)

```yaml
opencollection: "1.0.0"       # version string — REQUIRED
info:
  name: string                 # REQUIRED
  summary: string
  version: string
  authors:
    - name: string
      email: string
      url: string
config:
  environments: [...]          # Environment[]
  protobuf:
    protoFiles: [{ type: "file", path: string }]
    importPaths: [{ path: string, disabled?: bool }]
  proxy:
    disabled: bool
    inherit: bool
    config:
      protocol: string
      hostname: string
      port: number
      auth: false | { username, password }
      bypassProxy: string
  clientCertificates: [...]    # ClientCertificate[]
request:                       # RequestDefaults — inherited by all items
  headers: [...]
  metadata: [...]
  auth: Auth
  variables: [...]
  scripts: [...]
  settings: { http: {...}, graphql: {...} }
items: [...]                   # Item[] — folders + requests
docs: string | { content, type } | null
bundled: bool                  # true = single file; false = filesystem layout
extensions: {}                 # free-form extension object
```

**Hard rule**: `additionalProperties: false` on every object in the spec.
Any field you add that is not in this reference **will be rejected by the schema**.

---

### 2.2 Item Types

`items[]` is a polymorphic array. Each element is exactly one of:

| Discriminator field | Type |
|---|---|
| `info.type: "http"` | `HttpRequest` |
| `info.type: "graphql"` | `GraphQLRequest` |
| `info.type: "grpc"` | `GrpcRequest` |
| `info.type: "websocket"` | `WebSocketRequest` |
| `info.type: "folder"` | `Folder` |
| `type: "script"` | `ScriptFile` |

**Never invent a new `info.type` value.** The discriminator string must exactly match one of the six values above.

---

### 2.3 HttpRequest

```yaml
info:
  name: string
  description: Description
  type: "http"          # MUST be exactly "http"
  seq: number           # ordering in UI
  tags: [string]
http:
  method: string        # "GET", "POST", etc.
  url: string
  headers: [HttpRequestHeader]
  params: [HttpRequestParam]
  body: HttpRequestBody | [HttpRequestBodyVariant]
runtime:
  variables: [Variable]
  scripts: Scripts
  assertions: [Assertion]
  actions: [Action]
  auth: Auth
settings:
  encodeUrl: bool | "inherit"
  timeout: number | "inherit"
  followRedirects: bool | "inherit"
  maxRedirects: number | "inherit"
examples: [HttpRequestExample]
docs: string
```

### 2.4 GraphQLRequest

```yaml
info:
  type: "graphql"       # MUST be exactly "graphql"
  name, description, seq, tags
graphql:
  method: string
  url: string
  headers: [HttpRequestHeader]
  params: [HttpRequestParam]
  body: GraphQLBody | [GraphQLBodyVariant]
runtime:
  variables, scripts, assertions, actions, auth
settings:
  encodeUrl, timeout, followRedirects, maxRedirects
docs: string
```

Rocket also writes, and the schema guard knows about (`KNOWN_DEFERRED` in `schema_shape_tests.rs`):
- a top-level `uid`, the same deviation as HTTP requests. A file without one gets an in-memory uid on load, and `uid` is written on the next save;
- `settings.verifySsl` (`GraphQLRequestSettings.verifySsl`);
- `graphql.auth`, the request-level auth, same shape as `http.auth`.

`runtime.auth` is written for GraphQL but is not verified against the upstream schema (the guard does not inspect GraphQL `runtime`; the HTTP equivalent, `HttpRequestRuntime.auth`, is in `KNOWN_DEFERRED`).

### 2.5 GrpcRequest

```yaml
info:
  type: "grpc"          # MUST be exactly "grpc"
  name, description, seq, tags
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

Rocket also writes a top-level `uid` for gRPC requests, the same deviation as HTTP requests (`KNOWN_DEFERRED` in `schema_shape_tests.rs`). gRPC auth lives in the `grpc` block; the schema allows only `variables`, `scripts` and `assertions` in `runtime`. A legacy `runtime.auth` is still read, and never written.

- Rocket always writes `info.type` and `grpc.methodType`, and omits `grpc.auth` when there is no auth.
- A single untitled message is written as a plain string, and every other shape as `GrpcMessageVariant`s. A plain string reads back as the selected message.
- `runtime.variables` may carry Rocket's `initial` key, the same as HTTP (`Variable.initial` in `KNOWN_DEFERRED`).
- The schema guard does not check gRPC `runtime.assertions` keys.

### 2.6 WebSocketRequest

```yaml
info:
  type: "websocket"     # MUST be exactly "websocket"
  name, description, seq, tags
websocket:
  url: string
  headers: [HttpRequestHeader]
  message: WebSocketMessage | [WebSocketMessageVariant]
runtime:
  variables: [Variable]
  scripts: Scripts
  auth: Auth
settings:
  timeout: number | "inherit"
  keepAliveInterval: number | "inherit"
docs: string
```

Rocket also writes a top-level `uid` (the same deviation as HTTP requests; a file without one gets the derived uid `ws-<file name>` on load) and `websocket.auth`, the request-level auth, same shape as `http.auth`. The schema guard does not inspect WebSocket `runtime`.

### 2.7 Folder

```yaml
info:
  name: string
  description: Description
  type: "folder"        # MUST be exactly "folder"
  seq: number
  tags: [string]
items: [Item]           # recursive — folders can nest
request:                # RequestDefaults — inherited by children
  headers, metadata, auth, variables, scripts, settings
docs: string | { content, type } | null
```

### 2.8 ScriptFile

```yaml
type: "script"          # top-level discriminator (no `info` wrapper)
script: string          # the JS code
```

---

### 2.9 Shared Sub-Types

#### Message and body variants (GraphQL, WebSocket, gRPC)
```yaml
# GraphQLBodyVariant
title: string
selected: bool        # optional, default false
body: { query: string, variables: string }

# WebSocketMessageVariant
title: string
selected: bool        # optional, default false
message: { type: "text" | "json" | "xml" | "binary", data: string }

# GrpcMessageVariant
title: string
selected: bool        # optional, default false
message: string       # protobuf JSON text
```

#### HttpRequestHeader
```yaml
name: string    # REQUIRED
value: string   # REQUIRED
description: Description
disabled: bool
```

#### HttpRequestParam
```yaml
name: string                    # REQUIRED
value: string                   # REQUIRED
type: "query" | "path"          # REQUIRED
description: Description
disabled: bool
```

#### HttpRequestBody (oneOf)
- `RawBody`: `{ type: "json"|"text"|"xml"|"sparql", data: string }`
- `FormUrlEncodedBody`: `{ type: "form-urlencoded", data: [{ name, value, description?, disabled? }] }`
- `MultipartFormBody`: `{ type: "multipart-form", data: [{ name, type: "text"|"file", value: string|string[], description?, disabled? }] }`
- `FileBody`: `{ type: "file", data: [{ filePath, contentType, selected }] }`

#### GraphQLBody
```yaml
query: string
variables: string   # JSON string, NOT a parsed object
```

#### WebSocketMessage
```yaml
type: "text" | "json" | "xml" | "binary"   # REQUIRED
data: string                                # REQUIRED
```

#### GrpcMetadata
```yaml
name: string    # REQUIRED
value: string   # REQUIRED
description: Description
disabled: bool
```

#### Variable
```yaml
name: string
value: VariableValue | [VariableValueVariant]
description: Description
disabled: bool
```

`VariableValue` is either a plain `string` OR `{ type: "string"|"number"|"boolean"|"null"|"object", data: string }`.

`VariableValueVariant`: `{ title: string, selected?: bool, value: VariableValue }` — title is REQUIRED.

#### SecretVariable
```yaml
secret: true    # REQUIRED — this const is what distinguishes it from Variable
name: string
description: Description
disabled: bool
type: "string" | "number" | "boolean" | "null" | "object"
```

#### Description (oneOf)
- `{ content: string, type: string }` (MIME type, e.g. `"text/markdown"`)
- plain `string`
- `null`

#### Scripts (array)
```yaml
- type: "before-request" | "after-response" | "tests" | "hooks"  # REQUIRED
  code: string                                                     # REQUIRED
```

#### Assertion
```yaml
expression: string   # REQUIRED
operator: string     # REQUIRED
value: string
disabled: bool
description: Description
```

#### Action (currently only `ActionSetVariable`)
```yaml
type: "set-variable"   # REQUIRED
selector:
  expression: string   # REQUIRED
  method: "jsonq"      # REQUIRED — only "jsonq" is valid
variable:
  name: string         # REQUIRED
  scope: "runtime" | "request" | "folder" | "collection" | "environment"  # REQUIRED
phase: "before-request" | "after-response"
description: Description
disabled: bool
```

---

## 3. Auth Types — Complete Reference

`Auth` is a `oneOf` among these types plus the string `"inherit"`.

### 3.1 `"inherit"`
```yaml
auth: inherit    # string literal — inherits from parent folder/collection
```

**Known difference from Bruno.** Rocket writes `Auth::None` by leaving `auth` out, and an absent `auth` loads as `None`. At send time `None` and `Inherit` behave the same: both take the collection's auth (`merge_auth`; the WebSocket and gRPC paths do the same). Bruno keeps `none` and `inherit` as separate modes, so a Bruno request set to `none` sends no auth. Do not change `None` to mean "send no auth" on its own. New requests default to `None`, and Postman and Bruno requests with no auth field import as `None`, so they would all stop inheriting the collection's auth. A safe change needs `none` stored explicitly, an absent `auth` read as inherit, and the importers mapping a missing auth to inherit.

### 3.2 AWS Signature V4
```yaml
auth:
  type: awsv4    # REQUIRED
  accessKeyId: string
  secretAccessKey: string
  sessionToken: string
  service: string
  region: string
  profileName: string
```

### 3.3 Basic
```yaml
auth:
  type: basic    # REQUIRED
  username: string
  password: string
```

### 3.4 Bearer
```yaml
auth:
  type: bearer   # REQUIRED
  token: string
```

### 3.5 Digest
```yaml
auth:
  type: digest   # REQUIRED
  username: string
  password: string
```

Rocket answers Digest challenges in the HTTP executor (`rocket-http` `digest_sig`). The first request goes out unauthenticated and a 401 with a `Digest` challenge is retried once, plus once more for `stale=true`.

### 3.6 NTLM
```yaml
auth:
  type: ntlm     # REQUIRED
  username: string
  password: string
  domain: string
```

### 3.7 WSSE
```yaml
auth:
  type: wsse     # REQUIRED
  username: string
  password: string
```

### 3.8 API Key
```yaml
auth:
  type: apikey   # REQUIRED
  key: string
  value: string
  placement: "header" | "query"
```

### 3.9 OAuth 2.0 — Client Credentials
```yaml
auth:
  type: oauth2                        # REQUIRED
  flow: client_credentials            # REQUIRED
  accessTokenUrl: string
  refreshTokenUrl: string
  credentials:
    clientId: string
    clientSecret: string
    placement: "basic_auth_header" | "body"
  scope: string
  additionalParameters:
    accessTokenRequest: [{ name, value, placement: "header"|"query"|"body" }]
    refreshTokenRequest: [...]
  tokenConfig:
    id: string
    placement: { header: string } | { query: string }
  settings:
    autoFetchToken: bool
    autoRefreshToken: bool
```

### 3.10 OAuth 2.0 — Resource Owner Password
```yaml
auth:
  type: oauth2
  flow: resource_owner_password_credentials   # REQUIRED
  accessTokenUrl, refreshTokenUrl, credentials, scope
  resourceOwner:
    username: string
    password: string
  additionalParameters, tokenConfig, settings
```

### 3.11 OAuth 2.0 — Authorization Code
```yaml
auth:
  type: oauth2
  flow: authorization_code             # REQUIRED
  authorizationUrl, accessTokenUrl, refreshTokenUrl, callbackUrl
  credentials: { clientId, clientSecret, placement }
  scope, state
  pkce:
    disabled: bool
    method: "S256" | "plain"
  additionalParameters:
    authorizationRequest: [...]
    accessTokenRequest: [...]
    refreshTokenRequest: [...]
  tokenConfig, settings
```

### 3.12 OAuth 2.0 — Implicit
```yaml
auth:
  type: oauth2
  flow: implicit                       # REQUIRED
  authorizationUrl, callbackUrl
  credentials:
    clientId: string                   # implicit only has clientId, no secret
  scope, state
  additionalParameters:
    authorizationRequest: [...]
  tokenConfig, settings
```

---

### 3.13 OAuth 1.0
```yaml
auth:
  type: oauth1                 # REQUIRED; every other field is optional
  consumerKey: string
  consumerSecret: string
  accessToken: string
  accessTokenSecret: string
  callbackUrl: string          # "oob" for out-of-band
  verifier: string
  signatureMethod: "HMAC-SHA1" | "HMAC-SHA256" | "HMAC-SHA512" | "RSA-SHA1" | "RSA-SHA256" | "RSA-SHA512" | "PLAINTEXT"
  privateKey: { type: "file" | "text", value: string }   # RSA-* methods
  timestamp: string
  nonce: string
  version: string              # defaults to "1.0"
  realm: string
  placement: "header" | "query" | "body"
  includeBodyHash: bool
```
Rocket parses and persists this losslessly (`OcAuthTyped::OAuth1`, domain `Auth::OAuth1`). The HTTP executor signs HMAC-SHA1/256/512 and PLAINTEXT requests (`rocket-http` `oauth1_sig`). RSA-* methods return an error instead of sending an unsigned request.

---

## 4. Environment Type

```yaml
name: string             # REQUIRED
color: string
description: Description
variables:
  - Variable             # regular variable
  - SecretVariable       # secret: true — no value field
clientCertificates: [ClientCertificate]
extends: string          # name of another environment to inherit from
dotEnvFilePath: string
```

### ClientCertificate (oneOf)
```yaml
# PEM type:
domain: string           # REQUIRED
type: pem                # REQUIRED
certificateFilePath: string   # REQUIRED
privateKeyFilePath: string    # REQUIRED
passphrase: string

# PKCS12 type:
domain: string           # REQUIRED
type: pkcs12             # REQUIRED
pkcs12FilePath: string   # REQUIRED
passphrase: string
```

**Rocket extension keys (not in the OpenCollection schema).** Each piece of client certificate material may come from a RocketVault secret instead of a file. The secret is named by reference (`alias.secretName`, the same key external-secret values use), never by value:

```yaml
# PEM type: use instead of certificateFilePath / privateKeyFilePath
certificateSecret: string     # reference to a secret holding the certificate PEM
privateKeySecret: string      # reference to a secret holding the private key PEM
# PKCS12 type: use instead of pkcs12FilePath
pkcs12Secret: string          # reference to a secret holding the base64 text of the DER bundle
```

Each piece has exactly one source: a non-empty file path or a reference. With a reference the file path field is empty and is not written. A value that starts with `-----BEGIN` in a path or reference field is rejected on save, so key text never reaches the environment file. References are validated on save against the environment's `externalSecrets` bindings. The schema requires the file paths, so another OpenCollection tool may reject or drop an entry that uses a reference, the same trade-off as `externalSecrets`. Fetched values are never persisted. An unresolved reference fails a request or token request only when that certificate is the one selected for the URL.

**Rocket extension entry type `vault` (not in the OpenCollection schema).** An entry may name a certificate that RocketVault exports at send time, instead of naming material:

```yaml
- type: vault
  domain: api.example.com
  binding: prod          # External Secrets alias of this environment; the connection and vault come from it
  certificate: client-a  # certificate name in that vault
  format: pem            # pem (default) or pkcs12
```

Only names are stored: no id, version, password, key text or path. On save the binding must be one of the environment's `externalSecrets` aliases, the certificate name must be set, and no field may start with `-----BEGIN`. At send time Rocket exports the certificate only when it is the one selected for the URL (requests and OAuth2 token URLs alike), keeps the material in memory, and fails that request with a clear message if the export fails, with no fallback to another entry. Load tests cannot use it. Other OpenCollection tools do not know this type, and Rocket builds from before it fail to parse an environment file that holds it, so they hide that environment.

Rocket resolves a relative certificate file path against the collection folder (the parent of `environments/`) and rejects one with `..`. Absolute and `~/` paths are used as written. Rocket presents the first certificate whose `domain` matches the request URL (host, optional scheme and port, `*` wildcard). PKCS12 bundles and PEM keys work. A PKCS#8 PEM key may be encrypted (`BEGIN ENCRYPTED PRIVATE KEY`) when `passphrase` is set. Old OpenSSL `Proc-Type: 4,ENCRYPTED` keys and PBES1 keys return an error that says how to convert them. A matching certificate that cannot be loaded fails the request. A redirect that leaves the certificate's domain is not followed, so the certificate is not offered to another host; the 3xx response is returned. OAuth2 token requests use the same certificate, matched against the token URL.

---

## 5. RocketAPI File Layout on Disk

RocketAPI uses the **unbundled** layout (`bundled: false`).
Each item in `items[]` maps to a separate `.yml` file on disk.

```
~/.rocket-api/
  workspaces.yml                         ← list of all workspace paths
  <workspace-name>/
    workspace.yml                        ← { name, globalEnvironment: "env-name" }
    environments/                        ← GLOBAL environments (workspace-wide)
      <env-name>.yml                     ← Environment type
    collections/
      <collection-name>/
        opencollection.yml               ← top-level Collection (no items[])
        environments/                    ← REGULAR environments (per-collection)
          <env-name>.yml
        <folder-name>/
          folder.yml                     ← Folder type (no items[])
          <request-name>.yml             ← HttpRequest / GraphQLRequest / etc.
        <request-name>.yml
```

**Key layout rules:**
- `opencollection.yml` at collection root — contains `info`, `config`, `request` (defaults), `docs`. The `items[]` array is NOT written here; items live as individual files.
- `folder.yml` at each folder root — contains `info`, `request` (defaults), `docs`. No `items[]` array.
- Each request is its own `.yml` file named after the request (slugified).
- All file names and directory names are slugified (lowercase, hyphens).
- **All files are `.yml`, never `.json`.**

---

## 6. Variable Scopes & Resolution Order

Priority order (highest → lowest):

```
7. Runtime Variables      set by scripts at send time; stored in memory only
6. Request Variables      runtime.variables[] in the request .yml
5. Folder Variables       request.variables[] in folder.yml (walk full parent chain; innermost wins)
4. Environment Variables  active env in collection/environments/
3. Collection Variables   request.variables[] in opencollection.yml
2. Global Environment     selected env in workspace/environments/ (workspace.yml → globalEnvironment)
1. Process Env Variables  {{process.env.FOO}} syntax only
```

**Rules that must never be violated:**
- Environment (4) always beats Collection (3).
- Folder variables require walking the **full ancestor directory chain**, not just the immediate parent `folder.yml`.
- For `Variable.value`, resolution uses `value ?? initialValue` — the `initialValue` field is the Git-committed shared value; `value` is the local override. (This is a RocketAPI convention layered on top of the spec's `VariableValue` type.)
- Global environments are **regular `Environment` yml files** stored at the workspace level — they are NOT a special type.
- Runtime variables exist in memory only — never serialised to disk.

---

## 7. Serde / IPC Conventions

| Context | Convention |
|---|---|
| **On-disk YAML** | snake_case field names — match spec exactly (e.g. `accessTokenUrl`, `clientId` — spec uses camelCase here, follow spec) |
| **Tauri IPC (invoke/listen)** | camelCase — `#[serde(rename_all = "camelCase")]` on IPC DTOs only |
| **Internal Rust domain types** | snake_case — standard Rust |

**Rule**: Never put `#[serde(rename_all = "camelCase")]` on a type that is also serialised to disk. Keep IPC DTOs separate from persistence structs.

---

## 8. Rust Architecture Rules

- **Dependency injection**: `Box<dyn Trait>` pattern throughout — never construct concrete types inside domain logic.
- **Domain events**: Every significant backend operation emits a domain event (e.g. `CollectionCreated`, `RequestDeleted`). Do not skip this for "simple" operations.
- **Crate boundaries**: Cross-crate calls go through the `rocket-shared` public interface, not by importing internal modules.
- **Error handling**: Use `thiserror` for domain errors. Never use `unwrap()` in production paths — use `?` and propagate.
- **Git**: Use the `git2` crate (libgit2 bindings). Never shell out to `git` CLI.
- **File I/O**: All disk operations for collections and environments must go through `FsCollectionRepo` and `FsEnvironmentRepo`. Never write raw YAML directly from feature code.

---

## 9. Frontend Rules

- **UI components**: `shadcn/ui` primitives **only**. Never use raw `<button>`, `<input>`, `<dialog>`, `<select>`, or `<form>` HTML elements for interactive UI.
- **Exceptions** (the only allowed raw HTML for interactive elements): Monaco editor wrapper, canvas/SVG, CodeMirror 6 `SingleLineEditor` wrapper.
- **Icons**: `lucide-react` only — no inline SVGs.
- **Single-line variable-aware fields**: Use `SingleLineEditor` (CodeMirror 6). Never use Monaco for single-line fields.
- **Multi-line editors**: Monaco only (body editor, response viewer, Git diff viewer, conflict resolver).
- **State management**: Zustand stores. Never fully destructure store state at the top of a component — access fields individually to avoid unnecessary re-renders.
- **Conventional commits**: All commit messages must follow the conventional commits format (`feat:`, `fix:`, `chore:`, etc.).

---

## 10. Do's and Don'ts

### ✅ DO

- Always write files as `.yml`, not `.json`.
- Always use `additionalProperties: false` awareness — if a field doesn't exist in section 2–4 of this doc, it doesn't belong in the schema.
- Always discriminate request types by `info.type` — the type string must exactly match the spec enum.
- Always use `"inherit"` (string literal) when an auth or setting falls back to the parent — never `null`, never omit the field ambiguously.
- Always emit a domain event from the Rust backend for state-changing operations.
- Always route disk I/O through `FsCollectionRepo` / `FsEnvironmentRepo`.
- Always walk the full ancestor folder chain when resolving folder variables.
- Always keep IPC DTOs (camelCase serde) separate from persistence structs (snake_case / spec-case).
- Always check the variable resolution order in section 6 before implementing any variable lookup.

### ❌ DON'T

- **Don't add fields not in the spec** — `additionalProperties: false` means extra fields silently break schema compliance.
- **Don't use `info.type: "rest"`** — it's `"http"`. This is a common mistake when coming from Postman/Bruno thinking.
- **Don't store runtime variables to disk** — they are ephemeral, memory-only.
- **Don't conflate `Variable` and `SecretVariable`** — secrets have `secret: true` and no `value` field; they carry only a `type` (string/number/boolean/null/object).
- **Don't put `items[]` in `opencollection.yml` or `folder.yml`** — in unbundled layout, items are individual files on disk.
- **Don't use `.json` extension anywhere** — not for environments, not for workspaces, not for ui-state.
- **Don't write raw YAML from feature crates** — always use the repo layer.
- **Don't use raw HTML interactive elements** in the frontend — always use shadcn/ui primitives.
- **Don't use inline SVGs for icons** — always use `lucide-react`.
- **Don't shell out to `git` CLI** — always use the `git2` crate.
- **Don't use `unwrap()`** in production Rust code paths.
- **Don't put `#[serde(rename_all = "camelCase")]` on persistence structs** — camelCase serde is for IPC DTOs only.
- **Don't invent a new `ActionSetVariable` selector `method`** — only `"jsonq"` is valid per the spec.
- **Don't use `variables.graphql` field for GraphQL body variables** — `GraphQLBody.variables` is a JSON **string**, not a parsed object.

---

## 11. Quick-Reference: Field Name Spelling (spec is camelCase for these)

These field names appear in the OpenCollection spec in camelCase.
Use exactly these spellings in YAML files and Rust serde structs that represent spec types:

```
accessTokenUrl       authorizationUrl      callbackUrl
refreshTokenUrl      clientId              clientSecret
accessKeyId          secretAccessKey       sessionToken
profileName          pkcs12FilePath        certificateFilePath
privateKeyFilePath   protoFilePath         methodType
encodeUrl            followRedirects       maxRedirects
autoFetchToken       autoRefreshToken      bypassProxy
tokenConfig          additionalParameters  dotEnvFilePath
clientCertificates   importPaths           protoFiles
certificateSecret    privateKeySecret      pkcs12Secret     (Rocket extensions)
```

---

## 12. Spec Source

- Schema: `https://schema.opencollection.com/opencollection/v1.0.0.json`
- Spec: `https://spec.opencollection.com/`
- OpenCollection is a Bruno-built, community-supported project. It defines **how to use** an API (workflows, scripts, environments), complementing OpenAPI which defines **what** the API is.
