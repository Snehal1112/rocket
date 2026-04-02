# Fix: macOS x86_64 CI Cross-Compilation Failure

**Date:** 2026-04-01  
**Status:** Approved

## Problem

The `macos-latest` GitHub Actions runner is now Apple Silicon (`aarch64-apple-darwin`). Both `build.yml` and `release.yml` include a matrix entry that cross-compiles to `x86_64-apple-darwin` from this runner. The build fails because `reqwest 0.12` defaults to `native-tls`, which depends on `openssl-sys`, and `pkg-config` does not support cross-compilation on macOS.

## Fix

Change the workspace `reqwest` dependency in `Cargo.toml` to disable default features and use `rustls-tls` instead of `native-tls`. This removes `openssl-sys` from the dependency tree entirely.

```toml
# Cargo.toml — [workspace.dependencies]
reqwest = { version = "0.12", default-features = false, features = ["json", "cookies", "multipart", "rustls-tls", "http2", "charset", "macos-system-configuration"] }
```

The four explicit features preserve all existing behaviour that was previously provided by reqwest's defaults:

| Feature | Purpose |
|---|---|
| `rustls-tls` | Pure-Rust TLS — no system OpenSSL required |
| `http2` | HTTP/2 support |
| `charset` | Response encoding detection |
| `macos-system-configuration` | macOS network config integration (no-op on other platforms) |

No CI workflow files need to change. The fix is at the source level and applies to all three workflows automatically.

## Trade-offs Considered

- **rustls vs native-tls**: rustls does not read the OS certificate store, which can affect users in corporate environments with custom CA roots. This is an acceptable trade-off given the project's current stage and can be revisited with `rustls-platform-verifier` if needed.
- **Dedicated runners (macos-13 for x86_64)**: Avoids cross-compilation but ties the build to an aging runner that will eventually be deprecated.
- **Vendored OpenSSL**: Adds build time and CI complexity without removing the underlying dependency.

## Scope

Single file change: `Cargo.toml` (workspace root), one dependency line.
