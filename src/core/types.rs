use std::path::Path;

use serde::{Deserialize, Serialize};

/// Image sub-formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Tiff,
}

/// Input document formats supported for detection.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InputFormat {
    Pdf,
    Docx,
    Xlsx,
    Pptx,
    Html,
    Image(ImageFormat),
    Txt,
    Md,
    Csv,
}

impl InputFormat {
    /// Detect format from file extension.
    pub fn from_extension(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?;
        match ext.to_ascii_lowercase().as_str() {
            "pdf" => Some(Self::Pdf),
            "docx" => Some(Self::Docx),
            "xlsx" => Some(Self::Xlsx),
            "pptx" => Some(Self::Pptx),
            "html" | "htm" => Some(Self::Html),
            "png" => Some(Self::Image(ImageFormat::Png)),
            "jpg" | "jpeg" => Some(Self::Image(ImageFormat::Jpeg)),
            "tiff" | "tif" => Some(Self::Image(ImageFormat::Tiff)),
            "txt" | "text" => Some(Self::Txt),
            "md" | "markdown" => Some(Self::Md),
            "csv" => Some(Self::Csv),
            _ => None,
        }
    }

    /// Detect format from magic bytes.
    pub fn from_magic_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 4 {
            return None;
        }

        // PDF: %PDF
        if bytes.starts_with(b"%PDF") {
            return Some(Self::Pdf);
        }

        // PNG: 89 50 4E 47
        if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
            return Some(Self::Image(ImageFormat::Png));
        }

        // JPEG: FF D8 FF
        if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return Some(Self::Image(ImageFormat::Jpeg));
        }

        // TIFF: 49 49 2A 00 (little-endian) or 4D 4D 00 2A (big-endian)
        if bytes.starts_with(&[0x49, 0x49, 0x2A, 0x00])
            || bytes.starts_with(&[0x4D, 0x4D, 0x00, 0x2A])
        {
            return Some(Self::Image(ImageFormat::Tiff));
        }

        // ZIP-based (DOCX, XLSX, PPTX) - check for PK header, then look for content types
        if bytes.starts_with(&[0x50, 0x4B, 0x03, 0x04]) {
            let content = String::from_utf8_lossy(bytes);
            if content.contains("word/") {
                return Some(Self::Docx);
            }
            if content.contains("xl/") {
                return Some(Self::Xlsx);
            }
            if content.contains("ppt/") {
                return Some(Self::Pptx);
            }
            return Some(Self::Docx); // default ZIP-based to docx
        }

        None
    }

    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pdf => "PDF",
            Self::Docx => "DOCX",
            Self::Xlsx => "XLSX",
            Self::Pptx => "PPTX",
            Self::Html => "HTML",
            Self::Image(ImageFormat::Png) => "PNG",
            Self::Image(ImageFormat::Jpeg) => "JPEG",
            Self::Image(ImageFormat::Tiff) => "TIFF",
            Self::Txt => "TXT",
            Self::Md => "Markdown",
            Self::Csv => "CSV",
        }
    }
}

impl std::fmt::Display for InputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// Output text formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OutputFormat {
    #[default]
    Txt,
    Md,
    Json,
}

/// Conversion quality / execution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ConversionMode {
    /// Pure-Rust, instant conversion.
    #[default]
    Fast,
    /// High-quality conversion via GLM-OCR sidecar.
    HighQuality,
}

/// GLM inference engine selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GlmEngine {
    Transformers,
    Vllm,
    Sglang,
    Ollama,
}

impl GlmEngine {
    /// Auto-detect the best available engine.
    ///
    /// Priority: `GLM_ENGINE` env var > Ollama (if `OLLAMA_HOST` set) > Transformers fallback.
    /// Future: probe vLLM/SGLang endpoints.
    pub fn auto_detect() -> Self {
        if let Ok(val) = std::env::var("GLM_ENGINE") {
            match val.to_ascii_lowercase().as_str() {
                "transformers" => return Self::Transformers,
                "vllm" => return Self::Vllm,
                "sglang" => return Self::Sglang,
                "ollama" => return Self::Ollama,
                _ => {}
            }
        }
        if std::env::var("OLLAMA_HOST").is_ok() {
            return Self::Ollama;
        }
        Self::Transformers
    }
}

/// Request to convert a document to text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionRequest {
    pub input_path: std::path::PathBuf,
    pub output_path: Option<std::path::PathBuf>,
    pub input_format: InputFormat,
    pub output_format: OutputFormat,
    pub mode: ConversionMode,
}

/// Result of a successful conversion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionResult {
    pub output_path: std::path::PathBuf,
    pub markdown: String,
    pub metadata: ConversionMetadata,
}

