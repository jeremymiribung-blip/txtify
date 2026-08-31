use crate::core::error::TxtifyError;
use crate::core::traits::Converter;
use crate::core::types::{ConversionMode, ConversionRequest};

/// Registry of available converters.
///
/// Selection priority:
/// 1. Fast (pure-Rust) — always preferred
/// 2. Pandoc — second choice
/// 3. GLM-OCR sidecar — only in HighQuality mode
pub struct ConverterRegistry {
    converters: Vec<Box<dyn Converter>>,
}

impl ConverterRegistry {
    /// new — fn for txtify.
    pub fn new() -> Self {
        Self {
            converters: Vec::new(),
        }
    }

    /// register — fn for txtify.
    pub fn register(&mut self, converter: Box<dyn Converter>) {
        self.converters.push(converter);
    }

    /// Select the best converter for the given request.
    ///
    /// Priority: Fast (Fast mode) → GLM-OCR (HighQuality for Pdf/Image) → Pandoc → Fallback.
    pub fn select(&self, req: &ConversionRequest) -> Result<&dyn Converter, TxtifyError> {
        // HighQuality: prefer GLM-OCR for PDF and images (best layout/OCR)
        if req.mode == ConversionMode::HighQuality {
            let sidecar = self.converters.iter().find(|c| {
                (c.name() == "glm-ocr" || c.name() == "sidecar")
                    && c.supported_formats().contains(&req.input_format)
                    && c.supports_mode(req.mode)
            });
            if let Some(c) = sidecar {
                return Ok(c.as_ref());
            }
        }

        let fast = self
            .converters
            .iter()
            .find(|c| c.name() == "fast" && c.supported_formats().contains(&req.input_format));

        if let Some(c) = fast {
            if c.supports_mode(req.mode) {
                return Ok(c.as_ref());
            }
        }

        let pandoc = self
            .converters
            .iter()
            .find(|c| c.name() == "pandoc" && c.supported_formats().contains(&req.input_format));

        if let Some(c) = pandoc {
            if c.supports_mode(req.mode) {
                return Ok(c.as_ref());
            }
        }

        // Fallback: if HighQuality but GLM was not matched earlier due to name mismatch, try again
        if req.mode == ConversionMode::HighQuality {
            let sidecar = self.converters.iter().find(|c| {
                (c.name() == "glm-ocr" || c.name() == "sidecar")
                    && c.supported_formats().contains(&req.input_format)
            });

            if let Some(c) = sidecar {
                return Ok(c.as_ref());
            }
        }

        Err(TxtifyError::UnsupportedFormat(format!(
            "no converter found for {} in {:?} mode",
            req.input_format, req.mode
        )))
    }

    /// list — fn for txtify.
    pub fn list(&self) -> Vec<&str> {
        self.converters.iter().map(|c| c.name()).collect()
    }

    /// len — fn for txtify.
    pub fn len(&self) -> usize {
        self.converters.len()
    }

    /// is_empty — fn for txtify.
    pub fn is_empty(&self) -> bool {
        self.converters.is_empty()
    }
}

