use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use base64::{engine::general_purpose::STANDARD, Engine};
use rocket_app::collection_export_service::{CollectionExportService, PostmanExportOptions};
use rocket_export::ExportReport;
use rocket_infra::{FsCollectionFileSource, SharedCollectionEnvironmentRepo, SharedPathCollectionRepo};
use rocket_shared::error::DomainError;
use serde::{Deserialize, Serialize};
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

/// Choices of a Postman export, as the dialog sends them.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostmanExportOptionsDto {
    pub preserve_scripts: bool,
    pub include_environments: bool,
}

impl From<PostmanExportOptionsDto> for PostmanExportOptions {
    fn from(dto: PostmanExportOptionsDto) -> Self {
        Self {
            preserve_scripts: dto.preserve_scripts,
            include_environments: dto.include_environments,
        }
    }
}

/// Export a collection as a Postman Collection v2.1 file, or as a ZIP with its
/// environments. Returns the bytes as base64.
#[tauri::command]
pub async fn export_collection_postman(
    collection: String,
    options: PostmanExportOptionsDto,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<ExportResultDto, DomainError> {
    let service = CollectionExportService::new(Box::new(FsCollectionFileSource::new(Arc::clone(
        &*workspace_path,
    ))))
    .with_collections(Box::new(SharedPathCollectionRepo::new(Arc::clone(
        &*workspace_path,
    ))))
    .with_environments(Box::new(SharedCollectionEnvironmentRepo::with_secret_store(
        Arc::clone(&*workspace_path),
        crate::env_secret_store(),
    )));
    let out = service.export_postman(&collection, &options.into())?;
    Ok(ExportResultDto {
        data_base64: STANDARD.encode(&out.bytes),
        file_name: out.file_name,
        report: out.report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_dto_reads_camel_case_keys() {
        let dto: PostmanExportOptionsDto =
            serde_json::from_str(r#"{"preserveScripts":true,"includeEnvironments":false}"#)
                .expect("camelCase keys parse");
        assert!(dto.preserve_scripts);
        assert!(!dto.include_environments);
    }

    #[test]
    fn options_dto_maps_to_service_options() {
        let dto = PostmanExportOptionsDto {
            preserve_scripts: true,
            include_environments: true,
        };
        let opts = PostmanExportOptions::from(dto);
        assert!(opts.preserve_scripts);
        assert!(opts.include_environments);
    }
}
