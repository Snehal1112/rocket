# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Check compilation
cargo check -p rocket-git

# Run all tests for this crate
cargo test -p rocket-git

# Run a specific test
cargo test -p rocket-git <test_name>

# Run tests with output
cargo test -p rocket-git -- --nocapture
```

## Architecture

`rocket-git` is a pure domain crate — no I/O wiring, no Tauri, no async. It defines the git abstraction layer used by the broader Rocket API client.

### Trait + Implementation pattern

`GitService` (`service.rs`) is the public trait used by consumers. `Git2Service` (`git2_service/mod.rs`) is the only implementation, backed by `libgit2` via the `git2` crate (with SSH support) — named after the `git2` crate it wraps, the same way `rocket-http`'s `ReqwestExecutor` is named after `reqwest`.

`git2_service/mod.rs` defines the `Git2Service` struct and its `GitService` impl, which just delegates each method to a free function in one of these `pub(super)` submodules:

| Submodule | Operations |
|---|---|
| `repo` | `is_repo`, `init`, `clone_repo` |
| `status_diff` | `status`, `diff_file`, `diff_staged` |
| `staging` | `stage`, `unstage`, `discard`, `commit`, `log`, `diff_commit` |
| `branch` | `branches`, `switch_branch`, `checkout_remote_branch`, `create_branch`, `delete_branch`, `merge_branch` |
| `remote` | `list_remotes`, `add_remote`, `remove_remote`, `set_remote_url`, `push`, `fetch`, `pull` |
| `stash` | `stash_list`, `stash_save`, `stash_pop`, `stash_apply`, `stash_drop` |
| `conflict` | `conflicts`, `resolve_conflict`, `abort_merge` |
| `helpers` | shared internals: `open_repo`, `build_callbacks`, `map_git2_status`, `build_simple_diff`, `ahead_behind`, path/worktree helpers |
| `ssh_host_verification` | offline SSH host-key classification against a `known_hosts` file: `classify_remote_host`, `classify_known_host`, `parse_ssh_endpoint`, `openssh_sha256_fingerprint` |
| `tls_certificate_verification` | diagnostic-only HTTPS certificate classification: `classify_tls_certificate`, `https_port`, `sha256_fingerprint` |
| `trust_store` | the `SshTrustStore` trait plus `SystemSshTrustStore`/`FixedPathSshTrustStore` |
| `tls_verification_contracts` (`#[cfg(test)]`) | local, no-network integration fixture: a real self-signed-cert HTTPS listener exercised through an actual `clone_repo` call |

These submodule files share their names with the top-level domain-type modules (`crate::branch`, `crate::stash`, ...) declared in `lib.rs` — that's intentional, not duplication: the top-level module owns the domain **type** (e.g. `crate::branch::Branch`), the `git2_service` submodule of the same name owns the libgit2-backed **implementation** of the operations on that type.

All methods take a `path: &str` argument — the repository root on disk. There is no persistent repository handle; `Repository::open()` is called per-operation.

### SSH host and TLS certificate verification

`Git2Service` is no longer a unit struct — it carries an injectable `Arc<dyn SshTrustStore>` (`Git2Service::new()` uses `~/.ssh/known_hosts` via `SystemSshTrustStore`; `Git2Service::with_trust_store(...)` / `with_known_hosts_path(...)` inject an alternative, e.g. for tests). `helpers::build_callbacks` registers a single `certificate_check` callback that branches on `cert.as_hostkey()` (SSH) vs `cert.as_x509()` (HTTPS): SSH failures go through `ssh_host_verification::classify_remote_host` against the trust store; TLS failures go through `tls_certificate_verification::classify_tls_certificate`. Both are diagnostic-only — the accept/reject decision is always delegated back to libgit2/libssh2/OpenSSL via `CertificateCheckStatus::CertificatePassthrough`, and neither ever writes a trust decision to disk (no trust-on-first-use).

`RemoteVerificationState::map_error` enriches the resulting `git2::Error` into a typed `DomainError::SshUnknownHost` / `SshHostKeyChanged` / `SshHostVerificationUnavailable` / `TlsCertificateInvalid`, but the two transports do **not** report a rejected certificate the same way and the mapping is gated accordingly:

- **SSH** (`ssh_libssh2.c`) preserves `GIT_ECERTIFICATE` through to the caller — enrichment is gated on `error.code() == ErrorCode::Certificate`.
- **HTTPS** (`httpclient.c::check_certificate`, libgit2 1.8.1) does **not** preserve it — a rejected cert restores the original "SSL certificate is invalid" error under a generic error code, so only its class (`ErrorClass::Ssl`) survives. Enrichment for the TLS branch is gated on that class instead.

Either way, enrichment only ever fires when the callback actually recorded a failure for that connection attempt, so a stale/unrelated error is never misattributed. This asymmetry was only discovered by the real end-to-end fixture in `tls_verification_contracts` — a synthetic `git2::Error::new(ErrorCode::Certificate, ...)` unit test alone would have hidden it.

### Domain types

Each module owns its types and re-exports them via `lib.rs`:

| Module | Types |
|---|---|
| `status` | `GitStatus` (enum), `FileStatus`, `RepoStatus` |
| `diff` | `FileDiff`, `DiffHunk`, `DiffLine`, `LineType` |
| `branch` | `Branch`, `BranchList` |
| `commit` | `CommitInfo` |
| `stash` | `StashEntry` |
| `conflict` | `ConflictFile`, `ConflictResolution` |
| `credentials` | `GitCredentials` (enum: SshKey, SshAgent, UserPass, Token) |
| `remote` | `RemoteInfo` |
| `remote_verification` | `SshHostFailure`, `SshHostFailureKind` (enum: UnknownHost, ChangedHost, VerificationUnavailable) — declared as a private `mod`, types re-exported at the crate root |

All types derive `Serialize`/`Deserialize` with `camelCase` field names (for Tauri IPC). Enums use `lowercase` variant names.

### Key implementation details

- **Status**: A file with both staged and unstaged changes emits two separate `FileStatus` entries (one with `staged: true`, one with `staged: false`).
- **Diff**: Uses a simplistic all-removals-then-all-additions approach in a single hunk (`build_simple_diff`). Not a proper Myers diff.
- **Commit signature**: Requires git config user identity (user.name and user.email) — no fallback. If missing, operations return a `DomainError::Internal`.
- **Pull**: Implements fetch + fast-forward or merge commit. On conflicts, writes the index and returns `Ok(())`, leaving the repo in merge-in-progress state for the frontend to resolve.
- **Conflict resolution**: `ConflictResolution` supports `Ours`, `Theirs`, or `Custom { content }`. Resolution writes the file, stages it, and clears the conflict marker in the index.

### Testing

Tests live in `#[cfg(test)]` blocks within each module. All integration tests use `tempfile::TempDir` for ephemeral repos. The `setup_repo()` helper in the `#[cfg(test)] mod tests` block at the bottom of `git2_service/mod.rs` creates a repo with an initial commit on `main` — call it directly in new tests rather than duplicating the setup.

`git2_service/safety_contracts.rs` is a separate `#[cfg(test)]`-only submodule dedicated to path-safety and atomicity contracts for the staging/diff/discard/conflict operations: path-traversal and absolute/Windows-style path rejection, symlink handling (a leaf symlink is staged as a symlink but not followed), and batch preflight atomicity (`stage`/`unstage`/`discard` reject the whole batch, mutating nothing, if any single path in it is invalid). Add new safety-contract tests here rather than mixing them into the per-operation submodules.
