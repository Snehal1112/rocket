//! Wiring for the import commands: the filesystem pieces `ImportService` needs, built
//! in one place. The factory is managed state and holds the shared active-workspace
//! path, so every import goes to the workspace that is active at that moment.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use rocket_app::network::{CollectionImportPort, ImportedCollection, UNSAFE_ZIP_MESSAGE};
use rocket_collection::{Collection, CollectionRepository};
use rocket_environment::EnvironmentRepository;
use rocket_environment::EnvironmentRepositoryFactory;
use rocket_import::transfer_zip::{self, is_hidden_format_char, is_reserved_device_name};
use rocket_import::{ImportError, ImportService};
use rocket_infra::{FsCollectionRepo, FsEnvironmentRepo, FsImportFiles};
use rocket_shared::error::{DomainError, DomainResult};

/// Makes the environment repository of one collection under a workspace.
pub struct FsEnvFactory(PathBuf);

impl EnvironmentRepositoryFactory for FsEnvFactory {
    fn for_collection(&self, collection_name: &str) -> Box<dyn EnvironmentRepository> {
        Box::new(FsEnvironmentRepo::with_secret_store(
            self.0
                .join("collections")
                .join(collection_name)
                .join("environments"),
            crate::env_secret_store(),
        ))
    }
}

/// Builds `ImportService`s over the active workspace.
pub struct ImportServiceFactory {
    active_workspace_path: Arc<Mutex<PathBuf>>,
}

impl ImportServiceFactory {
    pub fn new(active_workspace_path: Arc<Mutex<PathBuf>>) -> Self {
        Self {
            active_workspace_path,
        }
    }

    /// The active workspace folder right now.
    pub fn workspace_path(&self) -> Result<PathBuf, String> {
        self.active_workspace_path
            .lock()
            .map(|path| path.clone())
            .map_err(|_| "workspace path lock poisoned".to_string())
    }

    /// An import service over the workspace at `base`. Take `base` from `workspace_path`
    /// once per command when the command also needs it, so both agree after a switch.
    pub fn service_for(&self, base: PathBuf) -> ImportService {
        ImportService::new(
            base.clone(),
            Box::new(FsCollectionRepo::new_standalone(base.join("collections"))),
            Box::new(FsEnvFactory(base)),
            std::sync::Arc::new(rocket_infra::FsImportFiles),
        )
    }

    /// An import service over the active workspace.
    pub fn service(&self) -> Result<ImportService, String> {
        Ok(self.service_for(self.workspace_path()?))
    }
}

/// The workspace id handed to `import_auto_from_zip`. The importer ignores it for a
/// collection ZIP: the target workspace is the folder of `ImportServiceFactory`.
const IMPORT_WORKSPACE_ID: &str = "default";

/// The file a live adapter keeps locked inside its private folder.
const LOCK_FILE: &str = "lock";

/// This process's private folder for temp ZIP copies, with its lock held open.
struct ProcessDir {
    path: PathBuf,
    _lock: std::fs::File,
}

/// Imports a staged ZIP from another node into the active workspace.
pub struct ImporterAdapter {
    factory: ImportServiceFactory,
    import_lock: Mutex<()>,
    temp_root: PathBuf,
    process_dir: Mutex<Option<ProcessDir>>,
    started: SystemTime,
}

impl ImporterAdapter {
    /// `active_workspace_path` is the shared path the other import commands use. `temp_root`
    /// is `<network dir>/import-tmp`. Touches no disk; the private folder is made on first use.
    pub fn new(active_workspace_path: Arc<Mutex<PathBuf>>, temp_root: PathBuf) -> Self {
        Self {
            factory: ImportServiceFactory::new(active_workspace_path),
            import_lock: Mutex::new(()),
            temp_root,
            process_dir: Mutex::new(None),
            started: SystemTime::now(),
        }
    }

    /// Test seam: pretends the adapter started at `started`.
    #[cfg(test)]
    fn with_started(mut self, started: SystemTime) -> Self {
        self.started = started;
        self
    }
}

/// A folder for one temp ZIP copy. It is removed when dropped, on every exit path.
struct CopyDir(PathBuf);

impl Drop for CopyDir {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_dir_all(&self.0) {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(error = %e, "could not remove a temp zip copy");
            }
        }
    }
}