/// Metadata about a conversion run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionMetadata {
    pub converter_used: String,
    pub duration_ms: u64,
    pub page_count: Option<u32>,
    pub needs_ocr: bool,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn from_extension_pdf() {
        let p = PathBuf::from("report.pdf");
        assert_eq!(InputFormat::from_extension(&p), Some(InputFormat::Pdf));
    }

    #[test]
    fn from_extension_docx() {
        let p = PathBuf::from("document.DOCX");
        assert_eq!(InputFormat::from_extension(&p), Some(InputFormat::Docx));
    }

    #[test]
    fn from_extension_xlsx() {
        let p = PathBuf::from("data.xlsx");
        assert_eq!(InputFormat::from_extension(&p), Some(InputFormat::Xlsx));
    }

    #[test]
    fn from_extension_pptx() {
        let p = PathBuf::from("slides.pptx");
        assert_eq!(InputFormat::from_extension(&p), Some(InputFormat::Pptx));
    }

    #[test]
    fn from_extension_html() {
        let p = PathBuf::from("page.htm");
        assert_eq!(InputFormat::from_extension(&p), Some(InputFormat::Html));
    }

    #[test]
    fn from_extension_image_variants() {
        assert_eq!(
            InputFormat::from_extension(&PathBuf::from("photo.png")),
            Some(InputFormat::Image(ImageFormat::Png))
        );
        assert_eq!(
            InputFormat::from_extension(&PathBuf::from("photo.jpg")),
            Some(InputFormat::Image(ImageFormat::Jpeg))
        );
        assert_eq!(
            InputFormat::from_extension(&PathBuf::from("scan.tiff")),
            Some(InputFormat::Image(ImageFormat::Tiff))
        );
    }

    #[test]
    fn from_extension_txt_md_csv() {
        assert_eq!(
            InputFormat::from_extension(&PathBuf::from("readme.txt")),
            Some(InputFormat::Txt)
        );
        assert_eq!(
            InputFormat::from_extension(&PathBuf::from("readme.md")),
            Some(InputFormat::Md)
        );
        assert_eq!(
            InputFormat::from_extension(&PathBuf::from("data.csv")),
            Some(InputFormat::Csv)
        );
    }

    #[test]
    fn from_extension_unknown() {
        assert_eq!(
            InputFormat::from_extension(&PathBuf::from("file.xyz")),
            None
        );
    }

    #[test]
    fn from_extension_no_extension() {
        assert_eq!(
            InputFormat::from_extension(&PathBuf::from("Makefile")),
            None
        );
    }

    #[test]
    fn from_magic_bytes_pdf() {
        let bytes = b"%PDF-1.4 some content";
        assert_eq!(InputFormat::from_magic_bytes(bytes), Some(InputFormat::Pdf));
    }

    #[test]
    fn from_magic_bytes_png() {
        let bytes = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
        assert_eq!(
            InputFormat::from_magic_bytes(&bytes),
            Some(InputFormat::Image(ImageFormat::Png))
        );
    }

    #[test]
    fn from_magic_bytes_jpeg() {
        let bytes = [0xFF, 0xD8, 0xFF, 0xE0, 0, 0, 0, 0];
        assert_eq!(
            InputFormat::from_magic_bytes(&bytes),
            Some(InputFormat::Image(ImageFormat::Jpeg))
        );
    }

    #[test]
    fn from_magic_bytes_tiff_le() {
        let bytes = [0x49, 0x49, 0x2A, 0x00, 0, 0, 0, 0];
        assert_eq!(
            InputFormat::from_magic_bytes(&bytes),
            Some(InputFormat::Image(ImageFormat::Tiff))
        );
    }

    #[test]
    fn from_magic_bytes_tiff_be() {
        let bytes = [0x4D, 0x4D, 0x00, 0x2A, 0, 0, 0, 0];
        assert_eq!(
            InputFormat::from_magic_bytes(&bytes),
            Some(InputFormat::Image(ImageFormat::Tiff))
        );
    }

    #[test]
    fn from_magic_bytes_too_short() {
        assert_eq!(InputFormat::from_magic_bytes(b"PK"), None);
    }

    #[test]
    fn from_magic_bytes_unknown() {
        let bytes = [0x00, 0x00, 0x00, 0x00];
        assert_eq!(InputFormat::from_magic_bytes(&bytes), None);
    }

    #[test]
    fn display_labels() {
        assert_eq!(InputFormat::Pdf.to_string(), "PDF");
        assert_eq!(InputFormat::Image(ImageFormat::Png).to_string(), "PNG");
        assert_eq!(InputFormat::Txt.to_string(), "TXT");
    }

    #[test]
    fn output_format_default() {
        assert_eq!(OutputFormat::default(), OutputFormat::Txt);
    }

    #[test]
    fn conversion_mode_default() {
        assert_eq!(ConversionMode::default(), ConversionMode::Fast);
    }
}
