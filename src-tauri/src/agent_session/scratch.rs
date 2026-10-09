use std::path::{Path, PathBuf};

use rocket_app::SessionIsolation;
use rocket_shared::error::DomainError;

/// The directory name that holds every session's scratch, inside a per-user
/// location (see `scratch_parent`).
pub const SCRATCH_PARENT_DIR: &str = "rocket-agent-sessions";

/// The file in each scratch root that records the owning process id.
const OWNER_MARKER: &str = ".owner-pid";

/// How long a root of unknown liveness is kept, where pid liveness cannot be
/// checked (Unix without `/proc`).
const UNCERTAIN_KEEP: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// One session's private scratch: `<root>/cwd` is the agent's working
/// directory and `<root>/config` its `CLAUDE_CONFIG_DIR`. Both start empty.
/// Dropping the value removes the whole root, including anything the agent
/// wrote there.
pub struct SessionScratch {
    root: PathBuf,
    cwd: PathBuf,
    config_dir: PathBuf,
}

impl SessionScratch {
    /// Creates a scratch under the per-user scratch parent.
    pub fn create() -> std::io::Result<Self> {
        Self::create_in(&scratch_parent())
    }

    /// Creates a scratch under `parent`, which is created when missing.
    pub fn create_in(parent: &Path) -> std::io::Result<Self> {
        ensure_private_parent(parent)?;
        let root = parent.join(uuid::Uuid::new_v4().to_string());
        create_private_dir(&root)?;
        // From here on, an early return drops `scratch` and removes the root.
        let scratch = Self {
            cwd: root.join("cwd"),
            config_dir: root.join("config"),
            root,
        };
        std::fs::write(scratch.root.join(OWNER_MARKER), std::process::id().to_string())?;
        create_private_dir(&scratch.cwd)?;
        create_private_dir(&scratch.config_dir)?;
        Ok(scratch)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    /// Returns the cwd string for `session/new` and the isolation inputs
    /// that point `CLAUDE_CONFIG_DIR` at this scratch.
    pub fn isolation(&self) -> Result<(String, SessionIsolation), DomainError> {
        let cwd = path_to_string(&self.cwd)?;
        let config_dir = path_to_string(&self.config_dir)?;
        Ok((cwd, SessionIsolation::new(config_dir)))
    }
}

impl Drop for SessionScratch {
    fn drop(&mut self) {
        // Best effort. The config dir holds Claude Code transcripts and
        // state, such as tool results, so a leftover is swept at startup.
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Picks a per-user parent: the runtime dir when set, else the Rocket data
/// dir, else the system temp dir. Every choice is verified on use.
fn scratch_parent() -> PathBuf {
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(runtime).join(SCRATCH_PARENT_DIR);
    }
    if let Some(home) = dirs::home_dir() {
        return home.join(".rocket-api").join(SCRATCH_PARENT_DIR);
    }
    std::env::temp_dir().join(SCRATCH_PARENT_DIR)
}

/// Whether the process `pid` is running. `None` means it cannot be told.
fn pid_alive(pid: u32) -> Option<bool> {
    if pid == std::process::id() {
        return Some(true);
    }
    #[cfg(target_os = "linux")]
    {
        Some(Path::new("/proc").join(pid.to_string()).exists())
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// Whether a scratch root belongs to a running Rocket instance and must be
/// kept. A root is stale when its owner pid is dead, or its marker is missing
/// or unparseable (a crash before the marker was written, or foreign data).
/// When liveness cannot be determined, the root is kept only while it is
/// younger than 24 hours, so a stray root is eventually cleaned up.
fn is_live_root(root: &Path) -> bool {
    let pid = std::fs::read_to_string(root.join(OWNER_MARKER))
        .ok()
        .and_then(|text| text.trim().parse::<u32>().ok());
    let Some(pid) = pid else {
        return false;
    };
    match pid_alive(pid) {
        Some(alive) => alive,
        None => std::fs::metadata(root)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age < UNCERTAIN_KEEP),
    }
}

/// Removes every stale `<uuid>/` directory under the scratch parent, left
/// behind by a crashed run. Roots owned by a running process, such as a
/// second Rocket instance, are kept (see `is_live_root`). Run once at
/// startup, before any session can start.
/// Only real directories whose names parse as UUIDs are removed. Symlinks
/// and other files are never followed or touched. Returns how many were
/// removed. A missing parent is not an error.
pub fn sweep_stale_scratch() -> usize {
    sweep_stale_scratch_in(&scratch_parent())
}

/// Same as `sweep_stale_scratch`, for an explicit `parent`.
pub fn sweep_stale_scratch_in(parent: &Path) -> usize {
    let entries = match std::fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return 0,
        Err(e) => {
            tracing::warn!("cannot read the agent scratch parent {}: {e}", parent.display());
            return 0;
        }
    };
    let mut removed = 0;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                tracing::warn!("cannot read an agent scratch entry: {e}");
                continue;
            }
        };
        let is_uuid = entry
            .file_name()
            .to_str()
            .is_some_and(|name| uuid::Uuid::parse_str(name).is_ok());
        // `DirEntry::file_type` does not follow symlinks.
        let is_real_dir = entry.file_type().is_ok_and(|t| t.is_dir());
        if !is_uuid || !is_real_dir || is_live_root(&entry.path()) {
            continue;
        }
        match std::fs::remove_dir_all(entry.path()) {
            Ok(()) => removed += 1,
            Err(e) => tracing::warn!(
                "cannot remove the stale agent scratch {}: {e}",
                entry.path().display()
            ),
        }
    }
    removed
}

