use rocket_app::GitAppService;
use rocket_git::{
    BranchList, CommitInfo, ConflictFile, ConflictResolution,
    FetchResult, FileDiff, GitCredentials, RemoteInfo, RepoStatus, StashEntry,
};
use rocket_shared::error::DomainError;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitIdentity {
    pub name: String,
    pub email: String,
}

#[tauri::command]
pub fn git_is_repo(collection_path: String, svc: State<'_, GitAppService>) -> Result<bool, DomainError> {
    Ok(svc.is_repo(&collection_path))
}

#[tauri::command]
pub fn git_init(collection_path: String, svc: State<'_, GitAppService>) -> Result<(), DomainError> {
    svc.init(&collection_path)
}

#[tauri::command]
pub fn git_clone(url: String, dest_path: String, creds: GitCredentials, svc: State<'_, GitAppService>) -> Result<(), DomainError> {
    svc.clone_repo(&url, &dest_path, &creds)
}

#[tauri::command]
pub fn git_status(collection_path: String, svc: State<'_, GitAppService>) -> Result<RepoStatus, DomainError> {
    svc.status(&collection_path)
}

#[tauri::command]
pub fn git_diff(collection_path: String, file: String, svc: State<'_, GitAppService>) -> Result<FileDiff, DomainError> {
    svc.diff_file(&collection_path, &file)
}

#[tauri::command]
pub fn git_diff_staged(collection_path: String, file: String, svc: State<'_, GitAppService>) -> Result<FileDiff, DomainError> {
    svc.diff_staged(&collection_path, &file)
}

#[tauri::command]
pub fn git_diff_commit(collection_path: String, oid: String, svc: State<'_, GitAppService>) -> Result<Vec<FileDiff>, DomainError> {
    svc.diff_commit(&collection_path, &oid)
}

#[tauri::command]
pub fn git_stage(collection_path: String, files: Vec<String>, svc: State<'_, GitAppService>) -> Result<(), DomainError> {
    let refs: Vec<&str> = files.iter().map(|s| s.as_str()).collect();
    svc.stage(&collection_path, &refs)
}

#[tauri::command]
pub fn git_unstage(collection_path: String, files: Vec<String>, svc: State<'_, GitAppService>) -> Result<(), DomainError> {
    let refs: Vec<&str> = files.iter().map(|s| s.as_str()).collect();
    svc.unstage(&collection_path, &refs)
}

#[tauri::command]
pub fn git_discard(collection_path: String, files: Vec<String>, svc: State<'_, GitAppService>) -> Result<(), DomainError> {
    let refs: Vec<&str> = files.iter().map(|s| s.as_str()).collect();
    svc.discard(&collection_path, &refs)
}

#[tauri::command]
pub fn git_commit(collection_path: String, message: String, svc: State<'_, GitAppService>) -> Result<CommitInfo, DomainError> {
    svc.commit(&collection_path, &message)
}

#[tauri::command]
pub fn git_log(collection_path: String, limit: usize, svc: State<'_, GitAppService>) -> Result<Vec<CommitInfo>, DomainError> {
    svc.log(&collection_path, limit)
}

#[tauri::command]
pub fn git_push(collection_path: String, remote: String, creds: GitCredentials, svc: State<'_, GitAppService>) -> Result<(), DomainError> {
    svc.push(&collection_path, &remote, &creds)
}

#[tauri::command]
pub fn git_pull(collection_path: String, remote: String, creds: GitCredentials, svc: State<'_, GitAppService>) -> Result<(), DomainError> {
    svc.pull(&collection_path, &remote, &creds)
}

#[tauri::command]
pub fn git_fetch(collection_path: String, remote: String, creds: GitCredentials, svc: State<'_, GitAppService>) -> Result<FetchResult, DomainError> {
    svc.fetch(&collection_path, &remote, &creds)
}

#[tauri::command]
pub fn git_branches(collection_path: String, svc: State<'_, GitAppService>) -> Result<BranchList, DomainError> {
    svc.branches(&collection_path)
}

