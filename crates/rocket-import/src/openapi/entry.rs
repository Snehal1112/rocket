//! Import an OpenAPI spec from a file on disk or from a link.

use crate::error::{ImportError, ImportResult};
use crate::importer::ImportService;
use crate::openapi::{fetch_target, parse_spec_url, source_record, GroupBy};
use crate::report::ImportReport;
use rocket_collection::openapi_source::{OpenApiSourceRepository, SpecLocation};
use rocket_http::spec_fetch::{
    SpecFetchFailure, SpecFetchOutcome, SpecFetchRequest, SpecFetcher, DEFAULT_MAX_SPEC_BYTES,
};
use std::path::Path;

/// What an OpenAPI import from a file or a link gives back.
#[derive(Debug, Clone)]
pub struct OpenApiImportResult {
    pub report: ImportReport,
    /// The name of the collection that was created.
    pub collection: String,
    /// The link the spec came from. `None` for a file import.
    pub source_url: Option<String>,
    /// Problems that did not stop the import.
    pub warnings: Vec<String>,
}

/// A message the user can act on. Private repositories need a token, which is not wired yet.
fn describe_fetch_failure(failure: &SpecFetchFailure) -> String {
    match failure {
        SpecFetchFailure::Unauthorized | SpecFetchFailure::NotFound => {
            "The file was not found or is private. Links to private repositories \
             are not supported yet."
                .to_string()
        }
        other => other.to_string(),
    }
}

/// True when the ref looks like a commit id (7 to 40 hex characters), so it never changes.
fn is_commit_ref(location: &SpecLocation) -> bool {
    match location {
        SpecLocation::Github { git_ref, .. } => {
            (7..=40).contains(&git_ref.len()) && git_ref.chars().all(|c| c.is_ascii_hexdigit())
        }
        SpecLocation::Https { .. } => false,
    }
}

/// The last path segment of a link, without its query. Used to name the collection.
fn label_from_link(link: &str) -> String {
    let path = link.split(['?', '#']).next().unwrap_or_default();
    path.rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or("openapi")
        .to_string()
}

