use rocket_git::service::GitService;
use rocket_git::{
    BranchList, CommitInfo, ConflictFile, ConflictResolution,
    FileDiff, GitCredentials, RemoteInfo, RepoStatus, StashEntry,
};
use rocket_shared::error::DomainResult;
use rocket_shared::events::{DomainEvent, EventPublisher};

pub struct GitAppService {
    git: Box<dyn GitService>,
    events: Box<dyn EventPublisher>,
}

impl GitAppService {
    pub fn new(git: Box<dyn GitService>, events: Box<dyn EventPublisher>) -> Self {
        Self { git, events }
    }

    // Repository
    pub fn is_repo(&self, path: &str) -> bool {
        self.git.is_repo(path)
    }

    pub fn init(&self, path: &str) -> DomainResult<()> {
        self.git.init(path)
    }

    pub fn clone_repo(&self, url: &str, dest_path: &str, creds: &GitCredentials) -> DomainResult<()> {
        self.git.clone_repo(url, dest_path, creds)?;
        self.events.publish(DomainEvent::GitCloned {
            url: url.to_string(),
            dest: dest_path.to_string(),
        });
        Ok(())
    }

    // Remotes
    pub fn list_remotes(&self, path: &str) -> DomainResult<Vec<RemoteInfo>> {
        self.git.list_remotes(path)
    }

    pub fn add_remote(&self, path: &str, name: &str, url: &str) -> DomainResult<()> {
        self.git.add_remote(path, name, url)?;
        self.events.publish(DomainEvent::GitRemoteAdded {
            collection: path.to_string(),
            name: name.to_string(),
            url: url.to_string(),
        });
        Ok(())
    }

    pub fn remove_remote(&self, path: &str, name: &str) -> DomainResult<()> {
        self.git.remove_remote(path, name)?;
        self.events.publish(DomainEvent::GitRemoteRemoved {
            collection: path.to_string(),
            name: name.to_string(),
        });
        Ok(())
    }

    pub fn set_remote_url(&self, path: &str, name: &str, url: &str) -> DomainResult<()> {
        self.git.set_remote_url(path, name, url)
    }

    // Status + diff
    pub fn status(&self, path: &str) -> DomainResult<RepoStatus> {
        self.git.status(path)
    }

    pub fn diff_file(&self, path: &str, file: &str) -> DomainResult<FileDiff> {
        self.git.diff_file(path, file)
    }

    pub fn diff_staged(&self, path: &str, file: &str) -> DomainResult<FileDiff> {
        self.git.diff_staged(path, file)
    }

    // Staging
    pub fn stage(&self, path: &str, files: &[&str]) -> DomainResult<()> {
        self.git.stage(path, files)?;
        self.events.publish(DomainEvent::GitStatusChanged {
            collection: path.to_string(),
        });
        Ok(())
    }

    pub fn unstage(&self, path: &str, files: &[&str]) -> DomainResult<()> {
        self.git.unstage(path, files)?;
        self.events.publish(DomainEvent::GitStatusChanged {
            collection: path.to_string(),
        });
        Ok(())
    }

    pub fn discard(&self, path: &str, files: &[&str]) -> DomainResult<()> {
        self.git.discard(path, files)?;
        self.events.publish(DomainEvent::GitStatusChanged {
            collection: path.to_string(),
        });
        Ok(())
    }

    // Commit
    pub fn commit(&self, path: &str, message: &str) -> DomainResult<CommitInfo> {
        let info = self.git.commit(path, message)?;
        self.events.publish(DomainEvent::GitCommit {
            collection: path.to_string(),
            message: message.to_string(),
            sha: info.id.clone(),
        });
        Ok(info)
    }

    pub fn log(&self, path: &str, limit: usize) -> DomainResult<Vec<CommitInfo>> {
        self.git.log(path, limit)
    }

    // Remote
    pub fn push(&self, path: &str, remote: &str, creds: &GitCredentials) -> DomainResult<()> {
        self.git.push(path, remote, creds)?;
        self.events.publish(DomainEvent::GitPush {
            collection: path.to_string(),
            remote: remote.to_string(),
        });
        Ok(())
    }

