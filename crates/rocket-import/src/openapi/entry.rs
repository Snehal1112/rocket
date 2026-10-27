//! Import an OpenAPI spec from a file on disk or from a link.

use crate::converter::openapi::CONVERTER_VERSION;
use crate::error::{ImportError, ImportResult};
use crate::importer::ImportService;
use crate::openapi::{
    bundle_spec, fetch_target, parse_spec_target, parse_spec_url, source_record, Bundle, GroupBy,
};
use crate::report::ImportReport;
use rocket_collection::openapi_baseline::{
    BaselineOperation, OpenApiBaseline, OpenApiBaselineRepository, BASELINE_FILE_VERSION,
};
use rocket_collection::openapi_source::{OpenApiSourceRepository, RemoteRefState, SpecLocation};
use rocket_http::spec_fetch::{
    LocalReadFailure, LocalSpecReader, SpecFetchFailure, SpecFetchOutcome, SpecFetchRequest,
    SpecFetcher, DEFAULT_MAX_SPEC_BYTES,
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
        // A network error can carry a host or path, so only a fixed sentence goes on.
        SpecFetchFailure::Network(_) => "The file could not be fetched.".to_string(),
        // A file on this computer: a fixed sentence that names no path.
        SpecFetchFailure::LocalFile(failure) => match failure {
            LocalReadFailure::NotFound => "The spec file was not found on this computer.",
            LocalReadFailure::NotAFile => "The path is not a file.",
            LocalReadFailure::TooLarge => "The file is larger than the 25 MB limit.",
            LocalReadFailure::Io => "The spec file could not be read.",
        }
        .to_string(),
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

/// A spec fetched from a link and bundled with the files it refers to.
pub struct FetchedSpec {
    pub location: SpecLocation,
    /// The link without credentials or query, safe to show.
    pub display_url: String,
    pub bundle: Bundle,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

/// Where a spec can come from: the network, or a file on this computer.
pub struct SpecSources<'a> {
    pub fetcher: &'a dyn SpecFetcher,
    pub reader: &'a dyn LocalSpecReader,
}

/// Reads or fetches the spec named by `input`: a link, a `file://` URL or a path. A relative
/// path is read from `root`. A token is for GitHub links only. A file is read by the reader and
/// its references to other files are not followed. Nothing is written.
pub async fn fetch_spec_bundle_from(
    sources: &SpecSources<'_>,
    root: &Path,
    input: &str,
    token: Option<&str>,
) -> ImportResult<FetchedSpec> {
    let location = parse_spec_target(input, root)?;
    let token = token.map(str::trim).filter(|t| !t.is_empty());
    let Some(path) = location.resolve_file(root) else {
        return fetch_location_bundle(sources.fetcher, location, token).await;
    };
    if token.is_some() {
        return Err(ImportError::SpecUnavailable(
            "Tokens work with GitHub links only.".to_string(),
        ));
    }
    let body = sources
        .reader
        .read(&path, DEFAULT_MAX_SPEC_BYTES)
        .map_err(|f| {
            ImportError::SpecUnavailable(fetch_failure_message(
                &SpecFetchFailure::LocalFile(f),
                false,
            ))
        })?;
    let text = String::from_utf8(body)
        .map_err(|_| ImportError::NotAnOpenApiSpec("the file is not text".to_string()))?;
    let display_url = location.display_url();
    let bundle = bundle_spec(sources.fetcher, &text, &location, None, &[]).await?;
    Ok(FetchedSpec {
        location,
        display_url,
        bundle,
        etag: None,
        last_modified: None,
    })
}

/// Fetches the spec behind `link` and bundles its remote references. A token is sent to GitHub
/// links only and is never stored. Nothing is written.
pub async fn fetch_spec_bundle(
    fetcher: &dyn SpecFetcher,
    link: &str,
    token: Option<&str>,
) -> ImportResult<FetchedSpec> {
    let location = parse_spec_url(link)?;
    let token = token.map(str::trim).filter(|t| !t.is_empty());
    fetch_location_bundle(fetcher, location, token).await
}

async fn fetch_location_bundle(
    fetcher: &dyn SpecFetcher,
    location: SpecLocation,
    token: Option<&str>,
) -> ImportResult<FetchedSpec> {
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
    let outcome = fetcher
        .fetch(request)
        .await
        .map_err(|f| ImportError::SpecUnavailable(fetch_failure_message(&f, token.is_some())))?;
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
    let bundle = bundle_spec(fetcher, &text, &location, token, &[]).await?;
    Ok(FetchedSpec {
        location,
        display_url,
        bundle,
        etag,
        last_modified,
    })
}

/// What a fetch learned about a spec besides its text.
#[derive(Default)]
struct SpecValidators {
    etag: Option<String>,
    last_modified: Option<String>,
    refs: Vec<RemoteRefState>,
    fetch_failed: bool,
}

/// Reads a spec file from disk as text, with the size cap. A path that is not a regular file is
/// refused.
fn read_spec_file_text(path: &Path) -> ImportResult<String> {
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
    String::from_utf8(bytes)
        .map_err(|_| ImportError::NotAnOpenApiSpec("the file is not text".to_string()))
}

fn file_label_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "openapi".to_string())
}

