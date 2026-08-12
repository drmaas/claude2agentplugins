use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("Git error: {0}")]
    Git(#[from] git2::Error),

    #[error("Walk error: {0}")]
    Walk(#[from] walkdir::Error),

    #[error("Validation error: {message}")]
    Validation {
        message: String,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Conversion error: {0}")]
    Conversion(String),

    #[error("Plugin not found at {path}: {reason}")]
    NotFound { path: String, reason: String },

    #[error("{0}")]
    Anyhow(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
