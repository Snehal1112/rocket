use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use git2::{Repository, Status};
use rocket_shared::error::{DomainError, DomainResult};

use crate::credentials::GitCredentials;
use crate::diff::{DiffHunk, DiffLine, LineType};
use crate::status::GitStatus;
use crate::{SshHostFailure, SshHostFailureKind, TlsCertificateFailure};

use super::ssh_host_verification::{
    classify_remote_host, openssh_sha256_fingerprint, SshHostClassification,
};
use super::tls_certificate_verification::classify_tls_certificate;

/// A diagnostic recorded from a rejected certificate, keyed by which
/// transport produced it. Recording is diagnostic-only — see
/// `certificate_failure_policy`.
#[derive(Clone)]
enum CertificateFailure {
    Ssh(SshHostFailure),
    Tls(TlsCertificateFailure),
}

#[derive(Clone, Default)]
pub(super) struct RemoteVerificationState {
    failure: Arc<Mutex<Option<CertificateFailure>>>,
}

impl RemoteVerificationState {
    /// Map a failed clone/fetch/push/pull to a domain error, enriching it
    /// with the classification our own `certificate_check` callback
    /// recorded — if any — for the transport that produced this error.
    ///
    /// The two transports do not report a rejected certificate the same
    /// way: SSH (`ssh_libssh2.c`) preserves `GIT_ECERTIFICATE` through to
    /// the caller, but libgit2's OpenSSL HTTPS stream
    /// (`httpclient.c::check_certificate`) does not — it restores the
    /// original "SSL certificate is invalid" error under a generic error
    /// code, only its class (`Ssl`) survives. So SSH enrichment is gated on
    /// the code exactly matching `Certificate`; TLS enrichment on the
    /// class matching `Ssl`. Either way, enrichment only ever fires when
    /// our callback actually recorded a failure for this connection
    /// attempt, so a recorded TLS failure never gets attached to an
    /// unrelated error.
    pub(super) fn map_error(&self, error: git2::Error) -> DomainError {
        let failure = self.failure.lock().ok().and_then(|failure| failure.clone());
        match failure {
            Some(CertificateFailure::Ssh(SshHostFailure {
                kind,
                host,
                port,
                algorithm,
                fingerprint,
            })) if error.code() == git2::ErrorCode::Certificate => match kind {
                SshHostFailureKind::UnknownHost => DomainError::SshUnknownHost {
                    host,
                    port,
                    algorithm,
                    fingerprint,
                },
                SshHostFailureKind::ChangedHost => DomainError::SshHostKeyChanged {
                    host,
                    port,
                    algorithm,
                    fingerprint,
                },
                SshHostFailureKind::VerificationUnavailable => {
                    DomainError::SshHostVerificationUnavailable {
                        host,
                        port,
                        algorithm,
                        fingerprint,
                    }
                }
            },
            Some(CertificateFailure::Tls(TlsCertificateFailure {
                host,
                port,
                fingerprint,
            })) if error.code() == git2::ErrorCode::Certificate
                || error.class() == git2::ErrorClass::Ssl =>
            {
                DomainError::TlsCertificateInvalid {
                    host,
                    port,
                    fingerprint,
                }
            }
            _ => DomainError::Internal(error.to_string()),
        }
    }
}

