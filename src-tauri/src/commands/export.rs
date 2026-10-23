use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use base64::{engine::general_purpose::STANDARD, Engine};
use rocket_app::collection_export_service::CollectionExportService;
use rocket_export::ExportReport;
use rocket_infra::{FsCollectionFileSource, SharedPathCollectionRepo};
use rocket_shared::error::DomainError;
use serde::Serialize;
use tauri::State;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResultDto {
    pub data_base64: String,
    pub file_name: String,
    pub report: ExportReport,
}

/// Export a collection as a Bruno ZIP. Returns the bytes as base64.
#[tauri::command]
pub async fn export_collection_bruno(
    collection: String,
    include_environments: bool,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<ExportResultDto, DomainError> {
    let service = CollectionExportService::new(Box::new(FsCollectionFileSource::new(Arc::clone(
        &*workspace_path,
    ))));
    let out = service.export_bruno(&collection, include_environments)?;
    Ok(ExportResultDto {
        data_base64: STANDARD.encode(&out.bytes),
        file_name: out.file_name,
        report: out.report,
    })
}

/// Export a collection as a Postman Collection v2.1 file. Returns the bytes as base64.
#[tauri::command]
pub async fn export_collection_postman(
    collection: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<ExportResultDto, DomainError> {
    let service = CollectionExportService::new(Box::new(FsCollectionFileSource::new(Arc::clone(
        &*workspace_path,
    ))))
    .with_collections(Box::new(SharedPathCollectionRepo::new(Arc::clone(
        &*workspace_path,
    ))));
    let out = service.export_postman(&collection, &Default::default())?;
    Ok(ExportResultDto {
        data_base64: STANDARD.encode(&out.bytes),
        file_name: out.file_name,
        report: out.report,
    })
}
