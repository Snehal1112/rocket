//! Collection export use cases.

use rocket_export::{bruno_zip, CollectionFileSource, ExportReport};
use rocket_shared::error::DomainResult;

/// The bytes of an export, the suggested file name and the report.
#[derive(Debug, Clone)]
pub struct ExportOutput {
    pub bytes: Vec<u8>,
    pub file_name: String,
    pub report: ExportReport,
}

pub struct CollectionExportService {
    files: Box<dyn CollectionFileSource>,
}

fn safe_file_stem(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    if cleaned.trim().is_empty() {
        "collection".to_string()
    } else {
        cleaned
    }
}

impl CollectionExportService {
    pub fn new(files: Box<dyn CollectionFileSource>) -> Self {
        Self { files }
    }

    pub fn export_bruno(
        &self,
        collection: &str,
        include_environments: bool,
    ) -> DomainResult<ExportOutput> {
        let files = self.files.list_files(collection)?;
        let stem = safe_file_stem(collection);
        let (bytes, report) = bruno_zip(files, &stem, include_environments)?;
        Ok(ExportOutput {
            bytes,
            file_name: format!("{stem}.zip"),
            report,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_export::SourceFile;

    struct OneFile;

    impl CollectionFileSource for OneFile {
        fn list_files(&self, _collection: &str) -> DomainResult<Vec<SourceFile>> {
            Ok(vec![SourceFile {
                rel_path: "opencollection.yml".to_string(),
                bytes: b"info:\n  name: my-api\n".to_vec(),
            }])
        }
    }

    #[test]
    fn export_bruno_names_the_file() {
        let service = CollectionExportService::new(Box::new(OneFile));
        let out = service.export_bruno("my-api", true).expect("exports");
        assert_eq!(out.file_name, "my-api.zip");
        assert!(!out.bytes.is_empty());
    }
}