/// Build credential and fail-closed verification callbacks for remote operations.
///
/// The callback includes a one-shot guard: if libgit2 calls it more than once
/// (which happens when credentials are rejected and it retries), we return an
/// error on the second call so the operation fails fast instead of looping.
pub(super) fn build_callbacks(
    creds: &GitCredentials,
    remote_url: &str,
    known_hosts_path: Option<PathBuf>,
) -> (git2::RemoteCallbacks<'static>, RemoteVerificationState) {
    let mut callbacks = git2::RemoteCallbacks::new();
    let verification = RemoteVerificationState::default();
    let failure_slot = Arc::clone(&verification.failure);
    let remote_url = remote_url.to_owned();
    // This callback is invoked only after libgit2's HTTPS certificate or SSH
    // host-key verification fails. Classification is diagnostic only, and
    // passthrough always preserves libgit2's rejection.
    callbacks.certificate_check(move |cert, host| {
        if let Some(host_key) = cert.as_hostkey() {
            let algorithm = host_key
                .hostkey_type()
                .map(|key_type| key_type.name().to_owned())
                .unwrap_or_else(|| "unknown".into());
            let native_digest = host_key.hash_sha256().map(|digest| digest.as_slice());
            let classification = match host_key.hostkey() {
                Some(raw_key) => classify_remote_host(
                    &remote_url,
                    host,
                    known_hosts_path.as_deref(),
                    &algorithm,
                    raw_key,
                    native_digest,
                ),
                None => SshHostClassification::Failure(SshHostFailure {
                    kind: SshHostFailureKind::VerificationUnavailable,
                    host: host.to_owned(),
                    port: 22,
                    algorithm,
                    fingerprint: openssh_sha256_fingerprint(native_digest, &[]),
                }),
            };
            if let SshHostClassification::Failure(failure) = classification {
                if let Ok(mut slot) = failure_slot.lock() {
                    if slot.is_none() {
                        *slot = Some(CertificateFailure::Ssh(failure));
                    }
                }
            }
        } else if let Some(x509) = cert.as_x509() {
            let failure = classify_tls_certificate(&remote_url, host, x509.data());
            if let Ok(mut slot) = failure_slot.lock() {
                if slot.is_none() {
                    *slot = Some(CertificateFailure::Tls(failure));
                }
            }
        }
        Ok(certificate_failure_policy())
    });
    let creds = creds.clone();
    let mut used = false;
    callbacks.credentials(move |_url, username, _allowed| {
        if used {
            return Err(git2::Error::from_str(
                "authentication failed: check credentials and remote URL",
            ));
        }
        used = true;
        match &creds {
            GitCredentials::SshKey {
                private_key_path,
                passphrase,
            } => {
                // libgit2 does not expand `~` — do it ourselves so that paths
                // like `~/.ssh/id_ed25519_snehal1112` resolve correctly.
                let expanded = if private_key_path.starts_with('~') {
                    std::env::var("HOME")
                        .map(|home| private_key_path.replacen('~', &home, 1))
                        .unwrap_or_else(|_| private_key_path.clone())
                } else {
                    private_key_path.clone()
                };
                git2::Cred::ssh_key(
                    username.unwrap_or("git"),
                    None,
                    Path::new(&expanded),
                    passphrase.as_deref(),
                )
            }
            GitCredentials::SshAgent => git2::Cred::ssh_key_from_agent(username.unwrap_or("git")),
            GitCredentials::UserPass {
                username: u,
                password,
            } => git2::Cred::userpass_plaintext(u, password),
            GitCredentials::Token { token } => git2::Cred::userpass_plaintext("oauth2", token),
        }
    });
    (callbacks, verification)
}

fn certificate_failure_policy() -> git2::CertificateCheckStatus {
    git2::CertificateCheckStatus::CertificatePassthrough
}

/// Open a git repository at the given path.
pub(super) fn open_repo(path: &str) -> DomainResult<Repository> {
    Repository::open(path).map_err(|e| DomainError::Internal(e.to_string()))
}

/// A path in Git's repository-relative, forward-slash-separated namespace.
///
/// Invalid input is rejected rather than normalized so every caller uses the
/// exact path that was authorized.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct GitRelativePath(String);

