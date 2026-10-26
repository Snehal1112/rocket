use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_app::OpenApiSyncService;
use rocket_environment::{EnvironmentRepository, SecretStore};
use rocket_http::SpecFetcher;
use rocket_import::{
    EnvironmentRepositoryFactory, GroupBy, ImportReport, ImportService, OpenApiImportResult,
};
use rocket_infra::{
    FsCollectionRepo, FsEnvironmentRepo, FsOpenApiBaselineRepo, FsOpenApiSourceRepo,
    KeyringSecretStore,
};
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
    pub source_id: Option<String>,
    pub warnings: Vec<String>,
}

impl From<OpenApiImportResult> for OpenApiImportDto {
    fn from(r: OpenApiImportResult) -> Self {
        Self {
            report: r.report,
            collection: r.collection,
            source_url: r.source_url,
            source_id: r.source_id,
            warnings: r.warnings,
        }
    }
}

fn parse_group_by(text: &str) -> Result<GroupBy, String> {
    GroupBy::parse(text).ok_or_else(|| "folder layout must be tags or paths".to_string())
}

/// Trims a token from the UI. A blank value means no token.
fn token_arg(token: Option<String>) -> Option<String> {
    token
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

const TOKEN_NOT_SAVED: &str =
    "The token could not be saved, so checks on this private repository will fail.";

/// Stores the token in the keychain. The error text never holds the token.
fn store_token(
    fetcher: &Arc<dyn SpecFetcher>,
    secrets: Arc<dyn SecretStore>,
    collection_root: &std::path::Path,
    token: &str,
) -> bool {
    OpenApiSyncService::new(Arc::clone(fetcher), Box::new(FsOpenApiSourceRepo), secrets)
        .set_token(collection_root, token)
        .is_ok()
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
    token: Option<String>,
    target_workspace_id: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<OpenApiImportDto, String> {
    let group_by = parse_group_by(&group_by)?;
    let base = workspace_path
        .lock()
        .map_err(|_| "workspace path lock poisoned".to_string())?
        .clone();
    let token = token_arg(token);
    let mut dto: OpenApiImportDto = make_import_service(base.clone())
        .import_openapi_url(
            fetcher.inner().as_ref(),
            &FsOpenApiSourceRepo,
            &FsOpenApiBaselineRepo,
            &link,
            group_by,
            token.as_deref(),
            &target_workspace_id,
        )
        .await
        .map(OpenApiImportDto::from)
        .map_err(|e| e.to_string())?;
    if let Some(token) = token.as_deref() {
        let root = base.join("collections").join(&dto.collection);
        let secrets: Arc<dyn SecretStore> = Arc::new(KeyringSecretStore::new_openapi_sync());
        if !store_token(fetcher.inner(), secrets, &root, token) {
            dto.warnings.push(TOKEN_NOT_SAVED.to_string());
        }
    }
    Ok(dto)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_arg_trims_and_drops_blank_values() {
        assert_eq!(token_arg(None), None);
        assert_eq!(token_arg(Some("  ".to_string())), None);
        assert_eq!(
            token_arg(Some(" abc ".to_string())),
            Some("abc".to_string())
        );
    }

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
