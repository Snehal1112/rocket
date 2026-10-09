//! Context chips for the assistant composer.
//!
//! The text of a chip is built and masked in `rocket-app` from the same masked views the MCP
//! read tools use. These commands only validate, map the DTOs and return the result.

use std::sync::Arc;

use rocket_app::{
    ChipKind, McpToolService, ResponseChipHeader, ResponseChipInput, ResponseChipRequest,
    ResponseChipTest,
};
use rocket_shared::error::DomainError;
use rocket_shared::types::{Auth, Body, Header, QueryParam};
use serde::Deserialize;
use tauri::State;

use crate::commands::acp_session_dto::PromptResourceDto;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChipKindDto {
    Request,
    Folder,
    Collection,
    Environment,
}

impl From<ChipKindDto> for ChipKind {
    fn from(kind: ChipKindDto) -> Self {
        match kind {
            ChipKindDto::Request => Self::Request,
            ChipKindDto::Folder => Self::Folder,
            ChipKindDto::Collection => Self::Collection,
            ChipKindDto::Environment => Self::Environment,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseChipHeaderDto {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseChipTestDto {
    pub name: String,
    pub passed: bool,
    pub error: Option<String>,
}

/// The request of the tab that holds the response, as it is on screen.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseChipRequestDto {
    pub headers: Vec<Header>,
    pub query_params: Vec<QueryParam>,
    pub body: Option<Body>,
    pub auth: Auth,
}

/// A JavaScript number can arrive as a float, so the counters are read as `f64`.
fn counter(value: f64) -> u64 {
    if value.is_finite() && value > 0.0 {
        value as u64
    } else {
        0
    }
}

/// The last response of a request tab, as the frontend holds it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseChipDto {
    pub method: String,
    pub url: String,
    pub status: u16,
    pub status_text: String,
    pub duration_ms: f64,
    pub size_bytes: f64,
    pub headers: Vec<ResponseChipHeaderDto>,
    pub body: String,
    pub is_binary: bool,
    pub tests: Vec<ResponseChipTestDto>,
    pub request: Option<ResponseChipRequestDto>,
}

impl From<ResponseChipDto> for ResponseChipInput {
    fn from(dto: ResponseChipDto) -> Self {
        Self {
            method: dto.method,
            url: dto.url,
            status: dto.status,
            status_text: dto.status_text,
            duration_ms: counter(dto.duration_ms),
            size_bytes: counter(dto.size_bytes),
            headers: dto
                .headers
                .into_iter()
                .map(|h| ResponseChipHeader {
                    key: h.key,
                    value: h.value,
                })
                .collect(),
            body: dto.body,
            is_binary: dto.is_binary,
            tests: dto
                .tests
                .into_iter()
                .map(|t| ResponseChipTest {
                    name: t.name,
                    passed: t.passed,
                    error: t.error,
                })
                .collect(),
            request: dto.request.map(|r| ResponseChipRequest {
                headers: r.headers,
                query_params: r.query_params,
                body: r.body,
                auth: r.auth,
            }),
        }
    }
}

fn resource(chip: rocket_app::ChipResource) -> PromptResourceDto {
    PromptResourceDto {
        uri: chip.uri,
        mime_type: Some("text/plain".to_string()),
        text: chip.text,
    }
}

/// Builds the masked, size-capped text resource of a request, folder, collection or
/// environment chip. `path` is the request or folder path, or the environment name.
#[tauri::command]
pub fn build_assistant_chip_resource(
    kind: ChipKindDto,
    collection: String,
    path: Option<String>,
    mcp_tool_svc: State<'_, Arc<McpToolService>>,
) -> Result<PromptResourceDto, DomainError> {
    mcp_tool_svc
        .build_chip_resource(kind.into(), &collection, path.as_deref())
        .map(resource)
}

/// Masks the last response of a request tab and returns it as a text resource.
/// `environment_name` is the tab's active environment, whose vault secrets are masked too.
#[tauri::command]
pub async fn mask_assistant_response(
    collection: String,
    request_path: String,
    environment_name: Option<String>,
    response: ResponseChipDto,
    mcp_tool_svc: State<'_, Arc<McpToolService>>,
) -> Result<PromptResourceDto, DomainError> {
    mcp_tool_svc
        .mask_response_chip(
            &collection,
            &request_path,
            environment_name.as_deref(),
            &response.into(),
        )
        .await
        .map(resource)
}
