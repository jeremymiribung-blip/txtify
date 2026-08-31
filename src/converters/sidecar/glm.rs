use async_trait::async_trait;

use crate::config::SidecarConfig;
use crate::converters::sidecar::client::SidecarClient;
use crate::core::error::TxtifyError;
use crate::core::traits::Converter;
use crate::core::types::{
    ConversionMetadata, ConversionMode, ConversionRequest, ConversionResult, ImageFormat,
    InputFormat,
};

/// High-quality converter delegating to GLM-OCR sidecar.
///
/// Only supports `ConversionMode::HighQuality`. Name is `glm-ocr-0.9b` per spec,
/// but also registers as `glm-ocr` for registry compatibility.
#[derive(Debug)]
pub struct GlmOcrConverter {
    client: SidecarClient,
}

impl GlmOcrConverter {
    /// new — fn for txtify.
    pub fn new(client: SidecarClient) -> Self {
        Self { client }
    }

    /// from_config — fn for txtify.
    pub fn from_config(config: &SidecarConfig) -> Self {
        Self {
            client: SidecarClient::from_config(config),
        }
    }

    /// from_default_config — fn for txtify.
    pub fn from_default_config() -> Self {
        Self::from_config(&SidecarConfig::default())
    }

    /// client — fn for txtify.
    pub fn client(&self) -> &SidecarClient {
        &self.client
    }
}

impl Default for GlmOcrConverter {
    fn default() -> Self {
        Self::from_default_config()
    }
}

#[async_trait]
impl Converter for GlmOcrConverter {
    fn name(&self) -> &str {
        "glm-ocr-0.9b"
    }

    fn supported_formats(&self) -> &[InputFormat] {
        &[
            InputFormat::Pdf,
            InputFormat::Image(ImageFormat::Png),
            InputFormat::Image(ImageFormat::Jpeg),
            InputFormat::Image(ImageFormat::Tiff),
            InputFormat::Html,
            InputFormat::Docx,
            InputFormat::Pptx,
            InputFormat::Xlsx,
        ]
    }

    fn supports_mode(&self, mode: ConversionMode) -> bool {
        matches!(mode, ConversionMode::HighQuality)
    }

    async fn convert(&self, req: &ConversionRequest) -> Result<ConversionResult, TxtifyError> {
        if req.mode != ConversionMode::HighQuality {
            return Err(TxtifyError::UnsupportedFormat(format!(
                "GlmOcrConverter only supports HighQuality mode, got {:?}",
                req.mode
            )));
        }

        if !self.supported_formats().contains(&req.input_format) {
            return Err(TxtifyError::UnsupportedFormat(format!(
                "GlmOcrConverter does not support {}",
                req.input_format
            )));
        }

        if !self.client.is_available() {
            return Err(TxtifyError::SidecarNotFound(
                "glm-ocr sidecar not found. Hint: pip install -r sidecar/requirements.txt (requires python + sidecar/txtify_sidecar.py)".to_string(),
            ));
        }

        let start = std::time::Instant::now();

        // Delegate to SidecarClient with engine=glm_ocr
        let (markdown, pages) = match self.client.convert(&req.input_path, "md", "glm_ocr").await {
            Ok(v) => v,
            Err(TxtifyError::SidecarNotFound(msg)) => {
                // Ensure hint contains pip install
                if msg.contains("pip install -r sidecar/requirements.txt") {
                    return Err(TxtifyError::SidecarNotFound(msg));
                }
                return Err(TxtifyError::SidecarNotFound(format!(
                    "{msg} (hint: pip install -r sidecar/requirements.txt)"
                )));
            }
            Err(TxtifyError::ConversionFailed(msg))
                if msg.contains("pip install -r sidecar/requirements.txt") =>
            {
                return Err(TxtifyError::SidecarNotFound(msg));
            }
            Err(TxtifyError::ConversionFailed(msg)) if msg.contains("No module named") => {
                return Err(TxtifyError::SidecarNotFound(format!(
                    "{msg} (hint: pip install -r sidecar/requirements.txt)"
                )));
            }
            Err(e) => return Err(e),
        };

        let output_path = req
            .output_path
            .clone()
            .unwrap_or_else(|| req.input_path.with_extension("md"));

        Ok(ConversionResult {
            output_path,
            markdown,
            metadata: ConversionMetadata {
                converter_used: self.name().to_string(),
                duration_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
                page_count: pages,
                needs_ocr: true,
            },
        })
    }
}

