use rocket_shared::error::DomainError;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("not a Bruno directory (no marker file found): {0}")]
    NotABrunoDirectory(PathBuf),

    #[error("parse error in {path}: {message}")]
    ParseError { path: PathBuf, message: String },

    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("domain error: {0}")]
    DomainError(#[from] DomainError),

    #[error("zip extraction failed: {0}")]
    ZipExtractionFailed(String),

    #[error("zip is empty or contains no top-level directory")]
    EmptyZip,
}

pub type ImportResult<T> = Result<T, ImportError>;
