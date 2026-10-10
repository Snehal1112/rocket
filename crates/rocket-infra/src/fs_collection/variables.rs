use std::fs;

use rocket_collection::{generate_uid, Collection, CollectionVariable, RequestScriptPhase};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;
use crate::conversions::folder_oc_variables;
use crate::conversions::{oc_http_request_to_request, request_to_oc_http_request};
use crate::oc::{
    OcGraphQLRequest, OcGraphQLRequestRuntime, OcHttpRequest, OcGrpcRequest,
    OcHttpRequestRuntime, OcRequestDefaults, OcVariable, OcWebSocketRequest,
};

use super::folder_file::parse_folder_yml;
use super::folder_settings::{edit_folder_yml, get_folder_settings};
use super::paths::resolve_request_path;
use super::FsCollectionRepo;

pub(super) fn get_folder_chain_variables(
    repo: &FsCollectionRepo,
    collection: &str,
    request_path: &str,
) -> DomainResult<Vec<CollectionVariable>> {
    folder_chain_variables(repo, collection, request_path, false)
}

/// The folder chain walk. With `strict` off a `folder.yml` that cannot be read or parsed is
/// skipped. With `strict` on it is an error. A missing `folder.yml` is always skipped.
pub(super) fn folder_chain_variables(
    repo: &FsCollectionRepo,
    collection: &str,
    request_path: &str,
    strict: bool,
) -> DomainResult<Vec<CollectionVariable>> {
    Collection::validate_name(collection)?;
    let collection_dir = repo.collection_path(collection);
    let path = std::path::Path::new(request_path);
    let dir_components: Vec<&str> = path
        .parent()
        .unwrap_or(std::path::Path::new(""))
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();

    // Root-level request — no ancestor folders to read.
    if dir_components.is_empty() {
        return Ok(Vec::new());
    }

    let _span = tracing::debug_span!(
        "get_folder_chain_variables",
        collection,
        request_path,
        depth = dir_components.len()
    )
    .entered();

    let mut chain: Vec<Vec<CollectionVariable>> = Vec::new();
    let mut current = collection_dir.clone();
    for segment in &dir_components {
        current = current.join(segment);
        let folder_yml = current.join("folder.yml");
        if !folder_yml.exists() {
            continue;
        }
        let content = match fs::read_to_string(&folder_yml) {
            Ok(content) => content,
            Err(e) if strict => return Err(e.into()),
            Err(_) => continue,
        };
        let oc_folder = match parse_folder_yml(&content) {
            Ok(oc_folder) => oc_folder,
            Err(e) if strict => {
                return Err(DomainError::Internal(format!("Failed to parse folder.yml: {e}")))
            }
            Err(_) => continue,
        };
        let Some(req) = oc_folder.request else {
            continue;
        };
        let Some(vars) = req.variables else {
            continue;
        };
        chain.push(vars.into_iter().map(CollectionVariable::from).collect());
    }
    Ok(rocket_collection::settings::merge_folder_chain_variables(
        chain,
    ))
}

pub(super) fn save_folder_variables(
    repo: &FsCollectionRepo,
    collection: &str,
    folder_path: &str,
    vars: Vec<CollectionVariable>,
) -> DomainResult<()> {
    // Only `request.variables` changes. Docs, scripts and auth stay exactly as they
    // are on disk, and a folder directory that does not exist is still created.
    edit_folder_yml(repo, collection, folder_path, false, move |oc_folder| {
        let req_defaults = oc_folder.request.take().unwrap_or_default();
        let oc_vars = folder_oc_variables(&vars, req_defaults.variables.as_deref());
        oc_folder.request = Some(OcRequestDefaults {
            variables: oc_vars,
            ..req_defaults
        });
    })
}

pub(super) fn get_folder_variables(
    repo: &FsCollectionRepo,
    collection: &str,
    folder_path: &str,
) -> DomainResult<Vec<CollectionVariable>> {
    Ok(get_folder_settings(repo, collection, folder_path)?.variables)
}

pub(super) fn get_request_variables(
    repo: &FsCollectionRepo,
    collection: &str,
    request_path: &str,
) -> DomainResult<Vec<CollectionVariable>> {
    Collection::validate_name(collection)?;
    let collection_dir = repo.collection_path(collection);
    let file_path = resolve_request_path(repo, &collection_dir, request_path)?;
    // A request file that does not exist (a tab not saved yet) is `NotFound`, so callers can
    // tell it apart from a real read failure.
    let content = match fs::read_to_string(&file_path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(DomainError::NotFound(format!("request '{request_path}'")))
        }
        Err(e) => return Err(e.into()),
    };
    let vars = runtime_variables_of(&content)?
        .into_iter()
        .map(CollectionVariable::from)
        .collect();
    Ok(vars)
}

