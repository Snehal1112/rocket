use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_environment::EnvironmentRepository;
use rocket_http::SpecFetcher;
use rocket_import::{
    EnvironmentRepositoryFactory, GroupBy, ImportReport, ImportService, OpenApiImportResult,
};
use rocket_infra::{FsCollectionRepo, FsEnvironmentRepo, FsOpenApiSourceRepo};
use serde::Serialize;
use tauri::State;

struct FsEnvFactory(PathBuf);
impl EnvironmentRepositoryFactory for FsEnvFactory {
    fn make(&self, collection_name: &str) -> Box<dyn EnvironmentRepository> {
        Box::new(FsEnvironmentRepo::with_secret_store(
            self.0
                .join("collections")
                .join(collection_name)
                .join("environments"),
            crate::env_secret_store(),
        ))
    }
}

fn make_import_service(base: PathBuf) -> ImportService {
    ImportService::new(
        base.clone(),
        Box::new(FsCollectionRepo::new_standalone(base.join("collections"))),
        Box::new(FsEnvFactory(base)),
    )
}

/// Import a Bruno collection or workspace directory. Type is auto-detected from content.
#[tauri::command]
pub async fn import_bruno(
    path: String,
    target_workspace_id: String,
    create_new_workspace: Option<bool>,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<ImportReport, String> {
    let base = workspace_path
        .lock()
        .map_err(|_| "workspace path lock poisoned".to_string())?
        .clone();
    make_import_service(base)
        .import_auto(
            &PathBuf::from(&path),
            &target_workspace_id,
            create_new_workspace.unwrap_or(false),
        )
        .map_err(|e| e.to_string())
}

/// Import a Postman Collection JSON file (v2.0 or v2.1).
#[tauri::command]
pub async fn import_postman_collection(
    path: String,
    target_workspace_id: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<ImportReport, String> {
    let base = workspace_path
        .lock()
        .map_err(|_| "workspace path lock poisoned".to_string())?
        .clone();
    make_import_service(base)
        .import_postman_collection(&PathBuf::from(&path), &target_workspace_id)
        .map_err(|e| e.to_string())
}

/// Import a Postman environment JSON file into an existing collection.
#[tauri::command]
pub async fn import_postman_environment(
    json_path: String,
    collection_name: String,
    target_workspace_id: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<ImportReport, String> {
    let base = workspace_path
        .lock()
        .map_err(|_| "workspace path lock poisoned".to_string())?
        .clone();
    make_import_service(base)
        .import_postman_environment(
            &PathBuf::from(&json_path),
            &collection_name,
            &target_workspace_id,
        )
        .map_err(|e| e.to_string())
}

/// Import a WSDL 1.1 file as a collection of SOAP requests.
#[tauri::command]
pub async fn import_wsdl(
    path: String,
    target_workspace_id: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<ImportReport, String> {
    let base = workspace_path
        .lock()
        .map_err(|_| "workspace path lock poisoned".to_string())?
        .clone();
    make_import_service(base)
        .import_wsdl(&PathBuf::from(&path), &target_workspace_id)
        .map_err(|e| e.to_string())
}

/// Extract a Bruno ZIP and import the contained collection or workspace.
#[tauri::command]
pub async fn import_bruno_zip(
    zip_path: String,
    target_workspace_id: String,
    create_new_workspace: Option<bool>,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<ImportReport, String> {
    let base = workspace_path
        .lock()
        .map_err(|_| "workspace path lock poisoned".to_string())?
        .clone();
    make_import_service(base)
        .import_auto_from_zip(
            &PathBuf::from(&zip_path),
            &target_workspace_id,
            create_new_workspace.unwrap_or(false),
        )
        .map_err(|e| e.to_string())
}

/// What an OpenAPI import returns to the UI.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiImportDto {
    pub report: ImportReport,
    pub collection: String,
    pub source_url: Option<String>,
    pub warnings: Vec<String>,
}

impl From<OpenApiImportResult> for OpenApiImportDto {
    fn from(r: OpenApiImportResult) -> Self {
        Self {
            report: r.report,
            collection: r.collection,
            source_url: r.source_url,
            warnings: r.warnings,
        }
    }
}

fn parse_group_by(text: &str) -> Result<GroupBy, String> {
    GroupBy::parse(text).ok_or_else(|| "folder layout must be tags or paths".to_string())
}

/// Import an OpenAPI or Swagger spec file as a new collection.
#[tauri::command]
pub async fn import_openapi_file(
    path: String,
    group_by: String,
    target_workspace_id: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<OpenApiImportDto, String> {
    let group_by = parse_group_by(&group_by)?;
    let base = workspace_path
        .lock()
        .map_err(|_| "workspace path lock poisoned".to_string())?
        .clone();
    make_import_service(base)
        .import_openapi_file(&PathBuf::from(&path), group_by, &target_workspace_id)
        .map(OpenApiImportDto::from)
        .map_err(|e| e.to_string())
}

/// Import an OpenAPI or Swagger spec from a link as a new collection.
#[tauri::command]
pub async fn import_openapi_url(
    link: String,
    group_by: String,
    target_workspace_id: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<OpenApiImportDto, String> {
    let group_by = parse_group_by(&group_by)?;
    let base = workspace_path
        .lock()
        .map_err(|_| "workspace path lock poisoned".to_string())?
        .clone();
    make_import_service(base)
        .import_openapi_url(
            fetcher.inner().as_ref(),
            &FsOpenApiSourceRepo,
            &link,
            group_by,
            &target_workspace_id,
        )
        .await
        .map(OpenApiImportDto::from)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_group_by_accepts_tags_and_paths_only() {
        assert!(parse_group_by("tags").is_ok());
        assert!(parse_group_by("paths").is_ok());
        assert_eq!(
            parse_group_by("folders").err().as_deref(),
            Some("folder layout must be tags or paths")
        );
    }
}
