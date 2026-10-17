# Home-dir and tilde-expansion helper Implementation Plan

> **For agentic workers:** This plan is executed by a `factory-builder` subagent through `superpowers:executing-plans`, skipping that skill's final whole-branch review, ledger scripts and progress file. Steps use checkbox (`- [ ]`) syntax. The driver commits, so skip any commit step.

**Goal:** `rocket-shared` owns the one home-directory function and the one tilde-expansion function, and every place that resolved the home directory or expanded `~` uses them.

**Architecture:** A new `rocket_shared::paths` module holds `home_dir()` (a thin call to `dirs::home_dir()`), `expand_tilde_with()` (the pure rules, tested without touching the environment) and `expand_tilde()` (the rules applied to `home_dir()`). Task 1 adds the module. Task 2 replaces the three hand-written copies. Task 3 replaces the eight `dirs::home_dir()` calls and removes the `dirs` dependency from `rocket-git`, `rocket-infra` and `src-tauri`.

**Tech Stack:** Rust, the `dirs` crate (version 5, already pinned in the workspace `Cargo.toml`).

**Spec:** `docs/superpowers/specs/2026-10-17-home-dir-tilde-helper-design.md`

## Global Constraints

- On Linux and macOS with `HOME` set, every migrated call site returns the value it returned before. Error types and messages stay the same, except the three accepted differences in the spec: a bare `~` now expands; `~user/x` in an SSH key path is left unchanged; an unset or empty `HOME` now resolves through `dirs`.
- `absolutize` in `crates/rocket-app/src/client_certificates.rs` is not changed.
- No IPC contract, persistence format or serde attribute changes. No `~\` support.
- No unwrap calls in production code, no shelling out to `git`.
- Run `cargo` only with `-j4` and per crate (`-p <crate>`). Never `cargo test --workspace` or `--all`, and never a bare `cargo check -j4` from the repository root, which builds the whole app. Do not build the `src-tauri` package (named `rocket`) yourself; the driver's checks stage does it.
- The tests never set `HOME`. They use `expand_tilde_with` with explicit homes, or compare with `rocket_shared::paths::home_dir()`.

## Review Focus

- An empty `HOME` must not become a relative home. `dirs` treats it as unset; `home_dir()` returns `Some` only for an absolute path. Task 1 pins this with `home_dir_is_absolute_when_present`.
- `~/` with a home gives the home directory itself, not a path with a trailing empty part. Task 1 pins this with `tilde_slash_gives_home`.
- `~user` and `~user/x` stay unchanged. Task 1 pins this with `tilde_user_is_unchanged`, and Task 2 pins it for certificates with `certificate_path_with_tilde_user_is_unresolved`.
- A text that merely contains `~` later, such as `a/~/b`, stays unchanged. Task 1 pins this with `tilde_inside_the_text_is_unchanged`.
- `grpc_service` must still say `cannot find the home directory` for a `~/` proto path when no home exists. Task 2 pins this with `tilde_proto_path_without_home_is_an_error`.

---

### Task 1: Add `rocket_shared::paths`

**Files:**
- Create: `crates/rocket-shared/src/paths.rs`
- Modify: `crates/rocket-shared/src/lib.rs` (add `pub mod paths;` in the module list, in alphabetical order after `oauth2`)
- Modify: `crates/rocket-shared/Cargo.toml` (add `dirs.workspace = true` under `[dependencies]`)

**Interfaces:**
- Produces: `pub fn home_dir() -> Option<PathBuf>`, `pub fn expand_tilde_with(input: &str, home: Option<&Path>) -> PathBuf`, `pub fn expand_tilde(input: &str) -> PathBuf`, all in `rocket_shared::paths`. Later tasks use exactly these names and types.

- [ ] **Step 1: Write the failing tests** in a `#[cfg(test)] mod tests` at the bottom of `paths.rs`, using `expand_tilde_with` with `Some(Path::new("/home/u"))` and `None`. Each test is named below with its assertions:
  - `tilde_alone_gives_home`: `"~"` with the home gives `/home/u`.
  - `tilde_slash_x_joins_home`: `"~/x"` gives `/home/u/x`; `"~/a/b"` gives `/home/u/a/b`.
  - `tilde_slash_gives_home`: `"~/"` gives `/home/u`.
  - `tilde_user_is_unchanged`: `"~user"` and `"~user/x"` come back as `PathBuf::from` of the same text.
  - `tilde_inside_the_text_is_unchanged`: `"a/~/b"`, `"/abs/~"` and `""` come back unchanged.
  - `plain_paths_are_unchanged`: `"/etc/hosts"` and `"rel/path"` come back unchanged, with and without a home.
  - `no_home_leaves_every_tilde_form_unchanged`: with `None`, `"~"`, `"~/x"` and `"~/"` come back as the same text.
  - `home_dir_is_absolute_when_present`: `if let Some(h) = home_dir() { assert!(h.is_absolute()) }`. It does not set `HOME`.
  - `expand_tilde_uses_home_dir`: `expand_tilde("~/x")` equals `expand_tilde_with("~/x", home_dir().as_deref())`.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-shared paths`. Expected: a compile error, because the module and functions do not exist yet.
- [ ] **Step 3: Implement** the three functions with the signatures above. `home_dir` returns `dirs::home_dir()` filtered to absolute paths. `expand_tilde_with` follows the rules table in the spec: only the exact text `~` and the prefix `~/` expand, and only when a home is given; everything else comes back as `PathBuf::from(input)`. `expand_tilde(input)` is `expand_tilde_with(input, home_dir().as_deref())`. Give each function a short doc comment, ending every comment with a full stop.
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-shared paths` and `cargo check -j4 -p rocket-shared`. Expected: all nine tests pass and the check is clean.