/// True for a name that is one plain folder name on every platform. The receiver builds the
/// name with `receiver_display_name`, but the adapter does not trust its caller.
fn is_plain_folder_name(name: &str) -> bool {
    !name.is_empty()
        && name == name.trim()
        && !name.starts_with('.')
        && !name.ends_with('.')
        && !is_reserved_device_name(name)
        && !name.chars().any(|c| {
            c.is_control()
                || is_hidden_format_char(c)
                || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
}

/// Removes the collection folders one failed import made, as `ImportService::created_roots`
/// reports them. Nothing else is touched, so a collection that anything else created in the
/// meantime survives. A root that is not directly inside `collections` is skipped.
fn remove_created_roots(roots: &[PathBuf], collections: &Path) {
    for root in roots {
        if root.parent() != Some(collections) {
            tracing::warn!("a reported collection root outside the collections folder was kept");
            continue;
        }
        match std::fs::remove_dir_all(root) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!(error = %e, "could not remove a half-imported collection"),
        }
    }
}

/// Imports the validated copy with `service` and cleans up after a failure.
fn import_with(
    service: &ImportService,
    copy: &Path,
    collections: &Path,
) -> DomainResult<ImportedCollection> {
    let report = match service.import_auto_from_zip(copy, IMPORT_WORKSPACE_ID, false) {
        Ok(report) => report,
        Err(e) => {
            remove_created_roots(&service.created_roots(), collections);
            return Err(DomainError::Internal(format!(
                "The collection could not be imported: {e}"
            )));
        }
    };
    match report.created_collections.into_iter().next() {
        Some(name) => Ok(ImportedCollection { name }),
        None => {
            remove_created_roots(&service.created_roots(), collections);
            Err(DomainError::Internal(
                "The import created no collection.".into(),
            ))
        }
    }
}

/// Maps a refusal of the safety checks. Only a ZIP the validator refuses gets
/// `UNSAFE_ZIP_MESSAGE` (Ruling 5). A file that cannot be read at all is an I/O error. The
/// reason goes to the log without the payload.
fn map_validation_error(e: ImportError) -> DomainError {
    match e {
        ImportError::IoError(io) => {
            tracing::warn!(error = %io, "a staged zip could not be read");
            DomainError::Io("The staged collection file could not be read.".into())
        }
        other => {
            tracing::warn!(reason = %other, "a transferred zip was rejected");
            DomainError::InvalidInput(UNSAFE_ZIP_MESSAGE.into())
        }
    }
}

/// Only the owner may enter the folder. Other platforms keep their default, which is per user.
#[cfg(unix)]
fn make_private(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn make_private(_dir: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Numbers the per-import copy folders inside this process's private folder.
static COPY_SEQ: AtomicU64 = AtomicU64::new(0);

impl ImporterAdapter {
    fn repo(&self) -> DomainResult<FsCollectionRepo> {
        let base = self
            .factory
            .workspace_path()
            .map_err(DomainError::Internal)?;
        Ok(FsCollectionRepo::new_standalone(base.join("collections")))
    }

    /// This process's private folder, made on first use: a random name under `temp_root`
    /// (mode 0700 on unix) with a `lock` file this adapter keeps locked while it lives.
    fn process_dir(&self) -> DomainResult<PathBuf> {
        let mut slot = self
            .process_dir
            .lock()
            .map_err(|_| DomainError::Internal("The temp folder lock is poisoned.".into()))?;
        if let Some(dir) = slot.as_ref() {
            return Ok(dir.path.clone());
        }
        std::fs::create_dir_all(&self.temp_root)?;
        make_private(&self.temp_root)?;
        let path = tempfile::Builder::new()
            .prefix(&format!("p{}-", std::process::id()))
            .tempdir_in(&self.temp_root)?
            .keep();
        make_private(&path)?;
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.join(LOCK_FILE))?;
        lock.try_lock()
            .map_err(|e| DomainError::Io(format!("The temp folder could not be locked: {e}")))?;
        *slot = Some(ProcessDir {
            path: path.clone(),
            _lock: lock,
        });
        Ok(path)
    }

    fn new_copy_dir(&self) -> DomainResult<CopyDir> {
        let n = COPY_SEQ.fetch_add(1, Ordering::SeqCst);
        let dir = self.process_dir()?.join(n.to_string());
        std::fs::create_dir(&dir)?;
        Ok(CopyDir(dir))
    }

    /// Copies the staged file into the private folder as `<display_name>.zip` (the importer
    /// names the collection from the stem) and runs the strict checks on that copy, which is
    /// the file the importer reads. The copy goes when the returned guard is dropped.
    fn prepare_copy(
        &self,
        zip_path: &Path,
        display_name: &str,
    ) -> DomainResult<(CopyDir, PathBuf)> {
        let copy_dir = self.new_copy_dir()?;
        let copy = copy_dir.0.join(format!("{display_name}.zip"));
        std::fs::copy(zip_path, &copy).map_err(|e| {
            tracing::warn!(error = %e, "a staged zip could not be copied");
            DomainError::Io("The staged collection file could not be read.".into())
        })?;
        let files = FsImportFiles;
        transfer_zip::validate(&files, &copy)
            .and_then(|()| transfer_zip::require_collection_root(&files, &copy))
            .map_err(map_validation_error)?;
        Ok((copy_dir, copy))
    }

    /// True when `dir` belongs to an instance that is gone: its `lock` can be locked, or it has
    /// no `lock` and is older than this adapter's start.
    fn is_stale(&self, dir: &Path) -> DomainResult<bool> {
        let opened = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(dir.join(LOCK_FILE));
        match opened {
            Ok(file) => match file.try_lock() {
                Ok(()) => {
                    // Released before the folder is removed, which Windows needs.
                    drop(file);
                    Ok(true)
                }
                Err(std::fs::TryLockError::WouldBlock) => Ok(false),
                Err(std::fs::TryLockError::Error(e)) => Err(e.into()),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let modified = std::fs::metadata(dir)?.modified()?;
                Ok(modified < self.started)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Removes the private folders of instances that are gone, for example after a crash.
    /// Never this adapter's own folder and never a live instance's. A missing root is fine.
    /// Call it at app start, before the first import.
    pub fn purge_temp(&self) -> DomainResult<()> {
        let own = match self.process_dir.lock() {
            Ok(slot) => slot.as_ref().map(|d| d.path.clone()),
            Err(_) => {
                return Err(DomainError::Internal(
                    "The temp folder lock is poisoned.".into(),
                ))
            }
        };
        let entries = match std::fs::read_dir(&self.temp_root) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if own.as_ref() == Some(&path) || !entry.file_type()?.is_dir() {
                continue;
            }
            if self.is_stale(&path)? {
                match std::fs::remove_dir_all(&path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => tracing::warn!(error = %e, "could not remove a stale temp folder"),
                }
            }
        }
        Ok(())
    }
}

impl CollectionImportPort for ImporterAdapter {
    fn existing_names(&self) -> DomainResult<Vec<String>> {
        Ok(self.repo()?.list()?.into_iter().map(|s| s.name).collect())
    }

    fn import_zip(&self, zip_path: &Path, display_name: &str) -> DomainResult<ImportedCollection> {
        if !is_plain_folder_name(display_name) || Collection::validate_name(display_name).is_err() {
            return Err(DomainError::InvalidInput(
                "The collection name is not valid.".into(),
            ));
        }
        // The guard removes the copy on every exit path below.
        let (_copy_dir, copy) = self.prepare_copy(zip_path, display_name)?;

        // One import at a time: the importer resolves a free name and then creates it, which
        // is not atomic. Two imports of the same name would otherwise race.
        let _guard = self
            .import_lock
            .lock()
            .map_err(|_| DomainError::Internal("The import lock is poisoned.".into()))?;
        // One read of the workspace path, so the service and the cleanup agree after a switch.
        let base = self
            .factory
            .workspace_path()
            .map_err(DomainError::Internal)?;
        let service = self.factory.service_for(base.clone());
        import_with(&service, &copy, &base.join("collections"))
    }
}

/// Startup cleanup for the team network send: staged chunk files and the adapter's stale
/// temp dirs. A failure is logged and the start goes on.
pub(crate) fn purge_transfer_temp(
    staging: &dyn rocket_app::TransferStaging,
    importer: &ImporterAdapter,
) {
    if let Err(err) = staging.purge() {
        tracing::warn!(code = "transfer_staging_purge_failed", error = %err, "startup");
    }
    if let Err(err) = importer.purge_temp() {
        tracing::warn!(code = "transfer_temp_purge_failed", error = %err, "startup");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_startup_purge_clears_staging_and_stale_temp_dirs() {
        let root = tempfile::TempDir::new().expect("tempdir");
        let staging = rocket_infra::FsTransferStaging::new(root.path().join("transfers"));
        let mut left =
            rocket_app::TransferStaging::create(&staging, "0123456789abcdef0123456789abcdef")
                .expect("staged file");
        left.append(b"half a zip").expect("append");
        // A crash never runs finish or discard.
        std::mem::forget(left);

        // The temp dir of an instance that exited: its lock file exists and nobody holds it.
        let temp_root = root.path().join("import-tmp");
        let stale = temp_root.join("4242-stale");
        std::fs::create_dir_all(&stale).expect("stale dir");
        std::fs::write(stale.join("lock"), b"").expect("stale lock file");
        std::fs::write(stale.join("Orders.zip"), b"old copy").expect("stale copy");
        let ws = tempfile::TempDir::new().expect("workspace");
        let adapter =
            ImporterAdapter::new(Arc::new(Mutex::new(ws.path().to_path_buf())), temp_root);

        purge_transfer_temp(&staging, &adapter);

        let staged_left = std::fs::read_dir(root.path().join("transfers"))
            .map(|entries| entries.count())
            .unwrap_or(0);
        assert_eq!(staged_left, 0, "no staged file survives the start");
        assert!(
            !stale.exists(),
            "the temp dir of an exited instance is removed"
        );
    }

    #[test]
    fn workspace_path_follows_the_shared_path() {
        let shared = Arc::new(Mutex::new(PathBuf::from("/ws/a")));
        let factory = ImportServiceFactory::new(Arc::clone(&shared));
        assert_eq!(
            factory.workspace_path().expect("path"),
            PathBuf::from("/ws/a")
        );

        *shared.lock().expect("lock") = PathBuf::from("/ws/b");

        assert_eq!(
            factory.workspace_path().expect("path"),
            PathBuf::from("/ws/b")
        );
    }

    #[test]
    fn poisoned_lock_is_an_error_not_a_panic() {
        let shared = Arc::new(Mutex::new(PathBuf::from("/ws/a")));
        let poisoner = Arc::clone(&shared);
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.lock().expect("lock");
            panic!("poison the lock");
        })
        .join();
        let factory = ImportServiceFactory::new(shared);

        assert_eq!(
            factory.workspace_path().err().as_deref(),
            Some("workspace path lock poisoned")
        );
        assert!(factory.service().is_err());
    }

    #[test]
    fn env_factory_points_at_the_collection_environments_folder() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let factory = FsEnvFactory(tmp.path().to_path_buf());
        let repo = factory.for_collection("My API");
        // A new, empty collection has no environments yet.
        assert!(repo.list().expect("list").is_empty());
    }

    // ---- ImporterAdapter ----

    use std::io::{self, Read, Write};
    use std::time::Duration;

    use rocket_app::effective_capabilities;
    use rocket_collection::settings::SandboxMode;
    use rocket_collection::CollectionTrustStore;
    use rocket_import::{DirEntryInfo, EntryKind, ImportFiles, ReadSeek, WorkDir};
    use rocket_infra::FsCollectionTrustStore;
    use zip::write::SimpleFileOptions;

    const HOUR: Duration = Duration::from_secs(3600);

    fn shared(ws: &Path) -> Arc<Mutex<PathBuf>> {
        Arc::new(Mutex::new(ws.to_path_buf()))
    }

    fn adapter() -> (tempfile::TempDir, ImporterAdapter) {
        let ws = tempfile::TempDir::new().expect("tempdir");
        std::fs::create_dir_all(ws.path().join("collections")).expect("collections dir");
        // The temp copies go inside the test workspace, outside `collections`.
        let adapter = ImporterAdapter::new(shared(ws.path()), ws.path().join("transfer-tmp"));
        (ws, adapter)
    }

    /// The ZIP copies left in any per-process folder under the temp root. The `lock` file and
    /// the empty process folders do not count.
    fn temp_leftovers(ws: &Path) -> Vec<String> {
        let Ok(dirs) = std::fs::read_dir(ws.join("transfer-tmp")) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for dir in dirs.filter_map(|e| e.ok()) {
            let Ok(entries) = std::fs::read_dir(dir.path()) else {
                continue;
            };
            for entry in entries.filter_map(|e| e.ok()) {
                let name = entry.file_name().to_string_lossy().to_string();
                if name != LOCK_FILE {
                    out.push(name);
                }
            }
        }
        out
    }

    /// Runs an import that passes the safety checks and still fails inside the importer: a
    /// plain FILE called `Blocked` sits where the collection folder must go, and `list()` does
    /// not show it, so the importer picks the name `Blocked` and `create` refuses it.
    fn failing_import(ws: &Path, adapter: &ImporterAdapter) -> DomainError {
        std::fs::write(
            ws.join("collections").join("Blocked"),
            b"a file, not a folder",
        )
        .expect("blocker");
        let zips = tempfile::TempDir::new().expect("tempdir");
        let zip = good_zip(zips.path(), "a.zip");
        adapter
            .import_zip(&zip, "Blocked")
            .expect_err("the import must fail")
    }

    fn write_zip(path: &Path, entries: &[(&str, &str)]) {
        let file = std::fs::File::create(path).expect("create zip");
        let mut writer = zip::ZipWriter::new(file);
        for (name, text) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .expect("start file");
            writer.write_all(text.as_bytes()).expect("write entry");
        }
        writer.finish().expect("finish zip");
    }

    /// A good wrapped collection whose `info.name` differs from any display name.
    fn good_zip(dir: &Path, file: &str) -> PathBuf {
        let path = dir.join(file);
        write_zip(
            &path,
            &[
                (
                    "Inner Name/opencollection.yml",
                    "opencollection: \"1.0.0\"\ninfo:\n  name: Inner Name\n",
                ),
                (
                    "Inner Name/get.yml",
                    "info:\n  name: get\nhttp:\n  method: GET\n  url: https://example.test\n",
                ),
            ],
        );
        path
    }

    fn collection_dirs(ws: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(ws.join("collections"))
            .expect("read collections")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        names
    }

    fn unsafe_message(err: &DomainError) -> bool {
        matches!(err, DomainError::InvalidInput(text) if text == UNSAFE_ZIP_MESSAGE)
    }

    /// The real port, except that making a folder inside `collections/` fails. Before it fails
    /// it creates the collection `Other`, as another import command or a git pull might in the
    /// middle of this import.
    struct ConcurrentCreatorFiles {
        collections: PathBuf,
    }

    impl ImportFiles for ConcurrentCreatorFiles {
        fn list_dir(&self, dir: &Path) -> io::Result<Vec<DirEntryInfo>> {
            FsImportFiles.list_dir(dir)
        }
        fn kind(&self, path: &Path) -> io::Result<EntryKind> {
            FsImportFiles.kind(path)
        }
        fn create_dir_all(&self, dir: &Path) -> io::Result<()> {
            if dir.starts_with(&self.collections) {
                FsCollectionRepo::new_standalone(self.collections.clone())
                    .create("Other")
                    .map_err(|e| io::Error::other(e.to_string()))?;
                return Err(io::Error::other("disk full"));
            }
            FsImportFiles.create_dir_all(dir)
        }
        fn copy(&self, src: &Path, dest: &Path) -> io::Result<()> {
            FsImportFiles.copy(src, dest)
        }
        fn read_to_string(&self, path: &Path) -> io::Result<String> {
            FsImportFiles.read_to_string(path)
        }
        fn write(&self, path: &Path, text: &str) -> io::Result<()> {
            FsImportFiles.write(path, text)
        }
        fn regular_file_len(&self, path: &Path) -> io::Result<Option<u64>> {
            FsImportFiles.regular_file_len(path)
        }
        fn read_up_to(&self, path: &Path, max_bytes: u64) -> io::Result<Vec<u8>> {
            FsImportFiles.read_up_to(path, max_bytes)
        }
        fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
            FsImportFiles.canonicalize(path)
        }
        fn open_read(&self, path: &Path) -> io::Result<Box<dyn ReadSeek>> {
            FsImportFiles.open_read(path)
        }
        fn temp_dir(&self) -> io::Result<Box<dyn WorkDir>> {
            FsImportFiles.temp_dir()
        }
        fn write_stream(&self, path: &Path, from: &mut dyn Read) -> io::Result<u64> {
            FsImportFiles.write_stream(path, from)
        }
    }

    #[test]
    fn existing_names_lists_the_collections_of_the_active_workspace() {
        let (ws, adapter) = adapter();
        assert!(adapter.existing_names().expect("names").is_empty());
        FsCollectionRepo::new_standalone(ws.path().join("collections"))
            .create("Billing")
            .expect("create");
        assert_eq!(
            adapter.existing_names().expect("names"),
            vec!["Billing".to_string()]
        );
    }

    #[test]
    fn existing_names_follow_a_workspace_switch() {
        let (ws_a, _) = adapter();
        let ws_b = tempfile::TempDir::new().expect("tempdir");
        std::fs::create_dir_all(ws_b.path().join("collections")).expect("dir");
        FsCollectionRepo::new_standalone(ws_b.path().join("collections"))
            .create("OnlyInB")
            .expect("create");
        let shared = shared(ws_a.path());
        let adapter = ImporterAdapter::new(Arc::clone(&shared), ws_a.path().join("transfer-tmp"));
        assert!(adapter.existing_names().expect("names").is_empty());

        *shared.lock().expect("lock") = ws_b.path().to_path_buf();

        assert_eq!(
            adapter.existing_names().expect("names"),
            vec!["OnlyInB".to_string()]
        );
    }

    #[test]
    fn import_zip_names_the_collection_from_the_display_name() {
        let (ws, adapter) = adapter();
        let zips = tempfile::TempDir::new().expect("tempdir");
        let zip = good_zip(zips.path(), "whatever-the-staged-file-is-called.zip");

        let imported = adapter
            .import_zip(&zip, "Orders (from Ana)")
            .expect("import");

        assert_eq!(imported.name, "Orders (from Ana)");
        assert_eq!(
            collection_dirs(ws.path()),
            vec!["Orders (from Ana)".to_string()]
        );
        assert!(ws
            .path()
            .join("collections/Orders (from Ana)/get.yml")
            .exists());
        assert!(zip.exists(), "the staged file is the caller's to delete");
    }

    #[test]
    fn import_zip_adds_a_suffix_on_a_clash_and_never_overwrites() {
        let (ws, adapter) = adapter();
        let zips = tempfile::TempDir::new().expect("tempdir");
        let zip = good_zip(zips.path(), "a.zip");
        adapter.import_zip(&zip, "Orders").expect("first");
        let marker = ws.path().join("collections/Orders/keep.txt");
        std::fs::write(&marker, "mine").expect("marker");

        let second = adapter.import_zip(&zip, "Orders").expect("second");

        assert_eq!(second.name, "Orders-1");
        assert_eq!(std::fs::read_to_string(&marker).expect("marker"), "mine");
    }

    #[test]
    fn import_zip_keeps_a_non_ascii_name() {
        let (ws, adapter) = adapter();
        let zips = tempfile::TempDir::new().expect("tempdir");
        let zip = good_zip(zips.path(), "a.zip");

        let imported = adapter
            .import_zip(&zip, "Café – Ünï 日本語")
            .expect("import");

        assert_eq!(imported.name, "Café – Ünï 日本語");
        assert!(ws
            .path()
            .join("collections/Café – Ünï 日本語/opencollection.yml")
            .exists());
        assert!(adapter
            .existing_names()
            .expect("names")
            .contains(&"Café – Ünï 日本語".to_string()));
    }

    #[test]
    fn import_zip_imports_a_flat_root_zip() {
        let (_ws, adapter) = adapter();
        let zips = tempfile::TempDir::new().expect("tempdir");
        let zip = zips.path().join("flat.zip");
        write_zip(
            &zip,
            &[(
                "opencollection.yml",
                "opencollection: \"1.0.0\"\ninfo:\n  name: x\n",
            )],
        );
        let imported = adapter.import_zip(&zip, "Flat").expect("import");
        assert_eq!(imported.name, "Flat");
    }

    #[test]
    fn import_zip_refuses_a_single_file_zip_without_a_marker() {
        let (ws, adapter) = adapter();
        let zips = tempfile::TempDir::new().expect("tempdir");
        let zip = zips.path().join("notes.zip");
        write_zip(&zip, &[("notes.txt", "hello")]);

        let err = adapter.import_zip(&zip, "Notes").expect_err("refused");

        assert!(unsafe_message(&err), "error was {err:?}");
        assert!(collection_dirs(ws.path()).is_empty());
    }

    #[test]
    fn import_zip_refuses_an_unsafe_zip_and_writes_nothing() {
        let (ws, adapter) = adapter();
        let zips = tempfile::TempDir::new().expect("tempdir");
        let slip = zips.path().join("slip.zip");
        write_zip(
            &slip,
            &[
                ("c/opencollection.yml", "info:\n  name: x\n"),
                ("../../evil.txt", "pwned"),
            ],
        );
        let not_a_zip = zips.path().join("junk.zip");
        std::fs::write(&not_a_zip, b"junk").expect("junk");
        let workspace_zip = zips.path().join("ws.zip");
        write_zip(
            &workspace_zip,
            &[
                ("w/workspace.yml", "opencollection: \"1.0.0\"\n"),
                ("w/opencollection.yml", "info:\n  name: x\n"),
            ],
        );

        for bad in [&slip, &not_a_zip, &workspace_zip] {
            let err = adapter.import_zip(bad, "Evil").expect_err("refused");
            assert!(unsafe_message(&err), "error for {bad:?} was {err:?}");
        }

        assert!(collection_dirs(ws.path()).is_empty());
        assert!(!ws.path().join("evil.txt").exists());
        assert!(!ws
            .path()
            .parent()
            .expect("parent")
            .join("evil.txt")
            .exists());
    }

    #[test]
    fn import_zip_refuses_names_that_are_not_a_plain_folder_name() {
        let (ws, adapter) = adapter();
        let zips = tempfile::TempDir::new().expect("tempdir");
        let zip = good_zip(zips.path(), "a.zip");
        let bad_names = [
            "a/b",
            "a\\b",
            "..",
            ".hidden",
            "",
            "   ",
            "x\0y",
            "Orders.",
            "Orders ",
            " Orders",
            "a:b",
            "tab\there",
            "CON",
            "nul.txt",
            "a\u{202E}b",
            "a\u{200B}b",
        ];
        for name in bad_names {
            let err = adapter.import_zip(&zip, name).expect_err("refused");
            assert!(
                matches!(err, DomainError::InvalidInput(_)),
                "name {name:?} gave {err:?}"
            );
        }
        assert!(collection_dirs(ws.path()).is_empty());
    }

    #[test]
    fn two_imports_of_the_same_name_make_two_collections() {
        let (ws, adapter) = adapter();
        let adapter = Arc::new(adapter);
        let zips = tempfile::TempDir::new().expect("tempdir");
        let first = good_zip(zips.path(), "one.zip");
        let second = good_zip(zips.path(), "two.zip");

        let handles: Vec<_> = [first, second]
            .into_iter()
            .map(|zip| {
                let adapter = Arc::clone(&adapter);
                std::thread::spawn(move || adapter.import_zip(&zip, "Race"))
            })
            .collect();
        let mut names: Vec<String> = handles
            .into_iter()
            .map(|h| h.join().expect("thread").expect("import").name)
            .collect();
        names.sort();

        assert_eq!(names, vec!["Race".to_string(), "Race-1".to_string()]);
        assert_eq!(
            collection_dirs(ws.path()),
            vec!["Race".to_string(), "Race-1".to_string()]
        );
    }

    #[test]
    fn remove_created_roots_removes_only_the_listed_folders_inside_collections() {
        let (ws, _adapter) = adapter();
        let collections = ws.path().join("collections");
        let repo = FsCollectionRepo::new_standalone(collections.clone());
        repo.create("Made by the import").expect("listed");
        repo.create("Someone else's").expect("not listed");
        // A listed path outside `collections/` is never removed, whatever the list says.
        let outside = ws.path().join("outside");
        std::fs::create_dir_all(&outside).expect("outside dir");
        let missing = collections.join("Already gone");

        remove_created_roots(
            &[
                collections.join("Made by the import"),
                outside.clone(),
                missing,
            ],
            &collections,
        );

        assert_eq!(
            collection_dirs(ws.path()),
            vec!["Someone else's".to_string()]
        );
        assert!(outside.is_dir());
    }

    #[test]
    fn a_collection_created_during_a_failed_import_survives() {
        let (ws, adapter) = adapter();
        let collections = ws.path().join("collections");
        let zips = tempfile::TempDir::new().expect("tempdir");
        let zip = zips.path().join("a.zip");
        write_zip(
            &zip,
            &[
                (
                    "c/opencollection.yml",
                    "opencollection: \"1.0.0\"\ninfo:\n  name: c\n",
                ),
                (
                    "c/sub/get.yml",
                    "info:\n  name: get\nhttp:\n  method: GET\n  url: https://x.test\n",
                ),
            ],
        );
        let (_copy_dir, copy) = adapter.prepare_copy(&zip, "Pushed").expect("copy");
        let service = ImportService::new(
            ws.path().to_path_buf(),
            Box::new(FsCollectionRepo::new_standalone(collections.clone())),
            Box::new(FsEnvFactory(ws.path().to_path_buf())),
            Arc::new(ConcurrentCreatorFiles {
                collections: collections.clone(),
            }),
        );

        let err = import_with(&service, &copy, &collections).expect_err("the import fails");

        assert!(
            matches!(&err, DomainError::Internal(text) if text.starts_with("The collection could not be imported")),
            "error was {err:?}"
        );
        assert_eq!(service.created_roots(), vec![collections.join("Pushed")]);
        // The import's own folder goes; the collection made during the import stays.
        assert_eq!(collection_dirs(ws.path()), vec!["Other".to_string()]);
    }

    #[test]
    fn a_failed_import_returns_an_import_error_and_leaves_no_new_collection() {
        let (ws, adapter) = adapter();
        let repo = FsCollectionRepo::new_standalone(ws.path().join("collections"));
        repo.create("Keep").expect("older collection");

        let err = failing_import(ws.path(), &adapter);

        assert!(
            matches!(&err, DomainError::Internal(text) if text.starts_with("The collection could not be imported")),
            "error was {err:?}"
        );
        assert_eq!(
            adapter.existing_names().expect("names"),
            vec!["Keep".to_string()]
        );
        // The blocker file is the test's own and is not a collection.
        assert!(ws.path().join("collections/Blocked").is_file());
    }

    #[test]
    fn the_unsafe_message_is_used_only_for_validator_refusals() {
        let (ws, adapter) = adapter();
        let zips = tempfile::TempDir::new().expect("tempdir");
        let good = good_zip(zips.path(), "a.zip");

        // A bad name, a failed import and a missing staged file are not "unsafe".
        let bad_name = adapter.import_zip(&good, "a/b").expect_err("bad name");
        assert!(!unsafe_message(&bad_name), "got {bad_name:?}");
        assert!(matches!(bad_name, DomainError::InvalidInput(_)));
        let failed = failing_import(ws.path(), &adapter);
        assert!(!unsafe_message(&failed), "got {failed:?}");
        let missing = adapter
            .import_zip(&zips.path().join("missing.zip"), "Orders")
            .expect_err("missing file");
        assert!(!unsafe_message(&missing), "got {missing:?}");

        // A validator refusal is.
        let junk = zips.path().join("junk.zip");
        std::fs::write(&junk, b"junk").expect("junk");
        let refused = adapter.import_zip(&junk, "Orders").expect_err("refused");
        assert!(unsafe_message(&refused), "got {refused:?}");
    }

    #[test]
    fn a_missing_staged_file_is_an_io_error_not_the_unsafe_message() {
        let (ws, adapter) = adapter();
        let zips = tempfile::TempDir::new().expect("tempdir");

        let err = adapter
            .import_zip(&zips.path().join("missing.zip"), "Orders")
            .expect_err("missing file");

        assert!(matches!(err, DomainError::Io(_)), "error was {err:?}");
        assert!(collection_dirs(ws.path()).is_empty());
    }

    #[test]
    fn the_validated_copy_is_the_file_that_is_imported() {
        let (ws, adapter) = adapter();
        let zips = tempfile::TempDir::new().expect("tempdir");
        let zip = good_zip(zips.path(), "staged.zip");

        let (copy_dir, copy) = adapter.prepare_copy(&zip, "Pushed").expect("copy");
        let private = adapter.process_dir().expect("process dir");
        assert!(copy.starts_with(&private), "{copy:?} is not in {private:?}");
        assert_eq!(copy.file_name().expect("name"), "Pushed.zip");

        // Spoiling the staged original after the check changes nothing: the importer reads the
        // copy that was validated.
        std::fs::write(&zip, b"junk").expect("spoil the original");
        let base = ws.path().to_path_buf();
        let service = adapter.factory.service_for(base.clone());
        let imported = import_with(&service, &copy, &base.join("collections")).expect("import");
        assert_eq!(imported.name, "Pushed");

        drop(copy_dir);
        assert!(temp_leftovers(ws.path()).is_empty());

        // And a copy of an unsafe ZIP is refused and removed.
        let err = adapter.prepare_copy(&zip, "Junk").err().expect("refused");
        assert!(unsafe_message(&err), "got {err:?}");
        assert!(temp_leftovers(ws.path()).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn the_private_temp_folders_are_mode_0700() {
        use std::os::unix::fs::PermissionsExt;

        let (ws, adapter) = adapter();
        let private = adapter.process_dir().expect("process dir");
        for dir in [ws.path().join("transfer-tmp"), private] {
            let mode = std::fs::metadata(&dir).expect("meta").permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "{dir:?} has mode {mode:o}");
        }
    }

    #[test]
    fn the_temp_copy_is_removed_on_success_and_on_failure() {
        let (ws, adapter) = adapter();
        let zips = tempfile::TempDir::new().expect("tempdir");
        let zip = good_zip(zips.path(), "a.zip");

        adapter.import_zip(&zip, "Orders").expect("import");
        assert!(temp_leftovers(ws.path()).is_empty(), "left after success");

        let _ = failing_import(ws.path(), &adapter);
        assert!(temp_leftovers(ws.path()).is_empty(), "left after failure");

        let junk = zips.path().join("junk.zip");
        std::fs::write(&junk, b"junk").expect("junk");
        let _ = adapter.import_zip(&junk, "Orders");
        assert!(temp_leftovers(ws.path()).is_empty(), "left after a refusal");
    }

    #[test]
    fn purge_temp_is_fine_with_a_missing_root() {
        let (_ws, adapter) = adapter();
        adapter
            .purge_temp()
            .expect("a missing folder is not an error");
    }

    #[test]
    fn purge_temp_keeps_the_folder_of_a_live_adapter() {
        let (ws, first) = adapter();
        let live = first.process_dir().expect("process dir");
        std::fs::write(live.join("in-flight.zip"), b"zip").expect("in-flight copy");
        // A second instance on the same root that started long after the first.
        let second = ImporterAdapter::new(shared(ws.path()), ws.path().join("transfer-tmp"))
            .with_started(SystemTime::now() + HOUR);

        second.purge_temp().expect("purge");
        first.purge_temp().expect("purge by the owner");

        assert!(
            live.join("in-flight.zip").exists(),
            "a live instance's copy was deleted"
        );
        let zips = tempfile::TempDir::new().expect("tempdir");
        first
            .import_zip(&good_zip(zips.path(), "a.zip"), "Orders")
            .expect("still imports");
    }

    #[test]
    fn purge_temp_removes_a_folder_whose_lock_is_free() {
        let (ws, adapter) = adapter();
        // What a process that exited leaves behind: its lock file is no longer held.
        let stale = ws.path().join("transfer-tmp/p4242-old");
        std::fs::create_dir_all(&stale).expect("stale dir");
        std::fs::write(stale.join(LOCK_FILE), b"").expect("lock file");
        std::fs::write(stale.join("Orders.zip"), b"zip").expect("stale copy");

        adapter.purge_temp().expect("purge");

        assert!(!stale.exists());
    }

    #[test]
    fn purge_temp_removes_an_old_folder_without_a_lock_and_keeps_a_young_one() {
        let (ws, _adapter) = adapter();
        let lockless = ws.path().join("transfer-tmp/p4243-nolock");
        std::fs::create_dir_all(&lockless).expect("dir");

        // Started before the folder was made: the folder may belong to a live instance that has
        // not written its lock yet, so it stays.
        ImporterAdapter::new(shared(ws.path()), ws.path().join("transfer-tmp"))
            .with_started(SystemTime::now() - HOUR)
            .purge_temp()
            .expect("purge");
        assert!(lockless.exists(), "a young folder without a lock must stay");

        // Started after the folder was made: it is a leftover.
        ImporterAdapter::new(shared(ws.path()), ws.path().join("transfer-tmp"))
            .with_started(SystemTime::now() + HOUR)
            .purge_temp()
            .expect("purge");
        assert!(!lockless.exists(), "an old folder without a lock must go");
    }

    #[test]
    fn an_imported_collection_has_no_trust_record_and_runs_only_in_safe_mode() {
        // No script gate exists (second-review R1): the scripts of a received collection run in
        // the Safe sandbox. What the trust gate denies is Developer mode, extra script folders,
        // agent runs, host env and vault access, until the user allows them.
        let (ws, adapter) = adapter();
        let zips = tempfile::TempDir::new().expect("tempdir");
        // A pushed collection that asks for developer mode with the real key.
        let zip = zips.path().join("a.zip");
        write_zip(
            &zip,
            &[
                (
                    "c/opencollection.yml",
                    "opencollection: \"1.0.0\"\ninfo:\n  name: c\nextensions:\n  rocketapi:\n    sandboxMode: developer\n",
                ),
                ("c/get.yml", "info:\n  name: get\nhttp:\n  method: GET\n  url: https://x.test\n"),
            ],
        );

        let imported = adapter.import_zip(&zip, "Pushed").expect("import");

        let repo = FsCollectionRepo::new_standalone(ws.path().join("collections"));
        // The request reached the stored settings, so the Safe check below can fail.
        let stored = repo.get_settings(&imported.name).expect("settings");
        assert_eq!(
            stored.sandbox_mode,
            SandboxMode::Developer,
            "the fixture must store developer mode, or the Safe assertion proves nothing"
        );

        let store = FsCollectionTrustStore::new(ws.path().join("trust.yml"));
        let identity = repo.collection_identity(&imported.name).expect("identity");
        assert_eq!(
            store.grant_for(&identity).expect("read"),
            None,
            "no trust record"
        );

        let effective = effective_capabilities(&repo, &store, &imported.name);
        assert!(!effective.vault_values && !effective.vault_certificates);
        assert!(!effective.process_env);
        assert!(!effective.agent_run);
        assert!(effective.context_roots.is_empty());
        assert_eq!(
            effective.sandbox_mode,
            SandboxMode::Safe,
            "the requested developer mode stays denied until the user trusts the collection"
        );
    }

    #[test]
    fn a_received_zip_asking_for_developer_mode_gets_no_trust_record() {
        // The received case end to end: the sender suffix in the name, a script entry and a
        // hostile `opencollection.yml` that asks for Developer mode.
        let (ws, adapter) = adapter();
        let zips = tempfile::TempDir::new().expect("tempdir");
        let zip = zips.path().join("Orders.zip");
        write_zip(
            &zip,
            &[
                (
                    "Orders/opencollection.yml",
                    "opencollection: \"1.0.0\"\ninfo:\n  name: Orders\nextensions:\n  rocketapi:\n    sandboxMode: developer\n",
                ),
                (
                    "Orders/users/get.yml",
                    "info:\n  name: get\nhttp:\n  method: GET\n  url: https://x.test\nruntime:\n  scripts:\n  - type: before-request\n    code: \"rok.setVar('a', 1)\"\n",
                ),
            ],
        );

        let imported = adapter
            .import_zip(&zip, "Orders (from ann)")
            .expect("the import succeeds");
        assert_eq!(imported.name, "Orders (from ann)");

        // The requested mode reached the disk, so the Safe assert below can fail.
        let repo = FsCollectionRepo::new_standalone(ws.path().join("collections"));
        let requested = repo.get_settings(&imported.name).expect("settings");
        assert_eq!(
            requested.sandbox_mode,
            SandboxMode::Developer,
            "the ZIP asked for it"
        );

        let trust_file = ws.path().join("trust.yml");
        let store = FsCollectionTrustStore::new(trust_file.clone());
        let identity = repo.collection_identity(&imported.name).expect("identity");
        assert_eq!(
            store.grant_for(&identity).expect("read"),
            None,
            "no trust record"
        );
        let effective = effective_capabilities(&repo, &store, &imported.name);
        assert_eq!(effective.sandbox_mode, SandboxMode::Safe);
        assert!(!effective.process_env && !effective.agent_run);
        assert!(!effective.vault_values && !effective.vault_certificates);
        assert!(!trust_file.exists(), "importing never writes a trust grant");
    }
}
