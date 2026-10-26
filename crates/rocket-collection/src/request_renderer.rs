//! The port that renders collection files the way the repository writes them. The real one
//! lives in `rocket-infra`.

use crate::{CollectionSettings, Request};
use rocket_shared::error::DomainResult;

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
