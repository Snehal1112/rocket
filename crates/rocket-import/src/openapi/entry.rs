//! Import an OpenAPI spec from a file on disk or from a link.

use crate::converter::openapi::CONVERTER_VERSION;
use crate::error::{ImportError, ImportResult};
use crate::importer::ImportService;
use crate::openapi::{fetch_target, parse_spec_url, source_record, GroupBy};
use crate::report::ImportReport;
use rocket_collection::openapi_baseline::{
    BaselineOperation, OpenApiBaseline, OpenApiBaselineRepository, BASELINE_FILE_VERSION,
};
use rocket_collection::openapi_source::{OpenApiSourceRepository, SpecLocation};
use rocket_http::spec_fetch::{
    SpecFetchFailure, SpecFetchOutcome, SpecFetchRequest, SpecFetcher, DEFAULT_MAX_SPEC_BYTES,
};
use std::io::Read;
use std::path::Path;

/// What an OpenAPI import from a file or a link gives back.
#[derive(Debug, Clone)]
pub struct OpenApiImportResult {
    pub report: ImportReport,
    /// The name of the collection that was created.
    pub collection: String,
    /// The link the spec came from. `None` for a file import.
    pub source_url: Option<String>,
    /// The id written to the source file. `None` for a file import.
    pub source_id: Option<String>,
    /// Problems that did not stop the import.
    pub warnings: Vec<String>,
}

/// A message the user can act on. `token_sent` says whether the request carried a token.
pub fn fetch_failure_message(failure: &SpecFetchFailure, token_sent: bool) -> String {
    match failure {
        SpecFetchFailure::Unauthorized | SpecFetchFailure::NotFound if token_sent => {
            "The file was not found, or the token was rejected or lacks Contents read access."
                .to_string()
        }
        SpecFetchFailure::Unauthorized | SpecFetchFailure::NotFound => {
            "The file was not found, or it needs a sign-in. \
             For a private GitHub repository, add an access token."
                .to_string()
        }
        other => other.to_string(),
    }
}

/// Message for a fetch that sent no token.
#[cfg(test)]
fn describe_fetch_failure(failure: &SpecFetchFailure) -> String {
    fetch_failure_message(failure, false)
}

/// Checks that the bytes are UTF-8 text that parses as a supported spec. Writes nothing.
pub fn check_spec_text(bytes: &[u8]) -> ImportResult<()> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| ImportError::NotAnOpenApiSpec("the file is not text".to_string()))?;
    crate::openapi::parse_openapi(text).map(|_| ())
}

/// How sure we are that a link points to a commit.
enum CommitRef {
    /// A full 40 character id, which can never change.
    Certain,
    /// 7 to 39 hex characters, which could also be a branch or tag name.
    Possible,
}

fn commit_ref(location: &SpecLocation) -> Option<CommitRef> {
    if location.ref_is_immutable() {
        return Some(CommitRef::Certain);
    }
    match location {
        SpecLocation::Github { git_ref, .. }
            if (7..40).contains(&git_ref.len())
                && git_ref.chars().all(|c| c.is_ascii_hexdigit()) =>
        {
            Some(CommitRef::Possible)
        }
        _ => None,
    }
}

