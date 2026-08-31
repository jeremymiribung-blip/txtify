#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use txtify::converters::fast::{
    DocxConverter, FastConverter, HtmlConverter, PdfConverter, PptxConverter, TextConverter,
    XlsxConverter,
};
use txtify::core::traits::Converter;
use txtify::core::types::{ConversionMode, ConversionRequest, InputFormat, OutputFormat};

fn fixture_path(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("fixtures");
    p.push(name);
    p
}

fn make_request(path: PathBuf, format: InputFormat) -> ConversionRequest {
    ConversionRequest {
        input_path: path,
        output_path: None,
        input_format: format,
        output_format: OutputFormat::Md,
        mode: ConversionMode::Fast,
    }
}

#[tokio::test]
async fn docx_contains_heading_and_table() {
    let path = fixture_path("sample.docx");
    assert!(path.exists(), "fixture missing: {}", path.display());
    let conv = DocxConverter;
    let req = make_request(path, InputFormat::Docx);
    let result = conv.convert(&req).await.expect("docx convert failed");
    let md = result.markdown;
    // Heading
    assert!(
        md.contains("#") && md.contains("Test Heading"),
        "heading missing in md: {md}"
    );
    // Table
    assert!(
        md.contains("|") && md.contains("Header1") && md.contains("Header2"),
        "table header missing in md: {md}"
    );
    assert!(
        md.contains("Cell1") && md.contains("Cell2"),
        "table cell missing in md: {md}"
    );
    // Bold/italic
    assert!(
        md.contains("**") || md.contains("Bold"),
        "bold missing in md: {md}"
    );
}

#[tokio::test]
async fn xlsx_contains_sheet_and_table() {
    let path = fixture_path("sample.xlsx");
    assert!(path.exists(), "fixture missing: {}", path.display());
    let conv = XlsxConverter;
    let req = make_request(path, InputFormat::Xlsx);
    let result = conv.convert(&req).await.expect("xlsx convert failed");
    let md = result.markdown;
    assert!(md.contains("## Sheet1"), "sheet heading missing: {md}");
    assert!(
        md.contains("| Name | Age |") || (md.contains("Name") && md.contains("Age")),
        "header missing: {md}"
    );
    assert!(
        md.contains("Alice") && md.contains("30"),
        "row missing: {md}"
    );
}

#[tokio::test]
async fn pptx_contains_slides() {
    let path = fixture_path("sample.pptx");
    assert!(path.exists(), "fixture missing: {}", path.display());
    let conv = PptxConverter;
    let req = make_request(path, InputFormat::Pptx);
    let result = conv.convert(&req).await.expect("pptx convert failed");
    let md = result.markdown;
    assert!(md.contains("## Slide 1"), "slide 1 heading missing: {md}");
    assert!(
        md.contains("Hello Slide 1") || md.contains("Slide 1"),
        "slide 1 text missing: {md}"
    );
    assert!(md.contains("## Slide 2"), "slide 2 heading missing: {md}");
}

#[tokio::test]
async fn html_contains_heading_and_table() {
    let path = fixture_path("sample.html");
    assert!(path.exists(), "fixture missing: {}", path.display());
    let conv = HtmlConverter;
    let req = make_request(path, InputFormat::Html);
    let result = conv.convert(&req).await.expect("html convert failed");
    let md = result.markdown;
    assert!(md.contains("Heading One"), "heading missing in md: {md}");
    assert!(
        md.contains("Header1") || md.contains("Cell1"),
        "table missing in md: {md}"
    );
}

#[tokio::test]
async fn text_passthrough() {
    let path = fixture_path("sample.txt");
    assert!(path.exists(), "fixture missing: {}", path.display());
    let conv = TextConverter;
    let req = make_request(path, InputFormat::Txt);
    let result = conv.convert(&req).await.expect("text convert failed");
    let md = result.markdown;
    assert!(md.contains("Hello text"), "text missing: {md}");
}

#[tokio::test]
async fn pdf_contains_pages_and_text() {
    let path = fixture_path("sample.pdf");
    assert!(path.exists(), "fixture missing: {}", path.display());
    let conv = PdfConverter;
    let req = make_request(path, InputFormat::Pdf);
    let result = conv.convert(&req).await.expect("pdf convert failed");
    let md = result.markdown;
    assert!(md.contains("--- Page 1 ---"), "page marker missing: {md}");
    assert!(
        md.contains("Hello PDF") || md.contains("Txtify Sample"),
        "pdf text missing: {md}"
    );
    assert!(result.metadata.page_count.is_some());
}

