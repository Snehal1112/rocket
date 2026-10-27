//! The read-only spec viewer model: the latest stored spec, grouped by tag. It reads local
//! files only, so it never needs consent, a token or the network.

use std::collections::HashSet;

use rocket_collection::CollectionRepository;
use rocket_import::ParsedSpec;
use rocket_shared::error::{DomainError, DomainResult};

use crate::collection_lock::collection_lock;
use crate::openapi_sync_service::OpenApiSyncService;
use crate::openapi_sync_view::{SpecOpRow, SpecTagGroup, SpecView, SpecViewState};

impl OpenApiSyncService {
    /// The bytes of the latest stored spec and whether they are the pending copy. A pending
    /// file counts only when the source holds its hash, since a leftover file is harmless.
    /// `spec_view` calls it under the collection lock.
    pub(crate) fn latest_spec_bytes(
        &self,
        root: &std::path::Path,
        source: &rocket_collection::openapi_source::OpenApiSource,
    ) -> DomainResult<Option<(Vec<u8>, bool)>> {
        if source.pending_sha256.is_some() && self.pending_is_sound(root, source) {
            if let Some(bytes) = self.sources.load_pending(root)? {
                return Ok(Some((bytes, true)));
            }
        }
        Ok(self.sources.load_applied(root)?.map(|bytes| (bytes, false)))
    }

    /// The latest stored spec: the pending copy when one waits, else the applied copy. The
    /// files are read under the collection lock, so an apply never shows half a state.
    pub fn spec_view(&self, collection: &str) -> DomainResult<SpecView> {
        let deps = self.deps()?;
        let pinned = deps.collections.pinned();
        let repo: &dyn CollectionRepository =
            pinned.as_deref().unwrap_or(deps.collections.as_ref());
        let root = repo.collection_root_path(collection)?;
        let lock = collection_lock(&root);
        let _guard = lock.lock();
        let source = self.load(&root)?;
        let (bytes, state) = match self.latest_spec_bytes(&root, &source)? {
            Some((bytes, true)) => (bytes, SpecViewState::UpdatesWaiting),
            Some((bytes, false)) => (bytes, SpecViewState::InSync),
            None => return Ok(no_copy()),
        };
        let spec = String::from_utf8(bytes)
            .ok()
            .and_then(|text| ParsedSpec::parse(&text).ok())
            .ok_or_else(|| {
                DomainError::InvalidInput("The saved spec cannot be shown.".to_string())
            })?;
        let known: HashSet<&str> = source.operations.iter().map(|o| o.key.as_str()).collect();
        let waiting = state == SpecViewState::UpdatesWaiting;
        let mut groups: Vec<SpecTagGroup> = Vec::new();
        for op in spec.operation_views() {
            let row = SpecOpRow {
                is_new: waiting && !known.contains(op.key.as_str()),
                key: op.key,
                method: op.method,
                path: op.path,
                summary: op.summary,
                parameters: op.parameters,
                responses: op.responses,
            };
            match groups.iter_mut().find(|g| g.tag == op.tag) {
                Some(group) => group.operations.push(row),
                None => groups.push(SpecTagGroup {
                    tag: op.tag,
                    operations: vec![row],
                }),
            }
        }
        let local = self.local_state(collection)?;
        Ok(SpecView {
            title: spec.title().to_string(),
            version: spec.version().map(str::to_string),
            fetched_at: local.last_checked_at.or(source.last_checked_at),
            state,
            groups,
        })
    }
}

fn no_copy() -> SpecView {
    SpecView {
        title: String::new(),
        version: None,
        fetched_at: None,
        state: SpecViewState::NoCopy,
        groups: Vec::new(),
    }
}
