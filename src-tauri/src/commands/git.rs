use rocket_app::GitAppService;
use rocket_git::{
    BranchList, CommitInfo, ConflictFile, ConflictResolution,
    FileDiff, GitCredentials, RepoStatus, StashEntry,
};
use rocket_shared::error::DomainError;
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
pub fn git_fetch(collection_path: String, remote: String, creds: GitCredentials, svc: State<'_, GitAppService>) -> Result<(), DomainError> {
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
