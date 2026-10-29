# Clear the clippy backlog Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `cargo clippy -j4 -p <crate> --all-targets -- -D warnings` passes for `rocket-infra`, `rocket-export`, `rocket-app` and `rocket-import`, so the factory can gate all four.

**Architecture:** Four warnings remain (measured 2026-10-29 on Rust 1.99.0). Three are in `rocket-infra` and one is in `rocket-export`; `rocket-app` and `rocket-import` already pass. Each fix is mechanical and keeps behaviour. The "failing test" here is the clippy command itself, and the existing tests guard the behaviour.

**Tech Stack:** Rust, `cargo clippy`.

**Spec:** GitHub issue #122 (https://github.com/Snehal1112/rocket/issues/122). The warning list comes from a clippy run, not from the ticket text.

## Global Constraints

- Touch only the four files named in the tasks. No other formatting, import-order or lint edits.
- `crates/rocket-infra/**` matches the factory's risk rules. If the factory runs this plan, expect `needs-human`. Run it natively or approve it by hand.
- Cargo commands carry `-j4` and `-p <crate>`. Never a bare workspace run. Use the `--all-targets` form the ticket gives, because the benches and tests are where the warnings are.
- Do not add `#[allow(...)]` attributes. Fix each warning at its cause.
- No production unwrap calls. Tests keep their `.expect("...")` style.
- `~/.factory/projects/rocket/project.toml` is the owner's file. This plan does not edit it. The last step tells the owner which lines to add.

## Review Focus

1. The bench helper still alternates GET and POST by index parity. Pinned by the bench compiling with `--all-targets` and by running the check in Task 1, Step 5.
2. The cookie test still expects nine identical rejections after `0 true`. Pinned by `cookies_without_a_host_reject_with_not_available` passing unchanged.
3. The callback memory-budget check keeps its meaning (8 MiB per endpoint) and now fails at compile time if the constants grow. Pinned by `queued_callback_bytes_stay_within_the_memory_budget` still being present and passing.
4. `Box::default()` builds the same value as `Box::new(OAuth1Auth::default())`. Pinned by `inherit_none_and_unsupported` passing unchanged.
5. Fixing `rocket-infra` and `rocket-export` leaves `rocket-app` and `rocket-import` clean, because a warning in a path dependency fails its dependents. Pinned by the final four-crate run in Task 2, Step 5.

## File Structure

- Modify `crates/rocket-infra/benches/support/mod.rs` (line 172), `crates/rocket-infra/src/scripting/engine.rs` (line 4674), `crates/rocket-infra/src/callback_server.rs` (line 384), and `crates/rocket-export/src/postman_auth.rs` (line 391). Line numbers are from the measurement and may have moved; find the code by the snippets below.

---

### Task 1: Clear the three `rocket-infra` warnings

