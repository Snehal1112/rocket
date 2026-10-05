use std::fs;
use std::path::Path;

use rocket_collection::{
    generate_uid, request_filename_for, Collection, GraphQlRequest, Request, RequestKind,
    WebSocketRequest,
};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;
use crate::conversions::{
    graphql_to_oc, oc_graphql_to_domain, oc_http_request_to_request, oc_websocket_to_request,
    request_to_oc_http_request, websocket_to_oc_websocket, with_file_identity,
};
use crate::oc::{OcGraphQLRequest, OcHttpRequest, OcWebSocketRequest};

use super::paths::resolve_request_path;
use super::FsCollectionRepo;

pub(super) fn get_request(
    repo: &FsCollectionRepo,
    collection: &str,
    path: &str,
) -> DomainResult<Request> {
    Collection::validate_name(collection)?;
    let collection_dir = repo.collection_path(collection);

    // Try .yml first, then .json for backward compatibility.
    let yml_path = if path.ends_with(".yml") || path.ends_with(".yaml") {
        path.to_string()
    } else {
        format!("{}.yml", path.strip_suffix(".json").unwrap_or(path))
    };

    // Try .yml first.
    if let Ok(file_path) = repo.validate_path(&collection_dir, Path::new(&yml_path)) {
        if file_path.exists()
            && file_path
                .extension()
                .is_some_and(|e| e == "yml" || e == "yaml")
        {
            let content = fs::read_to_string(&file_path)?;
            let oc: OcHttpRequest = serde_yaml::from_str(&content)
                .map_err(|e| DomainError::Internal(format!("Failed to parse YAML request: {e}")))?;
            let mut req = oc_http_request_to_request(oc);
            req.file_name = file_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string());
            // Give a uid-less file an in-memory uid only. This is a pure read, so the
            // file is not rewritten; the uid is persisted by the next save_request.
            if req.uid.is_empty() {
                req.uid = generate_uid();
            }
            return Ok(req);
        }
    }

    // Fall back to .json for legacy files.
    let json_path = if path.ends_with(".json") {
        path.to_string()
    } else {
        format!("{}.json", path)
    };
    let file_path = repo
        .validate_path(&collection_dir, Path::new(&json_path))
        .or_else(|_| repo.validate_path(&collection_dir, Path::new(path)))?;
    if !file_path.exists() {
        return Err(DomainError::NotFound(format!("{}/{}", collection, path)));
    }
    let content = fs::read_to_string(&file_path)?;
    Ok(serde_json::from_str(&content)?)
}

#[tracing::instrument(name = "collection_save_request", skip(repo, request), fields(collection_name = %collection, request_path = %path))]
pub(super) fn save_request(
    repo: &FsCollectionRepo,
    collection: &str,
    path: &str,
    request: &Request,
) -> DomainResult<String> {
    Collection::validate_name(collection)?;
    let mutex = repo.collection_mutex(collection);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    if request.uid.is_empty() {
        return Err(DomainError::Internal(format!(
            "save_request: empty uid on request for '{path}' in collection '{collection}'; callers must construct via Request::new()"
        )));
    }

    let collection_dir = repo.collection_path(collection);
    let normalized = request_filename_for(path);
    let file_path = repo.validate_path(&collection_dir, Path::new(&normalized))?;

    let mut oc = request_to_oc_http_request(request);

    // Preserve variables stored by save_request_variables: the IPC payload
    // does not carry them, so they would otherwise be silently erased.
    if request.variables.is_empty() && file_path.exists() {
        if let Ok(existing_content) = fs::read_to_string(&file_path) {
            if let Ok(existing_oc) = serde_yaml::from_str::<OcHttpRequest>(&existing_content) {
                if let Some(existing_runtime) = existing_oc.runtime {
                    if !existing_runtime.variables.is_empty() {
                        let runtime = oc.runtime.get_or_insert_with(Default::default);
                        runtime.variables = existing_runtime.variables;
                    }
                }
            }
        }
    }

    let yaml = serde_yaml::to_string(&oc)
        .map_err(|e| DomainError::Internal(format!("Failed to serialize request YAML: {e}")))?;

    atomic_write(&file_path, yaml.as_bytes())?;

    // Return the actual filename relative to the collection directory.
    let actual = file_path
        .strip_prefix(&collection_dir)
        .unwrap_or(&file_path)
        .to_string_lossy()
        .to_string();
    Ok(actual)
}

