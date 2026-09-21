use std::fs;
use std::path::{Path, PathBuf};

use git2::{IndexEntry, Oid, Repository, Signature};
use rocket_shared::error::DomainError;
use tempfile::TempDir;

use super::Git2Service;
use crate::conflict::ConflictResolution;
use crate::credentials::GitCredentials;
use crate::service::GitService;

const TRACKED_FILE: &str = "tracked.txt";
const COMMITTED_CONTENT: &str = "committed\n";
const OUTSIDE_CONTENT: &str = "outside-original\n";

struct RepoFixture {
    root: TempDir,
    repo_path: PathBuf,
}

impl RepoFixture {
    fn new() -> Self {
        let root = TempDir::new().expect("create fixture root");
        let repo_path = root.path().join("repo");
        fs::create_dir(&repo_path).expect("create repository directory");

        let repo = Repository::init(&repo_path).expect("initialize repository");
        repo.set_head("refs/heads/main")
            .expect("set deterministic default branch");
        set_identity(&repo);

        fs::write(repo_path.join(TRACKED_FILE), COMMITTED_CONTENT)
            .expect("write initial tracked file");
        commit_paths(&repo, &[TRACKED_FILE], "initial");

        fs::write(root.path().join("outside.txt"), OUTSIDE_CONTENT)
            .expect("write outside fixture file");

        Self { root, repo_path }
    }

    fn path(&self) -> &str {
        self.repo_path
            .to_str()
            .expect("temporary repository path must be UTF-8")
    }