**Files:**
- Modify: `crates/rocket-infra/benches/support/mod.rs`
- Modify: `crates/rocket-infra/src/scripting/engine.rs`
- Modify: `crates/rocket-infra/src/callback_server.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: a `rocket-infra` that passes `-D warnings` with `--all-targets`, which Task 2 relies on for `rocket-app`.

- [ ] **Step 1: Read the code**

Read the three sites: `http_request` in `benches/support/mod.rs` (`if i % 2 == 0`), the end of `cookies_without_a_host_reject_with_not_available` in `engine.rs` (`expected.extend(std::iter::repeat("rok.cookies is not available here").take(9))`), and `queued_callback_bytes_stay_within_the_memory_budget` in `callback_server.rs` (`assert!(CHANNEL_CAPACITY * MAX_BODY_BYTES <= 8 * 1024 * 1024, "...")`). Confirm that `CHANNEL_CAPACITY` and `MAX_BODY_BYTES` are `const` items, because the third fix needs that.

- [ ] **Step 2: Run clippy and see it fail**

Run: `cargo clippy -j4 -p rocket-infra --all-targets -- -D warnings`
Expected: it fails with three distinct errors: `clippy::manual_is_multiple_of` at the bench helper (printed twice, because the bench and the test target both include that file), `clippy::manual_repeat_n` in `engine.rs`, and `clippy::assertions_on_constants` in `callback_server.rs`.

- [ ] **Step 3: Fix the three sites**

- `benches/support/mod.rs`: change `i % 2 == 0` to `i.is_multiple_of(2)`. `i` is a `usize`.
- `engine.rs`: change `std::iter::repeat("rok.cookies is not available here").take(9)` to `std::iter::repeat_n("rok.cookies is not available here", 9)`.
- `callback_server.rs`: keep the test and its doc comment, and wrap the existing assert in a const block: `const { assert!(CHANNEL_CAPACITY * MAX_BODY_BYTES <= 8 * 1024 * 1024, "queued callbacks may hold at most 8 MiB per endpoint") };`. Both constants are `pub const usize` items (`callback_server.rs:24` and `:26`), and a const assert with a literal message is allowed.

- [ ] **Step 4: Run the behaviour checks**

Run: `cargo test -j4 -p rocket-infra cookies_without_a_host_reject_with_not_available` and `cargo test -j4 -p rocket-infra queued_callback_bytes_stay_within_the_memory_budget`
Expected: both pass (1 passed each).

- [ ] **Step 5: Run clippy and see it pass**

Run: `cargo clippy -j4 -p rocket-infra --all-targets -- -D warnings`
Expected: no warnings and exit code 0. Run `git diff --stat` and confirm only the three listed files changed.

- [ ] **Step 6: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill, path-scoped to the three files. Subject: `chore(infra): clear the remaining clippy warnings`. Footer: `Relates to: #122`.

---

### Task 2: Clear the `rocket-export` warning and verify all four crates

**Files:**
- Modify: `crates/rocket-export/src/postman_auth.rs`

**Interfaces:**
- Consumes: Task 1's clean `rocket-infra`.
- Produces: four crates that pass `-D warnings`.

- [ ] **Step 1: Read the code**

Read `inherit_none_and_unsupported` in the `tests` module of `crates/rocket-export/src/postman_auth.rs`, around `Auth::OAuth1(Box::new(OAuth1Auth::default()))`.

- [ ] **Step 2: Run clippy and see it fail**

Run: `cargo clippy -j4 -p rocket-export --all-targets -- -D warnings`
Expected: it fails with one error, `clippy::box_default`, at `postman_auth.rs` in the test `inherit_none_and_unsupported`.

- [ ] **Step 3: Fix the site**

Change `Box::new(OAuth1Auth::default())` to `Box::default()`.

- [ ] **Step 4: Run the behaviour check**

Run: `cargo test -j4 -p rocket-export postman_auth`
Expected: all `postman_auth` tests pass, including `inherit_none_and_unsupported`.

- [ ] **Step 5: Verify all four crates**

Run each of these and expect exit code 0 with no warnings:
`cargo clippy -j4 -p rocket-export --all-targets -- -D warnings`,
`cargo clippy -j4 -p rocket-infra --all-targets -- -D warnings`,
`cargo clippy -j4 -p rocket-app --all-targets -- -D warnings`,
`cargo clippy -j4 -p rocket-import --all-targets -- -D warnings`.
Run `git diff --stat` and confirm only `crates/rocket-export/src/postman_auth.rs` changed in this task.

- [ ] **Step 6: Commit and hand the gate to the owner**

Commit with the `dev-workflow-skills:1-git-commit` skill, path-scoped to `crates/rocket-export/src/postman_auth.rs`. Subject: `chore(export): clear the box_default clippy warning`. Footer: `Fixes: #122`.

Then tell the owner to add these four paths to the clippy `[[settings.checks]]` table in `~/.factory/projects/rocket/project.toml` (the `paths = [...]` list that already holds the twelve gated crates): `"crates/rocket-app/**"`, `"crates/rocket-export/**"`, `"crates/rocket-import/**"`, `"crates/rocket-infra/**"`. Do not edit that file in this plan.

Next plan: none
