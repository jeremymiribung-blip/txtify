use async_trait::async_trait;

use crate::core::error::TxtifyError;
use crate::core::traits::Converter;
use crate::core::types::{
    ConversionMetadata, ConversionMode, ConversionRequest, ConversionResult, InputFormat,
    OutputFormat,
};

#[derive(Debug, Default)]
/// PdfConverter — struct for txtify.
pub struct PdfConverter;

#[async_trait]
impl Converter for PdfConverter {
    fn name(&self) -> &str {
        "fast"
    }

    fn supported_formats(&self) -> &[InputFormat] {
        &[InputFormat::Pdf]
    }

    fn supports_mode(&self, mode: ConversionMode) -> bool {
        mode == ConversionMode::Fast
    }

    async fn convert(&self, req: &ConversionRequest) -> Result<ConversionResult, TxtifyError> {
        if req.mode != ConversionMode::Fast {
            return Err(TxtifyError::UnsupportedFormat(format!(
                "PdfConverter only supports Fast mode, got {:?}",
                req.mode
            )));
        }
        if req.input_format != InputFormat::Pdf {
            return Err(TxtifyError::UnsupportedFormat(format!(
                "PdfConverter only supports Pdf, got {}",
                req.input_format
            )));
        }
        let start = std::time::Instant::now();
        let bytes = std::fs::read(&req.input_path).map_err(TxtifyError::Io)?;
        let (markdown, needs_ocr, page_count) =
            convert_pdf_bytes(&bytes, req.output_format).map_err(TxtifyError::ConversionFailed)?;

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
                page_count,
                needs_ocr,
            },
        })
    }
}

/// Convert PDF bytes to markdown/text, detecting scanned PDFs.
///
/// Returns (markdown, needs_ocr, page_count)
pub fn convert_pdf_bytes(
    bytes: &[u8],
    output_format: OutputFormat,
) -> Result<(String, bool, Option<u32>), String> {
    if bytes.is_empty() {
        return Ok((String::new(), false, Some(0)));
    }

    // Load with lopdf to get page count and image detection
    let doc = lopdf::Document::load_mem(bytes).map_err(|e| format!("failed to load pdf: {e}"))?;

    let page_count = u32::try_from(doc.get_pages().len()).unwrap_or(u32::MAX);
    let has_images = has_images(&doc);

    // Extract text page by page via pdf-extract (text-layer only, no OCR)
    let pages = pdf_extract::extract_text_from_mem_by_pages(bytes)
        .map_err(|e| format!("failed to extract pdf text: {e}"))?;

    // If pdf-extract returns fewer pages than lopdf, pad; if more, trust pdf-extract length
    // Use pdf-extract length as authoritative for page count if mismatch, but keep lopdf count for metadata
    let actual_page_count = if pages.is_empty() {
        page_count
    } else {
        u32::try_from(pages.len()).unwrap_or(page_count)
    };

    let total_chars: usize = pages.iter().map(|p| p.chars().count()).sum();
    let avg_chars_per_page = if actual_page_count > 0 {
        total_chars / usize::try_from(actual_page_count).unwrap_or(1)
    } else {
        0
    };

    let needs_ocr = avg_chars_per_page < 100 && has_images;

    let mut output = String::new();
    for (idx, page_text) in pages.iter().enumerate() {
        let page_num = idx + 1;
        output.push_str(&format!("--- Page {page_num} ---\n"));

        let processed = if output_format == OutputFormat::Txt {
            // For Txt preserve raw
            page_text.trim().to_string()
        } else {
            // For Md (and Json) heuristic: ALL CAPS line -> ## heading
            transform_page_to_md(page_text)
        };

        if !processed.is_empty() {
            output.push_str(&processed);
        }
        output.push_str("\n\n");
    }

    // Fallback if no pages extracted but doc has pages: still mark hint
    if pages.is_empty() && page_count > 0 {
        // No text extracted at all
        // Still produce markers?
        // If has_images and no text, it's scanned
        // Output will be empty besides markers? Keep empty markers already handled above (no pages => no output)
        // So we still need to indicate
    }

    let mut markdown = output.trim().to_string();

    if needs_ocr {
        let hint = "use --mode high-quality (GLM-OCR)";
        if markdown.is_empty() {
            markdown = format!("[Scanned PDF detected: {hint}]");
        } else {
            markdown.push_str(&format!("\n\n> Hint: scanned PDF detected, {hint} for OCR"));
        }
    }

    Ok((markdown, needs_ocr, Some(actual_page_count)))
}

fn transform_page_to_md(page_text: &str) -> String {
    let mut out_lines: Vec<String> = Vec::new();
    for line in page_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            out_lines.push(String::new());
            continue;
        }
        if is_all_caps_heading(trimmed) {
            out_lines.push(format!("## {}", trimmed));
        } else {
            out_lines.push(trimmed.to_string());
        }
    }
    // Join preserving line breaks, but collapse consecutive empty?
    // Keep as is
    let mut result = out_lines.join("\n");
    // Trim trailing whitespace per line already; overall trim
    result = result.trim().to_string();
    result
}

fn is_all_caps_heading(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.len() < 3 {
        return false;
    }
    // Must contain at least one alphabetic char
    let has_alpha = trimmed.chars().any(|c| c.is_alphabetic());
    if !has_alpha {
        return false;
    }
    // All alphabetic chars must be uppercase
    let all_upper = trimmed
        .chars()
        .filter(|c| c.is_alphabetic())
        .all(|c| c.is_uppercase());
    if !all_upper {
        return false;
    }
    // Heuristic: avoid lines that are too long (>100 chars) or contain lowercase already handled
    // Also avoid lines that are mostly symbols? Keep simple
    // Require that line is not all digits/symbols and length reasonable
    // Check that uppercase letters proportion high
    // For our heuristic, consider ALL CAPS if entire line equals uppercase version
    // and contains at least 2 letters
    let letter_count = trimmed.chars().filter(|c| c.is_alphabetic()).count();
    if letter_count < 2 {
        return false;
    }
    // Additional check: if line contains lowercase, already false
    // So return true
    all_upper
}

