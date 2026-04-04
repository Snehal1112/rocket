// Bruno .bru and .yml parser — implemented in plan-02 and plan-03.
pub(crate) mod ast;
pub(crate) mod lexer;
pub(crate) mod parser;
pub(crate) mod yml_adapter;
pub(crate) mod zip_extractor;

use std::path::Path;
use crate::error::ImportResult;
use ast::BruDocument;

/// Detect format from file extension and parse into a BruDocument.
/// `.bru` → lexer/parser; `.yml`/`.yaml` → yml_adapter.
pub(crate) fn parse_file(path: &Path) -> ImportResult<BruDocument> {
    let content = std::fs::read_to_string(path)
        .map_err(crate::error::ImportError::IoError)?;

    match path.extension().and_then(|e| e.to_str()) {
        Some("bru") => parser::parse(&content),
        Some("yml") | Some("yaml") => yml_adapter::bru_document_from_yml_str(&content),
        _ => Err(crate::error::ImportError::ParseError {
            path: path.to_path_buf(),
            message: "unsupported file extension (expected .bru or .yml)".into(),
        }),
    }
}

/// Parse a Bruno environment file (`.yml` or `.bru`) into a BruDocument.
pub(crate) fn parse_env_file(path: &Path) -> ImportResult<BruDocument> {
    let content = std::fs::read_to_string(path)
        .map_err(crate::error::ImportError::IoError)?;

    match path.extension().and_then(|e| e.to_str()) {
        Some("yml") | Some("yaml") => yml_adapter::bru_document_from_yml_env_str(&content),
        Some("bru") => parser::parse(&content),
        _ => Err(crate::error::ImportError::ParseError {
            path: path.to_path_buf(),
            message: "unsupported env file extension".into(),
        }),
    }
}