/// Reads at most `cap` bytes. One byte more than the cap means the source is too large.
fn read_capped(reader: impl Read, cap: u64) -> ImportResult<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(cap + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > cap {
        return Err(ImportError::SpecUnavailable(
            "The file is larger than the 25 MB limit.".to_string(),
        ));
    }
    Ok(bytes)
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
        let file = std::fs::File::open(path)?;
        let meta = file.metadata()?;
        if !meta.is_file() {
            return Err(ImportError::SpecUnavailable(
                "That path is not a regular file.".to_string(),
            ));
        }
        if meta.len() > DEFAULT_MAX_SPEC_BYTES as u64 {
            return Err(ImportError::SpecUnavailable(
                "The file is larger than the 25 MB limit.".to_string(),
            ));
        }
        let bytes = read_capped(file, DEFAULT_MAX_SPEC_BYTES as u64)?;
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
            source_id: None,
            warnings: Vec::new(),
        })
    }

    /// Fetch a spec from a link, import it as a new collection and remember the link in the
    /// collection's source file. A failed fetch creates nothing. A source file that cannot be
    /// written is a warning, not an error. A token is sent to GitHub links only and is never
    /// stored here; the caller keeps it in the keychain.
    pub async fn import_openapi_url(
        &self,
        fetcher: &dyn SpecFetcher,
        sources: &dyn OpenApiSourceRepository,
        baselines: &dyn OpenApiBaselineRepository,
        link: &str,
        group_by: GroupBy,
        token: Option<&str>,
        workspace_id: &str,
    ) -> ImportResult<OpenApiImportResult> {
        let location = parse_spec_url(link)?;
        let token = token.map(str::trim).filter(|t| !t.is_empty());
        if token.is_some() && !matches!(location, SpecLocation::Github { .. }) {
            return Err(ImportError::SpecUnavailable(
                "Tokens work with GitHub links only.".to_string(),
            ));
        }
        let target = fetch_target(&location, token.is_some());
        let request = SpecFetchRequest {
            url: target.url,
            accept: target.accept,
            bearer_token: token.map(str::to_string),
            if_none_match: None,
            if_modified_since: None,
            max_bytes: DEFAULT_MAX_SPEC_BYTES,
        };
        let outcome = fetcher.fetch(request).await.map_err(|f| {
            ImportError::SpecUnavailable(fetch_failure_message(&f, token.is_some()))
        })?;
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
        let text = String::from_utf8(body)
            .map_err(|_| ImportError::NotAnOpenApiSpec("the file is not text".to_string()))?;
        let display_url = location.display_url();
        let imported = self.import_openapi_text(
            &text,
            &label_from_link(&display_url),
            group_by,
            workspace_id,
        )?;

        let mut warnings = Vec::new();
        let record = source_record(
            &location,
            group_by,
            text.as_bytes(),
            etag,
            last_modified,
            &imported,
        );
        let root = self.collections_root().join(&imported.collection);
        if let Err(e) = sources.save(&root, &record) {
            warnings.push(format!("Sync could not be set up: {e}"));
        }
        let baseline = OpenApiBaseline {
            version: BASELINE_FILE_VERSION,
            converter_version: CONVERTER_VERSION,
            base_url: imported.base_url.clone(),
            operations: imported
                .operations
                .iter()
                .map(|op| BaselineOperation {
                    key: op.key.clone(),
                    request: op.baseline.clone(),
                })
                .collect(),
        };
        if baselines.save(&root, &baseline).is_err() {
            warnings.push(
                "The baseline snapshot could not be saved. \
                 The review will be cautious until the next sync."
                    .to_string(),
            );
        }
        match commit_ref(&location) {
            Some(CommitRef::Certain) => warnings.push(
                "This link points to a commit, so the file will never change and \
                 sync will find nothing new."
                    .to_string(),
            ),
            Some(CommitRef::Possible) => warnings.push(
                "This link may point to a commit. If it does, the spec will never \
                 change and sync will find nothing new."
                    .to_string(),
            ),
            None => {}
        }
        Ok(OpenApiImportResult {
            report: imported.report,
            collection: imported.collection,
            source_url: Some(display_url),
            source_id: Some(record.source_id.clone()),
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
    use rocket_collection::openapi_baseline::OpenApiBaseline as Baseline;
    use rocket_collection::openapi_source::{OpenApiSource, OpenApiSourceRepository, SpecLocation};
    use rocket_collection::CollectionRepository;
    use rocket_http::spec_fetch::{
        SpecFetchFailure, SpecFetchOutcome, SpecFetchRequest, SpecFetcher, DEFAULT_MAX_SPEC_BYTES,
    };
    use rocket_infra::{FsCollectionRepo, FsOpenApiBaselineRepo, FsOpenApiSourceRepo};
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
        fn save_pending(&self, _root: &Path, _spec: &[u8]) -> DomainResult<()> {
            Err(DomainError::Internal("disk full".to_string()))
        }
        fn load_pending(&self, _root: &Path) -> DomainResult<Option<Vec<u8>>> {
            Ok(None)
        }
        fn clear_pending(&self, _root: &Path) -> DomainResult<()> {
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
            .import_openapi_url(
                &fetcher,
                &FsOpenApiSourceRepo,
                &FsOpenApiBaselineRepo,
                LINK,
                GroupBy::Tags,
                None,
                "ws",
            )
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
            .import_openapi_url(
                &fetcher,
                &FsOpenApiSourceRepo,
                &FsOpenApiBaselineRepo,
                link,
                GroupBy::Tags,
                None,
                "ws",
            )
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
            .import_openapi_url(
                &fetcher,
                &FsOpenApiSourceRepo,
                &FsOpenApiBaselineRepo,
                LINK,
                GroupBy::Tags,
                None,
                "ws",
            )
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
            .import_openapi_url(
                &fetcher,
                &FsOpenApiSourceRepo,
                &FsOpenApiBaselineRepo,
                LINK,
                GroupBy::Tags,
                None,
                "ws",
            )
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
            .import_openapi_url(
                &fetcher,
                &FsOpenApiSourceRepo,
                &FsOpenApiBaselineRepo,
                LINK,
                GroupBy::Tags,
                None,
                "ws",
            )
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
            .import_openapi_url(
                &fetcher,
                &FailingSources,
                &FsOpenApiBaselineRepo,
                LINK,
                GroupBy::Tags,
                None,
                "ws",
            )
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
            .import_openapi_url(
                &fetcher,
                &FsOpenApiSourceRepo,
                &FsOpenApiBaselineRepo,
                sha,
                GroupBy::Tags,
                None,
                "ws",
            )
            .await
            .expect("import");
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("points to a commit"));
        assert!(result.warnings[0].contains("never change"));

        let branch = "https://github.com/o/r/blob/main/api.yaml";
        let result = service
            .import_openapi_url(
                &fetcher,
                &FsOpenApiSourceRepo,
                &FsOpenApiBaselineRepo,
                branch,
                GroupBy::Tags,
                None,
                "ws",
            )
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
            assert!(text.contains("access token"), "{text}");
        }
        let limited = SpecFetchFailure::RateLimited {
            retry_after_secs: Some(60),
        };
        assert!(describe_fetch_failure(&limited).contains("60"));
        assert!(describe_fetch_failure(&SpecFetchFailure::TooLarge).contains("25 MB"));
        assert!(describe_fetch_failure(&SpecFetchFailure::NotHttps).contains("https"));
    }

    #[test]
    fn token_failure_text_names_the_token() {
        let with = fetch_failure_message(&SpecFetchFailure::Unauthorized, true);
        assert!(with.contains("token"), "{with}");
        let without = fetch_failure_message(&SpecFetchFailure::Unauthorized, false);
        assert!(without.contains("private"), "{without}");
    }

    #[test]
    fn check_spec_text_accepts_a_spec_and_rejects_the_rest() {
        assert!(check_spec_text(fixture().as_bytes()).is_ok());
        assert!(check_spec_text(b"<html></html>").is_err());
        assert!(check_spec_text(&[0xff, 0xfe, 0x00]).is_err());
    }

    #[test]
    fn read_capped_rejects_more_than_the_cap() {
        let exact = read_capped(std::io::Cursor::new(vec![b'a'; 10]), 10).expect("at the cap");
        assert_eq!(exact.len(), 10);
        let err = read_capped(std::io::Cursor::new(vec![b'a'; 11]), 10).expect_err("over");
        assert!(matches!(err, ImportError::SpecUnavailable(_)), "{err:?}");
    }

    #[test]
    fn directory_is_rejected_as_a_spec_file() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let err = service
            .import_openapi_file(ws.path(), GroupBy::Tags, "ws")
            .expect_err("should fail");
        assert!(matches!(err, ImportError::SpecUnavailable(_)), "{err:?}");
        assert_eq!(collection_count(ws.path()), 0);
    }

    #[tokio::test]
    async fn all_hex_branch_name_gets_the_hedged_commit_warning() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let fetcher = FakeFetcher::body(&fixture(), None);
        let link = "https://github.com/o/r/blob/deadbeef/api.yaml";
        let result = service
            .import_openapi_url(
                &fetcher,
                &FsOpenApiSourceRepo,
                &FsOpenApiBaselineRepo,
                link,
                GroupBy::Tags,
                None,
                "ws",
            )
            .await
            .expect("import");
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("may point to a commit"));
        assert!(!result.warnings[0].contains("points to a commit"));
    }

    const GH_LINK: &str = "https://github.com/o/r/blob/main/api/orders.json";

    #[tokio::test]
    async fn github_link_with_a_token_uses_the_contents_api_and_a_bearer() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let fetcher = FakeFetcher::body(&fixture(), None);
        service
            .import_openapi_url(
                &fetcher,
                &FsOpenApiSourceRepo,
                &FsOpenApiBaselineRepo,
                GH_LINK,
                GroupBy::Tags,
                Some("t0k"),
                "ws",
            )
            .await
            .expect("import");
        let request = fetcher.last_request();
        assert!(
            request.url.starts_with("https://api.github.com/"),
            "{}",
            request.url
        );
        assert_eq!(request.bearer_token.as_deref(), Some("t0k"));
        assert!(
            request.accept.as_deref().is_some_and(|a| a.contains("raw")),
            "{:?}",
            request.accept
        );
    }

    #[tokio::test]
    async fn https_link_with_a_token_is_refused_before_any_fetch() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let fetcher = FakeFetcher::body(&fixture(), None);
        let err = service
            .import_openapi_url(
                &fetcher,
                &FsOpenApiSourceRepo,
                &FsOpenApiBaselineRepo,
                LINK,
                GroupBy::Tags,
                Some("t0k"),
                "ws",
            )
            .await
            .expect_err("should fail");
        assert!(matches!(err, ImportError::SpecUnavailable(_)), "{err:?}");
        assert!(fetcher.last.lock().expect("lock").is_none());
        assert_eq!(collection_count(ws.path()), 0);
        assert!(!err.to_string().contains("t0k"));
    }

    fn files_under(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    files_under(&path, out);
                } else {
                    out.push(path);
                }
            }
        }
    }

    #[tokio::test]
    async fn token_never_reaches_the_source_file_or_the_result() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let fetcher = FakeFetcher::body(&fixture(), None);
        let result = service
            .import_openapi_url(
                &fetcher,
                &FsOpenApiSourceRepo,
                &FsOpenApiBaselineRepo,
                GH_LINK,
                GroupBy::Tags,
                Some("t0k"),
                "ws",
            )
            .await
            .expect("import");
        let mut files = Vec::new();
        files_under(ws.path(), &mut files);
        assert!(!files.is_empty());
        for file in files {
            let bytes = std::fs::read(&file).expect("read");
            let text = String::from_utf8_lossy(&bytes);
            assert!(!text.contains("t0k"), "{}", file.display());
        }
        assert!(result.warnings.iter().all(|w| !w.contains("t0k")));
        assert!(!format!("{result:?}").contains("t0k"));
    }

    #[tokio::test]
    async fn unauthorized_with_a_token_says_the_token_was_rejected() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let fetcher = FakeFetcher::new(Err(SpecFetchFailure::Unauthorized));
        let err = service
            .import_openapi_url(
                &fetcher,
                &FsOpenApiSourceRepo,
                &FsOpenApiBaselineRepo,
                GH_LINK,
                GroupBy::Tags,
                Some("t0k"),
                "ws",
            )
            .await
            .expect_err("should fail");
        let text = err.to_string();
        assert!(text.contains("token was rejected"), "{text}");
        assert!(!text.contains("t0k"));
        assert_eq!(collection_count(ws.path()), 0);
    }

    #[tokio::test]
    async fn link_import_returns_the_source_id_written_to_the_file() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let fetcher = FakeFetcher::body(&fixture(), None);
        let result = service
            .import_openapi_url(
                &fetcher,
                &FsOpenApiSourceRepo,
                &FsOpenApiBaselineRepo,
                LINK,
                GroupBy::Tags,
                None,
                "ws",
            )
            .await
            .expect("import");
        let root = ws.path().join("collections").join(&result.collection);
        let source = FsOpenApiSourceRepo
            .load(&root)
            .expect("load")
            .expect("source file");
        assert!(!source.source_id.is_empty());
        assert_eq!(result.source_id, Some(source.source_id));

        let file = ws.path().join("orders.json");
        std::fs::write(&file, fixture()).expect("write");
        let from_file = service
            .import_openapi_file(&file, GroupBy::Tags, "ws")
            .expect("import");
        assert_eq!(from_file.source_id, None);
    }

    struct FailingBaselines;

    impl OpenApiBaselineRepository for FailingBaselines {
        fn load(&self, _root: &Path) -> DomainResult<Option<Baseline>> {
            Ok(None)
        }
        fn save(&self, _root: &Path, _baseline: &Baseline) -> DomainResult<()> {
            Err(DomainError::Internal("disk full".to_string()))
        }
        fn delete(&self, _root: &Path) -> DomainResult<()> {
            Ok(())
        }
    }

    async fn import_fixture(ws: &Path) -> OpenApiImportResult {
        let service = ImportService::new_with_workspace_path(ws);
        let fetcher = FakeFetcher::body(&fixture(), None);
        service
            .import_openapi_url(
                &fetcher,
                &FsOpenApiSourceRepo,
                &FsOpenApiBaselineRepo,
                LINK,
                GroupBy::Tags,
                None,
                "ws",
            )
            .await
            .expect("import")
    }

    #[tokio::test]
    async fn link_import_writes_a_baseline_matching_the_saved_requests() {
        let ws = TempDir::new().expect("tempdir");
        let result = import_fixture(ws.path()).await;
        let root = ws.path().join("collections").join(&result.collection);
        let baseline = FsOpenApiBaselineRepo
            .load(&root)
            .expect("load")
            .expect("baseline file");
        let source = FsOpenApiSourceRepo
            .load(&root)
            .expect("load")
            .expect("source file");
        assert_eq!(baseline.operations.len(), result.report.imported);
        assert_eq!(baseline.operations.len(), source.operations.len());
        let repo = FsCollectionRepo::new_standalone(ws.path().join("collections"));
        for (op, entry) in baseline.operations.iter().zip(&source.operations) {
            assert_eq!(op.key, entry.key);
            let mut saved = repo
                .get_request(&result.collection, &entry.request_path)
                .expect("saved request");
            let mut snap = op.request.clone();
            saved.file_name = None;
            snap.file_name = None;
            assert_eq!(snap, saved, "{}", op.key);
        }
        let ignore = std::fs::read_to_string(root.join(".rocket/.gitignore")).expect("gitignore");
        assert!(ignore.contains("openapi-baseline.json"), "{ignore}");
    }

    #[tokio::test]
    async fn baseline_records_the_converter_version_and_base_url() {
        let ws = TempDir::new().expect("tempdir");
        let result = import_fixture(ws.path()).await;
        let root = ws.path().join("collections").join(&result.collection);
        let baseline = FsOpenApiBaselineRepo
            .load(&root)
            .expect("load")
            .expect("baseline file");
        assert_eq!(baseline.converter_version, CONVERTER_VERSION);
        assert_eq!(baseline.version, BASELINE_FILE_VERSION);
        let spec: serde_json::Value = serde_json::from_str(&fixture()).expect("json");
        let expected = spec["servers"][0]["url"].as_str().unwrap_or_default();
        assert!(!expected.is_empty());
        assert_eq!(baseline.base_url, expected);
    }

    #[tokio::test]
    async fn baseline_write_failure_is_a_warning_not_an_error() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let fetcher = FakeFetcher::body(&fixture(), None);
        let result = service
            .import_openapi_url(
                &fetcher,
                &FsOpenApiSourceRepo,
                &FailingBaselines,
                LINK,
                GroupBy::Tags,
                None,
                "ws",
            )
            .await
            .expect("import");
        assert_eq!(result.warnings.len(), 1);
        assert!(
            result.warnings[0].contains("cautious"),
            "{:?}",
            result.warnings
        );
        let root = ws.path().join("collections").join(&result.collection);
        assert!(FsOpenApiSourceRepo.load(&root).expect("load").is_some());
    }

    #[test]
    fn file_import_writes_no_baseline() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let file = ws.path().join("orders.json");
        std::fs::write(&file, fixture()).expect("write");
        let result = service
            .import_openapi_file(&file, GroupBy::Tags, "ws")
            .expect("import");
        let root = ws.path().join("collections").join(&result.collection);
        assert!(!root.join(".rocket/openapi-baseline.json").exists());
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