#[tauri::command]
pub fn git_switch_branch(collection_path: String, name: String, svc: State<'_, GitAppService>) -> Result<(), DomainError> {
    svc.switch_branch(&collection_path, &name)
}

#[tauri::command]
pub fn git_checkout_remote_branch(collection_path: String, name: String, svc: State<'_, GitAppService>) -> Result<(), DomainError> {
    svc.checkout_remote_branch(&collection_path, &name)
}

#[tauri::command]
pub fn git_create_branch(collection_path: String, name: String, svc: State<'_, GitAppService>) -> Result<(), DomainError> {
    svc.create_branch(&collection_path, &name)
}

#[tauri::command]
pub fn git_delete_branch(collection_path: String, name: String, svc: State<'_, GitAppService>) -> Result<(), DomainError> {
    svc.delete_branch(&collection_path, &name)
}

#[tauri::command]
pub fn git_merge_branch(collection_path: String, name: String, svc: State<'_, GitAppService>) -> Result<(), DomainError> {
    svc.merge_branch(&collection_path, &name)
}

#[tauri::command]
pub fn git_stash_list(collection_path: String, svc: State<'_, GitAppService>) -> Result<Vec<StashEntry>, DomainError> {
    svc.stash_list(&collection_path)
}

#[tauri::command]
pub fn git_stash_save(collection_path: String, message: String, svc: State<'_, GitAppService>) -> Result<(), DomainError> {
    svc.stash_save(&collection_path, &message)
}

#[tauri::command]
pub fn git_stash_pop(collection_path: String, index: usize, svc: State<'_, GitAppService>) -> Result<(), DomainError> {
    svc.stash_pop(&collection_path, index)
}

#[tauri::command]
pub fn git_stash_apply(collection_path: String, index: usize, svc: State<'_, GitAppService>) -> Result<(), DomainError> {
    svc.stash_apply(&collection_path, index)
}

#[tauri::command]
pub fn git_stash_drop(collection_path: String, index: usize, svc: State<'_, GitAppService>) -> Result<(), DomainError> {
    svc.stash_drop(&collection_path, index)
}

#[tauri::command]
pub fn git_conflicts(collection_path: String, svc: State<'_, GitAppService>) -> Result<Vec<ConflictFile>, DomainError> {
    svc.conflicts(&collection_path)
}

#[tauri::command]
pub fn git_resolve_conflict(collection_path: String, file: String, resolution: ConflictResolution, svc: State<'_, GitAppService>) -> Result<(), DomainError> {
    svc.resolve_conflict(&collection_path, &file, &resolution)
}

#[tauri::command]
pub fn git_abort_merge(collection_path: String, svc: State<'_, GitAppService>) -> Result<(), DomainError> {
    svc.abort_merge(&collection_path)
}

#[tauri::command]
pub fn git_list_remotes(collection_path: String, svc: State<'_, GitAppService>) -> Result<Vec<RemoteInfo>, DomainError> {
    svc.list_remotes(&collection_path)
}

#[tauri::command]
pub fn git_add_remote(collection_path: String, name: String, url: String, svc: State<'_, GitAppService>) -> Result<(), DomainError> {
    svc.add_remote(&collection_path, &name, &url)
}

#[tauri::command]
pub fn git_remove_remote(collection_path: String, name: String, svc: State<'_, GitAppService>) -> Result<(), DomainError> {
    svc.remove_remote(&collection_path, &name)
}

#[tauri::command]
pub fn git_set_remote_url(collection_path: String, name: String, url: String, svc: State<'_, GitAppService>) -> Result<(), DomainError> {
    svc.set_remote_url(&collection_path, &name, &url)
}

const KEYRING_SERVICE: &str = "rocket-api";

fn keyring_account(workspace_id: &str) -> String {
    format!("git-credentials-{}", workspace_id)
}