impl GitRelativePath {
    pub(super) fn parse(value: &str) -> DomainResult<Self> {
        if value.is_empty() {
            return Err(invalid_git_path(value, "path must not be empty"));
        }
        if value.contains('\0') {
            return Err(invalid_git_path(value, "path must not contain NUL"));
        }
        if value.starts_with('/') {
            return Err(invalid_git_path(value, "absolute paths are not allowed"));
        }
        if value.contains('\\') {
            return Err(invalid_git_path(
                value,
                "backslashes and mixed separators are not allowed",
            ));
        }

        let bytes = value.as_bytes();
        if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
            return Err(invalid_git_path(
                value,
                "Windows drive and prefix paths are not allowed",
            ));
        }

        let components = value.split('/').collect::<Vec<_>>();
        #[cfg(windows)]
        if components.iter().any(|component| component.contains(':')) {
            return Err(invalid_git_path(
                value,
                "Windows alternate-data-stream and prefix separators are not allowed",
            ));
        }
        if components.iter().any(|component| component.is_empty()) {
            return Err(invalid_git_path(
                value,
                "empty, repeated, and trailing path components are not allowed",
            ));
        }
        if components
            .iter()
            .any(|component| matches!(*component, "." | ".."))
        {
            return Err(invalid_git_path(
                value,
                "dot path components are not allowed",
            ));
        }
        if components
            .first()
            .is_some_and(|component| component.eq_ignore_ascii_case(".git"))
        {
            return Err(invalid_git_path(
                value,
                "the top-level .git directory is not allowed",
            ));
        }
        if Path::new(value)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(invalid_git_path(
                value,
                "path contains a non-normal native component",
            ));
        }

        Ok(Self(value.to_string()))
    }

    pub(super) fn as_str(&self) -> &str {
        &self.0
    }

    pub(super) fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }
}

fn invalid_git_path(value: &str, reason: &str) -> DomainError {
    DomainError::InvalidInput(format!("invalid Git-relative path {value:?}: {reason}"))
}

pub(super) fn validate_batch_paths(paths: &[GitRelativePath]) -> DomainResult<()> {
    for (index, path) in paths.iter().enumerate() {
        for other in &paths[index + 1..] {
            let path_value = path.as_str();
            let other_value = other.as_str();
            let overlaps = path_value == other_value
                || is_path_ancestor(path_value, other_value)
                || is_path_ancestor(other_value, path_value);
            if overlaps {
                return Err(DomainError::InvalidInput(format!(
                    "overlapping Git-relative paths are not allowed: {path_value:?} and {other_value:?}"
                )));
            }
        }
    }
    Ok(())
}

fn is_path_ancestor(parent: &str, child: &str) -> bool {
    child
        .strip_prefix(parent)
        .is_some_and(|remainder| remainder.starts_with('/'))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WorktreeLeafKind {
    Missing,
    File,
    Directory,
    Symlink,
    Other,
}

#[derive(Debug)]
pub(super) struct InspectedWorktreePath {
    relative: GitRelativePath,
    full_path: PathBuf,
    leaf_kind: WorktreeLeafKind,
}

impl InspectedWorktreePath {
    pub(super) fn relative(&self) -> &GitRelativePath {
        &self.relative
    }

    pub(super) fn full_path(&self) -> &Path {
        &self.full_path
    }

    pub(super) fn leaf_kind(&self) -> WorktreeLeafKind {
        self.leaf_kind
    }
}

/// Inspect a validated path without following its leaf or any parent symlink.
///
/// Existing parents are canonicalized and checked against the worktree root.
/// This uses `std::fs` path checks and therefore cannot eliminate TOCTOU races
/// where another process swaps a component after inspection.
pub(super) fn inspect_worktree_path(
    repo: &Repository,
    relative: GitRelativePath,
) -> DomainResult<InspectedWorktreePath> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| DomainError::InvalidInput("repository has no working directory".into()))?;
    let root = fs::canonicalize(workdir).map_err(|error| DomainError::Io(error.to_string()))?;
    let components = relative.as_str().split('/').collect::<Vec<_>>();
    let (leaf, parents) = components
        .split_last()
        .ok_or_else(|| invalid_git_path(relative.as_str(), "path must not be empty"))?;

    let mut current = root.clone();
    let mut parent_missing = false;
    for component in parents {
        current.push(component);
        if parent_missing {
            continue;
        }

        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(invalid_git_path(
                    relative.as_str(),
                    "symlinked parent components are not allowed",
                ));
            }
            Ok(metadata) if metadata.is_dir() => {
                let canonical = fs::canonicalize(&current)
                    .map_err(|error| DomainError::Io(error.to_string()))?;
                if !canonical.starts_with(&root) {
                    return Err(invalid_git_path(
                        relative.as_str(),
                        "parent component escapes the working directory",
                    ));
                }
            }
            Ok(_) => {
                return Err(invalid_git_path(
                    relative.as_str(),
                    "parent component is not a directory",
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                parent_missing = true;
            }
            Err(error) => return Err(DomainError::Io(error.to_string())),
        }
    }

    current.push(leaf);
    let leaf_kind = if parent_missing {
        WorktreeLeafKind::Missing
    } else {
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => WorktreeLeafKind::Symlink,
            Ok(metadata) if metadata.is_file() => WorktreeLeafKind::File,
            Ok(metadata) if metadata.is_dir() => WorktreeLeafKind::Directory,
            Ok(_) => WorktreeLeafKind::Other,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => WorktreeLeafKind::Missing,
            Err(error) => return Err(DomainError::Io(error.to_string())),
        }
    };

    Ok(InspectedWorktreePath {
        relative,
        full_path: current,
        leaf_kind,
    })
}

