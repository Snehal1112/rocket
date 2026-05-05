use std::fs;

use rocket_collection::{Collection, CollectionVariable};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;
use crate::opencollection::{OcFolderInfo, OcHttpRequest, OcHttpRequestRuntime, OcRequestDefaults, OcVariable};

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
        if !folder_yml.exists() { continue; }
        let Ok(content) = fs::read_to_string(&folder_yml) else { continue; };
        let Ok(info) = serde_yaml::from_str::<OcFolderInfo>(&content) else { continue; };
        let Some(req) = info.request else { continue; };
        let Some(vars) = req.variables else { continue; };
        chain.push(
            vars.into_iter()
                .map(CollectionVariable::from)
                .collect(),
        );
    }
    Ok(rocket_collection::settings::merge_folder_chain_variables(chain))
}

pub(super) fn save_folder_variables(
    repo: &FsCollectionRepo,
    collection: &str,
    folder_path: &str,
    vars: Vec<CollectionVariable>,
) -> DomainResult<()> {
    Collection::validate_name(collection)?;
    let mutex = repo.collection_mutex(collection);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    let collection_dir = repo.collection_path(collection);
    let folder_dir = if folder_path.is_empty() {
        collection_dir.clone()
    } else {
        repo.validate_path(&collection_dir, std::path::Path::new(folder_path))?
    };
    let folder_yml_path = folder_dir.join("folder.yml");
    let mut info: OcFolderInfo = if folder_yml_path.exists() {
        let content = fs::read_to_string(&folder_yml_path)?;
        serde_yaml::from_str::<OcFolderInfo>(&content)
            .map_err(|e| DomainError::Internal(format!("Failed to parse folder.yml: {e}")))?
    } else {
        OcFolderInfo::default()
    };
    let oc_vars: Vec<OcVariable> = vars.into_iter().map(OcVariable::from).collect();
    let req_defaults = info.request.take().unwrap_or_default();
    info.request = Some(OcRequestDefaults {
        variables: if oc_vars.is_empty() { None } else { Some(oc_vars) },
        ..req_defaults
    });
    let yaml = serde_yaml::to_string(&info)
        .map_err(|e| DomainError::Internal(format!("Failed to serialize folder.yml: {e}")))?;
    atomic_write(&folder_yml_path, yaml.as_bytes())?;
    Ok(())
}

pub(super) fn get_folder_variables(
    repo: &FsCollectionRepo,
    collection: &str,
    folder_path: &str,
) -> DomainResult<Vec<CollectionVariable>> {
    Collection::validate_name(collection)?;
    let collection_dir = repo.collection_path(collection);
    let folder_dir = if folder_path.is_empty() {
        collection_dir.clone()
    } else {
        repo.validate_path(&collection_dir, std::path::Path::new(folder_path))?
    };
    let folder_yml = folder_dir.join("folder.yml");
    if !folder_yml.exists() {
        return Ok(vec![]);
    }
    let content = fs::read_to_string(&folder_yml)?;
    let info: OcFolderInfo = serde_yaml::from_str(&content)
        .map_err(|e| DomainError::Internal(format!("Failed to parse folder.yml: {e}")))?;
    let vars = info
        .request
        .and_then(|r| r.variables)
        .unwrap_or_default()
        .into_iter()
        .map(CollectionVariable::from)
        .collect();
    Ok(vars)
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
    let req: OcHttpRequest = serde_yaml::from_str(&content)
        .map_err(|e| DomainError::Internal(format!("Failed to parse request file: {e}")))?;
    let vars = req
        .runtime
        .map(|r| r.variables)
        .unwrap_or_default()
        .into_iter()
        .map(CollectionVariable::from)
        .collect();
    Ok(vars)
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
    let mut req: OcHttpRequest = serde_yaml::from_str(&content)
        .map_err(|e| DomainError::Internal(format!("Failed to parse request file: {e}")))?;
    let oc_vars: Vec<OcVariable> = vars.into_iter().map(OcVariable::from).collect();
    let runtime = req.runtime.take().unwrap_or_default();
    req.runtime = Some(OcHttpRequestRuntime {
        variables: oc_vars,
        ..runtime
    });
    let yaml = serde_yaml::to_string(&req)
        .map_err(|e| DomainError::Internal(format!("Failed to serialize request file: {e}")))?;
    atomic_write(&file_path, yaml.as_bytes())?;
    Ok(())
}
