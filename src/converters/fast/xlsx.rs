use async_trait::async_trait;
use calamine::{open_workbook, Data, Reader, Xlsx};
use std::fs::File;
use std::io::BufReader;

use crate::core::error::TxtifyError;
use crate::core::traits::Converter;
use crate::core::types::{
    ConversionMetadata, ConversionMode, ConversionRequest, ConversionResult, InputFormat,
};

#[derive(Debug, Default)]
/// XlsxConverter — struct for txtify.
pub struct XlsxConverter;

#[async_trait]
impl Converter for XlsxConverter {
    fn name(&self) -> &str {
        "fast"
    }

    fn supported_formats(&self) -> &[InputFormat] {
        &[InputFormat::Xlsx]
    }

    fn supports_mode(&self, mode: ConversionMode) -> bool {
        mode == ConversionMode::Fast
    }

    async fn convert(&self, req: &ConversionRequest) -> Result<ConversionResult, TxtifyError> {
        if req.mode != ConversionMode::Fast {
            return Err(TxtifyError::UnsupportedFormat(format!(
                "XlsxConverter only supports Fast mode, got {:?}",
                req.mode
            )));
        }
        if req.input_format != InputFormat::Xlsx {
            return Err(TxtifyError::UnsupportedFormat(format!(
                "XlsxConverter only supports Xlsx, got {}",
                req.input_format
            )));
        }
        let start = std::time::Instant::now();
        // Ensure file exists to return Io on not found
        std::fs::metadata(&req.input_path).map_err(TxtifyError::Io)?;

        let markdown = convert_xlsx_path(&req.input_path).map_err(TxtifyError::ConversionFailed)?;

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

/// convert_xlsx_path — fn for txtify.
pub fn convert_xlsx_path(path: &std::path::Path) -> Result<String, String> {
    let mut workbook: Xlsx<BufReader<File>> =
        open_workbook(path).map_err(|e| format!("failed to read xlsx: {e}"))?;
    xlsx_to_markdown(&mut workbook)
}

fn xlsx_to_markdown(workbook: &mut Xlsx<BufReader<File>>) -> Result<String, String> {
    let sheet_names = workbook.sheet_names().to_owned();
    if sheet_names.is_empty() {
        return Ok(String::new());
    }
    let mut md = String::new();
    for sheet_name in sheet_names {
        let range = match workbook.worksheet_range(&sheet_name) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if range.is_empty() {
            // still add sheet heading?
            md.push_str(&format!("## {sheet_name}\n\n"));
            continue;
        }
        md.push_str(&format!("## {sheet_name}\n\n"));
        let rows: Vec<Vec<String>> = range
            .rows()
            .map(|row| {
                row.iter()
                    .map(|cell| format_cell(cell).replace('|', "\\|").trim().to_string())
                    .collect()
            })
            .collect();

        if rows.is_empty() {
            continue;
        }
        // Ensure at least one row; treat first row as header
        let max_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        let normalized: Vec<Vec<String>> = rows
            .into_iter()
            .map(|mut r| {
                while r.len() < max_cols {
                    r.push(String::new());
                }
                r
            })
            .collect();

        // Header
        md.push_str("| ");
        md.push_str(&normalized[0].join(" | "));
        md.push_str(" |\n");
        md.push_str("| ");
        let sep: Vec<String> = (0..max_cols).map(|_| "---".to_string()).collect();
        md.push_str(&sep.join(" | "));
        md.push_str(" |\n");
        for row in normalized.iter().skip(1) {
            // skip rows that are entirely empty
            if row.iter().all(|c| c.trim().is_empty()) {
                continue;
            }
            md.push_str("| ");
            md.push_str(&row.join(" | "));
            md.push_str(" |\n");
        }
        md.push('\n');
    }
    Ok(md.trim().to_string())
}

fn format_cell(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(s) => s.clone(),
        Data::Float(f) => {
            // Avoid trailing .0 if integer-like? Keep as is but remove unnecessary trailing zeros?
            // Use default to_string but if it's like 15.0, calamine may store as 15.0, we should output 15 if possible?
            // Let's check: if f.fract() == 0.0, format as integer
            if f.fract() == 0.0 {
                // check if within i64 range
                // Format without decimal
                format!("{}", *f as i64)
            } else {
                // Use to_string but trim trailing zeros?
                let s = format!("{f}");
                s
            }
        }
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::Error(e) => format!("{e}"),
        Data::DateTime(d) => format!("{d}"),
        Data::DateTimeIso(s) => s.clone(),
        Data::DurationIso(s) => s.clone(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn xlsx_heading_and_table() {
        // Create a minimal xlsx in memory via zip + manual xml? Simpler to use the fixture generation helper
        // For unit test, create a temp file using zip manual construction via calamine's expectation
        // Instead, we construct an xlsx file using zip crate manually with minimal valid structure
        let temp_path = std::env::temp_dir().join("txtify_test_minimal.xlsx");
        create_minimal_xlsx(&temp_path).expect("create xlsx");

        let md = convert_xlsx_path(&temp_path).unwrap();
        // Clean up
        let _ = std::fs::remove_file(&temp_path);
        assert!(md.contains("## Sheet1"), "sheet heading missing: {md}");
        assert!(md.contains("| Name | Age |"), "header missing: {md}");
        assert!(
            md.contains("| Alice | 30 |") || md.contains("| Alice |"),
            "row missing: {md}"
        );
    }

    fn create_minimal_xlsx(path: &std::path::Path) -> Result<(), String> {
        // Build minimal xlsx using zip crate
        use std::io::Write;
        let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        // [Content_Types].xml
        zip.start_file("[Content_Types].xml", options)
            .map_err(|e| e.to_string())?;
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>
<Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>
</Types>"#).map_err(|e| e.to_string())?;

        // _rels/.rels
        zip.start_file("_rels/.rels", options)
            .map_err(|e| e.to_string())?;
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"#).map_err(|e| e.to_string())?;

        // xl/workbook.xml
        zip.start_file("xl/workbook.xml", options)
            .map_err(|e| e.to_string())?;
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<sheets>
<sheet name="Sheet1" sheetId="1" r:id="rId1"/>
</sheets>
</workbook>"#).map_err(|e| e.to_string())?;

        // xl/_rels/workbook.xml.rels
        zip.start_file("xl/_rels/workbook.xml.rels", options)
            .map_err(|e| e.to_string())?;
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/>
</Relationships>"#).map_err(|e| e.to_string())?;

        // xl/styles.xml
        zip.start_file("xl/styles.xml", options)
            .map_err(|e| e.to_string())?;
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"></styleSheet>"#,
        )
        .map_err(|e| e.to_string())?;

        // xl/sharedStrings.xml (empty)
        zip.start_file("xl/sharedStrings.xml", options)
            .map_err(|e| e.to_string())?;
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="0" uniqueCount="0"></sst>"#).map_err(|e| e.to_string())?;

        // xl/worksheets/sheet1.xml
        zip.start_file("xl/worksheets/sheet1.xml", options)
            .map_err(|e| e.to_string())?;
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<sheetData>
<row r="1">
<c r="A1" t="inlineStr"><is><t>Name</t></is></c>
<c r="B1" t="inlineStr"><is><t>Age</t></is></c>
</row>
<row r="2">
<c r="A2" t="inlineStr"><is><t>Alice</t></is></c>
<c r="B2"><v>30</v></c>
</row>
<row r="3">
<c r="A3" t="inlineStr"><is><t>Bob</t></is></c>
<c r="B3"><v>25</v></c>
</row>
</sheetData>
</worksheet>"#,
        )
        .map_err(|e| e.to_string())?;

        // docProps/core.xml (minimal)
        zip.start_file("docProps/core.xml", options)
            .map_err(|e| e.to_string())?;
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties"></cp:coreProperties>"#).map_err(|e| e.to_string())?;

        // docProps/app.xml
        zip.start_file("docProps/app.xml", options)
            .map_err(|e| e.to_string())?;
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"></Properties>"#).map_err(|e| e.to_string())?;

        zip.finish().map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn file_not_found_returns_io() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let conv = XlsxConverter;
            let req = crate::core::types::ConversionRequest {
                input_path: PathBuf::from("/tmp/txtify_nonexistent_xlsx_12345.xlsx"),
                output_path: None,
                input_format: crate::core::types::InputFormat::Xlsx,
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
        let c = XlsxConverter;
        assert!(c.supports_mode(crate::core::types::ConversionMode::Fast));
        assert!(!c.supports_mode(crate::core::types::ConversionMode::HighQuality));
    }
}
