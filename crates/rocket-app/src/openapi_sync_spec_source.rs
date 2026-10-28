//! The raw text of the latest stored spec, for the read-only editor of the spec viewer. It reads
//! local files only, so it never needs consent, a token or the network.

use rocket_collection::CollectionRepository;
use rocket_shared::error::{DomainError, DomainResult};

use crate::collection_lock::collection_lock;
use crate::openapi_sync_service::OpenApiSyncService;
use crate::openapi_sync_view::{SpecSource, SpecSourceFormat};

/// The largest spec copy whose text is sent to the page.
pub const MAX_SOURCE_BYTES: usize = 12 * 1024 * 1024;

const PENDING_COPY_MISSING: &str =
    "A newer spec is waiting but its copy is missing. Run Check now.";

impl OpenApiSyncService {
    /// The text of the latest stored spec: the pending copy when one waits, else the applied copy.
    pub fn spec_source(&self, collection: &str) -> DomainResult<SpecSource> {
        self.spec_source_capped(collection, MAX_SOURCE_BYTES)
    }

    /// Same as `spec_source` with a custom size cap, so a test needs no huge file.
    pub(crate) fn spec_source_capped(
        &self,
        collection: &str,
        max_bytes: usize,
    ) -> DomainResult<SpecSource> {
        let deps = self.deps()?;
        let pinned = deps.collections.pinned();
        let repo: &dyn CollectionRepository =
            pinned.as_deref().unwrap_or(deps.collections.as_ref());
        let root = repo.collection_root_path(collection)?;
        let bytes = {
            let lock = collection_lock(&root);
            let _guard = lock.lock();
            let source = self.load(&root)?;
            // Same guard as the viewer: a waiting spec without its copy must not look in sync.
            if source.pending_sha256.is_some() && !self.pending_is_sound(&root, &source) {
                return Err(DomainError::InvalidInput(PENDING_COPY_MISSING.to_string()));
            }
            match self.latest_spec_bytes(&root, &source)? {
                Some((bytes, _)) => bytes,
                None => {
                    return Ok(SpecSource {
                        text: None,
                        format: SpecSourceFormat::Yaml,
                        size_bytes: 0,
                        too_large: false,
                    })
                }
            }
        };
        let size_bytes = bytes.len() as u64;
        if bytes.len() > max_bytes {
            return Ok(SpecSource {
                text: None,
                format: SpecSourceFormat::Yaml,
                size_bytes,
                too_large: true,
            });
        }
        let mut text = String::from_utf8(bytes).map_err(|_| {
            DomainError::InvalidInput("The saved spec cannot be shown.".to_string())
        })?;
        // A leading BOM would show as a stray char in the editor and hide the JSON brace.
        if text.starts_with('\u{feff}') {
            text.remove(0);
        }
        let format = if text.trim_start().starts_with('{') {
            SpecSourceFormat::Json
        } else {
            SpecSourceFormat::Yaml
        };
        Ok(SpecSource {
            text: Some(text),
            format,
            size_bytes,
            too_large: false,
        })
    }
}