impl ImportService {
    /// Import a spec file from disk as a new collection. No source file is written, because
    /// there is no link to sync from.
    pub fn import_openapi_file(
        &self,
        path: &Path,
        group_by: GroupBy,
        workspace_id: &str,
    ) -> ImportResult<OpenApiImportResult> {
        let size = std::fs::metadata(path)?.len();
        if size > DEFAULT_MAX_SPEC_BYTES as u64 {
            return Err(ImportError::SpecUnavailable(
                "The file is larger than the 25 MB limit.".to_string(),
            ));
        }
        let bytes = std::fs::read(path)?;
        let text = String::from_utf8(bytes)
            .map_err(|_| ImportError::NotAnOpenApiSpec("the file is not text".to_string()))?;
        let label = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "openapi".to_string());
        let outcome = self.import_openapi_text(&text, &label, group_by, workspace_id)?;
        Ok(OpenApiImportResult {
            report: outcome.report,
            collection: outcome.collection,
            source_url: None,
            warnings: Vec::new(),
        })
    }

    /// Fetch a spec from a link, import it as a new collection and remember the link in the
    /// collection's source file. A failed fetch creates nothing. A source file that cannot be
    /// written is a warning, not an error.
    pub async fn import_openapi_url(
        &self,
        fetcher: &dyn SpecFetcher,
        sources: &dyn OpenApiSourceRepository,
        link: &str,
        group_by: GroupBy,
        workspace_id: &str,
    ) -> ImportResult<OpenApiImportResult> {
        let location = parse_spec_url(link)?;
        let target = fetch_target(&location, false);
        let request = SpecFetchRequest {
            url: target.url,
            accept: target.accept,
            bearer_token: None,
            if_none_match: None,
            if_modified_since: None,
            max_bytes: DEFAULT_MAX_SPEC_BYTES,
        };
        let outcome = fetcher
            .fetch(request)
            .await
            .map_err(|f| ImportError::SpecUnavailable(describe_fetch_failure(&f)))?;
        let (body, etag, last_modified) = match outcome {
            SpecFetchOutcome::Fetched {
                body,
                etag,
                last_modified,
                ..
            } => (body, etag, last_modified),
            SpecFetchOutcome::NotModified => {
                return Err(ImportError::SpecUnavailable(
                    "The server sent no file.".to_string(),
                ))
            }
        };
        let text = String::from_utf8(body.clone())
            .map_err(|_| ImportError::NotAnOpenApiSpec("the file is not text".to_string()))?;
        let display_url = location.display_url();
        let imported = self.import_openapi_text(
            &text,
            &label_from_link(&display_url),
            group_by,
            workspace_id,
        )?;

        let mut warnings = Vec::new();
        let record = source_record(&location, group_by, &body, etag, last_modified, &imported);
        let root = self.collections_root().join(&imported.collection);
        if let Err(e) = sources.save(&root, &record) {
            warnings.push(format!("Sync could not be set up: {e}"));
        }
        if is_commit_ref(&location) {
            warnings.push(
                "This link points to a commit, so the file will never change and \
                 sync will find nothing new."
                    .to_string(),
            );
        }
        Ok(OpenApiImportResult {
            report: imported.report,
            collection: imported.collection,
            source_url: Some(display_url),
            warnings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ImportError;
    use crate::importer::ImportService;
    use crate::openapi::GroupBy;
    use async_trait::async_trait;
    use rocket_collection::openapi_source::{OpenApiSource, OpenApiSourceRepository, SpecLocation};
    use rocket_http::spec_fetch::{
        SpecFetchFailure, SpecFetchOutcome, SpecFetchRequest, SpecFetcher, DEFAULT_MAX_SPEC_BYTES,
    };
    use rocket_infra::FsOpenApiSourceRepo;
    use rocket_shared::error::{DomainError, DomainResult};
    use std::path::Path;
    use std::sync::Mutex;
    use tempfile::TempDir;

    fn fixture() -> String {
        let path = format!(
            "{}/tests/fixtures/openapi/orders-3.0.json",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(path).expect("fixture should be readable")
    }

    struct FakeFetcher {
        reply: Result<SpecFetchOutcome, SpecFetchFailure>,
        last: Mutex<Option<SpecFetchRequest>>,
    }

    impl FakeFetcher {
        fn new(reply: Result<SpecFetchOutcome, SpecFetchFailure>) -> Self {
            Self {
                reply,
                last: Mutex::new(None),
            }
        }

        fn body(body: &str, etag: Option<&str>) -> Self {
            Self::new(Ok(SpecFetchOutcome::Fetched {
                body: body.as_bytes().to_vec(),
                etag: etag.map(str::to_string),
                last_modified: None,
                final_url: "https://example.com/orders.json".to_string(),
            }))
        }

        fn last_request(&self) -> SpecFetchRequest {
            self.last
                .lock()
                .expect("lock")
                .clone()
                .expect("a request was made")
        }
    }

    #[async_trait]
    impl SpecFetcher for FakeFetcher {
        async fn fetch(
            &self,
            request: SpecFetchRequest,
        ) -> Result<SpecFetchOutcome, SpecFetchFailure> {
            *self.last.lock().expect("lock") = Some(request);
            self.reply.clone()
        }
    }

    struct FailingSources;

    impl OpenApiSourceRepository for FailingSources {
        fn load(&self, _root: &Path) -> DomainResult<Option<OpenApiSource>> {
            Ok(None)
        }
        fn save(&self, _root: &Path, _source: &OpenApiSource) -> DomainResult<()> {
            Err(DomainError::Internal("disk full".to_string()))
        }
        fn delete(&self, _root: &Path) -> DomainResult<()> {
            Ok(())
        }
    }

    fn collection_count(ws: &Path) -> usize {
        match std::fs::read_dir(ws.join("collections")) {
            Ok(entries) => entries.count(),
            Err(_) => 0,
        }
    }

    const LINK: &str = "https://example.com/specs/orders.json";

    #[tokio::test]
    async fn url_import_creates_collection_and_source_file() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let fetcher = FakeFetcher::body(&fixture(), Some("\"e1\""));
        let result = service
            .import_openapi_url(&fetcher, &FsOpenApiSourceRepo, LINK, GroupBy::Tags, "ws")
            .await
            .expect("import");
        let root = ws.path().join("collections").join(&result.collection);
        assert!(root.is_dir());
        assert_eq!(result.source_url, Some(LINK.to_string()));
        let source = FsOpenApiSourceRepo
            .load(&root)
            .expect("load")
            .expect("source file");
        assert!(matches!(source.location, SpecLocation::Https { .. }));
        assert_eq!(source.group_by, "tags");
        assert_eq!(source.etag, Some("\"e1\"".to_string()));
        assert_eq!(source.operations.len(), 3);
        assert!(result.warnings.is_empty());
    }

    #[tokio::test]
    async fn github_blob_link_is_fetched_from_the_raw_host_without_a_token() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let fetcher = FakeFetcher::body(&fixture(), None);
        let link = "https://github.com/o/r/blob/main/api/orders.json";
        let result = service
            .import_openapi_url(&fetcher, &FsOpenApiSourceRepo, link, GroupBy::Tags, "ws")
            .await
            .expect("import");
        let request = fetcher.last_request();
        assert!(request
            .url
            .starts_with("https://raw.githubusercontent.com/"));
        assert!(request.bearer_token.is_none());
        assert_eq!(request.max_bytes, DEFAULT_MAX_SPEC_BYTES);
        let root = ws.path().join("collections").join(&result.collection);
        let source = FsOpenApiSourceRepo
            .load(&root)
            .expect("load")
            .expect("source file");
        assert!(matches!(source.location, SpecLocation::Github { .. }));
    }

    #[tokio::test]
    async fn failed_fetch_creates_no_collection() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let fetcher = FakeFetcher::new(Err(SpecFetchFailure::NotFound));
        let err = service
            .import_openapi_url(&fetcher, &FsOpenApiSourceRepo, LINK, GroupBy::Tags, "ws")
            .await
            .expect_err("should fail");
        assert!(matches!(err, ImportError::SpecUnavailable(_)), "{err:?}");
        assert_eq!(collection_count(ws.path()), 0);
    }

    #[tokio::test]
    async fn not_modified_is_an_error() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let fetcher = FakeFetcher::new(Ok(SpecFetchOutcome::NotModified));
        let err = service
            .import_openapi_url(&fetcher, &FsOpenApiSourceRepo, LINK, GroupBy::Tags, "ws")
            .await
            .expect_err("should fail");
        assert!(matches!(err, ImportError::SpecUnavailable(_)), "{err:?}");
        assert_eq!(collection_count(ws.path()), 0);
    }

    #[tokio::test]
    async fn html_body_creates_no_collection() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let fetcher = FakeFetcher::body("<html><body>Sign in</body></html>", None);
        let err = service
            .import_openapi_url(&fetcher, &FsOpenApiSourceRepo, LINK, GroupBy::Tags, "ws")
            .await
            .expect_err("should fail");
        assert!(matches!(err, ImportError::NotAnOpenApiSpec(_)), "{err:?}");
        assert_eq!(collection_count(ws.path()), 0);
    }

    #[tokio::test]
    async fn source_file_failure_does_not_fail_the_import() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let fetcher = FakeFetcher::body(&fixture(), None);
        let result = service
            .import_openapi_url(&fetcher, &FailingSources, LINK, GroupBy::Tags, "ws")
            .await
            .expect("import");
        assert!(ws
            .path()
            .join("collections")
            .join(&result.collection)
            .is_dir());
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].to_lowercase().contains("sync"));
    }

    #[tokio::test]
    async fn commit_sha_ref_warns_that_nothing_will_change() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let fetcher = FakeFetcher::body(&fixture(), None);
        let sha = "https://github.com/o/r/blob/0123456789abcdef0123456789abcdef01234567/api.yaml";
        let result = service
            .import_openapi_url(&fetcher, &FsOpenApiSourceRepo, sha, GroupBy::Tags, "ws")
            .await
            .expect("import");
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("commit"));

        let branch = "https://github.com/o/r/blob/main/api.yaml";
        let result = service
            .import_openapi_url(&fetcher, &FsOpenApiSourceRepo, branch, GroupBy::Tags, "ws")
            .await
            .expect("import");
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn file_import_writes_no_source_file() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let file = ws.path().join("orders.json");
        std::fs::write(&file, fixture()).expect("write");
        let result = service
            .import_openapi_file(&file, GroupBy::Tags, "ws")
            .expect("import");
        assert_eq!(result.source_url, None);
        let root = ws.path().join("collections").join(&result.collection);
        assert!(root.is_dir());
        assert!(!root.join(".rocket/openapi-source.yml").exists());
    }

    #[test]
    fn file_over_the_size_cap_is_rejected() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let file = ws.path().join("big.json");
        let handle = std::fs::File::create(&file).expect("create");
        handle
            .set_len(DEFAULT_MAX_SPEC_BYTES as u64 + 1)
            .expect("set_len");
        let err = service
            .import_openapi_file(&file, GroupBy::Tags, "ws")
            .expect_err("should fail");
        assert!(matches!(err, ImportError::SpecUnavailable(_)), "{err:?}");
        assert_eq!(collection_count(ws.path()), 0);
    }

    #[test]
    fn failure_messages_name_the_cause() {
        for failure in [SpecFetchFailure::Unauthorized, SpecFetchFailure::NotFound] {
            let text = describe_fetch_failure(&failure);
            assert!(text.contains("private"), "{text}");
            assert!(text.contains("not supported yet"), "{text}");
        }
        let limited = SpecFetchFailure::RateLimited {
            retry_after_secs: Some(60),
        };
        assert!(describe_fetch_failure(&limited).contains("60"));
        assert!(describe_fetch_failure(&SpecFetchFailure::TooLarge).contains("25 MB"));
        assert!(describe_fetch_failure(&SpecFetchFailure::NotHttps).contains("https"));
    }

    #[test]
    fn group_by_parse_accepts_only_the_two_layouts() {
        assert_eq!(GroupBy::parse("tags"), Some(GroupBy::Tags));
        assert_eq!(GroupBy::parse("paths"), Some(GroupBy::Paths));
        for bad in ["Tags", "", "tag"] {
            assert_eq!(GroupBy::parse(bad), None, "{bad}");
        }
    }
}
