use std::fs;

use rocket_collection::{Collection, CollectionVariable};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;
use crate::oc::{
    OcFolder, OcFolderInfo, OcGraphQLRequest, OcGraphQLRequestRuntime, OcHttpRequest,
    OcHttpRequestRuntime, OcRequestDefaults, OcVariable,
};

use super::folder_file::{parse_folder_yml, read_folder_yml, write_folder_yml};
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
    let mut oc_folder = if folder_yml_path.exists() {
        read_folder_yml(&folder_yml_path)?
    } else {
        let name = folder_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        OcFolder {
            info: OcFolderInfo {
                name,
                ..OcFolderInfo::default()
            },
            items: None,
            request: None,
            docs: None,
        }
    };
    let oc_vars: Vec<OcVariable> = vars.into_iter().map(OcVariable::from).collect();
    let req_defaults = oc_folder.request.take().unwrap_or_default();
    oc_folder.request = Some(OcRequestDefaults {
        variables: if oc_vars.is_empty() {
            None
        } else {
            Some(oc_vars)
        },
        ..req_defaults
    });
    write_folder_yml(&folder_yml_path, &oc_folder)?;
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
    let oc_folder = read_folder_yml(&folder_yml)?;
    let vars = oc_folder
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
    let vars = runtime_variables_of(&content)?
        .into_iter()
        .map(CollectionVariable::from)
        .collect();
    Ok(vars)
}

/// Reads `runtime.variables` from an HTTP or GraphQL request file.
fn runtime_variables_of(content: &str) -> DomainResult<Vec<OcVariable>> {
    let http_err = match serde_yaml::from_str::<OcHttpRequest>(content) {
        Ok(req) => return Ok(req.runtime.map(|r| r.variables).unwrap_or_default()),
        Err(e) => e,
    };
    match serde_yaml::from_str::<OcGraphQLRequest>(content) {
        Ok(g) => Ok(g.runtime.map(|r| r.variables).unwrap_or_default()),
        // Keep the HTTP error: it is the precise one for a broken HTTP file.
        Err(_) => Err(DomainError::Internal(format!(
            "Failed to parse request file: {http_err}"
        ))),
    }
}

/// Returns the file content with `runtime.variables` replaced, for an HTTP or GraphQL request file.
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
    match serde_yaml::from_str::<OcGraphQLRequest>(content) {
        Ok(mut g) => {
            let runtime = g.runtime.take().unwrap_or_default();
            g.runtime = Some(OcGraphQLRequestRuntime {
                variables: vars,
                ..runtime
            });
            serde_yaml::to_string(&g).map_err(to_err)
        }
        Err(_) => Err(DomainError::Internal(format!(
            "Failed to parse request file: {http_err}"
        ))),
    }
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
