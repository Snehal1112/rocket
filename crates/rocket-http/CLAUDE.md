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
| `load_test` | Phase-based load testing harness. Each `LoadTestPhase` carries a `PhaseTarget` (either `Concurrency(N)` users or `Rps(N)` requests/sec); a single config must use one unit for all phases. `run_load_test_v2()` branches on the unit: concurrency mode uses a `Semaphore` whose permits are reshaped at phase boundaries, rps mode uses a `RateDriver` token bucket whose rate is updated continuously between checkpoints. Both modes share the same `RingBuffer<RequestLogEntry>`, snapshot task, and `LoadTestProgressEvent` shape. The legacy `run_load_test()` is kept for backwards compatibility with existing tests. |

## Key Design Rules

- `HttpRequest` is the **resolved** request (variables already substituted). It is distinct from `rocket-collection`'s `Request`, which is a saved template.
- `RequestOptions` defaults: `follow_redirects = true`, `timeout_ms = 30_000`, `verify_ssl = true`. These are applied via `#[serde(default)]`, so missing fields in JSON deserialise correctly.
- Auth is **stateless and functional**: `acquire_token`, `sign_request`, and `generate_pkce` are standalone functions. The service layer (`rocket-app`) calls them during request preparation — nothing here holds token state.
- `AwsCredentials` and `SignedHeaders` do **not** derive `serde` (they are transient signing artefacts, never serialised for IPC).
- All other public types derive `serde::{Serialize, Deserialize}` with `#[serde(rename_all = "camelCase")]` for Tauri IPC compatibility.
- `CookieJar::add` silently replaces a cookie with the same name — upsert semantics.
- `run_load_test` uses a `Semaphore` to cap concurrency and classifies each request as one of three outcomes: `Success` (HTTP status < 400), `StatusFail` (status ≥ 400), or `TransportFail` (executor error). `Success` and `StatusFail` both contribute latency to the stats; only `TransportFail` is excluded (no latency sample). `failed = failed_transport + failed_status`. Optional `interval_ms` on `LoadTestConfig` adds a staggered-start delay between spawns.
- `LoadTestConfigV2` uses `#[serde(default)]` on `success_rule` and `ring_buffer_size` so callers that omit them get safe defaults (400 / 5 000). The three new fields on `LoadTestResult` (`phase_timeline`, `request_log`, `time_series`) also carry `#[serde(default)]` for backwards-compat with older Tauri call sites.
- `LoadTestPhase` deserialization is backward-compatible: configs saved before the `PhaseTarget` refactor used `targetConcurrency: number`; the manual `Deserialize` impl accepts that legacy field and rewrites it to `PhaseTarget::Concurrency(value)`. New code should always emit the `target: { kind, value }` shape (Serialize is derived and always uses the new shape).
- Mixed-unit configs (some `Concurrency`, some `Rps` phases) are not supported. `LoadTestConfigV2::has_uniform_target_unit()` returns `false` for them; the service layer (`rocket-app`) is responsible for rejecting these before calling `run_load_test_v2`. If validation is bypassed, the runtime falls back to the first phase's unit and reinterprets later phases' values under that unit.

## Relationships

- Depends on `rocket-shared` for `DomainResult`, `DomainError`, and shared types (`Auth`, `Body`, `Header`, `HttpMethod`, `QueryParam`).
- `rocket-infra` provides the concrete `ReqwestExecutor` (implements `HttpExecutor`) and `FsCookieRepository` (implements `CookieRepository`).
- `rocket-app`'s `RequestExecutionService` receives `Box<dyn HttpExecutor>` via constructor injection.
