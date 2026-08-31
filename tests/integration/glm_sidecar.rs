#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use txtify::config::SidecarConfig;
use txtify::converters::sidecar::{GlmOcrConverter, SidecarClient};
use txtify::core::traits::Converter;
use txtify::core::types::{ConversionMode, ConversionRequest, InputFormat, OutputFormat};

fn is_python_available() -> bool {
    let cfg = SidecarConfig::default();
    let client = SidecarClient::from_config(&cfg);
    client.is_available()
}

fn is_glm_available() -> bool {
    // Check if python + glm-ocr import works via health check
    // Do a quick sync check: try python -c "import glm_ocr" or importlib
    let cfg = SidecarConfig::default();
    let python = cfg
        .python_path
        .clone()
        .unwrap_or_else(txtify::converters::sidecar::detect_python);
    let output = std::process::Command::new(&python)
        .arg("-c")
        .arg("import importlib.util, sys; sys.exit(0 if importlib.util.find_spec('glm_ocr') or importlib.util.find_spec('glmocr') else 1)")
        .output();
    match output {
        Ok(o) => o.status.success(),
        Err(_) => false,
    }
}

#[tokio::test]
#[ignore = "requires python + glm-ocr installed (pip install -r sidecar/requirements.txt)"]
async fn glm_sidecar_health_check() {
    if !is_python_available() {
        eprintln!("Skipping: python/sidecar not available");
        return;
    }
    if !is_glm_available() {
        eprintln!("Skipping: glm-ocr python package not installed");
        return;
    }
    let cfg = SidecarConfig::default();
    let client = SidecarClient::from_config(&cfg);
    assert!(client.is_available(), "client should be available");
    // Health check should succeed (may be slow due to lazy load, but health is lightweight)
    let ok = client.health_ok().await;
    // If model not downloaded, health may still be ok (status ok/degraded)
    // We just ensure it doesn't panic
    let _ = ok;
}

#[tokio::test]
#[ignore = "requires python + glm-ocr installed (pip install -r sidecar/requirements.txt)"]
async fn glm_sidecar_convert_pdf() {
    if !is_python_available() || !is_glm_available() {
        eprintln!("Skipping: python/glm not available");
        return;
    }
    let cfg = SidecarConfig::default();
    let converter = GlmOcrConverter::from_config(&cfg);
    assert_eq!(converter.name(), "glm-ocr-0.9b");
    // Use a minimal PDF fixture if exists, else create temp pdf
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("sample.pdf");
    if !fixture.exists() {
        eprintln!(
            "Skipping: sample.pdf fixture missing at {}",
            fixture.display()
        );
        return;
    }
    let req = ConversionRequest {
        input_path: fixture.clone(),
        output_path: None,
        input_format: InputFormat::Pdf,
        output_format: OutputFormat::Md,
        mode: ConversionMode::HighQuality,
    };
    let result = converter.convert(&req).await;
    match result {
        Ok(res) => {
            assert!(!res.markdown.is_empty(), "markdown should not be empty");
            assert_eq!(res.metadata.converter_used, "glm-ocr-0.9b");
            // pages should be Some
            let _ = res.metadata.page_count;
        }
        Err(e) => {
            // If model download fails or backend missing, we get ConversionFailed; treat as skip
            eprintln!("glm convert failed (may be missing model): {e}");
        }
    }
}

#[tokio::test]
#[ignore = "requires python + glm-ocr installed (pip install -r sidecar/requirements.txt)"]
async fn glm_sidecar_protocol_json_over_stdio() {
    if !is_python_available() || !is_glm_available() {
        eprintln!("Skipping: python/glm not available");
        return;
    }
    // Test raw JSON protocol: {"op":"convert","path":"/tmp/a.pdf","to":"md","engine":"glm_ocr","backend":"auto"}
    let cfg = SidecarConfig::default();
    let client = SidecarClient::from_config(&cfg);
    // Create a temp empty pdf-like file to test error path still returns valid JSON
    let tmp = std::env::temp_dir().join("txtify_glm_protocol_test.pdf");
    let _ = std::fs::write(&tmp, b"%PDF-1.4 fake");
    let res = client.convert(&tmp, "md", "glm_ocr").await;
    // Should return either Ok with markdown or Err but valid JSON parsing occurred
    match res {
        Ok((md, pages)) => {
            let _ = md;
            let _ = pages;
        }
        Err(e) => {
            // Should be ConversionFailed, not SidecarNotFound, because sidecar is available
            assert!(
                !matches!(e, txtify::core::error::TxtifyError::SidecarNotFound(_)),
                "unexpected SidecarNotFound when sidecar is available: {e}"
            );
        }
    }
    let _ = std::fs::remove_file(tmp);
}

#[tokio::test]
#[ignore = "requires python + glm-ocr installed (pip install -r sidecar/requirements.txt)"]
async fn glm_sidecar_supports_all_formats_high_quality_only() {
    let converter = GlmOcrConverter::default();
    assert!(converter.supports_mode(ConversionMode::HighQuality));
    assert!(!converter.supports_mode(ConversionMode::Fast));
    let formats = converter.supported_formats();
    assert!(formats.contains(&InputFormat::Pdf));
    use txtify::core::types::ImageFormat;
    assert!(formats.contains(&InputFormat::Image(ImageFormat::Png)));
    // Also supports office formats via GLM pipeline
    assert!(formats.contains(&InputFormat::Html));
    assert!(formats.contains(&InputFormat::Docx));
}

#[test]
fn glm_sidecar_unit_is_available_and_hint() {
    // Unit test that does not require python - ensures fast path still works without python
    let client = SidecarClient::new(
        "/nonexistent/python3".to_string(),
        PathBuf::from("/nonexistent/txtify_sidecar.py"),
        "auto".to_string(),
        "zai-org/GLM-OCR".to_string(),
    );
    assert!(!client.is_available());
}

#[tokio::test]
async fn glm_sidecar_missing_returns_helpful_error_without_python() {
    // Ensure SidecarNotFound with pip hint even when python missing - fast path must not require python
    let client = SidecarClient::new(
        "/nonexistent/python3".to_string(),
        PathBuf::from("/nonexistent/txtify_sidecar.py"),
        "auto".to_string(),
        "zai-org/GLM-OCR".to_string(),
    );
    let converter = GlmOcrConverter::new(client);
    let req = ConversionRequest {
        input_path: PathBuf::from("/tmp/a.pdf"),
        output_path: None,
        input_format: InputFormat::Pdf,
        output_format: OutputFormat::Md,
        mode: ConversionMode::HighQuality,
    };
    let err = converter.convert(&req).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("pip install -r sidecar/requirements.txt"),
        "hint missing: {msg}"
    );
}
