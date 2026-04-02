# Fix: macOS x86_64 CI via Vendored OpenSSL

**Date:** 2026-04-01  
**Status:** Approved

## Background

Two prior attempts to fix the macOS x86_64 CI failure:
1. Switching `reqwest` from `native-tls` to `rustls-tls` — fixed reqwest's chain but `git2`'s C dependencies (`libgit2-sys`, `libssh2-sys`) still pulled in `openssl-sys`.
2. Using `macos-13` (Intel runner) — `macos-13` is not available in this GitHub Actions setup (`The configuration 'macos-13-us-default' is not supported`).

## Problem

Cross-compiling from `macos-latest` (Apple Silicon, aarch64) to `x86_64-apple-darwin` fails because:
- `libgit2-sys` and `libssh2-sys` depend on `openssl-sys`
- `openssl-sys` uses `pkg-config` to locate OpenSSL at build time
- `pkg-config` does not support cross-compilation on macOS

## Fix

### 1. Cargo.toml — add vendored OpenSSL

Add `openssl` as a direct workspace dependency with the `vendored` feature. This causes the `openssl-sys` build script to compile OpenSSL from source instead of using `pkg-config`. All downstream crates that depend on `openssl-sys` (including `libgit2-sys` and `libssh2-sys`) automatically use the vendored build.

```toml
openssl = { version = "0.10", features = ["vendored"] }
```

### 2. Both CI workflow files — revert runner + add cross-compilation step

Revert `macos-13` back to `macos-latest` for the `x86_64-apple-darwin` entry. Add a step before the Tauri build that sets C compiler environment variables so clang targets x86_64 when compiling vendored C libraries:

```yaml
- name: Set up macOS cross-compilation environment
  if: matrix.rust_target == 'x86_64-apple-darwin'
  run: |
    echo "CC_x86_64_apple_darwin=clang" >> $GITHUB_ENV
    echo "CFLAGS_x86_64_apple_darwin=-target x86_64-apple-macos10.12" >> $GITHUB_ENV
    echo "LDFLAGS_x86_64_apple_darwin=-target x86_64-apple-macos10.12" >> $GITHUB_ENV
```

macOS Xcode's clang supports both arm64 and x86_64 targets from the same host — no extra toolchain required. The `-target x86_64-apple-macos10.12` flag directs clang to produce x86_64 code.

**Files changed:**
- `Cargo.toml` (workspace root)
- `.github/workflows/build.yml`
- `.github/workflows/release.yml`

## Trade-offs

- **Build time**: Vendored OpenSSL compiles from source, adding ~5–10 minutes to the macOS x86_64 build. Acceptable given no Intel runner is available.
- **Self-contained**: No system library dependency for TLS — the build is fully reproducible regardless of runner state.
- **Scope**: The `openssl` dependency is workspace-wide but the `vendored` feature only activates the source compilation; it does not change the TLS behaviour of any crate at runtime.

## Scope

Three files: `Cargo.toml`, `build.yml`, `release.yml`.
