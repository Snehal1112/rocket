//! Reads the runner's iteration data file from disk.

use std::io::Read;
use std::path::Path;

use rocket_collection::{
    format_for_path, validate_data_file_path, CollectionRepository, DataFileReader, DataFileSource,
    DataFileText, MAX_DATA_FILE_BYTES,
};
use rocket_shared::error::{DomainError, DomainResult};

/// Reads a per-run data file as given, or the collection's saved default.
///
/// It holds the collection repository the runner uses, so a workspace switch cannot make
/// a run read another workspace's file.
pub struct FsDataFileReader {
    collections: Box<dyn CollectionRepository>,
}

impl FsDataFileReader {
    pub fn new(collections: Box<dyn CollectionRepository>) -> Self {
        Self { collections }
    }

    fn read_saved_default(&self, collection: &str, saved: &str) -> DomainResult<DataFileText> {
        validate_data_file_path(saved)?;
        let root = self.collections.collection_root_path(collection)?;
        let canonical_root = root.canonicalize()?;
        let joined = canonical_root.join(saved);
        let canonical = joined.canonicalize().map_err(|e| missing_or_io(saved, e))?;
        if !canonical.starts_with(&canonical_root) {
            return Err(DomainError::InvalidInput(
                "The saved data file must stay inside the collection folder.".into(),
            ));
        }
        let text = read_limited(&canonical, saved)?;
        Ok(DataFileText {
            file_name: saved.to_string(),
            text,
            source: DataFileSource::SavedDefault,
        })
    }
}

impl DataFileReader for FsDataFileReader {
    fn read(
        &self,
        collection: &str,
        per_run_path: Option<&str>,
    ) -> DomainResult<Option<DataFileText>> {
        if let Some(path) = per_run_path.filter(|p| !p.trim().is_empty()) {
            let text = read_limited(Path::new(path), path)?;
            return Ok(Some(DataFileText {
                file_name: path.to_string(),
                text,
                source: DataFileSource::PerRun,
            }));
        }
        let settings = self.collections.get_settings(collection)?;
        match settings.runner_data_file {
            Some(saved) => self.read_saved_default(collection, &saved).map(Some),
            None => Ok(None),
        }
    }
}

fn missing_or_io(shown: &str, err: std::io::Error) -> DomainError {
    if err.kind() == std::io::ErrorKind::NotFound {
        DomainError::NotFound(format!("The data file '{shown}' was not found."))
    } else {
        DomainError::Internal(format!("Could not read the data file '{shown}': {err}"))
    }
}