pub(super) fn rename_request(
    repo: &FsCollectionRepo,
    collection: &str,
    old_path: &str,
    new_path: &str,
) -> DomainResult<()> {
    Collection::validate_name(collection)?;
    let mutex = repo.collection_mutex(collection);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    let collection_dir = repo.collection_path(collection);
    let old_file = resolve_request_path(repo, &collection_dir, old_path)?;
    let new_ext =
        if new_path.ends_with(".yml") || new_path.ends_with(".yaml") || new_path.ends_with(".json")
        {
            new_path.to_string()
        } else {
            format!("{}.yml", new_path)
        };
    let new_file = repo.validate_path(&collection_dir, Path::new(&new_ext))?;
    fs::rename(&old_file, &new_file)?;
    Ok(())
}

pub(super) fn delete_request(
    repo: &FsCollectionRepo,
    collection: &str,
    path: &str,
) -> DomainResult<()> {
    Collection::validate_name(collection)?;
    let collection_dir = repo.collection_path(collection);
    let file_path = resolve_request_path(repo, &collection_dir, path)?;
    if !file_path.exists() {
        return Err(DomainError::NotFound(format!("{}/{}", collection, path)));
    }
    fs::remove_file(&file_path)?;
    Ok(())
}

pub(super) fn get_graphql_request(
    repo: &FsCollectionRepo,
    collection: &str,
    path: &str,
) -> DomainResult<GraphQlRequest> {
    Collection::validate_name(collection)?;
    let collection_dir = repo.collection_path(collection);
    let file_path = resolve_request_path(repo, &collection_dir, path)?;
    if !file_path.exists() {
        return Err(DomainError::NotFound(format!("{}/{}", collection, path)));
    }
    let content = fs::read_to_string(&file_path)?;
    let oc: OcGraphQLRequest = serde_yaml::from_str(&content)
        .map_err(|e| DomainError::Internal(format!("Failed to parse GraphQL request: {e}")))?;
    let mut request = oc_graphql_to_domain(oc);
    request.file_name = file_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string());
    // A uid-less file gets an in-memory uid only; the next save persists it.
    if request.uid.is_empty() {
        request.uid = generate_uid();
    }
    Ok(request)
}

#[tracing::instrument(name = "collection_save_graphql_request", skip(repo, request), fields(collection_name = %collection, request_path = %path))]
pub(super) fn save_graphql_request(
    repo: &FsCollectionRepo,
    collection: &str,
    path: &str,
    request: &GraphQlRequest,
) -> DomainResult<String> {
    Collection::validate_name(collection)?;
    let mutex = repo.collection_mutex(collection);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    if request.uid.is_empty() {
        return Err(DomainError::Internal(format!(
            "save_graphql_request: empty uid on request for '{path}' in collection '{collection}'; callers must construct via GraphQlRequest::new()"
        )));
    }

    let collection_dir = repo.collection_path(collection);
    let normalized = request_filename_for(path);
    let file_path = repo.validate_path(&collection_dir, Path::new(&normalized))?;

    let mut oc = graphql_to_oc(request);

    // Request variables are saved on their own path, so an empty list in the
    // payload must keep what is on disk.
    if request.variables.is_empty() && file_path.exists() {
        if let Ok(existing_content) = fs::read_to_string(&file_path) {
            if let Ok(existing) = serde_yaml::from_str::<OcGraphQLRequest>(&existing_content) {
                if let Some(existing_runtime) = existing.runtime {
                    if !existing_runtime.variables.is_empty() {
                        let runtime = oc.runtime.get_or_insert_with(Default::default);
                        runtime.variables = existing_runtime.variables;
                    }
                }
            }
        }
    }

    let yaml = serde_yaml::to_string(&oc).map_err(|e| {
        DomainError::Internal(format!("Failed to serialize GraphQL request YAML: {e}"))
    })?;
    atomic_write(&file_path, yaml.as_bytes())?;

    let actual = file_path
        .strip_prefix(&collection_dir)
        .unwrap_or(&file_path)
        .to_string_lossy()
        .to_string();
    Ok(actual)
}

