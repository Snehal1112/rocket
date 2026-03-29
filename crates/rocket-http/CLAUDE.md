# rocket-http

Provides HTTP request/response abstractions, auth utilities, and cookie management. It defines the executor trait and all auth logic (OAuth 2.0/PKCE, AWS SigV4, Basic, Bearer, API Key); no I/O implementation lives here.

## Key Public Types and Traits

| Type / Trait | Description |
|---|---|
| `HttpExecutor` | Async trait — implementations (e.g. `ReqwestExecutor` in `rocket-infra`) execute an `HttpRequest` and return an `HttpResponse`. |
| `HttpRequest` | Fully-resolved request: method, URL, headers, query params, body, auth config, and `RequestOptions`. |
| `HttpResponse` | Response with status, headers, body, and timing metrics. Helper methods: `is_success`, `is_redirect`, `is_client_error`, `is_server_error`. |
| `RequestOptions` | Per-request flags: `follow_redirects`, `timeout_ms`, `verify_ssl`. |
| `OAuthConfig` / `OAuthToken` | OAuth 2.0 configuration (client_credentials, password, authorization_code) and token response with expiry tracking (`is_expired()`). |
| `PkcePair` | PKCE code-verifier/challenge pair for secure OAuth flows. |
| `CookieRepository` | Trait for persistent cookie storage (`get_all`, `get_by_domain`, `save`, `clear`). |
| `Cookie` / `CookieJar` | Domain-scoped in-memory cookie storage. |
| `AwsCredentials` / `SignedHeaders` | AWS Signature v4 signing support. |

## Key Patterns

- All traits are `Send + Sync` for `Box<dyn Trait>` use in service structs.
- All fallible methods return `DomainResult<T>` from `rocket-shared`.
- Auth is modular: `acquire_token`, `sign_request`, `generate_pkce` are standalone functions the service layer calls during request preparation.
- All public types derive `serde::{Serialize, Deserialize}` for Tauri IPC.

## Relationships

- Depends on `rocket-shared` for error types and shared domain primitives.
- `rocket-infra` provides the concrete `ReqwestExecutor` and `FsCookieRepository`.
- `rocket-app`'s `RequestExecutionService` receives `Box<dyn HttpExecutor>` via constructor injection.
