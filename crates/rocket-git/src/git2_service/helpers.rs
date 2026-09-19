use std::fs;
use std::path::{Component, Path, PathBuf};

use git2::{Repository, Status};
use rocket_shared::error::{DomainError, DomainResult};

use crate::credentials::GitCredentials;
use crate::diff::{DiffHunk, DiffLine, LineType};
use crate::status::GitStatus;

/// Build credential callbacks for remote operations.
///
/// The callback includes a one-shot guard: if libgit2 calls it more than once
/// (which happens when credentials are rejected and it retries), we return an
/// error on the second call so the operation fails fast instead of looping.
pub(super) fn build_callbacks(creds: &GitCredentials) -> git2::RemoteCallbacks<'_> {
    let mut callbacks = git2::RemoteCallbacks::new();
    // Accept any SSH host key — Windows often has an empty known_hosts so
    // libgit2 raises GIT_ECERTIFICATE (-17) without this. Authentication
    // is still enforced via the SSH private key credential below.
    callbacks.certificate_check(|_cert, _host| Ok(git2::CertificateCheckStatus::CertificateOk));
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
    callbacks
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

#[cfg(test)]
mod tests {
    use super::GitRelativePath;

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