/// Map a git2 status bitflag to a (GitStatus, staged) pair.
pub(super) fn map_git2_status(status: Status) -> (GitStatus, bool) {
    if status.contains(Status::CONFLICTED) {
        return (GitStatus::Conflicted, false);
    }

    // Index (staged) flags take priority when present.
    if status.contains(Status::INDEX_NEW) {
        return (GitStatus::Added, true);
    }
    if status.contains(Status::INDEX_MODIFIED) {
        return (GitStatus::Modified, true);
    }
    if status.contains(Status::INDEX_DELETED) {
        return (GitStatus::Deleted, true);
    }
    if status.contains(Status::INDEX_RENAMED) {
        return (GitStatus::Renamed, true);
    }

    // Work-tree (unstaged) flags.
    if status.contains(Status::WT_NEW) {
        return (GitStatus::Untracked, false);
    }
    if status.contains(Status::WT_MODIFIED) {
        return (GitStatus::Modified, false);
    }
    if status.contains(Status::WT_DELETED) {
        return (GitStatus::Deleted, false);
    }
    if status.contains(Status::WT_RENAMED) {
        return (GitStatus::Renamed, false);
    }

    (GitStatus::Unchanged, false)
}

/// Read the content of a file from the HEAD commit tree.
pub(super) fn get_head_content(repo: &Repository, file: &GitRelativePath) -> Option<String> {
    let head = repo.head().ok()?;
    let commit = head.peel_to_commit().ok()?;
    let tree = commit.tree().ok()?;
    let entry = tree.get_path(file.as_path()).ok()?;
    let blob = repo.find_blob(entry.id()).ok()?;
    std::str::from_utf8(blob.content()).ok().map(String::from)
}

/// Read the content of a file from the staging index.
pub(super) fn get_index_content(repo: &Repository, file: &GitRelativePath) -> Option<String> {
    let index = repo.index().ok()?;
    let entry = index.get_path(file.as_path(), 0)?;
    let blob = repo.find_blob(entry.id).ok()?;
    std::str::from_utf8(blob.content()).ok().map(String::from)
}