#[tokio::test]
async fn image_fixtures_exist_and_detect() {
    for name in ["sample.png", "sample.jpg", "sample.tiff"] {
        let path = fixture_path(name);
        assert!(path.exists(), "image fixture missing: {}", path.display());
        let fmt = txtify::core::types::InputFormat::from_extension(&path);
        assert!(fmt.is_some(), "failed to detect {name}: {fmt:?}");
        // magic bytes detection
        let bytes = std::fs::read(&path).expect("read image");
        let magic = txtify::core::types::InputFormat::from_magic_bytes(&bytes);
        assert!(
            magic.is_some(),
            "magic detect failed for {name}: bytes {bytes:?}"
        );
    }
}

#[tokio::test]
async fn fast_converter_delegates_docx() {
    let path = fixture_path("sample.docx");
    let conv = FastConverter;
    let req = make_request(path, InputFormat::Docx);
    let result = conv.convert(&req).await.expect("fast docx failed");
    assert!(
        result.markdown.contains("Test Heading") || result.markdown.contains("Heading"),
        "fast docx heading missing"
    );
}

#[tokio::test]
async fn fast_converter_delegates_xlsx() {
    let path = fixture_path("sample.xlsx");
    let conv = FastConverter;
    let req = make_request(path, InputFormat::Xlsx);
    let result = conv.convert(&req).await.expect("fast xlsx failed");
    assert!(
        result.markdown.contains("Sheet1"),
        "fast xlsx sheet missing"
    );
}

#[tokio::test]
async fn file_not_found_returns_io() {
    let conv = DocxConverter;
    let req = make_request(
        PathBuf::from("/tmp/txtify_nonexistent_integration_12345.docx"),
        InputFormat::Docx,
    );
    let err = conv.convert(&req).await.unwrap_err();
    assert!(
        matches!(err, txtify::core::error::TxtifyError::Io(_)),
        "expected Io, got {err:?}"
    );

    let conv = XlsxConverter;
    let req = make_request(
        PathBuf::from("/tmp/txtify_nonexistent_integration_12345.xlsx"),
        InputFormat::Xlsx,
    );
    let err = conv.convert(&req).await.unwrap_err();
    assert!(
        matches!(err, txtify::core::error::TxtifyError::Io(_)),
        "expected Io xlsx, got {err:?}"
    );

    let conv = PptxConverter;
    let req = make_request(
        PathBuf::from("/tmp/txtify_nonexistent_integration_12345.pptx"),
        InputFormat::Pptx,
    );
    let err = conv.convert(&req).await.unwrap_err();
    assert!(
        matches!(err, txtify::core::error::TxtifyError::Io(_)),
        "expected Io pptx, got {err:?}"
    );

    let conv = HtmlConverter;
    let req = make_request(
        PathBuf::from("/tmp/txtify_nonexistent_integration_12345.html"),
        InputFormat::Html,
    );
    let err = conv.convert(&req).await.unwrap_err();
    assert!(
        matches!(err, txtify::core::error::TxtifyError::Io(_)),
        "expected Io html, got {err:?}"
    );

    let conv = TextConverter;
    let req = make_request(
        PathBuf::from("/tmp/txtify_nonexistent_integration_12345.txt"),
        InputFormat::Txt,
    );
    let err = conv.convert(&req).await.unwrap_err();
    assert!(
        matches!(err, txtify::core::error::TxtifyError::Io(_)),
        "expected Io txt, got {err:?}"
    );
}

#[test]
fn supports_only_fast() {
    let docx = DocxConverter;
    assert!(docx.supports_mode(ConversionMode::Fast));
    assert!(!docx.supports_mode(ConversionMode::HighQuality));

    let xlsx = XlsxConverter;
    assert!(xlsx.supports_mode(ConversionMode::Fast));
    assert!(!xlsx.supports_mode(ConversionMode::HighQuality));

    let pptx = PptxConverter;
    assert!(pptx.supports_mode(ConversionMode::Fast));
    assert!(!pptx.supports_mode(ConversionMode::HighQuality));

    let html = HtmlConverter;
    assert!(html.supports_mode(ConversionMode::Fast));
    assert!(!html.supports_mode(ConversionMode::HighQuality));

    let text = TextConverter;
    assert!(text.supports_mode(ConversionMode::Fast));
    assert!(!text.supports_mode(ConversionMode::HighQuality));

    let fast = FastConverter;
    assert!(fast.supports_mode(ConversionMode::Fast));
    assert!(!fast.supports_mode(ConversionMode::HighQuality));
}

#[test]
fn fast_converter_supported_formats() {
    let fast = FastConverter;
    let formats = fast.supported_formats();
    assert!(formats.contains(&InputFormat::Docx));
    assert!(formats.contains(&InputFormat::Xlsx));
    assert!(formats.contains(&InputFormat::Pptx));
    assert!(formats.contains(&InputFormat::Html));
    assert!(formats.contains(&InputFormat::Txt));
}
