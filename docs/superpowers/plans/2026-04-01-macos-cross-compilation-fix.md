# macOS Cross-Compilation Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the `openssl-sys` dependency that prevents cross-compiling to `x86_64-apple-darwin` on Apple Silicon CI runners by switching `reqwest` from `native-tls` to `rustls-tls`.

**Architecture:** Single workspace-level dependency change in `Cargo.toml`. Disabling `default-features` on `reqwest` removes the `default-tls` → `native-tls` → `openssl-sys` chain; explicitly re-listing the other default features (`http2`, `charset`, `macos-system-configuration`) preserves existing behaviour.

**Tech Stack:** Rust / Cargo workspace, reqwest 0.12, rustls

---

### Task 1: Update reqwest workspace dependency

**Files:**
- Modify: `Cargo.toml` (workspace root, `[workspace.dependencies]` section, line 30)

- [ ] **Step 1: Edit the reqwest line in `Cargo.toml`**

  Find this line in `[workspace.dependencies]`:
  ```toml
  reqwest = { version = "0.12", features = ["json", "cookies", "multipart"] }
  ```

  Replace it with:
  ```toml
  reqwest = { version = "0.12", default-features = false, features = ["json", "cookies", "multipart", "rustls-tls", "http2", "charset", "macos-system-configuration"] }
  ```

- [ ] **Step 2: Verify `cargo check` passes (native target)**

  Run from the workspace root:
  ```bash
  cargo check
  ```
  Expected: no errors. This confirms the feature set is valid and compilation succeeds for the host target.

- [ ] **Step 3: Verify openssl-sys is no longer a direct dependency**

  ```bash
  cargo tree -p rocket --depth 5 2>/dev/null | grep openssl || echo "openssl-sys not found (expected)"
  ```
  Expected output: `openssl-sys not found (expected)`

  > Note: `openssl-sys` may still appear as a transitive dependency of unrelated crates (e.g. some git2 paths). What matters is it is no longer reachable through the reqwest chain. If it still appears, check which crate brings it in — it should not be `reqwest` → `native-tls` → `openssl-sys`.

- [ ] **Step 4: Commit**

  ```bash
  git add Cargo.toml Cargo.lock
  git commit -m "fix(deps): switch reqwest to rustls-tls to fix macOS cross-compilation"
  ```

---

### Task 2: Validate the fix works for the cross-compilation target (optional local check)

> Skip this task if you don't have an Apple Silicon Mac locally. The CI run after push is the definitive check.

**Files:** none (validation only)

- [ ] **Step 1: Add the x86_64 target if not already present**

  ```bash
  rustup target add x86_64-apple-darwin
  ```
  Expected: `info: component 'rust-std' for target 'x86_64-apple-darwin' is up to date` (or installs it).

- [ ] **Step 2: Run cargo check for the cross-compilation target**

  ```bash
  cargo check --target x86_64-apple-darwin
  ```
  Expected: no errors, and specifically no mention of `openssl` or `pkg-config`.

- [ ] **Step 3: Confirm CI passes**

  Push the branch and verify the `macos-latest / x86_64-apple-darwin` matrix job in `build.yml` succeeds on GitHub Actions.
