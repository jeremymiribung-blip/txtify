use async_trait::async_trait;

use crate::core::error::TxtifyError;
use crate::core::traits::Converter;
use crate::core::types::{
    ConversionMetadata, ConversionMode, ConversionRequest, ConversionResult, InputFormat,
};

#[derive(Debug, Default)]
/// HtmlConverter — struct for txtify.
pub struct HtmlConverter;

#[async_trait]
impl Converter for HtmlConverter {
    fn name(&self) -> &str {
        "fast"
    }

    fn supported_formats(&self) -> &[InputFormat] {
        &[InputFormat::Html]
    }

    fn supports_mode(&self, mode: ConversionMode) -> bool {
        mode == ConversionMode::Fast
    }

    async fn convert(&self, req: &ConversionRequest) -> Result<ConversionResult, TxtifyError> {
        if req.mode != ConversionMode::Fast {
            return Err(TxtifyError::UnsupportedFormat(format!(
                "HtmlConverter only supports Fast mode, got {:?}",
                req.mode
            )));
        }
        if req.input_format != InputFormat::Html {
            return Err(TxtifyError::UnsupportedFormat(format!(
                "HtmlConverter only supports Html, got {}",
                req.input_format
            )));
        }
        let start = std::time::Instant::now();
        let bytes = std::fs::read(&req.input_path).map_err(TxtifyError::Io)?;
        let markdown = convert_html_bytes(&bytes).map_err(TxtifyError::ConversionFailed)?;

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
                page_count: None,
                needs_ocr: false,
            },
        })
    }
}

/// convert_html_bytes — fn for txtify.
pub fn convert_html_bytes(bytes: &[u8]) -> Result<String, String> {
    // Use encoding_rs to decode bytes (like TextConverter) to handle non-UTF8
    let (cow, _, had_errors) = {
        // Check BOM
        if let Some((encoding, bom_len)) = encoding_rs::Encoding::for_bom(bytes) {
            let (decoded, _, _) = encoding.decode(&bytes[bom_len..]);
            (decoded, encoding, false)
        } else {
            // Try UTF-8 first
            let (decoded, encoding_used, had_errors) = encoding_rs::UTF_8.decode(bytes);
            if !had_errors {
                (decoded, encoding_used, false)
            } else {
                // Fallback to windows-1252 for legacy html
                let (decoded2, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
                (decoded2, encoding_rs::WINDOWS_1252, had_errors)
            }
        }
    };
    let _ = had_errors;
    let html_str = cow.into_owned();
    // Use html2md to convert
    let md = html2md::parse_html(&html_str);
    Ok(md.trim().to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn html_heading_and_table() {
        let html = r#"<html><body>
<h1>Heading One</h1>
<p>Paragraph with <strong>bold</strong> and <em>italic</em></p>
<table><tr><th>Header1</th><th>Header2</th></tr><tr><td>Cell1</td><td>Cell2</td></tr></table>
<ul><li>Item 1</li><li>Item 2</li></ul>
</body></html>"#;
        let md = convert_html_bytes(html.as_bytes()).unwrap();
        assert!(
            md.contains("# Heading One") || md.contains("Heading One"),
            "heading missing: {md}"
        );
        assert!(
            md.contains("Header1") && md.contains("Header2"),
            "table headers missing: {md}"
        );
        assert!(md.contains("Cell1"), "table cell missing: {md}");
    }

    #[test]
    fn file_not_found_returns_io() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let conv = HtmlConverter;
            let req = crate::core::types::ConversionRequest {
                input_path: PathBuf::from("/tmp/txtify_nonexistent_html_12345.html"),
                output_path: None,
                input_format: crate::core::types::InputFormat::Html,
                output_format: crate::core::types::OutputFormat::Md,
                mode: crate::core::types::ConversionMode::Fast,
            };
            let err = conv.convert(&req).await.unwrap_err();
            assert!(
                matches!(err, crate::core::error::TxtifyError::Io(_)),
                "expected Io, got {err:?}"
            );
        });
    }

    #[test]
    fn supports_only_fast() {
        let c = HtmlConverter;
        assert!(c.supports_mode(crate::core::types::ConversionMode::Fast));
        assert!(!c.supports_mode(crate::core::types::ConversionMode::HighQuality));
    }
}
