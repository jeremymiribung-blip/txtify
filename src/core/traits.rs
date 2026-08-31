use std::path::Path;

use async_trait::async_trait;

use crate::core::error::TxtifyError;
use crate::core::types::{ConversionMode, ConversionRequest, ConversionResult, InputFormat};

/// Asynchronous document converter.
#[async_trait]
pub trait Converter: Send + Sync {
    /// Human-readable name (e.g. "fast", "pandoc", "glm-ocr").
    fn name(&self) -> &str;

    /// Formats this converter can handle.
    fn supported_formats(&self) -> &[InputFormat];

    /// Whether the converter supports the given execution mode.
    fn supports_mode(&self, mode: ConversionMode) -> bool;

    /// Convert a document and return the result.
    async fn convert(&self, req: &ConversionRequest) -> Result<ConversionResult, TxtifyError>;
}

/// Detect input format from file path (extension + magic bytes).
pub trait FormatDetector: Send + Sync {
    fn detect(&self, path: &Path) -> Result<InputFormat, TxtifyError>;
}