/// Build a simple line-by-line diff producing hunks.
///
/// Produces a single hunk with all old lines as removals followed by all new
/// lines as additions. This is structurally correct for Monaco's DiffEditor,
/// which applies its own Myers diff on `oldContent`/`newContent` and ignores
/// the hunk structure. VisualDiffView also parses `oldContent`/`newContent`
/// directly and does not rely on hunks, so both consumers are unaffected.
///
/// Do NOT use `hunks` for semantic diff consumers — replace with the `similar`
/// crate for a proper Myers diff when hunk-level accuracy is needed.
pub(super) fn build_simple_diff(old: &Option<String>, new: &Option<String>) -> Vec<DiffHunk> {
    let old_lines: Vec<&str> = old
        .as_deref()
        .map(|s| s.lines().collect())
        .unwrap_or_default();
    let new_lines: Vec<&str> = new
        .as_deref()
        .map(|s| s.lines().collect())
        .unwrap_or_default();

    if old_lines == new_lines {
        return Vec::new();
    }

    let mut lines = Vec::new();
    for l in &old_lines {
        lines.push(DiffLine {
            content: l.to_string(),
            line_type: LineType::Remove,
        });
    }
    for l in &new_lines {
        lines.push(DiffLine {
            content: l.to_string(),
            line_type: LineType::Add,
        });
    }

    vec![DiffHunk {
        old_start: 1,
        old_lines: old_lines.len() as u32,
        new_start: 1,
        new_lines: new_lines.len() as u32,
        lines,
    }]
}

/// Count the number of files changed in a commit relative to its first parent.
/// For the initial commit (no parent), diffs against an empty tree.
pub(super) fn count_commit_files(repo: &Repository, commit: &git2::Commit) -> usize {
    let new_tree = match commit.tree() {
        Ok(t) => t,
        Err(_) => return 0,
    };
    let old_tree: Option<git2::Tree> = commit.parent(0).ok().and_then(|p| p.tree().ok());

    repo.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), None)
        .ok()
        .and_then(|d| d.stats().ok())
        .map(|s| s.files_changed())
        .unwrap_or(0)
}

/// Extract the current branch name from the repository HEAD.
pub(super) fn branch_name(repo: &Repository) -> String {
    repo.head()
        .ok()
        .and_then(|r| r.shorthand().map(String::from))
        .unwrap_or_else(|| "main".to_string())
}

/// Compute how many commits the local branch is ahead/behind the upstream.
pub(super) fn ahead_behind(repo: &Repository) -> (usize, usize) {
    let head = match repo.head() {
        Ok(r) => r,
        Err(_) => return (0, 0),
    };

    let local_oid = match head.target() {
        Some(oid) => oid,
        None => return (0, 0),
    };

    let branch_name = head.shorthand().unwrap_or("main");

    let branch = match repo.find_branch(branch_name, git2::BranchType::Local) {
        Ok(b) => b,
        Err(_) => return (0, 0),
    };

    // Try the configured upstream first.
    let upstream_oid = branch
        .upstream()
        .ok()
        .and_then(|u| u.get().target())
        // Fall back to refs/remotes/<remote>/<branch> for each configured remote.
        .or_else(|| {
            let remotes = repo.remotes().ok()?;
            remotes.iter().flatten().find_map(|remote_name| {
                let refname = format!("refs/remotes/{}/{}", remote_name, branch_name);
                repo.find_reference(&refname).ok().and_then(|r| r.target())
            })
        });

    match upstream_oid {
        Some(oid) => repo.graph_ahead_behind(local_oid, oid).unwrap_or((0, 0)),
        None => (0, 0),
    }
}

