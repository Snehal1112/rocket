use std::collections::HashMap;
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rocket_shared::error::{DomainError, DomainResult};
use serde::Serialize;
use uuid::{Uuid, Version};

pub const DEFAULT_CLONE_DESTINATION_CAPABILITY_TTL: Duration = Duration::from_secs(5 * 60);
pub const DEFAULT_CLONE_DESTINATION_MAX_ENTRIES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CloneDestinationGrant {
    pub capability: String,
    pub display_path: String,
    pub expires_in_seconds: u64,
}

#[derive(Debug)]
struct CapabilityEntry {
    selected_path: PathBuf,
    canonical_path: PathBuf,
    identity: DestinationIdentity,
    expires_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DestinationIdentity {
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(windows)]
    volume_serial_number: u32,
    #[cfg(windows)]
    file_index: u64,
}

#[derive(Debug)]
struct ValidatedDestination {
    canonical_path: PathBuf,
    identity: DestinationIdentity,
}

#[derive(Debug)]
pub struct CloneDestinationCapabilities {
    entries: Mutex<HashMap<Uuid, CapabilityEntry>>,
    ttl: Duration,
    max_entries: usize,
}

impl CloneDestinationCapabilities {
    pub fn new(ttl: Duration, max_entries: usize) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl,
            max_entries,
        }
    }

    pub fn issue(&self, selected_path: impl AsRef<Path>) -> DomainResult<CloneDestinationGrant> {
        let selected_path = selected_path.as_ref().to_path_buf();
        let validated = validate_destination(&selected_path, None)?;
        let now = Instant::now();
        let expires_at = now.checked_add(self.ttl).ok_or_else(|| {
            DomainError::InvalidInput("Clone destination capability TTL is too large".into())
        })?;

        let capability = {
            let mut entries = self.lock_entries()?;
            prune_expired(&mut entries, now);

            if entries.len() >= self.max_entries {
                return Err(DomainError::Conflict(
                    "Clone destination capability store is full".into(),
                ));
            }

            let capability = next_unique_capability(&entries);
            entries.insert(
                capability,
                CapabilityEntry {
                    selected_path: selected_path.clone(),
                    canonical_path: validated.canonical_path.clone(),
                    identity: validated.identity,
                    expires_at,
                },
            );
            capability
        };

        Ok(CloneDestinationGrant {
            capability: capability.to_string(),
            display_path: selected_path.to_string_lossy().into_owned(),
            expires_in_seconds: duration_as_ceil_seconds(self.ttl),
        })
    }

    pub fn consume(&self, capability: &str) -> DomainResult<PathBuf> {
        let capability = parse_capability(capability)?;
        let entry = {
            let now = Instant::now();
            let mut entries = self.lock_entries()?;
            prune_expired(&mut entries, now);
            entries.remove(&capability).ok_or_else(invalid_capability)?
        };

        let validated = validate_destination(
            &entry.selected_path,
            Some((&entry.canonical_path, entry.identity)),
        )?;
        Ok(validated.canonical_path)
    }

    fn lock_entries(
        &self,
    ) -> DomainResult<std::sync::MutexGuard<'_, HashMap<Uuid, CapabilityEntry>>> {
        self.entries.lock().map_err(|_| {
            DomainError::Internal("Clone destination capability store lock is poisoned".into())
        })
    }
}

impl Default for CloneDestinationCapabilities {
    fn default() -> Self {
        Self::new(
            DEFAULT_CLONE_DESTINATION_CAPABILITY_TTL,
            DEFAULT_CLONE_DESTINATION_MAX_ENTRIES,
        )
    }
}

fn parse_capability(capability: &str) -> DomainResult<Uuid> {
    let capability = Uuid::parse_str(capability).map_err(|_| invalid_capability())?;
    if capability.get_version() != Some(Version::Random) {
        return Err(invalid_capability());
    }
    Ok(capability)
}

fn invalid_capability() -> DomainError {
    DomainError::InvalidInput("Clone destination capability is invalid or expired".into())
}

fn next_unique_capability(entries: &HashMap<Uuid, CapabilityEntry>) -> Uuid {
    loop {
        let capability = Uuid::new_v4();
        if !entries.contains_key(&capability) {
            return capability;
        }
    }
}

fn prune_expired(entries: &mut HashMap<Uuid, CapabilityEntry>, now: Instant) {
    entries.retain(|_, entry| entry.expires_at > now);
}

fn duration_as_ceil_seconds(duration: Duration) -> u64 {
    duration
        .as_secs()
        .saturating_add(u64::from(duration.subsec_nanos() > 0))
}

