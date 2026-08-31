use thiserror::Error;

/// Errors produced by txtify.
#[derive(Debug, Error)]
pub enum TxtifyError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("conversion failed: {0}")]
    ConversionFailed(String),

    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("sidecar not found: {0}")]
    SidecarNotFound(String),

    #[error("config error: {0}")]
    ConfigError(String),

    #[error("shell error: {0}")]
    ShellError(String),

    #[error("operation timed out")]
    Timeout,
}