/// Serialisable mirror of GitCredentials — used only for keychain persistence.
/// Kept separate from the domain type so the wire format is stable even if
/// the domain enum changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GitCredentialsPayload {
    #[serde(rename_all = "camelCase")]
    SshKey {
        private_key_path: String,
        passphrase: Option<String>,
    },
    SshAgent,
    #[serde(rename_all = "camelCase")]
    UserPass {
        username: String,
        password: String,
    },
    Token {
        token: String,
    },
}

impl From<GitCredentials> for GitCredentialsPayload {
    fn from(c: GitCredentials) -> Self {
        match c {
            GitCredentials::SshKey { private_key_path, passphrase } =>
                GitCredentialsPayload::SshKey { private_key_path, passphrase },
            GitCredentials::SshAgent => GitCredentialsPayload::SshAgent,
            GitCredentials::UserPass { username, password } =>
                GitCredentialsPayload::UserPass { username, password },
            GitCredentials::Token { token } => GitCredentialsPayload::Token { token },
        }
    }
}

impl From<GitCredentialsPayload> for GitCredentials {
    fn from(p: GitCredentialsPayload) -> Self {
        match p {
            GitCredentialsPayload::SshKey { private_key_path, passphrase } =>
                GitCredentials::SshKey { private_key_path, passphrase },
            GitCredentialsPayload::SshAgent => GitCredentials::SshAgent,
            GitCredentialsPayload::UserPass { username, password } =>
                GitCredentials::UserPass { username, password },
            GitCredentialsPayload::Token { token } => GitCredentials::Token { token },
        }
    }
}

/// Return the absolute path of the first default SSH private key found in
/// `~/.ssh/`, checking id_ed25519 → id_rsa → id_ecdsa → id_dsa in order.
/// Returns None if the home directory cannot be determined or no key exists.
#[tauri::command]
pub fn get_default_ssh_key_path() -> Option<String> {
    let home = dirs::home_dir()?;
    let ssh_dir = home.join(".ssh");
    for name in ["id_ed25519", "id_rsa", "id_ecdsa", "id_dsa"] {
        let path = ssh_dir.join(name);
        if path.exists() {
            return path.to_str().map(str::to_owned);
        }
    }
    None
}

/// Return all SSH private key paths found in `~/.ssh/` — both standard-named
/// keys (id_ed25519, id_rsa, …) and custom-named keys (id_ed25519_snehal1112,
/// etc.). A file is treated as a private key if a corresponding `.pub` file
/// exists alongside it. Standard names are returned first; the rest are sorted
/// alphabetically.
#[tauri::command]
pub fn list_ssh_key_paths() -> Vec<String> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let ssh_dir = home.join(".ssh");
    let Ok(entries) = std::fs::read_dir(&ssh_dir) else {
        return Vec::new();
    };

    const STANDARD: &[&str] = &["id_ed25519", "id_rsa", "id_ecdsa", "id_dsa"];
    // Files in ~/.ssh/ that are definitely not private keys.
    const SKIP: &[&str] = &[
        "known_hosts", "known_hosts.old", "authorized_keys",
        "config", "environment", "rc",
    ];

    let mut standard: Vec<String> = Vec::new();
    let mut custom: Vec<String> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() { continue; }

        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if name.ends_with(".pub") || name.starts_with('.') { continue; }
        if SKIP.contains(&name.as_str()) { continue; }

        // Require a paired .pub file — strong signal this is a key pair.
        if !ssh_dir.join(format!("{}.pub", name)).exists() { continue; }

        let Some(path_str) = path.to_str().map(String::from) else { continue };

        if STANDARD.contains(&name.as_str()) {
            standard.push(path_str);
        } else {
            custom.push(path_str);
        }
    }

    // Standard names in the preferred order; custom names alphabetically after.
    standard.sort_by_key(|p| {
        let name = std::path::Path::new(p).file_name()
            .and_then(|n| n.to_str()).unwrap_or("");
        STANDARD.iter().position(|&s| s == name).unwrap_or(usize::MAX)
    });
    custom.sort();
    standard.extend(custom);
    standard
}