### Task 2: Replace the three hand-written copies

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-infra/src/reqwest_executor.rs` (`read_certificate_file`, about line 985)
- Modify: `crates/rocket-app/src/grpc_service.rs` (`resolve_proto_path`, about line 530)
- Modify: `crates/rocket-git/src/git2_service/helpers.rs` (the `GitCredentials::SshKey` arm, about line 171)
- Test: the existing `#[cfg(test)]` modules of the first two files (add one if absent)

**Interfaces:**
- Consumes: `rocket_shared::paths::{expand_tilde, expand_tilde_with, home_dir}` from Task 1.
- Produces: a private helper in `grpc_service.rs`, `fn expand_home_prefix(raw: &str, home: Option<&Path>) -> DomainResult<Option<PathBuf>>`. It returns `Ok(None)` when `raw` is not `~` or `~/...`, `Err(DomainError::InvalidInput("cannot find the home directory".into()))` when it is and `home` is `None`, and `Ok(Some(expanded))` otherwise, using `expand_tilde_with`.

- [ ] **Step 1: Write the pinning tests first** and run them against the old code, so each passes before the change:
  - In `reqwest_executor.rs`: `certificate_path_with_tilde_user_is_unresolved` calls `read_certificate_file("~nobody/cert.pem")` and expects `DomainError::InvalidInput` whose text contains `cannot be resolved`. `relative_certificate_path_is_unresolved` does the same for `"rel/cert.pem"`.
  - In `grpc_service.rs`: `tilde_user_proto_path_is_relative` calls `resolve_proto_path(None, "~nobody/x.proto")` and expects `InvalidInput` with `a relative proto path needs a collection`. `tilde_proto_path_uses_the_home_dir` expects `resolve_proto_path(None, "~/x.proto")` to give `home_dir().join("x.proto")` when `rocket_shared::paths::home_dir()` is `Some`.
