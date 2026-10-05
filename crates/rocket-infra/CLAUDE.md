# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# rocket-infra

Provides all filesystem and network implementations for the repository and service traits defined in the domain crates. This is the only crate that does I/O; everything else operates on trait objects.

## Commands

```bash
# Run all tests for this crate
cargo test -p rocket-infra

# Run a single test by name (substring match)
cargo test -p rocket-infra settings_roundtrip

# Fast compile check
cargo check -p rocket-infra
```

## Workspace role

`src-tauri` wires these concrete types into the service graph at startup. Domain crates (`rocket-collection`, `rocket-environment`, etc.) define the traits; `rocket-infra` provides the structs that implement them. `rocket-app` services hold `Box<dyn Trait>` and never depend on this crate directly.

## Public types

| Type | Implements | Notes |
|---|---|---|
| `FsCollectionRepo` | `CollectionRepository` | Reads/writes OpenCollection YAML under a given base directory. |
| `SharedPathCollectionRepo` | `CollectionRepository` | Wraps `FsCollectionRepo` behind an `Arc<Mutex<PathBuf>>` so the active workspace can change at runtime without rebuilding the service graph. |
| `FsEnvironmentRepo` | `EnvironmentRepository` | One `.yml` file per environment under `environments/`. |
| `FsHistoryRepo` | `HistoryRepository` | One `.yml` file per history entry under `history/`, sorted newest-first. |
| `FsTemplateRepo` | `TemplateRepository` | Template storage under `templates/`. |
| `FsCookieRepo` | `CookieRepository` | Cookie jar storage under `cookies/`. |
| `FsWorkspaceRepo` | `WorkspaceRepository` | Persists the workspace registry to `workspaces.yml`. Creates a "My Workspace" on first load. |
| `FsWorkspaceConfigRepo` | `WorkspaceConfigRepository` | Reads/writes per-workspace `workspace.yml` (collections list, description, environment settings). |
| `ReqwestExecutor` | `HttpExecutor` | Executes HTTP requests via `reqwest`. Handles all auth schemes, body types, and AWS SigV4 signing. |
| `NotifyFileWatcher` | — | Wraps the `notify` crate; publishes `DomainEvent::FileChanged` via `EventPublisher` when collection files change. |

## Internal modules

These are `pub` in `lib.rs` but are serialization-layer details — callers outside this crate should not depend on them directly.

- `opencollection` — serde structs mirroring the OpenCollection YAML schema (`OcCollection`, `OcHttpRequest`, `OcFolderInfo`, etc.). Used only for serialization; domain types are used everywhere else. Also contains GraphQL, gRPC, and WebSocket structs for schema completeness, but the repo only round-trips `OcHttpRequest` for individual request files — other protocol types land as `OpaqueProtocolItem` in the domain layer.
- `oc_conversions` — bidirectional `From` impls between domain types and `Oc*` serde structs. The boundary layer between persistence and domain.
- `migration` — detects and converts legacy JSON collections to OpenCollection YAML on first access; idempotent.

## Key patterns

**Path validation.** Every file path accepted by `FsCollectionRepo` is validated with `validate_path()`, which canonicalizes the nearest existing ancestor and checks that the resolved path stays inside the collection base directory. Path traversal attempts return `DomainError::InvalidInput`.

**On-disk format.** Collections are directories. Each directory contains `opencollection.yml` (metadata + settings), `folder.yml` (spec `Folder` shape: `info`/`request`/`docs`, read and written only through `fs_collection/folder_file.rs`, which also reads the legacy bare-`FolderInfo` shape), request files as `.yml`, and `_order.yml` (explicit item ordering). Legacy `.json` request files and the old `.uid` sidecar are auto-migrated on first access.

**UID storage.** UIDs are stored inside `opencollection.yml` and `folder.yml`. The legacy `.uid` file is read as a fallback during migration and then deleted.