/// Creates `parent` with mode 0700 when missing. On Unix an existing parent
/// must be a real directory (not a symlink), owned by the current user, with
/// no group or other access. Anything else is refused.
fn ensure_private_parent(parent: &Path) -> std::io::Result<()> {
    if let Some(grandparent) = parent.parent() {
        std::fs::create_dir_all(grandparent)?;
    }
    match create_private_dir(parent) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    verify_private_parent(parent)
}

#[cfg(unix)]
fn verify_private_parent(parent: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let refuse = |why: &str| {
        std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("unsafe agent scratch parent {}: {why}", parent.display()),
        )
    };
    let meta = std::fs::symlink_metadata(parent)?;
    if !meta.file_type().is_dir() {
        return Err(refuse("not a real directory"));
    }
    if meta.uid() != current_uid()? {
        return Err(refuse("not owned by the current user"));
    }
    if meta.permissions().mode() & 0o077 != 0 {
        return Err(refuse("group or other access is set"));
    }
    Ok(())
}

#[cfg(not(unix))]
fn verify_private_parent(_parent: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Reads the current uid from a file this process creates, since the owner of
/// a new file is the current user.
#[cfg(unix)]
fn current_uid() -> std::io::Result<u32> {
    use std::os::unix::fs::MetadataExt;
    let probe = std::env::temp_dir().join(format!(".rocket-uid-{}", uuid::Uuid::new_v4()));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)?;
    let uid = std::fs::metadata(&probe).map(|m| m.uid());
    let _ = std::fs::remove_file(&probe);
    uid
}

