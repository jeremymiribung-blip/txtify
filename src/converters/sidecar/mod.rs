/// client — mod for txtify.
pub mod client;
/// glm — mod for txtify.
pub mod glm;

/// client — use for txtify.
pub use client::{detect_python, find_sidecar_script, SidecarClient};
/// glm — use for txtify.
pub use glm::{GlmOcrConverter, SidecarConverterGlmOcrLegacy};

use async_trait::async_trait;

use crate::core::error::TxtifyError;
use crate::core::traits::Converter;
use crate::core::types::{
    ConversionMode, ConversionRequest, ConversionResult, ImageFormat, InputFormat,
};

/// Legacy alias for backwards compatibility - delegates to GlmOcrConverter.
///
/// Name remains "glm-ocr" for registry selection, while GlmOcrConverter uses "glm-ocr-0.9b".
#[derive(Debug, Default)]
pub struct SidecarConverter(pub GlmOcrConverter);

impl SidecarConverter {
    /// new — fn for txtify.
    pub fn new() -> Self {
        Self(GlmOcrConverter::default())
    }
}

#[async_trait]
impl Converter for SidecarConverter {
    fn name(&self) -> &str {
        "glm-ocr"
    }

    fn supported_formats(&self) -> &[InputFormat] {
        &[
            InputFormat::Pdf,
            InputFormat::Docx,
            InputFormat::Pptx,
            InputFormat::Xlsx,
            InputFormat::Html,
            InputFormat::Image(ImageFormat::Png),
            InputFormat::Image(ImageFormat::Jpeg),
            InputFormat::Image(ImageFormat::Tiff),
        ]
    }

    fn supports_mode(&self, mode: ConversionMode) -> bool {
        matches!(mode, ConversionMode::HighQuality)
    }

    async fn convert(&self, req: &ConversionRequest) -> Result<ConversionResult, TxtifyError> {
        self.0.convert(req).await
    }
}