- [ ] **Step 2: Write the failing test** `tilde_proto_path_without_home_is_an_error` for `expand_home_prefix("~/x.proto", None)`, expecting the error text `cannot find the home directory`. Also `expand_home_prefix("rel.proto", None)` is `Ok(None)` and `expand_home_prefix("~/x", Some(Path::new("/h")))` is `Ok(Some("/h/x"))`. Run `cargo test -j4 -p rocket-app grpc_service` and see it fail to compile.
- [ ] **Step 3: Implement.**
  - `read_certificate_file` computes `expanded` as `rocket_shared::paths::expand_tilde(path)` and keeps the `is_absolute` check, the error text and the file read unchanged. Update the doc comment to say a leading `~` or `~/` is the home directory.
  - `grpc_service.rs` adds `expand_home_prefix` and `resolve_proto_path` calls it with `rocket_shared::paths::home_dir().as_deref()`; when it returns `Some(path)`, that is returned with `extra` as before. The rest of the function is unchanged.
  - `helpers.rs` replaces the `if private_key_path.starts_with('~') { ... }` block with `let expanded = rocket_shared::paths::expand_tilde(private_key_path);`, and passes `expanded.as_path()` to `git2::Cred::ssh_key`. Update the comment to say `~` and `~/` are expanded and `~user` is left alone.
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-infra reqwest_executor`, `cargo test -j4 -p rocket-app grpc_service` and `cargo test -j4 -p rocket-git`. Expected: all pass, including the pinning tests from Step 1.
- [ ] **Step 5: Run** `grep -rn "HOME\|USERPROFILE" crates/rocket-infra/src crates/rocket-app/src crates/rocket-git/src --include='*.rs'`. Expected: no hit outside comments and tests that name those variables for another purpose (for example a `GEMINI_CLI_HOME` name).

### Task 3: Replace the `dirs::home_dir()` calls and drop `dirs`

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-git/src/git2_service/ssh_host_verification.rs` (`default_known_hosts_path`, about line 35)
- Modify: `crates/rocket-infra/src/aws_profile.rs` (`credentials_file_path`, about line 79)
- Modify: `src-tauri/src/commands/git.rs` (about lines 885 and 903), `src-tauri/src/commands/app.rs` (about lines 9 and 20), `src-tauri/src/lib.rs` (about line 261), `src-tauri/src/agent_session/scratch.rs` (about line 130)
- Modify: `crates/rocket-git/Cargo.toml`, `crates/rocket-infra/Cargo.toml`, `src-tauri/Cargo.toml` (remove the `dirs.workspace = true` line from each)

**Interfaces:**
- Consumes: `rocket_shared::paths::home_dir() -> Option<PathBuf>` from Task 1.

- [ ] **Step 1: Confirm the call sites** with `grep -rn "dirs::" crates src-tauri/src --include='*.rs'`. Expected: exactly the eight sites above, all `dirs::home_dir()`. If another `dirs::` function appears, stop and keep `dirs` in that crate.
- [ ] **Step 2: Replace each `dirs::home_dir()` with `rocket_shared::paths::home_dir()`.** Each site keeps its own handling of a missing home: an `Option` result, an empty list, or the existing error text (`Home directory not found`, `Home directory could not be determined`). `scratch.rs` keeps its order: the runtime directory, then the home directory, then the temp directory.
- [ ] **Step 3: Remove** the `dirs.workspace = true` line from the three `Cargo.toml` files. The workspace `dirs = "5"` entry and the `rocket-shared` dependency from Task 1 stay.
- [ ] **Step 4: Run** `cargo check -j4 -p rocket-shared`, `cargo check -j4 -p rocket-git` and `cargo check -j4 -p rocket-infra`. Expected: all clean, with no unused-dependency or unresolved-import errors. Do not build the `rocket` (src-tauri) package yourself: a first build in a fresh worktree compiles the whole app and stalls. The driver's checks stage runs `cargo check -j4` for the `src-tauri` changes.
- [ ] **Step 5: Run** `cargo test -j4 -p rocket-infra aws_profile` and `cargo test -j4 -p rocket-git`. Expected: all pass.
- [ ] **Step 6: Verify no copies remain.** Run `grep -rn "dirs::\|\"HOME\"\|\"USERPROFILE\"" crates src-tauri/src --include='*.rs'`. Expected: hits only in `crates/rocket-shared/src/paths.rs` (the `dirs::home_dir()` call) and, if any, test code or comments about other variables. Paste the output into the job's final message.

Next plan: none
