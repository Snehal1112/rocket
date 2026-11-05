# Phase 5 fixes, Plan 4: dependency cleanup (C-2, C-3, C-4)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The domain crates stop carrying dependencies they do not use in production: `tempfile` in `rocket-import`, `reqwest` and the full `tokio` feature set in `rocket-http`, and `git2` in `src-tauri`.

**Architecture:** Three small, independent edits. Each is proven by `cargo check` / `cargo tree` plus the crate tests.

**Tech Stack:** Cargo manifests, Rust.

**Spec:** `.claude/reviews/2026-11-04-ddd-phase5-audit.md` (C-2, C-3, C-4).

## Global Constraints

- No behaviour change. No new dependency except `url` (already a dependency of `rocket-http`).
- No `unwrap` in production code. `cargo` commands take `-j4`; never `--workspace`.

## Review Focus

- `rocket-import` tests that use `tempfile` still compile (dev-dependency).
- `is_loopback_http` gives the same answers for `http://localhost`, `http://127.0.0.1`, `http://[::1]`, `http://localhost.evil.com` and a URL with user info.
- The libgit2 timeouts are still set once, before any other thread starts (the call stays at the same place in `lib.rs`).
- `cargo tree -p rocket-http` no longer lists `reqwest` as a direct dependency.

---

### Task 1: `tempfile` becomes a dev-dependency (C-3)

**Files:**
- Modify: `crates/rocket-import/Cargo.toml`

**Interfaces:** none.

- [ ] **Step 1: Write the check.** `grep -rn "tempfile" crates/rocket-import/src --include=*.rs` and confirm every hit is inside a `#[cfg(test)]` block. (Expected: all test-only. If one is in production code, stop and rule on it.)
- [ ] **Step 2: Run** `cargo check -j4 -p rocket-import` after moving the line `tempfile = "3"` from `[dependencies]` to `[dev-dependencies]` — Expected: pass.
- [ ] **Step 3: Run** `cargo test -j4 -p rocket-import --no-run` — Expected: builds.
- [ ] **Step 4: Run** `cargo check -j4 -p rocket-infra` — Expected: pass (infra has its own `tempfile`).
- [ ] **Step 5: Commit** with the `dev-workflow-skills:1-git-commit` skill (pathspec `crates/rocket-import/Cargo.toml`, `Cargo.lock` if it changed).

### Task 2: `rocket-http` stops using `reqwest` (C-2)

**Files:**
- Modify: `crates/rocket-http/Cargo.toml` (lines 9, 13: `tokio`, `reqwest`), `crates/rocket-http/src/spec_fetch.rs` (`is_loopback_http` ~155 and its callers and tests), `crates/rocket-http/CLAUDE.md` (the "no leaks remain" line)
- Test: `spec_fetch.rs` `mod tests`

**Interfaces:**
- Produces: `pub fn is_loopback_http(url: &url::Url) -> bool` (was `&reqwest::Url`; `reqwest::Url` is a re-export of `url::Url`, so callers that pass a parsed URL keep compiling).

- [ ] **Step 1: Write the test first.** `is_loopback_http_matrix` in `spec_fetch.rs` with the five URLs from Review Focus and their expected answers (`true, true, true, false, false`). Run `cargo test -j4 -p rocket-http is_loopback_http_matrix` — Expected: PASS against the old code (it pins behaviour before the change).
- [ ] **Step 2: Run** `grep -rn "reqwest" crates/rocket-http/src --include=*.rs` — Expected: only `spec_fetch.rs`. If more, list them in the ledger and keep the dependency for those.
- [ ] **Step 3: Implement.** Change the parameter type to `url::Url`, remove `reqwest` from `Cargo.toml`. Replace `tokio.workspace = true` with the features `rocket-http` really uses (`grep -rn "tokio::" crates/rocket-http/src` to find them, for example `rt`, `sync`, `time`, `macros`).
- [ ] **Step 4: Run** `cargo check -j4`, `cargo test -j4 -p rocket-http`, `cargo tree -j4 -p rocket-http -e normal --depth 1 | grep reqwest` — Expected: pass, and no output from the grep.
- [ ] **Step 5: Commit** (pathspec the manifest, `spec_fetch.rs`, the crate `CLAUDE.md`, `Cargo.lock`).

### Task 3: libgit2 timeouts move into `rocket-git` (C-4)

**Files:**
- Modify: `crates/rocket-git/src/lib.rs` (new `pub fn set_network_timeouts()`), `src-tauri/src/lib.rs:180-190` (call it), `src-tauri/Cargo.toml:57` (drop `git2` if nothing else uses it), `crates/rocket-git/CLAUDE.md` (say it is the only way in, with no exception)
- Test: `crates/rocket-git/src/` unit test

**Interfaces:**
- Produces: `pub fn set_network_timeouts()` in `rocket-git`. It sets connect timeout 10_000 ms and server timeout 60_000 ms, keeps the `unsafe` block and its safety comment (process-global state, call before any other thread exists). It ignores the `Result`s the way the current code does.

- [ ] **Step 1: Write the failing test.** `set_network_timeouts_can_be_called` (call it, then read `git2::opts::get_server_connect_timeout_in_milliseconds()` if the crate version exposes the getter; otherwise only assert it does not panic). Run `cargo test -j4 -p rocket-git set_network_timeouts` — Expected: compile error (red).
- [ ] **Step 2: Implement** the function in `rocket-git` with the existing comment text moved along.
- [ ] **Step 3: Replace** the `unsafe` block in `src-tauri/src/lib.rs` with `rocket_git::set_network_timeouts();` at the same position. Run `grep -rn "git2" src-tauri/src src-tauri/Cargo.toml` — Expected: if no other use remains, remove the dependency line.
- [ ] **Step 4: Run** `cargo check -j4`, `cargo test -j4 -p rocket-git`, `cargo clippy -j4 -p rocket-git -- -D warnings` — Expected: pass.
- [ ] **Step 5: Commit** (pathspec the touched files, `Cargo.lock`).