/// Legacy alias expected by registry (name "glm-ocr").
#[derive(Debug, Default)]
pub struct SidecarConverterGlmOcrLegacy;

const GLM_SUPPORTED_FORMATS: &[InputFormat] = &[
    InputFormat::Pdf,
    InputFormat::Image(ImageFormat::Png),
    InputFormat::Image(ImageFormat::Jpeg),
    InputFormat::Image(ImageFormat::Tiff),
    InputFormat::Html,
    InputFormat::Docx,
    InputFormat::Pptx,
    InputFormat::Xlsx,
];

#[async_trait]
impl Converter for SidecarConverterGlmOcrLegacy {
    fn name(&self) -> &str {
        "glm-ocr"
    }

    fn supported_formats(&self) -> &[InputFormat] {
        GLM_SUPPORTED_FORMATS
    }

    fn supports_mode(&self, mode: ConversionMode) -> bool {
        matches!(mode, ConversionMode::HighQuality)
    }

    async fn convert(&self, req: &ConversionRequest) -> Result<ConversionResult, TxtifyError> {
        GlmOcrConverter::default().convert(req).await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn glm_converter_name() {
        let c = GlmOcrConverter::default();
        assert_eq!(c.name(), "glm-ocr-0.9b");
    }

    #[test]
    fn glm_converter_supports_only_high_quality() {
        let c = GlmOcrConverter::default();
        assert!(!c.supports_mode(ConversionMode::Fast));
        assert!(c.supports_mode(ConversionMode::HighQuality));
    }

    #[test]
    fn glm_converter_supported_formats_include_pdf_image() {
        let c = GlmOcrConverter::default();
        let formats = c.supported_formats();
        assert!(formats.contains(&InputFormat::Pdf));
        assert!(formats.contains(&InputFormat::Image(ImageFormat::Png)));
        assert!(formats.contains(&InputFormat::Image(ImageFormat::Jpeg)));
        assert!(formats.contains(&InputFormat::Image(ImageFormat::Tiff)));
        // Also supports office/html per spec
        assert!(formats.contains(&InputFormat::Html));
        assert!(formats.contains(&InputFormat::Docx));
        assert!(formats.contains(&InputFormat::Pptx));
        assert!(formats.contains(&InputFormat::Xlsx));
    }

    #[tokio::test]
    async fn glm_converter_rejects_fast_mode() {
        let c = GlmOcrConverter::default();
        let req = ConversionRequest {
            input_path: PathBuf::from("/tmp/a.pdf"),
            output_path: None,
            input_format: InputFormat::Pdf,
            output_format: crate::core::types::OutputFormat::Md,
            mode: ConversionMode::Fast,
        };
        let err = c.convert(&req).await.unwrap_err();
        assert!(matches!(err, TxtifyError::UnsupportedFormat(_)));
    }

    #[tokio::test]
    async fn glm_converter_returns_sidecar_not_found_when_missing() {
        // Use a client pointing to nonexistent python to trigger SidecarNotFound
        let client = SidecarClient::new(
            "/nonexistent/python3".to_string(),
            PathBuf::from("/nonexistent/txtify_sidecar.py"),
            "auto".to_string(),
            "zai-org/GLM-OCR".to_string(),
        );
        let c = GlmOcrConverter::new(client);
        let req = ConversionRequest {
            input_path: PathBuf::from("/tmp/a.pdf"),
            output_path: None,
            input_format: InputFormat::Pdf,
            output_format: crate::core::types::OutputFormat::Md,
            mode: ConversionMode::HighQuality,
        };
        let err = c.convert(&req).await.unwrap_err();
        assert!(matches!(err, TxtifyError::SidecarNotFound(_)));
        let msg = err.to_string();
        assert!(
            msg.contains("pip install -r sidecar/requirements.txt"),
            "hint missing in {msg}"
        );
    }

    #[tokio::test]
    async fn glm_converter_unsupported_format() {
        let c = GlmOcrConverter::default();
        let req = ConversionRequest {
            input_path: PathBuf::from("/tmp/a.csv"),
            output_path: None,
            input_format: InputFormat::Csv,
            output_format: crate::core::types::OutputFormat::Md,
            mode: ConversionMode::HighQuality,
        };
        let err = c.convert(&req).await.unwrap_err();
        assert!(matches!(err, TxtifyError::UnsupportedFormat(_)));
    }
}