/// Reads `runtime.variables` from an HTTP, GraphQL, WebSocket or gRPC request file.
fn runtime_variables_of(content: &str) -> DomainResult<Vec<OcVariable>> {
    let http_err = match serde_yaml::from_str::<OcHttpRequest>(content) {
        Ok(req) => return Ok(req.runtime.map(|r| r.variables).unwrap_or_default()),
        Err(e) => e,
    };
    if let Ok(g) = serde_yaml::from_str::<OcGraphQLRequest>(content) {
        return Ok(g.runtime.map(|r| r.variables).unwrap_or_default());
    }
    if let Ok(ws) = serde_yaml::from_str::<OcWebSocketRequest>(content) {
        return Ok(ws.runtime.map(|r| r.variables).unwrap_or_default());
    }
    if let Ok(g) = serde_yaml::from_str::<OcGrpcRequest>(content) {
        return Ok(g.runtime.map(|r| r.variables).unwrap_or_default());
    }
    // Keep the HTTP error: it is the precise one for a broken HTTP file.
    Err(DomainError::Internal(format!(
        "Failed to parse request file: {http_err}"
    )))
}

/// Returns the file content with `runtime.variables` replaced, for an HTTP, GraphQL, WebSocket or gRPC request file.
fn with_runtime_variables(content: &str, vars: Vec<OcVariable>) -> DomainResult<String> {
    let to_err = |e: serde_yaml::Error| {
        DomainError::Internal(format!("Failed to serialize request file: {e}"))
    };
    let http_err = match serde_yaml::from_str::<OcHttpRequest>(content) {
        Ok(mut req) => {
            let runtime = req.runtime.take().unwrap_or_default();
            req.runtime = Some(OcHttpRequestRuntime {
                variables: vars,
                ..runtime
            });
            return serde_yaml::to_string(&req).map_err(to_err);
        }
        Err(e) => e,
    };
    if let Ok(mut g) = serde_yaml::from_str::<OcGraphQLRequest>(content) {
        let runtime = g.runtime.take().unwrap_or_default();
        g.runtime = Some(OcGraphQLRequestRuntime {
            variables: vars,
            ..runtime
        });
        return serde_yaml::to_string(&g).map_err(to_err);
    }
    if let Ok(mut ws) = serde_yaml::from_str::<OcWebSocketRequest>(content) {
        let mut runtime = ws.runtime.take().unwrap_or_default();
        runtime.variables = vars;
        ws.runtime = Some(runtime);
        return serde_yaml::to_string(&ws).map_err(to_err);
    }
    if let Ok(mut g) = serde_yaml::from_str::<OcGrpcRequest>(content) {
        let mut runtime = g.runtime.take().unwrap_or_default();
        runtime.variables = vars;
        g.runtime = Some(runtime);
        return serde_yaml::to_string(&g).map_err(to_err);
    }
    Err(DomainError::Internal(format!(
        "Failed to parse request file: {http_err}"
    )))
}

pub(super) fn save_request_variables(
    repo: &FsCollectionRepo,
    collection: &str,
    request_path: &str,
    vars: Vec<CollectionVariable>,
) -> DomainResult<()> {
    Collection::validate_name(collection)?;
    let mutex = repo.collection_mutex(collection);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    let collection_dir = repo.collection_path(collection);
    let file_path = resolve_request_path(repo, &collection_dir, request_path)?;
    let content = fs::read_to_string(&file_path)?;
    let oc_vars: Vec<OcVariable> = vars.into_iter().map(OcVariable::from).collect();
    let yaml = with_runtime_variables(&content, oc_vars)?;
    atomic_write(&file_path, yaml.as_bytes())?;
    Ok(())
}

pub(super) fn save_request_script(
    repo: &FsCollectionRepo,
    collection: &str,
    request_path: &str,
    phase: RequestScriptPhase,
    body: String,
) -> DomainResult<()> {
    Collection::validate_name(collection)?;
    let mutex = repo.collection_mutex(collection);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    let collection_dir = repo.collection_path(collection);
    let file_path = resolve_request_path(repo, &collection_dir, request_path)?;
    let content = fs::read_to_string(&file_path)?;
    let oc: OcHttpRequest = serde_yaml::from_str(&content)
        .map_err(|e| DomainError::Internal(format!("Failed to parse request file: {e}")))?;

    // Round-trip through the domain Request rather than editing
    // runtime.scripts directly. The shared conversions already map the three
    // script fields to and from the OC YAML script list, so this avoids a
    // second copy of that mapping.
    let mut req = oc_http_request_to_request(oc);
    // A uid-less file would otherwise be written back with an empty uid.
    // Persist a real one instead, as save_request does.
    if req.uid.is_empty() {
        req.uid = generate_uid();
    }
    match phase {
        RequestScriptPhase::PreRequest => req.pre_request_script = Some(body),
        RequestScriptPhase::PostResponse => req.post_response_script = Some(body),
        RequestScriptPhase::Tests => req.tests = Some(body),
    }
    let oc = request_to_oc_http_request(&req);

    let yaml = serde_yaml::to_string(&oc)
        .map_err(|e| DomainError::Internal(format!("Failed to serialize request file: {e}")))?;
    atomic_write(&file_path, yaml.as_bytes())?;
    Ok(())
}
