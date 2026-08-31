/// docx — mod for txtify.
pub mod docx;
/// html — mod for txtify.
pub mod html;
/// pdf — mod for txtify.
pub mod pdf;
/// pptx — mod for txtify.
pub mod pptx;
/// text — mod for txtify.
pub mod text;
/// xlsx — mod for txtify.
pub mod xlsx;

/// docx — use for txtify.
pub use docx::DocxConverter;
/// html — use for txtify.
pub use html::HtmlConverter;
/// pdf — use for txtify.
pub use pdf::PdfConverter;
/// pptx — use for txtify.
pub use pptx::PptxConverter;
/// text — use for txtify.
pub use text::TextConverter;
/// xlsx — use for txtify.
pub use xlsx::XlsxConverter;

use async_trait::async_trait;

use crate::core::error::TxtifyError;
use crate::core::traits::Converter;
use crate::core::types::{ConversionMode, ConversionRequest, ConversionResult, InputFormat};

/// Aggregated fast converter (pure Rust, instant) that delegates to format-specific converters.
#[derive(Debug, Default)]
pub struct FastConverter;

#[async_trait]
impl Converter for FastConverter {
    fn name(&self) -> &str {
        "fast"
    }

    fn supported_formats(&self) -> &[InputFormat] {
        &[
            InputFormat::Pdf,
            InputFormat::Docx,
            InputFormat::Xlsx,
            InputFormat::Pptx,
            InputFormat::Html,
            InputFormat::Txt,
            InputFormat::Md,
            InputFormat::Csv,
        ]
    }

    fn supports_mode(&self, mode: ConversionMode) -> bool {
        mode == ConversionMode::Fast
    }

    async fn convert(&self, req: &ConversionRequest) -> Result<ConversionResult, TxtifyError> {
        if req.mode != ConversionMode::Fast {
            return Err(TxtifyError::UnsupportedFormat(format!(
                "FastConverter only supports Fast mode, got {:?}",
                req.mode
            )));
        }
        // Delegate based on input format
        match req.input_format {
            InputFormat::Pdf => {
                let c = PdfConverter;
                c.convert(req).await
            }
            InputFormat::Docx => {
                let c = DocxConverter;
                c.convert(req).await
            }
            InputFormat::Xlsx => {
                let c = XlsxConverter;
                c.convert(req).await
            }
            InputFormat::Pptx => {
                let c = PptxConverter;
                c.convert(req).await
            }
            InputFormat::Html => {
                let c = HtmlConverter;
                c.convert(req).await
            }
            InputFormat::Txt | InputFormat::Md | InputFormat::Csv => {
                let c = TextConverter;
                c.convert(req).await
            }
            _ => Err(TxtifyError::UnsupportedFormat(format!(
                "FastConverter does not support {} in Fast mode",
                req.input_format
            ))),
        }
    }
}
