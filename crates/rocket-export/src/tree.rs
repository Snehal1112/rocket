//! The typed collection tree the exporters read.

use rocket_collection::{CollectionSettings, FolderSettings, GraphQlRequest, Request};

/// A whole collection, ready to serialize.
#[derive(Debug, Clone)]
pub struct CollectionExportTree {
    pub name: String,
    pub settings: CollectionSettings,
    pub root: ExportFolder,
}

/// One folder and its items.
#[derive(Debug, Clone)]
pub struct ExportFolder {
    pub name: String,
    pub settings: FolderSettings,
    pub items: Vec<ExportItem>,
}

/// One item in a folder.
// Folders are rare next to requests, so boxing them would only add churn.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum ExportItem {
    Request(Box<Request>),
    GraphQl(Box<GraphQlRequest>),
    Folder(ExportFolder),
    /// An item that no exporter writes. `kind` names it in the report.
    Skipped {
        path: String,
        kind: String,
    },
}
