//! The public types of an apply: the selection a user approved, the inputs and the output.

use super::merge::Choice;
use super::plan::SyncInputs;
use rocket_collection::openapi_baseline::OpenApiBaseline;
use rocket_collection::openapi_local::OpenApiLocalState;
use rocket_collection::openapi_source::OpenApiSource;
use rocket_collection::{CollectionSettings, Request};
use rocket_shared::error::DomainResult;
use std::collections::BTreeMap;

/// Renders files the way the repository writes them. The real one lives in `rocket-infra`.
pub trait RequestRenderer {
    /// The exact bytes the repository writes for a request at `request_path` (no extension).
    fn render(&self, request_path: &str, request: &Request) -> DomainResult<Vec<u8>>;
    /// The relative path of the collection settings file and its bytes.
    fn render_settings(&self, settings: &CollectionSettings) -> DomainResult<(String, Vec<u8>)>;
    /// The bytes of a new `folder.yml` for the folder at `folder_path`.
    fn render_folder(&self, folder_path: &str) -> DomainResult<Vec<u8>>;
    /// The round-trip guard: true when the request file at `request_path` holds nothing but
    /// what Rocket models, so rewriting it loses nothing.
    fn can_rewrite(&self, request_path: &str, request: &Request) -> DomainResult<bool>;
}

/// What the user approved for one item. `digest` is `item_digest` of the item they saw.
#[derive(Debug, Clone, PartialEq)]
pub struct ApprovedItem {
    pub key: String,
    pub digest: String,
    pub choices: BTreeMap<String, Choice>,
}

/// An item the user chose to skip at the fingerprint they saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkipRequest {
    pub key: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Selection {
    pub items: Vec<ApprovedItem>,
    pub skips: Vec<SkipRequest>,
    /// Names of the variables to apply.
    pub variables: Vec<String>,
    /// `(old key, new key)` pairs the user linked as renames.
    pub rename_links: Vec<(String, String)>,
}

pub struct ApplyInputs<'a> {
    pub plan_inputs: SyncInputs<'a>,
    /// Hex SHA-256 of the pending spec the plan was built from.
    pub pending_sha256: String,
    pub selection: &'a Selection,
    pub renderer: &'a dyn RequestRenderer,
    /// Every file of the collection as a path relative to its root, requests and `folder.yml`.
    pub existing_paths: &'a std::collections::HashSet<String>,
    pub settings: &'a CollectionSettings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChangeKind {
    Put(Vec<u8>),
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub rel_path: String,
    pub kind: FileChangeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppliedKind {
    Created,
    Updated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedItem {
    pub key: String,
    pub kind: AppliedKind,
    /// The request path (no extension) after the apply.
    pub request_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ApplyReport {
    pub applied: Vec<AppliedItem>,
    /// Keys the user approved that no longer match the plan, or links that were refused.
    pub stale: Vec<String>,
    /// Keys left alone because their file holds more than Rocket models.
    pub not_rewritable: Vec<String>,
    pub deleted: Vec<String>,
    pub detached: Vec<String>,
    pub skipped: usize,
    /// Plan items that were neither applied nor skipped nor stale.
    pub remaining: usize,
}

/// The files to write and the new state. The source, baseline, local and pending files are
/// staged by the caller from the returned structs.
#[derive(Debug, Clone)]
pub struct ApplyOutput {
    pub files: Vec<FileChange>,
    pub source: OpenApiSource,
    pub baseline: OpenApiBaseline,
    pub local: OpenApiLocalState,
    /// True when nothing is left to review. The caller then removes the pending copy.
    pub finalized: bool,
    pub report: ApplyReport,
}

