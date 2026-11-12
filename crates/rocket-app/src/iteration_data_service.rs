//! Loads and parses the runner's iteration data. It never reads files itself.

use rocket_collection::{
    format_for_path, parse_iteration_data, DataFileReader, DataFileSource, IterationData,
};
use rocket_scripting::IterationInfo;
use rocket_shared::error::{DomainError, DomainResult};

/// Builds the script-facing info for row `index`. `total` is the row count.
/// An index past the last row gets an empty data map.
pub fn iteration_info(data: &IterationData, index: usize) -> IterationInfo {
    IterationInfo {
        index,
        total: data.rows.len(),
        data: data.rows.get(index).cloned().unwrap_or_default(),
    }
}

/// A parsed data file and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedIterationData {
    pub data: IterationData,
    pub file_name: String,
    pub source: DataFileSource,
}

pub struct IterationDataService {
    reader: Box<dyn DataFileReader>,
}

impl IterationDataService {
    pub fn new(reader: Box<dyn DataFileReader>) -> Self {
        Self { reader }
    }

    /// Reads and parses the data file. The per-run path wins, otherwise the saved default
    /// is used. Returns `None` when neither is set.
    pub fn load(
        &self,
        collection: &str,
        per_run_path: Option<&str>,
    ) -> DomainResult<Option<LoadedIterationData>> {
        let Some(file) = self.reader.read(collection, per_run_path)? else {
            return Ok(None);
        };
        let format = format_for_path(&file.file_name)?;
        let data = parse_iteration_data(format, &file.text).map_err(|e| match e {
            DomainError::InvalidInput(m) => {
                DomainError::InvalidInput(format!("{}: {m}", file.file_name))
            }
            other => other,
        })?;
        Ok(Some(LoadedIterationData {
            data,
            file_name: file.file_name,
            source: file.source,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::DataFileText;
    use std::sync::{Arc, Mutex};

    type Seen = Arc<Mutex<Vec<Option<String>>>>;

    /// Returns a fixed file and records the per-run paths it was asked for.
    struct FakeReader {
        file: Option<DataFileText>,
        seen: Seen,
    }

    impl DataFileReader for FakeReader {
        fn read(
            &self,
            _collection: &str,
            per_run_path: Option<&str>,
        ) -> DomainResult<Option<DataFileText>> {
            self.seen
                .lock()
                .expect("lock")
                .push(per_run_path.map(str::to_string));
            Ok(self.file.clone())
        }
    }

    fn service(file: Option<DataFileText>) -> (IterationDataService, Seen) {
        let seen: Seen = Arc::new(Mutex::new(Vec::new()));
        let reader = FakeReader {
            file,
            seen: Arc::clone(&seen),
        };
        (IterationDataService::new(Box::new(reader)), seen)
    }

    fn text(name: &str, body: &str) -> DataFileText {
        DataFileText {
            file_name: name.to_string(),
            text: body.to_string(),
            source: DataFileSource::PerRun,
        }
    }

    #[test]
    fn per_run_path_is_passed_to_the_reader() {
        let (svc, seen) = service(Some(text("d.csv", "a\n1\n")));
        let loaded = svc.load("api", Some("d.csv")).expect("load").expect("some");
        assert_eq!(loaded.data.rows.len(), 1);
        assert_eq!(loaded.file_name, "d.csv");
        assert_eq!(loaded.source, DataFileSource::PerRun);
        assert_eq!(*seen.lock().expect("lock"), vec![Some("d.csv".to_string())]);
    }

    #[test]
    fn a_reader_returning_none_gives_none() {
        let (svc, _seen) = service(None);
        assert_eq!(svc.load("api", None).expect("load"), None);
    }

    #[test]
    fn a_wrong_extension_is_rejected_after_reading() {
        let (svc, seen) = service(Some(text("d.txt", "a\n1\n")));
        let err = svc.load("api", Some("d.txt")).expect_err("must fail");
        assert!(matches!(err, DomainError::InvalidInput(_)), "{err:?}");
        assert_eq!(seen.lock().expect("lock").len(), 1);
    }

    #[test]
    fn iteration_info_carries_index_total_and_the_row() {
        let (svc, _seen) = service(Some(text("d.csv", "a\n1\n2\n")));
        let data = svc.load("api", None).expect("load").expect("some").data;
        let info = iteration_info(&data, 1);
        assert_eq!(info.index, 1);
        assert_eq!(info.total, 2);
        assert_eq!(info.data.get("a").map(String::as_str), Some("2"));
    }
}
