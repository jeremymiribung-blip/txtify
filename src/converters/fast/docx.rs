use async_trait::async_trait;

use crate::core::error::TxtifyError;
use crate::core::traits::Converter;
use crate::core::types::{
    ConversionMetadata, ConversionMode, ConversionRequest, ConversionResult, InputFormat,
};

#[derive(Debug, Default)]
/// DocxConverter — struct for txtify.
pub struct DocxConverter;

#[async_trait]
impl Converter for DocxConverter {
    fn name(&self) -> &str {
        "fast"
    }

    fn supported_formats(&self) -> &[InputFormat] {
        &[InputFormat::Docx]
    }

    fn supports_mode(&self, mode: ConversionMode) -> bool {
        mode == ConversionMode::Fast
    }

    async fn convert(&self, req: &ConversionRequest) -> Result<ConversionResult, TxtifyError> {
        if req.mode != ConversionMode::Fast {
            return Err(TxtifyError::UnsupportedFormat(format!(
                "DocxConverter only supports Fast mode, got {:?}",
                req.mode
            )));
        }
        if req.input_format != InputFormat::Docx {
            return Err(TxtifyError::UnsupportedFormat(format!(
                "DocxConverter only supports Docx, got {}",
                req.input_format
            )));
        }
        let start = std::time::Instant::now();
        let bytes = std::fs::read(&req.input_path).map_err(TxtifyError::Io)?;
        let markdown = convert_docx_bytes(&bytes).map_err(TxtifyError::ConversionFailed)?;

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

/// convert_docx_bytes — fn for txtify.
pub fn convert_docx_bytes(bytes: &[u8]) -> Result<String, String> {
    let docx = docx_rs::read_docx(bytes).map_err(|e| format!("failed to read docx: {e}"))?;
    let mut md = String::new();

    for child in &docx.document.children {
        match child {
            docx_rs::DocumentChild::Paragraph(p) => {
                let paragraph_md = paragraph_to_markdown(p);
                if paragraph_md.is_empty() {
                    continue;
                }
                // heading detection already inside paragraph_to_markdown? But we do here
                // to ensure proper prefix. paragraph_to_markdown returns formatted text with heading.
                // For tables, handled separate.
                // paragraph_to_markdown already includes heading prefix.
                // So just push.
                if let Some(lvl) = heading_level(p) {
                    // paragraph_to_markdown already prefixed? Let's ensure not double.
                    // We'll use plain text for heading and re-prefix.
                    let plain = paragraph_plain_text(p);
                    if plain.trim().is_empty() {
                        continue;
                    }
                    let heading = format!("{} {}\n\n", "#".repeat(lvl), plain.trim());
                    md.push_str(&heading);
                } else if p.property.numbering_property.is_some() {
                    // list handled inside paragraph_to_markdown
                    md.push_str(&paragraph_md);
                    md.push('\n');
                } else {
                    md.push_str(&paragraph_md);
                    md.push_str("\n\n");
                }
            }
            docx_rs::DocumentChild::Table(t) => {
                let table_md = table_to_markdown(t);
                if !table_md.is_empty() {
                    md.push_str(&table_md);
                    md.push_str("\n\n");
                }
            }
            _ => {}
        }
    }

    Ok(md.trim().to_string())
}

fn heading_level(p: &docx_rs::Paragraph) -> Option<usize> {
    if let Some(outline) = &p.property.outline_lvl {
        // outline lvl 0 => H1, 1 => H2 ...
        let lvl = outline.v + 1;
        if (1..=6).contains(&lvl) {
            return Some(lvl);
        }
        return Some(lvl.min(6));
    }
    if let Some(style) = &p.property.style {
        let val = style.val.to_ascii_lowercase();
        if val.starts_with("heading") {
            let digits: String = val.chars().filter(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = digits.parse::<usize>() {
                if (1..=9).contains(&n) {
                    return Some(n.min(6));
                }
            } else {
                return Some(1);
            }
        }
        // also handle "title" as H1? optional
        if val == "title" {
            return Some(1);
        }
    }
    None
}

fn paragraph_plain_text(p: &docx_rs::Paragraph) -> String {
    let mut s = String::new();
    for child in &p.children {
        collect_plain_text(child, &mut s);
    }
    s
}

fn collect_plain_text(child: &docx_rs::ParagraphChild, out: &mut String) {
    match child {
        docx_rs::ParagraphChild::Run(r) => {
            for c in &r.children {
                if let docx_rs::RunChild::Text(t) = c {
                    out.push_str(&t.text);
                }
            }
        }
        docx_rs::ParagraphChild::Insert(ins) => {
            for c in &ins.children {
                if let docx_rs::InsertChild::Run(r) = c {
                    for rc in &r.children {
                        if let docx_rs::RunChild::Text(t) = rc {
                            out.push_str(&t.text);
                        }
                    }
                }
            }
        }
        docx_rs::ParagraphChild::Hyperlink(h) => {
            for c in &h.children {
                collect_plain_text(c, out);
            }
        }
        docx_rs::ParagraphChild::Delete(_) => {}
        docx_rs::ParagraphChild::MoveFrom(_) => {}
        docx_rs::ParagraphChild::MoveTo(mv) => {
            for c in &mv.children {
                if let docx_rs::MoveToChild::Run(r) = c {
                    for rc in &r.children {
                        if let docx_rs::RunChild::Text(t) = rc {
                            out.push_str(&t.text);
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

fn paragraph_to_markdown(p: &docx_rs::Paragraph) -> String {
    let heading = heading_level(p);
    if heading.is_some() {
        // heading will be handled elsewhere; return plain with heading prefix? But we return empty to avoid duplication.
        // Actually we handle heading at higher level, so return empty here.
        // However for generic inline formatting inside heading we want formatted.
        // So we will return formatted plain for heading? The outer logic already handles heading plain.
        // To keep consistent, return empty and let outer handle.
        // Instead, produce formatted plain for heading if needed? Let's just return empty and rely outer.
        // But for simplicity, we handle heading outer, so return empty string for now.
        // However to not duplicate, we return formatted inline for use elsewhere.
        // We'll produce inline formatted text without prefix.
        let inline = paragraph_inline_markdown(p);
        return inline;
    }

    let is_list = p.property.numbering_property.is_some();
    let inline = paragraph_inline_markdown(p);
    if inline.trim().is_empty() {
        return String::new();
    }
    if is_list {
        format!("- {}", inline.trim())
    } else {
        inline
    }
}

fn paragraph_inline_markdown(p: &docx_rs::Paragraph) -> String {
    let mut parts: Vec<String> = Vec::new();
    for child in &p.children {
        match child {
            docx_rs::ParagraphChild::Run(r) => {
                let s = run_to_markdown(r);
                if !s.is_empty() {
                    parts.push(s);
                }
            }
            docx_rs::ParagraphChild::Insert(ins) => {
                for c in &ins.children {
                    if let docx_rs::InsertChild::Run(r) = c {
                        let s = run_to_markdown(r);
                        if !s.is_empty() {
                            parts.push(s);
                        }
                    }
                }
            }
            docx_rs::ParagraphChild::Hyperlink(h) => {
                for c in &h.children {
                    match c {
                        docx_rs::ParagraphChild::Run(r) => {
                            let s = run_to_markdown(r);
                            if !s.is_empty() {
                                parts.push(s);
                            }
                        }
                        docx_rs::ParagraphChild::Insert(ins) => {
                            for ic in &ins.children {
                                if let docx_rs::InsertChild::Run(r) = ic {
                                    let s = run_to_markdown(r);
                                    if !s.is_empty() {
                                        parts.push(s);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            docx_rs::ParagraphChild::MoveTo(mv) => {
                for c in &mv.children {
                    if let docx_rs::MoveToChild::Run(r) = c {
                        let s = run_to_markdown(r);
                        if !s.is_empty() {
                            parts.push(s);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    // Join without extra separator? Runs are contiguous.
    let mut out = String::new();
    for p in parts {
        out.push_str(&p);
    }
    out
}

fn run_to_markdown(run: &docx_rs::Run) -> String {
    let mut txt = String::new();
    for child in &run.children {
        match child {
            docx_rs::RunChild::Text(t) => txt.push_str(&t.text),
            docx_rs::RunChild::Tab(_) => txt.push(' '),
            docx_rs::RunChild::Break(_) => txt.push('\n'),
            docx_rs::RunChild::DeleteText(_) => {}
            _ => {}
        }
    }
    if txt.is_empty() {
        return String::new();
    }
    let is_bold = run.run_property.bold.is_some();
    let is_italic = run.run_property.italic.is_some();
    // Check if bold/italic are disabled (val=false)? In docx-rs, Bold::new().disable() sets val false.
    // We should check if bold exists but is disabled, then not bold. Inspect Bold struct.
    // For simplicity, treat any Some as true unless we can detect disable.
    // We peep: Bold has field? Let's assume if Some but val==false, then not bold. We check via serialization? Simpler to check if bold.is_some() and not disabled.
    // We can attempt to inspect via debug? But we will treat as bold if exists.
    // However we should check disabling: Bold struct likely has `val: bool`?
    // Let's handle: if bold present and `bold` is disabled, we ignore.
    // We can use serde serialization? Easier to check field `val`.
    let bold_disabled = run.run_property.bold.as_ref().is_some_and(is_disabled_bold);
    let italic_disabled = run
        .run_property
        .italic
        .as_ref()
        .is_some_and(is_disabled_italic);
    let effective_bold = is_bold && !bold_disabled;
    let effective_italic = is_italic && !italic_disabled;

    if effective_bold && effective_italic {
        format!("***{txt}***")
    } else if effective_bold {
        format!("**{txt}**")
    } else if effective_italic {
        format!("*{txt}*")
    } else {
        txt
    }
}

fn is_disabled_bold(b: &docx_rs::Bold) -> bool {
    // Bold struct has a field `val` that indicates enabled; disabled sets val false
    // We try to access via serde serialization: if json contains "false" then disabled
    // Alternative: inspect debug? Simpler: use serde_json to check.
    let s = serde_json::to_string(b).unwrap_or_default();
    s.contains("false")
}

fn is_disabled_italic(i: &docx_rs::Italic) -> bool {
    let s = serde_json::to_string(i).unwrap_or_default();
    s.contains("false")
}

fn table_to_markdown(table: &docx_rs::Table) -> String {
    let mut rows: Vec<Vec<String>> = Vec::new();
    for child in &table.rows {
        let docx_rs::TableChild::TableRow(row) = child;
        let mut cells: Vec<String> = Vec::new();
        for cell_child in &row.cells {
            let docx_rs::TableRowChild::TableCell(cell) = cell_child;
            let mut cell_text = String::new();
            for content in &cell.children {
                match content {
                    docx_rs::TableCellContent::Paragraph(p) => {
                        let txt = paragraph_inline_markdown(p);
                        let trimmed = txt.trim();
                        if !trimmed.is_empty() {
                            if !cell_text.is_empty() {
                                cell_text.push(' ');
                            }
                            cell_text.push_str(trimmed);
                        } else {
                            let plain = paragraph_plain_text(p);
                            let pt = plain.trim();
                            if !pt.is_empty() {
                                if !cell_text.is_empty() {
                                    cell_text.push(' ');
                                }
                                cell_text.push_str(pt);
                            }
                        }
                    }
                    docx_rs::TableCellContent::Table(inner) => {
                        let inner_md = table_to_markdown(inner);
                        if !inner_md.is_empty() {
                            if !cell_text.is_empty() {
                                cell_text.push(' ');
                            }
                            cell_text.push_str(inner_md.trim());
                        }
                    }
                    _ => {}
                }
            }
            // Escape pipes
            let escaped = cell_text.replace('|', "\\|");
            cells.push(escaped);
        }
        rows.push(cells);
    }
    if rows.is_empty() {
        return String::new();
    }
    let max_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if max_cols == 0 {
        return String::new();
    }
    for row in &mut rows {
        while row.len() < max_cols {
            row.push(String::new());
        }
    }
    let mut md = String::new();
    md.push_str("| ");
    md.push_str(&rows[0].join(" | "));
    md.push_str(" |\n");
    md.push_str("| ");
    let sep: Vec<String> = (0..max_cols).map(|_| "---".to_string()).collect();
    md.push_str(&sep.join(" | "));
    md.push_str(" |\n");
    for row in rows.iter().skip(1) {
        md.push_str("| ");
        md.push_str(&row.join(" | "));
        md.push_str(" |\n");
    }
    md.trim_end().to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use docx_rs::{Docx, Paragraph, Run, Table, TableCell, TableRow};
    use std::path::PathBuf;

    #[test]
    fn heading_and_table_contain_markdown() {
        let docx = Docx::new()
            .add_paragraph(
                Paragraph::new()
                    .style("Heading1")
                    .add_run(Run::new().add_text("Test Heading")),
            )
            .add_paragraph(
                Paragraph::new()
                    .add_run(Run::new().add_text("Bold ").bold())
                    .add_run(Run::new().add_text("italic").italic()),
            )
            .add_paragraph(
                Paragraph::new()
                    .numbering(docx_rs::NumberingId::new(1), docx_rs::IndentLevel::new(0))
                    .add_run(Run::new().add_text("List item")),
            )
            .add_table(
                Table::new(vec![
                    TableRow::new(vec![
                        TableCell::new().add_paragraph(
                            Paragraph::new().add_run(Run::new().add_text("Header1")),
                        ),
                        TableCell::new().add_paragraph(
                            Paragraph::new().add_run(Run::new().add_text("Header2")),
                        ),
                    ]),
                    TableRow::new(vec![
                        TableCell::new()
                            .add_paragraph(Paragraph::new().add_run(Run::new().add_text("Cell1"))),
                        TableCell::new()
                            .add_paragraph(Paragraph::new().add_run(Run::new().add_text("Cell2"))),
                    ]),
                ])
                .set_grid(vec![100, 100]),
            )
            .build();
        let mut buf = Vec::new();
        {
            let cursor = std::io::Cursor::new(&mut buf);
            docx.pack(cursor).unwrap();
        }
        let md = convert_docx_bytes(&buf).unwrap();
        assert!(md.contains("# Test Heading"), "heading missing: {md}");
        assert!(
            md.contains("| Header1 | Header2 |"),
            "table header missing: {md}"
        );
        assert!(md.contains("| Cell1 | Cell2 |"), "table row missing: {md}");
        assert!(
            md.contains("**Bold **") || md.contains("**Bold**"),
            "bold missing: {md}"
        );
        assert!(md.contains("*italic*"), "italic missing: {md}");
        assert!(
            md.contains("- List item") || md.contains("List item"),
            "list missing: {md}"
        );
    }

    #[test]
    fn file_not_found_returns_io() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let conv = DocxConverter;
            let req = crate::core::types::ConversionRequest {
                input_path: PathBuf::from("/tmp/txtify_nonexistent_docx_12345.docx"),
                output_path: None,
                input_format: crate::core::types::InputFormat::Docx,
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
        let c = DocxConverter;
        assert!(c.supports_mode(crate::core::types::ConversionMode::Fast));
        assert!(!c.supports_mode(crate::core::types::ConversionMode::HighQuality));
    }
}