/// Persist git credentials to the OS keychain (macOS Keychain, Windows
/// Credential Manager, Linux Secret Service). The passphrase, if present,
/// is stored inside the encrypted keychain entry — never written to disk.
#[tauri::command]
pub fn save_git_credentials(workspace_id: String, creds: GitCredentialsPayload) -> Result<(), DomainError> {
    let json = serde_json::to_string(&creds)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let entry = keyring::Entry::new(KEYRING_SERVICE, &keyring_account(&workspace_id))
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    entry.set_password(&json).map_err(|e| DomainError::Internal(e.to_string()))
}

/// Load previously saved git credentials from the OS keychain.
/// Returns None if no entry exists yet (first run). Errors if the keychain
/// is unavailable (e.g. locked) — callers should treat this as no-credentials.
#[tauri::command]
pub fn load_git_credentials(workspace_id: String) -> Result<Option<GitCredentialsPayload>, DomainError> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, &keyring_account(&workspace_id))
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    match entry.get_password() {
        Ok(json) => {
            let creds: GitCredentialsPayload =
                serde_json::from_str(&json).map_err(|e| DomainError::Internal(e.to_string()))?;
            Ok(Some(creds))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(DomainError::Internal(e.to_string())),
    }
}

/// Read user.name and user.email from the repo's git config (local → global → system).
/// Returns empty strings when the values are unset — never errors on a missing entry.
#[tauri::command]
pub fn git_get_identity(path: String) -> Result<GitIdentity, DomainError> {
    let repo = git2::Repository::open(&path)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let cfg = repo.config()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let name = cfg.get_string("user.name").unwrap_or_default();
    let email = cfg.get_string("user.email").unwrap_or_default();
    Ok(GitIdentity { name, email })
}

/// Write user.name and user.email to the repo-local .git/config.
#[tauri::command]
pub fn git_set_identity(path: String, name: String, email: String) -> Result<(), DomainError> {
    let repo = git2::Repository::open(&path)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let cfg = repo.config()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    // open_level gives us the repo-local config only, so we never write to ~/.gitconfig.
    let mut local = cfg.open_level(git2::ConfigLevel::Local)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    local.set_str("user.name", &name)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    local.set_str("user.email", &email)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn init_repo(dir: &TempDir) -> String {
        let path = dir.path().to_string_lossy().to_string();
        git2::Repository::init(&path).unwrap();
        path
    }

    #[test]
    fn get_identity_returns_empty_when_unset() {
        let dir = TempDir::new().unwrap();
        let path = init_repo(&dir);
        let identity = git_get_identity(path).unwrap();
        assert_eq!(identity.name, "");
        assert_eq!(identity.email, "");
    }

    #[test]
    fn set_and_get_identity_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = init_repo(&dir);
        git_set_identity(path.clone(), "Bob".into(), "bob@example.com".into()).unwrap();
        let identity = git_get_identity(path).unwrap();
        assert_eq!(identity.name, "Bob");
        assert_eq!(identity.email, "bob@example.com");
    }

    #[test]
    fn keyring_account_includes_workspace_id() {
        assert_eq!(keyring_account("ws-abc"), "git-credentials-ws-abc");
        assert_eq!(keyring_account("default"), "git-credentials-default");
    }

    #[test]
    fn set_identity_writes_to_local_config_only() {
        let dir = TempDir::new().unwrap();
        let path = init_repo(&dir);
        git_set_identity(path.clone(), "Local".into(), "local@test.com".into()).unwrap();
        // Read config at local level only to confirm it's there.
        let repo = git2::Repository::open(&path).unwrap();
        let cfg = repo.config().unwrap();
        let local = cfg.open_level(git2::ConfigLevel::Local).unwrap();
        assert_eq!(local.get_string("user.name").unwrap(), "Local");
        assert_eq!(local.get_string("user.email").unwrap(), "local@test.com");
    }
}
