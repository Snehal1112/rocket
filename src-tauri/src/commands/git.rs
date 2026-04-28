use rocket_app::GitAppService;
use rocket_git::{
    BranchList, CommitInfo, ConflictFile, ConflictResolution,
    FetchResult, FileDiff, GitCredentials, RemoteInfo, RepoStatus, StashEntry,
};
use rocket_shared::error::DomainError;
use serde::{Deserialize, Serialize};
use tauri::State;

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
const KEYRING_ACCOUNT: &str = "git-credentials";

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

/// Return the absolute path of the first default SSH private key found in
/// `~/.ssh/`, checking id_ed25519 → id_rsa → id_ecdsa → id_dsa in order.
/// Returns None if the home directory cannot be determined or no key exists.
#[tauri::command]
pub fn get_default_ssh_key_path() -> Option<String> {
    let home = dirs::home_dir()?;
    let ssh_dir = home.join(".ssh");
    for name in &["id_ed25519", "id_rsa", "id_ecdsa", "id_dsa"] {
        let path = ssh_dir.join(name);
        if path.exists() {
            return path.to_str().map(str::to_owned);
        }
    }
    None
}

/// Persist git credentials to the OS keychain (macOS Keychain, Windows
/// Credential Manager, Linux Secret Service). The passphrase, if present,
/// is stored inside the encrypted keychain entry — never written to disk.
#[tauri::command]
pub fn save_git_credentials(creds: GitCredentialsPayload) -> Result<(), String> {
    let json = serde_json::to_string(&creds).map_err(|e| e.to_string())?;
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| e.to_string())?;
    entry.set_password(&json).map_err(|e| e.to_string())
}

/// Load previously saved git credentials from the OS keychain.
/// Returns None if no entry exists yet (first run). Errors if the keychain
/// is unavailable (e.g. locked) — callers should treat this as no-credentials.
#[tauri::command]
pub fn load_git_credentials() -> Result<Option<GitCredentialsPayload>, String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(json) => {
            let creds: GitCredentialsPayload =
                serde_json::from_str(&json).map_err(|e| e.to_string())?;
            Ok(Some(creds))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
