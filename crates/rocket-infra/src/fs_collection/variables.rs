use std::fs;

use rocket_collection::{Collection, CollectionVariable};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;
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
        let Ok(content) = fs::read_to_string(&folder_yml) else {
            continue;
        };
        let Ok(oc_folder) = parse_folder_yml(&content) else {
            continue;
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
    let oc_vars: Vec<OcVariable> = vars.into_iter().map(OcVariable::from).collect();
    // Only `request.variables` changes. Docs, scripts and auth stay exactly as they
    // are on disk, and a folder directory that does not exist is still created.
    edit_folder_yml(repo, collection, folder_path, false, move |oc_folder| {
        let req_defaults = oc_folder.request.take().unwrap_or_default();
        oc_folder.request = Some(OcRequestDefaults {
            variables: if oc_vars.is_empty() {
                None
            } else {
                Some(oc_vars)
            },
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
    let content = fs::read_to_string(&file_path)?;
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
