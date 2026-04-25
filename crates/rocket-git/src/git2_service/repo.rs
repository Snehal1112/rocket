use std::path::Path;

use git2::Repository;
use rocket_shared::error::{DomainError, DomainResult};

use crate::credentials::GitCredentials;

use super::helpers::build_callbacks;

#[tracing::instrument(name = "git_is_repo", fields(path = %path))]
pub(super) fn is_repo(path: &str) -> bool {
    Repository::open(path).is_ok()
}

#[tracing::instrument(name = "git_init", fields(repo_path = %path))]
pub(super) fn init(path: &str) -> DomainResult<()> {
    Repository::init(path).map_err(|e| DomainError::Internal(e.to_string()))?;
    Ok(())
}

#[tracing::instrument(name = "git_clone_repo", skip(creds), fields(url = %url, target_path = %dest_path))]
pub(super) fn clone_repo(
    url: &str,
    dest_path: &str,
    creds: &GitCredentials,
) -> DomainResult<()> {
    let dest = Path::new(dest_path);
    if dest.is_dir() && std::fs::read_dir(dest).map_or(false, |mut d| d.next().is_some()) {
        return Err(DomainError::InvalidInput(format!(
            "Destination '{}' already exists and is not empty. Please choose an empty directory or a new path.",
            dest_path
        )));
    }

    let callbacks = build_callbacks(creds);
    let mut fetch_opts = git2::FetchOptions::new();
    fetch_opts.remote_callbacks(callbacks);

    git2::build::RepoBuilder::new()
        .fetch_options(fetch_opts)
        .clone(url, dest)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    Ok(())
}