    fn outside_path(&self) -> PathBuf {
        self.root.path().join("outside.txt")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RepositorySnapshot {
    head: ReferenceSnapshot,
    refs: Vec<ReferenceSnapshot>,
    index: Vec<IndexEntrySnapshot>,
    worktree: Vec<WorktreeEntrySnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ReferenceSnapshot {
    name: String,
    symbolic_target: Option<String>,
    direct_target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct IndexEntrySnapshot {
    path: Vec<u8>,
    oid: String,
    mode: u32,
    flags: u16,
    flags_extended: u16,
}

impl From<IndexEntry> for IndexEntrySnapshot {
    fn from(entry: IndexEntry) -> Self {
        Self {
            path: entry.path.clone(),
            oid: entry.id.to_string(),
            mode: entry.mode,
            flags: entry.flags,
            flags_extended: entry.flags_extended,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct WorktreeEntrySnapshot {
    path: String,
    kind: WorktreeEntryKind,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum WorktreeEntryKind {
    Directory,
    File(Vec<u8>),
    Symlink(String),
}

fn set_identity(repo: &Repository) {
    let config = repo.config().expect("open repository config");
    let mut local = config
        .open_level(git2::ConfigLevel::Local)
        .expect("open local repository config");
    local
        .set_str("user.name", "Safety Contract")
        .expect("set test user name");
    local
        .set_str("user.email", "safety@example.test")
        .expect("set test user email");
}

fn signature() -> Signature<'static> {
    Signature::new(
        "Safety Contract",
        "safety@example.test",
        &git2::Time::new(1, 0),
    )
    .expect("create deterministic signature")
}

fn commit_paths(repo: &Repository, paths: &[&str], message: &str) -> Oid {
    let mut index = repo.index().expect("open index");
    for path in paths {
        let full_path = repo
            .workdir()
            .expect("fixture repository has a worktree")
            .join(path);
        if full_path.exists() {
            index
                .add_path(Path::new(path))
                .expect("add fixture path to index");
        } else {
            index
                .remove_path(Path::new(path))
                .expect("remove fixture path from index");
        }
    }
    index.write().expect("write fixture index");
    let tree_oid = index.write_tree().expect("write fixture tree");
    let tree = repo.find_tree(tree_oid).expect("find fixture tree");
    let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
    let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
    let signature = signature();

    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &parents,
    )
    .expect("create fixture commit")
}

fn snapshot(repo_path: &Path) -> RepositorySnapshot {
    let repo = Repository::open(repo_path).expect("open repository for snapshot");
    let head = snapshot_reference(
        &repo
            .find_reference("HEAD")
            .expect("fixture repository must have HEAD"),
    );

    let mut refs = repo
        .references()
        .expect("enumerate references")
        .map(|reference| snapshot_reference(&reference.expect("read reference")))
        .collect::<Vec<_>>();
    refs.sort();

    let mut index = repo
        .index()
        .expect("open index for snapshot")
        .iter()
        .map(IndexEntrySnapshot::from)
        .collect::<Vec<_>>();
    index.sort();

    let mut worktree = Vec::new();
    snapshot_worktree(repo_path, repo_path, &mut worktree);
    worktree.sort();

    RepositorySnapshot {
        head,
        refs,
        index,
        worktree,
    }
}

fn snapshot_reference(reference: &git2::Reference<'_>) -> ReferenceSnapshot {
    ReferenceSnapshot {
        name: reference.name().unwrap_or("<non-utf8>").to_string(),
        symbolic_target: reference.symbolic_target().ok().flatten().map(String::from),
        direct_target: reference.target().map(|oid| oid.to_string()),
    }
}

fn snapshot_worktree(root: &Path, directory: &Path, entries: &mut Vec<WorktreeEntrySnapshot>) {
    let mut children = fs::read_dir(directory)
        .expect("read worktree directory")
        .map(|entry| entry.expect("read worktree entry"))
        .collect::<Vec<_>>();
    children.sort_by_key(|entry| entry.file_name());

    for entry in children {
        let path = entry.path();
        if path == root.join(".git") {
            continue;
        }

        let relative = path
            .strip_prefix(root)
            .expect("worktree entry must be below root")
            .to_string_lossy()
            .replace('\\', "/");
        let metadata = fs::symlink_metadata(&path).expect("read worktree metadata");
        let kind = if metadata.file_type().is_symlink() {
            WorktreeEntryKind::Symlink(
                fs::read_link(&path)
                    .expect("read worktree symlink")
                    .to_string_lossy()
                    .into_owned(),
            )
        } else if metadata.is_dir() {
            WorktreeEntryKind::Directory
        } else {
            WorktreeEntryKind::File(fs::read(&path).expect("read worktree file"))
        };

        entries.push(WorktreeEntrySnapshot {
            path: relative,
            kind,
        });
        if metadata.is_dir() {
            snapshot_worktree(root, &path, entries);
        }
    }
}

fn read_optional(path: &Path) -> Option<Vec<u8>> {
    fs::read(path).ok()
}

fn discard_artifacts(parent: &Path) -> Vec<String> {
    let mut artifacts = fs::read_dir(parent)
        .expect("read artifact parent")
        .map(|entry| entry.expect("read artifact entry").file_name())
        .filter_map(|name| name.into_string().ok())
        .filter(|name| {
            name.starts_with(".rocket-checkout-") || name.starts_with(".rocket-discard-backup-")
        })
        .collect::<Vec<_>>();
    artifacts.sort();
    artifacts
}

fn index_blob(repo_path: &Path, path: &str) -> Vec<u8> {
    let repo = Repository::open(repo_path).expect("open repository");
    let index = repo.index().expect("open index");
    let entry = index
        .get_path(Path::new(path), 0)
        .expect("find stage-zero index entry");
    let content = repo
        .find_blob(entry.id)
        .expect("find indexed blob")
        .content()
        .to_vec();
    content
}

#[derive(Clone, Copy, Debug)]
enum UnsafePath {
    Parent,
    NativeAbsolute,
    #[cfg(unix)]
    SymlinkEscape,
    #[cfg(not(windows))]
    WindowsDriveAbsolute,
    #[cfg(not(windows))]
    WindowsUncAbsolute,
}

fn unsafe_paths() -> Vec<UnsafePath> {
    let mut paths = vec![UnsafePath::Parent, UnsafePath::NativeAbsolute];
    #[cfg(unix)]
    paths.push(UnsafePath::SymlinkEscape);
    #[cfg(not(windows))]
    {
        paths.extend([
            UnsafePath::WindowsDriveAbsolute,
            UnsafePath::WindowsUncAbsolute,
        ]);
    }
    paths
}

fn unsafe_path(fixture: &RepoFixture, case: UnsafePath) -> String {
    match case {
        UnsafePath::Parent => "../outside.txt".to_string(),
        UnsafePath::NativeAbsolute => fixture.outside_path().to_string_lossy().into_owned(),
        #[cfg(unix)]
        UnsafePath::SymlinkEscape => {
            std::os::unix::fs::symlink(fixture.root.path(), fixture.repo_path.join("escape"))
                .expect("create repository-relative symlink escape");
            "escape/outside.txt".to_string()
        }
        #[cfg(not(windows))]
        UnsafePath::WindowsDriveAbsolute => "C:\\outside\\victim.txt".to_string(),
        #[cfg(not(windows))]
        UnsafePath::WindowsUncAbsolute => "\\\\server\\share\\victim.txt".to_string(),
    }
}

fn empty_credentials() -> GitCredentials {
    GitCredentials::UserPass {
        username: String::new(),
        password: String::new(),
    }
}

fn create_remote_with_branch_files(
    main_files: &[(&str, &str)],
    feature_files: &[(&str, &str)],
) -> (TempDir, TempDir) {
    let bare_dir = TempDir::new().expect("create bare remote directory");
    let bare_repo = Repository::init_bare(bare_dir.path()).expect("initialize bare remote");
    bare_repo
        .set_head("refs/heads/main")
        .expect("set bare remote HEAD");

    let seed_dir = TempDir::new().expect("create seed clone directory");
    let seed_repo = Repository::clone(
        bare_dir.path().to_str().expect("UTF-8 remote path"),
        seed_dir.path(),
    )
    .expect("clone bare remote for seeding");
    seed_repo
        .set_head("refs/heads/main")
        .expect("set seed branch");

    for (path, content) in main_files {
        fs::write(seed_dir.path().join(path), content).expect("write main branch fixture file");
    }
    let main_paths = main_files.iter().map(|(path, _)| *path).collect::<Vec<_>>();
    let main_oid = commit_paths(&seed_repo, &main_paths, "remote main");
    seed_repo
        .find_remote("origin")
        .expect("find seed origin")
        .push(&["refs/heads/main:refs/heads/main"], None)
        .expect("push remote main");

    if !feature_files.is_empty() {
        let main_commit = seed_repo.find_commit(main_oid).expect("find main commit");
        seed_repo
            .branch("feature", &main_commit, false)
            .expect("create feature branch");
        drop(main_commit);
        seed_repo
            .set_head("refs/heads/feature")
            .expect("switch seed HEAD to feature");
        seed_repo
            .checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .expect("checkout feature branch");

        for (path, content) in feature_files {
            fs::write(seed_dir.path().join(path), content)
                .expect("write feature branch fixture file");
        }
        let feature_paths = feature_files
            .iter()
            .map(|(path, _)| *path)
            .collect::<Vec<_>>();
        commit_paths(&seed_repo, &feature_paths, "remote feature");
        seed_repo
            .find_remote("origin")
            .expect("find seed origin")
            .push(&["refs/heads/feature:refs/heads/feature"], None)
            .expect("push remote feature");
    }

    (bare_dir, seed_dir)
}

#[test]
fn repository_snapshot_captures_refs_index_and_worktree() {
    let fixture = RepoFixture::new();
    let before = snapshot(&fixture.repo_path);

    fs::write(fixture.repo_path.join(TRACKED_FILE), "staged\n").expect("write staged content");
    Git2Service::new()
        .stage(fixture.path(), &[TRACKED_FILE])
        .expect("stage fixture content");
    fs::write(fixture.repo_path.join("untracked.txt"), "untracked\n")
        .expect("write untracked content");
    let repo = Repository::open(&fixture.repo_path).expect("reopen fixture repository");
    let head_oid = repo
        .head()
        .expect("fixture has HEAD")
        .target()
        .expect("fixture HEAD is direct through its branch");
    repo.reference(
        "refs/heads/snapshot-check",
        head_oid,
        false,
        "exercise snapshot ref detection",
    )
    .expect("create snapshot-check ref");
    let after = snapshot(&fixture.repo_path);

    assert_ne!(
        before.index, after.index,
        "snapshot must detect index changes"
    );
    assert_ne!(
        before.worktree, after.worktree,
        "snapshot must detect worktree changes"
    );
    assert_eq!(before.head, after.head, "staging must not move HEAD");
    assert_ne!(before.refs, after.refs, "snapshot must detect ref changes");
}

#[test]
fn diff_rejects_parent_absolute_and_windows_style_paths_without_mutation() {
    let mut failures = Vec::new();

    for case in unsafe_paths() {
        let fixture = RepoFixture::new();
        let unsafe_path = unsafe_path(&fixture, case);
        let before_repo = snapshot(&fixture.repo_path);
        let before_outside = read_optional(&fixture.outside_path());

        let result = Git2Service::new().diff_file(fixture.path(), &unsafe_path);
        let after_repo = snapshot(&fixture.repo_path);
        let after_outside = read_optional(&fixture.outside_path());

        if result.is_ok() || before_repo != after_repo || before_outside != after_outside {
            failures.push(format!(
                "{case:?}: result={result:?}, repo_unchanged={}, outside_unchanged={}",
                before_repo == after_repo,
                before_outside == after_outside
            ));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn discard_rejects_parent_absolute_and_windows_style_paths_without_mutation() {
    let mut failures = Vec::new();

    for case in unsafe_paths() {
        let fixture = RepoFixture::new();
        let unsafe_path = unsafe_path(&fixture, case);
        let before_repo = snapshot(&fixture.repo_path);
        let before_outside = read_optional(&fixture.outside_path());

        let result = Git2Service::new().discard(fixture.path(), &[unsafe_path.as_str()]);
        let after_repo = snapshot(&fixture.repo_path);
        let after_outside = read_optional(&fixture.outside_path());

        if result.is_ok() || before_repo != after_repo || before_outside != after_outside {
            failures.push(format!(
                "{case:?}: result={result:?}, repo_unchanged={}, outside_unchanged={}",
                before_repo == after_repo,
                before_outside == after_outside
            ));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn resolve_rejects_parent_absolute_and_windows_style_paths_without_mutation() {
    let mut failures = Vec::new();

    for case in unsafe_paths() {
        let fixture = RepoFixture::new();
        let unsafe_path = unsafe_path(&fixture, case);
        let before_repo = snapshot(&fixture.repo_path);
        let before_outside = read_optional(&fixture.outside_path());
        let resolution = ConflictResolution::Custom {
            content: "attacker-controlled replacement\n".to_string(),
        };

        let result = Git2Service::new().resolve_conflict(fixture.path(), &unsafe_path, &resolution);
        let after_repo = snapshot(&fixture.repo_path);
        let after_outside = read_optional(&fixture.outside_path());

        if result.is_ok() || before_repo != after_repo || before_outside != after_outside {
            failures.push(format!(
                "{case:?}: result={result:?}, repo_unchanged={}, outside_unchanged={}",
                before_repo == after_repo,
                before_outside == after_outside
            ));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[cfg(unix)]
#[test]
fn leaf_symlink_is_staged_as_symlink_but_not_followed_by_diff_or_discard() {
    use std::os::unix::fs::symlink;

    let fixture = RepoFixture::new();
    let service = Git2Service::new();
    let outside_before = read_optional(&fixture.outside_path());
    symlink(fixture.outside_path(), fixture.repo_path.join("link.txt"))
        .expect("create leaf symlink");

    assert!(
        service.diff_file(fixture.path(), "link.txt").is_err(),
        "diff must reject a symlink leaf"
    );
    service
        .stage(fixture.path(), &["link.txt"])
        .expect("stage symlink itself");
    let repo = Repository::open(&fixture.repo_path).expect("open repository");
    let entry = repo
        .index()
        .expect("open index")
        .get_path(Path::new("link.txt"), 0)
        .expect("find staged symlink");
    assert_eq!(entry.mode, 0o120000, "index entry must retain symlink mode");

    // The symlink is now staged, so "discard" (restore unstaged changes)
    // must restore from the INDEX, not delete it — the worktree already
    // matches what's staged, so this is a no-op that leaves the symlink
    // entry itself in place without ever following it.
    service
        .discard(fixture.path(), &["link.txt"])
        .expect("discard restores the staged symlink entry");
    let restored = fs::symlink_metadata(fixture.repo_path.join("link.txt"))
        .expect("discard must leave the staged symlink entry in place");
    assert!(
        restored.file_type().is_symlink(),
        "discard must not replace the symlink with a regular file"
    );
    assert_eq!(
        read_optional(&fixture.outside_path()),
        outside_before,
        "symlink target must not be changed"
    );
}

#[cfg(unix)]
#[test]
fn symlinked_parent_is_rejected_for_stage() {
    use std::os::unix::fs::symlink;

    let fixture = RepoFixture::new();
    symlink(fixture.root.path(), fixture.repo_path.join("escape"))
        .expect("create symlinked parent");
    let before = snapshot(&fixture.repo_path);

    let result = Git2Service::new().stage(fixture.path(), &["escape/outside.txt"]);

    assert!(result.is_err(), "stage must reject a symlinked parent");
    assert_eq!(snapshot(&fixture.repo_path), before);
}

#[test]
fn stage_batch_preflight_is_atomic() {
    let fixture = RepoFixture::new();
    fs::write(fixture.repo_path.join(TRACKED_FILE), "changed\n").expect("modify tracked file");
    let before = snapshot(&fixture.repo_path);

    let result = Git2Service::new().stage(fixture.path(), &[TRACKED_FILE, "../outside.txt"]);

    assert!(result.is_err(), "invalid batch must be rejected");
    assert_eq!(snapshot(&fixture.repo_path), before);
}

#[test]
fn unstage_batch_preflight_is_atomic() {
    let fixture = RepoFixture::new();
    let service = Git2Service::new();
    fs::write(fixture.repo_path.join(TRACKED_FILE), "staged\n").expect("modify tracked file");
    service
        .stage(fixture.path(), &[TRACKED_FILE])
        .expect("stage tracked file");
    let before = snapshot(&fixture.repo_path);

    let result = service.unstage(fixture.path(), &[TRACKED_FILE, "../outside.txt"]);

    assert!(result.is_err(), "invalid batch must be rejected");
    assert_eq!(snapshot(&fixture.repo_path), before);
}

#[test]
fn discard_batch_preflight_is_atomic() {
    let fixture = RepoFixture::new();
    fs::write(fixture.repo_path.join(TRACKED_FILE), "changed\n").expect("modify tracked file");
    let before = snapshot(&fixture.repo_path);

    let result = Git2Service::new().discard(fixture.path(), &[TRACKED_FILE, "../outside.txt"]);

    assert!(result.is_err(), "invalid batch must be rejected");
    assert_eq!(snapshot(&fixture.repo_path), before);
}

#[test]
fn unstage_literal_metacharacter_path_only_updates_exact_entry() {
    const LITERAL: &str = "literal[1].txt";
    const GLOB_MATCH: &str = "literal1.txt";

    let fixture = RepoFixture::new();
    let service = Git2Service::new();
    fs::write(fixture.repo_path.join(LITERAL), "literal original\n")
        .expect("write literal metacharacter file");
    fs::write(fixture.repo_path.join(GLOB_MATCH), "glob original\n")
        .expect("write pathspec-match file");
    let repo = Repository::open(&fixture.repo_path).expect("open repository");
    commit_paths(&repo, &[LITERAL, GLOB_MATCH], "add metacharacter fixtures");

    fs::write(fixture.repo_path.join(LITERAL), "literal staged\n")
        .expect("modify literal metacharacter file");
    fs::write(fixture.repo_path.join(GLOB_MATCH), "glob staged\n")
        .expect("modify pathspec-match file");
    service
        .stage(fixture.path(), &[LITERAL, GLOB_MATCH])
        .expect("stage both files");

    service
        .unstage(fixture.path(), &[LITERAL])
        .expect("unstage exact literal path");

    assert_eq!(
        index_blob(&fixture.repo_path, LITERAL),
        b"literal original\n"
    );
    assert_eq!(index_blob(&fixture.repo_path, GLOB_MATCH), b"glob staged\n");
}

#[test]
fn discard_literal_metacharacter_path_only_restores_exact_entry() {
    const LITERAL: &str = "literal[1].txt";
    const GLOB_MATCH: &str = "literal1.txt";

    let fixture = RepoFixture::new();
    let service = Git2Service::new();
    fs::write(fixture.repo_path.join(LITERAL), "literal original\n")
        .expect("write literal metacharacter file");
    fs::write(fixture.repo_path.join(GLOB_MATCH), "glob original\n")
        .expect("write pathspec-match file");
    let repo = Repository::open(&fixture.repo_path).expect("open repository");
    commit_paths(&repo, &[LITERAL, GLOB_MATCH], "add metacharacter fixtures");

    fs::write(fixture.repo_path.join(LITERAL), "literal changed\n")
        .expect("modify literal metacharacter file");
    fs::write(fixture.repo_path.join(GLOB_MATCH), "glob changed\n")
        .expect("modify pathspec-match file");

    service
        .discard(fixture.path(), &[LITERAL])
        .expect("discard exact literal path");

    assert_eq!(
        fs::read(fixture.repo_path.join(LITERAL)).expect("read restored literal file"),
        b"literal original\n"
    );
    assert_eq!(
        fs::read(fixture.repo_path.join(GLOB_MATCH)).expect("read untouched matching file"),
        b"glob changed\n"
    );
}

#[test]
fn tracked_file_collision_with_non_empty_directory_rejects_without_mutation() {
    const FIRST_PATH: &str = "first.txt";
    let fixture = RepoFixture::new();
    fs::write(fixture.repo_path.join(FIRST_PATH), "first original\n")
        .expect("write first tracked file");
    let repo = Repository::open(&fixture.repo_path).expect("open repository");
    commit_paths(&repo, &[FIRST_PATH], "add first tracked file");
    fs::write(fixture.repo_path.join(FIRST_PATH), "first changed\n")
        .expect("modify first tracked file");

    let tracked_path = fixture.repo_path.join(TRACKED_FILE);
    fs::remove_file(&tracked_path).expect("remove tracked file");
    fs::create_dir(&tracked_path).expect("create colliding directory");
    fs::write(tracked_path.join("descendant.txt"), "preserve me\n")
        .expect("write colliding descendant");
    let before = snapshot(&fixture.repo_path);

    let result = Git2Service::new().discard(fixture.path(), &[FIRST_PATH, TRACKED_FILE]);

    assert!(result.is_err(), "non-empty directory collision must reject");
    assert_eq!(snapshot(&fixture.repo_path), before);
    assert!(discard_artifacts(&fixture.repo_path).is_empty());
}

#[cfg(unix)]
#[test]
fn tracked_symlink_discard_restores_link_type_and_target() {
    use std::os::unix::fs::symlink;

    const LINK: &str = "tracked-link";
    let fixture = RepoFixture::new();
    fs::write(fixture.repo_path.join("target-a"), "target a\n").expect("write first target");
    fs::write(fixture.repo_path.join("target-b"), "target b\n").expect("write second target");
    symlink("target-a", fixture.repo_path.join(LINK)).expect("create tracked symlink");
    let repo = Repository::open(&fixture.repo_path).expect("open repository");
    commit_paths(&repo, &[LINK], "add tracked symlink");

    fs::remove_file(fixture.repo_path.join(LINK)).expect("remove original symlink");
    symlink("target-b", fixture.repo_path.join(LINK)).expect("replace symlink target");
    Git2Service::new()
        .discard(fixture.path(), &[LINK])
        .expect("restore tracked symlink");

    assert!(
        fs::symlink_metadata(fixture.repo_path.join(LINK))
            .expect("inspect restored symlink")
            .file_type()
            .is_symlink(),
        "restored entry must remain a symlink"
    );
    assert_eq!(
        fs::read_link(fixture.repo_path.join(LINK)).expect("read restored symlink"),
        PathBuf::from("target-a")
    );
    assert!(discard_artifacts(&fixture.repo_path).is_empty());
}

#[cfg(unix)]
#[test]
fn tracked_executable_discard_restores_executable_mode() {
    use std::os::unix::fs::PermissionsExt;

    const SCRIPT: &str = "script.sh";
    let fixture = RepoFixture::new();
    let script_path = fixture.repo_path.join(SCRIPT);
    fs::write(&script_path, "#!/bin/sh\necho original\n").expect("write executable script");
    fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))
        .expect("make script executable");
    let repo = Repository::open(&fixture.repo_path).expect("open repository");
    commit_paths(&repo, &[SCRIPT], "add executable script");

    fs::write(&script_path, "changed\n").expect("change executable script");
    fs::set_permissions(&script_path, fs::Permissions::from_mode(0o644))
        .expect("remove executable mode");
    Git2Service::new()
        .discard(fixture.path(), &[SCRIPT])
        .expect("restore executable script");

    let mode = fs::metadata(&script_path)
        .expect("inspect restored script")
        .permissions()
        .mode();
    assert_ne!(mode & 0o111, 0, "executable bits must be restored");
    assert_eq!(
        fs::read(&script_path).expect("read restored script"),
        b"#!/bin/sh\necho original\n"
    );
    assert!(discard_artifacts(&fixture.repo_path).is_empty());
}

#[test]
fn successful_discard_leaves_no_temporary_or_backup_artifacts() {
    let fixture = RepoFixture::new();
    fs::write(fixture.repo_path.join(TRACKED_FILE), "changed\n").expect("modify tracked file");

    Git2Service::new()
        .discard(fixture.path(), &[TRACKED_FILE])
        .expect("discard tracked change");

    assert!(discard_artifacts(&fixture.repo_path).is_empty());
}

#[test]
fn discard_with_separate_git_directory_materializes_in_worktree() {
    let root = TempDir::new().expect("create separate-git-dir fixture");
    let worktree = root.path().join("worktree");
    let git_dir = root.path().join("git-data");
    fs::create_dir(&worktree).expect("create separate worktree");
    let mut options = git2::RepositoryInitOptions::new();
    options.workdir_path(&worktree).initial_head("main");
    let repo =
        Repository::init_opts(&git_dir, &options).expect("initialize separate git directory");
    set_identity(&repo);
    fs::write(worktree.join(TRACKED_FILE), COMMITTED_CONTENT).expect("write tracked file");
    commit_paths(&repo, &[TRACKED_FILE], "initial");
    fs::write(worktree.join(TRACKED_FILE), "changed\n").expect("modify tracked file");

    Git2Service::new()
        .discard(
            git_dir.to_str().expect("UTF-8 git directory"),
            &[TRACKED_FILE],
        )
        .expect("discard through separate git directory");

    assert_eq!(
        fs::read(worktree.join(TRACKED_FILE)).expect("read restored tracked file"),
        COMMITTED_CONTENT.as_bytes()
    );
    assert!(discard_artifacts(&worktree).is_empty());
}

#[test]
fn overlapping_discard_paths_reject_before_mutation() {
    let fixture = RepoFixture::new();
    let directory = fixture.repo_path.join("untracked");
    fs::create_dir(&directory).expect("create untracked directory");
    fs::write(directory.join("child.txt"), "keep me\n").expect("write untracked child");
    let before = snapshot(&fixture.repo_path);

    let result = Git2Service::new().discard(fixture.path(), &["untracked", "untracked/child.txt"]);

    assert!(result.is_err(), "overlapping paths must be rejected");
    assert_eq!(snapshot(&fixture.repo_path), before);
}

#[test]
fn discard_directory_with_tracked_descendants_rejects_without_mutation() {
    let fixture = RepoFixture::new();
    let directory = fixture.repo_path.join("tracked-directory");
    fs::create_dir(&directory).expect("create tracked directory");
    fs::write(directory.join("tracked.txt"), "tracked\n").expect("write tracked descendant");
    let repo = Repository::open(&fixture.repo_path).expect("open repository");
    commit_paths(
        &repo,
        &["tracked-directory/tracked.txt"],
        "add tracked descendant",
    );
    fs::write(directory.join("untracked.txt"), "untracked\n").expect("write untracked descendant");
    let before = snapshot(&fixture.repo_path);

    let result = Git2Service::new().discard(fixture.path(), &["tracked-directory"]);

    assert!(result.is_err(), "directory discard must be rejected");
    assert_eq!(snapshot(&fixture.repo_path), before);
}

#[test]
fn deleted_tracked_file_remains_stageable() {
    let fixture = RepoFixture::new();
    fs::remove_file(fixture.repo_path.join(TRACKED_FILE)).expect("delete tracked file");

    Git2Service::new()
        .stage(fixture.path(), &[TRACKED_FILE])
        .expect("stage tracked deletion");

    let repo = Repository::open(&fixture.repo_path).expect("open repository");
    assert!(
        repo.index()
            .expect("open index")
            .get_path(Path::new(TRACKED_FILE), 0)
            .is_none(),
        "staged deletion must remove the stage-zero entry"
    );
}

#[test]
fn discard_unstaged_preserves_staged_index_blob_and_restores_worktree() {
    let fixture = RepoFixture::new();
    let service = Git2Service::new();

    fs::write(fixture.repo_path.join(TRACKED_FILE), "staged-v2\n").expect("write staged version");
    service
        .stage(fixture.path(), &[TRACKED_FILE])
        .expect("stage second version");
    let staged_blob = index_blob(&fixture.repo_path, TRACKED_FILE);

    fs::write(fixture.repo_path.join(TRACKED_FILE), "unstaged-v3\n")
        .expect("write unstaged version");
    service
        .discard(fixture.path(), &[TRACKED_FILE])
        .expect("discard unstaged changes");

    assert_eq!(
        index_blob(&fixture.repo_path, TRACKED_FILE),
        staged_blob,
        "discard must preserve the staged index blob"
    );
    assert_eq!(
        fs::read(fixture.repo_path.join(TRACKED_FILE)).expect("read restored worktree file"),
        staged_blob,
        "discard must restore the worktree from the staged blob"
    );
}

#[test]
fn remote_checkout_collision_rejects_without_refs_index_or_worktree_mutation() {
    let (bare_dir, _seed_dir) = create_remote_with_branch_files(
        &[("base.txt", "base\n")],
        &[("clash.txt", "remote feature content\n")],
    );
    let local_dir = TempDir::new().expect("create local clone directory");
    let local_path = local_dir.path().to_string_lossy().into_owned();
    Repository::clone(
        bare_dir.path().to_str().expect("UTF-8 remote path"),
        local_dir.path(),
    )
    .expect("clone local checkout fixture");
    fs::write(
        local_dir.path().join("clash.txt"),
        "local untracked content\n",
    )
    .expect("write colliding untracked file");

    let before = snapshot(local_dir.path());
    let result =
        Git2Service::new().checkout_remote_branch(&local_path, "origin/feature", false, None);
    let after = snapshot(local_dir.path());

    assert_eq!(
        after, before,
        "rejected remote checkout must not mutate HEAD, refs, index, or worktree"
    );
    assert!(
        result.is_err(),
        "colliding remote checkout must be rejected"
    );
}

#[test]
fn forced_remote_branch_reset_rejects_dirty_worktree_without_mutation() {
    let (origin_bare, _origin_seed) =
        create_remote_with_branch_files(&[("base.txt", "base\n")], &[]);
    let (other_bare, _other_seed) =
        create_remote_with_branch_files(&[("other.txt", "from other remote\n")], &[]);

    let local_dir = TempDir::new().expect("create local clone directory");
    let local_path = local_dir.path().to_string_lossy().into_owned();
    let local_repo = Repository::clone(
        origin_bare.path().to_str().expect("UTF-8 remote path"),
        local_dir.path(),
    )
    .expect("clone local checkout fixture");
    set_identity(&local_repo);
    local_repo
        .remote(
            "other",
            other_bare.path().to_str().expect("UTF-8 remote path"),
        )
        .expect("add unrelated second remote");

    let service = Git2Service::new();
    let creds = GitCredentials::UserPass {
        username: String::new(),
        password: String::new(),
    };
    service
        .fetch(&local_path, "other", &creds)
        .expect("fetch second remote");

    // An uncommitted edit to a tracked file on the branch about to be reset.
    fs::write(
        local_dir.path().join("base.txt"),
        "local uncommitted edit\n",
    )
    .expect("dirty a tracked file");

    let before = snapshot(local_dir.path());
    let result = service.checkout_remote_branch(&local_path, "other/main", true, None);
    let after = snapshot(local_dir.path());

    assert!(
        matches!(
            result,
            Err(DomainError::InvalidInput(ref message))
                if message.contains("uncommitted changes")
        ),
        "a forced reset over a dirty worktree must be rejected: {result:?}"
    );
    assert_eq!(
        after, before,
        "rejected forced reset must not mutate HEAD, refs, index, or worktree"
    );
    assert_eq!(
        fs::read(local_dir.path().join("base.txt")).expect("read dirtied file"),
        b"local uncommitted edit\n",
        "the uncommitted change must survive verbatim"
    );
}

#[test]
fn pull_collision_rejects_without_refs_index_or_worktree_mutation() {
    let (bare_dir, _seed_dir) =
        create_remote_with_branch_files(&[("clash.txt", "remote content\n")], &[]);
    let local_dir = TempDir::new().expect("create unborn local directory");
    let local_path = local_dir.path().to_string_lossy().into_owned();
    let local_repo =
        Repository::init(local_dir.path()).expect("initialize unborn local repository");
    local_repo
        .set_head("refs/heads/main")
        .expect("set unborn local branch");
    local_repo
        .remote(
            "origin",
            bare_dir.path().to_str().expect("UTF-8 remote path"),
        )
        .expect("add local remote");
    drop(local_repo);

    let service = Git2Service::new();
    service
        .fetch(&local_path, "origin", &empty_credentials())
        .expect("prefetch collision target");
    fs::write(
        local_dir.path().join("clash.txt"),
        "local untracked content\n",
    )
    .expect("write colliding untracked file");

    let before = snapshot(local_dir.path());
    let result = service.pull(&local_path, "origin", &empty_credentials());
    let after = snapshot(local_dir.path());

    assert_eq!(
        after, before,
        "rejected pull must not mutate HEAD, refs, index, or worktree"
    );
    assert!(result.is_err(), "colliding pull must be rejected");
}

#[test]
fn resolve_non_conflict_rejects_without_write() {
    let fixture = RepoFixture::new();
    let before = snapshot(&fixture.repo_path);
    let resolution = ConflictResolution::Custom {
        content: "must not be written\n".to_string(),
    };

    let result = Git2Service::new().resolve_conflict(fixture.path(), TRACKED_FILE, &resolution);
    let after = snapshot(&fixture.repo_path);

    assert_eq!(
        after, before,
        "non-conflict resolution must not mutate refs, index, or worktree"
    );
    assert!(result.is_err(), "a non-conflict path must be rejected");
}

#[test]
fn resolve_modify_delete_conflict_with_deleted_side_removes_file() {
    let fixture = RepoFixture::new();
    let service = Git2Service::new();

    service
        .create_branch(fixture.path(), "delete-side")
        .expect("create deletion branch");
    fs::remove_file(fixture.repo_path.join(TRACKED_FILE)).expect("delete file on branch");
    service
        .stage(fixture.path(), &[TRACKED_FILE])
        .expect("stage branch deletion");
    service
        .commit(fixture.path(), "delete tracked file")
        .expect("commit branch deletion");

    service
        .switch_branch(fixture.path(), "main")
        .expect("return to main");
    fs::write(fixture.repo_path.join(TRACKED_FILE), "modified on main\n")
        .expect("modify file on main");
    service
        .stage(fixture.path(), &[TRACKED_FILE])
        .expect("stage main modification");
    service
        .commit(fixture.path(), "modify tracked file")
        .expect("commit main modification");

    assert!(
        service.merge_branch(fixture.path(), "delete-side").is_err(),
        "fixture must create a modify/delete conflict"
    );
    assert!(
        service
            .conflicts(fixture.path())
            .expect("list conflicts")
            .iter()
            .any(|conflict| conflict.path == TRACKED_FILE),
        "fixture must expose the tracked-file conflict"
    );

    service
        .resolve_conflict(fixture.path(), TRACKED_FILE, &ConflictResolution::Theirs)
        .expect("accept deleted side");

    assert!(
        !fixture.repo_path.join(TRACKED_FILE).exists(),
        "accepting the deleted side must remove the worktree file"
    );
    let repo = Repository::open(&fixture.repo_path).expect("open resolved repository");
    assert!(
        repo.index()
            .expect("open resolved index")
            .get_path(Path::new(TRACKED_FILE), 0)
            .is_none(),
        "accepting the deleted side must not create an empty stage-zero blob"
    );
    assert!(
        service
            .conflicts(fixture.path())
            .expect("list remaining conflicts")
            .is_empty(),
        "resolution must clear the conflict"
    );
}

#[cfg(unix)]
#[test]
fn resolve_conflict_theirs_preserves_executable_mode() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = RepoFixture::new();
    let service = Git2Service::new();
    let script_path = fixture.repo_path.join(TRACKED_FILE);

    service
        .create_branch(fixture.path(), "exec-branch")
        .expect("create exec branch");
    fs::write(&script_path, "executable version\n").expect("write executable version");
    fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).expect("make executable");
    service
        .stage(fixture.path(), &[TRACKED_FILE])
        .expect("stage executable version");
    service
        .commit(fixture.path(), "make executable")
        .expect("commit executable version");

    service
        .switch_branch(fixture.path(), "main")
        .expect("return to main");
    fs::write(&script_path, "main version\n").expect("modify on main");
    service
        .stage(fixture.path(), &[TRACKED_FILE])
        .expect("stage main modification");
    service
        .commit(fixture.path(), "modify on main")
        .expect("commit main modification");

    assert!(
        service.merge_branch(fixture.path(), "exec-branch").is_err(),
        "fixture must create a modify/modify conflict"
    );

    service
        .resolve_conflict(fixture.path(), TRACKED_FILE, &ConflictResolution::Theirs)
        .expect("accept the executable side");

    let mode = fs::metadata(&script_path)
        .expect("inspect resolved file")
        .permissions()
        .mode();
    assert_ne!(
        mode & 0o111,
        0,
        "resolving to the executable side must preserve the executable bit"
    );
}

#[test]
fn abort_outside_merge_rejects_and_preserves_dirty_index_and_worktree() {
    let fixture = RepoFixture::new();
    let service = Git2Service::new();

    fs::write(fixture.repo_path.join(TRACKED_FILE), "staged work\n")
        .expect("write staged dirty content");
    service
        .stage(fixture.path(), &[TRACKED_FILE])
        .expect("stage dirty content");
    fs::write(fixture.repo_path.join(TRACKED_FILE), "unstaged work\n")
        .expect("write unstaged dirty content");
    fs::write(fixture.repo_path.join("untracked.txt"), "untracked work\n")
        .expect("write untracked dirty content");

    let before = snapshot(&fixture.repo_path);
    let result = service.abort_merge(fixture.path());
    let after = snapshot(&fixture.repo_path);

    assert_eq!(
        after, before,
        "abort outside a merge must preserve refs, index, and all dirty worktree content"
    );
    assert!(result.is_err(), "abort outside a merge must be rejected");
}
