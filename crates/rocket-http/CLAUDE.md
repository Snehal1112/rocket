# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Crate Does

`rocket-http` is a pure-domain crate: it defines HTTP request/response types, auth utilities, cookie management, and a load-testing harness. It contains **no I/O implementations** — the concrete `ReqwestExecutor` and `FsCookieRepository` live in `rocket-infra`.

## Commands

```bash
# Validate (fast)
cargo check -p rocket-http

# Run all tests
cargo test -p rocket-http

# Run a single test
cargo test -p rocket-http <test_name>
```

## Module Map

| Module | Purpose |
|---|---|
| `executor` | `HttpExecutor` async trait — single method `execute(&HttpRequest) -> DomainResult<HttpResponse>` |
| `request` | `HttpRequest` (fully-resolved, ready-to-send) and `RequestOptions` (timeout, SSL, redirect flags) |
| `response` | `HttpResponse` with status helpers (`is_success`, `is_redirect`, etc.) and case-insensitive `header_value()` |
| `oauth2` | `OAuthConfig`, `OAuthToken`, and `acquire_token()` — supports `client_credentials`, `password`, `authorization_code` grants |
| `pkce` | `generate_pkce()` → `PkcePair` (verifier + challenge per RFC 7636) |
| `aws_sig` | `sign_request()` → `SignedHeaders` — full AWS Signature Version 4 HMAC chain |
| `cookie` | `Cookie` and `CookieJar` (in-memory, domain-scoped; `add` replaces by name) |
| `cookie_repository` | `CookieRepository` trait for persistent cookie storage |
| `load_test` | `run_load_test()` — fires N concurrent requests bounded by a semaphore, returns `LoadTestResult` with p50/p95/p99 latency stats |

## Key Design Rules

- `HttpRequest` is the **resolved** request (variables already substituted). It is distinct from `rocket-collection`'s `Request`, which is a saved template.
- `RequestOptions` defaults: `follow_redirects = true`, `timeout_ms = 30_000`, `verify_ssl = true`. These are applied via `#[serde(default)]`, so missing fields in JSON deserialise correctly.
- Auth is **stateless and functional**: `acquire_token`, `sign_request`, and `generate_pkce` are standalone functions. The service layer (`rocket-app`) calls them during request preparation — nothing here holds token state.
- `AwsCredentials` and `SignedHeaders` do **not** derive `serde` (they are transient signing artefacts, never serialised for IPC).
- All other public types derive `serde::{Serialize, Deserialize}` with `#[serde(rename_all = "camelCase")]` for Tauri IPC compatibility.
- `CookieJar::add` silently replaces a cookie with the same name — upsert semantics.
- `run_load_test` uses a `Semaphore` to cap concurrency; failed requests contribute to `failed` count but not to latency stats.

## Relationships

- Depends on `rocket-shared` for `DomainResult`, `DomainError`, and shared types (`Auth`, `Body`, `Header`, `HttpMethod`, `QueryParam`).
- `rocket-infra` provides the concrete `ReqwestExecutor` (implements `HttpExecutor`) and `FsCookieRepository` (implements `CookieRepository`).
- `rocket-app`'s `RequestExecutionService` receives `Box<dyn HttpExecutor>` via constructor injection.