**`SharedPathCollectionRepo` pattern.** The active workspace path is held in `Arc<Mutex<PathBuf>>`. Each repository call creates a short-lived `FsCollectionRepo` pointing at `<workspace>/collections`, so switching workspaces only requires updating the shared path.

**OAuth2 client credentials.** `ReqwestExecutor` fetches tokens synchronously as part of `execute()`. Other OAuth2 flows (authorization code, implicit) are not implemented and are silently skipped.

**Client certificates (mTLS).** `ReqwestExecutor` and `ReqwestTokenClientProvider` share `identity_for_url` and `load_identity`. They take `rocket_http::ResolvedClientCertificate`, not the persisted `ClientCertificate`. Only the entry chosen for the URL (first domain match) is loaded. `CertificateSource::File` is read from disk, `CertificateSource::Inline` (bytes fetched from RocketVault) is used from memory, and both go through `pem_key::unencrypted_key_pem` (PEM) or `Identity::from_pkcs12_der` (PKCS12). `CertificateMaterial::Unavailable { reason }` (a reference that could not be resolved) is an `InvalidInput(reason)` only when that entry is the selected one, with no fallback to another entry. Inline bytes and passphrases are `Zeroizing` and never written to disk, logged or put in an error message. The real handshake test is `cargo test -j4 -p rocket-infra mutual_tls_handshake -- --ignored` (needs the `openssl` CLI). `CertificateMaterial::Deferred` that reaches `load_identity` is an `InvalidInput` error (it should have been fetched first). An inline PKCS12 bundle that fails to open gets a hint that it may be a legacy RC2 or 3DES bundle. RocketVault certificates are listed and exported by the client in `rocketvault/certificates.rs` (wire shapes in `certificate_api.rs`). It keeps a name-to-id cache, keyed by connection id, base URL, vault and name, which `forget_connection` clears for a connection. Exported material is never cached, and decode errors never echo server text. The PKCS12 legacy RC2 risk is untested pending the ignored live test `live_rocketvault_export_loads_as_a_tls_identity`.

**Client certificates (mTLS), request flow.** `RequestExecutionService::resolve_request` copies the selected environment's client certificates onto `HttpRequest.options` as `ResolvedClientCertificate`s (placeholders resolved, relative paths joined, RocketVault references fetched into inline bytes by `rocket-app`). `ReqwestExecutor::execute` picks the first one whose domain matches the URL (`rocket-http` `client_cert`), loads it with `load_identity` (from a file or from inline bytes; an `Unavailable` entry is an error only when it is the one picked, with no fallback), and builds a dedicated client for it, because the shared client cache has no identity in its key. The TLS backend is native-tls, which only accepts an unencrypted PKCS#8 key, so PKCS12 bundles and PEM keys load, and an encrypted PKCS#8 PEM key (`BEGIN ENCRYPTED PRIVATE KEY`, PBES2 with PBKDF2 or scrypt and AES or 3DES) is decrypted in memory with the certificate's `passphrase` by `pem_key::unencrypted_key_pem` (the `pkcs8` crate; the key bytes are wiped on drop and nothing is written to disk). Old OpenSSL `Proc-Type: 4,ENCRYPTED` keys and PBES1 keys are rejected with a hint to convert them (`openssl pkcs8 -topk8 -v2 aes-256-cbc`) or use PKCS12. A wrong passphrase gives one message, "wrong passphrase or damaged file", and never echoes the passphrase. A matching certificate that cannot be loaded fails the request instead of being skipped. A client offers its identity to every host it connects to, so with a certificate the redirect policy stops at a redirect that leaves the certificate's domain and returns the 3xx response (the user can send to the new host on purpose); the redirect limit still applies. A leading `~/` in a path is the home directory, and a relative path is resolved against the collection folder (`EnvironmentRepositoryFactory::collection_dir`) by `rocket-app`, unless it contains `..`. A path that is still relative reaches the executor and is rejected with a message saying what is allowed. OAuth2 token requests present the certificate too: the client-credentials fetch inside a send and, through `ReqwestTokenClientProvider` (a `rocket_http::TokenClientProvider` wired into `OAuth2Service` at startup), direct grants, refresh and the authorization-code exchange. The certificate is matched against the token URL's host, not the API host, and a matching certificate that cannot be loaded fails the token request. The OAuth2 commands resolve the environment's RocketVault values first (`RequestExecutionService::resolve_external_secrets`, which needs no vault access for an environment without bindings), so `{{alias.secretName}}` works in token requests, for the client secret and the certificate passphrase alike. Not covered: the legacy `oauth2_auth_code_flow` command, which builds its own client. The handshake test is `mutual_tls_handshake_against_openssl_s_server` (`--ignored`, needs the openssl CLI); fixtures are in `test-fixtures/mtls`.