/// Rejects a wrong extension before any read, then reads at most the limit plus one byte.
///
/// The bounded read also covers a file that reports length 0 (such as a `/proc` entry) or
/// grows after the metadata check.
fn read_limited(path: &Path, shown: &str) -> DomainResult<String> {
    format_for_path(shown)?;
    let meta = std::fs::metadata(path).map_err(|e| missing_or_io(shown, e))?;
    if !meta.is_file() {
        return Err(DomainError::InvalidInput(format!(
            "The data file '{shown}' is not a file."
        )));
    }
    let file = std::fs::File::open(path).map_err(|e| missing_or_io(shown, e))?;
    let mut bytes = Vec::new();
    file.take(MAX_DATA_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| missing_or_io(shown, e))?;
    if bytes.len() > MAX_DATA_FILE_BYTES {
        return Err(DomainError::InvalidInput(format!(
            "The data file '{shown}' is too large."
        )));
    }
    String::from_utf8(bytes).map_err(|_| {
        DomainError::InvalidInput(format!("The data file '{shown}' is not valid UTF-8 text."))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs_collection::FsCollectionRepo;
    use rocket_collection::CollectionSettings;
    use std::fs;
    use tempfile::TempDir;

    /// Builds a workspace with collection "api" and an optional saved default.
    fn setup(saved: Option<&str>) -> (TempDir, FsDataFileReader) {
        let dir = TempDir::new().expect("temp dir");
        let repo = FsCollectionRepo::new_standalone(dir.path().to_path_buf());
        repo.create("api").expect("create collection");
        repo.save_settings(
            "api",
            &CollectionSettings {
                runner_data_file: saved.map(str::to_string),
                ..CollectionSettings::default()
            },
        )
        .expect("save settings");
        (dir, FsDataFileReader::new(Box::new(repo)))
    }

    fn api_dir(dir: &TempDir) -> std::path::PathBuf {
        dir.path().join("api")
    }

    #[test]
    fn per_run_path_is_read_as_given() {
        let (dir, reader) = setup(None);
        let file = dir.path().join("outside.csv");
        fs::write(&file, "a\n1\n").expect("write");
        let path = file.to_string_lossy().to_string();
        let got = reader
            .read("api", Some(&path))
            .expect("read")
            .expect("some");
        assert_eq!(got.text, "a\n1\n");
        assert_eq!(got.source, DataFileSource::PerRun);
        assert_eq!(got.file_name, path);
    }

    #[test]
    fn saved_default_is_read_inside_the_collection() {
        let (dir, reader) = setup(Some("data/users.csv"));
        fs::create_dir_all(api_dir(&dir).join("data")).expect("mkdir");
        fs::write(api_dir(&dir).join("data/users.csv"), "a\n1\n").expect("write");
        let got = reader.read("api", None).expect("read").expect("some");
        assert_eq!(got.text, "a\n1\n");
        assert_eq!(got.source, DataFileSource::SavedDefault);
        assert_eq!(got.file_name, "data/users.csv");
    }

    #[test]
    fn per_run_path_wins_over_the_saved_default() {
        let (dir, reader) = setup(Some("saved.csv"));
        fs::write(api_dir(&dir).join("saved.csv"), "saved\n").expect("write");
        let file = dir.path().join("run.csv");
        fs::write(&file, "run\n").expect("write");
        let path = file.to_string_lossy().to_string();
        let got = reader
            .read("api", Some(&path))
            .expect("read")
            .expect("some");
        assert_eq!(got.source, DataFileSource::PerRun);
    }

    #[test]
    fn saved_default_with_dotdot_is_rejected() {
        let (dir, reader) = setup(Some("../secret.csv"));
        fs::write(dir.path().join("secret.csv"), "a\n1\n").expect("write");
        let err = reader.read("api", None).expect_err("must fail");
        assert!(matches!(err, DomainError::InvalidInput(_)), "{err:?}");
    }

    #[test]
    fn saved_default_absolute_is_rejected() {
        let (dir, reader) = setup(None);
        let file = dir.path().join("abs.csv");
        fs::write(&file, "a\n1\n").expect("write");
        let repo = FsCollectionRepo::new_standalone(dir.path().to_path_buf());
        repo.save_settings(
            "api",
            &CollectionSettings {
                runner_data_file: Some(file.to_string_lossy().to_string()),
                ..CollectionSettings::default()
            },
        )
        .expect("save settings");
        let err = reader.read("api", None).expect_err("must fail");
        assert!(matches!(err, DomainError::InvalidInput(_)), "{err:?}");
    }

    #[cfg(unix)]
    #[test]
    fn saved_default_symlink_out_of_the_collection_is_rejected() {
        let (dir, reader) = setup(Some("link.csv"));
        let outside = dir.path().join("outside.csv");
        fs::write(&outside, "a\n1\n").expect("write");
        std::os::unix::fs::symlink(&outside, api_dir(&dir).join("link.csv")).expect("symlink");
        let err = reader.read("api", None).expect_err("must fail");
        assert!(matches!(err, DomainError::InvalidInput(_)), "{err:?}");
    }

    #[test]
    fn neither_path_set_returns_none() {
        let (_dir, reader) = setup(None);
        assert_eq!(reader.read("api", None).expect("read"), None);
        assert_eq!(reader.read("api", Some("  ")).expect("read"), None);
    }

    #[test]
    fn a_file_over_the_size_limit_is_rejected_before_reading_it_all() {
        let (dir, reader) = setup(Some("big.csv"));
        let file = fs::File::create(api_dir(&dir).join("big.csv")).expect("create");
        file.set_len(MAX_DATA_FILE_BYTES as u64 + 1)
            .expect("set_len");
        let err = reader.read("api", None).expect_err("must fail");
        match err {
            DomainError::InvalidInput(m) => assert!(m.contains("too large"), "{m}"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_file_with_a_wrong_extension_is_rejected_without_reading() {
        let (dir, reader) = setup(None);
        // The file does not exist, so a read would report not found instead.
        let missing = dir.path().join("nope.txt");
        let err = reader.read("api", missing.to_str()).expect_err("must fail");
        assert!(matches!(err, DomainError::InvalidInput(_)), "{err:?}");
    }

    #[test]
    fn a_file_over_the_limit_is_rejected_by_the_bounded_read() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("big.csv");
        fs::write(&path, vec![b'a'; MAX_DATA_FILE_BYTES + 1]).expect("write");
        let err = read_limited(&path, "big.csv").expect_err("must fail");
        assert!(err.to_string().contains("too large"), "{err}");
        fs::write(&path, vec![b'a'; MAX_DATA_FILE_BYTES]).expect("write");
        assert!(read_limited(&path, "big.csv").is_ok());
    }

    #[test]
    fn a_missing_file_names_the_path_in_the_error() {
        let (_dir, reader) = setup(Some("gone.csv"));
        let err = reader.read("api", None).expect_err("must fail");
        assert!(err.to_string().contains("gone.csv"), "{err}");
        let err = reader
            .read("api", Some("/no/such/run.csv"))
            .expect_err("must fail");
        assert!(err.to_string().contains("/no/such/run.csv"), "{err}");
    }
}