fn validate_destination(
    selected_path: &Path,
    expected: Option<(&Path, DestinationIdentity)>,
) -> DomainResult<ValidatedDestination> {
    let initial_metadata = destination_metadata(selected_path)?;
    let initial_identity = destination_identity(&initial_metadata)?;

    if let Some((_, expected_identity)) = expected {
        if initial_identity != expected_identity {
            return Err(DomainError::InvalidInput(
                "Clone destination identity changed after capability issuance".into(),
            ));
        }
    }

    let canonical_path = fs::canonicalize(selected_path).map_err(|error| {
        DomainError::InvalidInput(format!(
            "Failed to canonicalize clone destination '{}': {error}",
            selected_path.display()
        ))
    })?;

    if let Some((expected_path, _)) = expected {
        if canonical_path != expected_path {
            return Err(DomainError::InvalidInput(
                "Clone destination canonical path changed after capability issuance".into(),
            ));
        }
    }

    ensure_empty(selected_path)?;

    let final_metadata = destination_metadata(selected_path)?;
    let final_identity = destination_identity(&final_metadata)?;
    let final_canonical_path = fs::canonicalize(selected_path).map_err(|error| {
        DomainError::InvalidInput(format!(
            "Failed to re-canonicalize clone destination '{}': {error}",
            selected_path.display()
        ))
    })?;

    if final_identity != initial_identity || final_canonical_path != canonical_path {
        return Err(DomainError::InvalidInput(
            "Clone destination changed while it was being validated".into(),
        ));
    }

    Ok(ValidatedDestination {
        canonical_path,
        identity: initial_identity,
    })
}

fn destination_metadata(path: &Path) -> DomainResult<Metadata> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        DomainError::InvalidInput(format!(
            "Clone destination '{}' is not accessible: {error}",
            path.display()
        ))
    })?;

    if metadata.file_type().is_symlink() {
        return Err(DomainError::InvalidInput(format!(
            "Clone destination '{}' must not be a symlink",
            path.display()
        )));
    }
    if !metadata.is_dir() {
        return Err(DomainError::InvalidInput(format!(
            "Clone destination '{}' must be a directory",
            path.display()
        )));
    }

    Ok(metadata)
}

fn ensure_empty(path: &Path) -> DomainResult<()> {
    let mut entries = fs::read_dir(path).map_err(|error| {
        DomainError::Io(format!(
            "Failed to read clone destination '{}': {error}",
            path.display()
        ))
    })?;

    match entries.next() {
        None => Ok(()),
        Some(Ok(_)) => Err(DomainError::InvalidInput(format!(
            "Clone destination '{}' must be empty",
            path.display()
        ))),
        Some(Err(error)) => Err(DomainError::Io(format!(
            "Failed to inspect clone destination '{}': {error}",
            path.display()
        ))),
    }
}

