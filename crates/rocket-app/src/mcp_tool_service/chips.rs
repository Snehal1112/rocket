//! Context chips for the assistant composer.
//!
//! The text of a chip is built here from the same masked views the MCP read tools return, so
//! the composer never builds or masks credential text itself. These methods need no session:
//! the only scope check is that the collection belongs to the active workspace.

use std::collections::HashSet;
use std::time::Duration;

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

/// How long the vault secrets of all environments may take to resolve, all together.
const VAULT_RESOLVE_DEADLINE: Duration = Duration::from_secs(8);

/// Whether `path` is absolute, has a drive prefix or leaves its base directory. Backslashes are
/// treated as separators, so a Windows-style path is refused on every platform.
pub(super) fn is_unsafe_relative_path(path: &str) -> bool {
    use std::path::{Component, Path};
    let normalized = path.replace('\\', "/");
    let bytes = normalized.as_bytes();
    let drive_prefix = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    path.is_empty()
        || path.contains('\0')
        || drive_prefix
        || Path::new(&normalized).components().any(|component| {
            matches!(
                component,
                Component::Prefix(_) | Component::RootDir | Component::ParentDir
            )
        })
}

/// Rejects a path that is absolute or leaves the collection.
fn check_relative_path(path: &str) -> DomainResult<()> {
    if is_unsafe_relative_path(path) {
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
                (text, self.chip_secrets(collection, None, &[])?.0)
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
                (text, self.chip_secrets(collection, None, &[])?.0)
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
                (text, self.chip_secrets(collection, None, &[])?.0)
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
                    self.chip_secrets(collection, Some(path), &[&request])?.0,
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
    /// of the collection, the RocketVault secrets of `environment_name`, and the
    /// literal credentials of both the saved request and the tab's request, plus the `Basic`
    /// header values built from them. This is the masking `run_request` applies to a response.
    /// Fails closed: if the environment has vault bindings that cannot be resolved (or take
    /// longer than 8 s in total), the chip is refused.
    /// Values that only exist at run time in the frontend, such as a script's `setVar`, are
    /// not known here.
    pub async fn mask_response_chip(
        &self,
        collection: &str,
        request_path: &str,
        environment_name: Option<&str>,
        input: &ResponseChipInput,
    ) -> DomainResult<ChipResource> {
        self.mask_response_chip_within(
            collection,
            request_path,
            environment_name,
            input,
            VAULT_RESOLVE_DEADLINE,
        )
        .await
    }

    /// `mask_response_chip` with an explicit deadline for the whole vault resolution.
    pub(super) async fn mask_response_chip_within(
        &self,
        collection: &str,
        request_path: &str,
        environment_name: Option<&str>,
        input: &ResponseChipInput,
        deadline: std::time::Duration,
    ) -> DomainResult<ChipResource> {
        self.check_collection_listed(collection)?;
        check_relative_path(request_path)?;
        if let Some(name) = environment_name {
            Self::validate_environment_name(name)?;
        }
        // A saved request that does not exist (an unsaved tab) is fine. Any other failure means
        // its literal credentials cannot be masked, so the chip is refused.
        let saved = match self.read_request(collection, request_path) {
            Ok(request) => Some(request),
            Err(DomainError::NotFound(_)) => None,
            Err(_) => {
                return Err(DomainError::InvalidInput(
                    "could not read the saved request to mask this response; try again".to_string(),
                ))
            }
        };
        let title = saved
            .as_ref()
            .map_or_else(|| request_path.to_string(), |r| r.name.clone());
        // The request as the tab shows it, which may hold unsaved credentials.
        let on_screen = input.request.as_ref().map(|tab| {
            let mut request = Request::new("", HttpMethod::Get, input.url.as_str());
            request.headers = tab.headers.clone();
            request.query_params = tab.query_params.clone();
            request.body = tab.body.clone();
            request.auth = tab.auth.clone();
            request
        });
        let requests: Vec<&Request> = saved.iter().chain(on_screen.iter()).collect();
        let (mut secrets, mut env_names) =
            self.chip_secrets(collection, Some(request_path), &requests)?;
        // Vault values are fetched on demand. The named environment is not trusted to be the
        // one that produced the response, so every environment of the collection is resolved.
        // If a value cannot be had, the response cannot be masked safely, so the chip fails
        // instead of going out with a secret in it.
        if let Some(name) = environment_name {
            if !env_names.iter().any(|known| known == name) {
                env_names.push(name.to_string());
            }
        }
        // The whole batch shares one deadline. Distinct fetches run concurrently.
        let resolve_all = self
            .execution_svc
            .external_secret_values_for_environments(collection, &env_names);
        match tokio::time::timeout(deadline, resolve_all).await {
            Ok(Ok(values)) => secrets.extend(values),
            _ => {
                return Err(DomainError::InvalidInput(
                    "could not resolve vault secrets to mask this response; try again".to_string(),
                ))
            }
        }
        let text = mask_secret_text(&render_response(&title, input), &secrets);
        Ok(ChipResource {
            uri: chip_uri("last-response", collection, Some(request_path)),
            text: cap_text(&text, CHIP_TEXT_LIMIT_BYTES),
        })
    }

    /// The secret values to mask in a chip of `collection`. With a request path the request's
    /// own scopes and literal credentials are included.
    ///
    /// Fails closed: a lookup that errors would leave its secrets unmasked, so the chip is
    /// refused instead. A missing file is not an error, the repositories return defaults then.
    /// Also returns the names of the collection's environments, read once here.
    ///
    /// Known residual: `FsEnvironmentRepo::list` skips an environment file that fails to parse
    /// (it only logs a warning), so the secrets of a corrupt environment file cannot be masked.
    fn chip_secrets(
        &self,
        collection: &str,
        request_path: Option<&str>,
        requests: &[&Request],
    ) -> DomainResult<(HashSet<String>, Vec<String>)> {
        let refuse = |what: &str| {
            DomainError::InvalidInput(format!(
                "could not read {what} to mask this chip; try again"
            ))
        };
        let workspace_path = self
            .active_workspace_path
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let global = self
            .config_repo
            .load(&workspace_path)
            .map_err(|_| refuse("the workspace config"))?
            .global_environment;
        // A response can echo a secret of any environment, not only the selected one.
        let mut variable_secrets: HashSet<String> = self
            .execution_svc
            .try_secret_values_for_request(global.as_deref(), collection, None, request_path)
            .map_err(|_| refuse("the variable scopes"))?;
        let environments = self
            .environment_repo_factory
            .for_collection(collection)
            .list()
            .map_err(|_| refuse("the environments"))?;
        for env in &environments {
            variable_secrets.extend(
                env.variables
                    .iter()
                    .filter(|variable| variable.secret)
                    .map(|variable| variable.value.clone()),
            );
        }
        let env_names: Vec<String> = environments.iter().map(|env| env.name.clone()).collect();
        let mut secrets = variable_secrets.clone();
        if let Some(path) = request_path {
            // The helpers below take `Option<&CollectionSettings>` because other callers have
            // none. Here the settings are always read, and a read error refuses the chip.
            let settings = Some(
                self.collection_repo
                    .get_settings(collection)
                    .map_err(|_| refuse("the collection settings"))?,
            );
            let folders = self
                .collection_repo
                .get_folder_chain_settings(collection, path)
                .map_err(|_| refuse("the folder settings"))?;
            // The collection's and folders' own credentials count even with no request.
            let placeholder;
            let requests: &[&Request] = if requests.is_empty() {
                placeholder = Request::new("", HttpMethod::Get, "");
                &[&placeholder]
            } else {
                requests
            };
            for request in requests {
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
        }
        Ok((secrets, env_names))
    }
}