    pub fn pull(&self, path: &str, remote: &str, creds: &GitCredentials) -> DomainResult<()> {
        self.git.pull(path, remote, creds)?;
        self.events.publish(DomainEvent::GitPull {
            collection: path.to_string(),
            remote: remote.to_string(),
        });
        Ok(())
    }

    pub fn fetch(&self, path: &str, remote: &str, creds: &GitCredentials) -> DomainResult<()> {
        self.git.fetch(path, remote, creds)
    }

    // Branches
    pub fn branches(&self, path: &str) -> DomainResult<BranchList> {
        self.git.branches(path)
    }

    pub fn switch_branch(&self, path: &str, name: &str) -> DomainResult<()> {
        self.git.switch_branch(path, name)?;
        self.events.publish(DomainEvent::BranchSwitched {
            collection: path.to_string(),
            branch: name.to_string(),
        });
        Ok(())
    }

    pub fn checkout_remote_branch(&self, path: &str, name: &str) -> DomainResult<()> {
        self.git.checkout_remote_branch(path, name)?;
        self.events.publish(DomainEvent::BranchSwitched {
            collection: path.to_string(),
            branch: name.to_string(),
        });
        Ok(())
    }

    pub fn create_branch(&self, path: &str, name: &str) -> DomainResult<()> {
        self.git.create_branch(path, name)?;
        // Publish a switch event so the frontend updates its branch indicator.
        self.events.publish(DomainEvent::BranchSwitched {
            collection: path.to_string(),
            branch: name.to_string(),
        });
        Ok(())
    }

    pub fn delete_branch(&self, path: &str, name: &str) -> DomainResult<()> {
        self.git.delete_branch(path, name)
    }

    pub fn merge_branch(&self, path: &str, name: &str) -> DomainResult<()> {
        self.git.merge_branch(path, name)?;
        self.events.publish(DomainEvent::BranchMerged {
            collection: path.to_string(),
            branch: name.to_string(),
        });
        Ok(())
    }

    // Stash
    pub fn stash_list(&self, path: &str) -> DomainResult<Vec<StashEntry>> {
        self.git.stash_list(path)
    }

    pub fn stash_save(&self, path: &str, message: &str) -> DomainResult<()> {
        self.git.stash_save(path, message)?;
        self.events.publish(DomainEvent::GitStashChanged {
            collection: path.to_string(),
        });
        Ok(())
    }

    pub fn stash_pop(&self, path: &str, index: usize) -> DomainResult<()> {
        self.git.stash_pop(path, index)?;
        self.events.publish(DomainEvent::GitStashChanged {
            collection: path.to_string(),
        });
        Ok(())
    }

    pub fn stash_apply(&self, path: &str, index: usize) -> DomainResult<()> {
        self.git.stash_apply(path, index)?;
        self.events.publish(DomainEvent::GitStashChanged {
            collection: path.to_string(),
        });
        Ok(())
    }

    pub fn stash_drop(&self, path: &str, index: usize) -> DomainResult<()> {
        self.git.stash_drop(path, index)?;
        self.events.publish(DomainEvent::GitStashChanged {
            collection: path.to_string(),
        });
        Ok(())
    }

    // Conflicts
    pub fn conflicts(&self, path: &str) -> DomainResult<Vec<ConflictFile>> {
        let files = self.git.conflicts(path)?;
        if !files.is_empty() {
            self.events.publish(DomainEvent::GitConflictDetected {
                collection: path.to_string(),
                files: files.iter().map(|f| f.path.clone()).collect(),
            });
        }
        Ok(files)
    }

    pub fn resolve_conflict(&self, path: &str, file: &str, resolution: &ConflictResolution) -> DomainResult<()> {
        self.git.resolve_conflict(path, file, resolution)
    }

    pub fn abort_merge(&self, path: &str) -> DomainResult<()> {
        self.git.abort_merge(path)?;
        self.events.publish(DomainEvent::GitStatusChanged {
            collection: path.to_string(),
        });
        Ok(())
    }
}
