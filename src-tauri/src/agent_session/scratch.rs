use std::path::{Path, PathBuf};

use rocket_app::SessionIsolation;
use rocket_shared::error::DomainError;

/// The directory name that holds every session's scratch, inside a per-user
/// location (see `scratch_parent`).
pub const SCRATCH_PARENT_DIR: &str = "rocket-agent-sessions";

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
        // Best effort. A leftover directory holds no credential, because the
        // API key reaches the agent through its environment only.
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
}