/// Writes the source file, the saved copy and the baseline of an imported collection. A write
/// that fails becomes a warning. It returns the id written to the source file.
#[allow(clippy::too_many_arguments)]
fn write_sync_state(
    sources: &dyn OpenApiSourceRepository,
    baselines: &dyn OpenApiBaselineRepository,
    root: &Path,
    location: &SpecLocation,
    group_by: GroupBy,
    text: &str,
    validators: SpecValidators,
    imported: &crate::importer::OpenApiImportOutcome,
    warnings: &mut Vec<String>,
) -> String {
    let record = source_record(
        location,
        group_by,
        text.as_bytes(),
        validators.etag,
        validators.last_modified,
        validators.refs,
        validators.fetch_failed,
        imported,
    );
    match sources.save(root, &record) {
        // The saved copy lets the spec be shown offline. It is only useful next to a source.
        Ok(()) => {
            if let Err(e) = sources.save_applied(root, text.as_bytes()) {
                tracing::debug!(error = %e, "the saved spec copy could not be written");
                warnings.push("The saved copy of the spec could not be written.".to_string());
            }
        }
        Err(e) => warnings.push(format!("Sync could not be set up: {e}")),
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
    if baselines.save(root, &baseline).is_err() {
        warnings.push(
            "The baseline snapshot could not be saved. \
             The review will be cautious until the next sync."
                .to_string(),
        );
    }
    record.source_id
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
        let text = read_spec_file_text(path)?;
        let label = file_label_of(path);
        let outcome = self.import_openapi_text(&text, &label, group_by, workspace_id)?;
        Ok(OpenApiImportResult {
            report: outcome.report,
            collection: outcome.collection,
            source_url: None,
            source_id: None,
            warnings: Vec::new(),
        })
    }

    /// Import a spec file from disk as a new collection and keep the file as its source, so
    /// later checks read it again. The source file, the baseline and the saved copy are written
    /// like for a link import. A path inside or beside the new collection is stored relative to
    /// it, any other as an absolute path. A source that cannot be written is a warning.
    pub fn import_openapi_file_linked(
        &self,
        sources: &dyn OpenApiSourceRepository,
        baselines: &dyn OpenApiBaselineRepository,
        path: &Path,
        group_by: GroupBy,
        workspace_id: &str,
    ) -> ImportResult<OpenApiImportResult> {
        let text = read_spec_file_text(path)?;
        let imported =
            self.import_openapi_text(&text, &file_label_of(path), group_by, workspace_id)?;
        let root = self.collections_root().join(&imported.collection);
        let location = parse_spec_target(&path.to_string_lossy(), &root)?;
        let display_url = location.display_url();
        let mut warnings = Vec::new();
        let source_id = write_sync_state(
            sources,
            baselines,
            &root,
            &location,
            group_by,
            &text,
            SpecValidators::default(),
            &imported,
            &mut warnings,
        );
        Ok(OpenApiImportResult {
            report: imported.report,
            collection: imported.collection,
            source_url: Some(display_url),
            source_id: Some(source_id),
            warnings,
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
        let FetchedSpec {
            location,
            display_url,
            bundle,
            etag,
            last_modified,
        } = fetch_spec_bundle(fetcher, link, token).await?;
        let imported = self.import_openapi_text(
            &bundle.text,
            &label_from_link(&display_url),
            group_by,
            workspace_id,
        )?;

        let mut warnings = bundle.notes.clone();
        let root = self.collections_root().join(&imported.collection);
        let source_id = write_sync_state(
            sources,
            baselines,
            &root,
            &location,
            group_by,
            &bundle.text,
            SpecValidators {
                etag,
                last_modified,
                refs: bundle.refs.clone(),
                fetch_failed: bundle.fetch_failed,
            },
            &imported,
            &mut warnings,
        );
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
            source_id: Some(source_id),
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
        fn save_applied(&self, _root: &Path, _spec: &[u8]) -> DomainResult<()> {
            Err(DomainError::Internal("disk full".to_string()))
        }
        fn load_applied(&self, _root: &Path) -> DomainResult<Option<Vec<u8>>> {
            Ok(None)
        }
        fn applied_matches(&self, _root: &Path, _sha256: &str) -> DomainResult<bool> {
            Ok(false)
        }
    }

    /// Delegates to the file repo, but cannot write the applied copy.
    struct NoAppliedSources;

    impl OpenApiSourceRepository for NoAppliedSources {
        fn load(&self, root: &Path) -> DomainResult<Option<OpenApiSource>> {
            FsOpenApiSourceRepo.load(root)
        }
        fn save(&self, root: &Path, source: &OpenApiSource) -> DomainResult<()> {
            FsOpenApiSourceRepo.save(root, source)
        }
        fn delete(&self, root: &Path) -> DomainResult<()> {
            FsOpenApiSourceRepo.delete(root)
        }
        fn save_pending(&self, root: &Path, spec: &[u8]) -> DomainResult<()> {
            FsOpenApiSourceRepo.save_pending(root, spec)
        }
        fn load_pending(&self, root: &Path) -> DomainResult<Option<Vec<u8>>> {
            FsOpenApiSourceRepo.load_pending(root)
        }
        fn clear_pending(&self, root: &Path) -> DomainResult<()> {
            FsOpenApiSourceRepo.clear_pending(root)
        }
        fn save_applied(&self, _root: &Path, _spec: &[u8]) -> DomainResult<()> {
            Err(DomainError::Internal("disk full".to_string()))
        }
        fn load_applied(&self, root: &Path) -> DomainResult<Option<Vec<u8>>> {
            FsOpenApiSourceRepo.load_applied(root)
        }
        fn applied_matches(&self, root: &Path, sha256: &str) -> DomainResult<bool> {
            FsOpenApiSourceRepo.applied_matches(root, sha256)
        }
    }

    fn collection_count(ws: &Path) -> usize {
        match std::fs::read_dir(ws.join("collections")) {
            Ok(entries) => entries.count(),
            Err(_) => 0,
        }
    }

    /// Serves a different body per address. An address that is not listed is not found.
    struct MapFetcher {
        files: Vec<(String, String, Option<String>)>,
        requests: Mutex<Vec<SpecFetchRequest>>,
    }

    impl MapFetcher {
        fn new(files: Vec<(String, String, Option<String>)>) -> Self {
            Self {
                files,
                requests: Mutex::new(Vec::new()),
            }
        }

        fn requests(&self) -> Vec<SpecFetchRequest> {
            self.requests.lock().expect("lock").clone()
        }
    }

    #[async_trait]
    impl SpecFetcher for MapFetcher {
        async fn fetch(
            &self,
            request: SpecFetchRequest,
        ) -> Result<SpecFetchOutcome, SpecFetchFailure> {
            self.requests.lock().expect("lock").push(request.clone());
            match self.files.iter().find(|(url, _, _)| *url == request.url) {
                Some((url, body, etag)) => Ok(SpecFetchOutcome::Fetched {
                    body: body.as_bytes().to_vec(),
                    etag: etag.clone(),
                    last_modified: None,
                    final_url: url.clone(),
                }),
                None => Err(SpecFetchFailure::NotFound),
            }
        }
    }

    const SAMPLE_LINK: &str = "https://github.com/teamdigitale/api-openapi-samples/blob/master/openapi-v3/api-example.yaml";
    const SAMPLE_RAW: &str = "https://raw.githubusercontent.com/teamdigitale/api-openapi-samples/master/openapi-v3/api-example.yaml";
    const DEFS_URL: &str =
        "https://raw.githubusercontent.com/teamdigitale/openapi/0.0.3/docs/definitions.yaml";
    const SAMPLE_SPEC: &str = r##"
openapi: 3.0.3
info: { title: Sample, version: '1' }
paths:
  /items:
    get:
      operationId: listItems
      parameters:
        - $ref: 'https://raw.githubusercontent.com/teamdigitale/openapi/0.0.3/docs/definitions.yaml#/parameters/limit'
        - $ref: 'https://raw.githubusercontent.com/teamdigitale/openapi/0.0.3/docs/definitions.yaml#/parameters/offset'
        - $ref: 'https://raw.githubusercontent.com/teamdigitale/openapi/0.0.3/docs/definitions.yaml#/parameters/sort'
      responses: { '200': { description: ok } }
  /things:
    get:
      operationId: listThings
      parameters:
        - $ref: 'https://raw.githubusercontent.com/teamdigitale/openapi/0.0.3/docs/definitions.yaml#/parameters/limit'
      responses: { '200': { description: ok } }
"##;
    const DEFS: &str = r##"
parameters:
  limit: { name: limit, in: query, schema: { type: integer } }
  offset: { name: offset, in: query, schema: { type: integer } }
  sort: { name: sort, in: query, schema: { type: string } }
"##;

    fn sha(text: &str) -> String {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(text.as_bytes()))
    }

    fn sample_fetcher(defs: bool) -> MapFetcher {
        let mut files = vec![(SAMPLE_RAW.to_string(), SAMPLE_SPEC.to_string(), None)];
        if defs {
            files.push((
                DEFS_URL.to_string(),
                DEFS.to_string(),
                Some("\"d1\"".to_string()),
            ));
        }
        MapFetcher::new(files)
    }

    #[tokio::test]
    async fn import_follows_a_remote_parameter_file() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let fetcher = sample_fetcher(true);
        let result = service
            .import_openapi_url(
                &fetcher,
                &FsOpenApiSourceRepo,
                &FsOpenApiBaselineRepo,
                SAMPLE_LINK,
                GroupBy::Tags,
                None,
                "ws",
            )
            .await
            .expect("import");
        let requests = fetcher.requests();
        assert_eq!(requests.len(), 2, "main and definitions only");
        assert_eq!(requests[0].url, SAMPLE_RAW);
        assert_eq!(requests[1].url, DEFS_URL);
        assert!(requests[1].bearer_token.is_none());
        assert_eq!(result.report.imported, 2);
        assert!(
            result.report.skipped.is_empty(),
            "{:?}",
            result.report.skipped
        );
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        let root = ws.path().join("collections").join(&result.collection);
        let source = FsOpenApiSourceRepo
            .load(&root)
            .expect("load")
            .expect("source file");
        assert_eq!(source.remote_refs.len(), 1);
        assert_eq!(source.remote_refs[0].url, DEFS_URL);
        assert_eq!(source.remote_refs[0].etag.as_deref(), Some("\"d1\""));
        assert!(!source.remote_refs_incomplete);
        assert_ne!(source.spec_sha256, sha(SAMPLE_SPEC));
        let bundled = crate::openapi::bundle_spec(
            &sample_fetcher(true),
            SAMPLE_SPEC,
            &parse_spec_url(SAMPLE_LINK).expect("link"),
            None,
            &[],
        )
        .await
        .expect("bundle");
        assert_eq!(source.spec_sha256, sha(&bundled.text));
        let first = source
            .operations
            .iter()
            .find(|op| op.key == "listItems")
            .expect("operation");
        let repo = FsCollectionRepo::new_standalone(ws.path().join("collections"));
        let saved = repo
            .get_request(&result.collection, &first.request_path)
            .expect("saved request");
        let names: Vec<&str> = saved.query_params.iter().map(|q| q.key.as_str()).collect();
        assert_eq!(names, vec!["limit", "offset", "sort"]);
    }

    #[tokio::test]
    async fn import_writes_the_applied_copy() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let result = service
            .import_openapi_url(
                &sample_fetcher(true),
                &FsOpenApiSourceRepo,
                &FsOpenApiBaselineRepo,
                SAMPLE_LINK,
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
            .expect("source");
        let copy = FsOpenApiSourceRepo
            .load_applied(&root)
            .expect("load")
            .expect("applied copy");
        assert_eq!(
            sha(&String::from_utf8(copy).expect("utf8")),
            source.spec_sha256
        );
    }

    #[tokio::test]
    async fn import_warns_when_the_applied_copy_cannot_be_written() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let result = service
            .import_openapi_url(
                &sample_fetcher(true),
                &NoAppliedSources,
                &FsOpenApiBaselineRepo,
                SAMPLE_LINK,
                GroupBy::Tags,
                None,
                "ws",
            )
            .await
            .expect("the import still succeeds");
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w == "The saved copy of the spec could not be written."),
            "{:?}",
            result.warnings
        );
    }

    #[tokio::test]
    async fn import_with_an_unreachable_ref_still_creates_the_collection_with_one_note() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let result = service
            .import_openapi_url(
                &sample_fetcher(false),
                &FsOpenApiSourceRepo,
                &FsOpenApiBaselineRepo,
                SAMPLE_LINK,
                GroupBy::Tags,
                None,
                "ws",
            )
            .await
            .expect("import");
        assert_eq!(result.report.imported, 2);
        assert_eq!(result.warnings.len(), 1, "{:?}", result.warnings);
        assert!(result.warnings[0].contains("definitions.yaml"));
        // The bundler's note is the one warning. The unresolved refs also reach the report as
        // grouped NotFollowed rows (four refs to the same file), which is accepted.
        assert_eq!(
            result.report.skipped.len(),
            4,
            "{:?}",
            result.report.skipped
        );
        assert!(result.report.skipped.iter().all(|i| matches!(
            &i.reason,
            crate::report::SkipReason::NotFollowed(t) if t == "a $ref to definitions.yaml was not loaded"
        )));
        let root = ws.path().join("collections").join(&result.collection);
        let source = FsOpenApiSourceRepo
            .load(&root)
            .expect("load")
            .expect("source file");
        assert!(source.remote_refs.is_empty());
        assert!(source.remote_refs_incomplete);
    }

    #[tokio::test]
    async fn a_limit_hit_is_not_recorded_as_a_fetch_failure() {
        // 21 distinct files is one over the limit of 20, so the bundle is incomplete but
        // nothing failed to download.
        let mut spec = String::from(
            "openapi: 3.0.3\ninfo: { title: T, version: '1' }\npaths:\n  /a:\n    get:\n      parameters:\n",
        );
        let mut files = vec![];
        for i in 0..21 {
            let url = format!("https://example.com/defs/p{i}.yaml");
            spec.push_str(&format!("        - $ref: '{url}#/p'\n"));
            files.push((
                url,
                format!("p: {{ name: p{i}, in: query }}\n"),
                None::<String>,
            ));
        }
        spec.push_str("      responses: { '200': { description: ok } }\n");
        files.push((SAMPLE_RAW.to_string(), spec, None));
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let result = service
            .import_openapi_url(
                &MapFetcher::new(files),
                &FsOpenApiSourceRepo,
                &FsOpenApiBaselineRepo,
                SAMPLE_LINK,
                GroupBy::Tags,
                None,
                "ws",
            )
            .await
            .expect("import");
        assert!(!result.warnings.is_empty());
        let root = ws.path().join("collections").join(&result.collection);
        let source = FsOpenApiSourceRepo
            .load(&root)
            .expect("load")
            .expect("source file");
        assert_eq!(source.remote_refs.len(), 20);
        assert!(!source.remote_refs_incomplete);
    }

    #[tokio::test]
    async fn a_spec_without_remote_refs_has_the_same_hash_as_before() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let text = fixture();
        let fetcher = FakeFetcher::body(&text, None);
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
        assert_eq!(source.spec_sha256, sha(&text));
        assert!(source.remote_refs.is_empty());
        let bytes = std::fs::read_to_string(root.join(".rocket/openapi-source.yml"))
            .expect("source file bytes");
        assert!(!bytes.contains("remote_refs"), "{bytes}");
    }

    #[test]
    fn a_file_import_keeps_remote_refs_unresolved_with_the_new_wording() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let file = ws.path().join("api-example.yaml");
        std::fs::write(&file, SAMPLE_SPEC).expect("write");
        let result = service
            .import_openapi_file(&file, GroupBy::Tags, "ws")
            .expect("import");
        assert_eq!(result.report.imported, 2);
        assert_eq!(result.report.skipped.len(), 4);
        for item in &result.report.skipped {
            match &item.reason {
                crate::report::SkipReason::NotFollowed(text) => {
                    assert_eq!(text, "a $ref to definitions.yaml was not loaded");
                }
                other => panic!("unexpected reason {other:?}"),
            }
        }
    }

    #[test]
    fn a_linked_file_import_writes_source_baseline_and_applied_copy() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let file = ws.path().join("orders.json");
        std::fs::write(&file, fixture()).expect("write");
        let result = service
            .import_openapi_file_linked(
                &FsOpenApiSourceRepo,
                &FsOpenApiBaselineRepo,
                &file,
                GroupBy::Tags,
                "ws",
            )
            .expect("import");
        let root = ws.path().join("collections").join(&result.collection);
        let source = FsOpenApiSourceRepo
            .load(&root)
            .expect("load")
            .expect("source file");
        // The file is outside the collection, so its absolute path is kept.
        assert_eq!(
            source.location,
            SpecLocation::File {
                path: file.to_string_lossy().replace('\\', "/")
            }
        );
        assert_eq!(source.spec_sha256.len(), 64);
        assert!(source.etag.is_none() && source.remote_refs.is_empty());
        assert!(!source.operations.is_empty());
        assert!(FsOpenApiBaselineRepo
            .load(&root)
            .expect("baseline")
            .is_some());
        assert!(FsOpenApiSourceRepo
            .applied_matches(&root, &source.spec_sha256)
            .expect("applied"));
        assert!(result
            .source_url
            .as_deref()
            .is_some_and(|u| u.starts_with("file://")));
        assert_eq!(result.source_id.as_deref(), Some(source.source_id.as_str()));
        // The source file is written as version 2.
        let bytes =
            std::fs::read_to_string(root.join(".rocket/openapi-source.yml")).expect("bytes");
        assert!(bytes.contains("version: 2"), "{bytes}");
    }

    #[test]
    fn an_unlinked_file_import_still_writes_nothing() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let file = ws.path().join("orders.json");
        std::fs::write(&file, fixture()).expect("write");
        let result = service
            .import_openapi_file(&file, GroupBy::Tags, "ws")
            .expect("import");
        let root = ws.path().join("collections").join(&result.collection);
        assert!(result.source_url.is_none() && result.source_id.is_none());
        assert!(!root.join(".rocket").exists());
    }

    #[test]
    fn a_linked_import_of_a_bad_file_creates_nothing() {
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let file = ws.path().join("notes.txt");
        std::fs::write(&file, "hello").expect("write");
        assert!(service
            .import_openapi_file_linked(
                &FsOpenApiSourceRepo,
                &FsOpenApiBaselineRepo,
                &file,
                GroupBy::Tags,
                "ws",
            )
            .is_err());
        assert!(
            !ws.path().join("collections").exists()
                || std::fs::read_dir(ws.path().join("collections"))
                    .expect("dir")
                    .count()
                    == 0
        );
    }

    #[tokio::test]
    async fn the_source_file_never_holds_a_query_string() {
        let spec = SAMPLE_SPEC.replace(
            "definitions.yaml#/parameters/sort",
            "definitions.yaml?token=SECRET#/parameters/sort",
        );
        let ws = TempDir::new().expect("tempdir");
        let service = ImportService::new_with_workspace_path(ws.path());
        let fetcher = MapFetcher::new(vec![
            (SAMPLE_RAW.to_string(), spec, None),
            (DEFS_URL.to_string(), DEFS.to_string(), None),
        ]);
        let result = service
            .import_openapi_url(
                &fetcher,
                &FsOpenApiSourceRepo,
                &FsOpenApiBaselineRepo,
                SAMPLE_LINK,
                GroupBy::Tags,
                None,
                "ws",
            )
            .await
            .expect("import");
        let root = ws.path().join("collections").join(&result.collection);
        let bytes = std::fs::read_to_string(root.join(".rocket/openapi-source.yml"))
            .expect("source file bytes");
        assert!(!bytes.contains("SECRET") && !bytes.contains('?'), "{bytes}");
        assert!(
            result.warnings.iter().all(|w| !w.contains("SECRET")),
            "{:?}",
            result.warnings
        );
        assert!(!result
            .report
            .skipped
            .iter()
            .any(|i| format!("{:?}", i.reason).contains("SECRET")));
        // A deterministic skip does not make the bundle incomplete.
        let source = FsOpenApiSourceRepo
            .load(&root)
            .expect("load")
            .expect("source");
        assert!(!source.remote_refs_incomplete);
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

    /// Serves one file, or a failure, and records the paths it was asked for.
    struct FakeReader {
        reply: Result<Vec<u8>, rocket_http::LocalReadFailure>,
        asked: Mutex<Vec<std::path::PathBuf>>,
    }

    impl rocket_http::LocalSpecReader for FakeReader {
        fn read(
            &self,
            path: &Path,
            _max_bytes: usize,
        ) -> Result<Vec<u8>, rocket_http::LocalReadFailure> {
            self.asked.lock().expect("lock").push(path.to_path_buf());
            self.reply.clone()
        }
    }

    fn reader(reply: Result<Vec<u8>, rocket_http::LocalReadFailure>) -> FakeReader {
        FakeReader {
            reply,
            asked: Mutex::new(Vec::new()),
        }
    }

    #[tokio::test]
    async fn a_file_link_is_read_through_the_reader_and_never_fetched() {
        let fetcher = FakeFetcher::new(Err(SpecFetchFailure::Network("must not be called".into())));
        let reader = reader(Ok(fixture().into_bytes()));
        let sources = SpecSources {
            fetcher: &fetcher,
            reader: &reader,
        };
        let fetched =
            fetch_spec_bundle_from(&sources, Path::new("/work/shop"), "specs/api.json", None)
                .await
                .expect("fetched");
        assert_eq!(
            fetched.location,
            SpecLocation::File {
                path: "specs/api.json".to_string()
            }
        );
        assert_eq!(fetched.display_url, "file://specs/api.json");
        assert!(fetched.etag.is_none() && fetched.last_modified.is_none());
        assert_eq!(
            reader.asked.lock().expect("lock").as_slice(),
            [std::path::PathBuf::from("/work/shop/specs/api.json")]
        );
        assert!(fetcher.last.lock().expect("lock").is_none());
    }

    #[tokio::test]
    async fn a_token_is_refused_for_a_file_link() {
        let fetcher = FakeFetcher::new(Err(SpecFetchFailure::NotFound));
        let reader = reader(Ok(fixture().into_bytes()));
        let sources = SpecSources {
            fetcher: &fetcher,
            reader: &reader,
        };
        let err =
            fetch_spec_bundle_from(&sources, Path::new("/work/shop"), "api.json", Some("tok"))
                .await
                .err()
                .expect("refused");
        assert!(err.to_string().contains("GitHub"), "{err}");
        assert!(reader.asked.lock().expect("lock").is_empty());
    }

    #[tokio::test]
    async fn unreadable_files_give_fixed_sentences_without_a_path() {
        let fetcher = FakeFetcher::new(Err(SpecFetchFailure::NotFound));
        for (failure, want) in [
            (
                rocket_http::LocalReadFailure::NotFound,
                "The spec file was not found on this computer.",
            ),
            (
                rocket_http::LocalReadFailure::NotAFile,
                "The path is not a file.",
            ),
            (
                rocket_http::LocalReadFailure::Io,
                "The spec file could not be read.",
            ),
        ] {
            let reader = reader(Err(failure));
            let sources = SpecSources {
                fetcher: &fetcher,
                reader: &reader,
            };
            let err =
                fetch_spec_bundle_from(&sources, Path::new("/work/shop"), "secret/api.json", None)
                    .await
                    .err()
                    .expect("failed");
            let text = err.to_string();
            assert!(text.contains(want), "{text}");
            assert!(!text.contains("secret"), "{text}");
        }
    }

    #[tokio::test]
    async fn a_link_still_goes_to_the_fetcher() {
        let fetcher = FakeFetcher::body(&fixture(), Some("\"e1\""));
        let reader = reader(Err(rocket_http::LocalReadFailure::Io));
        let sources = SpecSources {
            fetcher: &fetcher,
            reader: &reader,
        };
        let fetched = fetch_spec_bundle_from(
            &sources,
            Path::new("/work/shop"),
            "https://example.com/api.json",
            None,
        )
        .await
        .expect("fetched");
        assert_eq!(fetched.etag.as_deref(), Some("\"e1\""));
        assert!(reader.asked.lock().expect("lock").is_empty());
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