#[cfg(unix)]
fn destination_identity(metadata: &Metadata) -> DomainResult<DestinationIdentity> {
    use std::os::unix::fs::MetadataExt;

    Ok(DestinationIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(windows)]
fn destination_identity(metadata: &Metadata) -> DomainResult<DestinationIdentity> {
    use std::os::windows::fs::MetadataExt;

    let volume_serial_number = metadata.volume_serial_number().ok_or_else(|| {
        DomainError::Internal("Clone destination volume identity is unavailable".into())
    })?;
    let file_index = metadata.file_index().ok_or_else(|| {
        DomainError::Internal("Clone destination file identity is unavailable".into())
    })?;

    Ok(DestinationIdentity {
        volume_serial_number,
        file_index,
    })
}

#[cfg(not(any(unix, windows)))]
fn destination_identity(_metadata: &Metadata) -> DomainResult<DestinationIdentity> {
    Err(DomainError::Internal(
        "Clone destination identity checks are unsupported on this platform".into(),
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};
    use std::thread;

    use tempfile::TempDir;

    use super::*;

    fn destination(root: &TempDir, name: &str) -> PathBuf {
        let path = root.path().join(name);
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn issue_and_consume_returns_canonical_destination() {
        let root = TempDir::new().unwrap();
        let path = destination(&root, "clone-here");
        let store = CloneDestinationCapabilities::new(Duration::from_secs(60), 8);

        let grant = store.issue(&path).unwrap();
        let capability = Uuid::parse_str(&grant.capability).unwrap();

        assert_eq!(capability.get_version(), Some(Version::Random));
        assert_eq!(grant.display_path, path.to_string_lossy());
        assert_eq!(grant.expires_in_seconds, 60);
        assert_eq!(
            store.consume(&grant.capability).unwrap(),
            path.canonicalize().unwrap()
        );
    }

    #[test]
    fn consumed_capability_cannot_be_replayed() {
        let root = TempDir::new().unwrap();
        let path = destination(&root, "clone-here");
        let store = CloneDestinationCapabilities::default();
        let grant = store.issue(&path).unwrap();

        store.consume(&grant.capability).unwrap();
        let replay = store.consume(&grant.capability);

        assert!(matches!(replay, Err(DomainError::InvalidInput(_))));
    }

    #[test]
    fn expired_capability_is_rejected() {
        let root = TempDir::new().unwrap();
        let path = destination(&root, "clone-here");
        let store = CloneDestinationCapabilities::new(Duration::from_millis(10), 8);
        let grant = store.issue(&path).unwrap();

        thread::sleep(Duration::from_millis(40));

        assert!(matches!(
            store.consume(&grant.capability),
            Err(DomainError::InvalidInput(_))
        ));
    }

    #[test]
    fn nonempty_destination_is_rejected_on_issue_and_consume() {
        let root = TempDir::new().unwrap();
        let nonempty_path = destination(&root, "nonempty-at-issue");
        fs::write(nonempty_path.join("existing.txt"), "content").unwrap();
        let store = CloneDestinationCapabilities::default();

        assert!(matches!(
            store.issue(&nonempty_path),
            Err(DomainError::InvalidInput(_))
        ));

        let path = destination(&root, "nonempty-at-consume");
        let grant = store.issue(&path).unwrap();
        fs::write(path.join("appeared.txt"), "content").unwrap();

        assert!(matches!(
            store.consume(&grant.capability),
            Err(DomainError::InvalidInput(_))
        ));
        fs::remove_file(path.join("appeared.txt")).unwrap();
        assert!(store.consume(&grant.capability).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_destination_is_rejected() {
        use std::os::unix::fs::symlink;

        let root = TempDir::new().unwrap();
        let target = destination(&root, "target");
        let link = root.path().join("link");
        symlink(&target, &link).unwrap();
        let store = CloneDestinationCapabilities::default();

        assert!(matches!(
            store.issue(&link),
            Err(DomainError::InvalidInput(_))
        ));
    }

    #[test]
    fn deleted_and_replaced_destination_is_rejected_and_burns_capability() {
        let root = TempDir::new().unwrap();
        let path = destination(&root, "clone-here");
        let store = CloneDestinationCapabilities::default();
        let grant = store.issue(&path).unwrap();

        fs::remove_dir(&path).unwrap();
        fs::create_dir(&path).unwrap();

        assert!(matches!(
            store.consume(&grant.capability),
            Err(DomainError::InvalidInput(_))
        ));
        assert!(store.consume(&grant.capability).is_err());
    }

    #[test]
    fn renamed_destination_is_rejected_even_if_the_original_directory_still_exists() {
        let root = TempDir::new().unwrap();
        let path = destination(&root, "clone-here");
        let moved_path = root.path().join("moved");
        let store = CloneDestinationCapabilities::default();
        let grant = store.issue(&path).unwrap();

        fs::rename(&path, &moved_path).unwrap();

        assert!(matches!(
            store.consume(&grant.capability),
            Err(DomainError::InvalidInput(_))
        ));
    }

    #[test]
    fn max_entries_fails_closed_and_expired_entries_are_pruned() {
        let root = TempDir::new().unwrap();
        let first = destination(&root, "first");
        let second = destination(&root, "second");
        let store = CloneDestinationCapabilities::new(Duration::from_secs(60), 1);
        let first_grant = store.issue(&first).unwrap();

        assert!(matches!(
            store.issue(&second),
            Err(DomainError::Conflict(_))
        ));

        store.consume(&first_grant.capability).unwrap();
        assert!(store.issue(&second).is_ok());

        let expiring_store = CloneDestinationCapabilities::new(Duration::from_millis(10), 1);
        expiring_store.issue(&first).unwrap();
        thread::sleep(Duration::from_millis(40));
        assert!(expiring_store.issue(&second).is_ok());
    }

    #[test]
    fn concurrent_consumers_allow_exactly_one_success() {
        const CONSUMERS: usize = 12;

        let root = TempDir::new().unwrap();
        let path = destination(&root, "clone-here");
        let store = Arc::new(CloneDestinationCapabilities::default());
        let grant = store.issue(&path).unwrap();
        let barrier = Arc::new(Barrier::new(CONSUMERS));
        let mut threads = Vec::new();

        for _ in 0..CONSUMERS {
            let store = Arc::clone(&store);
            let barrier = Arc::clone(&barrier);
            let capability = grant.capability.clone();
            threads.push(thread::spawn(move || {
                barrier.wait();
                store.consume(&capability).is_ok()
            }));
        }

        let successes = threads
            .into_iter()
            .map(|handle| usize::from(handle.join().unwrap()))
            .sum::<usize>();

        assert_eq!(successes, 1);
    }
}
