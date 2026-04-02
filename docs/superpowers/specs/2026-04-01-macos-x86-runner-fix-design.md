# Fix: macOS x86_64 CI Build Failure (git2/libgit2 OpenSSL)

**Date:** 2026-04-01  
**Status:** Approved

## Problem

The `macos-latest` GitHub Actions runner is Apple Silicon (`aarch64-apple-darwin`). The CI matrix includes an entry that cross-compiles to `x86_64-apple-darwin` from this runner. The build fails because `git2` (used by `rocket-git`) depends on two C libraries that require OpenSSL:

- `git2` (ssh feature) → `libssh2-sys` → `openssl-sys`
- `libgit2-sys` (https) → `openssl-sys`

`pkg-config` does not support cross-compilation on macOS, so both fail to locate OpenSSL when building for the non-host architecture.

Note: a prior attempt to fix this by switching `reqwest` from `native-tls` to `rustls-tls` addressed the wrong crate. The reqwest chain is now clean, but `git2`'s C library dependencies are the actual source of the failure.

## Fix

Change the `x86_64-apple-darwin` matrix entry in both workflow files from `macos-latest` to `macos-13` (GitHub's Intel runner). This eliminates cross-compilation entirely for that target — the x86_64 binary is built natively on x86_64 hardware.

**Files changed:** `.github/workflows/build.yml`, `.github/workflows/release.yml`

In each file, change:

```yaml
- platform: macos-latest
  rust_target: x86_64-apple-darwin
```

to:

```yaml
- platform: macos-13
  rust_target: x86_64-apple-darwin
```

The `aarch64-apple-darwin` entry remains on `macos-latest` (Apple Silicon, native).

## Why Not Other Approaches

- **Vendored OpenSSL**: Compiles OpenSSL from source on every CI run. Adds ~5–10 min build time and a large C dependency. Treating a symptom rather than eliminating cross-compilation.
- **CI env var overrides** (`OPENSSL_DIR` etc.): Fragile — depends on Homebrew paths and Rosetta availability on the runner. Breaks silently if runner configuration changes.

## Scope

Two files, one line changed in each: `.github/workflows/build.yml` and `.github/workflows/release.yml`.
