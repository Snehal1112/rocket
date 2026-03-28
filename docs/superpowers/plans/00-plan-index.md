# OpenCollection Full Spec — Plan Index (v2)

**Date:** 2026-03-28
**Schema:** v1.0.0 (89 types)
**Coverage target:** 100% of schema types

## Plan breakdown — 14 plans, 42 tasks (max 3 per plan)

### Layer 1: Domain model (P01-P03)

| Plan | Tasks | Types covered |
|---|---|---|
| **OC-P01** Domain: Description, Documentation, Assertion, Tag, Sequence | 3 | Description, Documentation, Assertion (full: +disabled, +description), Tag, Sequence |
| **OC-P02** Domain: Auth enum (all 10 types + WSSE) | 3 | Auth, AuthBasic, AuthBearer, AuthApiKey, AuthDigest, AuthNTLM, AuthWsse, AuthAwsV4 (+profileName), "inherit", "none" |
| **OC-P03** Domain: OAuth2 (4 flows + all sub-types) | 3 | AuthOAuth2, OAuth2ClientCredentialsFlow, OAuth2ResourceOwnerPasswordFlow, OAuth2AuthorizationCodeFlow, OAuth2ImplicitFlow, OAuth2ClientCredentials, OAuth2ResourceOwner, OAuth2PKCE, OAuth2AdditionalParameter, OAuth2TokenConfig, OAuth2TokenPlacement, OAuth2TokenPlacedInHeader, OAuth2TokenPlacedInQuery, OAuth2Settings |

### Layer 2: Domain extensions (P04-P05)

| Plan | Tasks | Types covered |
|---|---|---|
| **OC-P04** Domain: Variable system + Environment + Extensions | 3 | Variable (+description, typed VariableValue), SecretVariable (+type), VariableValueVariant, VariableValue, Environment (+color, +extends, +dotEnvFilePath, +clientCertificates), Extensions |
| **OC-P05** Domain: Request fields + Proxy + ClientCertificate | 3 | Request (seq, tags, description, scripts, assertions, actions, examples, docs), Proxy, ProxyConnectionConfig, ProxyAuth, ClientCertificate, PemCertificate, Pkcs12Certificate |

### Layer 3: YAML structs — core (P06-P08)

| Plan | Tasks | Types covered |
|---|---|---|
| **OC-P06** YAML: Shared types (Description, Variable, Auth, Settings) | 3 | OcDescription, OcVariable, OcSecretVariable, OcVariableValue, OcVariableValueVariant, OcAuth (all variants), OcRequestSettings, OcHttpRequestSettings, OcGraphQLRequestSettings |
| **OC-P07** YAML: HTTP request (Info, Details, Body, Runtime) | 3 | OcHttpRequest, OcHttpRequestInfo, OcHttpRequestDetails, OcHttpRequestParam, OcHttpRequestHeader, OcHttpRequestBody (Raw+Form+Multipart+File), OcHttpRequestBodyVariant, OcFileBody, OcFileBodyVariant, OcHttpRequestRuntime, OcScript, OcAssertion, OcAction, OcActionSetVariable, OcHttpRequestExample, OcHttpRequestSettings |
| **OC-P08** YAML: GraphQL + gRPC + WebSocket + Folder + Collection | 3 | OcGraphQLRequest, OcGraphQLBody, OcGraphQLBodyVariant, OcGrpcRequest, OcGrpcRequestDetails, OcGrpcMetadata, OcGrpcMessage, OcGrpcMessageVariant, OcWebSocketRequest, OcWebSocketMessage, OcWebSocketMessageVariant, OcFolder, OcFolderInfo, OcItem, OcScriptFile, OcCollectionConfig, OcProtobuf, OcProtoFile, OcInfo, OcAuthor, OcRequestDefaults, OcDocumentation, OcEnvironment (full), top-level OcCollection |

### Layer 4: Conversions (P09-P10)

| Plan | Tasks | Types covered |
|---|---|---|
| **OC-P09** Conversions: Header, Param, Body, Auth, Variable, Environment | 3 | All Oc* ↔ domain conversions for shared types |
| **OC-P10** Conversions: Full request files (HTTP, GraphQL, gRPC, WS, Folder) | 3 | OcHttpRequest ↔ Request, OcGraphQLRequest ↔ domain, OcGrpcRequest ↔ domain, OcWebSocketRequest ↔ domain, OcFolder ↔ Folder, OcCollection ↔ Collection |

### Layer 5: Infra repos (P11-P12)

| Plan | Tasks | Types covered |
|---|---|---|
| **OC-P11** FsCollectionRepo: create/read/write .yml + opencollection.yml + folder.yml | 3 | Full repo rewrite |
| **OC-P12** FsEnvironmentRepo + FsHistoryRepo + FsTemplateRepo + FsCookieRepo + FileWatcher | 3 | All remaining repos |

### Layer 6: Migration + Frontend (P13-P14)

| Plan | Tasks | Types covered |
|---|---|---|
| **OC-P13** Migration: JSON → OpenCollection YAML | 3 | Detection, conversion, auto-migrate on access |
| **OC-P14** Frontend: .yml handling + end-to-end verification | 3 | Tauri bridge, sidebar, smoke test |

## Type coverage matrix

**89/89 types covered (100%)**

Every `$defs` type from the schema mapped to a plan:
- P01: Description, Documentation, Assertion, Tag, Sequence (5)
- P02: Auth*, AuthBasic, AuthBearer, AuthApiKey, AuthDigest, AuthNTLM, AuthWsse, AuthAwsV4 (8)
- P03: AuthOAuth2, OAuth2ClientCredentialsFlow, OAuth2ResourceOwnerPasswordFlow, OAuth2AuthorizationCodeFlow, OAuth2ImplicitFlow, OAuth2ClientCredentials, OAuth2ResourceOwner, OAuth2PKCE, OAuth2AdditionalParameter, OAuth2TokenConfig, OAuth2TokenPlacement, OAuth2TokenPlacedInHeader, OAuth2TokenPlacedInQuery, OAuth2Settings (14)
- P04: Variable, SecretVariable, VariableValue, VariableValueVariant, Environment, Extensions (6)
- P05: HttpRequestExample, Proxy, ProxyConnectionConfig, ProxyAuth, ClientCertificate, PemCertificate, Pkcs12Certificate (7)
- P06: YAML structs for shared types (maps to P01-P05 domain types)
- P07: HttpRequest, HttpRequestInfo, HttpRequestDetails, HttpRequestParam, HttpRequestHeader, HttpRequestBody, RawBody, FormUrlEncodedBody, MultipartFormBody, FileBody, FileBodyVariant, HttpRequestBodyVariant, HttpRequestRuntime, HttpRequestSettings, Script, Scripts, ScriptFile, Action, ActionSetVariable, HttpResponseHeader (20)
- P08: GraphQLRequest, GraphQLRequestInfo, GraphQLRequestDetails, GraphQLRequestRuntime, GraphQLRequestSettings, GraphQLBody, GraphQLBodyVariant, GrpcRequest, GrpcRequestInfo, GrpcRequestDetails, GrpcRequestRuntime, GrpcMetadata, GrpcRequestMessage, GrpcMessage, GrpcMessageVariant, WebSocketRequest, WebSocketRequestInfo, WebSocketRequestDetails, WebSocketRequestRuntime, WebSocketMessage, WebSocketMessageVariant, Folder, FolderInfo, Item, CollectionConfig, Protobuf, ProtoFile, ProtoFileItem, ProtoFileImportPath, Info, Author, RequestDefaults, RequestSettings (33)
- P09-P14: conversions + infra (no new types, wire existing)
