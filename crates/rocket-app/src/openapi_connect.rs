//! Connecting a hand-made collection to an OpenAPI spec. `preview` fetches the spec at the
//! link the user typed, matches its operations to the requests and writes nothing.

use rocket_collection::CollectionRepository;
use rocket_import::{
    fetch_spec_bundle, match_requests, ImportError, MatchRequest, ParsedSpec, SpecOperationView,
};
use rocket_shared::error::{DomainError, DomainResult};

use crate::collection_lock::collection_lock;
use crate::openapi_sync_apply::flatten_requests;
use crate::openapi_sync_service::{sha256_hex, OpenApiSyncService};

/// A request of the collection as the Connect screens show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectRequestRow {
    pub uid: String,
    pub request_path: String,
    pub method: String,
    pub url: String,
    pub name: String,
}

/// An operation of the spec as the Connect screens show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectOperationRow {
    pub key: String,
    pub method: String,
    pub path: String,
    pub summary: Option<String>,
    pub tag: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectPair {
    pub request: ConnectRequestRow,
    pub operation: ConnectOperationRow,
}

/// Requests and operations that all match each other, for the user to sort out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectAmbiguous {
    pub requests: Vec<ConnectRequestRow>,
    pub operations: Vec<ConnectOperationRow>,
}

/// What a spec link would connect to, before anything is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectPreview {
    pub spec_title: String,
    pub spec_version: Option<String>,
    /// `request_digest` of the collection as the preview saw it. Confirm sends it back.
    pub digest: String,
    pub matched: Vec<ConnectPair>,
    pub suggested: Vec<ConnectPair>,
    pub in_spec_only: Vec<ConnectOperationRow>,
    pub yours_only: Vec<ConnectRequestRow>,
    pub ambiguous: Vec<ConnectAmbiguous>,
}

const ALREADY_CONNECTED: &str = "This collection is already connected to an OpenAPI spec.";
const NEEDS_LINK: &str = "Enter the spec link.";
const NOT_A_SPEC: &str = "The file is not a supported OpenAPI spec.";
const COULD_NOT_LOAD: &str = "The spec could not be loaded.";

/// A fixed sentence for a failed fetch or bundle. The fetch messages are already fixed, and
/// anything else (a parse error with a path, say) becomes a general sentence.
fn safe_import_message(error: ImportError) -> String {
    match error {
        ImportError::SpecUnavailable(message) => message,
        ImportError::NotAnOpenApiSpec(_) => NOT_A_SPEC.to_string(),
        _ => COULD_NOT_LOAD.to_string(),
    }
}

/// A hash over the uid, path, method and URL of every HTTP request, in a fixed order. Confirm
/// compares it with the preview's, so a collection edited in between asks for a new preview.
pub fn request_digest(requests: &[MatchRequest]) -> String {
    let mut lines: Vec<String> = requests
        .iter()
        .filter(|r| r.http)
        .map(|r| format!("{}\0{}\0{}\0{}", r.uid, r.request_path, r.method, r.url))
        .collect();
    lines.sort();
    sha256_hex(lines.join("\n").as_bytes())
}

pub(crate) fn match_requests_of(
    repo: &dyn CollectionRepository,
    collection: &str,
) -> DomainResult<Vec<MatchRequest>> {
    Ok(flatten_requests(&repo.get(collection)?.root)
        .into_iter()
        .map(|c| MatchRequest {
            uid: c.uid,
            request_path: c.request_path,
            method: c.request.method.to_string(),
            url: c.request.url,
            name: c.request.name,
            http: true,
        })
        .collect())
}

fn request_row(r: &MatchRequest) -> ConnectRequestRow {
    ConnectRequestRow {
        uid: r.uid.clone(),
        request_path: r.request_path.clone(),
        method: r.method.to_uppercase(),
        url: r.url.clone(),
        name: r.name.clone(),
    }
}

fn operation_row(o: &SpecOperationView) -> ConnectOperationRow {
    ConnectOperationRow {
        key: o.key.clone(),
        method: o.method.clone(),
        path: o.path.clone(),
        summary: o.summary.clone(),
        tag: o.tag.clone(),
    }
}

impl OpenApiSyncService {
    /// Fetches the spec at `link` and matches it to the requests of `collection`. The link and
    /// the optional token are the ones the user typed: no consent is granted, no token is
    /// stored and nothing is written.
    pub async fn preview(
        &self,
        collection: &str,
        link: &str,
        token: Option<&str>,
    ) -> DomainResult<ConnectPreview> {
        let link = link.trim();
        if link.is_empty() {
            return Err(DomainError::InvalidInput(NEEDS_LINK.to_string()));
        }
        let deps = self.deps()?;
        let pinned = deps.collections.pinned();
        let repo: &dyn CollectionRepository =
            pinned.as_deref().unwrap_or(deps.collections.as_ref());
        let root = repo.collection_root_path(collection)?;
        let requests = {
            let lock = collection_lock(&root);
            let _guard = lock.lock();
            if self.sources.load(&root)?.is_some() {
                return Err(DomainError::Conflict(ALREADY_CONNECTED.to_string()));
            }
            match_requests_of(repo, collection)?
        };
        let fetched = fetch_spec_bundle(self.fetcher.as_ref(), link, token)
            .await
            .map_err(|e| DomainError::InvalidInput(safe_import_message(e)))?;
        let spec = ParsedSpec::parse(&fetched.bundle.text)
            .map_err(|_| DomainError::InvalidInput(NOT_A_SPEC.to_string()))?;
        let views = spec.operation_views();
        let result = match_requests(&requests, &views, spec.servers());

        let req = |uid: &str| requests.iter().find(|r| r.uid == uid).map(request_row);
        let op = |key: &str| views.iter().find(|o| o.key == key).map(operation_row);
        let pairs = |list: &[rocket_import::MatchPair]| -> Vec<ConnectPair> {
            list.iter()
                .filter_map(|p| {
                    Some(ConnectPair {
                        request: req(&p.uid)?,
                        operation: op(&p.key)?,
                    })
                })
                .collect()
        };
        Ok(ConnectPreview {
            spec_title: spec.title().to_string(),
            spec_version: spec.version().map(str::to_string),
            digest: request_digest(&requests),
            matched: pairs(&result.matched),
            suggested: pairs(&result.suggested),
            in_spec_only: result.in_spec_only.iter().filter_map(|k| op(k)).collect(),
            yours_only: result.yours_only.iter().filter_map(|u| req(u)).collect(),
            ambiguous: result
                .ambiguous
                .iter()
                .map(|g| ConnectAmbiguous {
                    requests: g.uids.iter().filter_map(|u| req(u)).collect(),
                    operations: g.keys.iter().filter_map(|k| op(k)).collect(),
                })
                .collect(),
        })
    }
}
