use rocket_git::service::GitService;
use rocket_git::{
    BranchList, CommitInfo, ConflictFile, ConflictResolution, FetchResult, FileDiff,
    GitCredentials, RemoteInfo, RepoStatus, StashEntry,
};
use rocket_shared::error::DomainResult;
use rocket_shared::events::{DomainEvent, EventPublisher};

/// Removes the user and password from a URL so it is safe to publish.
/// Input without `scheme://` userinfo, such as scp-style `git@host:repo`, is returned as is.
fn strip_url_credentials(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else {
        return url.to_string();
    };
    let start = scheme_end + 3;
    let rest = &url[start..];
    let authority_len = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    match rest[..authority_len].rfind('@') {
        Some(at) => format!("{}{}", &url[..start], &rest[at + 1..]),
        None => url.to_string(),
    }
}

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

    pub fn clone_repo(
        &self,
        url: &str,
        dest_path: &str,
        creds: &GitCredentials,
    ) -> DomainResult<()> {
        self.git.clone_repo(url, dest_path, creds)?;
        self.events.publish(DomainEvent::GitCloned {
            url: strip_url_credentials(url),
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
            url: strip_url_credentials(url),
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

    pub fn diff_commit(&self, path: &str, oid: &str) -> DomainResult<Vec<FileDiff>> {
        self.git.diff_commit(path, oid)
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
    /// Push the current branch. `force` opts into force-with-lease semantics;
    /// see `GitService::push`.
    pub fn push(
        &self,
        path: &str,
        remote: &str,
        creds: &GitCredentials,
        force: bool,
    ) -> DomainResult<()> {
        self.git.push(path, remote, creds, force)?;
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

    pub fn fetch(
        &self,
        path: &str,
        remote: &str,
        creds: &GitCredentials,
    ) -> DomainResult<FetchResult> {
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

    pub fn checkout_remote_branch(
        &self,
        path: &str,
        name: &str,
        force: bool,
        as_name: Option<&str>,
    ) -> DomainResult<()> {
        self.git
            .checkout_remote_branch(path, name, force, as_name)?;
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

    pub fn stash_diff(&self, path: &str, index: usize) -> DomainResult<Vec<FileDiff>> {
        self.git.stash_diff(path, index)
    }

    // Conflicts
    pub fn conflicts(&self, path: &str) -> DomainResult<Vec<ConflictFile>> {
        self.git.conflicts(path)
    }

    pub fn resolve_conflict(
        &self,
        path: &str,
        file: &str,
        resolution: &ConflictResolution,
    ) -> DomainResult<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_doubles::{RecordingPublisher, SharedPublisher};
    use rocket_git::GitCredentials;
    use rocket_shared::error::DomainError;
    use std::sync::{Arc, Mutex};

    /// Fake git service. It records the URLs it receives. Calls the tests need
    /// succeed, or fail when `fail` is set. The other calls panic.
    struct FakeGit {
        urls: Arc<Mutex<Vec<String>>>,
        fail: bool,
        conflict_paths: Vec<String>,
    }

    impl FakeGit {
        fn outcome(&self) -> DomainResult<()> {
            if self.fail {
                Err(DomainError::Internal("git failed".into()))
            } else {
                Ok(())
            }
        }
    }

    impl GitService for FakeGit {
        fn is_repo(&self, _: &str) -> bool {
            unimplemented!()
        }
        fn init(&self, _: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn clone_repo(&self, url: &str, _: &str, _: &GitCredentials) -> DomainResult<()> {
            self.urls.lock().expect("lock").push(url.to_string());
            self.outcome()
        }
        fn list_remotes(&self, _: &str) -> DomainResult<Vec<RemoteInfo>> {
            unimplemented!()
        }
        fn add_remote(&self, _: &str, _: &str, url: &str) -> DomainResult<()> {
            self.urls.lock().expect("lock").push(url.to_string());
            Ok(())
        }
        fn remove_remote(&self, _: &str, _: &str) -> DomainResult<()> {
            self.outcome()
        }
        fn set_remote_url(&self, _: &str, _: &str, _: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn status(&self, _: &str) -> DomainResult<RepoStatus> {
            unimplemented!()
        }
        fn diff_file(&self, _: &str, _: &str) -> DomainResult<FileDiff> {
            unimplemented!()
        }
        fn diff_staged(&self, _: &str, _: &str) -> DomainResult<FileDiff> {
            unimplemented!()
        }
        fn diff_commit(&self, _: &str, _: &str) -> DomainResult<Vec<FileDiff>> {
            unimplemented!()
        }
        fn stage(&self, _: &str, _: &[&str]) -> DomainResult<()> {
            self.outcome()
        }
        fn unstage(&self, _: &str, _: &[&str]) -> DomainResult<()> {
            self.outcome()
        }
        fn discard(&self, _: &str, _: &[&str]) -> DomainResult<()> {
            self.outcome()
        }
        fn commit(&self, _: &str, message: &str) -> DomainResult<CommitInfo> {
            self.outcome()?;
            Ok(CommitInfo {
                id: "abc1234".into(),
                full_id: "abc1234def".into(),
                message: message.into(),
                author: "Tester".into(),
                author_email: "t@example.com".into(),
                timestamp: chrono::Utc::now(),
                files_changed: 1,
                masked_files: Vec::new(),
            })
        }
        fn log(&self, _: &str, _: usize) -> DomainResult<Vec<CommitInfo>> {
            unimplemented!()
        }
        fn push(&self, _: &str, _: &str, _: &GitCredentials, _: bool) -> DomainResult<()> {
            self.outcome()
        }
        fn pull(&self, _: &str, _: &str, _: &GitCredentials) -> DomainResult<()> {
            self.outcome()
        }
        fn fetch(&self, _: &str, _: &str, _: &GitCredentials) -> DomainResult<FetchResult> {
            unimplemented!()
        }
        fn branches(&self, _: &str) -> DomainResult<BranchList> {
            unimplemented!()
        }
        fn switch_branch(&self, _: &str, _: &str) -> DomainResult<()> {
            self.outcome()
        }
        fn checkout_remote_branch(
            &self,
            _: &str,
            _: &str,
            _: bool,
            _: Option<&str>,
        ) -> DomainResult<()> {
            self.outcome()
        }
        fn create_branch(&self, _: &str, _: &str) -> DomainResult<()> {
            self.outcome()
        }
        fn delete_branch(&self, _: &str, _: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn merge_branch(&self, _: &str, _: &str) -> DomainResult<()> {
            self.outcome()
        }
        fn stash_list(&self, _: &str) -> DomainResult<Vec<StashEntry>> {
            unimplemented!()
        }
        fn stash_save(&self, _: &str, _: &str) -> DomainResult<()> {
            self.outcome()
        }
        fn stash_pop(&self, _: &str, _: usize) -> DomainResult<()> {
            self.outcome()
        }
        fn stash_apply(&self, _: &str, _: usize) -> DomainResult<()> {
            self.outcome()
        }
        fn stash_drop(&self, _: &str, _: usize) -> DomainResult<()> {
            self.outcome()
        }
        fn stash_diff(&self, _: &str, _: usize) -> DomainResult<Vec<FileDiff>> {
            unimplemented!()
        }
        fn conflicts(&self, _: &str) -> DomainResult<Vec<ConflictFile>> {
            self.outcome()?;
            Ok(self
                .conflict_paths
                .iter()
                .map(|path| ConflictFile {
                    path: path.clone(),
                    ours: String::new(),
                    theirs: String::new(),
                    ancestor: None,
                })
                .collect())
        }
        fn resolve_conflict(&self, _: &str, _: &str, _: &ConflictResolution) -> DomainResult<()> {
            unimplemented!()
        }
        fn abort_merge(&self, _: &str) -> DomainResult<()> {
            self.outcome()
        }
    }

    fn build(
        fail: bool,
        conflict_paths: Vec<String>,
    ) -> (
        GitAppService,
        Arc<RecordingPublisher>,
        Arc<Mutex<Vec<String>>>,
    ) {
        let urls = Arc::new(Mutex::new(Vec::new()));
        let events = RecordingPublisher::new();
        let svc = GitAppService::new(
            Box::new(FakeGit {
                urls: urls.clone(),
                fail,
                conflict_paths,
            }),
            Box::new(SharedPublisher(events.clone())),
        );
        (svc, events, urls)
    }

    fn service() -> (
        GitAppService,
        Arc<RecordingPublisher>,
        Arc<Mutex<Vec<String>>>,
    ) {
        build(false, Vec::new())
    }

    fn failing_service() -> (GitAppService, Arc<RecordingPublisher>) {
        let (svc, events, _) = build(true, Vec::new());
        (svc, events)
    }

    #[test]
    fn strip_url_credentials_removes_userinfo() {
        let cases = [
            ("https://u:p@host/x", "https://host/x"),
            ("https://token@host/x", "https://host/x"),
            ("ssh://git@host/x", "ssh://host/x"),
            ("https://u:p@w@host:8443/x", "https://host:8443/x"),
            ("https://u:p@host", "https://host"),
            ("https://u:p@host?a=b", "https://host?a=b"),
            ("https://@host/x", "https://host/x"),
        ];
        for (input, want) in cases {
            assert_eq!(strip_url_credentials(input), want, "input {input}");
        }
    }

    #[test]
    fn strip_url_credentials_leaves_other_input_alone() {
        let cases = [
            "git@host:org/repo.git",
            "https://host/x",
            "https://host/a@b",
            "https://host/x?mail=a@b",
            "https://host/x#a@b",
            "/local/path/repo",
            "",
            "not a url @",
        ];
        for input in cases {
            assert_eq!(strip_url_credentials(input), input, "input {input}");
        }
    }

    #[test]
    fn clone_event_has_no_credentials_but_git_gets_original() {
        let (svc, events, urls) = service();
        let url = "https://user:token@host/org/repo.git";
        svc.clone_repo(url, "/dest", &GitCredentials::SshAgent)
            .expect("clone");
        assert_eq!(urls.lock().expect("lock").as_slice(), [url]);
        match events.events().as_slice() {
            [DomainEvent::GitCloned { url, dest }] => {
                assert_eq!(url, "https://host/org/repo.git");
                assert_eq!(dest, "/dest");
            }
            other => panic!("unexpected events: {other:?}"),
        }
    }

    #[test]
    fn add_remote_event_has_no_credentials_but_git_gets_original() {
        let (svc, events, urls) = service();
        let url = "https://user:token@host/org/repo.git";
        svc.add_remote("/repo", "origin", url).expect("add");
        assert_eq!(urls.lock().expect("lock").as_slice(), [url]);
        match events.events().as_slice() {
            [DomainEvent::GitRemoteAdded {
                collection,
                name,
                url,
            }] => {
                assert_eq!(collection, "/repo");
                assert_eq!(name, "origin");
                assert_eq!(url, "https://host/org/repo.git");
            }
            other => panic!("unexpected events: {other:?}"),
        }
    }

    #[test]
    fn commit_publishes_git_commit_with_sha() -> DomainResult<()> {
        let (svc, events, _) = service();
        let info = svc.commit("/repo", "add things")?;
        assert_eq!(info.id, "abc1234");
        match events.events().as_slice() {
            [DomainEvent::GitCommit {
                collection,
                message,
                sha,
            }] => {
                assert_eq!(collection, "/repo");
                assert_eq!(message, "add things");
                assert_eq!(sha, "abc1234");
            }
            other => panic!("unexpected events: {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn push_publishes_git_push() -> DomainResult<()> {
        let (svc, events, _) = service();
        svc.push("/repo", "origin", &GitCredentials::SshAgent, false)?;
        match events.events().as_slice() {
            [DomainEvent::GitPush { collection, remote }] => {
                assert_eq!(collection, "/repo");
                assert_eq!(remote, "origin");
            }
            other => panic!("unexpected events: {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn pull_publishes_git_pull() -> DomainResult<()> {
        let (svc, events, _) = service();
        svc.pull("/repo", "upstream", &GitCredentials::SshAgent)?;
        match events.events().as_slice() {
            [DomainEvent::GitPull { collection, remote }] => {
                assert_eq!(collection, "/repo");
                assert_eq!(remote, "upstream");
            }
            other => panic!("unexpected events: {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn switch_create_and_checkout_remote_publish_branch_switched() -> DomainResult<()> {
        let (svc, events, _) = service();
        svc.switch_branch("/repo", "main")?;
        svc.create_branch("/repo", "feature")?;
        svc.checkout_remote_branch("/repo", "origin/dev", false, None)?;
        let got: Vec<(String, String)> = events
            .events()
            .into_iter()
            .map(|e| match e {
                DomainEvent::BranchSwitched { collection, branch } => (collection, branch),
                other => panic!("unexpected event: {other:?}"),
            })
            .collect();
        let want = ["main", "feature", "origin/dev"].map(|b| ("/repo".to_string(), b.to_string()));
        assert_eq!(got, want);
        Ok(())
    }

    #[test]
    fn merge_publishes_branch_merged() -> DomainResult<()> {
        let (svc, events, _) = service();
        svc.merge_branch("/repo", "feature")?;
        match events.events().as_slice() {
            [DomainEvent::BranchMerged { collection, branch }] => {
                assert_eq!(collection, "/repo");
                assert_eq!(branch, "feature");
            }
            other => panic!("unexpected events: {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn every_stash_change_publishes_git_stash_changed() -> DomainResult<()> {
        let (svc, events, _) = service();
        svc.stash_save("/repo", "wip")?;
        svc.stash_pop("/repo", 0)?;
        svc.stash_apply("/repo", 0)?;
        svc.stash_drop("/repo", 0)?;
        let recorded = events.events();
        assert_eq!(recorded.len(), 4);
        for event in recorded {
            match event {
                DomainEvent::GitStashChanged { collection } => assert_eq!(collection, "/repo"),
                other => panic!("unexpected event: {other:?}"),
            }
        }
        Ok(())
    }

    #[test]
    fn stage_unstage_discard_and_abort_merge_publish_status_changed() -> DomainResult<()> {
        let (svc, events, _) = service();
        svc.stage("/repo", &["a.yml"])?;
        svc.unstage("/repo", &["a.yml"])?;
        svc.discard("/repo", &["a.yml"])?;
        svc.abort_merge("/repo")?;
        let recorded = events.events();
        assert_eq!(recorded.len(), 4);
        for event in recorded {
            match event {
                DomainEvent::GitStatusChanged { collection } => assert_eq!(collection, "/repo"),
                other => panic!("unexpected event: {other:?}"),
            }
        }
        Ok(())
    }

    #[test]
    fn remove_remote_publishes_git_remote_removed() -> DomainResult<()> {
        let (svc, events, _) = service();
        svc.remove_remote("/repo", "origin")?;
        match events.events().as_slice() {
            [DomainEvent::GitRemoteRemoved { collection, name }] => {
                assert_eq!(collection, "/repo");
                assert_eq!(name, "origin");
            }
            other => panic!("unexpected events: {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn conflicts_is_a_side_effect_free_query() -> DomainResult<()> {
        let (svc, events, _) = build(false, vec!["a.yml".into(), "b.yml".into()]);
        let files = svc.conflicts("/repo")?;
        assert_eq!(files.len(), 2);
        assert!(events.events().is_empty());
        Ok(())
    }

    #[test]
    fn conflicts_publishes_nothing_when_list_is_empty() -> DomainResult<()> {
        let (svc, events, _) = service();
        assert!(svc.conflicts("/repo")?.is_empty());
        assert!(events.events().is_empty());
        Ok(())
    }

    #[test]
    fn failed_commit_publishes_no_event() {
        let (svc, events) = failing_service();
        assert!(svc.commit("/repo", "msg").is_err());
        assert!(events.events().is_empty());
    }

    #[test]
    fn failed_push_publishes_no_event() {
        let (svc, events) = failing_service();
        assert!(svc
            .push("/repo", "origin", &GitCredentials::SshAgent, false)
            .is_err());
        assert!(events.events().is_empty());
    }

    #[test]
    fn failed_clone_publishes_no_event() {
        let (svc, events) = failing_service();
        assert!(svc
            .clone_repo("https://host/r.git", "/dest", &GitCredentials::SshAgent)
            .is_err());
        assert!(events.events().is_empty());
    }

    #[test]
    fn failed_pull_merge_and_stash_publish_no_event() {
        let (svc, events) = failing_service();
        assert!(svc
            .pull("/repo", "origin", &GitCredentials::SshAgent)
            .is_err());
        assert!(svc.merge_branch("/repo", "x").is_err());
        assert!(svc.stash_save("/repo", "m").is_err());
        assert!(svc.stage("/repo", &["a"]).is_err());
        assert!(svc.remove_remote("/repo", "origin").is_err());
        assert!(svc.switch_branch("/repo", "x").is_err());
        assert!(events.events().is_empty());
    }
}
