# macOS x86_64 Runner Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the macOS x86_64 CI build failure by switching the `x86_64-apple-darwin` matrix entry from `macos-latest` (Apple Silicon) to `macos-13` (Intel), eliminating cross-compilation and the resulting OpenSSL pkg-config failure.

**Architecture:** Two identical one-line changes in the CI workflow files. The `x86_64-apple-darwin` target will now build natively on an Intel runner instead of cross-compiling from Apple Silicon. No Rust code or Cargo dependencies change.

**Tech Stack:** GitHub Actions YAML

---

### Task 1: Fix build.yml

**Files:**
- Modify: `.github/workflows/build.yml:25`

- [ ] **Step 1: Edit the platform for the x86_64 matrix entry**

  Open `.github/workflows/build.yml`. Find this block (around line 23–25):
  ```yaml
          - platform: macos-latest
            rust_target: aarch64-apple-darwin
          - platform: macos-latest
            rust_target: x86_64-apple-darwin
  ```

  Change only the second `macos-latest` (the one paired with `x86_64-apple-darwin`):
  ```yaml
          - platform: macos-latest
            rust_target: aarch64-apple-darwin
          - platform: macos-13
            rust_target: x86_64-apple-darwin
  ```

- [ ] **Step 2: Verify the file looks correct**

  Run:
  ```bash
  grep -n "platform:" .github/workflows/build.yml
  ```
  Expected output:
  ```
  18:          - platform: ubuntu-22.04
  20:          - platform: windows-latest
  22:          - platform: macos-latest
  24:          - platform: macos-13
  ```
  Confirm `macos-latest` appears once (for aarch64) and `macos-13` appears once (for x86_64).

- [ ] **Step 3: Commit**

  ```bash
  git add .github/workflows/build.yml
  git commit -m "fix(ci): use macos-13 runner for x86_64 target to avoid cross-compilation"
  ```

---

### Task 2: Fix release.yml

**Files:**
- Modify: `.github/workflows/release.yml:24`

- [ ] **Step 1: Edit the platform for the x86_64 matrix entry**

  Open `.github/workflows/release.yml`. Find this block (around line 22–24):
  ```yaml
          - platform: macos-latest
            rust_target: aarch64-apple-darwin
          - platform: macos-latest
            rust_target: x86_64-apple-darwin
  ```

  Change only the second `macos-latest` (the one paired with `x86_64-apple-darwin`):
  ```yaml
          - platform: macos-latest
            rust_target: aarch64-apple-darwin
          - platform: macos-13
            rust_target: x86_64-apple-darwin
  ```

- [ ] **Step 2: Verify the file looks correct**

  Run:
  ```bash
  grep -n "platform:" .github/workflows/release.yml
  ```
  Expected output:
  ```
  17:          - platform: ubuntu-22.04
  19:          - platform: windows-latest
  21:          - platform: macos-latest
  23:          - platform: macos-13
  ```
  Confirm `macos-latest` appears once (for aarch64) and `macos-13` appears once (for x86_64).

- [ ] **Step 3: Commit**

  ```bash
  git add .github/workflows/release.yml
  git commit -m "fix(ci): use macos-13 runner for x86_64 target to avoid cross-compilation"
  ```
