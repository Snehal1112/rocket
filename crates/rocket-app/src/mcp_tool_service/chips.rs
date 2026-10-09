//! Context chips for the assistant composer.
//!
//! The text of a chip is built here from the same masked views the MCP read tools return, so
//! the composer never builds or masks credential text itself. These methods need no session:
//! the only scope check is that the collection belongs to the active workspace.

use std::collections::HashSet;

use rocket_collection::Request;
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::types::HttpMethod;

use super::McpToolService;
use crate::assistant_chip_text::{
    cap_text, chip_uri, render_collection, render_environment, render_folder, render_request,
    render_response, ChipKind, ChipResource, ResponseChipInput, CHIP_TEXT_LIMIT_BYTES,
};
use crate::mcp_read_views::{
    basic_header_values_from_secrets, literal_credential_values, mask_secret_text,
    MaskedEnvironment, MaskedFolderSettings, MaskedRequest, MaskedSettings,
};

/// Rejects a path that is absolute or leaves the collection.
fn check_relative_path(path: &str) -> DomainResult<()> {
    let bad = path.is_empty()
        || path.contains('\0')
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.split(['/', '\\']).any(|segment| segment == "..");
    if bad {
        Err(DomainError::InvalidInput("invalid path".to_string()))
    } else {
        Ok(())
    }
}

impl McpToolService {
    /// The text resource for a request, folder, collection or environment chip.
    ///
    /// `path` is the request or folder path inside the collection, or the environment name.
    /// The text comes from the masked read views, gets a last pass with the known secret
    /// values, and is capped at `CHIP_TEXT_LIMIT_BYTES`.
    pub fn build_chip_resource(
        &self,
        kind: ChipKind,
        collection: &str,
        path: Option<&str>,
    ) -> DomainResult<ChipResource> {
        self.check_collection_listed(collection)?;
        let (text, secrets) = match kind {
            ChipKind::Collection => {
                let settings = self.collection_repo.get_settings(collection)?;
                let text = render_collection(
                    collection,
                    &MaskedSettings::from_settings(&settings),
                    settings.docs.as_deref(),
                );
                (text, self.chip_secrets(collection, None, None))
            }
            ChipKind::Folder => {
                let path = path.ok_or_else(|| DomainError::InvalidInput("missing path".into()))?;
                check_relative_path(path)?;
                let settings = self.collection_repo.get_folder_settings(collection, path)?;
                let text = render_folder(
                    collection,
                    path,
                    &MaskedFolderSettings::from_settings(&settings),
                );
                (text, self.chip_secrets(collection, None, None))
            }
            ChipKind::Environment => {
                let name = path.ok_or_else(|| DomainError::InvalidInput("missing path".into()))?;
                Self::validate_environment_name(name)?;
                let env = self
                    .environment_repo_factory
                    .for_collection(collection)
                    .get(name)?;
                let text =
                    render_environment(collection, &MaskedEnvironment::from_environment(&env));
                (text, self.chip_secrets(collection, None, None))
            }
            ChipKind::Request => {
                let path = path.ok_or_else(|| DomainError::InvalidInput("missing path".into()))?;
                check_relative_path(path)?;
                let request = self.read_request(collection, path)?;
                let docs = request.docs.as_ref().and_then(|docs| docs.content());
                let text = render_request(
                    collection,
                    &MaskedRequest::from_request(path, &request),
                    docs,
                );
                (
                    text,
                    self.chip_secrets(collection, Some(path), Some(&request)),
                )
            }
        };
        // The views mask by name and shape. This pass also covers a known secret that sits in
        // free text, such as a script or a raw body.
        let text = cap_text(&mask_secret_text(&text, &secrets), CHIP_TEXT_LIMIT_BYTES);
        Ok(ChipResource {
            uri: chip_uri(kind.as_str(), collection, path),
            text,
        })
    }

    /// The text resource for a request's last response, which lives in the frontend.
    ///
    /// The status line, headers, URL and test errors are masked by name and shape. Then every
    /// secret that applies to the request is masked over the whole text: secret variables of
    /// the global environment, the collection, the folders, the request and every environment
    /// of the collection, plus the request's literal credentials and the `Basic` header values
    /// built from them. This is the masking `run_request` applies to a response body.
    /// Secrets that only RocketVault holds are not known here.
    pub fn mask_response_chip(
        &self,
        collection: &str,
        request_path: &str,
        input: &ResponseChipInput,
    ) -> DomainResult<ChipResource> {
        self.check_collection_listed(collection)?;
        check_relative_path(request_path)?;
        let request = self.read_request(collection, request_path).ok();
        let title = request
            .as_ref()
            .map_or_else(|| request_path.to_string(), |r| r.name.clone());
        let secrets = self.chip_secrets(collection, Some(request_path), request.as_ref());
        let text = mask_secret_text(&render_response(&title, input), &secrets);
        Ok(ChipResource {
            uri: chip_uri("last-response", collection, Some(request_path)),
            text: cap_text(&text, CHIP_TEXT_LIMIT_BYTES),
        })
    }

    /// The secret values to mask in a chip of `collection`. With a request path the request's
    /// own scopes and literal credentials are included.
    fn chip_secrets(
        &self,
        collection: &str,
        request_path: Option<&str>,
        request: Option<&Request>,
    ) -> HashSet<String> {
        let workspace_path = self
            .active_workspace_path
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let global = self
            .config_repo
            .load(&workspace_path)
            .ok()
            .and_then(|config| config.global_environment);
        // A response can echo a secret of any environment, not only the selected one.
        let mut variable_secrets: HashSet<String> = self.execution_svc.secret_values_for_request(
            global.as_deref(),
            collection,
            None,
            request_path,
        );
        let environments = self
            .environment_repo_factory
            .for_collection(collection)
            .list()
            .unwrap_or_default();
        for env in &environments {
            variable_secrets.extend(
                env.variables
                    .iter()
                    .filter(|variable| variable.secret)
                    .map(|variable| variable.value.clone()),
            );
        }
        let mut secrets = variable_secrets.clone();
        let fallback;
        let request = match (request, request_path) {
            (Some(request), _) => Some(request),
            (None, Some(_)) => {
                fallback = Request::new("", HttpMethod::Get, "");
                Some(&fallback)
            }
            (None, None) => None,
        };
        if let (Some(request), Some(path)) = (request, request_path) {
            let settings = self.collection_repo.get_settings(collection).ok();
            let folders = self
                .collection_repo
                .get_folder_chain_settings(collection, path)
                .unwrap_or_default();
            secrets.extend(literal_credential_values(
                request,
                settings.as_ref(),
                &folders,
            ));
            secrets.extend(basic_header_values_from_secrets(
                request,
                settings.as_ref(),
                &folders,
                &variable_secrets,
            ));
        }
        secrets
    }
}
