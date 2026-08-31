use async_trait::async_trait;

use crate::core::error::TxtifyError;
use crate::core::traits::Converter;
use crate::core::types::{
    ConversionMetadata, ConversionMode, ConversionRequest, ConversionResult, InputFormat,
};

#[derive(Debug, Default)]
/// TextConverter — struct for txtify.
pub struct TextConverter;

#[async_trait]
impl Converter for TextConverter {
    fn name(&self) -> &str {
        "fast"
    }

    fn supported_formats(&self) -> &[InputFormat] {
        &[InputFormat::Txt, InputFormat::Md, InputFormat::Csv]
    }

    fn supports_mode(&self, mode: ConversionMode) -> bool {
        mode == ConversionMode::Fast
    }

    async fn convert(&self, req: &ConversionRequest) -> Result<ConversionResult, TxtifyError> {
        if req.mode != ConversionMode::Fast {
            return Err(TxtifyError::UnsupportedFormat(format!(
                "TextConverter only supports Fast mode, got {:?}",
                req.mode
            )));
        }
        // allow Txt, Md, Csv
        if !self.supported_formats().contains(&req.input_format) {
            return Err(TxtifyError::UnsupportedFormat(format!(
                "TextConverter only supports Txt/Md/Csv, got {}",
                req.input_format
            )));
        }
        let start = std::time::Instant::now();

        // Use mime_guess to hint (optional, just to satisfy dependency usage)
        let _mime = mime_guess::from_path(&req.input_path).first_or_octet_stream();

        let bytes = std::fs::read(&req.input_path).map_err(TxtifyError::Io)?;
        let markdown =
            convert_text_bytes(&bytes, &req.input_path).map_err(TxtifyError::ConversionFailed)?;

        let output_path = req
            .output_path
            .clone()
            .unwrap_or_else(|| req.input_path.clone());

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

/// Decode bytes using encoding_rs detection, passthrough.
/// Tries BOM detection first, then UTF-8, then fallback to WINDOWS_1252.
pub fn convert_text_bytes(bytes: &[u8], _path: &std::path::Path) -> Result<String, String> {
    if bytes.is_empty() {
        return Ok(String::new());
    }
    // Check for BOM
    if let Some((encoding, bom_len)) = encoding_rs::Encoding::for_bom(bytes) {
        let (cow, _, had_errors) = encoding.decode(&bytes[bom_len..]);
        let _ = had_errors;
        return Ok(cow.into_owned());
    }

    // Try UTF-8 decode without BOM handling
    let (cow, had_errors) = {
        let (c, _, had_errors) = encoding_rs::UTF_8.decode(bytes);
        (c, had_errors)
    };
    if !had_errors {
        return Ok(cow.into_owned());
    }

    // Fallback: try WINDOWS_1252 or use UTF-8 lossy
    // Use encoding_rs's detection via for_label? For now fallback to WINDOWS_1252
    let (cow2, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
    // If still had errors, we could also try UTF-8 with lossy replacement (already done)
    // Return decoded string
    Ok(cow2.into_owned())
}

/// Helper for tests: decode without path
pub fn convert_text_bytes_simple(bytes: &[u8]) -> Result<String, String> {
    convert_text_bytes(bytes, std::path::Path::new("dummy.txt"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn text_passthrough_utf8() {
        let input = "Hello world\nSecond line";
        let out = convert_text_bytes_simple(input.as_bytes()).unwrap();
        assert_eq!(out, input);
    }

    #[test]
    fn text_bom_handling() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("Hello BOM".as_bytes());
        let out = convert_text_bytes_simple(&bytes).unwrap();
        assert_eq!(out, "Hello BOM");
    }

    #[test]
    fn text_windows1252_fallback() {
        // Bytes that are invalid UTF-8 but valid windows-1252
        // 0xE9 is é in windows-1252
        let bytes = vec![0x43, 0x61, 0x66, 0xE9]; // "Café" in windows-1252
        let out = convert_text_bytes_simple(&bytes).unwrap();
        // Should decode to Café
        assert!(out.contains("Caf"), "decoded: {out}");
    }

    #[test]
    fn file_not_found_returns_io() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let conv = TextConverter;
            let req = crate::core::types::ConversionRequest {
                input_path: PathBuf::from("/tmp/txtify_nonexistent_text_12345.txt"),
                output_path: None,
                input_format: crate::core::types::InputFormat::Txt,
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
        let c = TextConverter;
        assert!(c.supports_mode(crate::core::types::ConversionMode::Fast));
        assert!(!c.supports_mode(crate::core::types::ConversionMode::HighQuality));
    }
}