pub(super) fn get_websocket_request(
    repo: &FsCollectionRepo,
    collection: &str,
    path: &str,
) -> DomainResult<WebSocketRequest> {
    Collection::validate_name(collection)?;
    let collection_dir = repo.collection_path(collection);
    let file_path = resolve_request_path(repo, &collection_dir, path)?;
    if !file_path.exists() {
        return Err(DomainError::NotFound(format!("{collection}/{path}")));
    }
    let content = fs::read_to_string(&file_path)?;
    let oc: OcWebSocketRequest = serde_yaml::from_str(&content)
        .map_err(|e| DomainError::Internal(format!("Failed to parse WebSocket request: {e}")))?;
    let mut ws = oc_websocket_to_request(oc);
    let file_name = file_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    with_file_identity(&mut ws, &file_name);
    Ok(ws)
}

#[tracing::instrument(name = "collection_save_websocket", skip(repo, request), fields(collection_name = %collection, request_path = %path))]
pub(super) fn save_websocket_request(
    repo: &FsCollectionRepo,
    collection: &str,
    path: &str,
    request: &WebSocketRequest,
) -> DomainResult<String> {
    Collection::validate_name(collection)?;
    let mutex = repo.collection_mutex(collection);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    if request.uid.is_empty() {
        return Err(DomainError::Internal(format!(
            "save_websocket_request: empty uid on request for '{path}' in collection '{collection}'"
        )));
    }

    let collection_dir = repo.collection_path(collection);
    let normalized = request_filename_for(path);
    let file_path = repo.validate_path(&collection_dir, Path::new(&normalized))?;
    let mut oc = websocket_to_oc_websocket(request);

    // Request variables are saved on their own path (`save_request_variables`), so an empty
    // list in the payload must keep what is on disk. Same rule as `save_request`.
    if request.variables.is_empty() && file_path.exists() {
        if let Ok(existing_content) = fs::read_to_string(&file_path) {
            if let Ok(existing) = serde_yaml::from_str::<OcWebSocketRequest>(&existing_content) {
                if let Some(existing_runtime) = existing.runtime {
                    if !existing_runtime.variables.is_empty() {
                        let runtime = oc.runtime.get_or_insert_with(Default::default);
                        runtime.variables = existing_runtime.variables;
                    }
                }
            }
        }
    }

    let yaml = serde_yaml::to_string(&oc)
        .map_err(|e| DomainError::Internal(format!("Failed to serialize WebSocket YAML: {e}")))?;
    atomic_write(&file_path, yaml.as_bytes())?;

    Ok(file_path
        .strip_prefix(&collection_dir)
        .unwrap_or(&file_path)
        .to_string_lossy()
        .to_string())
}

/// Reads which protocol the request file holds from its protocol key.
pub(super) fn request_kind(
    repo: &FsCollectionRepo,
    collection: &str,
    path: &str,
) -> DomainResult<RequestKind> {
    Collection::validate_name(collection)?;
    let collection_dir = repo.collection_path(collection);
    let file_path = resolve_request_path(repo, &collection_dir, path)?;
    if !file_path.exists() {
        return Err(DomainError::NotFound(format!("{}/{}", collection, path)));
    }
    // Legacy JSON requests are always HTTP.
    if file_path.extension().is_some_and(|e| e == "json") {
        return Ok(RequestKind::Http);
    }
    let content = fs::read_to_string(&file_path)?;
    let value: serde_yaml::Value = serde_yaml::from_str(&content)
        .map_err(|e| DomainError::Internal(format!("Failed to parse request file: {e}")))?;
    Ok(if value.get("graphql").is_some() {
        RequestKind::GraphQl
    } else if value.get("grpc").is_some() {
        RequestKind::Grpc
    } else if value.get("websocket").is_some() {
        RequestKind::WebSocket
    } else {
        RequestKind::Http
    })
}
