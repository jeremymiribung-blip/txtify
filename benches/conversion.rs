#![allow(clippy::pedantic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, Criterion};
use txtify::converters::fast::{
    DocxConverter, HtmlConverter, PdfConverter, PptxConverter, TextConverter, XlsxConverter,
};
use txtify::core::traits::Converter;
use txtify::core::types::{ConversionMode, ConversionRequest, InputFormat, OutputFormat};

/// Setup fixtures path
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Benchmark helper
fn bench_converter<C: Converter>(c: &C, req: ConversionRequest, rt: &tokio::runtime::Runtime) {
    let _ = rt.block_on(c.convert(&req));
}

fn bench_fast_converters(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    let mut group = c.benchmark_group("fast_converters");

    // Text — 42ms target for Fast path
    let txt_path = fixture("sample.txt");
    if txt_path.exists() {
        let conv = TextConverter;
        group.bench_function("text_passthrough", |b| {
            b.iter(|| {
                let req = ConversionRequest {
                    input_path: txt_path.clone(),
                    output_path: None,
                    input_format: InputFormat::Txt,
                    output_format: OutputFormat::Md,
                    mode: ConversionMode::Fast,
                };
                bench_converter(&conv, req, &rt);
            })
        });
    }

    // HTML
    let html_path = fixture("sample.html");
    if html_path.exists() {
        let conv = HtmlConverter;
        group.bench_function("html_to_md", |b| {
            b.iter(|| {
                let req = ConversionRequest {
                    input_path: html_path.clone(),
                    output_path: None,
                    input_format: InputFormat::Html,
                    output_format: OutputFormat::Md,
                    mode: ConversionMode::Fast,
                };
                bench_converter(&conv, req, &rt);
            })
        });
    }

    // Docx
    let docx_path = fixture("sample.docx");
    if docx_path.exists() {
        let conv = DocxConverter;
        group.bench_function("docx_to_md", |b| {
            b.iter(|| {
                let req = ConversionRequest {
                    input_path: docx_path.clone(),
                    output_path: None,
                    input_format: InputFormat::Docx,
                    output_format: OutputFormat::Md,
                    mode: ConversionMode::Fast,
                };
                bench_converter(&conv, req, &rt);
            })
        });
    }

    // Xlsx
    let xlsx_path = fixture("sample.xlsx");
    if xlsx_path.exists() {
        let conv = XlsxConverter;
        group.bench_function("xlsx_to_md", |b| {
            b.iter(|| {
                let req = ConversionRequest {
                    input_path: xlsx_path.clone(),
                    output_path: None,
                    input_format: InputFormat::Xlsx,
                    output_format: OutputFormat::Md,
                    mode: ConversionMode::Fast,
                };
                bench_converter(&conv, req, &rt);
            })
        });
    }

    // PDF
    let pdf_path = fixture("sample.pdf");
    if pdf_path.exists() {
        let conv = PdfConverter;
        group.bench_function("pdf_extract", |b| {
            b.iter(|| {
                let req = ConversionRequest {
                    input_path: pdf_path.clone(),
                    output_path: None,
                    input_format: InputFormat::Pdf,
                    output_format: OutputFormat::Md,
                    mode: ConversionMode::Fast,
                };
                bench_converter(&conv, req, &rt);
            })
        });
    }

    // Pptx
    let pptx_path = fixture("sample.pptx");
    if pptx_path.exists() {
        let conv = PptxConverter;
        group.bench_function("pptx_to_md", |b| {
            b.iter(|| {
                let req = ConversionRequest {
                    input_path: pptx_path.clone(),
                    output_path: None,
                    input_format: InputFormat::Pptx,
                    output_format: OutputFormat::Md,
                    mode: ConversionMode::Fast,
                };
                bench_converter(&conv, req, &rt);
            })
        });
    }

    group.finish();
}

criterion_group!(benches, bench_fast_converters);
criterion_main!(benches);