fn create_private_dir(path: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

fn path_to_string(path: &Path) -> Result<String, DomainError> {
    path.to_str().map(str::to_string).ok_or_else(|| {
        DomainError::Internal("the agent scratch directory path is not valid UTF-8".to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn create_in_makes_empty_cwd_and_config_dirs_under_a_fresh_root() {
        let parent = TempDir::new().expect("tempdir");
        let scratch = SessionScratch::create_in(parent.path()).expect("create scratch");

        assert!(scratch.root().starts_with(parent.path()));
        assert!(scratch.cwd().is_dir());
        assert!(scratch.config_dir().is_dir());
        assert_ne!(scratch.cwd(), scratch.config_dir());
        assert_eq!(std::fs::read_dir(scratch.cwd()).expect("read cwd").count(), 0);
        assert_eq!(
            std::fs::read_dir(scratch.config_dir()).expect("read config").count(),
            0
        );
    }

    #[test]
    fn two_scratches_never_share_a_root() {
        let parent = TempDir::new().expect("tempdir");
        let a = SessionScratch::create_in(parent.path()).expect("create a");
        let b = SessionScratch::create_in(parent.path()).expect("create b");
        assert_ne!(a.root(), b.root());
    }

    #[test]
    fn dropping_removes_the_whole_root_even_with_files_inside() {
        let parent = TempDir::new().expect("tempdir");
        let scratch = SessionScratch::create_in(parent.path()).expect("create scratch");
        let root = scratch.root().to_path_buf();
        std::fs::create_dir_all(scratch.config_dir().join("projects"))
            .expect("agent-written subdir");
        std::fs::write(scratch.config_dir().join(".claude.json"), "{}")
            .expect("agent-written file");

        drop(scratch);

        assert!(!root.exists());
    }

    #[cfg(unix)]
    #[test]
    fn scratch_dirs_are_private_to_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let parent = TempDir::new().expect("tempdir");
        let scratch = SessionScratch::create_in(parent.path()).expect("create scratch");
        for dir in [scratch.root(), scratch.cwd(), scratch.config_dir()] {
            let mode = std::fs::metadata(dir).expect("metadata").permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "{} must be 0700", dir.display());
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_pre_existing_open_parent_is_rejected() {
        use std::os::unix::fs::PermissionsExt;
        let base = TempDir::new().expect("tempdir");
        let parent = base.path().join("open-parent");
        std::fs::create_dir(&parent).expect("create parent");
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o777))
            .expect("chmod parent");

        let err = SessionScratch::create_in(&parent).err().expect("must be rejected");
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_parent_is_rejected() {
        let base = TempDir::new().expect("tempdir");
        let target = base.path().join("target");
        std::fs::create_dir(&target).expect("create target");
        let link = base.path().join("link");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        let err = SessionScratch::create_in(&link).err().expect("must be rejected");
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn isolation_returns_the_cwd_and_points_config_dir_at_the_scratch() {
        let parent = TempDir::new().expect("tempdir");
        let scratch = SessionScratch::create_in(parent.path()).expect("create scratch");
        let (cwd, isolation) = scratch.isolation().expect("utf-8 paths");
        assert_eq!(Path::new(&cwd), scratch.cwd());
        assert_eq!(Path::new(&isolation.config_dir), scratch.config_dir());
    }

    fn root_with_owner(parent: &Path, owner: Option<&str>) -> PathBuf {
        let root = parent.join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(root.join("cwd")).expect("root");
        if let Some(owner) = owner {
            std::fs::write(root.join(OWNER_MARKER), owner).expect("marker");
        }
        root
    }

    #[test]
    fn create_in_writes_the_owner_marker() {
        let parent = TempDir::new().expect("tempdir");
        let scratch = SessionScratch::create_in(parent.path()).expect("create scratch");
        let text = std::fs::read_to_string(scratch.root().join(OWNER_MARKER)).expect("marker");
        assert_eq!(text, std::process::id().to_string());
    }

    #[test]
    fn sweep_keeps_a_root_owned_by_a_live_process() {
        let parent = TempDir::new().expect("tempdir");
        let live = root_with_owner(parent.path(), Some(&std::process::id().to_string()));
        assert_eq!(sweep_stale_scratch_in(parent.path()), 0);
        assert!(live.is_dir());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn sweep_removes_a_root_owned_by_a_dead_process() {
        let parent = TempDir::new().expect("tempdir");
        let dead = root_with_owner(parent.path(), Some("4294967294"));
        assert_eq!(sweep_stale_scratch_in(parent.path()), 1);
        assert!(!dead.exists());
    }

    #[test]
    fn sweep_removes_a_root_without_a_usable_marker() {
        let parent = TempDir::new().expect("tempdir");
        let none = root_with_owner(parent.path(), None);
        let bad = root_with_owner(parent.path(), Some("not-a-pid"));
        assert_eq!(sweep_stale_scratch_in(parent.path()), 2);
        assert!(!none.exists());
        assert!(!bad.exists());
    }

    #[test]
    fn sweep_removes_uuid_dirs_and_leaves_everything_else() {
        let parent = TempDir::new().expect("tempdir");
        let stale = parent.path().join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(stale.join("config/projects")).expect("stale dir");
        std::fs::write(stale.join("config/.claude.json"), "{}").expect("stale file");
        let other_dir = parent.path().join("not-a-uuid");
        std::fs::create_dir(&other_dir).expect("other dir");
        let uuid_named_file = parent.path().join(uuid::Uuid::new_v4().to_string() + ".txt");
        std::fs::write(&uuid_named_file, "x").expect("other file");
        let file_named_like_uuid = parent.path().join(uuid::Uuid::new_v4().to_string());
        std::fs::write(&file_named_like_uuid, "x").expect("uuid-named file");

        assert_eq!(sweep_stale_scratch_in(parent.path()), 1);

        assert!(!stale.exists());
        assert!(other_dir.is_dir());
        assert!(uuid_named_file.is_file());
        assert!(file_named_like_uuid.is_file());
    }

    #[cfg(unix)]
    #[test]
    fn sweep_does_not_follow_a_uuid_named_symlink() {
        let base = TempDir::new().expect("tempdir");
        let parent = base.path().join("parent");
        std::fs::create_dir(&parent).expect("parent");
        let target = base.path().join("target");
        std::fs::create_dir(&target).expect("target");
        std::fs::write(target.join("keep.txt"), "x").expect("target file");
        let link = parent.join(uuid::Uuid::new_v4().to_string());
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        assert_eq!(sweep_stale_scratch_in(&parent), 0);

        assert!(target.join("keep.txt").is_file());
        assert!(std::fs::symlink_metadata(&link).is_ok());
    }

    #[test]
    fn sweep_of_a_missing_parent_removes_nothing() {
        let base = TempDir::new().expect("tempdir");
        assert_eq!(sweep_stale_scratch_in(&base.path().join("absent")), 0);
    }
}