/// Remove any untracked worktree file whose content is byte-identical to
/// what `target_tree` specifies at that path, so a subsequent checkout can
/// recreate it fresh instead of libgit2 flagging the pre-existing file as a
/// conflict — matching real git's tolerance for adopting untracked files
/// that already match (checkout classifies any pre-existing workdir file at
/// a path a checkout wants to *add* as a conflict regardless of content, so
/// staging it into the index first does not help; only clearing it does).
/// If any untracked plain file collides with a *different*-content target
/// entry, nothing is removed and this returns a `Conflict` error, so a mixed
/// batch (some matching, some genuinely colliding) never leaves a partial
/// mutation behind — either all matches are cleared, or none are.
pub(super) fn clear_matching_untracked_paths(
    repo: &Repository,
    target_tree: &git2::Tree,
) -> DomainResult<()> {
    let baseline = repo
        .index()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let workdir = repo
        .workdir()
        .ok_or_else(|| DomainError::Internal("repository has no worktree".into()))?;

    let mut matches = Vec::new();
    let mut mismatches = Vec::new();
    target_tree
        .walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
            if entry.kind() != Some(git2::ObjectType::Blob) {
                return git2::TreeWalkResult::Ok;
            }
            let Some(name) = entry.name() else {
                return git2::TreeWalkResult::Ok;
            };
            let relative = format!("{dir}{name}");
            if baseline.get_path(Path::new(&relative), 0).is_some() {
                // Already tracked at this path — not an untracked collision.
                return git2::TreeWalkResult::Ok;
            }
            let full_path = workdir.join(&relative);
            let on_disk = match fs::symlink_metadata(&full_path) {
                Ok(metadata) if metadata.is_file() => fs::read(&full_path).ok(),
                // Not a plain file (missing, directory, symlink, ...) — leave
                // it for the real checkout to classify.
                _ => None,
            };
            let Some(on_disk) = on_disk else {
                return git2::TreeWalkResult::Ok;
            };
            let Ok(object) = entry.to_object(repo) else {
                return git2::TreeWalkResult::Ok;
            };
            let Some(blob) = object.as_blob() else {
                return git2::TreeWalkResult::Ok;
            };
            if blob.content() == on_disk.as_slice() {
                matches.push(full_path);
            } else {
                mismatches.push(relative);
            }
            git2::TreeWalkResult::Ok
        })
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    if !mismatches.is_empty() {
        return Err(DomainError::Conflict(format!(
            "untracked file(s) would be overwritten with different content: {}",
            mismatches.join(", ")
        )));
    }

    for full_path in matches {
        fs::remove_file(&full_path).map_err(|e| DomainError::Io(e.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        certificate_failure_policy, CertificateFailure, GitRelativePath, RemoteVerificationState,
    };
    use crate::{SshHostFailure, SshHostFailureKind, TlsCertificateFailure};
    use rocket_shared::error::DomainError;
    use std::sync::{Arc, Mutex};

    #[test]
    fn failed_remote_verification_is_never_overridden() {
        assert!(matches!(
            certificate_failure_policy(),
            git2::CertificateCheckStatus::CertificatePassthrough
        ));
    }

    fn verification_state(kind: SshHostFailureKind) -> RemoteVerificationState {
        RemoteVerificationState {
            failure: Arc::new(Mutex::new(Some(CertificateFailure::Ssh(SshHostFailure {
                kind,
                host: "git.example.com".into(),
                port: 22,
                algorithm: "ssh-ed25519".into(),
                fingerprint: "SHA256:abc".into(),
            })))),
        }
    }

    fn tls_verification_state() -> RemoteVerificationState {
        RemoteVerificationState {
            failure: Arc::new(Mutex::new(Some(CertificateFailure::Tls(
                TlsCertificateFailure {
                    host: "git.example.com".into(),
                    port: 443,
                    fingerprint: "SHA256:abc".into(),
                },
            )))),
        }
    }

    fn certificate_error() -> git2::Error {
        git2::Error::new(
            git2::ErrorCode::Certificate,
            git2::ErrorClass::Ssl,
            "certificate rejected",
        )
    }

    #[test]
    fn certificate_errors_map_each_ssh_failure_to_its_domain_variant() {
        assert!(matches!(
            verification_state(SshHostFailureKind::UnknownHost).map_error(certificate_error()),
            DomainError::SshUnknownHost {
                ref host,
                port: 22,
                ref algorithm,
                ref fingerprint,
            } if host == "git.example.com"
                && algorithm == "ssh-ed25519"
                && fingerprint == "SHA256:abc"
        ));
        assert!(matches!(
            verification_state(SshHostFailureKind::ChangedHost).map_error(certificate_error()),
            DomainError::SshHostKeyChanged { .. }
        ));
        assert!(matches!(
            verification_state(SshHostFailureKind::VerificationUnavailable)
                .map_error(certificate_error()),
            DomainError::SshHostVerificationUnavailable { .. }
        ));
    }

    #[test]
    fn certificate_error_maps_recorded_tls_failure_to_domain_variant() {
        assert!(matches!(
            tls_verification_state().map_error(certificate_error()),
            DomainError::TlsCertificateInvalid {
                ref host,
                port: 443,
                ref fingerprint,
            } if host == "git.example.com" && fingerprint == "SHA256:abc"
        ));
    }

    /// libgit2's OpenSSL stream backend does not preserve `GIT_ECERTIFICATE`
    /// through to the caller for a rejected HTTPS certificate — the error
    /// that actually reaches Rust has `ErrorCode::GenericError` with
    /// `ErrorClass::Ssl` (confirmed against libgit2 1.8.1's
    /// `httpclient.c::check_certificate`, which returns a raw `-1` here
    /// rather than `GIT_ECERTIFICATE`). The mapping must still use the
    /// classification our own callback recorded, not just the error code.
    #[test]
    fn generic_ssl_class_error_still_maps_recorded_tls_failure() {
        let error = git2::Error::new(
            git2::ErrorCode::GenericError,
            git2::ErrorClass::Ssl,
            "the SSL certificate is invalid",
        );
        assert!(matches!(
            tls_verification_state().map_error(error),
            DomainError::TlsCertificateInvalid {
                ref host,
                port: 443,
                ref fingerprint,
            } if host == "git.example.com" && fingerprint == "SHA256:abc"
        ));
    }

    #[test]
    fn generic_ssl_class_error_without_a_recorded_failure_stays_internal() {
        let error = git2::Error::new(
            git2::ErrorCode::GenericError,
            git2::ErrorClass::Ssl,
            "connection reset",
        );
        let mapped = RemoteVerificationState::default()
            .map_error(error)
            .to_string();
        assert!(mapped.contains("connection reset"));
    }

    #[test]
    fn non_certificate_error_does_not_use_recorded_ssh_failure() {
        let state = verification_state(SshHostFailureKind::UnknownHost);
        let error = git2::Error::new(
            git2::ErrorCode::Auth,
            git2::ErrorClass::Ssh,
            "authentication rejected",
        );

        let mapped = state.map_error(error).to_string();

        assert!(mapped.contains("authentication rejected"));
        assert!(!mapped.contains("unknown SSH host"));
    }

    #[test]
    fn git_relative_path_accepts_normal_forward_slash_paths() {
        for path in ["file.yml", "folder/file.yml", "name with spaces.txt"] {
            assert!(
                GitRelativePath::parse(path).is_ok(),
                "expected {path:?} to be valid"
            );
        }
    }

    #[test]
    fn git_relative_path_rejects_unsafe_or_non_canonical_forms() {
        for path in [
            "",
            "\0",
            "/absolute",
            ".",
            "..",
            "./file",
            "folder/../file",
            "folder//file",
            "folder/",
            "folder\\file",
            "folder\\mixed/file",
            "C:/file",
            "C:file",
            "C:\\file",
            "\\\\server\\share\\file",
            ".git",
            ".GIT/config",
        ] {
            assert!(
                GitRelativePath::parse(path).is_err(),
                "expected {path:?} to be rejected"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn git_relative_path_rejects_windows_ads_components() {
        for path in [
            "file.txt:stream",
            "folder:name/file.txt",
            "folder/file.txt:stream",
        ] {
            assert!(
                GitRelativePath::parse(path).is_err(),
                "expected Windows ADS path {path:?} to be rejected"
            );
        }
    }
}
