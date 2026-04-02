# Vendored OpenSSL Cross-Compilation Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the macOS x86_64 CI build by adding vendored OpenSSL (compiled from source) and setting C cross-compilation flags, eliminating the pkg-config dependency that fails on macOS cross-compilation.

**Architecture:** Add `openssl` with `vendored` feature to workspace deps so that `libgit2-sys` and `libssh2-sys` compile OpenSSL from source instead of looking for it via pkg-config. Add a CI step that sets `CC_x86_64_apple_darwin` and target-specific `CFLAGS`/`LDFLAGS` so clang produces x86_64 code when building those C libraries. Revert the `macos-13` runner back to `macos-latest`.

**Tech Stack:** Rust/Cargo workspace, openssl crate v0.10, GitHub Actions YAML, macOS Xcode clang

---

### Task 1: Add vendored OpenSSL to workspace dependencies

**Files:**
- Modify: `Cargo.toml` (workspace root, `[workspace.dependencies]` section)

- [ ] **Step 1: Add the openssl dependency**

  Open `Cargo.toml`. In `[workspace.dependencies]`, add this line after the existing deps (e.g., after the `base64` line):
  ```toml
  openssl = { version = "0.10", features = ["vendored"] }
  ```

- [ ] **Step 2: Verify `cargo check` passes**

  ```bash
  cargo check
  ```
  Expected: no errors. This confirms the dependency resolves and the feature is valid.

- [ ] **Step 3: Verify openssl-sys is now satisfied without pkg-config**

  ```bash
  cargo tree -p rocket-git 2>/dev/null | grep openssl
  ```
  Expected: lines showing `openssl` and `openssl-sys` in the tree. The presence of the vendored openssl crate means pkg-config will no longer be invoked for the target.

- [ ] **Step 4: Commit**

  ```bash
  git add Cargo.toml Cargo.lock
  git commit -m "fix(deps): add vendored OpenSSL to satisfy libgit2/libssh2 cross-compilation"
  ```

---

### Task 2: Update build.yml — revert runner and add cross-compilation step

**Files:**
- Modify: `.github/workflows/build.yml`

- [ ] **Step 1: Revert the x86_64 runner from macos-13 back to macos-latest**

  In `.github/workflows/build.yml`, find:
  ```yaml
          - platform: macos-13
            rust_target: x86_64-apple-darwin
  ```
  Change to:
  ```yaml
          - platform: macos-latest
            rust_target: x86_64-apple-darwin
  ```

- [ ] **Step 2: Add cross-compilation environment step**

  Find the `Install Linux dependencies` step (around line 43). Insert a new step **before** it (after the `Rust cache` step):

  ```yaml
      - name: Set up macOS cross-compilation environment
        if: matrix.rust_target == 'x86_64-apple-darwin'
        run: |
          echo "CC_x86_64_apple_darwin=clang" >> $GITHUB_ENV
          echo "CFLAGS_x86_64_apple_darwin=-target x86_64-apple-macos10.12" >> $GITHUB_ENV
          echo "LDFLAGS_x86_64_apple_darwin=-target x86_64-apple-macos10.12" >> $GITHUB_ENV
  ```

- [ ] **Step 3: Verify the final file structure**

  ```bash
  grep -n "platform:\|rust_target:\|cross-compilation" .github/workflows/build.yml
  ```
  Expected output includes:
  ```
  18:          - platform: ubuntu-22.04
  20:          - platform: windows-latest
  22:          - platform: macos-latest
  24:          - platform: macos-latest
  ...          Set up macOS cross-compilation environment
  ```
  Both macOS entries should use `macos-latest`.

- [ ] **Step 4: Commit**

  ```bash
  git add .github/workflows/build.yml
  git commit -m "fix(ci): revert to macos-latest + add cross-compilation env for x86_64 target"
  ```

---

### Task 3: Update release.yml — revert runner and add cross-compilation step

**Files:**
- Modify: `.github/workflows/release.yml`

- [ ] **Step 1: Revert the x86_64 runner from macos-13 back to macos-latest**

  In `.github/workflows/release.yml`, find:
  ```yaml
          - platform: macos-13
            rust_target: x86_64-apple-darwin
  ```
  Change to:
  ```yaml
          - platform: macos-latest
            rust_target: x86_64-apple-darwin
  ```

- [ ] **Step 2: Add cross-compilation environment step**

  Find the `Install Linux dependencies` step. Insert a new step **before** it (after the `Rust cache` step):

  ```yaml
      - name: Set up macOS cross-compilation environment
        if: matrix.rust_target == 'x86_64-apple-darwin'
        run: |
          echo "CC_x86_64_apple_darwin=clang" >> $GITHUB_ENV
          echo "CFLAGS_x86_64_apple_darwin=-target x86_64-apple-macos10.12" >> $GITHUB_ENV
          echo "LDFLAGS_x86_64_apple_darwin=-target x86_64-apple-macos10.12" >> $GITHUB_ENV
  ```

- [ ] **Step 3: Verify the final file structure**

  ```bash
  grep -n "platform:\|rust_target:\|cross-compilation" .github/workflows/release.yml
  ```
  Expected output includes:
  ```
  17:          - platform: ubuntu-22.04
  19:          - platform: windows-latest
  21:          - platform: macos-latest
  23:          - platform: macos-latest
  ...          Set up macOS cross-compilation environment
  ```
  Both macOS entries should use `macos-latest`.

- [ ] **Step 4: Commit**

  ```bash
  git add .github/workflows/release.yml
  git commit -m "fix(ci): revert to macos-latest + add cross-compilation env for x86_64 target"
  ```