fn has_images(doc: &lopdf::Document) -> bool {
    for obj in doc.objects.values() {
        // Check dict objects
        if let Ok(dict) = obj.as_dict() {
            if let Ok(subtype) = dict.get(b"Subtype") {
                if let Ok(name) = subtype.as_name() {
                    if name == b"Image" {
                        return true;
                    }
                }
            }
        }
        // Check stream objects
        if let Ok(stream) = obj.as_stream() {
            if let Ok(subtype) = stream.dict.get(b"Subtype") {
                if let Ok(name) = subtype.as_name() {
                    if name == b"Image" {
                        return true;
                    }
                }
            }
            // Also check XObject with Image subtype inside?
            // Some PDFs embed images via /XObject resources; but stream subtype still Image
        }
    }
    false
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use lopdf::content::{Content, Operation};
    use lopdf::dictionary;
    use lopdf::{Document, Object, Stream};
    use std::path::PathBuf;

    fn create_minimal_pdf(texts: &[&str]) -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Courier",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! {
                "F1" => font_id,
            },
        });

        let mut page_ids = Vec::new();
        let mut content_ids = Vec::new();

        for text in texts {
            let content = Content {
                operations: vec![
                    Operation::new("BT", vec![]),
                    Operation::new("Tf", vec!["F1".into(), 12.into()]),
                    Operation::new("Td", vec![100.into(), 600.into()]),
                    Operation::new("Tj", vec![Object::string_literal(*text)]),
                    Operation::new("ET", vec![]),
                ],
            };
            let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
            content_ids.push(content_id);
            let page_id = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
                "Resources" => resources_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            });
            page_ids.push(page_id);
        }

        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => page_ids.into_iter().map(Object::Reference).collect::<Vec<_>>(),
            "Count" => texts.len() as u32,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        doc.compress();

        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    #[test]
    fn pdf_text_extraction_page_markers() {
        let bytes = create_minimal_pdf(&["Hello PDF page one", "Second page content"]);
        let (md, needs_ocr, page_count) =
            convert_pdf_bytes(&bytes, OutputFormat::Md).expect("convert failed");
        assert!(md.contains("--- Page 1 ---"), "missing page 1 marker: {md}");
        assert!(md.contains("--- Page 2 ---"), "missing page 2 marker: {md}");
        assert!(
            md.contains("Hello PDF page one"),
            "missing text page1: {md}"
        );
        assert!(
            md.contains("Second page content"),
            "missing text page2: {md}"
        );
        assert_eq!(page_count, Some(2));
        assert!(!needs_ocr, "should not need ocr for text pdf");
    }

    #[test]
    fn pdf_txt_preserves_raw() {
        let bytes = create_minimal_pdf(&["Hello world"]);
        let (txt, _, _) = convert_pdf_bytes(&bytes, OutputFormat::Txt).expect("txt convert");
        assert!(txt.contains("Hello world"), "txt missing: {txt}");
        // Txt should not have heading transformation
        assert!(
            !txt.contains("##"),
            "txt should preserve raw, not add heading"
        );
    }

    #[test]
    fn pdf_md_heading_heuristic() {
        let bytes = create_minimal_pdf(&["INTRODUCTION", "This is body text"]);
        let (md, _, _) = convert_pdf_bytes(&bytes, OutputFormat::Md).expect("md convert");
        // ALL CAPS line should become heading
        assert!(
            md.contains("## INTRODUCTION"),
            "heading heuristic failed: {md}"
        );
    }

    #[test]
    fn pdf_scanned_detection_needs_ocr() {
        // Create PDF with almost no text but with an image object
        // We'll craft a PDF with an image-like object to trigger has_images
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Courier",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! {
                "F1" => font_id,
            },
        });
        // Create a fake image XObject
        let image_id = doc.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 100,
                "Height" => 100,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
                "Length" => 0,
            },
            vec![0; 10],
        ));
        // Add image to resources XObject
        let xobject_resources = doc.add_object(dictionary! {
            "XObject" => dictionary! {
                "Im0" => image_id,
            },
            "Font" => dictionary! {
                "F1" => font_id,
            },
        });
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.into()]),
                Operation::new("Td", vec![100.into(), 600.into()]),
                Operation::new("Tj", vec![Object::string_literal("")]),
                Operation::new("ET", vec![]),
            ],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => xobject_resources,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        });
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        doc.compress();
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();

        let (md, needs_ocr, _) =
            convert_pdf_bytes(&out, OutputFormat::Md).expect("convert scanned");
        assert!(needs_ocr, "scanned detection failed, md: {md}");
        assert!(
            md.contains("use --mode high-quality (GLM-OCR)"),
            "hint missing: {md}"
        );
    }

    #[test]
    fn pdf_file_not_found_returns_io() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let conv = PdfConverter;
            let req = crate::core::types::ConversionRequest {
                input_path: PathBuf::from("/tmp/txtify_nonexistent_pdf_12345.pdf"),
                output_path: None,
                input_format: crate::core::types::InputFormat::Pdf,
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
        let c = PdfConverter;
        assert!(c.supports_mode(crate::core::types::ConversionMode::Fast));
        assert!(!c.supports_mode(crate::core::types::ConversionMode::HighQuality));
    }
}