**WSSE, Digest, NTLM.** WSSE is signed in `apply_auth` (`rocket-http` `wsse_sig`: `Authorization: WSSE profile="UsernameToken"` plus `X-WSSE`). Digest is challenge-response: `execute` sends the request unauthenticated, and on a 401 with a Digest `WWW-Authenticate` challenge rebuilds the request and retries with the header from `rocket-http` `digest_sig` (one retry, plus one more for `stale=true`; a plain 401 after that is returned as-is). A challenge from a different origin than the one requested, reached through a redirect, is never answered. NTLM (NTLMv2 only, no signing or sealing) is a three-step handshake in `execute`: message 1 with the real request body (a 401 challenge means nothing was processed, so the body is uploaded again with message 3), the server's challenge in a 401, then message 3 with the full request, all on one dedicated single-connection HTTP/1 client (`ClientBuild.single_connection`). The first response body is drained so the connection returns to the pool. A response that is not a 401, or a 401 with no NTLM challenge, is returned as is, and a rejected login is the plain 401. Cookies: `RepoCookieStore` backs a reqwest cookie provider with the existing `CookieRepository` (see `cookie_store.rs`); `RequestOptions.use_cookie_jar` turns it off per request, and the load test runs with it off. Proxy: `ReqwestExecutor::with_proxy` reads a `SharedProxy` on every request and the client cache key includes its generation. SigV4: signed after the body is applied (`apply_aws_sigv4`), profile credentials come from `aws_profile.rs`.

**OAuth1.** `ReqwestExecutor::execute` signs the built request via `apply_oauth1` (after the body is applied, since form bodies are part of the signature). Placement is `header` (default), `query` or `body`. RSA-* signature methods fail the request with an error.

**AWS SigV4.** Like OAuth1, signed in `execute` via `apply_aws_sigv4` after the body is applied. The payload hash covers the real body; a streamed (multipart) body is signed as `UNSIGNED-PAYLOAD`. The signed `host` includes a non-default port. Credentials come from `aws_profile::resolve_credentials`: keys typed into the request win, otherwise a profile name is read from `~/.aws/credentials` (or `AWS_SHARED_CREDENTIALS_FILE`); empty keys with no profile fail the request instead of sending it. Errors and `Debug` output never contain keys or tokens.

**`OcAuth` serde design.** `OcAuth` is `#[serde(untagged)]`: the string `"inherit"` deserializes to `OcAuth::Inherit`; an object with a `type` field deserializes to `OcAuth::Typed`. New auth variants must go inside `OcAuthTyped` (tagged by `type`), not as new `OcAuth` variants.

**`OcItem` variant ordering.** The `OcItem` enum uses `#[serde(untagged)]`, so serde tries variants top-to-bottom. More specific types (those with a unique required field) must come before less specific ones — `Http` before `Folder`, etc. Changing variant order breaks deserialization of existing YAML files.

## Testing

All repository types have unit tests using `tempfile::TempDir` for ephemeral filesystem fixtures. `ReqwestExecutor` OAuth2 tests use `wiremock` to mock the token endpoint. Network tests that require live HTTP are marked `#[ignore]`.