impl Default for ConverterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ConverterRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConverterRegistry")
            .field("converters", &self.list())
            .finish()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::core::types::{
        ConversionMode, ConversionRequest, ConversionResult, InputFormat, OutputFormat,
    };
    use std::path::PathBuf;

    /// Stub converter that returns UnsupportedFormat.
    struct StubConverter {
        name: &'static str,
        formats: Vec<InputFormat>,
        modes: Vec<ConversionMode>,
    }

    impl StubConverter {
        fn fast() -> Self {
            Self {
                name: "fast",
                formats: vec![InputFormat::Txt, InputFormat::Md, InputFormat::Html],
                modes: vec![ConversionMode::Fast, ConversionMode::HighQuality],
            }
        }

        fn pandoc() -> Self {
            Self {
                name: "pandoc",
                formats: vec![
                    InputFormat::Docx,
                    InputFormat::Pptx,
                    InputFormat::Xlsx,
                    InputFormat::Pdf,
                    InputFormat::Html,
                    InputFormat::Md,
                ],
                modes: vec![ConversionMode::Fast, ConversionMode::HighQuality],
            }
        }

        fn glm_ocr() -> Self {
            Self {
                name: "glm-ocr",
                formats: vec![
                    InputFormat::Pdf,
                    InputFormat::Docx,
                    InputFormat::Pptx,
                    InputFormat::Xlsx,
                    InputFormat::Image(ImageFormat::Png),
                    InputFormat::Image(ImageFormat::Jpeg),
                    InputFormat::Image(ImageFormat::Tiff),
                ],
                modes: vec![ConversionMode::HighQuality],
            }
        }
    }

    #[async_trait::async_trait]
    impl Converter for StubConverter {
        fn name(&self) -> &str {
            self.name
        }

        fn supported_formats(&self) -> &[InputFormat] {
            &self.formats
        }

        fn supports_mode(&self, mode: ConversionMode) -> bool {
            self.modes.contains(&mode)
        }

        async fn convert(&self, _req: &ConversionRequest) -> Result<ConversionResult, TxtifyError> {
            Err(TxtifyError::UnsupportedFormat(format!(
                "{} stub: not implemented",
                self.name
            )))
        }
    }

    use crate::core::types::ImageFormat;

    fn make_request(input_format: InputFormat, mode: ConversionMode) -> ConversionRequest {
        ConversionRequest {
            input_path: PathBuf::from("test.pdf"),
            output_path: None,
            input_format,
            output_format: OutputFormat::Txt,
            mode,
        }
    }

    fn build_registry() -> ConverterRegistry {
        let mut reg = ConverterRegistry::new();
        reg.register(Box::new(StubConverter::fast()));
        reg.register(Box::new(StubConverter::pandoc()));
        reg.register(Box::new(StubConverter::glm_ocr()));
        reg
    }

    #[test]
    fn select_fast_mode_txt_prefers_fast_converter() {
        let reg = build_registry();
        let req = make_request(InputFormat::Txt, ConversionMode::Fast);
        let c = reg.select(&req).unwrap();
        assert_eq!(c.name(), "fast");
    }

    #[test]
    fn select_fast_mode_docx_skips_fast_selects_pandoc() {
        let reg = build_registry();
        let req = make_request(InputFormat::Docx, ConversionMode::Fast);
        let c = reg.select(&req).unwrap();
        assert_eq!(c.name(), "pandoc");
    }

    #[test]
    fn select_high_quality_pdf_prefers_glm_over_pandoc() {
        let reg = build_registry();
        let req = make_request(InputFormat::Pdf, ConversionMode::HighQuality);
        let c = reg.select(&req).unwrap();
        assert_eq!(c.name(), "glm-ocr");
    }

    #[test]
    fn select_high_quality_image_skips_fast_and_pandoc_selects_glm_ocr() {
        let reg = build_registry();
        let req = make_request(
            InputFormat::Image(ImageFormat::Png),
            ConversionMode::HighQuality,
        );
        let c = reg.select(&req).unwrap();
        assert_eq!(c.name(), "glm-ocr");
    }

    #[test]
    fn select_fast_mode_image_not_supported() {
        let reg = build_registry();
        let req = make_request(InputFormat::Image(ImageFormat::Jpeg), ConversionMode::Fast);
        let res = reg.select(&req);
        assert!(matches!(res, Err(TxtifyError::UnsupportedFormat(_))));
    }

    #[test]
    fn select_csv_not_supported_by_any() {
        let reg = build_registry();
        let req = make_request(InputFormat::Csv, ConversionMode::Fast);
        let res = reg.select(&req);
        assert!(matches!(res, Err(TxtifyError::UnsupportedFormat(_))));
    }

    #[test]
    fn select_high_quality_tiff_selects_glm_ocr_when_only_glm_supports() {
        let reg = build_registry();
        let req = make_request(
            InputFormat::Image(ImageFormat::Tiff),
            ConversionMode::HighQuality,
        );
        let c = reg.select(&req).unwrap();
        assert_eq!(c.name(), "glm-ocr");
    }

    #[test]
    fn registry_len_and_is_empty() {
        let mut reg = ConverterRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);

        reg.register(Box::new(StubConverter::fast()));
        assert!(!reg.is_empty());
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn registry_list_names() {
        let reg = build_registry();
        let names = reg.list();
        assert!(names.contains(&"fast"));
        assert!(names.contains(&"pandoc"));
        assert!(names.contains(&"glm-ocr"));
    }
}
