//! Export report types shown to the user after an export.

use serde::{Deserialize, Serialize};

/// What the exporter did with one item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportAction {
    Translated,
    Blanked,
    Skipped,
    Preserved,
}

/// One report entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportNote {
    pub path: String,
    pub feature: String,
    pub action: ExportAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// The full export report.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    pub exported: usize,
    pub items: Vec<ExportNote>,
}
